//! Snapshot tests for additional telemetry adapters (v2).
//!
//! Covers: Dakar Desert Rally, FlatOut, NASCAR, WRC Kylotonn (WRC 9),
//! V-Rally 4, rFactor 1, Gran Turismo Sport, GRID Autosport, Gravel (SimHub),
//! and Trackmania.

use openracing_telemetry_adapters::{
    DakarDesertRallyAdapter, FlatOutAdapter, GravelAdapter, GridAutosportAdapter, NascarAdapter,
    RFactor1Adapter, TelemetryAdapter, TrackmaniaAdapter, VRally4Adapter, WrcKylotonnAdapter,
    gran_turismo_7, wrc_kylotonn::WrcKylotonnVariant,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_f64(buf: &mut [u8], offset: usize, val: f64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

#[allow(dead_code)]
fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

// ─── Dakar Desert Rally ───────────────────────────────────────────────────────

fn make_dakar_packet() -> Vec<u8> {
    let mut data = vec![0u8; 40];
    data[0..4].copy_from_slice(b"DAKR"); // magic
    write_f32_le(&mut data, 8, 30.0); // speed_ms
    write_f32_le(&mut data, 12, 4000.0); // rpm
    data[16] = 3; // gear
    write_f32_le(&mut data, 20, 0.2); // lateral_g
    write_f32_le(&mut data, 24, 0.4); // longitudinal_g
    write_f32_le(&mut data, 28, 0.8); // throttle
    write_f32_le(&mut data, 32, 0.0); // brake
    write_f32_le(&mut data, 36, 0.1); // steering_angle
    data
}

#[test]
fn dakar_desert_rally_snapshot() -> TestResult {
    let adapter = DakarDesertRallyAdapter::new();
    let normalized = adapter.normalize(&make_dakar_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── FlatOut ─────────────────────────────────────────────────────────────────

fn make_flatout_packet() -> Vec<u8> {
    let mut data = vec![0u8; 36];
    data[0..4].copy_from_slice(b"FOTC"); // magic
    write_f32_le(&mut data, 8, 25.0); // speed_ms
    write_f32_le(&mut data, 12, 5500.0); // rpm
    data[16] = 4; // gear
    write_f32_le(&mut data, 20, 0.0); // lateral_g
    write_f32_le(&mut data, 24, 0.0); // longitudinal_g
    write_f32_le(&mut data, 28, 0.7); // throttle
    write_f32_le(&mut data, 32, 0.1); // brake
    data
}

#[test]
fn flatout_snapshot() -> TestResult {
    let adapter = FlatOutAdapter::new();
    let normalized = adapter.normalize(&make_flatout_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── NASCAR ──────────────────────────────────────────────────────────────────

fn make_nascar_packet() -> Vec<u8> {
    let mut data = vec![0u8; 92];
    write_f32_le(&mut data, 16, 50.0); // speed_ms
    write_f32_le(&mut data, 32, 0.0); // acc_x (longitudinal, m/s²)
    write_f32_le(&mut data, 36, 0.0); // acc_y (lateral, m/s²)
    write_f32_le(&mut data, 68, 3.0); // gear (float: 3 = third gear)
    write_f32_le(&mut data, 72, 7000.0); // rpm
    write_f32_le(&mut data, 80, 0.9); // throttle
    write_f32_le(&mut data, 84, 0.0); // brake
    write_f32_le(&mut data, 88, -0.2); // steer
    data
}

#[test]
fn nascar_snapshot() -> TestResult {
    let adapter = NascarAdapter::new();
    let normalized = adapter.normalize(&make_nascar_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC Kylotonn (WRC 9) ────────────────────────────────────────────────────

fn make_kylotonn_packet() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    write_f32_le(&mut data, 0, 0.45); // stage_progress
    write_f32_le(&mut data, 4, 32.0); // road_speed_ms
    write_f32_le(&mut data, 8, -0.3); // steering
    write_f32_le(&mut data, 12, 0.75); // throttle
    write_f32_le(&mut data, 16, 0.0); // brake
    write_f32_le(&mut data, 20, 0.0); // hand_brake
    write_f32_le(&mut data, 24, 0.0); // clutch
    write_u32(&mut data, 28, 3); // gear (0=reverse, 1..7=forward)
    write_f32_le(&mut data, 32, 5200.0); // rpm
    write_f32_le(&mut data, 36, 8000.0); // max_rpm
    data
}

#[test]
fn wrc_kylotonn_wrc9_snapshot() -> TestResult {
    let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9);
    let normalized = adapter.normalize(&make_kylotonn_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── V-Rally 4 ───────────────────────────────────────────────────────────────

fn make_vrally4_packet() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    write_f32_le(&mut data, 4, 22.0); // speed_ms
    write_f32_le(&mut data, 8, 0.15); // steering
    write_f32_le(&mut data, 12, 0.6); // throttle
    write_f32_le(&mut data, 16, 0.0); // brake
    write_f32_le(&mut data, 24, 0.0); // clutch
    write_u32(&mut data, 28, 2); // gear (2nd)
    write_f32_le(&mut data, 32, 4800.0); // rpm
    write_f32_le(&mut data, 36, 7500.0); // max_rpm
    data
}

#[test]
fn v_rally_4_snapshot() -> TestResult {
    let adapter = VRally4Adapter::new();
    let normalized = adapter.normalize(&make_vrally4_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor 1 ───────────────────────────────────────────────────────────────

fn make_rfactor1_packet() -> Vec<u8> {
    // OFF_GEAR = 1024; allocate 1025 bytes to cover all fields.
    let mut data = vec![0u8; 1025];
    write_f64(&mut data, 24, 0.0); // vel_x
    write_f64(&mut data, 32, 0.0); // vel_y
    write_f64(&mut data, 40, 50.0); // vel_z → speed = 50.0 m/s
    write_f64(&mut data, 312, 7000.0); // engine_rpm
    write_f64(&mut data, 992, -0.2); // steer_input
    write_f64(&mut data, 1000, 0.9); // throttle
    write_f64(&mut data, 1008, 0.0); // brake
    data[1024] = 3u8; // gear = 3
    data
}

#[test]
fn rfactor1_snapshot() -> TestResult {
    let adapter = RFactor1Adapter::new();
    let normalized = adapter.normalize(&make_rfactor1_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo Sport ───────────────────────────────────────────────────────
//
// GT Sport's normalize() runs Salsa20 decryption, so we test parsing using
// the public `parse_decrypted` function directly (same path used by internal
// GT Sport unit tests).

fn make_gts_packet() -> [u8; gran_turismo_7::PACKET_SIZE] {
    let mut buf = [0u8; gran_turismo_7::PACKET_SIZE];
    buf[gran_turismo_7::OFF_MAGIC..gran_turismo_7::OFF_MAGIC + 4]
        .copy_from_slice(&gran_turismo_7::MAGIC.to_le_bytes());
    buf
}

#[test]
fn gran_turismo_sport_snapshot() -> TestResult {
    let packet = make_gts_packet();
    let normalized = gran_turismo_7::parse_decrypted(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── GRID Autosport ──────────────────────────────────────────────────────────

fn make_grid_autosport_packet() -> Vec<u8> {
    let mut data = vec![0u8; 264];
    write_f32_le(&mut data, 108, 28.0); // wheel_speed_fl
    write_f32_le(&mut data, 112, 28.0); // wheel_speed_fr
    write_f32_le(&mut data, 100, 28.0); // wheel_speed_rl
    write_f32_le(&mut data, 104, 28.0); // wheel_speed_rr → speed = 28.0 m/s
    write_f32_le(&mut data, 116, 0.85); // throttle
    write_f32_le(&mut data, 120, 0.1); // steer
    write_f32_le(&mut data, 124, 0.0); // brake
    write_f32_le(&mut data, 132, 4.0); // gear (float: 4.0 → 4th gear)
    write_f32_le(&mut data, 136, 0.5); // gforce_lat
    write_f32_le(&mut data, 140, 0.3); // gforce_lon
    write_f32_le(&mut data, 148, 6200.0); // rpm
    write_f32_le(&mut data, 252, 8500.0); // max_rpm
    data
}

#[test]
fn grid_autosport_snapshot() -> TestResult {
    let adapter = GridAutosportAdapter::new();
    let normalized = adapter.normalize(&make_grid_autosport_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gravel (SimHub JSON bridge) ─────────────────────────────────────────────

#[test]
fn gravel_simhub_snapshot() -> TestResult {
    let adapter = GravelAdapter::new();
    let json = br#"{"SpeedMs":30.0,"Rpms":5000.0,"MaxRpms":8000.0,"Gear":"3","Throttle":70.0,"Brake":5.0,"Clutch":0.0,"SteeringAngle":45.0,"FuelPercent":60.0,"LateralGForce":0.8,"LongitudinalGForce":0.2,"FFBValue":0.15,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Trackmania ──────────────────────────────────────────────────────────────

#[test]
fn trackmania_snapshot() -> TestResult {
    let adapter = TrackmaniaAdapter::new();
    let json = br#"{"speed":45.0,"gear":3,"rpm":6500.0,"throttle":0.8,"brake":0.0,"steerAngle":-0.1,"engineRunning":true}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
