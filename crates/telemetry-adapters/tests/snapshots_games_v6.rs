//! Snapshot tests for additional telemetry adapters (v6).
//!
//! Covers: F1 25 (EA Sports F1 2025) and WRC 10 (Kylotonn).

use openracing_telemetry_adapters::{
    F1_25Adapter, TelemetryAdapter, WrcKylotonnAdapter, f1_25::build_car_telemetry_packet,
    wrc_kylotonn::WrcKylotonnVariant,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

// ─── F1 25 ────────────────────────────────────────────────────────────────────

#[test]
fn f1_25_snapshot() -> TestResult {
    let adapter = F1_25Adapter::new();
    // Car Telemetry (packet ID 6) for player at index 0.
    let packet = build_car_telemetry_packet(
        0,                        // player_index
        252,                      // speed_kmh (≈ 70.0 m/s)
        4,                        // gear
        12_000,                   // engine_rpm
        0.85,                     // throttle
        0.0,                      // brake
        0,                        // drs = off
        [26.0, 26.0, 26.5, 26.5], // tyre pressures (PSI, RL/RR/FL/FR)
    );
    let normalized = adapter.normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC 10 (Kylotonn) ───────────────────────────────────────────────────────

fn make_wrc10_packet() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    write_f32_le(&mut data, 0, 0.5); // stage_progress (exact 1/2)
    write_f32_le(&mut data, 4, 28.0); // road_speed_ms
    write_f32_le(&mut data, 8, 0.25); // steering (exact 1/4)
    write_f32_le(&mut data, 12, 0.75); // throttle (exact 3/4)
    write_f32_le(&mut data, 16, 0.0); // brake
    write_f32_le(&mut data, 20, 0.0); // hand_brake
    write_f32_le(&mut data, 24, 0.0); // clutch
    write_u32(&mut data, 28, 4); // gear (0=reverse, 1..7=forward)
    write_f32_le(&mut data, 32, 6000.0); // rpm (exact)
    write_f32_le(&mut data, 36, 8000.0); // max_rpm → rpm_fraction = 0.75 (exact 3/4)
    data
}

#[test]
fn wrc_10_snapshot() -> TestResult {
    let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
    let normalized = adapter.normalize(&make_wrc10_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
