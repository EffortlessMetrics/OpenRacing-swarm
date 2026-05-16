//! Firmware update manager with A/B partition support
//!
//! Provides atomic firmware updates with automatic rollback capability.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{error, info, warn};

use crate::error::FirmwareUpdateError;
use crate::hardware_version::HardwareVersion;
use crate::partition::{Partition, PartitionInfo};

/// Firmware update state machine states
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdateState {
    /// No update in progress, system is idle
    #[default]
    Idle,

    /// Downloading firmware image from remote source
    Downloading {
        /// Download progress as percentage (0-100)
        progress: u8,
    },

    /// Verifying firmware image signature and integrity
    Verifying,

    /// Writing firmware to device (flashing)
    Flashing {
        /// Flash progress as percentage (0-100)
        progress: u8,
    },

    /// Rebooting device to apply new firmware
    Rebooting,

    /// Update completed successfully
    Complete,

    /// Update failed with error information
    Failed {
        /// Error description
        error: String,
        /// Whether the error is recoverable (rollback possible)
        recoverable: bool,
    },
}

impl UpdateState {
    /// Check if an update is currently in progress
    pub fn is_in_progress(&self) -> bool {
        !matches!(
            self,
            UpdateState::Idle | UpdateState::Complete | UpdateState::Failed { .. }
        )
    }

    /// Check if FFB operations should be blocked
    pub fn should_block_ffb(&self) -> bool {
        self.is_in_progress()
    }
}

/// Progress information for firmware update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// Current phase of the update
    pub phase: UpdatePhase,

    /// Progress percentage (0-100)
    pub progress_percent: u8,

    /// Bytes transferred so far
    pub bytes_transferred: u64,

    /// Total bytes to transfer
    pub total_bytes: u64,

    /// Transfer rate in bytes per second
    pub transfer_rate_bps: u64,

    /// Estimated time remaining
    pub eta_seconds: Option<u64>,

    /// Current status message
    pub status_message: String,

    /// Any warnings or non-fatal errors
    pub warnings: Vec<String>,
}

/// Phases of firmware update process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdatePhase {
    /// Initializing update process
    Initializing,

    /// Verifying firmware image
    Verifying,

    /// Preparing target partition
    Preparing,

    /// Transferring firmware data
    Transferring,

    /// Validating transferred data
    Validating,

    /// Activating new firmware
    Activating,

    /// Running health checks
    HealthCheck,

    /// Update completed successfully
    Completed,

    /// Update failed, rolling back
    RollingBack,

    /// Update failed completely
    Failed,
}

/// Result of firmware update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    /// Device identifier
    pub device_id: String,

    /// Whether update was successful
    pub success: bool,

    /// Version before update
    pub old_version: Option<semver::Version>,

    /// Version after update
    pub new_version: Option<semver::Version>,

    /// Partition that was updated
    pub updated_partition: Option<Partition>,

    /// Whether rollback was performed
    pub rollback_performed: bool,

    /// Duration of update process
    #[serde(with = "duration_serde")]
    pub duration: Duration,

    /// Error message if update failed
    pub error: Option<String>,

    /// Final partition states
    pub partition_states: Vec<PartitionInfo>,
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// Firmware image metadata and binary payload.
///
/// Represents a complete firmware image ready for flashing, including the
/// binary data, cryptographic hash for integrity verification, optional
/// Ed25519 signature, and hardware compatibility constraints.
///
/// # Hardware Compatibility
///
/// The `min_hardware_version` and `max_hardware_version` fields define the
/// range of hardware revisions this firmware is compatible with. The update
/// manager checks these before flashing to prevent bricking devices with
/// incompatible firmware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareImage {
    /// Target device model/type
    pub device_model: String,

    /// Firmware version
    pub version: semver::Version,

    /// Minimum compatible hardware version
    pub min_hardware_version: Option<String>,

    /// Maximum compatible hardware version
    pub max_hardware_version: Option<String>,

    /// Firmware binary data
    #[serde(skip)]
    pub data: Vec<u8>,

    /// SHA256 hash of firmware data
    pub hash: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// Build timestamp
    pub build_timestamp: chrono::DateTime<chrono::Utc>,

    /// Release notes or changelog
    pub release_notes: Option<String>,

    /// Signature metadata for verification
    pub signature: Option<openracing_crypto::SignatureMetadata>,
}

/// Configuration for staged (gradual) firmware rollout.
///
/// Controls how firmware updates are deployed across a fleet of devices in
/// stages, with automatic rollback if the error rate exceeds the configured
/// threshold. Stage sizes double after each successful stage, starting from
/// `stage1_max_devices`.
///
/// # Defaults
///
/// | Parameter | Default |
/// |-----------|---------|
/// | `enabled` | `true` |
/// | `stage1_max_devices` | `10` |
/// | `min_success_rate` | `0.95` (95%) |
/// | `stage_delay_minutes` | `60` |
/// | `max_error_rate` | `0.05` (5%) |
/// | `monitoring_window_minutes` | `120` |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedRolloutConfig {
    /// Enable staged rollout
    pub enabled: bool,

    /// Maximum number of devices to update in first stage
    pub stage1_max_devices: u32,

    /// Minimum success rate required to proceed to next stage
    pub min_success_rate: f64,

    /// Time to wait between stages
    pub stage_delay_minutes: u32,

    /// Maximum error rate before automatic rollback
    pub max_error_rate: f64,

    /// Time window for monitoring success rate
    pub monitoring_window_minutes: u32,
}

impl Default for StagedRolloutConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stage1_max_devices: 10,
            min_success_rate: 0.95,
            stage_delay_minutes: 60,
            max_error_rate: 0.05,
            monitoring_window_minutes: 120,
        }
    }
}

/// Trait for device-specific firmware update operations.
///
/// Implementors provide the low-level commands to communicate with a
/// specific device's bootloader or firmware update interface. The
/// [`FirmwareUpdateManager`] orchestrates the update flow by calling
/// these methods in sequence.
///
/// # A/B Partition Model
///
/// Devices are expected to have two firmware partitions (A and B). The
/// manager writes new firmware to the inactive partition, validates it,
/// then atomically switches the boot target. If the health check fails
/// after reboot, the manager rolls back to the previous partition.
///
/// # Implementation Notes
///
/// All methods are async and may involve USB/HID communication with
/// the device. Implementations should handle transient communication
/// errors with appropriate retries at the transport level.
#[async_trait::async_trait]
pub trait FirmwareDevice: Send + Sync {
    /// Get device identifier
    fn device_id(&self) -> &str;

    /// Get device model/type
    fn device_model(&self) -> &str;

    /// Get current partition information
    async fn get_partition_info(&self) -> Result<Vec<PartitionInfo>>;

    /// Get currently active partition
    async fn get_active_partition(&self) -> Result<Partition>;

    /// Prepare a partition for firmware update
    async fn prepare_partition(&self, partition: Partition) -> Result<()>;

    /// Write firmware data to partition
    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()>;

    /// Validate firmware in partition
    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()>;

    /// Set partition as bootable
    async fn set_bootable(&self, partition: Partition, bootable: bool) -> Result<()>;

    /// Perform atomic swap to new partition
    async fn activate_partition(&self, partition: Partition) -> Result<()>;

    /// Reboot device to apply firmware change
    async fn reboot(&self) -> Result<()>;

    /// Check if device is responsive after reboot
    async fn health_check(&self) -> Result<()>;

    /// Get hardware version for compatibility checking
    async fn get_hardware_version(&self) -> Result<String>;
}

/// Firmware update manager with A/B partition support.
///
/// Orchestrates the full firmware update lifecycle for racing peripherals:
///
/// 1. **Compatibility check** — Validates hardware version against firmware
///    requirements.
/// 2. **Hash verification** — Computes SHA-256 of firmware data and compares
///    against the expected hash.
/// 3. **Partition preparation** — Erases the inactive (target) partition.
/// 4. **Transfer** — Writes firmware in 4 KiB chunks with progress reporting.
/// 5. **Validation** — Verifies the written data on-device.
/// 6. **Activation** — Atomically switches boot target to the new partition.
/// 7. **Health check** — Verifies device responsiveness after reboot (up to
///    5 attempts). If health checks fail, automatically rolls back to the
///    previous partition.
///
/// # Safety Preconditions
///
/// - Force feedback output is blocked while an update is in progress
///   (see [`UpdateState::should_block_ffb`]).
/// - Only one update per device is allowed at a time; concurrent requests
///   for the same device are rejected.
///
/// # Progress Reporting
///
/// Subscribe to the broadcast channel via the returned receiver to receive
/// [`UpdateProgress`] events throughout the update process.
pub struct FirmwareUpdateManager {
    #[allow(dead_code)]
    rollout_config: StagedRolloutConfig,
    progress_tx: broadcast::Sender<UpdateProgress>,
    active_updates: std::sync::Arc<tokio::sync::Mutex<HashMap<String, UpdateHandle>>>,
}

struct UpdateHandle {
    #[allow(dead_code)]
    device_id: String,
    cancel_tx: mpsc::Sender<()>,
    #[allow(dead_code)]
    progress_rx: mpsc::Receiver<UpdateProgress>,
}

impl FirmwareUpdateManager {
    /// Create a new firmware update manager
    pub fn new(rollout_config: StagedRolloutConfig) -> Self {
        let (progress_tx, _) = broadcast::channel(1000);

        Self {
            rollout_config,
            progress_tx,
            active_updates: std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Update firmware on a single device using the A/B partition strategy.
    ///
    /// Performs the full update lifecycle: compatibility check, hash
    /// verification, partition write, validation, activation, reboot, and
    /// health check. Automatically rolls back on health check failure.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - An update is already in progress for this device.
    /// - The firmware hash does not match the expected value.
    /// - The firmware is incompatible with the device hardware version.
    /// - Any device communication step fails (prepare, write, validate,
    ///   activate, reboot).
    /// - The post-reboot health check fails after 5 attempts and rollback
    ///   also fails ([`FirmwareUpdateError::RollbackFailed`]).
    pub async fn update_device_firmware(
        &self,
        device: Box<dyn FirmwareDevice>,
        firmware: &FirmwareImage,
    ) -> Result<UpdateResult> {
        let device_id = device.device_id().to_string();
        let start_time = Instant::now();

        info!("Starting firmware update for device: {}", device_id);

        {
            let active_updates = self.active_updates.lock().await;
            if active_updates.contains_key(&device_id) {
                return Err(anyhow::anyhow!(
                    "Update already in progress for device: {}",
                    device_id
                ));
            }
        }

        let (progress_tx, progress_rx) = mpsc::channel(100);
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);

        {
            let mut active_updates = self.active_updates.lock().await;
            active_updates.insert(
                device_id.clone(),
                UpdateHandle {
                    device_id: device_id.clone(),
                    cancel_tx,
                    progress_rx,
                },
            );
        }

        let result = self
            .perform_device_update(device, firmware, progress_tx, &mut cancel_rx)
            .await;

        {
            let mut active_updates = self.active_updates.lock().await;
            active_updates.remove(&device_id);
        }

        let duration = start_time.elapsed();
        match result {
            Ok((old_version, new_version, updated_partition, partition_states)) => {
                info!(
                    "Firmware update completed successfully for device: {}",
                    device_id
                );
                Ok(UpdateResult {
                    device_id,
                    success: true,
                    old_version,
                    new_version: Some(new_version),
                    updated_partition: Some(updated_partition),
                    rollback_performed: false,
                    duration,
                    error: None,
                    partition_states,
                })
            }
            Err(e) => {
                error!("Firmware update failed for device {}: {}", device_id, e);
                Ok(UpdateResult {
                    device_id,
                    success: false,
                    old_version: None,
                    new_version: None,
                    updated_partition: None,
                    rollback_performed: false,
                    duration,
                    error: Some(e.to_string()),
                    partition_states: Vec::new(),
                })
            }
        }
    }

    async fn perform_device_update(
        &self,
        device: Box<dyn FirmwareDevice>,
        firmware: &FirmwareImage,
        progress_tx: mpsc::Sender<UpdateProgress>,
        cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<(
        Option<semver::Version>,
        semver::Version,
        Partition,
        Vec<PartitionInfo>,
    )> {
        let device = &*device;
        let total_bytes = firmware.size_bytes;

        self.notify_initialization(&progress_tx, total_bytes).await;
        let (active_partition, target_partition, old_version) =
            self.plan_partition_swap(device, firmware).await?;

        self.verify_firmware_hash(firmware, &progress_tx).await?;
        self.prepare_target_partition(device, target_partition, total_bytes, &progress_tx)
            .await?;
        self.transfer_firmware_data(device, firmware, target_partition, &progress_tx, cancel_rx)
            .await?;
        self.validate_transferred_firmware(device, firmware, target_partition, &progress_tx)
            .await?;
        self.activate_new_firmware(device, target_partition, total_bytes, &progress_tx)
            .await?;
        self.run_post_update_health_check(device, total_bytes, active_partition, &progress_tx)
            .await?;
        let final_partition_info = self
            .finalize_update(device, total_bytes, &progress_tx)
            .await?;

        Ok((
            old_version,
            firmware.version.clone(),
            target_partition,
            final_partition_info,
        ))
    }

    /// Builds an [`UpdateProgress`] with no transfer rate, ETA, or warnings.
    ///
    /// Most phases of the update pipeline emit a single progress update with
    /// fixed defaults; this keeps the call sites focused on the values that
    /// actually vary per phase.
    fn simple_progress(
        phase: UpdatePhase,
        progress_percent: u8,
        bytes_transferred: u64,
        total_bytes: u64,
        status_message: impl Into<String>,
    ) -> UpdateProgress {
        UpdateProgress {
            phase,
            progress_percent,
            bytes_transferred,
            total_bytes,
            transfer_rate_bps: 0,
            eta_seconds: None,
            status_message: status_message.into(),
            warnings: Vec::new(),
        }
    }

    async fn notify_initialization(
        &self,
        progress_tx: &mpsc::Sender<UpdateProgress>,
        total_bytes: u64,
    ) {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Initializing,
                0,
                0,
                total_bytes,
                "Initializing firmware update",
            ),
        )
        .await;
    }

    async fn plan_partition_swap(
        &self,
        device: &dyn FirmwareDevice,
        firmware: &FirmwareImage,
    ) -> Result<(Partition, Partition, Option<semver::Version>)> {
        let hardware_version = device
            .get_hardware_version()
            .await
            .context("Failed to get hardware version")?;

        self.check_compatibility(firmware, &hardware_version)
            .context("Firmware compatibility check failed")?;

        let partition_info = device
            .get_partition_info()
            .await
            .context("Failed to get partition information")?;

        let active_partition = device
            .get_active_partition()
            .await
            .context("Failed to get active partition")?;

        let target_partition = active_partition.other();
        let old_version = partition_info
            .iter()
            .find(|p| p.partition == active_partition)
            .and_then(|p| p.version.clone());

        info!(
            "Updating from partition {:?} to {:?}",
            active_partition, target_partition
        );

        Ok((active_partition, target_partition, old_version))
    }

    async fn verify_firmware_hash(
        &self,
        firmware: &FirmwareImage,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Verifying,
                5,
                0,
                firmware.size_bytes,
                "Verifying firmware image",
            ),
        )
        .await;

        let computed_hash = self
            .compute_firmware_hash(&firmware.data)
            .context("Failed to compute firmware hash")?;

        if computed_hash != firmware.hash {
            return Err(
                FirmwareUpdateError::InvalidFirmware("Firmware hash mismatch".to_string()).into(),
            );
        }

        Ok(())
    }

    async fn prepare_target_partition(
        &self,
        device: &dyn FirmwareDevice,
        target_partition: Partition,
        total_bytes: u64,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Preparing,
                10,
                0,
                total_bytes,
                "Preparing target partition",
            ),
        )
        .await;

        device
            .prepare_partition(target_partition)
            .await
            .context("Failed to prepare target partition")?;

        Ok(())
    }

    async fn transfer_firmware_data(
        &self,
        device: &dyn FirmwareDevice,
        firmware: &FirmwareImage,
        target_partition: Partition,
        progress_tx: &mpsc::Sender<UpdateProgress>,
        cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Transferring,
                15,
                0,
                firmware.size_bytes,
                "Transferring firmware data",
            ),
        )
        .await;

        let transfer_start = Instant::now();
        let chunk_size = 4096;
        let mut bytes_transferred = 0u64;

        for (i, chunk) in firmware.data.chunks(chunk_size).enumerate() {
            if cancel_rx.try_recv().is_ok() {
                return Err(anyhow::anyhow!("Update cancelled by user"));
            }

            let offset = i * chunk_size;
            device
                .write_firmware_chunk(target_partition, offset as u64, chunk)
                .await
                .with_context(|| format!("Failed to write firmware chunk at offset {}", offset))?;

            bytes_transferred += chunk.len() as u64;

            if bytes_transferred.is_multiple_of(64 * 1024)
                || bytes_transferred == firmware.size_bytes
            {
                let elapsed = transfer_start.elapsed();
                let transfer_rate = if elapsed.as_secs() > 0 {
                    bytes_transferred / elapsed.as_secs()
                } else {
                    0
                };

                let eta = (firmware.size_bytes - bytes_transferred).checked_div(transfer_rate);

                let progress_percent = 15 + ((bytes_transferred * 60) / firmware.size_bytes) as u8;

                self.send_progress(
                    progress_tx,
                    UpdateProgress {
                        phase: UpdatePhase::Transferring,
                        progress_percent,
                        bytes_transferred,
                        total_bytes: firmware.size_bytes,
                        transfer_rate_bps: transfer_rate,
                        eta_seconds: eta,
                        status_message: format!(
                            "Transferred {} / {} bytes",
                            bytes_transferred, firmware.size_bytes
                        ),
                        warnings: Vec::new(),
                    },
                )
                .await;
            }
        }

        Ok(())
    }

    async fn validate_transferred_firmware(
        &self,
        device: &dyn FirmwareDevice,
        firmware: &FirmwareImage,
        target_partition: Partition,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Validating,
                75,
                firmware.size_bytes,
                firmware.size_bytes,
                "Validating transferred firmware",
            ),
        )
        .await;

        device
            .validate_partition(target_partition, &firmware.hash)
            .await
            .context("Firmware validation failed")?;

        Ok(())
    }

    async fn activate_new_firmware(
        &self,
        device: &dyn FirmwareDevice,
        target_partition: Partition,
        total_bytes: u64,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::Activating,
                85,
                total_bytes,
                total_bytes,
                "Activating new firmware",
            ),
        )
        .await;

        device
            .set_bootable(target_partition, true)
            .await
            .context("Failed to set target partition as bootable")?;

        device
            .activate_partition(target_partition)
            .await
            .context("Failed to activate target partition")?;

        device.reboot().await.context("Failed to reboot device")?;

        tokio::time::sleep(Duration::from_secs(10)).await;

        Ok(())
    }

    async fn run_post_update_health_check(
        &self,
        device: &dyn FirmwareDevice,
        total_bytes: u64,
        rollback_partition: Partition,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<()> {
        self.send_progress(
            progress_tx,
            Self::simple_progress(
                UpdatePhase::HealthCheck,
                95,
                total_bytes,
                total_bytes,
                "Running health checks",
            ),
        )
        .await;

        const MAX_HEALTH_CHECK_ATTEMPTS: u32 = 5;
        let mut health_check_attempts = 0;

        loop {
            match device.health_check().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    health_check_attempts += 1;
                    if health_check_attempts >= MAX_HEALTH_CHECK_ATTEMPTS {
                        warn!(
                            "Health check failed after {} attempts, attempting rollback",
                            MAX_HEALTH_CHECK_ATTEMPTS
                        );

                        if let Err(rollback_error) =
                            self.perform_rollback(device, rollback_partition).await
                        {
                            error!("Rollback failed: {}", rollback_error);
                            return Err(FirmwareUpdateError::RollbackFailed(format!(
                                "Health check failed and rollback failed: {} -> {}",
                                e, rollback_error
                            ))
                            .into());
                        }

                        return Err(FirmwareUpdateError::HealthCheckFailed(format!(
                            "Health check failed after {} attempts, rolled back to previous firmware",
                            MAX_HEALTH_CHECK_ATTEMPTS
                        ))
                        .into());
                    }

                    warn!(
                        "Health check attempt {} failed: {}, retrying...",
                        health_check_attempts, e
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn finalize_update(
        &self,
        device: &dyn FirmwareDevice,
        total_bytes: u64,
        progress_tx: &mpsc::Sender<UpdateProgress>,
    ) -> Result<Vec<PartitionInfo>> {
        self.send_progress(
            progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Completed,
                progress_percent: 100,
                bytes_transferred: total_bytes,
                total_bytes,
                transfer_rate_bps: 0,
                eta_seconds: Some(0),
                status_message: "Firmware update completed successfully".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        device
            .get_partition_info()
            .await
            .context("Failed to get final partition information")
    }

    async fn perform_rollback(
        &self,
        device: &dyn FirmwareDevice,
        rollback_partition: Partition,
    ) -> Result<()> {
        info!(
            "Performing firmware rollback to partition {:?}",
            rollback_partition
        );

        device
            .set_bootable(rollback_partition, true)
            .await
            .context("Failed to set rollback partition as bootable")?;

        device
            .activate_partition(rollback_partition)
            .await
            .context("Failed to activate rollback partition")?;

        device
            .reboot()
            .await
            .context("Failed to reboot device for rollback")?;

        tokio::time::sleep(Duration::from_secs(10)).await;

        device
            .health_check()
            .await
            .context("Health check failed after rollback")?;

        info!("Firmware rollback completed successfully");
        Ok(())
    }

    fn check_compatibility(&self, firmware: &FirmwareImage, hardware_version: &str) -> Result<()> {
        use std::cmp::Ordering;

        if let Some(min_version) = &firmware.min_hardware_version {
            match HardwareVersion::try_compare(hardware_version, min_version) {
                Some(Ordering::Less) => {
                    return Err(FirmwareUpdateError::InvalidFirmware(format!(
                        "Hardware version {} is below minimum required version {}",
                        hardware_version, min_version
                    ))
                    .into());
                }
                None => {
                    return Err(FirmwareUpdateError::InvalidFirmware(format!(
                        "Failed to parse hardware version '{}' or minimum version '{}'",
                        hardware_version, min_version
                    ))
                    .into());
                }
                _ => {}
            }
        }

        if let Some(max_version) = &firmware.max_hardware_version {
            match HardwareVersion::try_compare(hardware_version, max_version) {
                Some(Ordering::Greater) => {
                    return Err(FirmwareUpdateError::InvalidFirmware(format!(
                        "Hardware version {} is above maximum supported version {}",
                        hardware_version, max_version
                    ))
                    .into());
                }
                None => {
                    return Err(FirmwareUpdateError::InvalidFirmware(format!(
                        "Failed to parse hardware version '{}' or maximum version '{}'",
                        hardware_version, max_version
                    ))
                    .into());
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn compute_firmware_hash(&self, data: &[u8]) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(hex::encode(hasher.finalize()))
    }

    async fn send_progress(
        &self,
        progress_tx: &mpsc::Sender<UpdateProgress>,
        progress: UpdateProgress,
    ) {
        let _ = progress_tx.send(progress.clone()).await;
        let _ = self.progress_tx.send(progress);
    }

    /// Subscribe to progress updates
    pub fn subscribe_progress(&self) -> broadcast::Receiver<UpdateProgress> {
        self.progress_tx.subscribe()
    }

    /// Cancel an active update
    pub async fn cancel_update(&self, device_id: &str) -> Result<()> {
        let active_updates = self.active_updates.lock().await;
        if let Some(handle) = active_updates.get(device_id) {
            handle
                .cancel_tx
                .send(())
                .await
                .context("Failed to send cancel signal")?;
            info!("Sent cancel signal for device: {}", device_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "No active update found for device: {}",
                device_id
            ))
        }
    }

    /// Get list of devices with active updates
    pub async fn get_active_updates(&self) -> Vec<String> {
        let active_updates = self.active_updates.lock().await;
        active_updates.keys().cloned().collect()
    }

    /// Check if any firmware update is in progress
    pub async fn is_update_in_progress(&self) -> bool {
        let active_updates = self.active_updates.lock().await;
        !active_updates.is_empty()
    }
}

/// FFB blocker for mutual exclusion during firmware updates
#[derive(Debug)]
pub struct FfbBlocker {
    update_in_progress: AtomicBool,
    updating_device: RwLock<Option<String>>,
    update_state: RwLock<UpdateState>,
}

impl Default for FfbBlocker {
    fn default() -> Self {
        Self::new()
    }
}

impl FfbBlocker {
    /// Create a new FFB blocker
    pub fn new() -> Self {
        Self {
            update_in_progress: AtomicBool::new(false),
            updating_device: RwLock::new(None),
            update_state: RwLock::new(UpdateState::Idle),
        }
    }

    /// Check if FFB operations are currently blocked
    #[inline]
    pub fn is_ffb_blocked(&self) -> bool {
        self.update_in_progress.load(Ordering::Acquire)
    }

    /// Try to perform an FFB operation, returning an error if blocked
    pub fn try_ffb_operation(&self) -> Result<(), FirmwareUpdateError> {
        if self.is_ffb_blocked() {
            Err(FirmwareUpdateError::FfbBlocked)
        } else {
            Ok(())
        }
    }

    /// Begin a firmware update, blocking FFB operations
    pub async fn begin_update(&self, device_id: &str) -> Result<(), FirmwareUpdateError> {
        if self
            .update_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            let current_device = self.updating_device.read().await;
            let device_name = current_device.as_deref().unwrap_or("unknown");
            return Err(FirmwareUpdateError::UpdateInProgress(
                device_name.to_string(),
            ));
        }

        {
            let mut device = self.updating_device.write().await;
            *device = Some(device_id.to_string());
        }
        {
            let mut state = self.update_state.write().await;
            *state = UpdateState::Verifying;
        }

        info!("FFB blocked for firmware update on device: {}", device_id);
        Ok(())
    }

    /// Update the current update state
    pub async fn set_state(&self, new_state: UpdateState) {
        let mut state = self.update_state.write().await;
        *state = new_state;
    }

    /// Get the current update state
    pub async fn get_state(&self) -> UpdateState {
        self.update_state.read().await.clone()
    }

    /// End a firmware update, unblocking FFB operations
    pub async fn end_update(&self) {
        {
            let mut device = self.updating_device.write().await;
            let device_id = device.take();
            if let Some(id) = device_id {
                info!("FFB unblocked after firmware update on device: {}", id);
            }
        }
        {
            let mut state = self.update_state.write().await;
            *state = UpdateState::Idle;
        }

        self.update_in_progress.store(false, Ordering::Release);
    }

    /// Get the device ID currently being updated
    pub async fn get_updating_device(&self) -> Option<String> {
        self.updating_device.read().await.clone()
    }
}

/// Cached firmware image entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFirmware {
    /// Firmware device model
    pub device_model: String,

    /// Firmware version
    pub version: semver::Version,

    /// SHA256 hash of firmware data
    pub hash: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// When the firmware was cached
    pub cached_at: chrono::DateTime<chrono::Utc>,

    /// Path to the cached firmware file
    pub cache_path: PathBuf,

    /// Original signature metadata
    pub signature: Option<openracing_crypto::SignatureMetadata>,
}

/// Firmware cache for offline updates
pub struct FirmwareCache {
    cache_dir: PathBuf,
    index: RwLock<HashMap<String, CachedFirmware>>,
    max_cache_size: u64,
}

impl FirmwareCache {
    /// Create a new firmware cache
    pub async fn new(cache_dir: PathBuf, max_cache_size: u64) -> Result<Self> {
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .context("Failed to create firmware cache directory")?;

        let cache = Self {
            cache_dir,
            index: RwLock::new(HashMap::new()),
            max_cache_size,
        };

        cache.load_index().await?;

        Ok(cache)
    }

    fn cache_key(device_model: &str, version: &semver::Version) -> String {
        format!("{}_{}", device_model, version)
    }

    fn cache_filename(device_model: &str, version: &semver::Version) -> String {
        format!("{}_{}.fw", device_model, version)
    }

    async fn load_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");

        if index_path.exists() {
            let index_data = tokio::fs::read_to_string(&index_path)
                .await
                .context("Failed to read cache index")?;

            let loaded_index: HashMap<String, CachedFirmware> =
                serde_json::from_str(&index_data).context("Failed to parse cache index")?;

            let mut valid_entries = HashMap::new();
            for (key, entry) in loaded_index {
                if entry.cache_path.exists() {
                    valid_entries.insert(key, entry);
                } else {
                    warn!(
                        "Cached firmware file missing, removing from index: {}",
                        entry.cache_path.display()
                    );
                }
            }

            let mut index = self.index.write().await;
            *index = valid_entries;
        }

        Ok(())
    }

    async fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let index = self.index.read().await;

        let index_data =
            serde_json::to_string_pretty(&*index).context("Failed to serialize cache index")?;

        tokio::fs::write(&index_path, index_data)
            .await
            .context("Failed to write cache index")?;

        Ok(())
    }

    /// Add a firmware image to the cache
    pub async fn add(&self, firmware: &FirmwareImage) -> Result<()> {
        let key = Self::cache_key(&firmware.device_model, &firmware.version);
        let filename = Self::cache_filename(&firmware.device_model, &firmware.version);
        let cache_path = self.cache_dir.join(&filename);

        if self.max_cache_size > 0 {
            let current_size = self.get_cache_size().await;
            if current_size + firmware.size_bytes > self.max_cache_size {
                self.evict_oldest(firmware.size_bytes).await?;
            }
        }

        tokio::fs::write(&cache_path, &firmware.data)
            .await
            .context("Failed to write firmware to cache")?;

        let entry = CachedFirmware {
            device_model: firmware.device_model.clone(),
            version: firmware.version.clone(),
            hash: firmware.hash.clone(),
            size_bytes: firmware.size_bytes,
            cached_at: chrono::Utc::now(),
            cache_path,
            signature: firmware.signature.clone(),
        };

        {
            let mut index = self.index.write().await;
            index.insert(key, entry);
        }

        self.save_index().await?;

        info!(
            "Cached firmware: {} v{}",
            firmware.device_model, firmware.version
        );
        Ok(())
    }

    /// Get a firmware image from the cache
    pub async fn get(
        &self,
        device_model: &str,
        version: &semver::Version,
    ) -> Result<Option<FirmwareImage>> {
        let key = Self::cache_key(device_model, version);

        let entry = {
            let index = self.index.read().await;
            index.get(&key).cloned()
        };

        match entry {
            Some(cached) => {
                let data = tokio::fs::read(&cached.cache_path)
                    .await
                    .context("Failed to read cached firmware")?;

                let actual_hash = crate::delta::compute_data_hash(&data);

                if actual_hash != cached.hash {
                    warn!(
                        "Cached firmware hash mismatch, removing: {} v{}",
                        device_model, version
                    );
                    self.remove(device_model, version).await?;
                    return Ok(None);
                }

                let firmware = FirmwareImage {
                    device_model: cached.device_model,
                    version: cached.version,
                    min_hardware_version: None,
                    max_hardware_version: None,
                    data,
                    hash: cached.hash,
                    size_bytes: cached.size_bytes,
                    build_timestamp: cached.cached_at,
                    release_notes: None,
                    signature: cached.signature,
                };

                info!(
                    "Retrieved firmware from cache: {} v{}",
                    device_model, version
                );
                Ok(Some(firmware))
            }
            None => Ok(None),
        }
    }

    /// Check if a firmware image is in the cache
    pub async fn contains(&self, device_model: &str, version: &semver::Version) -> bool {
        let key = Self::cache_key(device_model, version);
        let index = self.index.read().await;
        index.contains_key(&key)
    }

    /// Remove a firmware image from the cache
    pub async fn remove(&self, device_model: &str, version: &semver::Version) -> Result<()> {
        let key = Self::cache_key(device_model, version);

        let entry = {
            let mut index = self.index.write().await;
            index.remove(&key)
        };

        if let Some(cached) = entry {
            if cached.cache_path.exists() {
                tokio::fs::remove_file(&cached.cache_path)
                    .await
                    .context("Failed to remove cached firmware file")?;
            }

            self.save_index().await?;

            info!("Removed firmware from cache: {} v{}", device_model, version);
        }

        Ok(())
    }

    /// Get the total size of cached firmware
    pub async fn get_cache_size(&self) -> u64 {
        let index = self.index.read().await;
        index.values().map(|e| e.size_bytes).sum()
    }

    /// Get the number of cached firmware images
    pub async fn get_cache_count(&self) -> usize {
        let index = self.index.read().await;
        index.len()
    }

    /// List all cached firmware
    pub async fn list(&self) -> Vec<CachedFirmware> {
        let index = self.index.read().await;
        index.values().cloned().collect()
    }

    async fn evict_oldest(&self, required_space: u64) -> Result<()> {
        let mut entries: Vec<_> = {
            let index = self.index.read().await;
            index.values().cloned().collect()
        };

        entries.sort_by_key(|a| a.cached_at);

        let mut freed_space = 0u64;
        for entry in entries {
            if freed_space >= required_space {
                break;
            }

            self.remove(&entry.device_model, &entry.version).await?;
            freed_space += entry.size_bytes;
        }

        Ok(())
    }

    /// Clear all cached firmware
    pub async fn clear(&self) -> Result<()> {
        let entries: Vec<_> = {
            let index = self.index.read().await;
            index
                .values()
                .map(|e| (e.device_model.clone(), e.version.clone()))
                .collect()
        };

        for (device_model, version) in entries {
            self.remove(&device_model, &version).await?;
        }

        info!("Cleared firmware cache");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_state_is_in_progress() {
        assert!(!UpdateState::Idle.is_in_progress());
        assert!(!UpdateState::Complete.is_in_progress());
        assert!(UpdateState::Verifying.is_in_progress());
        assert!(UpdateState::Flashing { progress: 50 }.is_in_progress());
    }

    #[test]
    fn test_update_state_should_block_ffb() {
        assert!(!UpdateState::Idle.should_block_ffb());
        assert!(UpdateState::Verifying.should_block_ffb());
        assert!(UpdateState::Flashing { progress: 50 }.should_block_ffb());
    }

    #[test]
    fn test_partition_other() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
    }

    #[tokio::test]
    async fn test_ffb_blocker_basic() -> Result<()> {
        let blocker = FfbBlocker::new();

        assert!(!blocker.is_ffb_blocked());

        blocker.begin_update("test-device").await?;
        assert!(blocker.is_ffb_blocked());

        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());

        Ok(())
    }

    #[tokio::test]
    async fn test_ffb_blocker_mutual_exclusion() -> Result<()> {
        let blocker = FfbBlocker::new();

        blocker.begin_update("device1").await?;

        let result = blocker.begin_update("device2").await;
        assert!(result.is_err());

        blocker.end_update().await;

        let result = blocker.begin_update("device2").await;
        assert!(result.is_ok());

        blocker.end_update().await;
        Ok(())
    }

    #[test]
    fn test_staged_rollout_config_default() {
        let config = StagedRolloutConfig::default();
        assert!(config.enabled);
        assert_eq!(config.stage1_max_devices, 10);
        assert!((config.min_success_rate - 0.95).abs() < 0.001);
    }
}
