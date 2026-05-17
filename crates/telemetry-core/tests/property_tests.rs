#![allow(clippy::redundant_closure)]
//! Property-based tests for openracing-telemetry.
//!
//! Uses proptest for randomized testing of telemetry value round-trips,
//! normalization boundaries, type conversions, and builder validation.

use openracing_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, NormalizedTelemetry,
    TelemetryFlags, TelemetryFrame, TelemetryValue,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Proptest strategies ─────────────────────────────────────────────────

fn finite_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        prop::num::f32::NORMAL,
        Just(0.0f32),
        Just(1.0f32),
        Just(-1.0f32),
    ]
}

fn unit_f32() -> impl Strategy<Value = f32> {
    -2.0f32..=2.0f32
}

fn positive_f32() -> impl Strategy<Value = f32> {
    0.0f32..=50000.0f32
}

// ── Builder round-trips via serde ───────────────────────────────────────

proptest! {
    #[test]
    fn builder_speed_ms_serde_round_trip(speed in positive_f32()) {
        let t = NormalizedTelemetry::builder().speed_ms(speed).build();
        let json = serde_json::to_string(&t).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(t.speed_ms, decoded.speed_ms);
    }

    #[test]
    fn builder_rpm_serde_round_trip(rpm in positive_f32()) {
        let t = NormalizedTelemetry::builder().rpm(rpm).build();
        let json = serde_json::to_string(&t).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(t.rpm, decoded.rpm);
    }

    #[test]
    fn builder_gear_serde_round_trip(gear in -1i8..=8i8) {
        let t = NormalizedTelemetry::builder().gear(gear).build();
        let json = serde_json::to_string(&t).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(t.gear, decoded.gear);
    }

    #[test]
    fn builder_ffb_scalar_serde_round_trip(ffb in unit_f32()) {
        let t = NormalizedTelemetry::builder().ffb_scalar(ffb).build();
        let json = serde_json::to_string(&t).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(t.ffb_scalar, decoded.ffb_scalar);
    }
}

// ── Normalization boundary properties ───────────────────────────────────

proptest! {
    #[test]
    fn ffb_scalar_always_in_range(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().ffb_scalar(value).build();
        prop_assert!(t.ffb_scalar >= -1.0);
        prop_assert!(t.ffb_scalar <= 1.0);
    }

    #[test]
    fn throttle_always_clamped_zero_to_one(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().throttle(value).build();
        prop_assert!(t.throttle >= 0.0);
        prop_assert!(t.throttle <= 1.0);
    }

    #[test]
    fn brake_always_clamped_zero_to_one(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().brake(value).build();
        prop_assert!(t.brake >= 0.0);
        prop_assert!(t.brake <= 1.0);
    }

    #[test]
    fn clutch_always_clamped_zero_to_one(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().clutch(value).build();
        prop_assert!(t.clutch >= 0.0);
        prop_assert!(t.clutch <= 1.0);
    }

    #[test]
    fn slip_ratio_always_clamped_zero_to_one(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().slip_ratio(value).build();
        prop_assert!(t.slip_ratio >= 0.0);
        prop_assert!(t.slip_ratio <= 1.0);
    }

    #[test]
    fn fuel_percent_always_clamped_zero_to_one(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().fuel_percent(value).build();
        prop_assert!(t.fuel_percent >= 0.0);
        prop_assert!(t.fuel_percent <= 1.0);
    }

    #[test]
    fn speed_never_negative(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().speed_ms(value).build();
        prop_assert!(t.speed_ms >= 0.0);
    }

    #[test]
    fn rpm_never_negative(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().rpm(value).build();
        prop_assert!(t.rpm >= 0.0);
    }

    #[test]
    fn max_rpm_never_negative(value in finite_f32()) {
        let t = NormalizedTelemetry::builder().max_rpm(value).build();
        prop_assert!(t.max_rpm >= 0.0);
    }
}

// ── NaN handling properties ─────────────────────────────────────────────

#[test]
fn builder_nan_values_produce_zero() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::NAN)
        .rpm(f32::NAN)
        .throttle(f32::NAN)
        .brake(f32::NAN)
        .clutch(f32::NAN)
        .lateral_g(f32::NAN)
        .longitudinal_g(f32::NAN)
        .vertical_g(f32::NAN)
        .ffb_scalar(f32::NAN)
        .slip_ratio(f32::NAN)
        .fuel_percent(f32::NAN)
        .build();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn builder_infinity_values_produce_zero() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::INFINITY)
        .rpm(f32::NEG_INFINITY)
        .build();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

// ── validated() normalization ───────────────────────────────────────────

proptest! {
    #[test]
    fn validated_always_produces_finite_values(
        speed in prop::num::f32::ANY,
        throttle in prop::num::f32::ANY,
        brake in prop::num::f32::ANY,
        ffb in prop::num::f32::ANY,
        fuel in prop::num::f32::ANY,
        slip in prop::num::f32::ANY,
    ) {
        let t = NormalizedTelemetry {
            speed_ms: speed,
            throttle,
            brake,
            ffb_scalar: ffb,
            fuel_percent: fuel,
            slip_ratio: slip,
            ..Default::default()
        };

        let v = t.validated();
        prop_assert!(v.speed_ms.is_finite());
        prop_assert!(v.throttle.is_finite());
        prop_assert!(v.brake.is_finite());
        prop_assert!(v.ffb_scalar.is_finite());
        prop_assert!(v.fuel_percent.is_finite());
        prop_assert!(v.slip_ratio.is_finite());

        prop_assert!(v.speed_ms >= 0.0);
        prop_assert!(v.throttle >= 0.0 && v.throttle <= 1.0);
        prop_assert!(v.brake >= 0.0 && v.brake <= 1.0);
        prop_assert!(v.ffb_scalar >= -1.0 && v.ffb_scalar <= 1.0);
        prop_assert!(v.fuel_percent >= 0.0 && v.fuel_percent <= 1.0);
        prop_assert!(v.slip_ratio >= 0.0 && v.slip_ratio <= 1.0);
    }
}

// ── Type conversion properties ──────────────────────────────────────────

proptest! {
    #[test]
    fn speed_kmh_is_3_6x_speed_ms(speed in positive_f32()) {
        let t = NormalizedTelemetry::builder().speed_ms(speed).build();
        let diff = (t.speed_kmh() - speed * 3.6).abs();
        prop_assert!(diff < 0.01);
    }

    #[test]
    fn speed_mph_is_2_237x_speed_ms(speed in positive_f32()) {
        let t = NormalizedTelemetry::builder().speed_ms(speed).build();
        let diff = (t.speed_mph() - speed * 2.237).abs();
        prop_assert!(diff < 0.01);
    }

    #[test]
    fn rpm_fraction_always_in_zero_to_one(
        rpm in 0.0f32..=20000.0f32,
        max_rpm in 1.0f32..=20000.0f32,
    ) {
        let t = NormalizedTelemetry::builder()
            .rpm(rpm)
            .max_rpm(max_rpm)
            .build();
        let frac = t.rpm_fraction();
        prop_assert!(frac >= 0.0);
        prop_assert!(frac <= 1.0);
    }
}

// ── TelemetryValue serde round-trip ─────────────────────────────────────

proptest! {
    #[test]
    fn telemetry_value_float_round_trip(val in finite_f32()) {
        let v = TelemetryValue::Float(val);
        let json = serde_json::to_string(&v).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: TelemetryValue = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(v, decoded);
    }

    #[test]
    fn telemetry_value_integer_round_trip(val in prop::num::i32::ANY) {
        let v = TelemetryValue::Integer(val);
        let json = serde_json::to_string(&v).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: TelemetryValue = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(v, decoded);
    }

    #[test]
    fn telemetry_value_boolean_round_trip(val in prop::bool::ANY) {
        let v = TelemetryValue::Boolean(val);
        let json = serde_json::to_string(&v).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: TelemetryValue = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(v, decoded);
    }

    #[test]
    fn telemetry_value_string_round_trip(val in "[a-zA-Z0-9_]{0,50}") {
        let v = TelemetryValue::String(val);
        let json = serde_json::to_string(&v).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let decoded: TelemetryValue = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(v, decoded);
    }
}

// ── TelemetryFrame ──────────────────────────────────────────────────────

proptest! {
    #[test]
    fn telemetry_frame_preserves_metadata(
        seq in 0u64..=1_000_000u64,
        raw_size in 0usize..=4096usize,
        ts in 0u64..=u64::MAX,
    ) {
        let data = NormalizedTelemetry::default();
        let frame = TelemetryFrame::new(data, ts, seq, raw_size);
        prop_assert_eq!(frame.timestamp_ns, ts);
        prop_assert_eq!(frame.sequence, seq);
        prop_assert_eq!(frame.raw_size, raw_size);
    }
}

// ── TelemetryFrame::from_telemetry ──────────────────────────────────────

#[test]
fn frame_from_telemetry_auto_timestamps() -> TestResult {
    let data = NormalizedTelemetry::builder().speed_ms(30.0).build();
    let frame = TelemetryFrame::from_telemetry(data, 1, 256);
    assert!(frame.timestamp_ns > 0);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, 256);
    Ok(())
}

// ── ConnectionState ─────────────────────────────────────────────────────

#[test]
fn connection_state_default_is_disconnected() -> TestResult {
    let state = ConnectionState::default();
    assert_eq!(state, ConnectionState::Disconnected);
    assert!(state.is_disconnected());
    assert!(!state.is_connected());
    assert!(!state.is_transitioning());
    Ok(())
}

#[test]
fn connection_state_connected() -> TestResult {
    let state = ConnectionState::Connected;
    assert!(state.is_connected());
    assert!(!state.is_disconnected());
    assert!(!state.is_transitioning());
    Ok(())
}

#[test]
fn connection_state_transitioning() -> TestResult {
    assert!(ConnectionState::Connecting.is_transitioning());
    assert!(ConnectionState::Reconnecting.is_transitioning());
    assert!(!ConnectionState::Connected.is_transitioning());
    assert!(!ConnectionState::Disconnected.is_transitioning());
    Ok(())
}

#[test]
fn connection_state_error_is_disconnected() -> TestResult {
    let state = ConnectionState::Error;
    assert!(state.is_disconnected());
    assert!(!state.is_connected());
    Ok(())
}

// ── ConnectionStateEvent ────────────────────────────────────────────────

#[test]
fn connection_event_is_connection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "iracing",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        None,
    );
    assert!(event.is_connection());
    assert!(!event.is_disconnection());
    assert_eq!(event.game_id, "iracing");
    assert!(event.timestamp_ns > 0);
    Ok(())
}

#[test]
fn connection_event_is_disconnection() -> TestResult {
    let event = ConnectionStateEvent::new(
        "acc",
        ConnectionState::Connected,
        ConnectionState::Disconnected,
        Some("timeout".to_string()),
    );
    assert!(event.is_disconnection());
    assert!(!event.is_connection());
    assert_eq!(event.reason.as_deref(), Some("timeout"));
    Ok(())
}

// ── DisconnectionConfig ─────────────────────────────────────────────────

#[test]
fn disconnection_config_default() -> TestResult {
    let config = DisconnectionConfig::default();
    assert_eq!(config.timeout_ms, 2000);
    assert!(config.auto_reconnect);
    assert_eq!(config.max_reconnect_attempts, 0);
    assert_eq!(config.reconnect_delay_ms, 1000);
    assert_eq!(config.timeout().as_millis(), 2000);
    assert_eq!(config.reconnect_delay().as_millis(), 1000);
    Ok(())
}

#[test]
fn disconnection_config_with_timeout() -> TestResult {
    let config = DisconnectionConfig::with_timeout(5000);
    assert_eq!(config.timeout_ms, 5000);
    assert!(config.auto_reconnect);
    Ok(())
}

// ── TelemetryFlags serde with missing fields ────────────────────────────

#[test]
fn telemetry_flags_deserialize_empty_json() -> TestResult {
    let json = r#"{}"#;
    let flags: TelemetryFlags = serde_json::from_str(json)?;
    assert!(flags.green_flag);
    assert!(!flags.yellow_flag);
    Ok(())
}

// ── Extended map via builder ────────────────────────────────────────────

proptest! {
    #[test]
    fn extended_map_keys_preserved(key in "[a-z_]{1,20}") {
        let t = NormalizedTelemetry::builder()
            .extended(&key, TelemetryValue::Integer(42))
            .build();
        let val = t.get_extended(&key);
        prop_assert_eq!(val, Some(&TelemetryValue::Integer(42)));
    }
}

// ── is_stationary threshold ─────────────────────────────────────────────

proptest! {
    #[test]
    fn stationary_below_threshold(speed in 0.0f32..0.5f32) {
        let t = NormalizedTelemetry::builder().speed_ms(speed).build();
        prop_assert!(t.is_stationary());
    }

    #[test]
    fn not_stationary_at_or_above_threshold(speed in 0.5f32..=200.0f32) {
        let t = NormalizedTelemetry::builder().speed_ms(speed).build();
        prop_assert!(!t.is_stationary());
    }
}

// ── total_g pythagorean ─────────────────────────────────────────────────

proptest! {
    #[test]
    fn total_g_is_non_negative(lat in finite_f32(), lon in finite_f32()) {
        let t = NormalizedTelemetry::builder()
            .lateral_g(lat)
            .longitudinal_g(lon)
            .build();
        prop_assert!(t.total_g() >= 0.0);
    }
}

// ── ConnectionState serde round-trip ────────────────────────────────────

#[test]
fn connection_state_serde_round_trip() -> TestResult {
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

// ── DisconnectionConfig serde round-trip ────────────────────────────────

#[test]
fn disconnection_config_serde_round_trip() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 3000,
        auto_reconnect: false,
        max_reconnect_attempts: 5,
        reconnect_delay_ms: 2000,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: DisconnectionConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.timeout_ms, config.timeout_ms);
    assert_eq!(decoded.auto_reconnect, config.auto_reconnect);
    assert_eq!(
        decoded.max_reconnect_attempts,
        config.max_reconnect_attempts
    );
    assert_eq!(decoded.reconnect_delay_ms, config.reconnect_delay_ms);
    Ok(())
}
