//! Snapshot tests for the Codemasters / EA family of telemetry adapters.
//!
//! Covers the eight adapters that share the Codemasters UDP protocol:
//! DiRT 3, DiRT 4, DiRT 5, DiRT Showdown, GRID 2019, GRID Autosport,
//! GRID Legends, and Race Driver: GRID.
//!
//! All adapters except DiRT 5 consume the fixed-layout 264-byte "Mode 1"
//! binary packet (little-endian `f32` at known byte offsets).  DiRT 5 uses
//! the Codemasters custom-UDP spec (mode 1, 15 fields × 4 bytes = 60 bytes).

use openracing_telemetry_adapters::{
    Dirt3Adapter, Dirt4Adapter, Dirt5Adapter, DirtShowdownAdapter, Grid2019Adapter,
    GridAutosportAdapter, GridLegendsAdapter, RaceDriverGridAdapter, TelemetryAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Codemasters Mode 1 packet constants (264 bytes) ─────────────────────────
const MODE1_SIZE: usize = 264;

// Byte offsets – all fields are little-endian `f32` (4 bytes each).
const OFF_WHEEL_SPEED_RL: usize = 100;
const OFF_WHEEL_SPEED_RR: usize = 104;
const OFF_WHEEL_SPEED_FL: usize = 108;
const OFF_WHEEL_SPEED_FR: usize = 112;
const OFF_THROTTLE: usize = 116;
const OFF_STEER: usize = 120;
const OFF_BRAKE: usize = 124;
const OFF_GEAR: usize = 132;
const OFF_GFORCE_LAT: usize = 136;
const OFF_GFORCE_LON: usize = 140;
const OFF_CURRENT_LAP: usize = 144;
const OFF_RPM: usize = 148;
const OFF_CAR_POSITION: usize = 156;
const OFF_FUEL_IN_TANK: usize = 180;
const OFF_FUEL_CAPACITY: usize = 184;
const OFF_IN_PIT: usize = 188;
const OFF_BRAKES_TEMP_FL: usize = 212;
const OFF_TYRES_PRESSURE_FL: usize = 228;
const OFF_LAST_LAP_TIME: usize = 248;
const OFF_MAX_RPM: usize = 252;
const OFF_MAX_GEARS: usize = 260;

/// Build a realistic Codemasters Mode 1 rally-stage packet.
///
/// Simulates mid-stage driving at ~30 m/s in 4th gear, moderate throttle,
/// trail-braking into a corner with lateral load transfer.
fn build_rally_stage_packet() -> Vec<u8> {
    let mut buf = vec![0u8; MODE1_SIZE];
    // Wheel speeds (m/s) – slight rear bias from RWD power
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, 29.5);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, 30.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, 31.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, 30.5);
    // Controls
    write_f32_le(&mut buf, OFF_THROTTLE, 0.65);
    write_f32_le(&mut buf, OFF_STEER, -0.18);
    write_f32_le(&mut buf, OFF_BRAKE, 0.12);
    write_f32_le(&mut buf, OFF_GEAR, 4.0);
    // G-forces
    write_f32_le(&mut buf, OFF_GFORCE_LAT, 1.2);
    write_f32_le(&mut buf, OFF_GFORCE_LON, -0.4);
    // Session
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 0.0); // first lap (0-indexed)
    write_f32_le(&mut buf, OFF_RPM, 5800.0);
    write_f32_le(&mut buf, OFF_MAX_RPM, 7500.0);
    write_f32_le(&mut buf, OFF_CAR_POSITION, 3.0);
    write_f32_le(&mut buf, OFF_FUEL_IN_TANK, 28.0);
    write_f32_le(&mut buf, OFF_FUEL_CAPACITY, 55.0);
    write_f32_le(&mut buf, OFF_IN_PIT, 0.0);
    // Tire temps (brake temp proxy) and pressures
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL, 85.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 4, 82.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 8, 78.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 12, 76.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL, 28.5);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 4, 28.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 8, 26.5);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 12, 26.0);
    write_f32_le(&mut buf, OFF_LAST_LAP_TIME, 0.0); // no completed lap yet
    write_f32_le(&mut buf, OFF_MAX_GEARS, 6.0);
    buf
}

/// Build a realistic Codemasters Mode 1 circuit-race packet.
///
/// Simulates high-speed circuit racing at ~55 m/s in 5th gear, hard on
/// the throttle exiting a corner, mild lateral G.
fn build_circuit_race_packet() -> Vec<u8> {
    let mut buf = vec![0u8; MODE1_SIZE];
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, 54.5);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, 55.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, 56.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, 55.5);
    write_f32_le(&mut buf, OFF_THROTTLE, 0.92);
    write_f32_le(&mut buf, OFF_STEER, 0.05);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_GEAR, 5.0);
    write_f32_le(&mut buf, OFF_GFORCE_LAT, 0.6);
    write_f32_le(&mut buf, OFF_GFORCE_LON, 0.8);
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 4.0); // lap 5 (0-indexed)
    write_f32_le(&mut buf, OFF_RPM, 7200.0);
    write_f32_le(&mut buf, OFF_MAX_RPM, 8500.0);
    write_f32_le(&mut buf, OFF_CAR_POSITION, 2.0);
    write_f32_le(&mut buf, OFF_FUEL_IN_TANK, 18.0);
    write_f32_le(&mut buf, OFF_FUEL_CAPACITY, 60.0);
    write_f32_le(&mut buf, OFF_IN_PIT, 0.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL, 92.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 4, 90.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 8, 80.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 12, 82.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL, 30.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 4, 29.5);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 8, 28.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 12, 28.5);
    write_f32_le(&mut buf, OFF_LAST_LAP_TIME, 78.42);
    write_f32_le(&mut buf, OFF_MAX_GEARS, 7.0);
    buf
}

/// Build a Codemasters Mode 1 derby-event packet.
///
/// Simulates a demolition-derby style event: low speed, 2nd gear, heavy
/// steering lock, aggressive G-forces from impacts.
fn build_derby_packet() -> Vec<u8> {
    let mut buf = vec![0u8; MODE1_SIZE];
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, 12.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, 11.5);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, 13.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, 12.5);
    write_f32_le(&mut buf, OFF_THROTTLE, 1.0);
    write_f32_le(&mut buf, OFF_STEER, -0.72);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_GEAR, 2.0);
    write_f32_le(&mut buf, OFF_GFORCE_LAT, 2.5);
    write_f32_le(&mut buf, OFF_GFORCE_LON, -1.8);
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 2.0);
    write_f32_le(&mut buf, OFF_RPM, 6200.0);
    write_f32_le(&mut buf, OFF_MAX_RPM, 7000.0);
    write_f32_le(&mut buf, OFF_CAR_POSITION, 5.0);
    write_f32_le(&mut buf, OFF_FUEL_IN_TANK, 10.0);
    write_f32_le(&mut buf, OFF_FUEL_CAPACITY, 40.0);
    write_f32_le(&mut buf, OFF_IN_PIT, 0.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL, 110.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 4, 105.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 8, 95.0);
    write_f32_le(&mut buf, OFF_BRAKES_TEMP_FL + 12, 100.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL, 25.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 4, 24.5);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 8, 24.0);
    write_f32_le(&mut buf, OFF_TYRES_PRESSURE_FL + 12, 23.5);
    write_f32_le(&mut buf, OFF_LAST_LAP_TIME, 45.8);
    write_f32_le(&mut buf, OFF_MAX_GEARS, 5.0);
    buf
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Build a DiRT 5 custom-UDP mode 1 packet (15 fields × 4 bytes = 60 bytes).
///
/// Field order: speed, engine_rate, gear(i32), steering_input, throttle_input,
/// brake_input, clutch_input, wheel_patch_speed_{fl,fr,rl,rr},
/// suspension_position_{fl,fr,rl,rr}.
fn build_dirt5_event_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 60];
    write_f32_le(&mut buf, 0, 22.0); // speed (m/s)
    write_f32_le(&mut buf, 4, 680.0); // engine_rate (rad/s) ≈ 6494 RPM
    write_i32_le(&mut buf, 8, 3); // gear
    write_f32_le(&mut buf, 12, 0.15); // steering_input
    write_f32_le(&mut buf, 16, 0.80); // throttle_input
    write_f32_le(&mut buf, 20, 0.0); // brake_input
    write_f32_le(&mut buf, 24, 0.0); // clutch_input
    write_f32_le(&mut buf, 28, 20.5); // wheel_patch_speed_fl
    write_f32_le(&mut buf, 32, 21.0); // wheel_patch_speed_fr
    write_f32_le(&mut buf, 36, 19.8); // wheel_patch_speed_rl
    write_f32_le(&mut buf, 40, 20.2); // wheel_patch_speed_rr
    write_f32_le(&mut buf, 44, 0.04); // suspension_position_fl
    write_f32_le(&mut buf, 48, 0.03); // suspension_position_fr
    write_f32_le(&mut buf, 52, 0.05); // suspension_position_rl
    write_f32_le(&mut buf, 56, 0.045); // suspension_position_rr
    buf
}

// ─── 1. DiRT 3 – rally stage ────────────────────────────────────────────────

#[test]
fn dirt3_rally_stage_snapshot() -> TestResult {
    let buf = build_rally_stage_packet();
    let adapter = Dirt3Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 2. DiRT 4 – rally stage ────────────────────────────────────────────────

#[test]
fn dirt4_rally_stage_snapshot() -> TestResult {
    let buf = build_rally_stage_packet();
    let adapter = Dirt4Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 3. DiRT 5 – event ──────────────────────────────────────────────────────

#[test]
fn dirt5_event_snapshot() -> TestResult {
    let buf = build_dirt5_event_packet();
    let adapter = Dirt5Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 4. DiRT Showdown – derby ────────────────────────────────────────────────

#[test]
fn dirt_showdown_derby_snapshot() -> TestResult {
    let buf = build_derby_packet();
    let adapter = DirtShowdownAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 5. GRID 2019 – race ────────────────────────────────────────────────────

#[test]
fn grid_2019_race_snapshot() -> TestResult {
    let buf = build_circuit_race_packet();
    let adapter = Grid2019Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 6. GRID Autosport – race ───────────────────────────────────────────────

#[test]
fn grid_autosport_race_snapshot() -> TestResult {
    let buf = build_circuit_race_packet();
    let adapter = GridAutosportAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 7. GRID Legends – race ─────────────────────────────────────────────────

#[test]
fn grid_legends_race_snapshot() -> TestResult {
    let buf = build_circuit_race_packet();
    let adapter = GridLegendsAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 8. Race Driver: GRID – race ────────────────────────────────────────────

#[test]
fn race_driver_grid_race_snapshot() -> TestResult {
    let buf = build_circuit_race_packet();
    let adapter = RaceDriverGridAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
