//! Game Integration Service
//!
//! Handles telemetry configuration, auto-switching, and game-specific integrations
//! according to requirements GI-01 and GI-03.

pub use crate::config_writers::{
    ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig, config_writer_factories,
};
use anyhow::Result;
use openracing_telemetry::{
    BddMatrixMetrics, CoveragePolicy, compare_runtime_registries_with_policies,
};
pub use openracing_telemetry_config::support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameSupportStatus, GameVersion,
    TelemetryFieldMapping, TelemetrySupport,
};
use openracing_telemetry_config::support::{load_default_matrix, normalize_game_id};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use tracing::warn;

/// Game integration service that manages telemetry configuration and auto-switching
pub struct GameService {
    support_matrix: Arc<RwLock<GameSupportMatrix>>,
    config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>>,
    writer_bdd_metrics: BddMatrixMetrics,
    active_game: Arc<RwLock<Option<String>>>,
}

/// Game status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStatusInfo {
    /// Currently active game ID, if any
    pub active_game: Option<String>,
    /// Whether telemetry is currently active and being received
    pub telemetry_active: bool,
    /// Currently active car ID from telemetry, if available
    pub car_id: Option<String>,
    /// Currently active track ID from telemetry, if available
    pub track_id: Option<String>,
}

impl GameService {
    /// Create new game service with YAML-loaded support matrix
    pub async fn new() -> Result<Self> {
        let support_matrix = Self::load_support_matrix().await?;
        let (config_writers, writer_bdd_metrics) = Self::build_config_writers(&support_matrix)?;

        Ok(Self {
            support_matrix: Arc::new(RwLock::new(support_matrix)),
            config_writers,
            writer_bdd_metrics,
            active_game: Arc::new(RwLock::new(None)),
        })
    }

    #[allow(clippy::type_complexity)]
    fn build_config_writers(
        support_matrix: &GameSupportMatrix,
    ) -> Result<(
        HashMap<String, Box<dyn ConfigWriter + Send + Sync>>,
        BddMatrixMetrics,
    )> {
        let mut config_writers = HashMap::new();
        let writer_factories: Vec<_> = config_writer_factories().to_vec();
        let coverage = compare_runtime_registries_with_policies(
            support_matrix.game_ids(),
            std::iter::empty::<&str>(),
            writer_factories.iter().map(|(writer_id, _)| *writer_id),
            CoveragePolicy::LENIENT,
            CoveragePolicy::MATRIX_COMPLETE,
        );
        let metrics = coverage.metrics();
        let writer_bdd_metrics = coverage
            .writer_coverage
            .bdd_metrics(CoveragePolicy::MATRIX_COMPLETE);

        info!(
            matrix_game_count = metrics.matrix_game_count,
            writer_matrix_coverage = metrics.writer.matrix_coverage_ratio,
            writer_registry_coverage = metrics.writer.registry_coverage_ratio,
            writer_missing_count = metrics.writer.missing_count,
            writer_extra_count = metrics.writer.extra_count,
            writer_parity_ok = writer_bdd_metrics.parity_ok,
            matrix_writer_parity_ok = coverage.writer_policy_ok(),
            "Config writer registry parity checked against support matrix"
        );

        if !coverage.writer_coverage.has_no_extra_coverage() {
            warn!(
                extra_writers = ?coverage.writer_coverage.extra_in_registry,
                "Config writer registry contains game IDs not present in support matrix"
            );
        }

        if !coverage.writer_policy_ok() {
            return Err(anyhow::anyhow!(
                "Missing config writers for matrix games: {:?}",
                coverage.writer_coverage.missing_in_registry
            ));
        }

        let writer_factory_lookup = writer_factories
            .iter()
            .map(|(writer_id, factory)| (writer_id.to_ascii_lowercase(), *factory))
            .collect::<HashMap<_, _>>();

        for (game_id, game_support) in &support_matrix.games {
            let config_writer_id = game_support.config_writer.to_ascii_lowercase();
            let factory = writer_factory_lookup
                .get(&*config_writer_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing config writer '{}' for game '{}'",
                        game_support.config_writer,
                        game_id
                    )
                })?;

            if config_writers.insert(game_id.clone(), factory()).is_some() {
                warn!(
                    game_id = %game_id,
                    config_writer = %game_support.config_writer,
                    "Overwriting existing config writer mapping; matrix entries should be unique"
                );
            }
        }

        Ok((config_writers, writer_bdd_metrics))
    }

    /// Load game support matrix from centralized telemetry-support metadata
    async fn load_support_matrix() -> Result<GameSupportMatrix> {
        let matrix = load_default_matrix().map_err(|err| {
            anyhow::anyhow!(
                "Failed to load default telemetry support matrix from shared metadata: {}",
                err
            )
        })?;
        info!(
            games_count = matrix.games.len(),
            "Loaded game support matrix"
        );
        Ok(matrix)
    }

    /// Configure telemetry for a specific game (GI-01)
    pub async fn configure_telemetry(
        &self,
        game_id: &str,
        game_path: &Path,
    ) -> Result<Vec<ConfigDiff>> {
        self.configure_telemetry_with_options(game_id, game_path, false)
            .await
    }

    /// Configure telemetry for a specific game with extra options.
    ///
    /// This is the primary path for callers that need feature-flagged options
    /// without changing default behavior.
    pub async fn configure_telemetry_with_options(
        &self,
        game_id: &str,
        game_path: &Path,
        enable_high_rate_iracing_360hz: bool,
    ) -> Result<Vec<ConfigDiff>> {
        let game_id = normalize_game_id(game_id);
        info!(game_id = %game_id, game_path = ?game_path, "Configuring telemetry");

        let support_matrix = self.support_matrix.read().await;
        let game_support = support_matrix
            .games
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;

        if enable_high_rate_iracing_360hz && !game_support.telemetry.supports_360hz_option {
            return Err(anyhow::anyhow!(
                "High-rate 360Hz telemetry option is not supported for game: {}",
                game_id
            ));
        }

        let config_writer = self
            .config_writers
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;

        // Create telemetry configuration
        let output_target = game_support
            .telemetry
            .output_target
            .clone()
            .unwrap_or_else(|| "127.0.0.1:12345".to_string());

        let telemetry_config = TelemetryConfig {
            enabled: true,
            update_rate_hz: if enable_high_rate_iracing_360hz {
                game_support
                    .telemetry
                    .high_rate_update_rate_hz
                    .unwrap_or(game_support.telemetry.update_rate_hz)
            } else {
                game_support.telemetry.update_rate_hz
            },
            output_method: game_support.telemetry.method.clone(),
            output_target,
            fields: game_support
                .versions
                .first()
                .map(|v| v.supported_fields.clone())
                .unwrap_or_default(),
            enable_high_rate_iracing_360hz,
        };

        // Write configuration and get diffs
        let diffs = config_writer.write_config(game_path, &telemetry_config)?;

        info!(game_id = %game_id, diffs_count = diffs.len(), "Telemetry configuration completed");
        Ok(diffs)
    }

    /// Get normalized telemetry field mapping for a game (GI-03)
    pub async fn get_telemetry_mapping(&self, game_id: &str) -> Result<TelemetryFieldMapping> {
        let game_id = normalize_game_id(game_id);
        let support_matrix = self.support_matrix.read().await;
        let game_support = support_matrix
            .games
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;

        Ok(game_support.telemetry.fields.clone())
    }

    /// Get list of supported games
    pub async fn get_supported_games(&self) -> Vec<String> {
        let support_matrix = self.support_matrix.read().await;
        support_matrix.game_ids()
    }

    /// Get stable game integrations from the matrix.
    pub async fn get_stable_games(&self) -> Vec<String> {
        let support_matrix = self.support_matrix.read().await;
        support_matrix.game_ids_by_status(GameSupportStatus::Stable)
    }

    /// Get experimental game integrations from the matrix.
    pub async fn get_experimental_games(&self) -> Vec<String> {
        let support_matrix = self.support_matrix.read().await;
        support_matrix.game_ids_by_status(GameSupportStatus::Experimental)
    }

    /// Get game support information
    pub async fn get_game_support(&self, game_id: &str) -> Result<GameSupport> {
        let game_id = normalize_game_id(game_id);
        let support_matrix = self.support_matrix.read().await;
        support_matrix
            .games
            .get(game_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))
    }

    /// Get currently active game
    pub async fn get_active_game(&self) -> Option<String> {
        self.active_game.read().await.clone()
    }

    /// Set active game for auto-switching
    pub async fn set_active_game(&self, game_id: Option<String>) -> Result<()> {
        let mut active_game = self.active_game.write().await;
        *active_game = game_id.clone();

        if let Some(game_id) = game_id {
            info!(game_id = %game_id, "Set active game");
        } else {
            info!("Cleared active game");
        }

        Ok(())
    }

    /// Validate configuration was applied correctly
    pub async fn validate_telemetry_config(&self, game_id: &str, game_path: &Path) -> Result<bool> {
        let game_id = normalize_game_id(game_id);
        let config_writer = self
            .config_writers
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;

        config_writer.validate_config(game_path)
    }

    /// Get expected configuration diffs for testing
    pub async fn get_expected_diffs(
        &self,
        game_id: &str,
        config: &TelemetryConfig,
    ) -> Result<Vec<ConfigDiff>> {
        let game_id = normalize_game_id(game_id);
        let config_writer = self
            .config_writers
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;

        config_writer.get_expected_diffs(config)
    }

    /// Return startup writer parity metrics aligned with telemetry BDD checks.
    pub fn writer_bdd_metrics(&self) -> &BddMatrixMetrics {
        &self.writer_bdd_metrics
    }

    /// Get game status (for IPC service compatibility)
    pub async fn get_game_status(&self) -> Result<GameStatusInfo> {
        let active_game = self.get_active_game().await;

        // For now, return basic status information
        // This could be enhanced to detect actual game state, telemetry activity, etc.
        Ok(GameStatusInfo {
            active_game,
            telemetry_active: false, // Would be determined by actual telemetry monitoring
            car_id: None,            // Would be populated from telemetry data
            track_id: None,          // Would be populated from telemetry data
        })
    }
}
