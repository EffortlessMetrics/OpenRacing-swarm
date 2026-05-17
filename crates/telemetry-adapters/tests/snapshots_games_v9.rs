//! Snapshot tests for telemetry adapters with realistic driving data (v9).
//!
//! Covers adapters NOT in v8: DiRT 3, Dirt 4, Dirt 5, GRID 2019.

use openracing_telemetry_adapters::{
    Dirt3Adapter, Dirt4Adapter, Dirt5Adapter, Grid2019Adapter, TelemetryAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

// ─── Codemasters Mode 1 shared builder ───────────────────────────────────────
// DiRT 3, Dirt 4, and GRID 2019 all share the 264-byte Codemasters Mode 1 layout.
// Offsets: wheel speeds @100-112, throttle@116, steer@120, brake@124,
//   gear@132, gforce_lat@136, gforce_lon@140, rpm@148,
//   fuel_in_tank@180, fuel_capacity@184, max_rpm@252, max_gears@260.

fn make_codemasters_mode1_data() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 100, 50.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 50.0); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 51.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 51.0); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.80); // throttle
    write_f32_le(&mut buf, 120, -0.15); // steer (slight left)
    write_f32_le(&mut buf, 124, 0.10); // brake
    write_f32_le(&mut buf, 132, 4.0); // gear (4th)
    write_f32_le(&mut buf, 136, 0.95); // gforce_lat
    write_f32_le(&mut buf, 140, 0.25); // gforce_lon
    write_f32_le(&mut buf, 148, 8000.0); // rpm
    write_f32_le(&mut buf, 180, 30.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 50.0); // fuel_capacity
    write_f32_le(&mut buf, 252, 9500.0); // max_rpm
    write_f32_le(&mut buf, 260, 6.0); // max_gears
    buf
}

// ─── DiRT 3 ──────────────────────────────────────────────────────────────────
// Codemasters Mode 1 (264-byte) packet.

#[test]
fn dirt3_realistic_snapshot() -> TestResult {
    let adapter = Dirt3Adapter::new();
    let normalized = adapter.normalize(&make_codemasters_mode1_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Dirt 4 ──────────────────────────────────────────────────────────────────
// Codemasters Mode 1 (264-byte) packet.

#[test]
fn dirt4_realistic_snapshot() -> TestResult {
    let adapter = Dirt4Adapter::new();
    let normalized = adapter.normalize(&make_codemasters_mode1_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Dirt 5 ──────────────────────────────────────────────────────────────────
// Custom UDP mode 1 packet: 15 sequential fields (7 base + 8 mode-1 extras).
// Base: speed(f32), engine_rate(f32), gear(i32), steer(f32), throttle(f32),
//       brake(f32), clutch(f32).
// Mode 1: wheel_patch_speed_{fl,fr,rl,rr}(f32), suspension_position_{fl,fr,rl,rr}(f32).

fn make_dirt5_mode1_data() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(60);
    buf.extend_from_slice(&50.0f32.to_le_bytes()); // speed (m/s)
    buf.extend_from_slice(&838.0f32.to_le_bytes()); // engine_rate (rad/s ≈ 8000 RPM)
    buf.extend_from_slice(&4i32.to_le_bytes()); // gear (4th)
    buf.extend_from_slice(&(-0.15f32).to_le_bytes()); // steering_input (slight left)
    buf.extend_from_slice(&0.80f32.to_le_bytes()); // throttle_input
    buf.extend_from_slice(&0.10f32.to_le_bytes()); // brake_input
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input
    buf.extend_from_slice(&51.0f32.to_le_bytes()); // wheel_patch_speed_fl
    buf.extend_from_slice(&51.0f32.to_le_bytes()); // wheel_patch_speed_fr
    buf.extend_from_slice(&50.0f32.to_le_bytes()); // wheel_patch_speed_rl
    buf.extend_from_slice(&50.0f32.to_le_bytes()); // wheel_patch_speed_rr
    buf.extend_from_slice(&0.02f32.to_le_bytes()); // suspension_position_fl
    buf.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_fr
    buf.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_rl
    buf.extend_from_slice(&0.03f32.to_le_bytes()); // suspension_position_rr
    buf
}

#[test]
fn dirt5_realistic_snapshot() -> TestResult {
    let adapter = Dirt5Adapter::new();
    let normalized = adapter.normalize(&make_dirt5_mode1_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── GRID 2019 ───────────────────────────────────────────────────────────────
// Codemasters Mode 1 (264-byte) packet.

#[test]
fn grid_2019_realistic_snapshot() -> TestResult {
    let adapter = Grid2019Adapter::new();
    let normalized = adapter.normalize(&make_codemasters_mode1_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
