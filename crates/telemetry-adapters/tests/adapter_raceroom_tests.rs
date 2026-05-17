//! Deep tests for the RaceRoom Racing Experience telemetry adapter.

use openracing_telemetry_adapters::{RaceRoomAdapter, TelemetryAdapter, TelemetryValue};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── R3E shared memory layout constants ──────────────────────────────────

const R3E_VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;
const G_ACCEL: f32 = 9.80665;

const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
const OFF_THROTTLE: usize = 1500;
const OFF_BRAKE: usize = 1508;
const OFF_CLUTCH: usize = 1516;
const OFF_STEER_INPUT: usize = 1524;
const OFF_LOCAL_ACCEL_X: usize = 1440;
const OFF_LOCAL_ACCEL_Y: usize = 1444;
const OFF_LOCAL_ACCEL_Z: usize = 1448;
const OFF_NUM_GEARS: usize = 1412;
const OFF_ENGINE_TEMP: usize = 1480;
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

// ── Helpers ─────────────────────────────────────────────────────────────

fn write_f32(data: &mut [u8], offset: usize, value: f32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(data: &mut [u8], offset: usize, value: i32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn make_r3e_memory(
    rpm: f32,
    speed: f32,
    steering: f32,
    throttle: f32,
    brake: f32,
    gear: i32,
) -> Vec<u8> {
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_i32(&mut data, OFF_GAME_PAUSED, 0);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 0);

    let rps = rpm * (std::f32::consts::PI / 30.0);
    let max_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
    write_f32(&mut data, OFF_ENGINE_RPS, rps);
    write_f32(&mut data, OFF_MAX_ENGINE_RPS, max_rps);
    write_f32(&mut data, OFF_FUEL_LEFT, 30.0);
    write_f32(&mut data, OFF_FUEL_CAPACITY, 60.0);
    write_f32(&mut data, OFF_SPEED, speed);
    write_f32(&mut data, OFF_STEER_INPUT, steering);
    write_f32(&mut data, OFF_THROTTLE, throttle);
    write_f32(&mut data, OFF_BRAKE, brake);
    write_f32(&mut data, OFF_CLUTCH, 0.0);
    write_i32(&mut data, OFF_GEAR, gear);

    write_i32(&mut data, OFF_NUM_GEARS, 6);
    write_f32(&mut data, OFF_ENGINE_TEMP, 95.0);
    write_f32(&mut data, OFF_LOCAL_ACCEL_X, G_ACCEL);
    write_f32(&mut data, OFF_LOCAL_ACCEL_Y, G_ACCEL);
    write_f32(&mut data, OFF_LOCAL_ACCEL_Z, 0.3 * G_ACCEL);

    write_i32(&mut data, OFF_POSITION, 3);
    write_i32(&mut data, OFF_COMPLETED_LAPS, 5);
    write_f32(&mut data, OFF_LAP_TIME_CURRENT, 62.5);
    write_f32(&mut data, OFF_LAP_TIME_BEST, 60.1);
    write_f32(&mut data, OFF_LAP_TIME_PREVIOUS, 61.3);
    write_f32(&mut data, OFF_DELTA_FRONT, 1.2);
    write_f32(&mut data, OFF_DELTA_BEHIND, 0.8);
    write_i32(&mut data, OFF_FLAG_GREEN, 1);

    write_f32(&mut data, OFF_TIRE_TEMP_FL_CENTER, 90.0);
    write_f32(&mut data, OFF_TIRE_TEMP_FR_CENTER, 92.0);
    write_f32(&mut data, OFF_TIRE_TEMP_RL_CENTER, 88.0);
    write_f32(&mut data, OFF_TIRE_TEMP_RR_CENTER, 91.0);

    write_f32(&mut data, OFF_TIRE_PRESSURE_FL, 170.0);
    write_f32(&mut data, OFF_TIRE_PRESSURE_FR, 172.0);
    write_f32(&mut data, OFF_TIRE_PRESSURE_RL, 168.0);
    write_f32(&mut data, OFF_TIRE_PRESSURE_RR, 171.0);
    data
}

// ── Shared memory parsing tests ─────────────────────────────────────────

#[test]
fn raceroom_parse_basic_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 5000.0).abs() < 1.0);
    assert!((result.speed_ms - 50.0).abs() < 0.01);
    assert!((result.steering_angle - 0.3).abs() < 0.01);
    assert!((result.throttle - 0.7).abs() < 0.01);
    assert_eq!(result.gear, 3);
    Ok(())
}

#[test]
fn raceroom_reject_wrong_version() {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    write_i32(&mut data, OFF_VERSION_MAJOR, 1);
    assert!(adapter.normalize(&data).is_err());
}

#[test]
fn raceroom_reject_too_small_buffer() {
    let adapter = RaceRoomAdapter::new();
    let data = vec![0u8; 100];
    assert!(adapter.normalize(&data).is_err());
}

#[test]
fn raceroom_paused_returns_default_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    write_i32(&mut data, OFF_GAME_PAUSED, 1);
    let result = adapter.normalize(&data)?;
    assert_eq!(
        result.rpm, 0.0,
        "paused game should return default telemetry"
    );
    Ok(())
}

#[test]
fn raceroom_in_menus_returns_default_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 1);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.rpm, 0.0, "in-menus should return default telemetry");
    Ok(())
}

// ── Driver standings tests ──────────────────────────────────────────────

#[test]
fn raceroom_position_and_laps() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.position, 3);
    assert_eq!(result.lap, 5);
    Ok(())
}

#[test]
fn raceroom_delta_times() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.delta_ahead_s - 1.2).abs() < 0.01);
    assert!((result.delta_behind_s - 0.8).abs() < 0.01);
    Ok(())
}

#[test]
fn raceroom_flags_all_active() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    write_i32(&mut data, OFF_FLAG_YELLOW, 1);
    write_i32(&mut data, OFF_FLAG_BLUE, 1);
    write_i32(&mut data, OFF_FLAG_CHECKERED, 1);
    write_i32(&mut data, OFF_IN_PITLANE, 1);
    write_i32(&mut data, OFF_PIT_LIMITER, 1);
    write_i32(&mut data, OFF_AID_ABS, 5);
    write_i32(&mut data, OFF_AID_TC, 5);
    let result = adapter.normalize(&data)?;
    assert!(result.flags.yellow_flag);
    assert!(result.flags.blue_flag);
    assert!(result.flags.green_flag);
    assert!(result.flags.checkered_flag);
    assert!(result.flags.in_pits);
    assert!(result.flags.pit_limiter);
    assert!(result.flags.abs_active);
    assert!(result.flags.traction_control);
    Ok(())
}

#[test]
fn raceroom_flags_none_active() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    // Override green flag default to inactive
    write_i32(&mut data, OFF_FLAG_GREEN, 0);
    let result = adapter.normalize(&data)?;
    assert!(!result.flags.yellow_flag);
    assert!(!result.flags.blue_flag);
    assert!(!result.flags.green_flag);
    assert!(!result.flags.checkered_flag);
    assert!(!result.flags.in_pits);
    Ok(())
}

// ── Sector times tests ──────────────────────────────────────────────────

#[test]
fn raceroom_lap_timing() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.current_lap_time_s - 62.5).abs() < 0.01);
    assert!((result.best_lap_time_s - 60.1).abs() < 0.01);
    assert!((result.last_lap_time_s - 61.3).abs() < 0.01);
    Ok(())
}

#[test]
fn raceroom_g_forces() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    // R3E: +X=left → negated → -1.0G lateral
    assert!((result.lateral_g - (-1.0)).abs() < 0.01);
    // R3E: +Y=up → same sign → 1.0G vertical
    assert!((result.vertical_g - 1.0).abs() < 0.01);
    // R3E: +Z=back → negated → -0.3G longitudinal
    assert!((result.longitudinal_g - (-0.3)).abs() < 0.01);
    Ok(())
}

#[test]
fn raceroom_tire_temps() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.tire_temps_c, [90, 92, 88, 91]);
    Ok(())
}

#[test]
fn raceroom_tire_pressures_kpa_to_psi() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    // 170 KPa * 0.14503774 ≈ 24.66 PSI
    assert!((result.tire_pressures_psi[0] - 170.0 * 0.14503774).abs() < 0.1);
    assert!(result.tire_pressures_psi[1] > 0.0);
    assert!(result.tire_pressures_psi[2] > 0.0);
    assert!(result.tire_pressures_psi[3] > 0.0);
    Ok(())
}

#[test]
fn raceroom_fuel_percent() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
    let result = adapter.normalize(&data)?;
    // fuel_left=30, fuel_capacity=60 → 0.5
    assert!((result.fuel_percent - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn raceroom_fuel_extended_fields() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
    let result = adapter.normalize(&data)?;
    assert_eq!(
        result.get_extended("fuel_left_l"),
        Some(&TelemetryValue::Float(30.0))
    );
    assert_eq!(
        result.get_extended("fuel_capacity_l"),
        Some(&TelemetryValue::Float(60.0))
    );
    Ok(())
}

#[test]
fn raceroom_engine_temp_and_num_gears() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.engine_temp_c - 95.0).abs() < 0.01);
    assert_eq!(result.num_gears, 6);
    Ok(())
}

#[test]
fn raceroom_steering_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(5000.0, 50.0, 2.5, 0.5, 0.0, 3);
    // Override steering to out-of-range value
    write_f32(&mut data, OFF_STEER_INPUT, 2.5);
    let result = adapter.normalize(&data)?;
    assert!(
        (result.steering_angle - 1.0).abs() < 0.01,
        "steering should be clamped to 1.0"
    );
    Ok(())
}

#[test]
fn raceroom_adapter_game_id_and_rate() {
    let adapter = RaceRoomAdapter::new();
    assert_eq!(adapter.game_id(), "raceroom");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(10));
}

#[test]
fn raceroom_zero_fuel_capacity_gives_zero_percent() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
    write_f32(&mut data, OFF_FUEL_CAPACITY, 0.0);
    write_f32(&mut data, OFF_FUEL_LEFT, 0.0);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.fuel_percent, 0.0);
    Ok(())
}
