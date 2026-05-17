//! Snapshot tests for iRacing telemetry adapter.
//!
//! Three scenarios: oval racing at high speed, road-course braking zone,
//! and pit lane entry with pit-road flag.

use openracing_telemetry_adapters::{IRacingAdapter, TelemetryAdapter};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── IRacingData byte offsets (see iracing.rs IRacingData #[repr(C)]) ────────
const OFF_SESSION_TIME: usize = 0;
const OFF_SESSION_FLAGS: usize = 4;
const OFF_SPEED: usize = 8;
const OFF_RPM: usize = 12;
const OFF_GEAR: usize = 16;
// 3 bytes alignment padding at 17..20
const OFF_THROTTLE: usize = 20;
const OFF_BRAKE: usize = 24;
const OFF_STEER_ANGLE: usize = 28;
const OFF_STEER_TORQUE: usize = 32;
const OFF_PCT_TORQUE_SIGN: usize = 36;
const OFF_MAX_FORCE_NM: usize = 40;
const OFF_LIMITER: usize = 44;
const OFF_LF_SLIP: usize = 48;
const OFF_RF_SLIP: usize = 52;
const OFF_LR_SLIP: usize = 56;
const OFF_RR_SLIP: usize = 60;
const OFF_LF_RPS: usize = 64;
const OFF_RF_RPS: usize = 68;
const OFF_LR_RPS: usize = 72;
const OFF_RR_RPS: usize = 76;
const OFF_LAP: usize = 80;
const OFF_BEST_LAP: usize = 84;
const OFF_FUEL_LEVEL: usize = 88;
const OFF_FUEL_PCT: usize = 92;
const OFF_ON_PIT_ROAD: usize = 96;
const OFF_CLUTCH: usize = 100;
const OFF_POSITION: usize = 104;
const OFF_LAST_LAP: usize = 108;
const OFF_CURRENT_LAP: usize = 112;
const OFF_LF_TEMP: usize = 116;
const OFF_RF_TEMP: usize = 120;
const OFF_LR_TEMP: usize = 124;
const OFF_RR_TEMP: usize = 128;
const OFF_LF_PRESS: usize = 132;
const OFF_RF_PRESS: usize = 136;
const OFF_LR_PRESS: usize = 140;
const OFF_RR_PRESS: usize = 144;
const OFF_LAT_ACCEL: usize = 148;
const OFF_LONG_ACCEL: usize = 152;
const OFF_VERT_ACCEL: usize = 156;
const OFF_WATER_TEMP: usize = 160;
const OFF_CAR_PATH: usize = 164;
const OFF_TRACK_NAME: usize = 228;

// Session flag constants (from irsdk_defines.h).
const FLAG_GREEN: u32 = 0x0000_0004;
const FLAG_YELLOW: u32 = 0x0000_0008;

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_string(dst: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&bytes[..len]);
    dst[len] = 0;
}

// ─── Scenario 1: Normal oval racing ─────────────────────────────────────────
// Indycar on a superspeedway: ~322 km/h in 4th gear with moderate throttle,
// slight left steering for oval banking, green flag, lap 12.

#[test]
fn iracing_oval_high_speed_snapshot() -> TestResult {
    let mut buf = vec![0u8; 320];
    write_f32_le(&mut buf, OFF_SESSION_TIME, 245.3);
    write_u32_le(&mut buf, OFF_SESSION_FLAGS, FLAG_GREEN);
    write_f32_le(&mut buf, OFF_SPEED, 89.4); // ~322 km/h
    write_f32_le(&mut buf, OFF_RPM, 8400.0);
    buf[OFF_GEAR] = 4i8 as u8;
    write_f32_le(&mut buf, OFF_THROTTLE, 0.78);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_STEER_ANGLE, -0.02); // slight left for banking
    write_f32_le(&mut buf, OFF_STEER_TORQUE, 8.5);
    write_f32_le(&mut buf, OFF_PCT_TORQUE_SIGN, 0.0);
    write_f32_le(&mut buf, OFF_MAX_FORCE_NM, 0.0);
    write_f32_le(&mut buf, OFF_LIMITER, 0.0);
    write_f32_le(&mut buf, OFF_LF_SLIP, 0.015);
    write_f32_le(&mut buf, OFF_RF_SLIP, 0.018);
    write_f32_le(&mut buf, OFF_LR_SLIP, 0.012);
    write_f32_le(&mut buf, OFF_RR_SLIP, 0.010);
    write_f32_le(&mut buf, OFF_LF_RPS, 43.0);
    write_f32_le(&mut buf, OFF_RF_RPS, 43.0);
    write_f32_le(&mut buf, OFF_LR_RPS, 43.0);
    write_f32_le(&mut buf, OFF_RR_RPS, 43.0);
    write_i32_le(&mut buf, OFF_LAP, 12);
    write_f32_le(&mut buf, OFF_BEST_LAP, 39.8);
    write_f32_le(&mut buf, OFF_FUEL_LEVEL, 22.5);
    write_f32_le(&mut buf, OFF_FUEL_PCT, 0.45);
    write_i32_le(&mut buf, OFF_ON_PIT_ROAD, 0);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_i32_le(&mut buf, OFF_POSITION, 5);
    write_f32_le(&mut buf, OFF_LAST_LAP, 40.1);
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 18.6);
    write_f32_le(&mut buf, OFF_LF_TEMP, 95.0);
    write_f32_le(&mut buf, OFF_RF_TEMP, 98.0);
    write_f32_le(&mut buf, OFF_LR_TEMP, 90.0);
    write_f32_le(&mut buf, OFF_RR_TEMP, 93.0);
    write_f32_le(&mut buf, OFF_LF_PRESS, 179.0);
    write_f32_le(&mut buf, OFF_RF_PRESS, 179.0);
    write_f32_le(&mut buf, OFF_LR_PRESS, 172.0);
    write_f32_le(&mut buf, OFF_RR_PRESS, 172.0);
    write_f32_le(&mut buf, OFF_LAT_ACCEL, 2.5);
    write_f32_le(&mut buf, OFF_LONG_ACCEL, 0.3);
    write_f32_le(&mut buf, OFF_VERT_ACCEL, 9.81);
    write_f32_le(&mut buf, OFF_WATER_TEMP, 88.0);
    write_string(&mut buf[OFF_CAR_PATH..OFF_CAR_PATH + 64], "dallaradw12");
    write_string(
        &mut buf[OFF_TRACK_NAME..OFF_TRACK_NAME + 64],
        "indianapolisoval",
    );

    let adapter = IRacingAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Road course braking zone ───────────────────────────────────
// GT3 car braking hard into a corner: heavy brake, no throttle, downshifted
// to 2nd gear, high lateral G from turn entry, elevated tire slip.

#[test]
fn iracing_road_course_braking_snapshot() -> TestResult {
    let mut buf = vec![0u8; 320];
    write_f32_le(&mut buf, OFF_SESSION_TIME, 180.7);
    write_u32_le(&mut buf, OFF_SESSION_FLAGS, FLAG_GREEN);
    write_f32_le(&mut buf, OFF_SPEED, 38.0); // ~137 km/h, mid-braking
    write_f32_le(&mut buf, OFF_RPM, 5800.0);
    buf[OFF_GEAR] = 2i8 as u8;
    write_f32_le(&mut buf, OFF_THROTTLE, 0.0);
    write_f32_le(&mut buf, OFF_BRAKE, 0.92);
    write_f32_le(&mut buf, OFF_STEER_ANGLE, 0.22); // turning right
    write_f32_le(&mut buf, OFF_STEER_TORQUE, 18.0);
    write_f32_le(&mut buf, OFF_PCT_TORQUE_SIGN, 0.0);
    write_f32_le(&mut buf, OFF_MAX_FORCE_NM, 0.0);
    write_f32_le(&mut buf, OFF_LIMITER, 0.0);
    write_f32_le(&mut buf, OFF_LF_SLIP, 0.08);
    write_f32_le(&mut buf, OFF_RF_SLIP, 0.09);
    write_f32_le(&mut buf, OFF_LR_SLIP, 0.05);
    write_f32_le(&mut buf, OFF_RR_SLIP, 0.06);
    write_f32_le(&mut buf, OFF_LF_RPS, 18.0);
    write_f32_le(&mut buf, OFF_RF_RPS, 18.5);
    write_f32_le(&mut buf, OFF_LR_RPS, 17.5);
    write_f32_le(&mut buf, OFF_RR_RPS, 17.0);
    write_i32_le(&mut buf, OFF_LAP, 5);
    write_f32_le(&mut buf, OFF_BEST_LAP, 98.2);
    write_f32_le(&mut buf, OFF_FUEL_LEVEL, 35.0);
    write_f32_le(&mut buf, OFF_FUEL_PCT, 0.58);
    write_i32_le(&mut buf, OFF_ON_PIT_ROAD, 0);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_i32_le(&mut buf, OFF_POSITION, 8);
    write_f32_le(&mut buf, OFF_LAST_LAP, 99.1);
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 52.3);
    write_f32_le(&mut buf, OFF_LF_TEMP, 102.0);
    write_f32_le(&mut buf, OFF_RF_TEMP, 105.0);
    write_f32_le(&mut buf, OFF_LR_TEMP, 96.0);
    write_f32_le(&mut buf, OFF_RR_TEMP, 99.0);
    write_f32_le(&mut buf, OFF_LF_PRESS, 186.0);
    write_f32_le(&mut buf, OFF_RF_PRESS, 186.0);
    write_f32_le(&mut buf, OFF_LR_PRESS, 179.0);
    write_f32_le(&mut buf, OFF_RR_PRESS, 179.0);
    write_f32_le(&mut buf, OFF_LAT_ACCEL, 8.5);
    write_f32_le(&mut buf, OFF_LONG_ACCEL, -12.0);
    write_f32_le(&mut buf, OFF_VERT_ACCEL, 9.81);
    write_f32_le(&mut buf, OFF_WATER_TEMP, 92.0);
    write_string(&mut buf[OFF_CAR_PATH..OFF_CAR_PATH + 64], "mercedesamggt3");
    write_string(&mut buf[OFF_TRACK_NAME..OFF_TRACK_NAME + 64], "spa");

    let adapter = IRacingAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Pit lane entry ─────────────────────────────────────────────
// Car entering pit lane at pit-road speed limit (~60 km/h). Yellow flag,
// on_pit_road flag set, low RPM, minimal tire slip.

#[test]
fn iracing_pit_lane_entry_snapshot() -> TestResult {
    let mut buf = vec![0u8; 320];
    write_f32_le(&mut buf, OFF_SESSION_TIME, 312.0);
    write_u32_le(&mut buf, OFF_SESSION_FLAGS, FLAG_YELLOW);
    write_f32_le(&mut buf, OFF_SPEED, 16.7); // ~60 km/h pit limiter
    write_f32_le(&mut buf, OFF_RPM, 3500.0);
    buf[OFF_GEAR] = 2i8 as u8;
    write_f32_le(&mut buf, OFF_THROTTLE, 0.15);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_STEER_ANGLE, 0.0);
    write_f32_le(&mut buf, OFF_STEER_TORQUE, 2.0);
    write_f32_le(&mut buf, OFF_PCT_TORQUE_SIGN, 0.0);
    write_f32_le(&mut buf, OFF_MAX_FORCE_NM, 0.0);
    write_f32_le(&mut buf, OFF_LIMITER, 0.0);
    write_f32_le(&mut buf, OFF_LF_SLIP, 0.005);
    write_f32_le(&mut buf, OFF_RF_SLIP, 0.005);
    write_f32_le(&mut buf, OFF_LR_SLIP, 0.003);
    write_f32_le(&mut buf, OFF_RR_SLIP, 0.003);
    write_f32_le(&mut buf, OFF_LF_RPS, 8.0);
    write_f32_le(&mut buf, OFF_RF_RPS, 8.0);
    write_f32_le(&mut buf, OFF_LR_RPS, 8.0);
    write_f32_le(&mut buf, OFF_RR_RPS, 8.0);
    write_i32_le(&mut buf, OFF_LAP, 8);
    write_f32_le(&mut buf, OFF_BEST_LAP, 100.5);
    write_f32_le(&mut buf, OFF_FUEL_LEVEL, 10.0);
    write_f32_le(&mut buf, OFF_FUEL_PCT, 0.18);
    write_i32_le(&mut buf, OFF_ON_PIT_ROAD, 1);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_i32_le(&mut buf, OFF_POSITION, 15);
    write_f32_le(&mut buf, OFF_LAST_LAP, 101.2);
    write_f32_le(&mut buf, OFF_CURRENT_LAP, 65.0);
    write_f32_le(&mut buf, OFF_LF_TEMP, 78.0);
    write_f32_le(&mut buf, OFF_RF_TEMP, 80.0);
    write_f32_le(&mut buf, OFF_LR_TEMP, 74.0);
    write_f32_le(&mut buf, OFF_RR_TEMP, 76.0);
    write_f32_le(&mut buf, OFF_LF_PRESS, 165.0);
    write_f32_le(&mut buf, OFF_RF_PRESS, 165.0);
    write_f32_le(&mut buf, OFF_LR_PRESS, 158.0);
    write_f32_le(&mut buf, OFF_RR_PRESS, 158.0);
    write_f32_le(&mut buf, OFF_LAT_ACCEL, 0.2);
    write_f32_le(&mut buf, OFF_LONG_ACCEL, -0.5);
    write_f32_le(&mut buf, OFF_VERT_ACCEL, 9.81);
    write_f32_le(&mut buf, OFF_WATER_TEMP, 82.0);
    write_string(&mut buf[OFF_CAR_PATH..OFF_CAR_PATH + 64], "corvettez06gt3r");
    write_string(
        &mut buf[OFF_TRACK_NAME..OFF_TRACK_NAME + 64],
        "daytonainternational",
    );

    let adapter = IRacingAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
