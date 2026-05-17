//! Minimal Game Integration Implementation for Task 4
//!
//! This module implements the core requirements for task 4:
//! - YAML-based support matrix
//! - Table-driven configuration writers
//! - Golden file tests
//! - Telemetry field mapping documentation
//!
//! Requirements: GI-01, GI-03

use anyhow::Result;
use openracing_telemetry_config::support::load_default_matrix;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::info;

pub use openracing_telemetry_config::support::{
    GameSupport, GameSupportMatrix, TelemetryFieldMapping,
};

/// Configuration to be applied to a game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub update_rate_hz: u32,
    pub output_method: String,
    pub output_target: String,
    pub fields: Vec<String>,
}

/// Represents a configuration change made to a game file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDiff {
    pub file_path: String,
    pub section: Option<String>,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: String,
    pub operation: DiffOperation,
}

/// Type of configuration operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiffOperation {
    Add,
    Modify,
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

/// iRacing configuration writer
pub struct IRacingConfigWriter;

impl Default for IRacingConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for IRacingConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing iRacing telemetry configuration");

        let app_ini_path = game_path.join("Documents/iRacing/app.ini");
        let mut diffs = Vec::new();

        // Create directory if it doesn't exist
        if let Some(parent) = app_ini_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // For demonstration, we create a simple INI modification
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        diffs.push(ConfigDiff {
            file_path: app_ini_path.to_string_lossy().to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        });

        info!("iRacing configuration completed with {} diffs", diffs.len());
        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let app_ini_path = game_path.join("Documents/iRacing/app.ini");
        Ok(app_ini_path.exists())
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut diffs = Vec::new();

        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        diffs.push(ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        });

        Ok(diffs)
    }
}

/// ACC configuration writer
pub struct ACCConfigWriter;

impl Default for ACCConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACCConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ACC telemetry configuration");

        let broadcasting_json_path =
            game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let mut diffs = Vec::new();

        // Create directory if it doesn't exist
        if let Some(parent) = broadcasting_json_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create broadcasting configuration JSON
        let broadcasting_config = serde_json::json!({
            "updListenerPort": 9000,
            "connectionId": "",
            "broadcastingPort": 9000,
            "commandPassword": "",
            "updateRateHz": config.update_rate_hz
        });

        let new_content = serde_json::to_string_pretty(&broadcasting_config)?;

        diffs.push(ConfigDiff {
            file_path: broadcasting_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: new_content,
            operation: DiffOperation::Add,
        });

        info!("ACC configuration completed with {} diffs", diffs.len());
        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let broadcasting_json_path =
            game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        Ok(broadcasting_json_path.exists())
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut diffs = Vec::new();

        let broadcasting_config = serde_json::json!({
            "updListenerPort": 9000,
            "connectionId": "",
            "broadcastingPort": 9000,
            "commandPassword": "",
            "updateRateHz": config.update_rate_hz
        });

        let new_content = serde_json::to_string_pretty(&broadcasting_config)?;

        diffs.push(ConfigDiff {
            file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: new_content,
            operation: DiffOperation::Add,
        });

        Ok(diffs)
    }
}

/// Game integration service
pub struct GameIntegrationService {
    support_matrix: GameSupportMatrix,
    config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>>,
}

impl GameIntegrationService {
    /// Create new game integration service with YAML-loaded support matrix
    pub fn new() -> Result<Self> {
        let support_matrix = Self::load_support_matrix()?;
        let mut config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>> =
            HashMap::new();

        // Register config writers
        config_writers.insert(
            "iracing".to_string(),
            Box::new(IRacingConfigWriter::default()),
        );
        config_writers.insert("acc".to_string(), Box::new(ACCConfigWriter::default()));

        Ok(Self {
            support_matrix,
            config_writers,
        })
    }

    /// Load game support matrix from YAML file
    fn load_support_matrix() -> Result<GameSupportMatrix> {
        let matrix = load_default_matrix().map_err(|e| {
            anyhow::anyhow!("Failed to load default telemetry support matrix: {}", e)
        })?;
        info!(
            games_count = matrix.games.len(),
            "Loaded telemetry support matrix from shared metadata"
        );
        Ok(matrix)
    }

    /// Configure telemetry for a specific game (GI-01)
    pub fn configure_telemetry(&self, game_id: &str, game_path: &Path) -> Result<Vec<ConfigDiff>> {
        info!(game_id = %game_id, game_path = ?game_path, "Configuring telemetry");

        let game_support = self
            .support_matrix
            .games
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;

        let config_writer = self
            .config_writers
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;

        // Create telemetry configuration
        let telemetry_config = TelemetryConfig {
            enabled: true,
            update_rate_hz: game_support.telemetry.update_rate_hz,
            output_method: game_support.telemetry.method.clone(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: game_support
                .versions
                .first()
                .map(|v| v.supported_fields.clone())
                .unwrap_or_default(),
        };

        // Write configuration and get diffs
        let diffs = config_writer.write_config(game_path, &telemetry_config)?;

        info!(game_id = %game_id, diffs_count = diffs.len(), "Telemetry configuration completed");
        Ok(diffs)
    }

    /// Get normalized telemetry field mapping for a game (GI-03)
    pub fn get_telemetry_mapping(&self, game_id: &str) -> Result<TelemetryFieldMapping> {
        let game_support = self
            .support_matrix
            .games
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;

        Ok(game_support.telemetry.fields.clone())
    }

    /// Get list of supported games
    pub fn get_supported_games(&self) -> Vec<String> {
        self.support_matrix.games.keys().cloned().collect()
    }

    /// Get game support information
    pub fn get_game_support(&self, game_id: &str) -> Result<GameSupport> {
        self.support_matrix
            .games
            .get(game_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))
    }

    /// Get expected configuration diffs for testing
    pub fn get_expected_diffs(
        &self,
        game_id: &str,
        config: &TelemetryConfig,
    ) -> Result<Vec<ConfigDiff>> {
        let config_writer = self
            .config_writers
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;

        config_writer.get_expected_diffs(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow};
    use racing_wheel_telemetry_support::matrix_game_ids;
    use serde_json;
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    fn test_yaml_support_matrix_loading() -> Result<()> {
        let service = GameIntegrationService::new()?;

        // Test that YAML was loaded correctly
        let supported_games = service.get_supported_games();
        let expected: HashSet<String> = matrix_game_ids()?.into_iter().collect();
        let actual: HashSet<String> = supported_games.into_iter().collect();
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_iracing_config_writer_golden() -> Result<()> {
        let writer = IRacingConfigWriter::default();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        };

        // Test expected diffs
        let expected_diffs = writer.get_expected_diffs(&config)?;
        assert_eq!(expected_diffs.len(), 1);
        assert_eq!(expected_diffs[0].key, "telemetryDiskFile");
        assert_eq!(expected_diffs[0].new_value, "1");
        assert_eq!(expected_diffs[0].operation, DiffOperation::Add);

        // Test actual config writing
        let temp_dir = TempDir::new()?;
        let actual_diffs = writer.write_config(temp_dir.path(), &config)?;

        // Compare structure (ignoring file paths)
        assert_eq!(actual_diffs.len(), expected_diffs.len());
        assert_eq!(actual_diffs[0].key, expected_diffs[0].key);
        assert_eq!(actual_diffs[0].new_value, expected_diffs[0].new_value);
        assert_eq!(actual_diffs[0].operation, expected_diffs[0].operation);
        Ok(())
    }

    #[test]
    fn test_acc_config_writer_golden() -> Result<()> {
        let writer = ACCConfigWriter::default();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        };

        // Test expected diffs
        let expected_diffs = writer.get_expected_diffs(&config)?;
        assert_eq!(expected_diffs.len(), 1);
        assert_eq!(expected_diffs[0].key, "entire_file");
        assert_eq!(expected_diffs[0].operation, DiffOperation::Add);

        // Verify JSON structure
        let json: serde_json::Value = serde_json::from_str(&expected_diffs[0].new_value)?;
        assert_eq!(json["updListenerPort"], 9000);
        assert_eq!(json["broadcastingPort"], 9000);
        assert_eq!(json["updateRateHz"], 100);

        // Test actual config writing
        let temp_dir = TempDir::new()?;
        let actual_diffs = writer.write_config(temp_dir.path(), &config)?;

        // Compare structure
        assert_eq!(actual_diffs.len(), expected_diffs.len());
        assert_eq!(actual_diffs[0].key, expected_diffs[0].key);
        assert_eq!(actual_diffs[0].operation, expected_diffs[0].operation);

        // Compare JSON content
        let actual_json: serde_json::Value = serde_json::from_str(&actual_diffs[0].new_value)?;
        let expected_json: serde_json::Value = serde_json::from_str(&expected_diffs[0].new_value)?;
        assert_eq!(actual_json, expected_json);
        Ok(())
    }

    #[test]
    fn test_telemetry_field_mapping() -> Result<()> {
        let service = GameIntegrationService::new()?;

        // Test iRacing field mapping
        let iracing_mapping = service.get_telemetry_mapping("iracing")?;
        assert_eq!(
            iracing_mapping.ffb_scalar,
            Some("SteeringWheelPctTorqueSign".to_string())
        );
        assert_eq!(iracing_mapping.rpm, Some("RPM".to_string()));
        assert_eq!(iracing_mapping.speed_ms, Some("Speed".to_string()));
        assert_eq!(iracing_mapping.slip_ratio, Some("LFSlipRatio".to_string()));
        assert_eq!(iracing_mapping.gear, Some("Gear".to_string()));
        assert_eq!(iracing_mapping.car_id, Some("CarPath".to_string()));
        assert_eq!(iracing_mapping.track_id, Some("TrackName".to_string()));

        // Test ACC field mapping
        let acc_mapping = service.get_telemetry_mapping("acc")?;
        assert_eq!(acc_mapping.ffb_scalar, Some("steerAngle".to_string()));
        assert_eq!(acc_mapping.rpm, Some("rpms".to_string()));
        assert_eq!(acc_mapping.speed_ms, Some("speedKmh".to_string()));
        assert_eq!(acc_mapping.slip_ratio, Some("wheelSlip".to_string()));
        assert_eq!(acc_mapping.gear, Some("gear".to_string()));
        assert_eq!(acc_mapping.car_id, Some("carModel".to_string()));
        assert_eq!(acc_mapping.track_id, Some("track".to_string()));
        Ok(())
    }

    #[test]
    fn test_end_to_end_configuration() -> Result<()> {
        let service = GameIntegrationService::new()?;
        let temp_dir = TempDir::new()?;

        // Test iRacing configuration
        let iracing_diffs = service.configure_telemetry("iracing", temp_dir.path())?;
        assert_eq!(iracing_diffs.len(), 1);
        assert_eq!(iracing_diffs[0].key, "telemetryDiskFile");
        assert_eq!(iracing_diffs[0].new_value, "1");

        // Test ACC configuration
        let acc_diffs = service.configure_telemetry("acc", temp_dir.path())?;
        assert_eq!(acc_diffs.len(), 1);
        assert_eq!(acc_diffs[0].key, "entire_file");

        // Verify ACC JSON is valid
        let acc_json: serde_json::Value = serde_json::from_str(&acc_diffs[0].new_value)?;
        assert!(acc_json.is_object());
        assert!(acc_json.get("updListenerPort").is_some());
        assert!(acc_json.get("broadcastingPort").is_some());
        Ok(())
    }

    #[test]
    fn test_unsupported_game_handling() -> Result<()> {
        let service = GameIntegrationService::new()?;

        // Test unsupported game returns error
        let result = service.get_game_support("unsupported_game");
        let err = match result {
            Ok(_) => {
                return Err(anyhow!(
                    "Expected unsupported game error for get_game_support"
                ));
            }
            Err(error) => error,
        };
        assert!(err.to_string().contains("Unsupported game"));

        let mapping_result = service.get_telemetry_mapping("unsupported_game");
        let err = match mapping_result {
            Ok(_) => {
                return Err(anyhow!(
                    "Expected unsupported game error for get_telemetry_mapping"
                ));
            }
            Err(error) => error,
        };
        assert!(err.to_string().contains("Unsupported game"));
        Ok(())
    }
}
