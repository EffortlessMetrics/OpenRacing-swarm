//! Deep tests v2 for telemetry-core: unit conversions, coordinate transforms,
//! timestamp precision, rate limiter edge cases, adaptive limiter behavior,
//! and telemetry data type coverage.

use openracing_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
    GameTelemetry, GameTelemetrySnapshot, NormalizedTelemetry, TelemetryError, TelemetryFrame,
    TelemetryValue,
    rate_limiter::{AdaptiveRateLimiter, RateLimiter, RateLimiterStats},
};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Unit conversions — speed (m/s ↔ km/h ↔ mph)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn speed_mps_to_kmh_conversion() -> TestResult {
    let t = GameTelemetry {
        speed_mps: 10.0,
        ..Default::default()
    };
    let kmh = t.speed_kmh();
    assert!(
        (kmh - 36.0).abs() < 0.01,
        "10 m/s should be ~36 km/h, got {kmh}"
    );
    Ok(())
}

#[test]
fn speed_mps_to_mph_conversion() -> TestResult {
    let t = GameTelemetry {
        speed_mps: 10.0,
        ..Default::default()
    };
    let mph = t.speed_mph();
    assert!(
        (mph - 22.37).abs() < 0.01,
        "10 m/s should be ~22.37 mph, got {mph}"
    );
    Ok(())
}

#[test]
fn speed_zero_conversions() -> TestResult {
    let t = GameTelemetry::default();
    assert!((t.speed_kmh()).abs() < f32::EPSILON);
    assert!((t.speed_mph()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn normalized_speed_kmh_matches_game_telemetry() -> TestResult {
    let speed_mps = 27.78; // ~100 km/h
    let t = NormalizedTelemetry::builder().speed_ms(speed_mps).build();
    let kmh = t.speed_kmh();
    assert!(
        (kmh - 100.008).abs() < 0.1,
        "27.78 m/s ≈ 100 km/h, got {kmh}"
    );
    Ok(())
}

#[test]
fn normalized_speed_mph_matches_game_telemetry() -> TestResult {
    let speed_mps = 44.704; // ~100 mph
    let t = NormalizedTelemetry::builder().speed_ms(speed_mps).build();
    let mph = t.speed_mph();
    assert!((mph - 100.0).abs() < 0.5, "44.704 m/s ≈ 100 mph, got {mph}");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Unit conversions — tire temperature (°C array field)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tire_temps_celsius_to_fahrenheit() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .tire_temps_c([80, 85, 78, 82])
        .build();

    // Manual °C→°F: F = C * 9/5 + 32
    let expected_f: Vec<f32> = t
        .tire_temps_c
        .iter()
        .map(|&c| c as f32 * 9.0 / 5.0 + 32.0)
        .collect();
    assert!((expected_f[0] - 176.0).abs() < 0.01, "80°C = 176°F");
    assert!((expected_f[1] - 185.0).abs() < 0.01, "85°C = 185°F");
    assert!((expected_f[2] - 172.4).abs() < 0.01, "78°C = 172.4°F");
    assert!((expected_f[3] - 179.6).abs() < 0.01, "82°C = 179.6°F");
    Ok(())
}

#[test]
fn tire_temps_zero_celsius() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .tire_temps_c([0, 0, 0, 0])
        .build();
    assert!(t.tire_temps_c.iter().all(|&c| c == 0));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Unit conversions — engine temperature
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn engine_temp_celsius_stored_and_convertible() -> TestResult {
    let t = NormalizedTelemetry::builder().engine_temp_c(90.0).build();
    assert!((t.engine_temp_c - 90.0).abs() < f32::EPSILON);
    // Manual °C→°F conversion check
    let fahrenheit = t.engine_temp_c * 9.0 / 5.0 + 32.0;
    assert!((fahrenheit - 194.0).abs() < 0.01, "90°C = 194°F");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Coordinate system / G-force transforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn total_g_pythagorean_3_4_5() -> TestResult {
    let t = GameTelemetry {
        lateral_g: 3.0,
        longitudinal_g: 4.0,
        ..Default::default()
    };
    assert!((t.total_g() - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn total_g_zero_when_stationary() -> TestResult {
    let t = GameTelemetry::default();
    assert!((t.total_g()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn total_g_negative_components() -> TestResult {
    let t = GameTelemetry {
        lateral_g: -3.0,
        longitudinal_g: -4.0,
        ..Default::default()
    };
    assert!(
        (t.total_g() - 5.0).abs() < 0.001,
        "magnitude should be positive"
    );
    Ok(())
}

#[test]
fn normalized_total_g_matches_game_telemetry() -> TestResult {
    let n = NormalizedTelemetry::builder()
        .lateral_g(1.5)
        .longitudinal_g(2.0)
        .build();
    let expected = (1.5f32 * 1.5 + 2.0 * 2.0).sqrt();
    assert!((n.total_g() - expected).abs() < 0.001);
    Ok(())
}

#[test]
fn vertical_g_independent_of_total_g() -> TestResult {
    let n = NormalizedTelemetry::builder()
        .lateral_g(1.0)
        .longitudinal_g(0.0)
        .vertical_g(9.81)
        .build();
    // total_g only uses lateral + longitudinal
    assert!((n.total_g() - 1.0).abs() < 0.001);
    assert!((n.vertical_g - 9.81).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Slip angle coordinate system
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn average_slip_angle_symmetric() -> TestResult {
    let t = GameTelemetry {
        slip_angle_fl: 0.1,
        slip_angle_fr: 0.1,
        slip_angle_rl: 0.1,
        slip_angle_rr: 0.1,
        ..Default::default()
    };
    assert!((t.average_slip_angle() - 0.1).abs() < 0.001);
    Ok(())
}

#[test]
fn front_rear_slip_angle_split() -> TestResult {
    let t = GameTelemetry {
        slip_angle_fl: 0.2,
        slip_angle_fr: 0.4,
        slip_angle_rl: 0.6,
        slip_angle_rr: 0.8,
        ..Default::default()
    };
    assert!((t.front_slip_angle() - 0.3).abs() < 0.001);
    assert!((t.rear_slip_angle() - 0.7).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Timestamp precision
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_now_ns_monotonically_increases() -> TestResult {
    let t1 = openracing_telemetry::telemetry_now_ns();
    std::thread::sleep(Duration::from_millis(1));
    let t2 = openracing_telemetry::telemetry_now_ns();
    assert!(t2 > t1, "telemetry_now_ns should increase over time");
    Ok(())
}

#[test]
fn telemetry_now_ns_nanosecond_granularity() -> TestResult {
    let t1 = openracing_telemetry::telemetry_now_ns();
    // Busy-wait briefly
    let mut t2 = t1;
    for _ in 0..10_000 {
        t2 = openracing_telemetry::telemetry_now_ns();
        if t2 > t1 {
            break;
        }
    }
    // Should have advanced by at least 1 ns
    assert!(t2 > t1, "timestamp should advance within busy loop");
    Ok(())
}

#[test]
fn snapshot_timestamp_ns_resolution() -> TestResult {
    let epoch = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let t = GameTelemetry::default();
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);

    // Should be at least 5ms = 5_000_000 ns
    assert!(
        snap.timestamp_ns >= 4_000_000,
        "snapshot timestamp_ns too low: {}",
        snap.timestamp_ns
    );
    Ok(())
}

#[test]
fn snapshot_same_epoch_yields_small_timestamp() -> TestResult {
    let epoch = Instant::now();
    let t = GameTelemetry::default();
    let snap = GameTelemetrySnapshot::from_telemetry(&t, epoch);
    // Should be very small (sub-millisecond)
    assert!(
        snap.timestamp_ns < 10_000_000,
        "same-instant snapshot should be <10ms"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Telemetry data types — NormalizedTelemetry full field coverage
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn builder_sets_all_motion_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(50.0)
        .steering_angle(-0.5)
        .throttle(0.75)
        .brake(0.25)
        .build();
    assert!((t.speed_ms - 50.0).abs() < f32::EPSILON);
    assert!((t.steering_angle - (-0.5)).abs() < f32::EPSILON);
    assert!((t.throttle - 0.75).abs() < f32::EPSILON);
    assert!((t.brake - 0.25).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_engine_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(7000.0)
        .max_rpm(9000.0)
        .gear(5)
        .num_gears(6)
        .build();
    assert!((t.rpm - 7000.0).abs() < f32::EPSILON);
    assert!((t.max_rpm - 9000.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 5);
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn builder_sets_lap_timing_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .lap(3)
        .position(5)
        .current_lap_time_s(62.5)
        .best_lap_time_s(61.0)
        .last_lap_time_s(63.2)
        .delta_ahead_s(-0.5)
        .delta_behind_s(1.2)
        .build();
    assert_eq!(t.lap, 3);
    assert_eq!(t.position, 5);
    assert!((t.current_lap_time_s - 62.5).abs() < f32::EPSILON);
    assert!((t.best_lap_time_s - 61.0).abs() < f32::EPSILON);
    assert!((t.last_lap_time_s - 63.2).abs() < f32::EPSILON);
    assert!((t.delta_ahead_s - (-0.5)).abs() < f32::EPSILON);
    assert!((t.delta_behind_s - 1.2).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_fuel_and_engine_temp() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .fuel_percent(0.65)
        .engine_temp_c(95.0)
        .build();
    assert!((t.fuel_percent - 0.65).abs() < f32::EPSILON);
    assert!((t.engine_temp_c - 95.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_tire_pressures() -> TestResult {
    let pressures = [28.0f32, 28.5, 27.5, 28.0];
    let t = NormalizedTelemetry::builder()
        .tire_pressures_psi(pressures)
        .build();
    assert_eq!(t.tire_pressures_psi, pressures);
    Ok(())
}

#[test]
fn telemetry_value_all_variants() -> TestResult {
    let f = TelemetryValue::Float(3.25);
    let i = TelemetryValue::Integer(42);
    let b = TelemetryValue::Boolean(true);
    let s = TelemetryValue::String("hello".to_string());

    // Ensure equality and debug work
    assert_eq!(f, TelemetryValue::Float(3.25));
    assert_eq!(i, TelemetryValue::Integer(42));
    assert_eq!(b, TelemetryValue::Boolean(true));
    assert_eq!(s, TelemetryValue::String("hello".to_string()));

    let f_json = serde_json::to_string(&f)?;
    let f_back: TelemetryValue = serde_json::from_str(&f_json)?;
    assert_eq!(f, f_back);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Validated — clamping
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validated_clamps_throttle_brake_to_0_1() -> TestResult {
    let t = NormalizedTelemetry {
        throttle: 1.5,
        brake: -0.3,
        ..Default::default()
    };
    let v = t.validated();
    assert!((v.throttle - 1.0).abs() < f32::EPSILON);
    assert!((v.brake - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_nan_to_zero() -> TestResult {
    let t = NormalizedTelemetry {
        speed_ms: f32::NAN,
        rpm: f32::NAN,
        throttle: f32::NAN,
        lateral_g: f32::NAN,
        ..Default::default()
    };
    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.rpm, 0.0);
    assert_eq!(v.throttle, 0.0);
    assert_eq!(v.lateral_g, 0.0);
    Ok(())
}

#[test]
fn validated_clamps_infinity_to_safe_values() -> TestResult {
    let t = NormalizedTelemetry {
        speed_ms: f32::INFINITY,
        ffb_scalar: f32::NEG_INFINITY,
        ..Default::default()
    };
    let v = t.validated();
    // speed_ms: infinity is not finite, so → 0.0
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.ffb_scalar, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// RPM fraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rpm_fraction_at_redline() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(8000.0)
        .max_rpm(8000.0)
        .build();
    assert!((t.rpm_fraction() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn rpm_fraction_zero_max_rpm_returns_zero() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .max_rpm(0.0)
        .build();
    assert!((t.rpm_fraction()).abs() < f32::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rate limiter — deep edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rate_limiter_set_rate_to_one_hz() -> TestResult {
    let mut limiter = RateLimiter::new(1);
    assert!(limiter.should_process());
    // Immediately after, should drop
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
    Ok(())
}

#[test]
fn rate_limiter_stats_from_reference() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    assert!(!limiter.should_process());

    let stats = RateLimiterStats::from(&limiter);
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 2);
    // drop rate should be ~66.67%
    assert!((stats.drop_rate_percent - 66.666).abs() < 1.0);
    Ok(())
}

#[test]
fn adaptive_limiter_high_cpu_reduces_rate() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    let initial = adaptive.stats().max_rate_hz;

    for _ in 0..20 {
        adaptive.update_cpu_usage(90.0);
    }
    let after_high_cpu = adaptive.stats().max_rate_hz;
    assert!(after_high_cpu < initial, "high CPU should reduce rate");
    Ok(())
}

#[test]
fn adaptive_limiter_low_cpu_increases_rate() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(500, 80.0);

    // First drive it down
    for _ in 0..10 {
        adaptive.update_cpu_usage(95.0);
    }
    let reduced = adaptive.stats().max_rate_hz;

    // Then let CPU drop
    for _ in 0..20 {
        adaptive.update_cpu_usage(10.0);
    }
    let recovered = adaptive.stats().max_rate_hz;
    assert!(recovered > reduced, "low CPU should allow rate recovery");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Connection state — From/Into and GameTelemetry conversion
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn game_telemetry_into_normalized() -> TestResult {
    let gt = GameTelemetry {
        speed_mps: 30.0,
        rpm: 6000.0,
        gear: 4,
        throttle: 0.8,
        brake: 0.1,
        lateral_g: 0.5,
        longitudinal_g: 0.3,
        ..Default::default()
    };
    let n: NormalizedTelemetry = gt.into();
    assert!((n.speed_ms - 30.0).abs() < f32::EPSILON);
    assert!((n.rpm - 6000.0).abs() < f32::EPSILON);
    assert_eq!(n.gear, 4);
    Ok(())
}

#[test]
fn game_telemetry_stationary_threshold() -> TestResult {
    let moving = GameTelemetry {
        speed_mps: 0.5,
        ..Default::default()
    };
    // 0.5 m/s is NOT stationary (threshold is < 0.5)
    assert!(!moving.is_stationary());

    let stopped = GameTelemetry {
        speed_mps: 0.49,
        ..Default::default()
    };
    assert!(stopped.is_stationary());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Disconnection tracker — max attempts enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tracker_max_reconnect_attempts_prevents_reconnect() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 10,
        auto_reconnect: true,
        max_reconnect_attempts: 2,
        reconnect_delay_ms: 10,
    };
    let mut tracker = DisconnectionTracker::new("test", config);

    // Connect then disconnect
    tracker.record_data_received();
    std::thread::sleep(Duration::from_millis(20));
    let _state = tracker.check_disconnection();

    // Attempt reconnects up to max
    tracker.mark_reconnecting(); // attempt 1
    tracker.mark_reconnecting(); // attempt 2

    // Error state: should not reconnect after max attempts
    tracker.mark_error("test failure".to_string());
    assert!(
        !tracker.should_reconnect(),
        "should not reconnect after max attempts"
    );
    Ok(())
}

#[test]
fn tracker_auto_reconnect_disabled() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 10,
        auto_reconnect: false,
        max_reconnect_attempts: 0,
        reconnect_delay_ms: 10,
    };
    let mut tracker = DisconnectionTracker::new("test", config);
    tracker.mark_error("lost connection".to_string());
    assert!(!tracker.should_reconnect());
    Ok(())
}

#[test]
fn connection_state_event_is_disconnection_logic() -> TestResult {
    let evt = ConnectionStateEvent::new(
        "game1",
        ConnectionState::Connected,
        ConnectionState::Disconnected,
        Some("timeout".to_string()),
    );
    assert!(evt.is_disconnection());
    assert!(!evt.is_connection());

    let evt2 = ConnectionStateEvent::new(
        "game1",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        None,
    );
    assert!(evt2.is_connection());
    assert!(!evt2.is_disconnection());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryError variants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_error_timeout_display_includes_ms() -> TestResult {
    let err = TelemetryError::Timeout { timeout_ms: 5000 };
    let msg = format!("{err}");
    assert!(
        msg.contains("5000"),
        "timeout display should include ms value"
    );
    Ok(())
}

#[test]
fn telemetry_error_invalid_data_display_includes_reason() -> TestResult {
    let err = TelemetryError::InvalidData {
        reason: "corrupt header".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("corrupt header"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryFrame construction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn frame_from_telemetry_has_nonzero_timestamp() -> TestResult {
    let data = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::from_telemetry(data, 42, 128);
    assert!(
        frame.timestamp_ns > 0,
        "from_telemetry should set current time"
    );
    assert_eq!(frame.sequence, 42);
    assert_eq!(frame.raw_size, 128);
    Ok(())
}
