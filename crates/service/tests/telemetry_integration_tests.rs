//! Integration tests for telemetry module
//!
//! These tests validate the complete telemetry functionality including:
//! - iRacing telemetry adapter with shared memory interface
//! - ACC telemetry adapter using UDP broadcast protocol  
//! - Rate limiter to protect RT thread from telemetry parsing overhead
//! - Telemetry normalization to common NormalizedTelemetry struct
//! - Record-and-replay fixtures for CI testing without running actual games
//! - Adapter tests with recorded game data for validation
//!
//! Requirements: GI-03, GI-04

use openracing_telemetry_config::support::matrix_game_ids;
use racing_wheel_service::telemetry::*;
use std::collections::HashSet;
use std::time::Duration;
use tempfile::tempdir;

#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

#[track_caller]
fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("unexpected None: {msg}"),
    }
}

#[test]
fn test_normalized_telemetry_creation() {
    let telemetry = NormalizedTelemetry::builder()
        .ffb_scalar(0.75)
        .rpm(6500.0)
        .speed_ms(45.0)
        .slip_ratio(0.15)
        .gear(4)
        .car_id("gt3_bmw")
        .track_id("spa")
        .build();

    assert!((telemetry.ffb_scalar - 0.75).abs() < 0.01);
    assert!((telemetry.rpm - 6500.0).abs() < 0.01);
    assert!((telemetry.speed_ms - 45.0).abs() < 0.01);
    assert!((telemetry.slip_ratio - 0.15).abs() < 0.01);
    assert_eq!(telemetry.gear, 4);
    assert_eq!(telemetry.car_id, Some("gt3_bmw".to_string()));
    assert_eq!(telemetry.track_id, Some("spa".to_string()));
}

#[test]
fn test_ffb_scalar_clamping() {
    let telemetry1 = NormalizedTelemetry::builder().ffb_scalar(1.5).build();
    assert!((telemetry1.ffb_scalar - 1.0).abs() < 0.01);

    let telemetry2 = NormalizedTelemetry::builder().ffb_scalar(-1.5).build();
    assert!((telemetry2.ffb_scalar - (-1.0)).abs() < 0.01);
}

#[test]
fn test_slip_ratio_clamping() {
    let telemetry1 = NormalizedTelemetry::builder().slip_ratio(1.5).build();
    assert!((telemetry1.slip_ratio - 1.0).abs() < 0.01);

    let telemetry2 = NormalizedTelemetry::builder().slip_ratio(-0.5).build();
    assert!((telemetry2.slip_ratio - 0.0).abs() < 0.01);
}

#[test]
fn test_invalid_values_rejected() {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(-100.0) // Negative RPM should be rejected
        .speed_ms(f32::NAN) // NaN should be rejected
        .build();

    assert!((telemetry.rpm - 0.0).abs() < 0.01); // Default to 0
    assert!((telemetry.speed_ms - 0.0).abs() < 0.01); // Default to 0
}

#[test]
fn test_speed_conversions() {
    let telemetry = NormalizedTelemetry::builder().speed_ms(27.78).build(); // 100 km/h

    assert!((telemetry.speed_kmh() - 100.0).abs() < 0.1);
    assert!((telemetry.speed_mph() - 62.14).abs() < 0.1);
}

#[test]
fn test_rpm_fraction() {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .max_rpm(8000.0)
        .build();

    let fraction = telemetry.rpm_fraction();
    assert!((fraction - 0.75).abs() < 0.01);
}

#[test]
fn test_flags() {
    let flags = TelemetryFlags {
        yellow_flag: true,
        pit_limiter: true,
        ..Default::default()
    };

    let telemetry = NormalizedTelemetry::builder().flags(flags).build();

    assert!(telemetry.has_active_flags());
    assert!(telemetry.flags.yellow_flag);
    assert!(telemetry.flags.pit_limiter);
}

#[test]
fn test_extended_data() {
    let telemetry = NormalizedTelemetry::default()
        .with_extended("fuel_level".to_string(), TelemetryValue::Float(45.5))
        .with_extended("lap_count".to_string(), TelemetryValue::Integer(12))
        .with_extended(
            "session_type".to_string(),
            TelemetryValue::String("Race".to_string()),
        );

    assert_eq!(telemetry.extended.len(), 3);

    if let Some(TelemetryValue::Float(fuel)) = telemetry.extended.get("fuel_level") {
        assert_eq!(*fuel, 45.5);
    } else {
        panic!("Expected fuel_level to be a float");
    }
}

#[test]
fn test_rate_limiter_creation() {
    let limiter = RateLimiter::new(1000);
    assert_eq!(limiter.max_rate_hz(), 1000);
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
}

#[test]
fn test_rate_limiting() {
    let mut limiter = RateLimiter::new(10); // 10 Hz = 100ms interval

    // First call should be allowed
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 1);

    // Immediate second call should be dropped
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
    assert_eq!(limiter.processed_count(), 1);
}

#[test]
fn test_drop_rate_calculation() {
    let mut limiter = RateLimiter::new(10);

    // Process one, drop one
    assert!(limiter.should_process());
    assert!(!limiter.should_process());

    assert_eq!(limiter.drop_rate_percent(), 50.0);
}

#[test]
fn test_stats_reset() {
    let mut limiter = RateLimiter::new(10);

    limiter.should_process();
    limiter.should_process(); // This will be dropped

    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 1);

    limiter.reset_stats();

    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
}

#[tokio::test]
async fn test_async_rate_limiting() {
    // Wrap test body with timeout to ensure test completes within 5 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let mut limiter = RateLimiter::new(100); // 100 Hz = 10ms interval

        let start = std::time::Instant::now();

        // First call should be immediate
        limiter.wait_for_slot().await;
        let first_elapsed = start.elapsed();

        // Second call should wait
        limiter.wait_for_slot().await;
        let second_elapsed = start.elapsed();

        // Should have waited at least the minimum interval
        assert!(second_elapsed >= first_elapsed + Duration::from_millis(8)); // Allow some tolerance
        assert_eq!(limiter.processed_count(), 2);
    };

    match tokio::time::timeout(Duration::from_secs(5), test_future).await {
        Ok(()) => {}
        Err(_elapsed) => {
            panic!(
                "test_async_rate_limiting timed out after 5 seconds - \
                 rate limiter may be blocked"
            );
        }
    }
}

#[test]
fn test_adaptive_rate_limiter() {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);

    // High CPU usage should reduce rate
    adaptive.update_cpu_usage(80.0);
    let stats_high = adaptive.stats();

    // Low CPU usage should increase rate
    adaptive.update_cpu_usage(20.0);
    let stats_low = adaptive.stats();

    // Rate should have been adjusted (though exact values depend on adjustment logic)
    assert!(stats_low.max_rate_hz >= stats_high.max_rate_hz);
}

#[test]
fn test_rate_limiter_stats() {
    let mut limiter = RateLimiter::new(100);

    limiter.should_process();
    limiter.should_process(); // Dropped
    limiter.should_process(); // Dropped

    let stats = RateLimiterStats::from(&limiter);

    assert_eq!(stats.max_rate_hz, 100);
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 2);
    assert!((stats.drop_rate_percent - 66.67).abs() < 0.1);
}

#[test]
fn test_recorder_creation() {
    let temp_dir = must(tempdir());
    let output_path = temp_dir.path().join("test_recording.json");

    let recorder = TelemetryRecorder::new(output_path);
    assert!(recorder.is_ok());
}

#[test]
fn test_recording_lifecycle() {
    let temp_dir = must(tempdir());
    let output_path = temp_dir.path().join("test_recording.json");

    let mut recorder = must(TelemetryRecorder::new(output_path.clone()));

    // Start recording
    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());

    // Record some frames
    let telemetry = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::new(telemetry, 1000000, 0, 64);
    recorder.record_frame(frame);

    assert_eq!(recorder.frame_count(), 1);

    // Stop recording
    let recording = must(recorder.stop_recording(Some("Test recording".to_string())));
    assert!(!recorder.is_recording());
    assert_eq!(recording.frames.len(), 1);
    assert_eq!(recording.metadata.game_id, "test_game");

    // Verify file was created
    assert!(output_path.exists());
}

#[test]
fn test_load_recording() {
    let temp_dir = must(tempdir());
    let output_path = temp_dir.path().join("test_recording.json");

    // Create and save a recording
    let mut recorder = must(TelemetryRecorder::new(output_path.clone()));
    recorder.start_recording("test_game".to_string());

    let telemetry = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::new(telemetry, 1000000, 0, 64);
    recorder.record_frame(frame);

    let _recording = must(recorder.stop_recording(Some("Test recording".to_string())));

    // Load the recording
    let loaded = must(TelemetryRecorder::load_recording(&output_path));
    assert_eq!(loaded.metadata.game_id, "test_game");
    assert_eq!(loaded.frames.len(), 1);
}

#[test]
fn test_telemetry_player() {
    let recording = TestFixtureGenerator::generate_racing_session(
        "test_game".to_string(),
        1.0,  // 1 second
        10.0, // 10 FPS
    );

    let mut player = TelemetryPlayer::new(recording);

    // Start playback
    player.start_playback();
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());

    // Should have frames to play
    assert!(player.get_next_frame().is_some());

    // Progress should increase
    assert!(player.progress() > 0.0);
}

#[test]
fn test_synthetic_fixture_generation() {
    let recording = TestFixtureGenerator::generate_racing_session(
        "test_game".to_string(),
        2.0,  // 2 seconds
        60.0, // 60 FPS
    );

    assert_eq!(recording.metadata.game_id, "test_game");
    assert_eq!(recording.metadata.frame_count, 120); // 2 * 60
    assert_eq!(recording.frames.len(), 120);

    // Check that frames have reasonable data
    for frame in &recording.frames {
        assert!(frame.data.rpm > 0.0);
        assert!(frame.data.speed_ms > 0.0);
        // ffb_scalar can be negative (braking/counterforce) or zero; just verify valid range
        assert!(frame.data.ffb_scalar.is_finite());
        assert!(frame.data.ffb_scalar >= -1.0 && frame.data.ffb_scalar <= 1.0);
    }
}

#[test]
fn test_test_scenarios() {
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];

    for scenario in scenarios {
        let recording = TestFixtureGenerator::generate_test_scenario(scenario, 1.0, 30.0);

        assert_eq!(recording.frames.len(), 30);
        assert!(recording.metadata.description.is_some());
    }
}

#[tokio::test]
async fn test_mock_adapter() {
    // Wrap test body with timeout to ensure test completes within 5 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let mut adapter = MockAdapter::new("test_game".to_string());
        adapter.set_running(true);

        assert_eq!(adapter.game_id(), "test_game");
        assert!(must(adapter.is_game_running().await));

        let mut receiver = must(adapter.start_monitoring().await);

        // Should receive telemetry frames
        let frame = must_some(
            must(tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await),
            "expected frame",
        );

        assert!(frame.data.rpm > 0.0);
        assert!(frame.data.speed_ms > 0.0);
        assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
    };

    match tokio::time::timeout(Duration::from_secs(5), test_future).await {
        Ok(()) => {}
        Err(_elapsed) => {
            panic!(
                "test_mock_adapter timed out after 5 seconds - \
                 mock adapter may be blocked"
            );
        }
    }
}

#[test]
fn test_iracing_adapter_creation() {
    let adapter = IRacingAdapter::new();
    assert_eq!(adapter.game_id(), "iracing");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn test_acc_adapter_creation() {
    let adapter = ACCAdapter::new();
    assert_eq!(adapter.game_id(), "acc");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn test_telemetry_service_creation() {
    let service = TelemetryService::new();

    let expected: HashSet<String> = must(matrix_game_ids()).into_iter().collect();
    let actual: HashSet<String> = service.matrix_game_ids().into_iter().collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_telemetry_service_monitoring() {
    // Wrap test body with timeout to ensure test completes within 5 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let mut service = TelemetryService::new();

        // Test starting monitoring for unsupported game
        let result = service.start_monitoring("unsupported_game").await;
        assert!(result.is_err());

        // Test checking if games are running
        let iracing_running = service.is_game_running("iracing").await;
        assert!(iracing_running.is_ok());

        let acc_running = service.is_game_running("acc").await;
        assert!(acc_running.is_ok());
    };

    match tokio::time::timeout(Duration::from_secs(5), test_future).await {
        Ok(()) => {}
        Err(_elapsed) => {
            panic!(
                "test_telemetry_service_monitoring timed out after 5 seconds - \
                 telemetry service may be blocked"
            );
        }
    }
}

/// Integration test that validates the complete telemetry pipeline
#[test]
fn test_complete_telemetry_pipeline() {
    let temp_dir = must(tempdir());
    let recording_path = temp_dir.path().join("pipeline_test.json");

    // Create a synthetic recording
    let recording = TestFixtureGenerator::generate_racing_session(
        "test_game".to_string(),
        1.0,  // 1 second
        60.0, // 60 FPS
    );

    // Save the recording
    let mut recorder = must(TelemetryRecorder::new(recording_path.clone()));
    recorder.start_recording("test_game".to_string());

    for frame in &recording.frames {
        recorder.record_frame(frame.clone());
    }

    let recording = must(recorder.stop_recording(Some("Pipeline test".to_string())));

    // Load the recording and verify it was persisted correctly
    let loaded_recording = must(TelemetryRecorder::load_recording(&recording_path));

    // Verify that we loaded the expected number of frames
    assert_eq!(loaded_recording.frames.len(), recording.frames.len());

    // Verify that the data is consistent after save/load
    for (original, loaded) in recording.frames.iter().zip(loaded_recording.frames.iter()) {
        assert_eq!(original.data.rpm, loaded.data.rpm);
        assert_eq!(original.data.speed_ms, loaded.data.speed_ms);
        assert_eq!(original.data.gear, loaded.data.gear);
    }

    // Verify player can be created and started (playback timing tested elsewhere)
    let mut player = TelemetryPlayer::new(loaded_recording);
    player.start_playback();
    assert!(!player.is_finished());
    assert_eq!(player.progress(), 0.0);
}

/// Test rate limiting protection for RT thread
#[test]
fn test_rate_limiting_protection() {
    let mut rate_limiter = RateLimiter::new(100); // 100 Hz max

    // Simulate high-frequency telemetry data
    let mut processed = 0;
    let mut dropped = 0;

    for _ in 0..1000 {
        if rate_limiter.should_process() {
            processed += 1;
        } else {
            dropped += 1;
        }
    }

    // Should have dropped most frames to protect RT thread
    assert!(dropped > processed);
    assert_eq!(rate_limiter.processed_count(), processed);
    assert_eq!(rate_limiter.dropped_count(), dropped);
}

/// Test telemetry adapter error handling
#[test]
fn test_adapter_error_handling() {
    let iracing_adapter = IRacingAdapter::new();
    let acc_adapter = ACCAdapter::new();

    // Test invalid data handling
    let invalid_data = vec![0u8; 10];

    let iracing_result = iracing_adapter.normalize(&invalid_data);
    assert!(iracing_result.is_err());

    let acc_result = acc_adapter.normalize(&invalid_data);
    assert!(acc_result.is_err());
}

/// Test telemetry data normalization consistency
#[test]
fn test_normalization_consistency() {
    // Create test data that should normalize consistently
    let test_cases: Vec<(f32, f32, i8, f32)> = vec![
        (5000.0, 50.0, 4, 0.5), // RPM, speed_ms, gear, ffb_scalar
        (7500.0, 75.0, 6, -0.3),
        (3000.0, 25.0, 2, 0.8),
    ];

    for (rpm, speed, gear, ffb) in test_cases {
        let telemetry = NormalizedTelemetry::builder()
            .rpm(rpm)
            .speed_ms(speed)
            .gear(gear)
            .ffb_scalar(ffb)
            .build();

        // Verify normalization is consistent
        assert!((telemetry.rpm - rpm).abs() < 0.01);
        assert!((telemetry.speed_ms - speed).abs() < 0.01);
        assert_eq!(telemetry.gear, gear);
        assert!((telemetry.ffb_scalar - ffb.clamp(-1.0, 1.0)).abs() < 0.01);
    }
}

/// Test telemetry field coverage information
#[test]
fn test_telemetry_field_coverage() {
    let telemetry = NormalizedTelemetry::builder()
        .ffb_scalar(0.75)
        .rpm(6500.0)
        .speed_ms(45.0)
        .slip_ratio(0.15)
        .gear(4)
        .car_id("gt3_bmw")
        .track_id("spa")
        .build();

    assert!(telemetry.has_ffb_data());
    assert!(telemetry.has_rpm_data());

    // Test RPM fraction calculation
    let telemetry_with_max = NormalizedTelemetry::builder()
        .rpm(6500.0)
        .max_rpm(8000.0)
        .build();
    let rpm_fraction = telemetry_with_max.rpm_fraction();
    assert!((rpm_fraction - 0.8125).abs() < 0.01); // 6500/8000 = 0.8125

    // Test speed conversions
    assert!((telemetry.speed_kmh() - 162.0).abs() < 0.1); // 45 m/s = 162 km/h
    assert!((telemetry.speed_mph() - 100.65).abs() < 0.1); // 45 m/s ≈ 100.65 mph
}

/// Test telemetry flags functionality
#[test]
fn test_telemetry_flags_comprehensive() {
    let flags = TelemetryFlags {
        yellow_flag: true,
        pit_limiter: true,
        drs_available: true,
        ers_available: true,
        traction_control: true,
        ..Default::default()
    };

    let telemetry = NormalizedTelemetry::builder().flags(flags).build();

    assert!(telemetry.has_active_flags());
    assert!(telemetry.flags.yellow_flag);
    assert!(telemetry.flags.pit_limiter);
    assert!(telemetry.flags.drs_available);
    assert!(telemetry.flags.ers_available);
    assert!(telemetry.flags.traction_control);
    assert!(!telemetry.flags.red_flag);
    assert!(!telemetry.flags.blue_flag);
}

/// Test extended telemetry data with all value types
#[test]
fn test_extended_telemetry_all_types() {
    let telemetry = NormalizedTelemetry::default()
        .with_extended("fuel_level".to_string(), TelemetryValue::Float(45.5))
        .with_extended("lap_count".to_string(), TelemetryValue::Integer(12))
        .with_extended(
            "session_type".to_string(),
            TelemetryValue::String("Race".to_string()),
        )
        .with_extended("drs_enabled".to_string(), TelemetryValue::Boolean(true));

    assert_eq!(telemetry.extended.len(), 4);

    // Verify each extended value type
    match telemetry.extended.get("fuel_level") {
        Some(TelemetryValue::Float(fuel)) => assert_eq!(fuel, &45.5),
        _ => panic!("Expected fuel_level to be a float"),
    }

    match telemetry.extended.get("lap_count") {
        Some(TelemetryValue::Integer(laps)) => assert_eq!(laps, &12),
        _ => panic!("Expected lap_count to be an integer"),
    }

    match telemetry.extended.get("session_type") {
        Some(TelemetryValue::String(session)) => assert_eq!(session, "Race"),
        _ => panic!("Expected session_type to be a string"),
    }

    match telemetry.extended.get("drs_enabled") {
        Some(TelemetryValue::Boolean(drs)) => assert!(drs),
        _ => panic!("Expected drs_enabled to be a boolean"),
    }
}

/// Test telemetry service recording functionality
#[test]
fn test_telemetry_service_recording() {
    let temp_dir = must(tempdir());
    let output_path = temp_dir.path().join("service_recording.json");

    let mut service = TelemetryService::new();

    // Enable recording
    must(service.enable_recording(output_path.clone()));

    // Disable recording
    service.disable_recording();

    // Test that we can enable/disable without errors
    // If we get here, no panics occurred
}
