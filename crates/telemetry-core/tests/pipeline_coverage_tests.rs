//! Pipeline coverage tests for telemetry-core.
//!
//! Focuses on gaps not covered by existing tests:
//! - Timestamp monotonicity (`telemetry_now_ns`)
//! - Serde roundtrips for domain types
//! - ConnectionState transitions and event coverage
//! - DisconnectionTracker/Config behavior
//! - TelemetryError display/variant exhaustiveness
//! - Property-based tests for GameTelemetry conversion

use openracing_telemetry::contracts::{FlagCoverage, TelemetryFieldCoverage};
use openracing_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
    GameTelemetry, GameTelemetrySnapshot, NormalizedTelemetry, TelemetryError, TelemetryFlags,
    TelemetryFrame, TelemetryValue, telemetry_now_ns,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Timestamp monotonicity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_now_ns_is_monotonic_across_calls() -> TestResult {
    let mut prev = telemetry_now_ns();
    for _ in 0..100 {
        let curr = telemetry_now_ns();
        assert!(
            curr >= prev,
            "timestamps must be monotonically non-decreasing"
        );
        prev = curr;
    }
    Ok(())
}

#[test]
fn telemetry_now_ns_returns_nonzero_after_brief_sleep() -> TestResult {
    std::thread::sleep(std::time::Duration::from_millis(1));
    let ts = telemetry_now_ns();
    assert!(ts > 0, "should be nonzero after a sleep");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Serde roundtrips
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalized_telemetry_json_roundtrip() -> TestResult {
    let original = NormalizedTelemetry::builder()
        .rpm(7500.0)
        .speed_ms(42.0)
        .gear(4)
        .throttle(0.85)
        .brake(0.0)
        .lateral_g(1.2)
        .longitudinal_g(-0.3)
        .slip_ratio(0.05)
        .car_id("test_car")
        .track_id("test_track")
        .extended("boost_psi", TelemetryValue::Float(14.7))
        .build();

    let json = serde_json::to_string(&original)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(deserialized.rpm, original.rpm);
    assert_eq!(deserialized.speed_ms, original.speed_ms);
    assert_eq!(deserialized.gear, original.gear);
    assert_eq!(deserialized.throttle, original.throttle);
    assert_eq!(deserialized.lateral_g, original.lateral_g);
    assert_eq!(deserialized.car_id, original.car_id);
    assert_eq!(deserialized.track_id, original.track_id);
    assert_eq!(deserialized.extended.len(), original.extended.len());
    Ok(())
}

#[test]
fn telemetry_frame_json_roundtrip() -> TestResult {
    let data = NormalizedTelemetry::builder()
        .rpm(3000.0)
        .speed_ms(20.0)
        .build();
    let frame = TelemetryFrame::new(data, 123_456_789, 42, 128);

    let json = serde_json::to_string(&frame)?;
    let deserialized: TelemetryFrame = serde_json::from_str(&json)?;

    assert_eq!(deserialized.timestamp_ns, frame.timestamp_ns);
    assert_eq!(deserialized.sequence, frame.sequence);
    assert_eq!(deserialized.raw_size, frame.raw_size);
    assert_eq!(deserialized.data.rpm, frame.data.rpm);
    assert_eq!(deserialized.data.speed_ms, frame.data.speed_ms);
    Ok(())
}

#[test]
fn telemetry_flags_json_roundtrip() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        in_pits: true,
        pit_limiter: true,
        drs_active: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };

    let json = serde_json::to_string(&flags)?;
    let deserialized: TelemetryFlags = serde_json::from_str(&json)?;

    assert_eq!(deserialized.yellow_flag, flags.yellow_flag);
    assert_eq!(deserialized.in_pits, flags.in_pits);
    assert_eq!(deserialized.pit_limiter, flags.pit_limiter);
    assert_eq!(deserialized.drs_active, flags.drs_active);
    assert_eq!(deserialized.abs_active, flags.abs_active);
    assert!(!deserialized.red_flag);
    Ok(())
}

#[test]
fn connection_state_event_json_roundtrip() -> TestResult {
    let event = ConnectionStateEvent::new(
        "forza_motorsport",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        Some("Data received".to_string()),
    );

    let json = serde_json::to_string(&event)?;
    let deserialized: ConnectionStateEvent = serde_json::from_str(&json)?;

    assert_eq!(deserialized.game_id, "forza_motorsport");
    assert_eq!(deserialized.previous_state, ConnectionState::Disconnected);
    assert_eq!(deserialized.new_state, ConnectionState::Connected);
    assert_eq!(deserialized.reason, Some("Data received".to_string()));
    Ok(())
}

#[test]
fn game_telemetry_snapshot_json_roundtrip() -> TestResult {
    let epoch = std::time::Instant::now();
    let telemetry = GameTelemetry {
        speed_mps: 55.5,
        rpm: 7200.0,
        gear: 5,
        steering_angle: -0.15,
        throttle: 1.0,
        brake: 0.0,
        lateral_g: 2.1,
        longitudinal_g: 0.8,
        slip_angle_fl: 0.01,
        slip_angle_fr: 0.02,
        slip_angle_rl: 0.03,
        slip_angle_rr: 0.04,
        ..Default::default()
    };

    let snapshot = GameTelemetrySnapshot::from_telemetry(&telemetry, epoch);
    let json = serde_json::to_string(&snapshot)?;
    let deserialized: GameTelemetrySnapshot = serde_json::from_str(&json)?;

    assert_eq!(deserialized.speed_mps, snapshot.speed_mps);
    assert_eq!(deserialized.rpm, snapshot.rpm);
    assert_eq!(deserialized.gear, snapshot.gear);
    assert_eq!(deserialized.steering_angle, snapshot.steering_angle);
    assert_eq!(deserialized.lateral_g, snapshot.lateral_g);
    Ok(())
}

#[test]
fn telemetry_field_coverage_json_roundtrip() -> TestResult {
    let coverage = TelemetryFieldCoverage {
        game_id: "iracing".to_string(),
        game_version: "2024.1".to_string(),
        ffb_scalar: true,
        rpm: true,
        speed: true,
        slip_ratio: true,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: true,
            red_flag: true,
            blue_flag: true,
            checkered_flag: true,
            green_flag: true,
            pit_limiter: true,
            in_pits: true,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: true,
            abs_active: true,
        },
        car_id: true,
        track_id: true,
        extended_fields: vec!["tire_wear_fl".to_string(), "fuel_kg".to_string()],
    };

    let json = serde_json::to_string(&coverage)?;
    let deserialized: TelemetryFieldCoverage = serde_json::from_str(&json)?;

    assert_eq!(deserialized.game_id, "iracing");
    assert!(deserialized.ffb_scalar);
    assert!(deserialized.flags.yellow_flag);
    assert!(!deserialized.flags.drs_available);
    assert_eq!(deserialized.extended_fields.len(), 2);
    Ok(())
}

#[test]
fn disconnection_config_json_roundtrip() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 5000,
        auto_reconnect: false,
        max_reconnect_attempts: 3,
        reconnect_delay_ms: 2000,
    };

    let json = serde_json::to_string(&config)?;
    let deserialized: DisconnectionConfig = serde_json::from_str(&json)?;

    assert_eq!(deserialized.timeout_ms, 5000);
    assert!(!deserialized.auto_reconnect);
    assert_eq!(deserialized.max_reconnect_attempts, 3);
    assert_eq!(deserialized.reconnect_delay_ms, 2000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ConnectionState transitions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn connection_state_default_is_disconnected() -> TestResult {
    let state = ConnectionState::default();
    assert!(state.is_disconnected());
    assert!(!state.is_connected());
    assert!(!state.is_transitioning());
    Ok(())
}

#[test]
fn connection_state_all_variants_classified() -> TestResult {
    let states = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
        ConnectionState::Error,
    ];

    for state in &states {
        // Every state must be exactly one of: connected, disconnected, or transitioning
        let connected = state.is_connected();
        let disconnected = state.is_disconnected();
        let transitioning = state.is_transitioning();
        let count = [connected, disconnected, transitioning]
            .iter()
            .filter(|&&b| b)
            .count();
        assert_eq!(
            count, 1,
            "state {state:?} should be exactly one category, got connected={connected} disconnected={disconnected} transitioning={transitioning}"
        );
    }
    Ok(())
}

#[test]
fn connection_state_event_detects_connection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test_game",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        None,
    );
    assert!(event.is_connection());
    assert!(!event.is_disconnection());
    Ok(())
}

#[test]
fn connection_state_event_detects_disconnection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test_game",
        ConnectionState::Connected,
        ConnectionState::Disconnected,
        None,
    );
    assert!(event.is_disconnection());
    assert!(!event.is_connection());
    Ok(())
}

#[test]
fn connection_state_event_transition_neither_connect_nor_disconnect() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test_game",
        ConnectionState::Connecting,
        ConnectionState::Reconnecting,
        Some("retry".to_string()),
    );
    assert!(!event.is_connection());
    assert!(!event.is_disconnection());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// DisconnectionTracker behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn disconnection_tracker_starts_disconnected() -> TestResult {
    let tracker = DisconnectionTracker::with_defaults("test_game");
    assert_eq!(tracker.state(), ConnectionState::Disconnected);
    assert!(!tracker.is_timed_out());
    assert_eq!(tracker.reconnect_attempts(), 0);
    Ok(())
}

#[test]
fn disconnection_tracker_records_data_and_connects() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.record_data_received();
    assert_eq!(tracker.state(), ConnectionState::Connected);
    Ok(())
}

#[test]
fn disconnection_tracker_mark_connecting() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.mark_connecting();
    assert_eq!(tracker.state(), ConnectionState::Connecting);
    Ok(())
}

#[test]
fn disconnection_tracker_mark_reconnecting_increments_attempts() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 1);
    assert_eq!(tracker.state(), ConnectionState::Reconnecting);
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 2);
    Ok(())
}

#[test]
fn disconnection_tracker_mark_error() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.mark_error("connection refused".to_string());
    assert_eq!(tracker.state(), ConnectionState::Error);
    Ok(())
}

#[test]
fn disconnection_tracker_reset_reconnect_attempts() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.mark_reconnecting();
    tracker.mark_reconnecting();
    tracker.reset_reconnect_attempts();
    assert_eq!(tracker.reconnect_attempts(), 0);
    Ok(())
}

#[test]
fn disconnection_tracker_should_reconnect_respects_config() -> TestResult {
    let config = DisconnectionConfig {
        auto_reconnect: false,
        ..Default::default()
    };
    let mut tracker = DisconnectionTracker::new("test_game", config);
    tracker.record_data_received();
    tracker.set_state(ConnectionState::Disconnected, None);
    assert!(
        !tracker.should_reconnect(),
        "auto_reconnect=false should prevent reconnect"
    );
    Ok(())
}

#[test]
fn disconnection_tracker_should_reconnect_respects_max_attempts() -> TestResult {
    let config = DisconnectionConfig {
        auto_reconnect: true,
        max_reconnect_attempts: 2,
        ..Default::default()
    };
    let mut tracker = DisconnectionTracker::new("test_game", config);
    tracker.set_state(ConnectionState::Disconnected, None);

    assert!(tracker.should_reconnect());
    tracker.mark_reconnecting();
    tracker.set_state(ConnectionState::Disconnected, None);
    assert!(tracker.should_reconnect());
    tracker.mark_reconnecting();
    tracker.set_state(ConnectionState::Disconnected, None);
    assert!(
        !tracker.should_reconnect(),
        "should stop after max attempts reached"
    );
    Ok(())
}

#[test]
fn disconnection_tracker_subscribe_receives_events() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    let mut rx = tracker.subscribe();

    tracker.mark_connecting();
    let event = rx.try_recv()?;
    assert_eq!(event.new_state, ConnectionState::Connecting);
    assert_eq!(event.game_id, "test_game");
    Ok(())
}

#[test]
fn disconnection_tracker_time_since_last_data_none_initially() -> TestResult {
    let tracker = DisconnectionTracker::with_defaults("test_game");
    assert!(tracker.time_since_last_data().is_none());
    Ok(())
}

#[test]
fn disconnection_tracker_time_since_last_data_some_after_receive() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test_game");
    tracker.record_data_received();
    let elapsed = tracker.time_since_last_data();
    assert!(elapsed.is_some());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// DisconnectionConfig
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn disconnection_config_default_values() -> TestResult {
    let config = DisconnectionConfig::default();
    assert_eq!(config.timeout_ms, 2000);
    assert!(config.auto_reconnect);
    assert_eq!(config.max_reconnect_attempts, 0);
    assert_eq!(config.reconnect_delay_ms, 1000);
    Ok(())
}

#[test]
fn disconnection_config_with_timeout() -> TestResult {
    let config = DisconnectionConfig::with_timeout(5000);
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.timeout(), std::time::Duration::from_millis(5000));
    assert_eq!(
        config.reconnect_delay(),
        std::time::Duration::from_millis(1000)
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryError display
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_error_variants_produce_meaningful_messages() -> TestResult {
    let errors: Vec<TelemetryError> = vec![
        TelemetryError::ConnectionFailed("test".to_string()),
        TelemetryError::GameNotRunning {
            game_id: "forza".to_string(),
        },
        TelemetryError::ParseError("bad data".to_string()),
        TelemetryError::SharedMemoryError("mmap fail".to_string()),
        TelemetryError::NetworkError("timeout".to_string()),
        TelemetryError::AlreadyConnected,
        TelemetryError::NotConnected,
        TelemetryError::Timeout { timeout_ms: 5000 },
        TelemetryError::InvalidData {
            reason: "truncated".to_string(),
        },
    ];

    for error in &errors {
        let msg = error.to_string();
        assert!(
            !msg.is_empty(),
            "error display must not be empty: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn telemetry_error_io_from_conversion() -> TestResult {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let telem_err: TelemetryError = io_err.into();
    let msg = telem_err.to_string();
    assert!(
        msg.contains("file missing"),
        "IO error reason should be preserved"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// GameTelemetry edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn game_telemetry_is_stationary_threshold() -> TestResult {
    let moving = GameTelemetry {
        speed_mps: 0.5,
        ..Default::default()
    };
    assert!(
        !moving.is_stationary(),
        "exactly 0.5 should not be stationary"
    );

    let stationary = GameTelemetry {
        speed_mps: 0.49,
        ..Default::default()
    };
    assert!(stationary.is_stationary());
    Ok(())
}

#[test]
fn game_telemetry_total_g_pythagorean() -> TestResult {
    let telemetry = GameTelemetry {
        lateral_g: 3.0,
        longitudinal_g: 4.0,
        ..Default::default()
    };
    assert!((telemetry.total_g() - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn game_telemetry_to_normalized_preserves_all_slip_angles() -> TestResult {
    let telemetry = GameTelemetry {
        slip_angle_fl: -0.5,
        slip_angle_fr: 0.3,
        slip_angle_rl: -0.2,
        slip_angle_rr: 0.1,
        ..Default::default()
    };

    let normalized = telemetry.to_normalized();
    assert_eq!(normalized.slip_angle_fl, -0.5);
    assert_eq!(normalized.slip_angle_fr, 0.3);
    assert_eq!(normalized.slip_angle_rl, -0.2);
    assert_eq!(normalized.slip_angle_rr, 0.1);

    // slip_ratio uses average_slip_angle().abs() — the abs of the average, not avg of abs
    let avg_slip: f32 = (-0.5 + 0.3 + -0.2 + 0.1) / 4.0;
    let expected_ratio = avg_slip.abs().min(1.0);
    assert!((normalized.slip_ratio - expected_ratio).abs() < 0.001);
    Ok(())
}

#[test]
fn game_telemetry_to_normalized_large_slip_clamps_to_one() -> TestResult {
    let telemetry = GameTelemetry {
        slip_angle_fl: 5.0,
        slip_angle_fr: 5.0,
        slip_angle_rl: 5.0,
        slip_angle_rr: 5.0,
        ..Default::default()
    };

    let normalized = telemetry.to_normalized();
    assert!(
        normalized.slip_ratio <= 1.0,
        "slip_ratio must be clamped to 1.0, got {}",
        normalized.slip_ratio
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Property-based tests
// ═══════════════════════════════════════════════════════════════════════════════

mod proptest_pipeline {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn game_telemetry_speed_conversions_consistent(speed in 0.0f32..500.0) {
            let t = GameTelemetry {
                speed_mps: speed,
                ..Default::default()
            };
            let kmh = t.speed_kmh();
            let mph = t.speed_mph();
            prop_assert!((kmh - speed * 3.6).abs() < 0.01);
            prop_assert!((mph - speed * 2.237).abs() < 0.01);
        }

        #[test]
        fn game_telemetry_total_g_nonnegative(lat in -10.0f32..10.0, lon in -10.0f32..10.0) {
            let t = GameTelemetry {
                lateral_g: lat,
                longitudinal_g: lon,
                ..Default::default()
            };
            prop_assert!(t.total_g() >= 0.0);
        }

        #[test]
        fn game_telemetry_to_normalized_slip_ratio_bounded(
            fl in -10.0f32..10.0,
            fr in -10.0f32..10.0,
            rl in -10.0f32..10.0,
            rr in -10.0f32..10.0,
        ) {
            let t = GameTelemetry {
                slip_angle_fl: fl,
                slip_angle_fr: fr,
                slip_angle_rl: rl,
                slip_angle_rr: rr,
                ..Default::default()
            };
            let n = t.to_normalized();
            prop_assert!(n.slip_ratio >= 0.0, "slip_ratio must be >= 0");
            prop_assert!(n.slip_ratio <= 1.0, "slip_ratio must be <= 1.0");
        }

        #[test]
        fn normalized_telemetry_json_roundtrip_property(
            rpm in 0.0f32..20000.0,
            speed in 0.0f32..500.0,
            gear in -1i8..8,
        ) {
            let original = NormalizedTelemetry::builder()
                .rpm(rpm)
                .speed_ms(speed)
                .gear(gear)
                .build();

            let json = serde_json::to_string(&original).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
            let deser: NormalizedTelemetry = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

            prop_assert_eq!(deser.rpm, original.rpm);
            prop_assert_eq!(deser.speed_ms, original.speed_ms);
            prop_assert_eq!(deser.gear, original.gear);
        }

        #[test]
        fn telemetry_frame_sequence_preserved(seq in 0u64..u64::MAX) {
            let data = NormalizedTelemetry::default();
            let frame = TelemetryFrame::new(data, 0, seq, 64);
            prop_assert_eq!(frame.sequence, seq);
        }

        #[test]
        fn telemetry_now_ns_never_decreases(iterations in 2u32..50) {
            let mut prev = telemetry_now_ns();
            for _ in 0..iterations {
                let curr = telemetry_now_ns();
                prop_assert!(curr >= prev, "monotonicity violation: {prev} > {curr}");
                prev = curr;
            }
        }
    }
}
