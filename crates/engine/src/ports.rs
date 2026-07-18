//! Port traits for clean architecture boundaries
//!
//! This module defines the port interfaces that separate the domain layer
//! from infrastructure concerns. These traits define contracts for external
//! dependencies without coupling to specific implementations.

use crate::hid::MozaInputState;
use crate::{DeviceEvent, DeviceInfo, DeviceInputs, RTResult, TelemetryData};
use async_trait::async_trait;
use racing_wheel_schemas::prelude::*;
use tokio::sync::mpsc;

/// HID device abstraction for real-time operations
///
/// This trait defines the contract for communicating with racing wheel hardware
/// at the lowest level. Implementations must be RT-safe for write operations.
pub trait HidDevice: Send + Sync {
    /// Write force feedback report (RT-safe, non-blocking)
    ///
    /// This method MUST be real-time safe:
    /// - No heap allocations
    /// - No blocking system calls
    /// - No locks that can block
    /// - Execution time must be bounded and predictable
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult;

    /// Read device telemetry (non-RT, async)
    ///
    /// This method is called from non-RT threads and can perform
    /// blocking I/O operations.
    fn read_telemetry(&mut self) -> Option<TelemetryData>;

    /// Get device capabilities (cached, RT-safe)
    fn capabilities(&self) -> &DeviceCapabilities;

    /// Get device info (cached, RT-safe)
    fn device_info(&self) -> &DeviceInfo;

    /// Check if device is connected (RT-safe)
    fn is_connected(&self) -> bool;

    /// Get device health status (non-RT)
    fn health_status(&self) -> DeviceHealthStatus;

    /// Read the latest decoded non-OWP Moza input snapshot when available.
    ///
    /// Implementations that do not expose these fields return `None`.
    fn moza_input_state(&self) -> Option<MozaInputState> {
        None
    }

    /// Read the latest decoded generic non-RT input snapshot for UI, diagnostics,
    /// and mode-aware safety logic.
    ///
    /// Implementations that do not expose these fields return `None`.
    fn read_inputs(&self) -> Option<DeviceInputs> {
        None
    }
}

/// Input-state reader for non-telemetry HID snapshots.
///
/// This trait keeps input decode pathways separate from telemetry packets
/// and avoids overloading a telemetry read call when devices expose richer
/// report layouts (for example Moza aggregated wheel inputs).
pub trait HidInputDevice: Send + Sync {
    /// Read the latest decoded Moza-style input snapshot from this device.
    fn read_inputs(&self) -> Option<DeviceInputs> {
        None
    }

    /// Backward-compatible typed snapshot API.
    fn moza_input_state(&self) -> Option<MozaInputState>;
}

/// Engine-internal extension for constructing a canonical [`DeviceInputs`]
/// snapshot from a decoded Moza input state.
///
/// This lives in the engine rather than in `openracing-device-types` because it
/// depends on the engine-internal [`MozaInputState`]; the domain crate stays
/// vendor-neutral. Bring this trait into scope to use the
/// `DeviceInputs::from_moza_input_state` associated function.
pub(crate) trait DeviceInputsMozaExt {
    /// Build a [`DeviceInputs`] snapshot from a decoded Moza input state.
    fn from_moza_input_state(state: &MozaInputState) -> Self;
}

impl DeviceInputsMozaExt for DeviceInputs {
    #[allow(dead_code)]
    fn from_moza_input_state(state: &MozaInputState) -> Self {
        let mut inputs = DeviceInputs {
            tick: state.tick,
            buttons: state.buttons,
            hat: state.hat,
            steering: Some(state.steering_u16),
            throttle: Some(state.throttle_u16),
            brake: Some(state.brake_u16),
            clutch_left: state.ks_snapshot.clutch_left,
            clutch_right: state.ks_snapshot.clutch_right,
            clutch_combined: state.ks_snapshot.clutch_combined,
            clutch_left_button: state.ks_snapshot.clutch_left_button,
            clutch_right_button: state.ks_snapshot.clutch_right_button,
            handbrake: Some(state.handbrake_u16),
            rotaries: [0i16; 8],
        };

        if inputs.clutch_left.is_none() && inputs.clutch_right.is_none() {
            inputs.clutch_left = state
                .ks_snapshot
                .clutch_left
                .or(state.ks_snapshot.clutch_combined)
                .or(Some(state.clutch_u16));
        }

        if inputs.clutch_combined.is_none() && inputs.clutch_left.is_none() {
            inputs.clutch_combined = Some(state.clutch_u16);
        }

        inputs.rotaries[0] = i16::from(state.rotary[0]);
        inputs.rotaries[1] = i16::from(state.rotary[1]);
        for idx in 2..inputs.rotaries.len() {
            inputs.rotaries[idx] = state.ks_snapshot.encoders[idx];
        }
        inputs
    }
}

/// Device health status information
#[derive(Debug, Clone)]
pub struct DeviceHealthStatus {
    /// Device temperature in degrees Celsius
    pub temperature_c: u8,
    /// Bitmask of active fault flags
    pub fault_flags: u8,
    /// Whether hands-on-wheel is currently detected
    pub hands_on: bool,
    /// Timestamp of the last successful communication with the device
    pub last_communication: std::time::Instant,
    /// Running count of communication errors since connection
    pub communication_errors: u32,
}

/// HID port abstraction for device enumeration and management
///
/// This trait defines the contract for discovering and opening HID devices.
/// It abstracts platform-specific device enumeration and connection logic.
#[async_trait]
pub trait HidPort: Send + Sync {
    /// List all available racing wheel devices
    ///
    /// Returns a list of device information for all compatible racing wheels
    /// currently connected to the system.
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>>;

    /// Open a device by ID for communication
    ///
    /// Returns a HidDevice instance that can be used for real-time communication
    /// with the specified device.
    async fn open_device(
        &self,
        id: &DeviceId,
    ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>>;

    /// Monitor for device connect/disconnect events
    ///
    /// Returns a receiver that will receive events when devices are connected
    /// or disconnected from the system.
    async fn monitor_devices(
        &self,
    ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>>;

    /// Refresh device list (force re-enumeration)
    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>>;
}

/// Telemetry data from racing games
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NormalizedTelemetry {
    /// Force feedback scalar from game (-1.0 to 1.0)
    pub ffb_scalar: f32,

    /// Engine RPM
    pub rpm: f32,

    /// Vehicle speed in m/s
    pub speed_ms: f32,

    /// Tire slip ratio (0.0 = no slip, 1.0 = full slip)
    pub slip_ratio: f32,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: i8,

    /// Racing flags and status
    pub flags: TelemetryFlags,

    /// Car identifier (if available)
    pub car_id: Option<String>,

    /// Track identifier (if available)
    pub track_id: Option<String>,

    /// Timestamp when telemetry was captured
    #[serde(skip, default = "std::time::Instant::now")]
    pub timestamp: std::time::Instant,
}

/// Racing flags and status information
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TelemetryFlags {
    /// Yellow caution flag is active
    pub yellow_flag: bool,
    /// Red flag / session stopped
    pub red_flag: bool,
    /// Blue flag — faster car approaching
    pub blue_flag: bool,
    /// Checkered flag — session complete
    pub checkered_flag: bool,
    /// Pit-lane speed limiter is engaged
    pub pit_limiter: bool,
    /// Drag Reduction System is enabled
    pub drs_enabled: bool,
    /// Energy Recovery System energy is available
    pub ers_available: bool,
    /// Vehicle is currently in the pit lane
    pub in_pit: bool,
}

/// Telemetry port abstraction for game integration
///
/// This trait defines the contract for receiving telemetry data from racing games.
/// Implementations handle game-specific protocols and normalize the data.
#[async_trait]
pub trait TelemetryPort: Send + Sync {
    /// Get the game identifier this port handles
    fn game_id(&self) -> &str;

    /// Configure the game for telemetry output
    ///
    /// This method should modify game configuration files to enable
    /// telemetry output in the format expected by this port.
    async fn configure_game(
        &self,
        install_path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Start monitoring for telemetry data
    ///
    /// Returns a receiver that will receive normalized telemetry data
    /// from the game at the game's update rate.
    async fn start_monitoring(
        &self,
    ) -> Result<mpsc::Receiver<NormalizedTelemetry>, Box<dyn std::error::Error>>;

    /// Stop monitoring telemetry data
    async fn stop_monitoring(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Check if telemetry is currently active
    fn is_monitoring(&self) -> bool;

    /// Get telemetry statistics
    fn get_statistics(&self) -> TelemetryStatistics;

    /// Validate game installation and telemetry configuration
    async fn validate_configuration(
        &self,
        install_path: &std::path::Path,
    ) -> Result<ConfigurationStatus, Box<dyn std::error::Error>>;
}

/// Telemetry statistics for monitoring health
#[derive(Debug, Clone, Default)]
pub struct TelemetryStatistics {
    /// Total number of telemetry packets received
    pub packets_received: u64,
    /// Number of packets dropped due to back-pressure or errors
    pub packets_dropped: u64,
    /// Timestamp of the most recent packet, if any
    pub last_packet_time: Option<std::time::Instant>,
    /// Smoothed average packet arrival rate in Hz
    pub average_rate_hz: f32,
    /// Running count of connection-level errors
    pub connection_errors: u32,
}

/// Configuration validation status
#[derive(Debug, Clone)]
pub struct ConfigurationStatus {
    /// Whether the current configuration is valid
    pub is_valid: bool,
    /// Detected game version string, if available
    pub game_version: Option<String>,
    /// Whether telemetry output is currently enabled in game config
    pub telemetry_enabled: bool,
    /// List of config changes that would be applied to enable telemetry
    pub expected_config_changes: Vec<ConfigChange>,
    /// Human-readable descriptions of any configuration issues found
    pub issues: Vec<String>,
}

/// Configuration change description
#[derive(Debug, Clone)]
pub struct ConfigChange {
    /// Path to the configuration file that would be modified
    pub file_path: std::path::PathBuf,
    /// Section within the file (e.g. INI section name)
    pub section: Option<String>,
    /// Key name of the setting
    pub key: String,
    /// Value required for telemetry to work correctly
    pub expected_value: String,
    /// Current value in the file, if any
    pub current_value: Option<String>,
}

/// Profile repository abstraction for persistence
///
/// This trait defines the contract for storing and retrieving profile configurations.
/// It abstracts the underlying storage mechanism (filesystem, database, etc.).
#[async_trait]
pub trait ProfileRepo: Send + Sync {
    /// Load a profile by ID
    async fn load_profile(&self, id: &ProfileId) -> Result<Profile, ProfileRepoError>;

    /// Save a profile
    async fn save_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError>;

    /// Delete a profile by ID
    async fn delete_profile(&self, id: &ProfileId) -> Result<(), ProfileRepoError>;

    /// List all available profiles
    async fn list_profiles(&self) -> Result<Vec<ProfileId>, ProfileRepoError>;

    /// Find profiles matching a scope
    async fn find_profiles_for_scope(
        &self,
        scope: &ProfileScope,
    ) -> Result<Vec<Profile>, ProfileRepoError>;

    /// Load the global default profile
    async fn load_global_profile(&self) -> Result<Profile, ProfileRepoError>;

    /// Save the global default profile
    async fn save_global_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError>;

    /// Check if a profile exists
    async fn profile_exists(&self, id: &ProfileId) -> Result<bool, ProfileRepoError>;

    /// Get profile metadata without loading full profile
    async fn get_profile_metadata(
        &self,
        id: &ProfileId,
    ) -> Result<ProfileMetadata, ProfileRepoError>;

    /// Backup profiles to a specified location
    async fn backup_profiles(&self, backup_path: &std::path::Path) -> Result<(), ProfileRepoError>;

    /// Restore profiles from a backup
    async fn restore_profiles(&self, backup_path: &std::path::Path)
    -> Result<(), ProfileRepoError>;

    /// Validate profile repository integrity
    async fn validate_repository(&self) -> Result<RepositoryStatus, ProfileRepoError>;
}

/// Profile repository error types
#[derive(Debug, thiserror::Error)]
pub enum ProfileRepoError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(ProfileId),

    #[error("Profile validation failed: {0}")]
    ValidationError(#[from] DomainError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Repository corruption detected: {0}")]
    CorruptionError(String),

    #[error("Permission denied: {0}")]
    PermissionError(String),

    #[error("Repository locked by another process")]
    LockError,

    #[error("Backup/restore error: {0}")]
    BackupError(String),
}

/// Repository health and status information
#[derive(Debug, Clone)]
pub struct RepositoryStatus {
    /// Overall repository health flag
    pub is_healthy: bool,
    /// Total number of profiles stored
    pub total_profiles: usize,
    /// IDs of profiles whose data failed integrity checks
    pub corrupted_profiles: Vec<ProfileId>,
    /// Expected profile files that could not be found on disk
    pub missing_files: Vec<std::path::PathBuf>,
    /// Paths where the current user lacks required permissions
    pub permission_issues: Vec<std::path::PathBuf>,
    /// Timestamp of the most recent backup, if any
    pub last_backup: Option<std::time::SystemTime>,
    /// Total disk space consumed by the profile repository
    pub disk_usage_bytes: u64,
}

/// Context information for profile resolution
#[derive(Debug, Clone)]
pub struct ProfileContext {
    /// Active game identifier (e.g. `"iracing"`)
    pub game: Option<String>,
    /// Active car identifier
    pub car: Option<String>,
    /// Active track identifier
    pub track: Option<String>,
    /// Connected device ID
    pub device_id: DeviceId,
    /// Session type (e.g. `"race"`, `"practice"`)
    pub session_type: Option<String>,
}

impl ProfileContext {
    /// Create a new profile context
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            game: None,
            car: None,
            track: None,
            device_id,
            session_type: None,
        }
    }

    /// Set game context
    pub fn with_game(mut self, game: String) -> Self {
        self.game = Some(game);
        self
    }

    /// Set car context
    pub fn with_car(mut self, car: String) -> Self {
        self.car = Some(car);
        self
    }

    /// Set track context
    pub fn with_track(mut self, track: String) -> Self {
        self.track = Some(track);
        self
    }

    /// Set session type context
    pub fn with_session_type(mut self, session_type: String) -> Self {
        self.session_type = Some(session_type);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_context_creation() -> Result<(), Box<dyn std::error::Error>> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let context = ProfileContext::new(device_id.clone());

        assert_eq!(context.device_id, device_id);
        assert!(context.game.is_none());
        assert!(context.car.is_none());
        assert!(context.track.is_none());
        Ok(())
    }

    #[test]
    fn test_profile_context_builder() -> Result<(), Box<dyn std::error::Error>> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let context = ProfileContext::new(device_id.clone())
            .with_game("iracing".to_string())
            .with_car("gt3".to_string())
            .with_track("spa".to_string())
            .with_session_type("race".to_string());

        assert_eq!(context.device_id, device_id);
        assert_eq!(context.game, Some("iracing".to_string()));
        assert_eq!(context.car, Some("gt3".to_string()));
        assert_eq!(context.track, Some("spa".to_string()));
        assert_eq!(context.session_type, Some("race".to_string()));
        Ok(())
    }

    #[test]
    fn test_telemetry_flags_default() {
        let flags = TelemetryFlags::default();
        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.drs_enabled);
        assert!(!flags.ers_available);
        assert!(!flags.in_pit);
    }

    #[test]
    fn test_telemetry_statistics_default() {
        let stats = TelemetryStatistics::default();
        assert_eq!(stats.packets_received, 0);
        assert_eq!(stats.packets_dropped, 0);
        assert!(stats.last_packet_time.is_none());
        assert_eq!(stats.average_rate_hz, 0.0);
        assert_eq!(stats.connection_errors, 0);
    }

    #[test]
    fn test_from_moza_input_state_maps_extended_ks_rotaries() {
        let mut state = MozaInputState::empty(42);
        state.rotary = [3, 9];
        state.ks_snapshot.encoders = [101, 202, 303, 404, 505, 606, 707, 808];

        let inputs = DeviceInputs::from_moza_input_state(&state);

        assert_eq!(inputs.tick, 42);
        assert_eq!(inputs.rotaries[0], 3);
        assert_eq!(inputs.rotaries[1], 9);
        assert_eq!(inputs.rotaries[2], 303);
        assert_eq!(inputs.rotaries[3], 404);
        assert_eq!(inputs.rotaries[4], 505);
        assert_eq!(inputs.rotaries[5], 606);
        assert_eq!(inputs.rotaries[6], 707);
        assert_eq!(inputs.rotaries[7], 808);
    }

    #[test]
    fn test_from_moza_input_state_clutch_fallback_prefers_combined_snapshot_then_raw() {
        let mut state = MozaInputState::empty(1);
        state.clutch_u16 = 1200;
        state.ks_snapshot.clutch_combined = Some(7000);

        let inputs = DeviceInputs::from_moza_input_state(&state);

        assert_eq!(inputs.clutch_left, Some(7000));
        assert_eq!(inputs.clutch_combined, Some(7000));
    }
}
