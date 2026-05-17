//! Deep protocol-level tests for the RaceRoom Racing Experience (R3E) shared-memory adapter.
//!
//! Exercises offset-based field parsing from the R3E SDK v3.4 struct layout,
//! version detection, flag extraction, G-force conventions, tire data,
//! and paused/menu state handling.

use openracing_telemetry_adapters::{RaceRoomAdapter, TelemetryAdapter, TelemetryValue};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── R3E shared memory layout constants ───────────────────────────────────────

const R3E_VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;

const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
#[allow(dead_code)]
const OFF_NUM_GEARS: usize = 1412;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
#[allow(dead_code)]
const OFF_ENGINE_TEMP: usize = 1480;
#[allow(dead_code)]
const OFF_THROTTLE: usize = 1500;
#[allow(dead_code)]
const OFF_BRAKE: usize = 1508;
#[allow(dead_code)]
const OFF_CLUTCH: usize = 1516;
const OFF_STEER_INPUT: usize = 1524;
const OFF_LOCAL_ACCEL_X: usize = 1440;
const OFF_LOCAL_ACCEL_Y: usize = 1444;
const OFF_LOCAL_ACCEL_Z: usize = 1448;
const OFF_POSITION: usize = 988;
const OFF_COMPLETED_LAPS: usize = 1028;
const OFF_LAP_TIME_BEST: usize = 1068;
const OFF_LAP_TIME_PREVIOUS: usize = 1084;
const OFF_LAP_TIME_CURRENT: usize = 1100;
const OFF_DELTA_FRONT: usize = 1124;
const OFF_DELTA_BEHIND: usize = 1128;
const OFF_FLAG_YELLOW: usize = 932;
const OFF_FLAG_BLUE: usize = 964;
const OFF_FLAG_GREEN: usize = 972;
const OFF_FLAG_CHECKERED: usize = 976;
const OFF_IN_PITLANE: usize = 848;
const OFF_PIT_LIMITER: usize = 1572;
const OFF_AID_ABS: usize = 1536;
const OFF_AID_TC: usize = 1540;
const OFF_TIRE_TEMP_FL_CENTER: usize = 1748;
const OFF_TIRE_TEMP_FR_CENTER: usize = 1772;
const OFF_TIRE_TEMP_RL_CENTER: usize = 1796;
const OFF_TIRE_TEMP_RR_CENTER: usize = 1820;
const OFF_TIRE_PRESSURE_FL: usize = 1712;
const OFF_TIRE_PRESSURE_FR: usize = 1716;
const OFF_TIRE_PRESSURE_RL: usize = 1720;
const OFF_TIRE_PRESSURE_RR: usize = 1724;

const G_ACCEL: f32 = 9.80665;

fn make_r3e_base() -> Vec<u8> {
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    set_i32(&mut data, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    // Not paused, not in menus
    set_i32(&mut data, OFF_GAME_PAUSED, 0);
    set_i32(&mut data, OFF_GAME_IN_MENUS, 0);
    data
}

fn set_f32(buf: &mut [u8], offset: usize, val: f32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn set_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn adapter() -> RaceRoomAdapter {
    RaceRoomAdapter::new()
}

// ── 1. Adapter metadata ─────────────────────────────────────────────────────

#[test]
fn r3e_game_id() -> TestResult {
    assert_eq!(adapter().game_id(), "raceroom");
    Ok(())
}

#[test]
fn r3e_update_rate_100hz() -> TestResult {
    assert_eq!(adapter().expected_update_rate(), Duration::from_millis(10));
    Ok(())
}

// ── 2. Version detection ────────────────────────────────────────────────────

#[test]
fn r3e_correct_version_accepted() -> TestResult {
    let data = make_r3e_base();
    let t = adapter().normalize(&data)?;
    // Should parse successfully (zero values for all fields)
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn r3e_wrong_version_rejected() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_VERSION_MAJOR, 2);
    assert!(adapter().normalize(&data).is_err());
    Ok(())
}

#[test]
fn r3e_version_zero_rejected() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_VERSION_MAJOR, 0);
    assert!(adapter().normalize(&data).is_err());
    Ok(())
}

// ── 3. Packet size validation ───────────────────────────────────────────────

#[test]
fn r3e_empty_buffer_rejected() -> TestResult {
    assert!(adapter().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn r3e_too_small_buffer_rejected() -> TestResult {
    let buf = vec![0u8; 100];
    assert!(adapter().normalize(&buf).is_err());
    Ok(())
}

// ── 4. Paused / menu state ──────────────────────────────────────────────────

#[test]
fn r3e_paused_returns_default_telemetry() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_SPEED, 50.0);
    set_i32(&mut data, OFF_GAME_PAUSED, 1);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.rpm, 0.0, "paused → default telemetry");
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn r3e_in_menus_returns_default_telemetry() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_SPEED, 50.0);
    set_i32(&mut data, OFF_GAME_IN_MENUS, 1);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.rpm, 0.0, "in-menus → default telemetry");
    Ok(())
}

// ── 5. RPM conversion (rad/s → RPM) ────────────────────────────────────────

#[test]
fn r3e_rpm_from_rps() -> TestResult {
    let mut data = make_r3e_base();
    // 5000 RPM = 5000 * π/30 ≈ 523.6 rad/s
    let rps = 5000.0f32 * (std::f32::consts::PI / 30.0);
    set_f32(&mut data, OFF_ENGINE_RPS, rps);
    let t = adapter().normalize(&data)?;
    assert!((t.rpm - 5000.0).abs() < 0.1, "rpm={}", t.rpm);
    Ok(())
}

#[test]
fn r3e_max_rpm_from_rps() -> TestResult {
    let mut data = make_r3e_base();
    let max_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
    set_f32(&mut data, OFF_MAX_ENGINE_RPS, max_rps);
    let t = adapter().normalize(&data)?;
    assert!((t.max_rpm - 8000.0).abs() < 0.1, "max_rpm={}", t.max_rpm);
    Ok(())
}

// ── 6. Field extraction accuracy ────────────────────────────────────────────

#[test]
fn r3e_speed_extraction() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_SPEED, 55.0);
    let t = adapter().normalize(&data)?;
    assert!((t.speed_ms - 55.0).abs() < 0.01);
    Ok(())
}

#[test]
fn r3e_negative_speed_becomes_absolute() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_SPEED, -30.0);
    let t = adapter().normalize(&data)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed should be |v|");
    Ok(())
}

#[test]
fn r3e_steering_clamped() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_STEER_INPUT, 2.0);
    let t = adapter().normalize(&data)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "steering clamped to 1.0"
    );
    Ok(())
}

#[test]
fn r3e_gear_forward() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_GEAR, 4);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.gear, 4);
    Ok(())
}

#[test]
fn r3e_gear_reverse() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_GEAR, -1);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn r3e_gear_neutral() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_GEAR, 0);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

// ── 7. Fuel calculation ─────────────────────────────────────────────────────

#[test]
fn r3e_fuel_percent_calculation() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_FUEL_LEFT, 30.0);
    set_f32(&mut data, OFF_FUEL_CAPACITY, 60.0);
    let t = adapter().normalize(&data)?;
    assert!((t.fuel_percent - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn r3e_zero_fuel_capacity_gives_zero_percent() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_FUEL_LEFT, 10.0);
    set_f32(&mut data, OFF_FUEL_CAPACITY, 0.0);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn r3e_fuel_extended_raw_values() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_FUEL_LEFT, 25.0);
    set_f32(&mut data, OFF_FUEL_CAPACITY, 50.0);
    let t = adapter().normalize(&data)?;
    assert_eq!(
        t.extended.get("fuel_left_l"),
        Some(&TelemetryValue::Float(25.0))
    );
    assert_eq!(
        t.extended.get("fuel_capacity_l"),
        Some(&TelemetryValue::Float(50.0))
    );
    Ok(())
}

// ── 8. G-force conventions ──────────────────────────────────────────────────

#[test]
fn r3e_lateral_g_sign_convention() -> TestResult {
    let mut data = make_r3e_base();
    // R3E +X = left, so 1G left → normalized -1G (lateral_g negated)
    set_f32(&mut data, OFF_LOCAL_ACCEL_X, G_ACCEL);
    let t = adapter().normalize(&data)?;
    assert!(
        (t.lateral_g - (-1.0)).abs() < 0.01,
        "lateral_g={}",
        t.lateral_g
    );
    Ok(())
}

#[test]
fn r3e_longitudinal_g_sign_convention() -> TestResult {
    let mut data = make_r3e_base();
    // R3E +Z = back, so 0.5G back → normalized -0.5G (longitudinal_g negated)
    set_f32(&mut data, OFF_LOCAL_ACCEL_Z, 0.5 * G_ACCEL);
    let t = adapter().normalize(&data)?;
    assert!(
        (t.longitudinal_g - (-0.5)).abs() < 0.01,
        "longitudinal_g={}",
        t.longitudinal_g
    );
    Ok(())
}

#[test]
fn r3e_vertical_g_preserves_sign() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_LOCAL_ACCEL_Y, G_ACCEL);
    let t = adapter().normalize(&data)?;
    assert!(
        (t.vertical_g - 1.0).abs() < 0.01,
        "vertical_g={}",
        t.vertical_g
    );
    Ok(())
}

// ── 9. Flags ────────────────────────────────────────────────────────────────

#[test]
fn r3e_yellow_flag() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_FLAG_YELLOW, 1);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.yellow_flag);
    Ok(())
}

#[test]
fn r3e_blue_flag() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_FLAG_BLUE, 1);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.blue_flag);
    Ok(())
}

#[test]
fn r3e_green_flag() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_FLAG_GREEN, 1);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.green_flag);
    Ok(())
}

#[test]
fn r3e_checkered_flag() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_FLAG_CHECKERED, 1);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.checkered_flag);
    Ok(())
}

#[test]
fn r3e_abs_active_at_value_5() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_AID_ABS, 5);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.abs_active);
    Ok(())
}

#[test]
fn r3e_abs_inactive_at_other_values() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_AID_ABS, 1);
    let t = adapter().normalize(&data)?;
    assert!(!t.flags.abs_active, "ABS should only be active at value 5");
    Ok(())
}

#[test]
fn r3e_tc_active_at_value_5() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_AID_TC, 5);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.traction_control);
    Ok(())
}

#[test]
fn r3e_pit_limiter_and_pitlane() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_IN_PITLANE, 1);
    set_i32(&mut data, OFF_PIT_LIMITER, 1);
    let t = adapter().normalize(&data)?;
    assert!(t.flags.in_pits);
    assert!(t.flags.pit_limiter);
    Ok(())
}

// ── 10. Tire data ───────────────────────────────────────────────────────────

#[test]
fn r3e_tire_temps_extraction() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_TIRE_TEMP_FL_CENTER, 90.0);
    set_f32(&mut data, OFF_TIRE_TEMP_FR_CENTER, 92.0);
    set_f32(&mut data, OFF_TIRE_TEMP_RL_CENTER, 88.0);
    set_f32(&mut data, OFF_TIRE_TEMP_RR_CENTER, 91.0);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.tire_temps_c, [90, 92, 88, 91]);
    Ok(())
}

#[test]
fn r3e_tire_pressures_kpa_to_psi() -> TestResult {
    let mut data = make_r3e_base();
    // 170 KPa * 0.14503774 ≈ 24.66 PSI
    set_f32(&mut data, OFF_TIRE_PRESSURE_FL, 170.0);
    set_f32(&mut data, OFF_TIRE_PRESSURE_FR, 170.0);
    set_f32(&mut data, OFF_TIRE_PRESSURE_RL, 170.0);
    set_f32(&mut data, OFF_TIRE_PRESSURE_RR, 170.0);
    let t = adapter().normalize(&data)?;
    assert!((t.tire_pressures_psi[0] - 24.66).abs() < 0.1);
    Ok(())
}

// ── 11. Scoring ─────────────────────────────────────────────────────────────

#[test]
fn r3e_position_and_laps() -> TestResult {
    let mut data = make_r3e_base();
    set_i32(&mut data, OFF_POSITION, 5);
    set_i32(&mut data, OFF_COMPLETED_LAPS, 12);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.position, 5);
    assert_eq!(t.lap, 12);
    Ok(())
}

#[test]
fn r3e_lap_times() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_LAP_TIME_CURRENT, 65.2);
    set_f32(&mut data, OFF_LAP_TIME_BEST, 62.1);
    set_f32(&mut data, OFF_LAP_TIME_PREVIOUS, 63.8);
    let t = adapter().normalize(&data)?;
    assert!((t.current_lap_time_s - 65.2).abs() < 0.01);
    assert!((t.best_lap_time_s - 62.1).abs() < 0.01);
    assert!((t.last_lap_time_s - 63.8).abs() < 0.01);
    Ok(())
}

#[test]
fn r3e_delta_times() -> TestResult {
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_DELTA_FRONT, 1.5);
    set_f32(&mut data, OFF_DELTA_BEHIND, 0.8);
    let t = adapter().normalize(&data)?;
    assert!((t.delta_ahead_s - 1.5).abs() < 0.01);
    assert!((t.delta_behind_s - 0.8).abs() < 0.01);
    Ok(())
}

// ── 12. Deterministic output ────────────────────────────────────────────────

#[test]
fn r3e_same_input_same_output() -> TestResult {
    let a = adapter();
    let mut data = make_r3e_base();
    set_f32(&mut data, OFF_SPEED, 40.0);
    let rps = 6000.0f32 * (std::f32::consts::PI / 30.0);
    set_f32(&mut data, OFF_ENGINE_RPS, rps);
    set_i32(&mut data, OFF_GEAR, 3);

    let t1 = a.normalize(&data)?;
    let t2 = a.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    Ok(())
}
