//! Snapshot tests for additional telemetry adapters (v4).
//!
//! Covers: RaceRoom, RBR, Rennsport, WTCR, WRC Generations, Wreckfest,
//! MotoGP (SimHub), MudRunner (SimHub), RIDE 5 (SimHub), and AC Rally.

use openracing_telemetry_adapters::{
    ACRallyAdapter, MotoGPAdapter, MudRunnerAdapter, RBRAdapter, RaceRoomAdapter, RennsportAdapter,
    Ride5Adapter, TelemetryAdapter, WrcGenerationsAdapter, WreckfestAdapter, WtcrAdapter,
    mudrunner::MudRunnerVariant,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

// ─── RaceRoom ─────────────────────────────────────────────────────────────────

fn make_raceroom_packet() -> Vec<u8> {
    // R3E_VIEW_SIZE = 4096; R3E SDK v3 offsets (pack(push, 1)).
    let mut buf = vec![0u8; 4096];
    write_i32(&mut buf, 0, 3); // version_major = 3
    write_i32(&mut buf, 20, 0); // game_paused = 0
    write_i32(&mut buf, 24, 0); // game_in_menus = 0
    // engine_rps in rad/s: 5500 RPM * π/30 ≈ 575.96 rad/s
    let rps_5500 = 5500.0f32 * std::f32::consts::PI / 30.0;
    let rps_8500 = 8500.0f32 * std::f32::consts::PI / 30.0;
    write_f32_le(&mut buf, 1396, rps_5500); // engine_rps
    write_f32_le(&mut buf, 1400, rps_8500); // max_engine_rps
    write_f32_le(&mut buf, 1456, 40.0); // fuel_left (f32, litres)
    write_f32_le(&mut buf, 1460, 80.0); // fuel_capacity (f32, litres)
    write_f32_le(&mut buf, 1392, 44.0); // car_speed m/s
    write_f32_le(&mut buf, 1524, 0.3); // steer_input_raw
    write_f32_le(&mut buf, 1500, 0.75); // throttle
    write_f32_le(&mut buf, 1508, 0.0); // brake
    write_f32_le(&mut buf, 1516, 0.0); // clutch
    write_i32(&mut buf, 1408, 4); // gear
    buf
}

#[test]
fn raceroom_snapshot() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let normalized = adapter.normalize(&make_raceroom_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── RBR ──────────────────────────────────────────────────────────────────────

fn make_rbr_packet() -> Vec<u8> {
    // MIN_PACKET_SIZE = 128
    let mut buf = vec![0u8; 184];
    write_f32_le(&mut buf, 12, 28.5); // speed_ms
    write_f32_le(&mut buf, 52, 0.8); // throttle
    write_f32_le(&mut buf, 56, 0.0); // brake
    write_f32_le(&mut buf, 60, 0.0); // clutch
    write_f32_le(&mut buf, 64, 3.0); // gear (3 = 3rd)
    write_f32_le(&mut buf, 68, -0.25); // steering
    write_f32_le(&mut buf, 112, 0.0); // handbrake
    write_f32_le(&mut buf, 116, 6200.0); // rpm
    buf
}

#[test]
fn rbr_snapshot() -> TestResult {
    let adapter = RBRAdapter::new();
    let normalized = adapter.normalize(&make_rbr_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Rennsport ────────────────────────────────────────────────────────────────

fn make_rennsport_packet() -> Vec<u8> {
    // MIN_PACKET_SIZE = 24; identifier byte = 0x52 ('R')
    let mut buf = vec![0u8; 24];
    buf[0] = 0x52; // identifier 'R'
    write_f32_le(&mut buf, 4, 180.0); // speed_kmh → 50.0 m/s
    write_f32_le(&mut buf, 8, 7200.0); // rpm
    buf[12] = 5u8; // gear (5th)
    write_f32_le(&mut buf, 16, 0.65); // ffb_scalar
    write_f32_le(&mut buf, 20, 0.08); // slip_ratio
    buf
}

#[test]
fn rennsport_snapshot() -> TestResult {
    let adapter = RennsportAdapter::new();
    let normalized = adapter.normalize(&make_rennsport_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WTCR ─────────────────────────────────────────────────────────────────────

fn make_wtcr_packet() -> Vec<u8> {
    // Codemasters Mode 1, MIN_PACKET_SIZE = 264
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 108, 33.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 33.0); // wheel_speed_fr
    write_f32_le(&mut buf, 100, 33.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 33.0); // wheel_speed_rr → speed = 33.0 m/s
    write_f32_le(&mut buf, 116, 0.9); // throttle
    write_f32_le(&mut buf, 120, 0.1); // steer
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 4.0); // gear (4th)
    write_f32_le(&mut buf, 136, 1.5); // gforce_lat
    write_f32_le(&mut buf, 140, 0.4); // gforce_lon
    write_f32_le(&mut buf, 144, 2.0); // current_lap
    write_f32_le(&mut buf, 148, 6800.0); // rpm
    write_f32_le(&mut buf, 156, 3.0); // car_position
    write_f32_le(&mut buf, 180, 35.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 60.0); // fuel_capacity
    write_f32_le(&mut buf, 188, 0.0); // in_pit
    write_f32_le(&mut buf, 248, 92.5); // last_lap_time_s
    write_f32_le(&mut buf, 252, 8500.0); // max_rpm
    write_f32_le(&mut buf, 260, 6.0); // max_gears
    buf
}

#[test]
fn wtcr_snapshot() -> TestResult {
    let adapter = WtcrAdapter::new();
    let normalized = adapter.normalize(&make_wtcr_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC Generations ──────────────────────────────────────────────────────────

fn make_wrc_generations_packet() -> Vec<u8> {
    // Same Codemasters Mode 1 layout as WTCR
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 4, 35.2); // lap_time (current_lap_time_s)
    write_f32_le(&mut buf, 32, 24.0); // velocity_x
    write_f32_le(&mut buf, 36, 0.5); // velocity_y
    write_f32_le(&mut buf, 40, 6.0); // velocity_z
    write_f32_le(&mut buf, 108, 25.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 25.0); // wheel_speed_fr
    write_f32_le(&mut buf, 100, 25.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 25.0); // wheel_speed_rr → speed = 25.0 m/s
    write_f32_le(&mut buf, 116, 0.6); // throttle
    write_f32_le(&mut buf, 120, -0.2); // steer
    write_f32_le(&mut buf, 124, 0.15); // brake
    write_f32_le(&mut buf, 132, 3.0); // gear (3rd)
    write_f32_le(&mut buf, 136, 0.9); // gforce_lat
    write_f32_le(&mut buf, 140, 0.3); // gforce_lon
    write_f32_le(&mut buf, 144, 1.0); // current_lap
    write_f32_le(&mut buf, 148, 5500.0); // rpm
    write_f32_le(&mut buf, 156, 5.0); // car_position
    write_f32_le(&mut buf, 180, 45.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 70.0); // fuel_capacity
    write_f32_le(&mut buf, 188, 0.0); // in_pit
    write_f32_le(&mut buf, 248, 78.3); // last_lap_time_s
    write_f32_le(&mut buf, 252, 7500.0); // max_rpm
    write_f32_le(&mut buf, 260, 5.0); // max_gears
    buf
}

#[test]
fn wrc_generations_snapshot() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let normalized = adapter.normalize(&make_wrc_generations_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Wreckfest ────────────────────────────────────────────────────────────────

fn make_wreckfest_packet() -> Vec<u8> {
    // MIN_PACKET_SIZE = 28; magic "WRKF" at offset 0
    let mut buf = vec![0u8; 28];
    buf[0..4].copy_from_slice(b"WRKF"); // magic
    write_f32_le(&mut buf, 8, 35.0); // speed_ms
    write_f32_le(&mut buf, 12, 4500.0); // rpm
    buf[16] = 3u8; // gear (3rd)
    write_f32_le(&mut buf, 20, 0.8); // lateral_g
    write_f32_le(&mut buf, 24, 0.3); // longitudinal_g
    buf
}

#[test]
fn wreckfest_snapshot() -> TestResult {
    let adapter = WreckfestAdapter::new();
    let normalized = adapter.normalize(&make_wreckfest_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── MotoGP (SimHub JSON) ─────────────────────────────────────────────────────

#[test]
fn motogp_simhub_snapshot() -> TestResult {
    let adapter = MotoGPAdapter::new();
    let json = br#"{"SpeedMs":38.0,"Rpms":9500.0,"MaxRpms":14000.0,"Gear":"4","Throttle":85.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":22.5,"FuelPercent":55.0,"LateralGForce":1.1,"LongitudinalGForce":0.6,"FFBValue":0.4,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── MudRunner (SimHub JSON) ──────────────────────────────────────────────────

#[test]
fn mudrunner_simhub_snapshot() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":8.0,"Rpms":2800.0,"MaxRpms":4500.0,"Gear":"2","Throttle":60.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":70.0,"LateralGForce":0.3,"LongitudinalGForce":0.5,"FFBValue":0.2,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn snowrunner_simhub_snapshot() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let json = br#"{"SpeedMs":5.0,"Rpms":2200.0,"MaxRpms":4500.0,"Gear":"1","Throttle":40.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":45.0,"FuelPercent":85.0,"LateralGForce":0.1,"LongitudinalGForce":0.2,"FFBValue":0.1,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── RIDE 5 (SimHub JSON) ─────────────────────────────────────────────────────

#[test]
fn ride5_simhub_snapshot() -> TestResult {
    let adapter = Ride5Adapter::new();
    let json = br#"{"SpeedMs":42.0,"Rpms":8200.0,"MaxRpms":12000.0,"Gear":"5","Throttle":78.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-30.0,"FuelPercent":45.0,"LateralGForce":0.9,"LongitudinalGForce":0.4,"FFBValue":0.35,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── AC Rally (probe-based discovery adapter) ─────────────────────────────────

#[test]
fn ac_rally_snapshot() -> TestResult {
    let adapter = ACRallyAdapter::new();
    // AC Rally normalize() runs a probe packet decoder that captures raw bytes
    // as diagnostic extended fields.
    let raw = b"\x01\x04Hello AC Rally probe packet";
    let normalized = adapter.normalize(raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
