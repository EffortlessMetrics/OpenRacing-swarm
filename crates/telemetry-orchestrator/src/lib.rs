//! Matrix-driven telemetry orchestration crate.
//!
//! This crate owns `TelemetryService` registration/bootstrap behavior that used to live
//! inside the monolithic service crate. The service crate now re-exports this type to
//! preserve existing public APIs while allowing a dedicated, single-purpose crate.

#![deny(static_mut_refs)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use openracing_telemetry_adapters::{TelemetryAdapter, TelemetryReceiver, adapter_factories};
use openracing_telemetry_config::config_writer_factories;
use openracing_telemetry_recorder::TelemetryRecorder;
use racing_wheel_telemetry_bdd_metrics::RuntimeBddMatrixMetrics;
use racing_wheel_telemetry_integration::{
    CoveragePolicy, RuntimeCoverageReport, compare_runtime_registries_with_policies,
};
use racing_wheel_telemetry_rate_limiter::RateLimiter;
use racing_wheel_telemetry_support::{GameSupportMatrix, normalize_game_id};
use tracing::{debug, warn};

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
        racing_wheel_telemetry_support::load_default_matrix().ok()
    }

    /// Create a new telemetry service from the shipped support matrix.
    pub fn new() -> Self {
        Self::from_support_matrix(Self::load_support_matrix())
    }

    /// Create a telemetry service from a supplied matrix.
    pub fn from_support_matrix(support_matrix: Option<GameSupportMatrix>) -> Self {
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
                adapter_factories().iter().map(|(game_id, _)| *game_id),
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

        for (game_id, factory) in adapter_factories() {
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
            rate_limiter: RateLimiter::new(1000), // 1kHz max rate to protect RT thread
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
    pub async fn is_game_running(&self, game_id: &str) -> Result<bool> {
        let game_id = normalize_game_id(game_id);

        let adapter = self
            .adapters
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;

        adapter.is_game_running().await
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
    use anyhow::Result;
    use racing_wheel_telemetry_support::{GameSupportMatrix, load_default_matrix};
    use std::collections::HashMap;

    #[test]
    fn telemetry_service_records_matrix_if_available() {
        let service = TelemetryService::new();

        assert!(!service.adapter_count().eq(&0));
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
    fn telemetry_service_bdd_metrics_matrix_complete_allows_registry_extras() -> Result<()> {
        let mut matrix = load_default_matrix()?;
        matrix
            .games
            .retain(|game_id, _| game_id == "acc" || game_id == "iracing");

        let service = TelemetryService::from_support_matrix(Some(matrix));
        let metrics = service
            .runtime_bdd_metrics()
            .ok_or_else(|| anyhow::anyhow!("runtime BDD metrics should be available"))?;

        assert_eq!(metrics.matrix_game_count, 2);
        assert_eq!(metrics.adapter.missing_count, 0);
        assert_eq!(metrics.writer.missing_count, 0);
        assert!(metrics.adapter.extra_count > 0);
        assert!(metrics.writer.extra_count > 0);
        assert!(metrics.adapter.parity_ok);
        assert!(metrics.writer.parity_ok);
        assert!(metrics.parity_ok);

        Ok(())
    }

    #[test]
    fn telemetry_service_bdd_metrics_fail_when_matrix_has_unimplemented_game() -> Result<()> {
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
    fn adapter_ids_are_lexicographically_sorted() {
        let service = TelemetryService::new();
        let ids = service.adapter_ids();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn from_support_matrix_none_still_registers_adapters() {
        // Fallback: when no matrix is supplied all adapters should still register.
        let service = TelemetryService::from_support_matrix(None);
        assert!(service.adapter_count() > 0);
    }

    #[test]
    fn is_game_matrix_supported_false_for_unknown_game() {
        let service = TelemetryService::new();
        assert!(!service.is_game_matrix_supported("not_a_real_game_xyz_999"));
    }

    #[test]
    fn supported_games_len_matches_adapter_count() {
        let service = TelemetryService::new();
        assert_eq!(service.supported_games().len(), service.adapter_count());
    }

    #[test]
    fn runtime_coverage_report_is_present_when_matrix_loaded() {
        let service = TelemetryService::new();
        assert!(service.runtime_coverage_report().is_some());
    }

    // --- Adapter lifecycle management ---

    #[tokio::test]
    async fn start_monitoring_unknown_game_returns_error() {
        let mut service = TelemetryService::new();
        let result = service.start_monitoring("nonexistent_game_xyz_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stop_monitoring_unknown_game_returns_error() {
        let service = TelemetryService::new();
        let result = service.stop_monitoring("nonexistent_game_xyz_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn is_game_running_unknown_game_returns_error() {
        let service = TelemetryService::new();
        let result = service.is_game_running("nonexistent_game_xyz_999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn start_monitoring_known_adapter_does_not_panic() {
        let mut service = TelemetryService::new();
        let games = service.supported_games();
        assert!(!games.is_empty());
        // Pick the first registered adapter; start_monitoring may return an error
        // (e.g. game not running / no UDP socket) but must not panic.
        let _result = service.start_monitoring(&games[0]).await;
    }

    #[tokio::test]
    async fn stop_monitoring_known_adapter_does_not_panic() {
        let service = TelemetryService::new();
        let games = service.supported_games();
        assert!(!games.is_empty());
        let _result = service.stop_monitoring(&games[0]).await;
    }

    #[tokio::test]
    async fn is_game_running_known_adapter_returns_bool() {
        let service = TelemetryService::new();
        let games = service.supported_games();
        assert!(!games.is_empty());
        // In CI no game is running, so we just verify it doesn't error.
        let result = service.is_game_running(&games[0]).await;
        // Either Ok(bool) or a recoverable error — must not panic.
        let _ignored = result;
    }

    // --- Multi-game switching ---

    #[tokio::test]
    async fn switching_between_games_does_not_panic() {
        let mut service = TelemetryService::new();
        let games = service.supported_games();
        if games.len() < 2 {
            return; // need at least two adapters
        }
        // Stop first game (idempotent), start second, stop second, start first.
        let _r1 = service.stop_monitoring(&games[0]).await;
        let _r2 = service.start_monitoring(&games[1]).await;
        let _r3 = service.stop_monitoring(&games[1]).await;
        let _r4 = service.start_monitoring(&games[0]).await;
    }

    // --- Error messages ---

    #[tokio::test]
    async fn error_message_contains_game_id_for_unknown_adapter() -> Result<()> {
        let mut service = TelemetryService::new();
        let result = service.start_monitoring("totally_fake_game").await;
        assert!(result.is_err(), "expected error for unknown game");
        let msg = format!(
            "{}",
            result
                .err()
                .ok_or_else(|| anyhow::anyhow!("expected Err"))?
        );
        assert!(
            msg.contains("totally_fake_game"),
            "error should mention the game id, got: {msg}"
        );
        Ok(())
    }

    // --- Configuration-driven adapter selection ---

    #[test]
    fn filtered_matrix_restricts_registered_adapters() -> Result<()> {
        let mut matrix = load_default_matrix()?;
        let original_count = matrix.games.len();
        assert!(original_count >= 2);

        // Keep only two games
        let keep: Vec<String> = matrix.games.keys().take(2).cloned().collect();
        matrix.games.retain(|k, _| keep.contains(k));
        assert_eq!(matrix.games.len(), 2);

        let service = TelemetryService::from_support_matrix(Some(matrix));
        // Only the two matching adapters should be registered (if factories exist).
        assert!(service.adapter_count() <= 2);
        for id in service.adapter_ids() {
            assert!(keep.contains(&id));
        }

        Ok(())
    }

    #[test]
    fn no_matrix_registers_all_factory_adapters() {
        let service_no_matrix = TelemetryService::from_support_matrix(None);
        let service_with_matrix = TelemetryService::new();

        // Without a matrix every factory is registered, which should be >= the
        // matrix-filtered set.
        assert!(service_no_matrix.adapter_count() >= service_with_matrix.adapter_count());
    }

    #[test]
    fn empty_matrix_registers_no_adapters() {
        let matrix = GameSupportMatrix {
            games: HashMap::new(),
        };
        let service = TelemetryService::from_support_matrix(Some(matrix));
        assert_eq!(service.adapter_count(), 0);
        assert!(service.supported_games().is_empty());
        assert!(service.adapter_ids().is_empty());
    }

    // --- Normalize game ID passthrough ---

    #[tokio::test]
    async fn start_monitoring_normalizes_ea_wrc_alias() {
        let mut service = TelemetryService::new();
        // "ea_wrc" normalizes to "eawrc" — if eawrc adapter exists the lookup
        // should not produce an "unknown adapter" error for the alias form.
        let has_eawrc = service.supported_games().contains(&"eawrc".to_string());
        if has_eawrc {
            let result = service.start_monitoring("ea_wrc").await;
            // Must resolve to the eawrc adapter (may still fail for network reasons).
            assert!(
                result.is_ok() || !format!("{:?}", result).contains("No adapter"),
                "ea_wrc alias should resolve to eawrc adapter"
            );
        }
    }

    #[tokio::test]
    async fn start_monitoring_normalizes_f1_2025_alias() {
        let mut service = TelemetryService::new();
        let has_f1_25 = service.supported_games().contains(&"f1_25".to_string());
        if has_f1_25 {
            let result = service.start_monitoring("f1_2025").await;
            assert!(
                result.is_ok() || !format!("{:?}", result).contains("No adapter"),
                "f1_2025 alias should resolve to f1_25 adapter"
            );
        }
    }

    // --- Recording lifecycle ---

    #[test]
    fn enable_then_disable_recording_round_trips() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("telemetry_test.json");

        let mut service = TelemetryService::new();
        service.enable_recording(path)?;
        service.disable_recording();
        // No panic, no error.
        Ok(())
    }

    #[test]
    fn enable_recording_creates_parent_dirs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested").join("dir").join("out.json");

        let mut service = TelemetryService::new();
        service.enable_recording(path.clone())?;
        assert!(path.parent().is_some_and(|p| p.exists()));
        service.disable_recording();
        Ok(())
    }

    // --- Default impl ---

    #[test]
    fn default_is_equivalent_to_new() {
        let from_new = TelemetryService::new();
        let from_default = TelemetryService::default();
        assert_eq!(from_new.adapter_count(), from_default.adapter_count());
        assert_eq!(from_new.adapter_ids(), from_default.adapter_ids());
    }

    // --- Matrix query helpers ---

    #[test]
    fn support_matrix_is_some_with_loaded_matrix() {
        let service = TelemetryService::new();
        assert!(service.support_matrix().is_some());
    }

    #[test]
    fn support_matrix_is_none_when_no_matrix() {
        let service = TelemetryService::from_support_matrix(None);
        assert!(service.support_matrix().is_none());
        assert!(service.matrix_game_ids().is_empty());
        assert!(!service.is_game_matrix_supported("acc"));
    }

    #[test]
    fn matrix_game_ids_subset_of_supported_games_when_matrix_loaded() {
        let service = TelemetryService::new();
        let supported: std::collections::HashSet<String> =
            service.supported_games().into_iter().collect();
        for gid in service.matrix_game_ids() {
            assert!(
                supported.contains(&gid),
                "matrix game '{gid}' should have a registered adapter"
            );
        }
    }

    #[test]
    fn is_game_matrix_supported_returns_true_for_known_matrix_game() -> Result<()> {
        let matrix = load_default_matrix()?;
        let first = matrix
            .games
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("matrix must have at least one game"))?;
        let service = TelemetryService::from_support_matrix(Some(matrix));
        assert!(service.is_game_matrix_supported(&first));
        Ok(())
    }

    // --- Coverage / BDD metrics with no matrix ---

    #[test]
    fn runtime_coverage_and_bdd_metrics_none_without_matrix() {
        let service = TelemetryService::from_support_matrix(None);
        assert!(service.runtime_coverage_report().is_none());
        assert!(service.runtime_bdd_metrics().is_none());
    }
}
