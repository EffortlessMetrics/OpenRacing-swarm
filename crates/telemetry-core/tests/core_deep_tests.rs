//! Deep tests for the telemetry-core crate.
//!
//! Covers core telemetry types, event processing, event routing
//! (connection state transitions), and error handling.

use openracing_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
    GameTelemetry, GameTelemetrySnapshot, NormalizedTelemetry, RateLimiter, RateLimiterStats,
    TelemetryError,
};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Core telemetry types
// ---------------------------------------------------------------------------

#[test]
fn game_telemetry_default_is_zeroed() -> TestResult {
    let t = GameTelemetry::default();
    assert_eq!(t.speed_mps, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.slip_angle_fl, 0.0);
    assert_eq!(t.slip_angle_fr, 0.0);
    assert_eq!(t.slip_angle_rl, 0.0);
    assert_eq!(t.slip_angle_rr, 0.0);
    Ok(())
}

#[test]
fn game_telemetry_speed_conversions() -> TestResult {
    let t = GameTelemetry {
        speed_mps: 10.0,
        ..Default::default()
    };
    assert!((t.speed_kmh() - 36.0).abs() < 0.1);
    assert!((t.speed_mph() - 22.37).abs() < 0.1);
    Ok(())
}

#[test]
fn game_telemetry_stationary_threshold() -> TestResult {
    assert!(
        GameTelemetry {
            speed_mps: 0.0,
            ..Default::default()
        }
        .is_stationary()
    );
    assert!(
        GameTelemetry {
            speed_mps: 0.49,
            ..Default::default()
        }
        .is_stationary()
    );
    assert!(
        !GameTelemetry {
            speed_mps: 0.5,
            ..Default::default()
        }
        .is_stationary()
    );
    assert!(
        !GameTelemetry {
            speed_mps: 100.0,
            ..Default::default()
        }
        .is_stationary()
    );
    Ok(())
}

#[test]
fn game_telemetry_total_g_pythagorean() -> TestResult {
    let t = GameTelemetry {
        lateral_g: 3.0,
        longitudinal_g: 4.0,
        ..Default::default()
    };
    assert!((t.total_g() - 5.0).abs() < 0.001);

    let t0 = GameTelemetry::default();
    assert_eq!(t0.total_g(), 0.0);
    Ok(())
}

#[test]
fn game_telemetry_slip_angle_computations() -> TestResult {
    let t = GameTelemetry {
        slip_angle_fl: 0.1,
        slip_angle_fr: 0.2,
        slip_angle_rl: 0.3,
        slip_angle_rr: 0.4,
        ..Default::default()
    };
    assert!((t.average_slip_angle() - 0.25).abs() < 0.001);
    assert!((t.front_slip_angle() - 0.15).abs() < 0.001);
    assert!((t.rear_slip_angle() - 0.35).abs() < 0.001);
    Ok(())
}

#[test]
fn game_telemetry_to_normalized_preserves_fields() -> TestResult {
    let t = GameTelemetry {
        speed_mps: 50.0,
        rpm: 7000.0,
        gear: 5,
        throttle: 0.9,
        brake: 0.05,
        steering_angle: -0.3,
        lateral_g: 2.0,
        longitudinal_g: 1.0,
        slip_angle_fl: 0.01,
        slip_angle_fr: 0.02,
        slip_angle_rl: 0.03,
        slip_angle_rr: 0.04,
        ..Default::default()
    };

    let n = t.to_normalized();
    assert_eq!(n.speed_ms, 50.0);
    assert_eq!(n.rpm, 7000.0);
    assert_eq!(n.gear, 5);
    assert_eq!(n.throttle, 0.9);
    assert_eq!(n.brake, 0.05);
    assert_eq!(n.steering_angle, -0.3);
    assert_eq!(n.lateral_g, 2.0);
    assert_eq!(n.longitudinal_g, 1.0);
    Ok(())
}

#[test]
fn game_telemetry_into_normalized_trait() -> TestResult {
    let t = GameTelemetry {
        speed_mps: 30.0,
        rpm: 4000.0,
        gear: 3,
        ..Default::default()
    };
    let n: NormalizedTelemetry = t.into();
    assert_eq!(n.speed_ms, 30.0);
    assert_eq!(n.rpm, 4000.0);
    assert_eq!(n.gear, 3);
    Ok(())
}

// ---------------------------------------------------------------------------
// GameTelemetrySnapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_preserves_all_fields() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry {
        speed_mps: 40.0,
        rpm: 5000.0,
        gear: 4,
        throttle: 0.7,
        brake: 0.2,
        lateral_g: 1.0,
        longitudinal_g: -0.5,
        slip_angle_fl: 0.01,
        slip_angle_fr: 0.02,
        slip_angle_rl: 0.03,
        slip_angle_rr: 0.04,
        ..Default::default()
    };

    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    assert_eq!(snap.speed_mps, 40.0);
    assert_eq!(snap.rpm, 5000.0);
    assert_eq!(snap.gear, 4);
    assert_eq!(snap.throttle, 0.7);
    assert_eq!(snap.brake, 0.2);
    assert_eq!(snap.lateral_g, 1.0);
    assert_eq!(snap.longitudinal_g, -0.5);
    Ok(())
}

#[test]
fn snapshot_json_round_trip() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry {
        speed_mps: 25.0,
        rpm: 3500.0,
        gear: 2,
        ..Default::default()
    };

    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    let json = serde_json::to_string(&snap)?;
    let restored: GameTelemetrySnapshot = serde_json::from_str(&json)?;

    assert_eq!(restored.speed_mps, snap.speed_mps);
    assert_eq!(restored.rpm, snap.rpm);
    assert_eq!(restored.gear, snap.gear);
    Ok(())
}

// ---------------------------------------------------------------------------
// Telemetry event processing — ConnectionState
// ---------------------------------------------------------------------------

#[test]
fn connection_state_queries() -> TestResult {
    assert!(ConnectionState::Connected.is_connected());
    assert!(!ConnectionState::Connected.is_disconnected());
    assert!(!ConnectionState::Connected.is_transitioning());

    assert!(!ConnectionState::Disconnected.is_connected());
    assert!(ConnectionState::Disconnected.is_disconnected());
    assert!(!ConnectionState::Disconnected.is_transitioning());

    assert!(ConnectionState::Connecting.is_transitioning());
    assert!(ConnectionState::Reconnecting.is_transitioning());

    assert!(ConnectionState::Error.is_disconnected());
    assert!(!ConnectionState::Error.is_connected());
    Ok(())
}

#[test]
fn connection_state_default_is_disconnected() -> TestResult {
    assert_eq!(ConnectionState::default(), ConnectionState::Disconnected);
    Ok(())
}

// ---------------------------------------------------------------------------
// Event routing — ConnectionStateEvent
// ---------------------------------------------------------------------------

#[test]
fn connection_state_event_is_connection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "iracing",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        Some("Data received".into()),
    );
    assert!(event.is_connection());
    assert!(!event.is_disconnection());
    assert_eq!(event.game_id, "iracing");
    assert!(event.timestamp_ns > 0);
    Ok(())
}

#[test]
fn connection_state_event_is_disconnection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "acc",
        ConnectionState::Connected,
        ConnectionState::Disconnected,
        None,
    );
    assert!(event.is_disconnection());
    assert!(!event.is_connection());
    Ok(())
}

#[test]
fn connection_state_event_transition_without_change() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test",
        ConnectionState::Connecting,
        ConnectionState::Connecting,
        None,
    );
    assert!(!event.is_connection());
    assert!(!event.is_disconnection());
    Ok(())
}

// ---------------------------------------------------------------------------
// DisconnectionConfig
// ---------------------------------------------------------------------------

#[test]
fn disconnection_config_defaults() -> TestResult {
    let config = DisconnectionConfig::default();
    assert_eq!(config.timeout_ms, 2000);
    assert!(config.auto_reconnect);
    assert_eq!(config.max_reconnect_attempts, 0);
    assert_eq!(config.reconnect_delay_ms, 1000);
    assert_eq!(config.timeout(), Duration::from_millis(2000));
    assert_eq!(config.reconnect_delay(), Duration::from_millis(1000));
    Ok(())
}

#[test]
fn disconnection_config_with_timeout() -> TestResult {
    let config = DisconnectionConfig::with_timeout(5000);
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.timeout(), Duration::from_millis(5000));
    assert!(config.auto_reconnect);
    Ok(())
}

// ---------------------------------------------------------------------------
// DisconnectionTracker — state transitions and reconnection
// ---------------------------------------------------------------------------

#[test]
fn tracker_initial_state_is_disconnected() -> TestResult {
    let tracker = DisconnectionTracker::with_defaults("iracing");
    assert_eq!(tracker.state(), ConnectionState::Disconnected);
    assert_eq!(tracker.reconnect_attempts(), 0);
    assert!(tracker.time_since_last_data().is_none());
    Ok(())
}

#[test]
fn tracker_data_received_transitions_to_connected() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("acc");
    tracker.record_data_received();
    assert_eq!(tracker.state(), ConnectionState::Connected);
    assert!(tracker.time_since_last_data().is_some());
    Ok(())
}

#[test]
fn tracker_mark_connecting() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    tracker.mark_connecting();
    assert_eq!(tracker.state(), ConnectionState::Connecting);
    Ok(())
}

#[test]
fn tracker_mark_reconnecting_increments_attempts() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    assert_eq!(tracker.reconnect_attempts(), 0);

    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 1);
    assert_eq!(tracker.state(), ConnectionState::Reconnecting);

    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 2);
    Ok(())
}

#[test]
fn tracker_mark_error() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    tracker.mark_error("something broke".into());
    assert_eq!(tracker.state(), ConnectionState::Error);
    Ok(())
}

#[test]
fn tracker_reset_reconnect_attempts() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    tracker.mark_reconnecting();
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 2);

    tracker.reset_reconnect_attempts();
    assert_eq!(tracker.reconnect_attempts(), 0);
    Ok(())
}

#[test]
fn tracker_should_reconnect_respects_config() -> TestResult {
    // auto_reconnect = true, unlimited attempts
    let mut tracker = DisconnectionTracker::with_defaults("test");
    tracker.mark_error("err".into());
    assert!(tracker.should_reconnect());

    // auto_reconnect = false
    let config = DisconnectionConfig {
        auto_reconnect: false,
        ..Default::default()
    };
    let mut tracker2 = DisconnectionTracker::new("test", config);
    tracker2.mark_error("err".into());
    assert!(!tracker2.should_reconnect());
    Ok(())
}

#[test]
fn tracker_max_reconnect_attempts_enforced() -> TestResult {
    let config = DisconnectionConfig {
        auto_reconnect: true,
        max_reconnect_attempts: 2,
        ..Default::default()
    };
    let mut tracker = DisconnectionTracker::new("test", config);

    tracker.mark_reconnecting(); // attempt 1
    tracker.mark_reconnecting(); // attempt 2
    // Now at max attempts, transition to error
    tracker.mark_error("failed".into());
    assert!(!tracker.should_reconnect());
    Ok(())
}

#[test]
fn tracker_subscribe_receives_state_changes() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    let mut rx = tracker.subscribe();

    tracker.mark_connecting();
    let event = rx.try_recv()?;
    assert_eq!(event.new_state, ConnectionState::Connecting);
    assert_eq!(event.previous_state, ConnectionState::Disconnected);
    assert_eq!(event.game_id, "test");
    Ok(())
}

#[test]
fn tracker_no_event_on_same_state_transition() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    let mut rx = tracker.subscribe();

    // Already disconnected, setting to disconnected should not fire
    tracker.set_state(ConnectionState::Disconnected, None);
    assert!(rx.try_recv().is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// Error handling — TelemetryError
// ---------------------------------------------------------------------------

#[test]
fn telemetry_error_display_messages() -> TestResult {
    let errors: Vec<(TelemetryError, &str)> = vec![
        (
            TelemetryError::ConnectionFailed("timeout".into()),
            "timeout",
        ),
        (
            TelemetryError::GameNotRunning {
                game_id: "acc".into(),
            },
            "acc",
        ),
        (TelemetryError::ParseError("bad data".into()), "bad data"),
        (
            TelemetryError::SharedMemoryError("access denied".into()),
            "access denied",
        ),
        (TelemetryError::NetworkError("refused".into()), "refused"),
        (TelemetryError::AlreadyConnected, "already"),
        (TelemetryError::NotConnected, "not connected"),
        (TelemetryError::Timeout { timeout_ms: 5000 }, "5000"),
        (
            TelemetryError::InvalidData {
                reason: "corrupt".into(),
            },
            "corrupt",
        ),
    ];

    for (err, expected_substr) in &errors {
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains(&expected_substr.to_lowercase()),
            "error message '{}' should contain '{}'",
            msg,
            expected_substr
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

#[test]
fn rate_limiter_first_call_always_processes() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 0);
    Ok(())
}

#[test]
fn rate_limiter_second_call_drops_if_too_fast() -> TestResult {
    let mut limiter = RateLimiter::new(10); // 10 Hz = 100ms interval
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
    Ok(())
}

#[test]
fn rate_limiter_drop_rate_percent() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    assert!(!limiter.should_process());

    // 1 processed, 2 dropped -> 66.67%
    let rate = limiter.drop_rate_percent();
    assert!((rate - 66.666).abs() < 1.0, "expected ~66.67%, got {rate}");
    Ok(())
}

#[test]
fn rate_limiter_reset_stats() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());

    limiter.reset_stats();
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
    assert_eq!(limiter.drop_rate_percent(), 0.0);
    Ok(())
}

#[test]
fn rate_limiter_set_max_rate_hz() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    assert_eq!(limiter.max_rate_hz(), 100);

    limiter.set_max_rate_hz(200);
    assert_eq!(limiter.max_rate_hz(), 200);

    // Zero is clamped to 1
    limiter.set_max_rate_hz(0);
    assert_eq!(limiter.max_rate_hz(), 1);
    Ok(())
}

#[test]
fn rate_limiter_stats_snapshot() -> TestResult {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());

    let stats = RateLimiterStats::from(&limiter);
    assert_eq!(stats.max_rate_hz, 60);
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 1);
    assert!((stats.drop_rate_percent - 50.0).abs() < f32::EPSILON);
    Ok(())
}

// ---------------------------------------------------------------------------
// telemetry_now_ns utility
// ---------------------------------------------------------------------------

#[test]
fn telemetry_now_ns_is_monotonic() -> TestResult {
    let t1 = openracing_telemetry::telemetry_now_ns();
    std::thread::sleep(Duration::from_millis(1));
    let t2 = openracing_telemetry::telemetry_now_ns();
    assert!(t2 >= t1, "timestamps should be monotonically increasing");
    Ok(())
}
