//! Snapshot tests for Forza Motorsport / Forza Horizon telemetry adapters.
//!
//! Three scenarios: normal race (CarDash), standstill idle (Sled), and
//! edge-case extreme values using the FH4-specific 324-byte packet.

use openracing_telemetry_adapters::{ForzaAdapter, ForzaHorizon4Adapter, TelemetryAdapter};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Sled byte offsets (first 232 bytes, shared by all formats) ───────────────
const OFF_IS_RACE_ON: usize = 0; // i32
const OFF_ENGINE_MAX_RPM: usize = 8; // f32
const OFF_CURRENT_RPM: usize = 16; // f32
const OFF_ACCEL_X: usize = 20; // f32 lateral accel (m/s²)
const OFF_ACCEL_Z: usize = 28; // f32 longitudinal accel (m/s²)
const OFF_VEL_X: usize = 32; // f32
const OFF_VEL_Y: usize = 36; // f32
const OFF_VEL_Z: usize = 40; // f32
const OFF_WHEEL_SPEED_FL: usize = 100; // f32 rad/s
const OFF_WHEEL_SPEED_FR: usize = 104;
const OFF_WHEEL_SPEED_RL: usize = 108;
const OFF_WHEEL_SPEED_RR: usize = 112;
const OFF_SLIP_ANGLE_FL: usize = 164; // f32
const OFF_SLIP_ANGLE_FR: usize = 168;
const OFF_SLIP_ANGLE_RL: usize = 172;
const OFF_SLIP_ANGLE_RR: usize = 176;
const OFF_SUSP_TRAVEL_FL: usize = 196; // f32 meters
const OFF_SUSP_TRAVEL_FR: usize = 200;
const OFF_SUSP_TRAVEL_RL: usize = 204;
const OFF_SUSP_TRAVEL_RR: usize = 208;

// ── CarDash extension offsets (base, no horizon offset) ──────────────────────
const OFF_DASH_SPEED: usize = 244; // f32 m/s
const OFF_DASH_TIRE_TEMP_FL: usize = 256; // f32 Fahrenheit
const OFF_DASH_TIRE_TEMP_FR: usize = 260;
const OFF_DASH_TIRE_TEMP_RL: usize = 264;
const OFF_DASH_TIRE_TEMP_RR: usize = 268;
const OFF_DASH_FUEL: usize = 276; // f32 0.0–1.0
const OFF_DASH_BEST_LAP: usize = 284; // f32 seconds
const OFF_DASH_LAST_LAP: usize = 288;
const OFF_DASH_CUR_LAP: usize = 292;
const OFF_DASH_LAP_NUMBER: usize = 300; // u16
const OFF_DASH_RACE_POS: usize = 302; // u8
const OFF_DASH_ACCEL: usize = 303; // u8 throttle (0-255)
const OFF_DASH_BRAKE: usize = 304; // u8 (0-255)
const OFF_DASH_CLUTCH: usize = 305; // u8 (0-255)
const OFF_DASH_GEAR: usize = 307; // u8 (0=R, 1=N, 2=1st …)
const OFF_DASH_STEER: usize = 308; // i8 (-127..127)

const SLED_SIZE: usize = 232;
const CARDASH_SIZE: usize = 311;
const FH4_CARDASH_SIZE: usize = 324;

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u16_le(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

// ─── Scenario 1: Normal race (CarDash, 311 bytes) ───────────────────────────
// Mid-race in 4th gear at ~180 km/h, partial throttle and light braking,
// moderate G-forces, realistic tire temps, half fuel, lap 3 of a race.

#[test]
fn forza_normal_race_snapshot() -> TestResult {
    let mut buf = vec![0u8; CARDASH_SIZE];

    // Sled section
    write_i32_le(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32_le(&mut buf, OFF_ENGINE_MAX_RPM, 8500.0);
    write_f32_le(&mut buf, OFF_CURRENT_RPM, 6800.0);
    write_f32_le(&mut buf, OFF_VEL_X, 30.0);
    write_f32_le(&mut buf, OFF_VEL_Y, 0.5);
    write_f32_le(&mut buf, OFF_VEL_Z, 38.0);
    write_f32_le(&mut buf, OFF_ACCEL_X, 8.0); // ~0.82G lateral
    write_f32_le(&mut buf, OFF_ACCEL_Z, 3.0); // ~0.31G longitudinal
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, 65.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, 66.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, 64.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, 64.5);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_FL, 0.03);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_FR, 0.04);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_RL, 0.02);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_RR, 0.025);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_FL, 0.08);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_FR, 0.07);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_RL, 0.09);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_RR, 0.085);

    // CarDash extension
    write_f32_le(&mut buf, OFF_DASH_SPEED, 48.5); // ~175 km/h
    buf[OFF_DASH_ACCEL] = 178; // ~70% throttle
    buf[OFF_DASH_BRAKE] = 25; // ~10% brake
    buf[OFF_DASH_CLUTCH] = 0;
    buf[OFF_DASH_GEAR] = 5; // 4th gear (byte 5 → gear 4)
    buf[OFF_DASH_STEER] = 15_i8 as u8; // slight right
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_FL, 195.0); // ~90°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_FR, 200.0); // ~93°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_RL, 185.0); // ~85°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_RR, 190.0); // ~88°C
    write_f32_le(&mut buf, OFF_DASH_FUEL, 0.48);
    write_f32_le(&mut buf, OFF_DASH_BEST_LAP, 95.4);
    write_f32_le(&mut buf, OFF_DASH_LAST_LAP, 96.1);
    write_f32_le(&mut buf, OFF_DASH_CUR_LAP, 42.7);
    write_u16_le(&mut buf, OFF_DASH_LAP_NUMBER, 3);
    buf[OFF_DASH_RACE_POS] = 5;

    let adapter = ForzaAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Standstill / idle (Sled, 232 bytes) ───────────────────────
// Car on the grid, engine idling at 850 RPM, no movement, race active.

#[test]
fn forza_standstill_idle_snapshot() -> TestResult {
    let mut buf = vec![0u8; SLED_SIZE];

    write_i32_le(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32_le(&mut buf, OFF_ENGINE_MAX_RPM, 7500.0);
    write_f32_le(&mut buf, OFF_CURRENT_RPM, 850.0);
    // All velocities, accelerations, wheel speeds, slip angles = 0 (default)

    let adapter = ForzaAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Edge case – FH4 extreme values (324 bytes) ────────────────
// Forza Horizon 4 packet (324 bytes) with extreme/boundary values:
// redline RPM, reverse gear, full brake, max steering, near-boiling tires,
// last lap of a race with very fast laps.
// FH4 has a 12-byte HorizonPlaceholder after the Sled section, so all
// dash offsets are shifted by +12.

#[test]
fn forza_fh4_edge_case_snapshot() -> TestResult {
    let mut buf = vec![0u8; FH4_CARDASH_SIZE];
    let ho: usize = 12; // FH4 horizon offset for dash fields

    // Sled section (same offsets as FM7)
    write_i32_le(&mut buf, OFF_IS_RACE_ON, 1);
    write_f32_le(&mut buf, OFF_ENGINE_MAX_RPM, 9000.0);
    write_f32_le(&mut buf, OFF_CURRENT_RPM, 9000.0); // at redline
    write_f32_le(&mut buf, OFF_VEL_X, 60.0);
    write_f32_le(&mut buf, OFF_VEL_Y, 2.0);
    write_f32_le(&mut buf, OFF_VEL_Z, 50.0);
    write_f32_le(&mut buf, OFF_ACCEL_X, 25.0); // ~2.55G lateral
    write_f32_le(&mut buf, OFF_ACCEL_Z, -15.0); // ~-1.53G braking
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, 120.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, 118.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, 125.0);
    write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, 130.0);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_FL, 0.35);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_FR, 0.40);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_RL, 0.50);
    write_f32_le(&mut buf, OFF_SLIP_ANGLE_RR, 0.55);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_FL, 0.20);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_FR, 0.18);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_RL, 0.22);
    write_f32_le(&mut buf, OFF_SUSP_TRAVEL_RR, 0.25);

    // CarDash extension (shifted by +12 for FH4)
    write_f32_le(&mut buf, OFF_DASH_SPEED + ho, 78.1); // ~281 km/h
    buf[OFF_DASH_ACCEL + ho] = 0; // no throttle
    buf[OFF_DASH_BRAKE + ho] = 255; // full brake
    buf[OFF_DASH_CLUTCH + ho] = 255; // full clutch
    buf[OFF_DASH_GEAR + ho] = 0; // reverse
    buf[OFF_DASH_STEER + ho] = (-127_i8) as u8; // full left lock
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_FL + ho, 400.0); // ~204°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_FR + ho, 410.0); // ~210°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_RL + ho, 380.0); // ~193°C
    write_f32_le(&mut buf, OFF_DASH_TIRE_TEMP_RR + ho, 420.0); // ~216°C
    write_f32_le(&mut buf, OFF_DASH_FUEL + ho, 0.01); // nearly empty
    write_f32_le(&mut buf, OFF_DASH_BEST_LAP + ho, 58.2);
    write_f32_le(&mut buf, OFF_DASH_LAST_LAP + ho, 59.8);
    write_f32_le(&mut buf, OFF_DASH_CUR_LAP + ho, 12.3);
    write_u16_le(&mut buf, OFF_DASH_LAP_NUMBER + ho, 20);
    buf[OFF_DASH_RACE_POS + ho] = 1;

    let adapter = ForzaHorizon4Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
