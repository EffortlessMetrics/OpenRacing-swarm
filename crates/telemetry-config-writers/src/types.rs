use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration to be applied to a game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// True if telemetry is to be enabled, false to disable.
    pub enabled: bool,
    /// Target update rate in Hz.
    pub update_rate_hz: u32,
    /// Output method (e.g. "shared_memory", "udp").
    pub output_method: String,
    /// Output target address or parameters.
    pub output_target: String,
    /// List of telemetry fields that should be recorded.
    pub fields: Vec<String>,
    /// iRacing-specific flag for enabling 360Hz high-rate telemetry.
    #[serde(default)]
    pub enable_high_rate_iracing_360hz: bool,
}

/// Represents a configuration change made to a game file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDiff {
    /// Path to the configuration file
    pub file_path: String,
    /// INI-style section name, if applicable
    pub section: Option<String>,
    /// Configuration key name
    pub key: String,
    /// Value prior to modification
    pub old_value: Option<String>,
    /// Value after modification
    pub new_value: String,
    /// Add, Modify, or Remove operation
    pub operation: DiffOperation,
}

/// Type of configuration operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiffOperation {
    /// Add a new key or file
    Add,
    /// Modify an existing key or file
    Modify,
    /// Remove an existing key
    Remove,
}

/// Configuration writer trait for game-specific config generation
pub trait ConfigWriter {
    /// Write telemetry configuration for the game
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>>;

    /// Validate that configuration was applied correctly
    fn validate_config(&self, game_path: &Path) -> Result<bool>;

    /// Get the expected configuration diffs for testing
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>>;
}
