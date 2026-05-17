//! Deep protocol-level tests for the rFactor 2 shared-memory telemetry adapter.
//!
//! Exercises struct construction, normalization, flag extraction, slip-ratio
//! calculation, FFB derivation, and edge-case handling against the rF2
//! SharedMemoryMapPlugin data structures.

use openracing_telemetry_adapters::rfactor2::{
    GamePhase, RF2ForceFeedback, RF2ScoringHeader, RF2VehicleTelemetry, RF2WheelTelemetry,
    RFactor2Adapter,
};
use openracing_telemetry_adapters::{TelemetryAdapter, TelemetryValue};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn adapter() -> RFactor2Adapter {
    RFactor2Adapter::new()
}

fn default_vehicle() -> RF2VehicleTelemetry {
    RF2VehicleTelemetry::default()
}

fn set_vehicle_name(v: &mut RF2VehicleTelemetry, name: &str) {
    let b = name.as_bytes();
    let n = b.len().min(63);
    v.vehicle_name[..n].copy_from_slice(&b[..n]);
    v.vehicle_name[n] = 0;
}

fn set_track_name(v: &mut RF2VehicleTelemetry, name: &str) {
    let b = name.as_bytes();
    let n = b.len().min(63);
    v.track_name[..n].copy_from_slice(&b[..n]);
    v.track_name[n] = 0;
}

// ── 1. Adapter metadata ─────────────────────────────────────────────────────

#[test]
fn rf2_game_id_is_rfactor2() -> TestResult {
    assert_eq!(adapter().game_id(), "rfactor2");
    Ok(())
}

#[test]
fn rf2_update_rate_60hz() -> TestResult {
    assert_eq!(adapter().expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

// ── 2. normalize() raw byte size validation ─────────────────────────────────

#[test]
fn rf2_normalize_empty_bytes_rejected() -> TestResult {
    let a = adapter();
    assert!(a.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn rf2_normalize_too_small_rejected() -> TestResult {
    let a = adapter();
    let buf = vec![0u8; 64];
    assert!(a.normalize(&buf).is_err());
    Ok(())
}

// ── 3. Speed from local velocity vector ─────────────────────────────────────

#[test]
fn rf2_speed_from_single_axis() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.local_vel = [50.0, 0.0, 0.0];
    let t = a.normalize_rf2_data(&v, None, None);
    assert!((t.speed_ms - 50.0).abs() < 0.1, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn rf2_speed_from_combined_axes() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    // sqrt(30² + 40²) = 50.0
    v.local_vel = [30.0, 0.0, 40.0];
    let t = a.normalize_rf2_data(&v, None, None);
    assert!((t.speed_ms - 50.0).abs() < 0.1, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn rf2_speed_zero_when_stationary() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let t = a.normalize_rf2_data(&v, None, None);
    assert!(t.speed_ms.abs() < 0.01);
    Ok(())
}

// ── 4. Gear encoding ────────────────────────────────────────────────────────

#[test]
fn rf2_gear_reverse() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.gear = -1;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(t.gear, -1, "gear -1 should be reverse");
    Ok(())
}

#[test]
fn rf2_gear_neutral() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.gear = 0;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(t.gear, 0, "gear 0 should be neutral");
    Ok(())
}

#[test]
fn rf2_gear_forward_range() -> TestResult {
    let a = adapter();
    for g in 1i32..=7 {
        let mut v = default_vehicle();
        v.gear = g;
        let t = a.normalize_rf2_data(&v, None, None);
        assert_eq!(t.gear, g as i8, "gear {g} mismatch");
    }
    Ok(())
}

// ── 5. RPM extraction ───────────────────────────────────────────────────────

#[test]
fn rf2_rpm_extraction() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.engine_rpm = 8500.0;
    let t = a.normalize_rf2_data(&v, None, None);
    assert!((t.rpm - 8500.0).abs() < 0.01);
    Ok(())
}

#[test]
fn rf2_rpm_zero_idle() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

// ── 6. Extended fields ──────────────────────────────────────────────────────

#[test]
fn rf2_extended_throttle_brake_clutch() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.unfiltered_throttle = 0.85;
    v.unfiltered_brake = 0.3;
    v.unfiltered_clutch = 0.1;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(
        t.extended.get("throttle"),
        Some(&TelemetryValue::Float(0.85))
    );
    assert_eq!(t.extended.get("brake"), Some(&TelemetryValue::Float(0.3)));
    assert_eq!(t.extended.get("clutch"), Some(&TelemetryValue::Float(0.1)));
    Ok(())
}

#[test]
fn rf2_extended_temperatures() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.engine_water_temp = 92.0;
    v.engine_oil_temp = 108.0;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(
        t.extended.get("water_temp"),
        Some(&TelemetryValue::Float(92.0))
    );
    assert_eq!(
        t.extended.get("oil_temp"),
        Some(&TelemetryValue::Float(108.0))
    );
    Ok(())
}

#[test]
fn rf2_extended_fuel_level() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.fuel = 42.5;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(
        t.extended.get("fuel_level"),
        Some(&TelemetryValue::Float(42.5))
    );
    Ok(())
}

// ── 7. Car and track strings ────────────────────────────────────────────────

#[test]
fn rf2_car_and_track_names() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    set_vehicle_name(&mut v, "formula_renault");
    set_track_name(&mut v, "spa_francorchamps");
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(t.car_id, Some("formula_renault".to_string()));
    assert_eq!(t.track_id, Some("spa_francorchamps".to_string()));
    Ok(())
}

#[test]
fn rf2_empty_names_produce_empty_or_none() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.vehicle_name[0] = 0;
    v.track_name[0] = 0;
    let t = a.normalize_rf2_data(&v, None, None);
    // Empty name may be None or Some("") depending on builder behaviour.
    let car = t.car_id.as_deref().unwrap_or("");
    let track = t.track_id.as_deref().unwrap_or("");
    assert!(car.is_empty(), "car_id should be empty, got {:?}", car);
    assert!(
        track.is_empty(),
        "track_id should be empty, got {:?}",
        track
    );
    Ok(())
}

// ── 8. Scoring flags ────────────────────────────────────────────────────────

#[test]
fn rf2_green_flag_from_game_phase() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let scoring = RF2ScoringHeader {
        game_phase: GamePhase::GreenFlag as i32,
        ..Default::default()
    };
    let t = a.normalize_rf2_data(&v, Some(&scoring), None);
    assert!(t.flags.green_flag);
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.checkered_flag);
    Ok(())
}

#[test]
fn rf2_yellow_flag_state() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let scoring = RF2ScoringHeader {
        yellow_flag_state: 1,
        game_phase: GamePhase::FullCourseYellow as i32,
        ..Default::default()
    };
    let t = a.normalize_rf2_data(&v, Some(&scoring), None);
    assert!(t.flags.yellow_flag);
    Ok(())
}

#[test]
fn rf2_checkered_flag_on_session_over() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let scoring = RF2ScoringHeader {
        game_phase: GamePhase::SessionOver as i32,
        ..Default::default()
    };
    let t = a.normalize_rf2_data(&v, Some(&scoring), None);
    assert!(t.flags.checkered_flag);
    Ok(())
}

#[test]
fn rf2_in_pits_flag() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let scoring = RF2ScoringHeader {
        in_pits: 1,
        ..Default::default()
    };
    let t = a.normalize_rf2_data(&v, Some(&scoring), None);
    assert!(t.flags.in_pits);
    Ok(())
}

#[test]
fn rf2_no_scoring_gives_default_flags() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let t = a.normalize_rf2_data(&v, None, None);
    // Without scoring data, flags come from TelemetryFlags::default()
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.checkered_flag);
    assert!(!t.flags.in_pits);
    Ok(())
}

// ── 9. Force feedback ───────────────────────────────────────────────────────

#[test]
fn rf2_ffb_from_force_feedback_map() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let ffb = RF2ForceFeedback { force_value: 0.75 };
    let t = a.normalize_rf2_data(&v, None, Some(&ffb));
    assert!(t.ffb_scalar != 0.0, "FFB should be non-zero");
    assert_eq!(
        t.extended.get("ffb_source"),
        Some(&TelemetryValue::String("force_feedback_map".to_string()))
    );
    Ok(())
}

#[test]
fn rf2_ffb_fallback_to_steering_shaft_torque() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.steering_shaft_torque = 12.0;
    let t = a.normalize_rf2_data(&v, None, None);
    assert_eq!(
        t.extended.get("ffb_source"),
        Some(&TelemetryValue::String(
            "telemetry_steering_shaft_torque".to_string()
        ))
    );
    Ok(())
}

#[test]
fn rf2_ffb_nan_becomes_zero() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let ffb = RF2ForceFeedback {
        force_value: f64::NAN,
    };
    // NaN force_value → stable_force_value returns None → falls back to steering_shaft_torque (0.0)
    let t = a.normalize_rf2_data(&v, None, Some(&ffb));
    assert_eq!(t.ffb_scalar, 0.0);
    Ok(())
}

// ── 10. Slip ratio ──────────────────────────────────────────────────────────

#[test]
fn rf2_slip_ratio_zero_when_stationary() -> TestResult {
    let a = adapter();
    let v = default_vehicle();
    let t = a.normalize_rf2_data(&v, None, None);
    assert!(t.slip_ratio.abs() < 0.001);
    Ok(())
}

#[test]
fn rf2_slip_ratio_nonzero_with_lateral_patch_vel() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.local_vel = [20.0, 0.0, 0.0]; // speed ~20 m/s
    for w in &mut v.wheels {
        w.lateral_patch_vel = 2.0;
    }
    let t = a.normalize_rf2_data(&v, None, None);
    assert!(t.slip_ratio > 0.0, "slip_ratio={}", t.slip_ratio);
    assert!(t.slip_ratio <= 1.0, "slip_ratio should be capped at 1.0");
    Ok(())
}

// ── 11. GamePhase enum values ───────────────────────────────────────────────

#[test]
fn rf2_game_phase_enum_values() -> TestResult {
    assert_eq!(GamePhase::Garage as i32, 0);
    assert_eq!(GamePhase::WarmUp as i32, 1);
    assert_eq!(GamePhase::GridWalk as i32, 2);
    assert_eq!(GamePhase::Formation as i32, 3);
    assert_eq!(GamePhase::Countdown as i32, 4);
    assert_eq!(GamePhase::GreenFlag as i32, 5);
    assert_eq!(GamePhase::FullCourseYellow as i32, 6);
    assert_eq!(GamePhase::SessionStopped as i32, 7);
    assert_eq!(GamePhase::SessionOver as i32, 8);
    assert_eq!(GamePhase::PausedOrReplay as i32, 9);
    Ok(())
}

// ── 12. Wheel data structure ────────────────────────────────────────────────

#[test]
fn rf2_wheel_telemetry_default_zeroed() -> TestResult {
    let w = RF2WheelTelemetry::default();
    assert_eq!(w.suspension_deflection, 0.0);
    assert_eq!(w.brake_temp, 0.0);
    assert_eq!(w.pressure, 0.0);
    assert_eq!(w.temperature, [0.0; 3]);
    assert_eq!(w.wear, 0.0);
    Ok(())
}

// ── 13. Deterministic output ────────────────────────────────────────────────

#[test]
fn rf2_deterministic_normalization() -> TestResult {
    let a = adapter();
    let mut v = default_vehicle();
    v.engine_rpm = 6000.0;
    v.local_vel = [30.0, 0.0, 0.0];
    v.gear = 3;
    let t1 = a.normalize_rf2_data(&v, None, None);
    let t2 = a.normalize_rf2_data(&v, None, None);
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    Ok(())
}
