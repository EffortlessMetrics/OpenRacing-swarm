//! Snapshot tests for remaining adapters without dedicated snapshot coverage.
//!
//! Covers: Forza Horizon (freeroam), LFS (race with dashboard flags),
//! ACC2 (stub adapter), AMS2 (race session with realistic values).

use openracing_telemetry_adapters::{
    ACC2Adapter, AMS2Adapter, ForzaHorizon4Adapter, ForzaHorizon5Adapter, LFSAdapter,
    TelemetryAdapter, ams2::AMS2SharedMemory,
};
use std::mem;

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_i32_le(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn struct_to_bytes<T: Sized>(s: &T) -> Vec<u8> {
    let size = mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    // SAFETY: T is a plain-data struct with no padding concerns for test use.
    unsafe {
        std::ptr::copy_nonoverlapping(s as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    buf
}

// ─── Forza Horizon: freeroam scenario (CarDash 311 bytes) ────────────────────
// Cruising on an open road at ~120 km/h in 5th gear, light throttle, no braking,
// comfortable engine temps, half tank of fuel.

const CARDASH_SIZE: usize = 311;

// Sled offsets
const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_VEL_X: usize = 32;
const OFF_VEL_Y: usize = 36;
const OFF_VEL_Z: usize = 40;

// CarDash offsets (no horizon offset for standard 311-byte packets)
const OFF_DASH_SPEED: usize = 244;
const OFF_DASH_FUEL: usize = 276;
const OFF_DASH_ACCEL: usize = 303;
const OFF_DASH_BRAKE: usize = 304;
const OFF_DASH_CLUTCH: usize = 305;
const OFF_DASH_GEAR: usize = 307;
const OFF_DASH_STEER: usize = 308;

fn make_fh_freeroam_cardash() -> Vec<u8> {
    let mut buf = vec![0u8; CARDASH_SIZE];
    // Sled section
    write_i32_le(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32_le(&mut buf, OFF_ENGINE_MAX_RPM, 7500.0);
    write_f32_le(&mut buf, OFF_CURRENT_RPM, 3200.0);
    write_f32_le(&mut buf, OFF_VEL_X, 33.3); // ~120 km/h
    write_f32_le(&mut buf, OFF_VEL_Y, 0.0);
    write_f32_le(&mut buf, OFF_VEL_Z, 0.5);

    // CarDash section
    write_f32_le(&mut buf, OFF_DASH_SPEED, 33.3);
    write_f32_le(&mut buf, OFF_DASH_FUEL, 0.52);
    buf[OFF_DASH_ACCEL] = 64; // ~25% throttle (cruising)
    buf[OFF_DASH_BRAKE] = 0;
    buf[OFF_DASH_CLUTCH] = 0;
    buf[OFF_DASH_GEAR] = 6; // 5th gear (0=R, 1=N, 2=1st, …)
    buf[OFF_DASH_STEER] = 3_i8 as u8; // slight right
    buf
}

#[test]
fn forza_horizon_4_freeroam_snapshot() -> TestResult {
    let adapter = ForzaHorizon4Adapter::new();
    let normalized = adapter.normalize(&make_fh_freeroam_cardash())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn forza_horizon_5_freeroam_snapshot() -> TestResult {
    let adapter = ForzaHorizon5Adapter::new();
    let normalized = adapter.normalize(&make_fh_freeroam_cardash())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── LFS: race with dashboard flags ─────────────────────────────────────────
// Hot-lap scenario: 4th gear, high RPM with shift light and ABS active,
// moderate fuel, partial braking into a corner.

const LFS_OUTGAUGE_SIZE: usize = 92;
const LFS_OFF_GEAR: usize = 10;
const LFS_OFF_SPEED: usize = 12;
const LFS_OFF_RPM: usize = 16;
const LFS_OFF_TURBO: usize = 20;
const LFS_OFF_ENG_TEMP: usize = 24;
const LFS_OFF_FUEL: usize = 28;
const LFS_OFF_OIL_PRESSURE: usize = 32;
const LFS_OFF_OIL_TEMP: usize = 36;
const LFS_OFF_SHOW_LIGHTS: usize = 44;
const LFS_OFF_THROTTLE: usize = 48;
const LFS_OFF_BRAKE: usize = 52;
const LFS_OFF_CLUTCH: usize = 56;

// Dashboard light flags from LFS InSim.txt
const DL_SHIFT: u32 = 0x0001;
const DL_ABS: u32 = 0x0400;

fn make_lfs_race_packet() -> Vec<u8> {
    let mut buf = vec![0u8; LFS_OUTGAUGE_SIZE];
    buf[LFS_OFF_GEAR] = 5; // raw 5 → normalized 4th gear
    write_f32_le(&mut buf, LFS_OFF_SPEED, 45.0); // ~162 km/h
    write_f32_le(&mut buf, LFS_OFF_RPM, 7200.0);
    write_f32_le(&mut buf, LFS_OFF_TURBO, 0.8);
    write_f32_le(&mut buf, LFS_OFF_ENG_TEMP, 98.0);
    write_f32_le(&mut buf, LFS_OFF_FUEL, 0.42);
    write_f32_le(&mut buf, LFS_OFF_OIL_PRESSURE, 4.5);
    write_f32_le(&mut buf, LFS_OFF_OIL_TEMP, 105.0);
    write_u32_le(&mut buf, LFS_OFF_SHOW_LIGHTS, DL_SHIFT | DL_ABS);
    write_f32_le(&mut buf, LFS_OFF_THROTTLE, 0.3);
    write_f32_le(&mut buf, LFS_OFF_BRAKE, 0.65);
    write_f32_le(&mut buf, LFS_OFF_CLUTCH, 0.0);
    buf
}

#[test]
fn lfs_race_with_flags_snapshot() -> TestResult {
    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&make_lfs_race_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── ACC2: stub adapter ─────────────────────────────────────────────────────
// ACC2 is a stub — normalize always returns default. Verify the snapshot
// captures the zeroed output for any input.

#[test]
fn acc2_stub_default_snapshot() -> TestResult {
    let adapter = ACC2Adapter::new();
    let normalized = adapter.normalize(&[])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn acc2_stub_arbitrary_input_snapshot() -> TestResult {
    let adapter = ACC2Adapter::new();
    let normalized = adapter.normalize(&[0xDE, 0xAD, 0xBE, 0xEF])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── AMS2: race session with realistic values ────────────────────────────────
// Mid-race at Interlagos: 5th gear, high speed, moderate fuel, active tyre data.

fn make_ams2_race_data() -> AMS2SharedMemory {
    let mut data = AMS2SharedMemory::default();
    data.version = 9;
    data.game_state = 2; // GAME_INGAME_PLAYING
    data.session_state = 5; // SESSION_RACE
    data.race_state = 2; // RACESTATE_RACING

    data.laps_completed = 4;
    data.laps_in_event = 15;
    data.best_lap_time = 78.5;
    data.last_lap_time = 79.2;

    data.speed = 55.0; // ~198 km/h
    data.rpm = 9500.0;
    data.max_rpm = 12000.0;
    data.gear = 5;
    data.num_gears = 6;

    data.throttle = 0.85;
    data.brake = 0.0;
    data.clutch = 0.0;
    data.steering = -0.12;

    data.fuel_level = 30.0;
    data.fuel_capacity = 80.0;

    data.water_temp_celsius = 92.0;
    data.oil_temp_celsius = 110.0;
    data.oil_pressure_kpa = 450.0;

    data.tyre_temp = [95.0, 97.0, 88.0, 90.0];
    data.brake_temp_celsius = [380.0, 400.0, 320.0, 340.0];

    data
}

#[test]
fn ams2_race_session_snapshot() -> TestResult {
    let adapter = AMS2Adapter::new();
    let raw = struct_to_bytes(&make_ams2_race_data());
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
