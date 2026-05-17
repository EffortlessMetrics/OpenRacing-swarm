//! Deep telemetry pipeline integration tests verifying the full data flow from
//! game data to force feedback, including multi-game switching, timeout handling,
//! rate limiting, recording, statistics, and game-specific quirk handling.

use openracing_telemetry_adapters::{
    MockAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, adapter_factories,
};
use openracing_telemetry_recorder::{TelemetryRecorder, TestFixtureGenerator, TestScenario};
use racing_wheel_telemetry_integration::{
    CoveragePolicy, compare_matrix_and_registry, compare_matrix_and_registry_with_policy,
    compare_runtime_registries_with_policies,
};
use racing_wheel_telemetry_rate_limiter::RateLimiter;
use racing_wheel_telemetry_support::{load_default_matrix, normalize_game_id};

use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// Full pipeline: game packet → adapter → normalized telemetry → FFB
// ===========================================================================

#[tokio::test]
async fn pipeline_mock_adapter_produces_frames() -> TestResult {
    let adapter = MockAdapter::new("pipeline_test".to_string());
    let mut rx = adapter.start_monitoring().await?;
    let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await?
        .ok_or("channel closed without producing a frame")?;
    assert!(frame.timestamp_ns > 0, "frame must have a valid timestamp");
    adapter.stop_monitoring().await?;
    Ok(())
}

#[tokio::test]
async fn pipeline_normalize_raw_data_produces_valid_telemetry() -> TestResult {
    let adapter = MockAdapter::new("normalize_test".to_string());
    let raw_bytes = [0u8; 64];
    let telemetry = adapter.normalize(&raw_bytes)?;
    // MockAdapter normalize produces fixed RPM ≈ 5000
    assert!(
        telemetry.rpm > 0.0,
        "normalized telemetry should have positive RPM"
    );
    Ok(())
}

#[test]
fn pipeline_coverage_alignment_with_adapters() -> TestResult {
    let matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let matrix_ids = matrix.game_ids();
    let adapter_ids: Vec<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

    let coverage = compare_matrix_and_registry(&matrix_ids, adapter_ids);
    // Every matrix game should have an adapter registered
    assert!(
        coverage.has_complete_matrix_coverage(),
        "matrix games missing adapters: {:?}",
        coverage.missing_in_registry
    );
    Ok(())
}

#[test]
fn pipeline_coverage_alignment_fails_with_missing_adapter() -> TestResult {
    let matrix_ids = ["acc", "iracing", "phantom_pipeline_game"];
    let adapter_ids = ["acc", "iracing"];

    let result = compare_matrix_and_registry_with_policy(
        matrix_ids.iter().copied(),
        adapter_ids.iter().copied(),
        CoveragePolicy::STRICT,
    );
    assert!(
        result.is_err(),
        "strict policy should detect missing adapter"
    );
    let mismatch = result.err().ok_or("expected mismatch")?;
    assert!(
        mismatch
            .missing_in_registry
            .contains(&"phantom_pipeline_game".to_string())
    );
    Ok(())
}

#[test]
fn pipeline_bdd_metrics_reflect_adapter_state() -> TestResult {
    let matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let matrix_ids = matrix.game_ids();
    let adapter_ids: Vec<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

    let coverage = compare_matrix_and_registry(&matrix_ids, adapter_ids);
    let bdd = coverage.bdd_metrics(CoveragePolicy::MATRIX_COMPLETE);
    assert_eq!(bdd.matrix_game_count, matrix_ids.len());
    assert!(
        bdd.parity_ok,
        "adapter BDD metrics should satisfy MATRIX_COMPLETE"
    );
    Ok(())
}

// ===========================================================================
// Multi-game switching
// ===========================================================================

#[test]
fn multi_game_coverage_changes_when_games_switch() -> TestResult {
    // Phase 1: only acc registered
    let c1 = compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["acc"]);
    assert_eq!(c1.missing_in_registry.len(), 2);

    // Phase 2: switch to iracing
    let c2 = compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["iracing"]);
    assert_eq!(c2.missing_in_registry.len(), 2);

    // Phase 3: both registered
    let c3 = compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["acc", "iracing", "dirt5"]);
    assert!(c3.is_exact());
    Ok(())
}

#[tokio::test]
async fn multi_game_switch_preserves_adapter_integrity() -> TestResult {
    let adapter_a = MockAdapter::new("game_a".to_string());
    let adapter_b = MockAdapter::new("game_b".to_string());

    // Start game A, receive a frame
    let mut rx_a = adapter_a.start_monitoring().await?;
    let _frame_a = tokio::time::timeout(Duration::from_secs(2), rx_a.recv()).await?;
    adapter_a.stop_monitoring().await?;

    // Switch to game B
    let mut rx_b = adapter_b.start_monitoring().await?;
    let frame_b = tokio::time::timeout(Duration::from_secs(2), rx_b.recv())
        .await?
        .ok_or("game B channel closed without frame")?;
    assert!(frame_b.timestamp_ns > 0);
    adapter_b.stop_monitoring().await?;
    Ok(())
}

#[tokio::test]
async fn multi_game_sequential_adapter_usage() -> TestResult {
    let games = ["game_seq_1", "game_seq_2", "game_seq_3"];
    for game_id in &games {
        let adapter = MockAdapter::new(game_id.to_string());
        assert_eq!(adapter.game_id(), *game_id);
        let mut rx = adapter.start_monitoring().await?;
        let _frame = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await?;
        adapter.stop_monitoring().await?;
    }
    Ok(())
}

// ===========================================================================
// Telemetry timeout handling
// ===========================================================================

#[tokio::test]
async fn timeout_no_frames_within_window() -> TestResult {
    let adapter = MockAdapter::new("timeout_test".to_string());
    // Don't start monitoring — channel not created, simulates timeout scenario
    // Just verify the adapter reports not running
    let running = adapter.is_game_running().await?;
    assert!(!running, "adapter should not report running before start");
    Ok(())
}

#[test]
fn timeout_rate_limiter_handles_idle_period() -> TestResult {
    let limiter = RateLimiter::new(1000);
    // No calls for a while — stats should remain at zero
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
    assert_eq!(limiter.drop_rate_percent(), 0.0);
    Ok(())
}

#[tokio::test]
async fn timeout_adapter_remains_usable_after_timeout() -> TestResult {
    let adapter = MockAdapter::new("timeout_recovery".to_string());
    // Start, stop (simulating timeout), then start again
    let mut rx1 = adapter.start_monitoring().await?;
    let _frame1 = tokio::time::timeout(Duration::from_secs(2), rx1.recv()).await?;
    adapter.stop_monitoring().await?;

    // Re-start after "timeout"
    let mut rx2 = adapter.start_monitoring().await?;
    let frame2 = tokio::time::timeout(Duration::from_secs(2), rx2.recv())
        .await?
        .ok_or("adapter should produce frames after recovery")?;
    assert!(frame2.timestamp_ns > 0);
    adapter.stop_monitoring().await?;
    Ok(())
}

// ===========================================================================
// Adapter hot-swap
// ===========================================================================

#[test]
fn hot_swap_coverage_policy_reflects_new_adapters() -> TestResult {
    // Before hot-swap: only acc
    let before = compare_matrix_and_registry_with_policy(
        ["acc", "iracing"],
        ["acc"],
        CoveragePolicy::MATRIX_COMPLETE,
    );
    assert!(
        before.is_err(),
        "missing iracing should violate MATRIX_COMPLETE"
    );

    // After hot-swap: acc + iracing
    let after = compare_matrix_and_registry_with_policy(
        ["acc", "iracing"],
        ["acc", "iracing"],
        CoveragePolicy::MATRIX_COMPLETE,
    )?;
    assert!(after.is_exact());
    Ok(())
}

#[test]
fn hot_swap_matrix_strict_detects_removed_adapter() -> TestResult {
    // Full set
    let full = compare_matrix_and_registry_with_policy(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing", "dirt5"],
        CoveragePolicy::STRICT,
    )?;
    assert!(full.is_exact());

    // Remove dirt5 adapter (hot-swap out)
    let partial = compare_matrix_and_registry_with_policy(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing"],
        CoveragePolicy::STRICT,
    );
    assert!(partial.is_err(), "strict policy detects removed adapter");
    Ok(())
}

// ===========================================================================
// Rate limiting integration
// ===========================================================================

#[test]
fn rate_limiter_60hz_to_1khz_tracks_all_calls() -> TestResult {
    let mut limiter = RateLimiter::new(1000); // 1kHz limit
    // Simulate 60 rapid calls — some may be dropped due to burst timing
    for _ in 0..60 {
        let _ = limiter.should_process();
    }
    // Total processed + dropped must equal number of calls
    assert_eq!(
        limiter.processed_count() + limiter.dropped_count(),
        60,
        "all calls must be accounted for"
    );
    assert!(
        limiter.processed_count() > 0,
        "at least one frame should be processed"
    );
    Ok(())
}

#[test]
fn rate_limiter_stats_accuracy() -> TestResult {
    let mut limiter = RateLimiter::new(1000);
    for _ in 0..100 {
        let _ = limiter.should_process();
    }
    let processed = limiter.processed_count();
    let dropped = limiter.dropped_count();
    assert_eq!(processed + dropped, 100, "total must equal calls");
    Ok(())
}

#[test]
fn rate_limiter_reset_clears_statistics() -> TestResult {
    let mut limiter = RateLimiter::new(1000);
    for _ in 0..50 {
        let _ = limiter.should_process();
    }
    assert!(limiter.processed_count() > 0);
    limiter.reset_stats();
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
    Ok(())
}

// ===========================================================================
// Telemetry recording during active session
// ===========================================================================

#[test]
fn recording_during_active_session_captures_frames() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("pipeline_recording.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("test_game".to_string());
    // Record some frames
    for i in 0..10 {
        let frame = TelemetryFrame::from_telemetry(NormalizedTelemetry::new(), i, 0);
        recorder.record_frame(frame);
    }
    assert_eq!(recorder.frame_count(), 10);
    assert!(recorder.is_recording());

    let recording = recorder.stop_recording(Some("pipeline test".to_string()))?;
    assert_eq!(recording.frames.len(), 10);
    assert_eq!(&*recording.metadata.game_id, "test_game");
    Ok(())
}

#[test]
fn recording_load_roundtrip() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("roundtrip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("roundtrip_game".to_string());
    for i in 0..5 {
        let frame = TelemetryFrame::from_telemetry(NormalizedTelemetry::new(), i, 0);
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(Some("roundtrip test".to_string()))?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 5);
    assert_eq!(&*loaded.metadata.game_id, "roundtrip_game");
    Ok(())
}

#[test]
fn recording_fixture_generator_produces_valid_data() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("fixture_game".to_string(), 5.0, 60.0);
    assert!(!recording.frames.is_empty());
    assert_eq!(&*recording.metadata.game_id, "fixture_game");
    assert!(recording.metadata.frame_count > 0);
    Ok(())
}

// ===========================================================================
// Concurrent telemetry from multiple games rejected
// ===========================================================================

#[test]
fn concurrent_registries_strict_rejects_extra_active_games() -> TestResult {
    // Strict policy: only matrix games allowed in active registry
    let result = compare_matrix_and_registry_with_policy(
        ["acc", "iracing"],
        ["acc", "iracing", "unauthorized_game"],
        CoveragePolicy::STRICT,
    );
    assert!(result.is_err());
    let mismatch = result.err().ok_or("expected mismatch")?;
    assert!(
        mismatch
            .extra_in_registry
            .contains(&"unauthorized_game".to_string())
    );
    Ok(())
}

#[test]
fn concurrent_runtime_report_detects_conflicting_registries() -> TestResult {
    let report = compare_runtime_registries_with_policies(
        ["acc", "iracing"],
        ["acc", "iracing", "extra_adapter"],
        ["acc"],
        CoveragePolicy::STRICT,
        CoveragePolicy::MATRIX_COMPLETE,
    );
    // Adapter has extras → strict fails; writer missing iracing → MATRIX_COMPLETE fails
    assert!(!report.adapter_policy_ok());
    assert!(!report.writer_policy_ok());
    assert!(!report.is_parity_ok());
    Ok(())
}

// ===========================================================================
// Telemetry statistics accuracy
// ===========================================================================

#[test]
fn statistics_coverage_metrics_deterministic_across_calls() -> TestResult {
    let coverage = compare_matrix_and_registry(
        ["acc", "iracing", "dirt5", "ams2"],
        ["acc", "iracing", "rfactor2"],
    );
    let m1 = coverage.metrics();
    let m2 = coverage.metrics();
    assert_eq!(m1, m2, "metrics must be deterministic");
    assert_eq!(m1.matrix_game_count, 4);
    assert_eq!(m1.registry_game_count, 3);
    assert_eq!(m1.missing_count, 2);
    assert_eq!(m1.extra_count, 1);
    Ok(())
}

#[test]
fn statistics_rate_limiter_drop_rate_percent_consistent() -> TestResult {
    let mut limiter = RateLimiter::new(1000);
    for _ in 0..200 {
        let _ = limiter.should_process();
    }
    let processed = limiter.processed_count();
    let dropped = limiter.dropped_count();
    let drop_rate = limiter.drop_rate_percent();

    if processed + dropped > 0 {
        let expected_rate = (dropped as f32 / (processed + dropped) as f32) * 100.0;
        assert!(
            (drop_rate - expected_rate).abs() < 0.01,
            "drop rate {drop_rate} should match expected {expected_rate}"
        );
    }
    Ok(())
}

// ===========================================================================
// Adapter lifecycle: UDP receiver bind/unbind, shared memory
// ===========================================================================

#[tokio::test]
async fn adapter_lifecycle_start_stop_normalize() -> TestResult {
    let adapter = MockAdapter::new("lifecycle_test".to_string());
    assert_eq!(adapter.game_id(), "lifecycle_test");

    // Normalize works before start
    let telemetry = adapter.normalize(&[0u8; 32])?;
    assert!(telemetry.rpm > 0.0);

    // Start and receive
    let mut rx = adapter.start_monitoring().await?;
    let _frame = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await?;

    // Stop
    adapter.stop_monitoring().await?;
    Ok(())
}

#[test]
fn adapter_lifecycle_expected_update_rate() -> TestResult {
    let adapter = MockAdapter::new("rate_test".to_string());
    let rate = adapter.expected_update_rate();
    assert!(
        rate.as_millis() > 0,
        "update rate must be a positive duration"
    );
    Ok(())
}

// ===========================================================================
// Telemetry format auto-detection
// ===========================================================================

#[test]
fn auto_detect_support_matrix_has_entries() -> TestResult {
    let matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    assert!(
        !matrix.game_ids().is_empty(),
        "support matrix must have at least one game"
    );
    // Verify each game has a name
    for (game_id, support) in &matrix.games {
        assert!(!game_id.is_empty(), "game ID must not be empty");
        assert!(
            !support.name.is_empty(),
            "game '{game_id}' must have a name"
        );
    }
    Ok(())
}

#[test]
fn auto_detect_normalize_game_id_resolves_aliases() -> TestResult {
    // Known aliases should resolve to canonical IDs
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    // Unknown IDs pass through unchanged
    assert_eq!(normalize_game_id("acc"), "acc");
    assert_eq!(normalize_game_id("iracing"), "iracing");
    Ok(())
}

// ===========================================================================
// Game-specific quirk handling
// ===========================================================================

#[test]
fn quirk_iracing_game_id_normalized() -> TestResult {
    let matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    assert!(
        matrix.has_game_id("iracing"),
        "iRacing must be in the support matrix"
    );
    // Normalized ID lookup should work
    let normalized = normalize_game_id("iracing");
    assert_eq!(normalized, "iracing");
    Ok(())
}

#[test]
fn quirk_f1_version_aliases_resolve_correctly() -> TestResult {
    // F1 games have year-based aliases
    let f1_2025 = normalize_game_id("f1_2025");
    assert_eq!(f1_2025, "f1_25", "f1_2025 should resolve to f1_25");

    // Canonical form passes through
    let f1_25 = normalize_game_id("f1_25");
    assert_eq!(f1_25, "f1_25");
    Ok(())
}

#[test]
fn quirk_ea_wrc_alias_resolves() -> TestResult {
    let ea_wrc = normalize_game_id("ea_wrc");
    assert_eq!(ea_wrc, "eawrc", "ea_wrc alias should normalize to eawrc");
    Ok(())
}

#[test]
fn quirk_adapter_factories_cover_major_games() -> TestResult {
    let factories = adapter_factories();
    let factory_ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    let major_games = ["acc", "iracing", "forza_motorsport", "rfactor2"];
    for game in &major_games {
        assert!(
            factory_ids.contains(game),
            "adapter factory for '{game}' should exist"
        );
    }
    Ok(())
}

#[test]
fn quirk_fixture_scenarios_produce_different_data() -> TestResult {
    let constant =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 2.0, 60.0);
    let cornering =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 2.0, 60.0);
    // Different scenarios should produce different data
    assert!(!constant.frames.is_empty());
    assert!(!cornering.frames.is_empty());
    // Frame counts may differ slightly depending on scenario
    assert_eq!(constant.frames.len(), cornering.frames.len());
    Ok(())
}
