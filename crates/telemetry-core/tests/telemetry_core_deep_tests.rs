//! Deep tests for telemetry-core: core types, normalization, data flow,
//! serialization roundtrips, error handling, and thread safety.

use openracing_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
    GameTelemetry, GameTelemetrySnapshot, NormalizedTelemetry, TelemetryError, TelemetryFlags,
    TelemetryFrame, TelemetryValue,
    contracts::{FlagCoverage, TelemetryFieldCoverage},
};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// GameTelemetry — edge cases and normalization
// ===========================================================================

#[test]
fn game_telemetry_negative_speed_normalizes_in_to_normalized() -> TestResult {
    let t = GameTelemetry {
        speed_mps: -10.0,
        ..Default::default()
    };
    let n = t.to_normalized();
    // Builder clamps negative speed to 0
    assert_eq!(n.speed_ms, 0.0);
    Ok(())
}

#[test]
fn game_telemetry_extreme_values_do_not_panic() -> TestResult {
    let t = GameTelemetry {
        speed_mps: f32::MAX,
        rpm: f32::MAX,
        gear: i8::MAX,
        throttle: f32::MAX,
        brake: f32::MAX,
        steering_angle: f32::MAX,
        lateral_g: f32::MAX,
        longitudinal_g: f32::MAX,
        slip_angle_fl: f32::MAX,
        slip_angle_fr: f32::MAX,
        slip_angle_rl: f32::MAX,
        slip_angle_rr: f32::MAX,
        ..Default::default()
    };
    // Ensure conversions don't panic
    let _kmh = t.speed_kmh();
    let _mph = t.speed_mph();
    let _avg = t.average_slip_angle();
    let _front = t.front_slip_angle();
    let _rear = t.rear_slip_angle();
    let _g = t.total_g();
    let _stat = t.is_stationary();
    let _n = t.to_normalized();
    Ok(())
}

#[test]
fn game_telemetry_nan_speed_conversions() -> TestResult {
    let t = GameTelemetry {
        speed_mps: f32::NAN,
        ..Default::default()
    };
    assert!(t.speed_kmh().is_nan());
    assert!(t.speed_mph().is_nan());
    // NAN is not < 0.5, so is_stationary returns false
    assert!(!t.is_stationary());
    Ok(())
}

#[test]
fn game_telemetry_negative_slip_angles() -> TestResult {
    let t = GameTelemetry {
        slip_angle_fl: -0.1,
        slip_angle_fr: -0.2,
        slip_angle_rl: 0.3,
        slip_angle_rr: 0.4,
        ..Default::default()
    };
    let avg = t.average_slip_angle();
    assert!((avg - 0.1).abs() < 0.001);
    assert!((t.front_slip_angle() - (-0.15)).abs() < 0.001);
    assert!((t.rear_slip_angle() - 0.35).abs() < 0.001);
    Ok(())
}

#[test]
fn game_telemetry_negative_g_forces_total_g() -> TestResult {
    let t = GameTelemetry {
        lateral_g: -3.0,
        longitudinal_g: -4.0,
        ..Default::default()
    };
    // sqrt(9 + 16) = 5.0 regardless of sign
    assert!((t.total_g() - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn game_telemetry_with_timestamp_uses_provided_instant() -> TestResult {
    let ts = Instant::now();
    let t = GameTelemetry::with_timestamp(ts);
    assert_eq!(t.speed_mps, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn game_telemetry_clone_is_independent() -> TestResult {
    let t1 = GameTelemetry {
        speed_mps: 50.0,
        rpm: 6000.0,
        ..Default::default()
    };
    let t2 = t1.clone();
    assert_eq!(t1.speed_mps, t2.speed_mps);
    assert_eq!(t1.rpm, t2.rpm);
    Ok(())
}

#[test]
fn game_telemetry_debug_format() -> TestResult {
    let t = GameTelemetry::default();
    let debug = format!("{t:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ===========================================================================
// GameTelemetrySnapshot — timestamp edge cases
// ===========================================================================

#[test]
fn snapshot_epoch_same_as_timestamp_yields_zero_ns() -> TestResult {
    let ts = Instant::now();
    let t = GameTelemetry::with_timestamp(ts);
    let snap = GameTelemetrySnapshot::from_telemetry(&t, ts);
    assert_eq!(snap.timestamp_ns, 0);
    Ok(())
}

#[test]
fn snapshot_epoch_after_timestamp_yields_zero_ns() -> TestResult {
    let t = GameTelemetry::with_timestamp(Instant::now());
    // Epoch after telemetry timestamp — saturating_duration_since produces 0
    std::thread::sleep(Duration::from_millis(1));
    let epoch = Instant::now();
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    assert_eq!(snap.timestamp_ns, 0);
    Ok(())
}

#[test]
fn snapshot_all_fields_preserved_including_slip_angles() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry {
        steering_angle: -1.5,
        slip_angle_fl: 0.11,
        slip_angle_fr: 0.22,
        slip_angle_rl: 0.33,
        slip_angle_rr: 0.44,
        ..Default::default()
    };
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    assert_eq!(snap.steering_angle, -1.5);
    assert_eq!(snap.slip_angle_fl, 0.11);
    assert_eq!(snap.slip_angle_fr, 0.22);
    assert_eq!(snap.slip_angle_rl, 0.33);
    assert_eq!(snap.slip_angle_rr, 0.44);
    Ok(())
}

#[test]
fn snapshot_serde_roundtrip_preserves_all_fields() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry {
        speed_mps: 42.0,
        rpm: 7777.0,
        gear: 5,
        throttle: 0.95,
        brake: 0.05,
        steering_angle: 0.12,
        lateral_g: 1.1,
        longitudinal_g: -0.3,
        slip_angle_fl: 0.01,
        slip_angle_fr: 0.02,
        slip_angle_rl: 0.03,
        slip_angle_rr: 0.04,
        ..Default::default()
    };
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    let json = serde_json::to_string(&snap)?;
    let restored: GameTelemetrySnapshot = serde_json::from_str(&json)?;
    assert_eq!(restored.speed_mps, snap.speed_mps);
    assert_eq!(restored.rpm, snap.rpm);
    assert_eq!(restored.gear, snap.gear);
    assert_eq!(restored.throttle, snap.throttle);
    assert_eq!(restored.brake, snap.brake);
    assert_eq!(restored.steering_angle, snap.steering_angle);
    assert_eq!(restored.lateral_g, snap.lateral_g);
    assert_eq!(restored.longitudinal_g, snap.longitudinal_g);
    assert_eq!(restored.slip_angle_fl, snap.slip_angle_fl);
    assert_eq!(restored.slip_angle_fr, snap.slip_angle_fr);
    assert_eq!(restored.slip_angle_rl, snap.slip_angle_rl);
    assert_eq!(restored.slip_angle_rr, snap.slip_angle_rr);
    assert_eq!(restored.timestamp_ns, snap.timestamp_ns);
    Ok(())
}

#[test]
fn snapshot_clone_and_debug() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry::default();
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    let cloned = snap.clone();
    assert_eq!(cloned.speed_mps, snap.speed_mps);
    let debug = format!("{snap:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ===========================================================================
// NormalizedTelemetry — normalization and data flow
// ===========================================================================

#[test]
fn normalized_telemetry_slip_ratio_clamped_from_game_telemetry() -> TestResult {
    let t = GameTelemetry {
        slip_angle_fl: 5.0,
        slip_angle_fr: 5.0,
        slip_angle_rl: 5.0,
        slip_angle_rr: 5.0,
        ..Default::default()
    };
    let n = t.to_normalized();
    // avg = 5.0, abs(5.0).min(1.0) = 1.0
    assert_eq!(n.slip_ratio, 1.0);
    Ok(())
}

#[test]
fn normalized_builder_multiple_extended_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("turbo_psi", TelemetryValue::Float(14.7))
        .extended("sector", TelemetryValue::Integer(2))
        .extended("headlights", TelemetryValue::Boolean(true))
        .extended("driver", TelemetryValue::String("Hamilton".into()))
        .build();

    assert_eq!(t.extended.len(), 4);
    assert_eq!(
        t.get_extended("turbo_psi"),
        Some(&TelemetryValue::Float(14.7))
    );
    assert_eq!(t.get_extended("sector"), Some(&TelemetryValue::Integer(2)));
    assert_eq!(
        t.get_extended("headlights"),
        Some(&TelemetryValue::Boolean(true))
    );
    assert_eq!(
        t.get_extended("driver"),
        Some(&TelemetryValue::String("Hamilton".into()))
    );
    Ok(())
}

#[test]
fn normalized_with_extended_method() -> TestResult {
    let t = NormalizedTelemetry::default()
        .with_extended("a", TelemetryValue::Integer(1))
        .with_extended("b", TelemetryValue::Integer(2));

    assert_eq!(t.extended.len(), 2);
    assert_eq!(t.get_extended("a"), Some(&TelemetryValue::Integer(1)));
    assert_eq!(t.get_extended("b"), Some(&TelemetryValue::Integer(2)));
    Ok(())
}

#[test]
fn normalized_validated_clamps_all_ranges() -> TestResult {
    let t = NormalizedTelemetry {
        speed_ms: -10.0,
        throttle: 2.0,
        brake: -1.0,
        clutch: 5.0,
        ffb_scalar: 3.0,
        fuel_percent: -0.5,
        slip_ratio: 1.5,
        ..NormalizedTelemetry::default()
    };

    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.throttle, 1.0);
    assert_eq!(v.brake, 0.0);
    assert_eq!(v.clutch, 1.0);
    assert_eq!(v.ffb_scalar, 1.0);
    assert_eq!(v.fuel_percent, 0.0);
    assert_eq!(v.slip_ratio, 1.0);
    Ok(())
}

#[test]
fn normalized_serde_roundtrip_with_all_fields() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        pit_limiter: true,
        drs_active: true,
        abs_active: true,
        safety_car: true,
        ..Default::default()
    };

    let t = NormalizedTelemetry::builder()
        .speed_ms(55.5)
        .rpm(8000.0)
        .max_rpm(9500.0)
        .gear(6)
        .num_gears(7)
        .throttle(1.0)
        .brake(0.0)
        .clutch(0.0)
        .steering_angle(-0.45)
        .lateral_g(2.5)
        .longitudinal_g(-1.2)
        .vertical_g(0.05)
        .slip_ratio(0.3)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .ffb_scalar(0.85)
        .ffb_torque_nm(15.0)
        .car_id("mclaren_720s")
        .track_id("monza")
        .session_id("qual_1")
        .position(5)
        .lap(7)
        .current_lap_time_s(82.3)
        .best_lap_time_s(80.1)
        .last_lap_time_s(81.5)
        .delta_ahead_s(-0.7)
        .delta_behind_s(1.2)
        .fuel_percent(0.42)
        .engine_temp_c(98.5)
        .tire_temps_c([95, 92, 88, 90])
        .tire_pressures_psi([27.0, 27.5, 26.0, 26.5])
        .sequence(42)
        .flags(flags)
        .extended("boost", TelemetryValue::Float(1.4))
        .build();

    let json = serde_json::to_string(&t)?;
    let d: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(d.speed_ms, 55.5);
    assert_eq!(d.rpm, 8000.0);
    assert_eq!(d.max_rpm, 9500.0);
    assert_eq!(d.gear, 6);
    assert_eq!(d.num_gears, 7);
    assert_eq!(d.throttle, 1.0);
    assert_eq!(d.brake, 0.0);
    assert_eq!(d.clutch, 0.0);
    assert_eq!(d.steering_angle, -0.45);
    assert_eq!(d.lateral_g, 2.5);
    assert_eq!(d.longitudinal_g, -1.2);
    assert_eq!(d.vertical_g, 0.05);
    assert_eq!(d.slip_ratio, 0.3);
    assert_eq!(d.ffb_scalar, 0.85);
    assert_eq!(d.ffb_torque_nm, 15.0);
    assert_eq!(d.car_id.as_deref(), Some("mclaren_720s"));
    assert_eq!(d.track_id.as_deref(), Some("monza"));
    assert_eq!(d.session_id.as_deref(), Some("qual_1"));
    assert_eq!(d.position, 5);
    assert_eq!(d.lap, 7);
    assert_eq!(d.current_lap_time_s, 82.3);
    assert_eq!(d.best_lap_time_s, 80.1);
    assert_eq!(d.last_lap_time_s, 81.5);
    assert_eq!(d.fuel_percent, 0.42);
    assert_eq!(d.engine_temp_c, 98.5);
    assert_eq!(d.tire_temps_c, [95, 92, 88, 90]);
    assert_eq!(d.tire_pressures_psi, [27.0, 27.5, 26.0, 26.5]);
    assert_eq!(d.sequence, 42);
    assert!(d.flags.yellow_flag);
    assert!(d.flags.pit_limiter);
    assert!(d.flags.drs_active);
    assert!(d.flags.abs_active);
    assert!(d.flags.safety_car);
    assert_eq!(d.get_extended("boost"), Some(&TelemetryValue::Float(1.4)));
    Ok(())
}

#[test]
fn normalized_has_ffb_data_both_paths() -> TestResult {
    let no_ffb = NormalizedTelemetry::default();
    assert!(!no_ffb.has_ffb_data());

    let negative_scalar = NormalizedTelemetry::builder().ffb_scalar(-0.5).build();
    assert!(negative_scalar.has_ffb_data());

    let negative_torque = NormalizedTelemetry::builder().ffb_torque_nm(-5.0).build();
    assert!(negative_torque.has_ffb_data());
    Ok(())
}

#[test]
fn normalized_rpm_fraction_edge_cases() -> TestResult {
    // rpm > max_rpm → clamped to 1.0
    let over = NormalizedTelemetry::builder()
        .rpm(10000.0)
        .max_rpm(8000.0)
        .build();
    assert_eq!(over.rpm_fraction(), 1.0);

    // Zero rpm
    let zero = NormalizedTelemetry::builder()
        .rpm(0.0)
        .max_rpm(8000.0)
        .build();
    assert_eq!(zero.rpm_fraction(), 0.0);

    // Both zero
    let both_zero = NormalizedTelemetry::default();
    assert_eq!(both_zero.rpm_fraction(), 0.0);
    Ok(())
}

// ===========================================================================
// TelemetryFrame — construction and data flow
// ===========================================================================

#[test]
fn telemetry_frame_zero_values() -> TestResult {
    let data = NormalizedTelemetry::default();
    let frame = TelemetryFrame::new(data, 0, 0, 0);
    assert_eq!(frame.timestamp_ns, 0);
    assert_eq!(frame.sequence, 0);
    assert_eq!(frame.raw_size, 0);
    assert_eq!(frame.data.speed_ms, 0.0);
    Ok(())
}

#[test]
fn telemetry_frame_from_telemetry_increments_sequence() -> TestResult {
    let data1 = NormalizedTelemetry::builder().speed_ms(10.0).build();
    let data2 = NormalizedTelemetry::builder().speed_ms(20.0).build();
    let f1 = TelemetryFrame::from_telemetry(data1, 1, 100);
    let f2 = TelemetryFrame::from_telemetry(data2, 2, 200);
    assert_eq!(f1.sequence, 1);
    assert_eq!(f2.sequence, 2);
    assert!(f2.timestamp_ns >= f1.timestamp_ns);
    Ok(())
}

#[test]
fn telemetry_frame_large_raw_size() -> TestResult {
    let data = NormalizedTelemetry::default();
    let frame = TelemetryFrame::new(data, 1, 1, usize::MAX);
    assert_eq!(frame.raw_size, usize::MAX);
    Ok(())
}

// ===========================================================================
// ConnectionState — exhaustive state queries
// ===========================================================================

#[test]
fn connection_state_all_variants_exclusive() -> TestResult {
    let states = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
        ConnectionState::Error,
    ];

    for state in &states {
        // At most one category can be true (connected, disconnected, transitioning)
        let categories = [
            state.is_connected(),
            state.is_disconnected(),
            state.is_transitioning(),
        ];
        let count = categories.iter().filter(|&&v| v).count();
        assert!(
            count == 1,
            "state {:?} should be in exactly one category, got {count}",
            state
        );
    }
    Ok(())
}

#[test]
fn connection_state_serde_all_variants() -> TestResult {
    let states = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
        ConnectionState::Error,
    ];
    for state in &states {
        let json = serde_json::to_string(state)?;
        let decoded: ConnectionState = serde_json::from_str(&json)?;
        assert_eq!(&decoded, state);
    }
    Ok(())
}

#[test]
fn connection_state_copy_and_eq() -> TestResult {
    let a = ConnectionState::Connected;
    let b = a; // Copy
    assert_eq!(a, b);
    Ok(())
}

// ===========================================================================
// ConnectionStateEvent — serialization and edge cases
// ===========================================================================

#[test]
fn connection_event_serde_roundtrip() -> TestResult {
    let event = ConnectionStateEvent::new(
        "iracing",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        Some("Shared memory opened".into()),
    );
    let json = serde_json::to_string(&event)?;
    let decoded: ConnectionStateEvent = serde_json::from_str(&json)?;
    assert_eq!(decoded.game_id, "iracing");
    assert_eq!(decoded.previous_state, ConnectionState::Disconnected);
    assert_eq!(decoded.new_state, ConnectionState::Connected);
    assert_eq!(decoded.reason.as_deref(), Some("Shared memory opened"));
    assert!(decoded.timestamp_ns > 0);
    Ok(())
}

#[test]
fn connection_event_none_reason() -> TestResult {
    let event = ConnectionStateEvent::new(
        "acc",
        ConnectionState::Connecting,
        ConnectionState::Error,
        None,
    );
    assert!(event.reason.is_none());
    assert!(!event.is_connection());
    assert!(!event.is_disconnection());
    Ok(())
}

#[test]
fn connection_event_reconnecting_to_connected_is_connection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test",
        ConnectionState::Reconnecting,
        ConnectionState::Connected,
        None,
    );
    assert!(event.is_connection());
    assert!(!event.is_disconnection());
    Ok(())
}

#[test]
fn connection_event_connected_to_reconnecting_not_disconnection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "test",
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
        None,
    );
    // Reconnecting is transitioning, not disconnected
    assert!(!event.is_disconnection());
    assert!(!event.is_connection());
    Ok(())
}

#[test]
fn connection_event_clone_preserves_fields() -> TestResult {
    let event = ConnectionStateEvent::new(
        "forza",
        ConnectionState::Connected,
        ConnectionState::Error,
        Some("UDP timeout".into()),
    );
    let cloned = event.clone();
    assert_eq!(cloned.game_id, event.game_id);
    assert_eq!(cloned.previous_state, event.previous_state);
    assert_eq!(cloned.new_state, event.new_state);
    assert_eq!(cloned.reason, event.reason);
    assert_eq!(cloned.timestamp_ns, event.timestamp_ns);
    Ok(())
}

// ===========================================================================
// DisconnectionConfig — serialization and edge cases
// ===========================================================================

#[test]
fn disconnection_config_serde_roundtrip() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 5000,
        auto_reconnect: false,
        max_reconnect_attempts: 10,
        reconnect_delay_ms: 3000,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: DisconnectionConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.timeout_ms, 5000);
    assert!(!decoded.auto_reconnect);
    assert_eq!(decoded.max_reconnect_attempts, 10);
    assert_eq!(decoded.reconnect_delay_ms, 3000);
    Ok(())
}

#[test]
fn disconnection_config_zero_timeout() -> TestResult {
    let config = DisconnectionConfig::with_timeout(0);
    assert_eq!(config.timeout(), Duration::from_millis(0));
    Ok(())
}

#[test]
fn disconnection_config_large_timeout() -> TestResult {
    let config = DisconnectionConfig::with_timeout(u64::MAX);
    assert_eq!(config.timeout_ms, u64::MAX);
    Ok(())
}

#[test]
fn disconnection_config_clone_and_debug() -> TestResult {
    let config = DisconnectionConfig::default();
    let cloned = config.clone();
    assert_eq!(cloned.timeout_ms, config.timeout_ms);
    let debug = format!("{config:?}");
    assert!(debug.contains("timeout_ms"));
    Ok(())
}

// ===========================================================================
// DisconnectionTracker — complex state machine flows
// ===========================================================================

#[test]
fn tracker_full_lifecycle_connect_disconnect_reconnect() -> TestResult {
    let config = DisconnectionConfig {
        auto_reconnect: true,
        max_reconnect_attempts: 3,
        ..Default::default()
    };
    let mut tracker = DisconnectionTracker::new("iracing", config);
    let mut rx = tracker.subscribe();

    // Start connecting
    tracker.mark_connecting();
    assert_eq!(tracker.state(), ConnectionState::Connecting);
    let event = rx.try_recv()?;
    assert_eq!(event.new_state, ConnectionState::Connecting);

    // Receive data → connected
    tracker.record_data_received();
    assert_eq!(tracker.state(), ConnectionState::Connected);
    assert_eq!(tracker.reconnect_attempts(), 0);
    let event = rx.try_recv()?;
    assert_eq!(event.new_state, ConnectionState::Connected);

    // Mark error → disconnected-like
    tracker.mark_error("connection lost".into());
    assert_eq!(tracker.state(), ConnectionState::Error);
    assert!(tracker.should_reconnect());

    // Reconnect attempts
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 1);
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 2);
    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 3);

    // At max attempts, transition to error
    tracker.mark_error("failed".into());
    assert!(!tracker.should_reconnect());
    Ok(())
}

#[test]
fn tracker_set_state_sender_replaces_previous() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    let mut rx1 = tracker.subscribe();

    // Replace with new sender
    let (tx2, mut rx2) = tokio::sync::mpsc::channel(16);
    tracker.set_state_sender(tx2);

    tracker.mark_connecting();
    // Old receiver should NOT get the event (sender was replaced)
    assert!(rx1.try_recv().is_err());
    // New receiver SHOULD get it
    let event = rx2.try_recv()?;
    assert_eq!(event.new_state, ConnectionState::Connecting);
    Ok(())
}

#[test]
fn tracker_check_disconnection_only_from_connected() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");

    // Not connected → check_disconnection should be no-op
    let state = tracker.check_disconnection();
    assert_eq!(state, ConnectionState::Disconnected);

    // Connecting → check_disconnection is no-op
    tracker.mark_connecting();
    let state = tracker.check_disconnection();
    assert_eq!(state, ConnectionState::Connecting);

    // Error → check_disconnection is no-op
    tracker.mark_error("err".into());
    let state = tracker.check_disconnection();
    assert_eq!(state, ConnectionState::Error);
    Ok(())
}

#[test]
fn tracker_time_since_last_data_increases() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    tracker.record_data_received();

    let d1 = tracker.time_since_last_data();
    assert!(d1.is_some());
    std::thread::sleep(Duration::from_millis(5));
    let d2 = tracker.time_since_last_data();
    assert!(d2.is_some());

    // d2 should be >= d1
    let d1_val = d1.ok_or("expected Some")?;
    let d2_val = d2.ok_or("expected Some")?;
    assert!(d2_val >= d1_val);
    Ok(())
}

#[test]
fn tracker_should_reconnect_error_state_auto_enabled() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    // Disconnected state with auto_reconnect = true → should reconnect
    assert!(tracker.should_reconnect());

    // Error state also counts as disconnected
    tracker.mark_error("err".into());
    assert!(tracker.should_reconnect());

    // Connected → should NOT reconnect
    tracker.record_data_received();
    assert!(!tracker.should_reconnect());

    // Connecting → not disconnected, should NOT reconnect
    tracker.mark_connecting();
    assert!(!tracker.should_reconnect());
    Ok(())
}

#[test]
fn tracker_channel_overflow_does_not_block() -> TestResult {
    let mut tracker = DisconnectionTracker::with_defaults("test");
    let _rx = tracker.subscribe(); // channel capacity 16

    // Fire more than 16 transitions — should not block
    for _ in 0..20 {
        tracker.mark_connecting();
        tracker.record_data_received();
        tracker.mark_error("err".into());
    }
    // Just ensure we didn't deadlock
    Ok(())
}

// ===========================================================================
// TelemetryError — error trait, display, conversions
// ===========================================================================

#[test]
fn telemetry_error_is_std_error() {
    let err = TelemetryError::ParseError("bad".into());
    let _dyn_err: &dyn std::error::Error = &err;
}

#[test]
fn telemetry_error_from_io_error() -> TestResult {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
    let telem_err = TelemetryError::from(io_err);
    let msg = telem_err.to_string();
    assert!(msg.contains("missing file"));
    Ok(())
}

#[test]
fn telemetry_error_shared_memory_display() -> TestResult {
    let err = TelemetryError::SharedMemoryError("cannot open".into());
    assert!(err.to_string().contains("cannot open"));
    Ok(())
}

#[test]
fn telemetry_error_network_display() -> TestResult {
    let err = TelemetryError::NetworkError("connection refused".into());
    assert!(err.to_string().contains("connection refused"));
    Ok(())
}

#[test]
fn telemetry_error_invalid_data_display() -> TestResult {
    let err = TelemetryError::InvalidData {
        reason: "checksum mismatch".into(),
    };
    assert!(err.to_string().contains("checksum mismatch"));
    Ok(())
}

#[test]
fn telemetry_error_debug_format() -> TestResult {
    let err = TelemetryError::Timeout { timeout_ms: 3000 };
    let debug = format!("{err:?}");
    assert!(debug.contains("3000"));
    Ok(())
}

// ===========================================================================
// TelemetryFlags — comprehensive coverage
// ===========================================================================

#[test]
fn telemetry_flags_all_true() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: true,
        blue_flag: true,
        checkered_flag: true,
        green_flag: true,
        pit_limiter: true,
        in_pits: true,
        drs_available: true,
        drs_active: true,
        ers_available: true,
        ers_active: true,
        launch_control: true,
        traction_control: true,
        abs_active: true,
        engine_limiter: true,
        safety_car: true,
        formation_lap: true,
        session_paused: true,
    };
    let json = serde_json::to_string(&flags)?;
    let decoded: TelemetryFlags = serde_json::from_str(&json)?;
    assert_eq!(flags, decoded);
    Ok(())
}

#[test]
fn telemetry_flags_clone_independence() -> TestResult {
    let a = TelemetryFlags {
        yellow_flag: true,
        ..Default::default()
    };
    let b = a.clone();
    assert_eq!(a, b);
    Ok(())
}

// ===========================================================================
// TelemetryFieldCoverage — construction and serde
// ===========================================================================

#[test]
fn telemetry_field_coverage_empty_extended() -> TestResult {
    let coverage = TelemetryFieldCoverage {
        game_id: "dirt5".into(),
        game_version: "1.0".into(),
        ffb_scalar: false,
        rpm: true,
        speed: true,
        slip_ratio: false,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        },
        car_id: false,
        track_id: false,
        extended_fields: vec![],
    };
    let json = serde_json::to_string(&coverage)?;
    let back: TelemetryFieldCoverage = serde_json::from_str(&json)?;
    assert_eq!(back.game_id, "dirt5");
    assert!(back.extended_fields.is_empty());
    assert!(back.flags.green_flag);
    assert!(!back.flags.yellow_flag);
    Ok(())
}

#[test]
fn flag_coverage_debug_and_clone() -> TestResult {
    let fc = FlagCoverage {
        yellow_flag: true,
        red_flag: false,
        blue_flag: true,
        checkered_flag: false,
        green_flag: true,
        pit_limiter: false,
        in_pits: false,
        drs_available: false,
        drs_active: false,
        ers_available: false,
        launch_control: false,
        traction_control: false,
        abs_active: false,
    };
    let cloned = fc.clone();
    assert_eq!(format!("{fc:?}"), format!("{cloned:?}"));
    Ok(())
}

// ===========================================================================
// telemetry_now_ns — thread safety
// ===========================================================================

#[test]
fn telemetry_now_ns_from_multiple_threads() -> TestResult {
    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let mut prev = openracing_telemetry::telemetry_now_ns();
                for _ in 0..100 {
                    let now = openracing_telemetry::telemetry_now_ns();
                    assert!(now >= prev, "timestamps must be monotonic");
                    prev = now;
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }
    Ok(())
}

#[test]
fn telemetry_now_ns_returns_nonzero_after_delay() -> TestResult {
    std::thread::sleep(Duration::from_millis(1));
    let ns = openracing_telemetry::telemetry_now_ns();
    assert!(ns > 0, "should be nonzero after 1ms delay");
    Ok(())
}

// ===========================================================================
// adapter_factories — static initialization
// ===========================================================================

#[test]
fn adapter_factories_returns_empty_slice() -> TestResult {
    let factories = openracing_telemetry::adapter_factories();
    // Without registrations, should be empty
    assert!(factories.is_empty());
    Ok(())
}

// ===========================================================================
// BddMatrixMetrics re-export — basic access
// ===========================================================================

#[test]
fn bdd_matrix_metrics_from_sets_exact() -> TestResult {
    use openracing_telemetry::BddMatrixMetrics;
    use openracing_telemetry::MatrixParityPolicy;

    let m = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    assert!(m.parity_ok);
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert_eq!(m.matrix_game_count, 2);
    assert_eq!(m.registry_game_count, 2);
    assert_eq!(m.matrix_coverage_ratio, 1.0);
    assert_eq!(m.registry_coverage_ratio, 1.0);
    Ok(())
}

#[test]
fn bdd_matrix_metrics_empty_inputs() -> TestResult {
    use openracing_telemetry::BddMatrixMetrics;
    use openracing_telemetry::MatrixParityPolicy;

    let m = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        MatrixParityPolicy::STRICT,
    );
    assert!(m.parity_ok);
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert_eq!(m.registry_coverage_ratio, 0.0);
    Ok(())
}

#[test]
fn runtime_bdd_matrix_metrics_combined() -> TestResult {
    use openracing_telemetry::{BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics};

    let adapter = BddMatrixMetrics::from_sets(["a", "b"], ["a", "b"], MatrixParityPolicy::STRICT);
    let writer = BddMatrixMetrics::from_sets(["a", "b"], ["a"], MatrixParityPolicy::STRICT);
    let rt = RuntimeBddMatrixMetrics::new(2, adapter, writer);
    assert!(!rt.parity_ok); // writer is missing "b"
    assert!(rt.adapter.parity_ok);
    assert!(!rt.writer.parity_ok);
    Ok(())
}

// ===========================================================================
// DEFAULT_DISCONNECTION_TIMEOUT_MS constant
// ===========================================================================

#[test]
fn default_disconnection_timeout_constant() {
    assert_eq!(openracing_telemetry::DEFAULT_DISCONNECTION_TIMEOUT_MS, 2000);
}
