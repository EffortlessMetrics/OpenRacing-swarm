//! Game Integration Module - Minimal Implementation
//!
//! Implements task 4: Game Support Matrix & Golden Writers
//! Requirements: GI-01, GI-03

use crate::game_support_matrix::create_default_matrix;
use anyhow::Result;
pub use openracing_telemetry_config::support::{
    GameSupport, GameSupportMatrix, TelemetryFieldMapping,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

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

/// Game integration service
pub struct GameIntegrationService {
    support_matrix: GameSupportMatrix,
    config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>>,
}

impl GameIntegrationService {
    /// Create new game integration service
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for GameIntegrationService {
    fn default() -> Self {
        let support_matrix = create_default_matrix();
        let config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>> = HashMap::new();

        Self {
            support_matrix,
            config_writers,
        }
    }
}

impl GameIntegrationService {
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
}
