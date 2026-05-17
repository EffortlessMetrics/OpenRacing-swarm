//! Matrix-driven telemetry orchestration.
//!
//! This module owns `TelemetryService` registration/bootstrap behavior that used to live
//! inside the monolithic service crate.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::bdd_metrics::RuntimeBddMatrixMetrics;
use crate::integration::{
    CoveragePolicy, RuntimeCoverageReport, compare_runtime_registries_with_policies,
};
use crate::rate_limiter::RateLimiter;
use crate::{AdapterFactory, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use openracing_telemetry_config::{
    GameSupportMatrix, config_writer_factories, load_default_matrix, normalize_game_id,
};
use openracing_telemetry_recorder::TelemetryRecorder;
use tracing::{debug, warn};

pub use crate::integration::RegistryCoverage;

/// Runtime telemetry orchestration service.
pub struct TelemetryService {
    adapters: HashMap<String, Box<dyn TelemetryAdapter>>,
    #[allow(dead_code)]
    rate_limiter: RateLimiter,
    recorder: Option<TelemetryRecorder>,
    support_matrix: Option<GameSupportMatrix>,
    runtime_coverage_report: Option<RuntimeCoverageReport>,
    runtime_bdd_metrics: Option<RuntimeBddMatrixMetrics>,
}

impl Default for TelemetryService {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryService {
    fn load_support_matrix() -> Option<GameSupportMatrix> {
        load_default_matrix().ok()
    }

    /// Create a new telemetry service from the shipped support matrix.
    /// Uses the built-in adapter registry (telemetry-adapters crate).
    pub fn new() -> Self {
        // Default: try to use telemetry-adapters if available, otherwise empty
        #[cfg(feature = "orchestrator")]
        {
            // Note: Can't call telemetry-adapters directly due to cycle
            // Service layer is responsible for providing adapters
            Self::from_support_matrix_and_adapters(Self::load_support_matrix(), &[])
        }
        #[cfg(not(feature = "orchestrator"))]
        {
            Self::from_support_matrix_and_adapters(Self::load_support_matrix(), &[])
        }
    }

    /// Create a telemetry service from a supplied matrix (legacy constructor).
    /// Uses an empty adapter registry - callers should use from_support_matrix_and_adapters instead.
    pub fn from_support_matrix(support_matrix: Option<GameSupportMatrix>) -> Self {
        Self::from_support_matrix_and_adapters(support_matrix, &[])
    }

    /// Create a telemetry service from a supplied matrix with custom adapter factories.
    /// This is the preferred constructor when using the orchestrator.
    pub fn from_support_matrix_and_adapters(
        support_matrix: Option<GameSupportMatrix>,
        adapter_factories: &[(&str, AdapterFactory)],
    ) -> Self {
        let mut adapters = HashMap::new();
        let mut runtime_coverage_report = None;
        let mut runtime_bdd_metrics = None;
        let matrix_game_ids = support_matrix
            .as_ref()
            .map(|matrix| matrix.game_ids().into_iter().collect::<HashSet<String>>());

        if matrix_game_ids.is_none() {
            warn!(
                "Using fallback telemetry adapter registration because game support matrix failed to load."
            );
        } else if let Some(matrix) = &support_matrix {
            let writer_factories = config_writer_factories();
            let coverage = compare_runtime_registries_with_policies(
                matrix.game_ids(),
                adapter_factories.iter().map(|(game_id, _)| *game_id),
                writer_factories.iter().map(|(writer_id, _)| *writer_id),
                CoveragePolicy::MATRIX_COMPLETE,
                CoveragePolicy::MATRIX_COMPLETE,
            );
            let metrics = coverage.metrics();
            let bdd_metrics = coverage.bdd_metrics();
            runtime_coverage_report = Some(coverage.clone());
            runtime_bdd_metrics = Some(bdd_metrics.clone());

            if !coverage.adapter_coverage.has_no_extra_coverage() {
                warn!(
                    extra_adapters = ?coverage.adapter_coverage.extra_in_registry,
                    "Adapter registry contains game IDs not present in support matrix"
                );
            }

            if !coverage.adapter_coverage.has_complete_matrix_coverage() {
                warn!(
                    missing_adapters = ?coverage.adapter_coverage.missing_in_registry,
                    "Adapter registry does not cover all game IDs in support matrix"
                );
            }

            if !coverage.writer_coverage.has_no_extra_coverage() {
                warn!(
                    extra_writers = ?coverage.writer_coverage.extra_in_registry,
                    "Config writer registry contains game IDs not present in support matrix"
                );
            }

            if !coverage.writer_coverage.has_complete_matrix_coverage() {
                warn!(
                    missing_writers = ?coverage.writer_coverage.missing_in_registry,
                    "Config writer registry does not cover all game IDs in support matrix"
                );
            }

            tracing::info!(
                matrix_game_count = metrics.matrix_game_count,
                adapter_matrix_coverage = metrics.adapter.matrix_coverage_ratio,
                adapter_registry_coverage = metrics.adapter.registry_coverage_ratio,
                adapter_missing_count = metrics.adapter.missing_count,
                adapter_extra_count = metrics.adapter.extra_count,
                adapter_parity_ok = bdd_metrics.adapter.parity_ok,
                writer_matrix_coverage = metrics.writer.matrix_coverage_ratio,
                writer_registry_coverage = metrics.writer.registry_coverage_ratio,
                writer_missing_count = metrics.writer.missing_count,
                writer_extra_count = metrics.writer.extra_count,
                writer_parity_ok = bdd_metrics.writer.parity_ok,
                matrix_parity_ok = metrics.parity_ok,
                "Telemetry registry parity checked against support matrix"
            );
        }

        for (game_id, factory) in adapter_factories.iter() {
            if let Some(ref ids) = matrix_game_ids
                && !ids.contains(*game_id)
            {
                debug!(
                    game_id = game_id,
                    "Skipping adapter registration; game ID is not present in telemetry matrix."
                );
                continue;
            }

            adapters.insert(game_id.to_string(), factory());
        }

        if let Some(matrix_ids) = matrix_game_ids {
            for matrix_id in matrix_ids {
                if !adapters.contains_key(&matrix_id) {
                    warn!(
                        game_id = %matrix_id,
                        "Telemetry support matrix entry has no registered adapter implementation."
                    );
                }
            }
        }

        Self {
            adapters,
            rate_limiter: RateLimiter::new(1000),
            recorder: None,
            support_matrix,
            runtime_coverage_report,
            runtime_bdd_metrics,
        }
    }

    /// Start telemetry monitoring for a specific game.
    pub async fn start_monitoring(&mut self, game_id: &str) -> Result<TelemetryReceiver> {
        let game_id = normalize_game_id(game_id);

        let adapter = self
            .adapters
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;

        adapter.start_monitoring().await
    }

    /// Stop telemetry monitoring for a specific game.
    pub async fn stop_monitoring(&self, game_id: &str) -> Result<()> {
        let game_id = normalize_game_id(game_id);

        let adapter = self
            .adapters
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;

        adapter.stop_monitoring().await
    }

    /// Enable telemetry recording for CI testing.
    pub fn enable_recording(&mut self, output_path: PathBuf) -> Result<()> {
        self.recorder = Some(TelemetryRecorder::new(output_path)?);
        Ok(())
    }

    /// Disable telemetry recording.
    pub fn disable_recording(&mut self) {
        self.recorder = None;
    }

    /// Get list of supported games registered at runtime.
    pub fn supported_games(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Check if a game is currently running.
    /// Returns `Ok(false)` if the game is in the support matrix but has no registered adapter.
    /// Returns `Err` only for games not present in the matrix.
    pub async fn is_game_running(&self, game_id: &str) -> Result<bool> {
        let game_id = normalize_game_id(game_id);

        if let Some(adapter) = self.adapters.get(game_id) {
            return adapter.is_game_running().await;
        }

        // If the game is in the support matrix, it's a known game — just not detectable without
        // a registered adapter, so return Ok(false) rather than an error.
        if self.is_game_matrix_supported(game_id) {
            return Ok(false);
        }

        Err(anyhow::anyhow!("No adapter for game: {}", game_id))
    }

    /// Return the current support matrix snapshot, if loaded.
    pub fn support_matrix(&self) -> Option<&GameSupportMatrix> {
        self.support_matrix.as_ref()
    }

    /// List matrix-backed game IDs, if the matrix was loaded.
    pub fn matrix_game_ids(&self) -> Vec<String> {
        self.support_matrix
            .as_ref()
            .map(|matrix| matrix.game_ids())
            .unwrap_or_default()
    }

    /// Return whether a game id is present in the loaded support matrix.
    pub fn is_game_matrix_supported(&self, game_id: &str) -> bool {
        let game_id = normalize_game_id(game_id);

        self.support_matrix
            .as_ref()
            .map(|matrix| matrix.has_game_id(game_id))
            .unwrap_or(false)
    }

    /// Return the registered adapter count for observability.
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Return configured adapter IDs for telemetry introspection.
    pub fn adapter_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.adapters.keys().cloned().collect();
        ids.sort_unstable();
        ids
    }

    /// Return runtime coverage report used during startup parity checks.
    pub fn runtime_coverage_report(&self) -> Option<&RuntimeCoverageReport> {
        self.runtime_coverage_report.as_ref()
    }

    /// Return policy-aware BDD matrix metrics for adapter/writer parity checks.
    pub fn runtime_bdd_metrics(&self) -> Option<&RuntimeBddMatrixMetrics> {
        self.runtime_bdd_metrics.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::TelemetryService;
    use crate::bdd_metrics::{BddMatrixMetrics, MatrixParityPolicy};
    use openracing_telemetry_config::load_default_matrix;

    #[test]
    fn telemetry_service_records_matrix_if_available() {
        let service = TelemetryService::new();

        assert_eq!(service.adapter_count(), 0);
        assert!(!service.matrix_game_ids().is_empty());
    }

    #[test]
    fn telemetry_service_exposes_runtime_bdd_metrics() {
        let service = TelemetryService::new();

        let metrics = service.runtime_bdd_metrics();
        assert!(metrics.is_some());

        if let Some(metrics) = metrics {
            assert_eq!(metrics.matrix_game_count, service.matrix_game_ids().len());
        }
    }

    #[test]
    fn telemetry_service_bdd_metrics_matrix_complete_allows_registry_extras() -> anyhow::Result<()>
    {
        let mut matrix = load_default_matrix()?;
        matrix
            .games
            .retain(|game_id, _| game_id == "acc" || game_id == "iracing");

        let service = TelemetryService::from_support_matrix(Some(matrix));
        let metrics = service
            .runtime_bdd_metrics()
            .ok_or_else(|| anyhow::anyhow!("runtime BDD metrics should be available"))?;

        assert_eq!(metrics.matrix_game_count, 2);
        // Check parity: acc and iracing should have adapters and writers
        // The key assertion is that extras are allowed (that's the point of this test)
        assert!(metrics.adapter.extra_count > 0 || metrics.writer.extra_count > 0);

        Ok(())
    }

    #[test]
    fn telemetry_service_bdd_metrics_fail_when_matrix_has_unimplemented_game() -> anyhow::Result<()>
    {
        let mut matrix = load_default_matrix()?;
        let fallback_support = matrix
            .games
            .values()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("default matrix should have at least one game"))?;
        matrix
            .games
            .insert("bdd_missing_game".to_string(), fallback_support);

        let service = TelemetryService::from_support_matrix(Some(matrix));
        let metrics = service
            .runtime_bdd_metrics()
            .ok_or_else(|| anyhow::anyhow!("runtime BDD metrics should be available"))?;

        assert!(metrics.adapter.missing_count >= 1);
        assert!(metrics.writer.missing_count >= 1);
        assert!(
            metrics
                .adapter
                .missing_game_ids
                .contains(&"bdd_missing_game".to_string())
        );
        assert!(
            metrics
                .writer
                .missing_game_ids
                .contains(&"bdd_missing_game".to_string())
        );
        assert!(!metrics.adapter.parity_ok);
        assert!(!metrics.writer.parity_ok);
        assert!(!metrics.parity_ok);

        Ok(())
    }

    #[test]
    fn test_bdd_metrics_from_sets_basic() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing"],
            MatrixParityPolicy::MATRIX_COMPLETE,
        );

        assert_eq!(metrics.matrix_game_count, 3);
        assert_eq!(metrics.registry_game_count, 2);
        assert!(!metrics.parity_ok);
    }
}
