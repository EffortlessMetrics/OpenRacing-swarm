//! Snapshot tests for Assetto Corsa (original) telemetry adapter.
//!
//! Three scenarios: race pace, braking zone, and pit stop.
//! Packets use the AC RTCarInfo struct (328 bytes, little-endian UDP).

use openracing_telemetry_adapters::{AssettoCorsaAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// AC RTCarInfo struct size.
const AC_RTCARINFO_SIZE: usize = 328;

// Byte offsets in the AC RTCarInfo struct (little-endian).
const OFF_SPEED_MS: usize = 16;
const OFF_ABS_IN_ACTION: usize = 21;
const OFF_TC_IN_ACTION: usize = 22;
const OFF_IN_PIT: usize = 24;
const OFF_ENGINE_LIMITER: usize = 25;
const OFF_ACCG_VERTICAL: usize = 28;
const OFF_ACCG_HORIZONTAL: usize = 32;
const OFF_ACCG_FRONTAL: usize = 36;
const OFF_LAP_TIME: usize = 40;
const OFF_LAST_LAP: usize = 44;
const OFF_BEST_LAP: usize = 48;
const OFF_LAP_COUNT: usize = 52;
const OFF_GAS: usize = 56;
const OFF_BRAKE: usize = 60;
const OFF_CLUTCH: usize = 64;
const OFF_RPM: usize = 68;
const OFF_STEER: usize = 72;
const OFF_GEAR: usize = 76;
const OFF_SLIP_ANGLE_FL: usize = 100;
const OFF_SLIP_RATIO_FL: usize = 132;

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ─── Scenario 1: Normal race pace ───────────────────────────────────────────
// 4th gear at ~200 km/h, lap 5, solid times, slight right turn, ABS flicker.

#[test]
fn ac_normal_race_pace_snapshot() -> TestResult {
    let mut buf = vec![0u8; AC_RTCARINFO_SIZE];
    write_f32(&mut buf, OFF_SPEED_MS, 55.56); // ~200 km/h
    write_f32(&mut buf, OFF_RPM, 7200.0);
    write_i32(&mut buf, OFF_GEAR, 5); // AC 5 → normalized 4th
    write_f32(&mut buf, OFF_STEER, 0.08); // slight right
    write_f32(&mut buf, OFF_GAS, 0.92);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_CLUTCH, 0.0);
    // G-forces: gentle cornering
    write_f32(&mut buf, OFF_ACCG_VERTICAL, 1.0);
    write_f32(&mut buf, OFF_ACCG_HORIZONTAL, 0.45);
    write_f32(&mut buf, OFF_ACCG_FRONTAL, 0.12);
    // Lap timing
    write_i32(&mut buf, OFF_LAP_TIME, 38_200); // 0:38.200
    write_i32(&mut buf, OFF_LAST_LAP, 92_400); // 1:32.400
    write_i32(&mut buf, OFF_BEST_LAP, 91_800); // 1:31.800
    write_i32(&mut buf, OFF_LAP_COUNT, 5);
    // Flags
    buf[OFF_ABS_IN_ACTION] = 1;
    // Slip angles (mild)
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL, 0.02);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 4, 0.03);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 8, 0.01);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 12, 0.015);
    // Slip ratios (low)
    write_f32(&mut buf, OFF_SLIP_RATIO_FL, 0.01);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 4, 0.012);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 8, 0.008);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 12, 0.009);

    let adapter = AssettoCorsaAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Braking zone ──────────────────────────────────────────────
// Heavy braking from high speed, downshift to 3rd, ABS active, high slip.

#[test]
fn ac_braking_zone_snapshot() -> TestResult {
    let mut buf = vec![0u8; AC_RTCARINFO_SIZE];
    write_f32(&mut buf, OFF_SPEED_MS, 41.67); // ~150 km/h (decelerating)
    write_f32(&mut buf, OFF_RPM, 6800.0);
    write_i32(&mut buf, OFF_GEAR, 4); // AC 4 → normalized 3rd
    write_f32(&mut buf, OFF_STEER, -0.15); // turning left into corner
    write_f32(&mut buf, OFF_GAS, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 0.95); // hard braking
    write_f32(&mut buf, OFF_CLUTCH, 0.0);
    // G-forces: heavy deceleration
    write_f32(&mut buf, OFF_ACCG_VERTICAL, 1.01);
    write_f32(&mut buf, OFF_ACCG_HORIZONTAL, -0.8);
    write_f32(&mut buf, OFF_ACCG_FRONTAL, -1.4);
    // Lap timing
    write_i32(&mut buf, OFF_LAP_TIME, 55_900); // 0:55.900
    write_i32(&mut buf, OFF_LAST_LAP, 93_100); // 1:33.100
    write_i32(&mut buf, OFF_BEST_LAP, 91_800); // 1:31.800
    write_i32(&mut buf, OFF_LAP_COUNT, 6);
    // Flags: ABS + TC
    buf[OFF_ABS_IN_ACTION] = 1;
    buf[OFF_TC_IN_ACTION] = 1;
    // Slip angles (elevated under braking)
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL, 0.18);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 4, 0.16);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 8, 0.22);
    write_f32(&mut buf, OFF_SLIP_ANGLE_FL + 12, 0.20);
    // Slip ratios (high under braking)
    write_f32(&mut buf, OFF_SLIP_RATIO_FL, 0.12);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 4, 0.11);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 8, 0.14);
    write_f32(&mut buf, OFF_SLIP_RATIO_FL + 12, 0.13);

    let adapter = AssettoCorsaAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Pit stop ──────────────────────────────────────────────────
// Crawling through pit lane in 1st gear, pit flag set, engine limiter on.

#[test]
fn ac_pit_stop_snapshot() -> TestResult {
    let mut buf = vec![0u8; AC_RTCARINFO_SIZE];
    write_f32(&mut buf, OFF_SPEED_MS, 16.67); // ~60 km/h pit limiter
    write_f32(&mut buf, OFF_RPM, 3200.0);
    write_i32(&mut buf, OFF_GEAR, 2); // AC 2 → normalized 1st
    write_f32(&mut buf, OFF_STEER, 0.0); // straight
    write_f32(&mut buf, OFF_GAS, 0.3);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_CLUTCH, 0.0);
    // G-forces: negligible
    write_f32(&mut buf, OFF_ACCG_VERTICAL, 1.0);
    write_f32(&mut buf, OFF_ACCG_HORIZONTAL, 0.0);
    write_f32(&mut buf, OFF_ACCG_FRONTAL, 0.0);
    // Lap timing: pit-in lap was slow
    write_i32(&mut buf, OFF_LAP_TIME, 72_000); // 1:12.000 (pit-out)
    write_i32(&mut buf, OFF_LAST_LAP, 118_500); // 1:58.500 (pit-in lap)
    write_i32(&mut buf, OFF_BEST_LAP, 91_800); // 1:31.800
    write_i32(&mut buf, OFF_LAP_COUNT, 12);
    // Flags: in pit, engine limiter
    buf[OFF_IN_PIT] = 1;
    buf[OFF_ENGINE_LIMITER] = 1;

    let adapter = AssettoCorsaAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
