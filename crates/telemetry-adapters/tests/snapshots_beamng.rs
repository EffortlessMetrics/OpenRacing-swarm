//! Snapshot tests for BeamNG.drive OutGauge telemetry adapter.
//!
//! Three scenarios: normal driving, stationary idle, and edge-case maximum values.

use openracing_telemetry_adapters::{BeamNGAdapter, TelemetryAdapter};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// OutGauge byte offsets (see beamng.rs for full layout documentation).
const OFF_GEAR: usize = 10;
const OFF_SPEED: usize = 12;
const OFF_RPM: usize = 16;
const OFF_TURBO: usize = 20;
const OFF_ENG_TEMP: usize = 24;
const OFF_FUEL: usize = 28;
const OFF_OIL_PRESSURE: usize = 32;
const OFF_OIL_TEMP: usize = 36;
const OFF_SHOW_LIGHTS: usize = 44;
const OFF_THROTTLE: usize = 48;
const OFF_BRAKE: usize = 52;
const OFF_CLUTCH: usize = 56;

// Dashboard light flags.
const DL_SHIFT: u32 = 0x0001;
const DL_PITSPEED: u32 = 0x0008;
const DL_TC: u32 = 0x0010;
const DL_ABS: u32 = 0x0400;

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ─── Scenario 1: Normal driving ──────────────────────────────────────────────
// Mid-corner in 3rd gear at ~120 km/h, partial throttle and light braking,
// engine warm, half fuel, slight turbo boost.

#[test]
fn beamng_normal_driving_snapshot() -> TestResult {
    let mut buf = vec![0u8; 96];
    buf[OFF_GEAR] = 4; // OutGauge 4 → normalized 3rd gear
    write_f32_le(&mut buf, OFF_SPEED, 33.3); // ~120 km/h
    write_f32_le(&mut buf, OFF_RPM, 5500.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 0.65);
    write_f32_le(&mut buf, OFF_BRAKE, 0.15);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, OFF_FUEL, 0.52);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 88.0);
    write_f32_le(&mut buf, OFF_TURBO, 0.45);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 4.2);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 102.0);
    write_u32_le(&mut buf, OFF_SHOW_LIGHTS, 0);

    let adapter = BeamNGAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Stationary / idle ──────────────────────────────────────────
// Engine running at idle in neutral, no movement, full fuel, cold oil.

#[test]
fn beamng_stationary_idle_snapshot() -> TestResult {
    let mut buf = vec![0u8; 92]; // 92-byte packet (no optional id)
    buf[OFF_GEAR] = 1; // OutGauge 1 → neutral
    write_f32_le(&mut buf, OFF_SPEED, 0.0);
    write_f32_le(&mut buf, OFF_RPM, 750.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 0.0);
    write_f32_le(&mut buf, OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, OFF_FUEL, 1.0);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 42.0);
    write_f32_le(&mut buf, OFF_TURBO, 0.0);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 2.0);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 38.0);
    write_u32_le(&mut buf, OFF_SHOW_LIGHTS, 0);

    let adapter = BeamNGAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Edge case – maximum values / unusual state ─────────────────
// Reverse gear at redline with full brake, shift light + ABS + TC + pit limiter
// all active, extreme temperatures, and over-range throttle (should clamp).

#[test]
fn beamng_edge_case_max_values_snapshot() -> TestResult {
    let mut buf = vec![0u8; 96];
    buf[OFF_GEAR] = 0; // OutGauge 0 → reverse
    write_f32_le(&mut buf, OFF_SPEED, 83.3); // ~300 km/h
    write_f32_le(&mut buf, OFF_RPM, 9500.0);
    write_f32_le(&mut buf, OFF_THROTTLE, 1.5); // over-range, should clamp to 1.0
    write_f32_le(&mut buf, OFF_BRAKE, 1.0);
    write_f32_le(&mut buf, OFF_CLUTCH, 1.0);
    write_f32_le(&mut buf, OFF_FUEL, 0.01);
    write_f32_le(&mut buf, OFF_ENG_TEMP, 130.0);
    write_f32_le(&mut buf, OFF_TURBO, 2.8);
    write_f32_le(&mut buf, OFF_OIL_PRESSURE, 7.5);
    write_f32_le(&mut buf, OFF_OIL_TEMP, 160.0);
    write_u32_le(
        &mut buf,
        OFF_SHOW_LIGHTS,
        DL_SHIFT | DL_PITSPEED | DL_TC | DL_ABS,
    );

    let adapter = BeamNGAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
