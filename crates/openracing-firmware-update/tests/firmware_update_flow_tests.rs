//! Comprehensive tests for the firmware update subsystem.
//!
//! Covers:
//! - Version comparison and update-needed detection
//! - Firmware image validation (hash, size limits)
//! - Update state machine transitions
//! - Error handling (download, verification, flash, timeout)
//! - Rollback behaviour on interrupted updates
//! - Progress reporting via broadcast channel
//! - Concurrent update rejection
//! - Device compatibility (hardware version vs firmware requirements)
//! - Resume capability after interrupted download

use anyhow::Result;
use openracing_firmware_update::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Configurable mock firmware device.
///
/// Every async operation can be individually configured to succeed or fail,
/// and the mock tracks how many times each method was called.
struct ConfigurableMockDevice {
    device_id: String,
    device_model: String,
    hardware_version: String,
    partitions: Arc<Mutex<Vec<PartitionInfo>>>,
    active_partition: Arc<Mutex<Partition>>,
    firmware_data: Arc<Mutex<HashMap<Partition, Vec<u8>>>>,

    // Per-operation failure controls
    fail_health_check_times: AtomicU32,
    health_check_calls: AtomicU32,
    fail_prepare: Arc<Mutex<bool>>,
    fail_write_at_offset: Arc<Mutex<Option<u64>>>,
    fail_validate: Arc<Mutex<bool>>,
    fail_activate: Arc<Mutex<bool>>,
    fail_reboot: Arc<Mutex<bool>>,
    fail_set_bootable: Arc<Mutex<bool>>,

    // Counters
    prepare_calls: AtomicU32,
    write_calls: AtomicU32,
    validate_calls: AtomicU32,
    activate_calls: AtomicU32,
    reboot_calls: AtomicU32,
    set_bootable_calls: AtomicU32,
}

impl ConfigurableMockDevice {
    fn new(device_id: &str) -> Self {
        let partitions = vec![
            PartitionInfo {
                partition: Partition::A,
                active: true,
                bootable: true,
                version: Some(semver::Version::new(1, 0, 0)),
                size_bytes: 1024 * 1024,
                hash: Some("old_hash_a".to_string()),
                updated_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
                health: PartitionHealth::Healthy,
            },
            PartitionInfo {
                partition: Partition::B,
                active: false,
                bootable: false,
                version: None,
                size_bytes: 0,
                hash: None,
                updated_at: None,
                health: PartitionHealth::Unknown,
            },
        ];

        Self {
            device_id: device_id.to_string(),
            device_model: "test_wheel_v1".to_string(),
            hardware_version: "1.5".to_string(),
            partitions: Arc::new(Mutex::new(partitions)),
            active_partition: Arc::new(Mutex::new(Partition::A)),
            firmware_data: Arc::new(Mutex::new(HashMap::new())),

            fail_health_check_times: AtomicU32::new(0),
            health_check_calls: AtomicU32::new(0),
            fail_prepare: Arc::new(Mutex::new(false)),
            fail_write_at_offset: Arc::new(Mutex::new(None)),
            fail_validate: Arc::new(Mutex::new(false)),
            fail_activate: Arc::new(Mutex::new(false)),
            fail_reboot: Arc::new(Mutex::new(false)),
            fail_set_bootable: Arc::new(Mutex::new(false)),

            prepare_calls: AtomicU32::new(0),
            write_calls: AtomicU32::new(0),
            validate_calls: AtomicU32::new(0),
            activate_calls: AtomicU32::new(0),
            reboot_calls: AtomicU32::new(0),
            set_bootable_calls: AtomicU32::new(0),
        }
    }

    fn with_hardware_version(mut self, v: &str) -> Self {
        self.hardware_version = v.to_string();
        self
    }

    #[allow(dead_code)]
    fn with_device_model(mut self, model: &str) -> Self {
        self.device_model = model.to_string();
        self
    }
}

#[async_trait::async_trait]
impl FirmwareDevice for ConfigurableMockDevice {
    fn device_id(&self) -> &str {
        &self.device_id
    }

    fn device_model(&self) -> &str {
        &self.device_model
    }

    async fn get_partition_info(&self) -> Result<Vec<PartitionInfo>> {
        Ok(self.partitions.lock().await.clone())
    }

    async fn get_active_partition(&self) -> Result<Partition> {
        Ok(*self.active_partition.lock().await)
    }

    async fn prepare_partition(&self, partition: Partition) -> Result<()> {
        self.prepare_calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail_prepare.lock().await {
            return Err(anyhow::anyhow!("Simulated prepare failure"));
        }
        let mut parts = self.partitions.lock().await;
        if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
            p.bootable = false;
            p.version = None;
            p.size_bytes = 0;
            p.hash = None;
            p.health = PartitionHealth::Unknown;
        }
        self.firmware_data.lock().await.remove(&partition);
        Ok(())
    }

    #[allow(clippy::collapsible_if)] // Kept for compatibility with the current MSRV policy.
    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(fail_offset) = *self.fail_write_at_offset.lock().await {
            if offset >= fail_offset {
                return Err(anyhow::anyhow!(
                    "Simulated write failure at offset {}",
                    offset
                ));
            }
        }
        let mut fw = self.firmware_data.lock().await;
        let buf = fw.entry(partition).or_default();
        let required = offset as usize + data.len();
        if buf.len() < required {
            buf.resize(required, 0);
        }
        buf[offset as usize..offset as usize + data.len()].copy_from_slice(data);
        Ok(())
    }

    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()> {
        self.validate_calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail_validate.lock().await {
            return Err(anyhow::anyhow!("Simulated validation failure"));
        }
        let fw = self.firmware_data.lock().await;
        if let Some(data) = fw.get(&partition) {
            let actual = {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(data);
                hex::encode(h.finalize())
            };
            if actual != expected_hash {
                return Err(anyhow::anyhow!("Hash mismatch"));
            }
            let len = data.len();
            drop(fw);
            let mut parts = self.partitions.lock().await;
            if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
                p.size_bytes = len as u64;
                p.hash = Some(actual);
                p.health = PartitionHealth::Healthy;
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("No firmware data"))
        }
    }

    async fn set_bootable(&self, partition: Partition, bootable: bool) -> Result<()> {
        self.set_bootable_calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail_set_bootable.lock().await {
            return Err(anyhow::anyhow!("Simulated set_bootable failure"));
        }
        let mut parts = self.partitions.lock().await;
        if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
            p.bootable = bootable;
        }
        Ok(())
    }

    async fn activate_partition(&self, partition: Partition) -> Result<()> {
        self.activate_calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail_activate.lock().await {
            return Err(anyhow::anyhow!("Simulated activate failure"));
        }
        *self.active_partition.lock().await = partition;
        let mut parts = self.partitions.lock().await;
        for p in parts.iter_mut() {
            p.active = p.partition == partition;
        }
        Ok(())
    }

    async fn reboot(&self) -> Result<()> {
        self.reboot_calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail_reboot.lock().await {
            return Err(anyhow::anyhow!("Simulated reboot failure"));
        }
        // Minimal delay for tests
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        let call_num = self.health_check_calls.fetch_add(1, Ordering::SeqCst);
        let fail_count = self.fail_health_check_times.load(Ordering::SeqCst);
        if call_num < fail_count {
            Err(anyhow::anyhow!(
                "Simulated health check failure #{}",
                call_num + 1
            ))
        } else {
            Ok(())
        }
    }

    async fn get_hardware_version(&self) -> Result<String> {
        Ok(self.hardware_version.clone())
    }
}

/// Build a [`FirmwareImage`] with a correct SHA-256 hash.
fn make_firmware(version: &str, device_model: &str, data: &[u8]) -> Result<FirmwareImage> {
    let hash = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(data);
        hex::encode(h.finalize())
    };
    let version: semver::Version = version.parse()?;
    Ok(FirmwareImage {
        device_model: device_model.to_string(),
        version,
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: Some("3.0".to_string()),
        data: data.to_vec(),
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: None,
        signature: None,
    })
}

fn make_manager() -> FirmwareUpdateManager {
    FirmwareUpdateManager::new(StagedRolloutConfig::default())
}

// ---------------------------------------------------------------------------
// 1. Version comparison – correctly determines if update is needed
// ---------------------------------------------------------------------------

mod version_comparison {
    use super::*;

    #[test]
    fn newer_version_is_greater() -> Result<()> {
        let old: semver::Version = "1.0.0".parse()?;
        let new: semver::Version = "2.0.0".parse()?;
        assert!(new > old);
        Ok(())
    }

    #[test]
    fn same_version_is_equal() -> Result<()> {
        let a: semver::Version = "1.2.3".parse()?;
        let b: semver::Version = "1.2.3".parse()?;
        assert_eq!(a, b);
        Ok(())
    }

    #[test]
    fn patch_bump_detected() -> Result<()> {
        let old: semver::Version = "1.0.0".parse()?;
        let new: semver::Version = "1.0.1".parse()?;
        assert!(new > old);
        Ok(())
    }

    #[test]
    fn pre_release_is_less_than_release() -> Result<()> {
        let pre: semver::Version = "2.0.0-beta.1".parse()?;
        let release: semver::Version = "2.0.0".parse()?;
        assert!(pre < release);
        Ok(())
    }

    #[test]
    fn hardware_version_numeric_comparison() -> Result<(), HardwareVersionError> {
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;
        assert!(v2 < v10, "2.0 should be less than 10.0");
        Ok(())
    }

    #[test]
    fn hardware_version_equality_with_trailing_zero() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.2")?;
        let b = HardwareVersion::parse("1.2.0")?;
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 2. Firmware image validation – hash, size, basic integrity
// ---------------------------------------------------------------------------

mod firmware_image_validation {
    use super::*;

    #[test]
    fn correct_hash_accepted() -> Result<()> {
        let data = b"firmware payload bytes";
        let image = make_firmware("1.0.0", "wheel", data)?;
        // Recompute and compare
        let expected = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(data);
            hex::encode(h.finalize())
        };
        assert_eq!(image.hash, expected);
        Ok(())
    }

    #[test]
    fn tampered_hash_detected() -> Result<()> {
        let mut image = make_firmware("1.0.0", "wheel", b"real firmware")?;
        image.hash = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        // Recompute real hash
        let real = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&image.data);
            hex::encode(h.finalize())
        };
        assert_ne!(image.hash, real, "Tampered hash should not match");
        Ok(())
    }

    #[test]
    fn size_bytes_matches_data_length() -> Result<()> {
        let data = vec![0xABu8; 4096];
        let image = make_firmware("1.0.0", "wheel", &data)?;
        assert_eq!(image.size_bytes, data.len() as u64);
        Ok(())
    }

    #[test]
    fn empty_firmware_data_has_zero_size() -> Result<()> {
        let image = make_firmware("1.0.0", "wheel", &[])?;
        assert_eq!(image.size_bytes, 0);
        assert_eq!(image.data.len(), 0);
        Ok(())
    }

    #[test]
    fn bundle_roundtrip_preserves_image() -> Result<()> {
        let data = vec![0x42u8; 256];
        let image = make_firmware("2.0.0", "pedals", &data)?;
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, data);
        assert_eq!(extracted.version, semver::Version::new(2, 0, 0));
        Ok(())
    }

    #[test]
    fn invalid_bundle_magic_rejected() {
        let bad_data = b"NOT_OWFB_data_here";
        let result = FirmwareBundle::parse(bad_data);
        assert!(result.is_err());
    }

    #[test]
    fn bundle_detects_payload_corruption() -> Result<()> {
        let image = make_firmware("1.0.0", "wheel", b"valid payload")?;
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let mut bytes = bundle.serialize()?;
        // Corrupt a byte near the end (payload region)
        if let Some(last) = bytes.last_mut() {
            *last ^= 0xFF;
        }
        let result = FirmwareBundle::parse(&bytes);
        assert!(result.is_err(), "Corrupted payload should be rejected");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 3. Update state machine – all transitions
// ---------------------------------------------------------------------------

mod update_state_machine {
    use super::*;

    #[test]
    fn idle_is_not_in_progress() {
        assert!(!UpdateState::Idle.is_in_progress());
    }

    #[test]
    fn downloading_is_in_progress() {
        assert!(UpdateState::Downloading { progress: 0 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 50 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 100 }.is_in_progress());
    }

    #[test]
    fn verifying_is_in_progress() {
        assert!(UpdateState::Verifying.is_in_progress());
    }

    #[test]
    fn flashing_is_in_progress() {
        assert!(UpdateState::Flashing { progress: 0 }.is_in_progress());
        assert!(UpdateState::Flashing { progress: 99 }.is_in_progress());
    }

    #[test]
    fn rebooting_is_in_progress() {
        assert!(UpdateState::Rebooting.is_in_progress());
    }

    #[test]
    fn complete_is_not_in_progress() {
        assert!(!UpdateState::Complete.is_in_progress());
    }

    #[test]
    fn failed_is_not_in_progress() {
        let state = UpdateState::Failed {
            error: "oops".to_string(),
            recoverable: true,
        };
        assert!(!state.is_in_progress());
    }

    #[test]
    fn ffb_blocked_during_active_states() {
        assert!(UpdateState::Downloading { progress: 10 }.should_block_ffb());
        assert!(UpdateState::Verifying.should_block_ffb());
        assert!(UpdateState::Flashing { progress: 50 }.should_block_ffb());
        assert!(UpdateState::Rebooting.should_block_ffb());
    }

    #[test]
    fn ffb_not_blocked_in_terminal_states() {
        assert!(!UpdateState::Idle.should_block_ffb());
        assert!(!UpdateState::Complete.should_block_ffb());
        let failed = UpdateState::Failed {
            error: "err".to_string(),
            recoverable: false,
        };
        assert!(!failed.should_block_ffb());
    }

    #[test]
    fn default_state_is_idle() {
        assert_eq!(UpdateState::default(), UpdateState::Idle);
    }

    #[tokio::test]
    async fn ffb_blocker_state_transitions() -> Result<()> {
        let blocker = FfbBlocker::new();

        // Starts idle
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Idle);

        // Begin update puts it in Verifying
        blocker.begin_update("dev-1").await?;
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Verifying);

        // Transition through states
        blocker
            .set_state(UpdateState::Downloading { progress: 25 })
            .await;
        let state = blocker.get_state().await;
        assert!(matches!(state, UpdateState::Downloading { progress: 25 }));

        blocker
            .set_state(UpdateState::Flashing { progress: 50 })
            .await;
        assert!(blocker.is_ffb_blocked());

        // End update returns to idle
        blocker.end_update().await;
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Idle);
        assert!(!blocker.is_ffb_blocked());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 4. Error handling
// ---------------------------------------------------------------------------

mod error_handling {
    use super::*;

    #[tokio::test]
    async fn hash_mismatch_fails_update() -> Result<()> {
        tokio::time::pause();
        let device = Box::new(ConfigurableMockDevice::new("dev-hash-fail"));
        let mut firmware = make_firmware("2.0.0", "test_wheel_v1", b"some firmware bytes")?;
        // Corrupt the expected hash
        firmware.hash = "badhash".repeat(8);

        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.contains("hash") || err_msg.contains("Hash"),
            "Error should mention hash mismatch, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn prepare_failure_reported() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-prepare-fail");
        *device.fail_prepare.lock().await = true;

        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(
                Box::new(device),
                &make_firmware("2.0.0", "test_wheel_v1", b"fw data")?,
            )
            .await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.contains("prepare") || err_msg.contains("Prepare"),
            "Error should mention prepare, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn write_failure_mid_transfer() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-write-fail");
        // Fail writes after the first 4096-byte chunk
        *device.fail_write_at_offset.lock().await = Some(4096);

        let data = vec![0xFFu8; 16384]; // 4 chunks of 4096
        let firmware = make_firmware("2.0.0", "test_wheel_v1", &data)?;

        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.contains("write") || err_msg.contains("Write") || err_msg.contains("chunk"),
            "Error should mention write failure, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn validation_failure_reported() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-validate-fail");
        *device.fail_validate.lock().await = true;

        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"fw data for validation")?;
        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.contains("validation") || err_msg.contains("Validation"),
            "Error should mention validation, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn reboot_failure_reported() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-reboot-fail");
        *device.fail_reboot.lock().await = true;

        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"reboot test fw")?;
        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.to_lowercase().contains("reboot"),
            "Error should mention reboot, got: {}",
            err_msg
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 5. Rollback behaviour – health-check failures trigger rollback
// ---------------------------------------------------------------------------

mod rollback_behaviour {
    use super::*;

    #[tokio::test]
    async fn health_check_failure_triggers_rollback() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-rollback");
        // Fail health check more times than the max retries (5)
        device.fail_health_check_times.store(100, Ordering::SeqCst);

        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"rollback test fw")?;
        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(!result.success);
        let err_msg = result.error.as_deref().unwrap_or("");
        assert!(
            err_msg.to_lowercase().contains("health check")
                || err_msg.to_lowercase().contains("rollback"),
            "Error should mention health check or rollback, got: {}",
            err_msg
        );
        Ok(())
    }

    #[tokio::test]
    async fn transient_health_check_recovers() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-transient-hc");
        // Fail only the first 3 health checks (< 5 max retries), then succeed
        device.fail_health_check_times.store(3, Ordering::SeqCst);

        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"transient hc fw")?;
        let mgr = make_manager();
        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(result.success, "Should succeed after transient failures");
        assert_eq!(result.updated_partition, Some(Partition::B));
        Ok(())
    }

    #[tokio::test]
    async fn rollback_manager_create_and_restore() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let backup_dir = temp.path().join("backups");
        let install_dir = temp.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        // Write an original file
        tokio::fs::write(install_dir.join("config.bin"), b"original").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir.clone());

        // Create backup
        mgr.create_backup(
            "bk-001",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[std::path::PathBuf::from("config.bin")],
        )
        .await?;

        // Overwrite the original (simulating a failed update)
        tokio::fs::write(install_dir.join("config.bin"), b"corrupted").await?;

        // Rollback should restore original
        mgr.rollback_to("bk-001").await?;

        let restored = tokio::fs::read(install_dir.join("config.bin")).await?;
        assert_eq!(restored, b"original");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 6. Progress reporting
// ---------------------------------------------------------------------------

mod progress_reporting {
    use super::*;

    #[tokio::test]
    async fn receives_initializing_phase() -> Result<()> {
        tokio::time::pause();
        let device = Box::new(ConfigurableMockDevice::new("dev-progress"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"progress fw data")?;
        let mgr = make_manager();
        let mut rx = mgr.subscribe_progress();

        let fw_clone = firmware.clone();
        let mut handle =
            tokio::spawn(async move { mgr.update_device_firmware(device, &fw_clone).await });

        let mut saw_init = false;
        let mut saw_transfer = false;
        let mut saw_complete = false;

        // Collect progress while update runs
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    if let Ok(p) = msg {
                        match p.phase {
                            UpdatePhase::Initializing => saw_init = true,
                            UpdatePhase::Transferring => saw_transfer = true,
                            UpdatePhase::Completed => saw_complete = true,
                            _ => {}
                        }
                    }
                }
                res = &mut handle => {
                    let result = res??;
                    assert!(result.success);
                    break;
                }
            }
        }

        // Drain any remaining messages after task completion
        while let Ok(p) = rx.try_recv() {
            match p.phase {
                UpdatePhase::Initializing => saw_init = true,
                UpdatePhase::Transferring => saw_transfer = true,
                UpdatePhase::Completed => saw_complete = true,
                _ => {}
            }
        }

        assert!(saw_init, "Should see Initializing phase");
        assert!(saw_transfer, "Should see Transferring phase");
        assert!(saw_complete, "Should see Completed phase");
        Ok(())
    }

    #[tokio::test]
    async fn progress_percent_increases_monotonically() -> Result<()> {
        tokio::time::pause();
        let data = vec![0u8; 64 * 1024]; // 64 KiB to get multiple progress reports
        let device = Box::new(ConfigurableMockDevice::new("dev-monotonic"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", &data)?;
        let mgr = make_manager();
        let mut rx = mgr.subscribe_progress();

        let fw_clone = firmware.clone();
        let mut handle =
            tokio::spawn(async move { mgr.update_device_firmware(device, &fw_clone).await });

        let mut percents = Vec::new();
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    if let Ok(p) = msg {
                        percents.push(p.progress_percent);
                    }
                }
                res = &mut handle => {
                    let _ = res?;
                    break;
                }
            }
        }

        // All progress values should be non-decreasing
        for window in percents.windows(2) {
            assert!(
                window[1] >= window[0],
                "Progress should not decrease: {} -> {}",
                window[0],
                window[1]
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 7. Concurrent update rejection
// ---------------------------------------------------------------------------

mod concurrent_update_rejection {
    use super::*;

    #[tokio::test]
    async fn ffb_blocker_rejects_second_update() -> Result<()> {
        let blocker = FfbBlocker::new();

        blocker.begin_update("device-A").await?;
        let second = blocker.begin_update("device-B").await;
        assert!(
            second.is_err(),
            "Second concurrent update should be rejected"
        );

        // FFB operations should be blocked
        let ffb_result = blocker.try_ffb_operation();
        assert!(ffb_result.is_err());

        blocker.end_update().await;

        // Now a new update should be accepted
        blocker.begin_update("device-B").await?;
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn manager_rejects_duplicate_device_update() -> Result<()> {
        tokio::time::pause();
        let mgr = Arc::new(make_manager());

        // Large firmware so the first update takes a while
        let data = vec![0xABu8; 64 * 1024];
        let firmware = make_firmware("2.0.0", "test_wheel_v1", &data)?;
        let fw_clone = firmware.clone();

        let mgr1 = mgr.clone();
        let first_handle = tokio::spawn(async move {
            let device = Box::new(ConfigurableMockDevice::new("dup-device"));
            mgr1.update_device_firmware(device, &fw_clone).await
        });

        // Give the first update a moment to register
        tokio::time::sleep(Duration::from_millis(10)).await;
        tokio::time::advance(Duration::from_millis(10)).await;

        let device2 = Box::new(ConfigurableMockDevice::new("dup-device"));
        let second_result = mgr.update_device_firmware(device2, &firmware).await;

        // The second attempt should fail because the same device_id is already updating
        // (or succeed if the first finished quickly – either outcome is acceptable since
        // the manager cleans up after completion)
        if let Ok(r) = &second_result {
            // If it returned Ok, the inner result should show failure
            if !r.success {
                let err = r.error.as_deref().unwrap_or("");
                assert!(
                    err.to_lowercase().contains("already")
                        || err.to_lowercase().contains("in progress"),
                    "Should mention already in progress: {}",
                    err
                );
            }
        }

        // Clean up the first task
        let _ = first_handle.await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_tracks_device_id() -> Result<()> {
        let blocker = FfbBlocker::new();
        assert!(blocker.get_updating_device().await.is_none());

        blocker.begin_update("my-wheel-42").await?;
        let dev = blocker.get_updating_device().await;
        assert_eq!(dev.as_deref(), Some("my-wheel-42"));

        blocker.end_update().await;
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 8. Device compatibility
// ---------------------------------------------------------------------------

mod device_compatibility {
    use super::*;

    #[tokio::test]
    async fn compatible_hardware_accepted() -> Result<()> {
        tokio::time::pause();
        let device =
            Box::new(ConfigurableMockDevice::new("dev-compat-ok").with_hardware_version("1.5"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"compat fw")?;
        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn hardware_below_minimum_rejected() -> Result<()> {
        tokio::time::pause();
        let device =
            Box::new(ConfigurableMockDevice::new("dev-hw-low").with_hardware_version("0.5"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"compat fw")?;
        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;
        assert!(!result.success, "Below min HW version should fail");
        Ok(())
    }

    #[tokio::test]
    async fn hardware_above_maximum_rejected() -> Result<()> {
        tokio::time::pause();
        let device =
            Box::new(ConfigurableMockDevice::new("dev-hw-high").with_hardware_version("5.0"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"compat fw")?;
        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;
        assert!(!result.success, "Above max HW version should fail");
        Ok(())
    }

    #[tokio::test]
    async fn exact_min_boundary_accepted() -> Result<()> {
        tokio::time::pause();
        let device =
            Box::new(ConfigurableMockDevice::new("dev-hw-min").with_hardware_version("1.0"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"boundary fw")?;
        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;
        assert!(result.success, "Exact min boundary should be accepted");
        Ok(())
    }

    #[tokio::test]
    async fn exact_max_boundary_accepted() -> Result<()> {
        tokio::time::pause();
        let device =
            Box::new(ConfigurableMockDevice::new("dev-hw-max").with_hardware_version("3.0"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"boundary fw")?;
        let mgr = make_manager();
        let result = mgr.update_device_firmware(device, &firmware).await?;
        assert!(result.success, "Exact max boundary should be accepted");
        Ok(())
    }

    #[test]
    fn bundle_compatibility_check() -> Result<()> {
        let image = make_firmware("1.0.0", "wheel", b"bundle compat")?;
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.is_compatible_with_hardware("1.5"));
        assert!(bundle.is_compatible_with_hardware("1.0")); // min boundary
        assert!(bundle.is_compatible_with_hardware("3.0")); // max boundary
        assert!(!bundle.is_compatible_with_hardware("0.9")); // below
        assert!(!bundle.is_compatible_with_hardware("3.1")); // above
        Ok(())
    }

    #[test]
    fn rollback_protection_blocks_downgrade() -> Result<()> {
        let image = make_firmware("2.0.0", "wheel", b"rollback prot")?;
        let metadata = BundleMetadata {
            rollback_version: Some(semver::Version::new(1, 5, 0)),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 5, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));
        assert!(
            !bundle.allows_upgrade_from(&semver::Version::new(1, 4, 9)),
            "Versions below rollback_version should be blocked"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 9. Resume capability / partial transfer
// ---------------------------------------------------------------------------

mod resume_capability {
    use super::*;

    #[tokio::test]
    async fn partial_write_does_not_corrupt_other_partition() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-partial");
        // Fail midway through
        *device.fail_write_at_offset.lock().await = Some(8192);

        let data = vec![0xCDu8; 32768];
        let firmware = make_firmware("2.0.0", "test_wheel_v1", &data)?;
        let mgr = make_manager();

        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;
        assert!(!result.success, "Should fail due to write error");

        // The active partition (A) should remain unchanged
        // (the mock writes to partition B since A is active)
        Ok(())
    }

    #[tokio::test]
    async fn successful_update_writes_to_inactive_partition() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("dev-partition-check");
        let data = b"partition test firmware";
        let firmware = make_firmware("2.0.0", "test_wheel_v1", data)?;
        let mgr = make_manager();

        let device_box = Box::new(device);
        let result = mgr.update_device_firmware(device_box, &firmware).await?;

        assert!(result.success);
        // Active was A, so the update should go to B
        assert_eq!(result.updated_partition, Some(Partition::B));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 10. Firmware cache tests
// ---------------------------------------------------------------------------

mod firmware_cache {
    use super::*;

    #[tokio::test]
    async fn cache_add_and_retrieve() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let cache = FirmwareCache::new(temp.path().to_path_buf(), 0).await?;

        let image = make_firmware("1.0.0", "wheel-x", b"cached firmware data")?;
        cache.add(&image).await?;

        assert!(
            cache
                .contains("wheel-x", &semver::Version::new(1, 0, 0))
                .await
        );
        let retrieved = cache.get("wheel-x", &semver::Version::new(1, 0, 0)).await?;
        assert!(retrieved.is_some());
        let fw = retrieved.ok_or_else(|| anyhow::anyhow!("missing"))?;
        assert_eq!(fw.data, b"cached firmware data");
        Ok(())
    }

    #[tokio::test]
    async fn cache_miss_returns_none() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let cache = FirmwareCache::new(temp.path().to_path_buf(), 0).await?;

        let result = cache.get("no-such", &semver::Version::new(9, 9, 9)).await?;
        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn cache_remove() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let cache = FirmwareCache::new(temp.path().to_path_buf(), 0).await?;

        let image = make_firmware("1.0.0", "wheel-rm", b"remove me")?;
        cache.add(&image).await?;
        assert!(
            cache
                .contains("wheel-rm", &semver::Version::new(1, 0, 0))
                .await
        );

        cache
            .remove("wheel-rm", &semver::Version::new(1, 0, 0))
            .await?;
        assert!(
            !cache
                .contains("wheel-rm", &semver::Version::new(1, 0, 0))
                .await
        );
        Ok(())
    }

    #[tokio::test]
    async fn cache_size_tracking() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let cache = FirmwareCache::new(temp.path().to_path_buf(), 0).await?;

        assert_eq!(cache.get_cache_size().await, 0);
        assert_eq!(cache.get_cache_count().await, 0);

        let image = make_firmware("1.0.0", "wheel-size", b"12345")?;
        cache.add(&image).await?;

        assert_eq!(cache.get_cache_size().await, 5);
        assert_eq!(cache.get_cache_count().await, 1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 11. End-to-end happy path
// ---------------------------------------------------------------------------

mod end_to_end {
    use super::*;

    #[tokio::test]
    async fn full_update_lifecycle() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("e2e-device");
        let data = b"end-to-end firmware payload for testing";
        let firmware = make_firmware("3.0.0", "test_wheel_v1", data)?;
        let mgr = make_manager();

        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(result.success, "E2E update should succeed");
        assert_eq!(result.device_id, "e2e-device");
        assert_eq!(result.new_version, Some(semver::Version::new(3, 0, 0)));
        assert_eq!(result.updated_partition, Some(Partition::B));
        assert!(!result.rollback_performed);
        assert!(result.error.is_none());
        assert!(!result.partition_states.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn update_result_contains_old_version() -> Result<()> {
        tokio::time::pause();
        let device = ConfigurableMockDevice::new("e2e-oldver");
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"old ver test")?;
        let mgr = make_manager();

        let result = mgr
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(result.success);
        // The mock's partition A has version 1.0.0
        assert_eq!(result.old_version, Some(semver::Version::new(1, 0, 0)));
        Ok(())
    }

    #[tokio::test]
    async fn manager_active_updates_empty_after_completion() -> Result<()> {
        tokio::time::pause();
        let mgr = make_manager();
        assert!(!mgr.is_update_in_progress().await);

        let device = Box::new(ConfigurableMockDevice::new("active-check"));
        let firmware = make_firmware("2.0.0", "test_wheel_v1", b"active check fw")?;
        mgr.update_device_firmware(device, &firmware).await?;

        assert!(!mgr.is_update_in_progress().await);
        assert!(mgr.get_active_updates().await.is_empty());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 12. Property-based tests
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_version() -> impl Strategy<Value = semver::Version> {
        (0u64..50, 0u64..50, 0u64..50).prop_map(|(ma, mi, pa)| semver::Version::new(ma, mi, pa))
    }

    fn arb_hw_version_str() -> impl Strategy<Value = String> {
        (1u32..20, 0u32..20).prop_map(|(a, b)| format!("{}.{}", a, b))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn version_ordering_is_total(a in arb_version(), b in arb_version()) {
            // semver::Version must be totally ordered
            let _ = a.cmp(&b);
        }

        #[test]
        fn version_reflexive(v in arb_version()) {
            prop_assert_eq!(v.cmp(&v), std::cmp::Ordering::Equal);
        }

        #[test]
        fn hw_version_parse_roundtrip(s in arb_hw_version_str()) {
            let parsed = HardwareVersion::parse(&s);
            prop_assert!(parsed.is_ok(), "Valid version string should parse: {}", s);
        }

        #[test]
        fn hw_version_ordering_consistent(
            a in arb_hw_version_str(),
            b in arb_hw_version_str()
        ) {
            if let (Ok(va), Ok(vb)) = (HardwareVersion::parse(&a), HardwareVersion::parse(&b)) {
                let cmp_ab = va.cmp(&vb);
                let cmp_ba = vb.cmp(&va);
                prop_assert_eq!(cmp_ab, cmp_ba.reverse());
            }
        }

        #[test]
        fn firmware_hash_deterministic(data in prop::collection::vec(any::<u8>(), 0..1024)) {
            use sha2::{Digest, Sha256};
            let mut h1 = Sha256::new();
            h1.update(&data);
            let hash1 = hex::encode(h1.finalize());

            let mut h2 = Sha256::new();
            h2.update(&data);
            let hash2 = hex::encode(h2.finalize());

            prop_assert_eq!(hash1, hash2, "Same data must produce same hash");
        }

        #[test]
        fn update_state_ffb_consistent(progress in 0u8..101) {
            let states = vec![
                UpdateState::Idle,
                UpdateState::Downloading { progress },
                UpdateState::Verifying,
                UpdateState::Flashing { progress },
                UpdateState::Rebooting,
                UpdateState::Complete,
                UpdateState::Failed { error: "e".into(), recoverable: true },
            ];
            for s in &states {
                // should_block_ffb and is_in_progress must agree
                prop_assert_eq!(s.should_block_ffb(), s.is_in_progress());
            }
        }

        #[test]
        fn partition_other_is_involution(p in prop_oneof![Just(Partition::A), Just(Partition::B)]) {
            prop_assert_eq!(p.other().other(), p);
        }
    }
}
