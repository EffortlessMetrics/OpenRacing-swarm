//! Debug snapshot tests for telemetry adapter output.
//!
//! Complements the existing `assert_yaml_snapshot!` tests by capturing the Rust
//! `Debug` representation of `NormalizedTelemetry`.  This catches changes that
//! yaml snapshots would miss (e.g. `#[serde(skip)]` / `#[serde(rename)]`
//! modifications, struct field additions).
//!
//! The non-deterministic `timestamp: Instant` field is scrubbed via an insta
//! regex filter so snapshots remain stable across runs.

use openracing_telemetry_adapters::{
    ACRallyAdapter, BeamNGAdapter, DakarDesertRallyAdapter, Dirt3Adapter, Dirt5Adapter,
    Ets2Adapter, FlatOutAdapter, ForzaAdapter, MotoGPAdapter, NascarAdapter, PCars2Adapter,
    RBRAdapter, RaceRoomAdapter, RennsportAdapter, SimHubAdapter, TelemetryAdapter,
    TrackmaniaAdapter, VRally4Adapter, WrcKylotonnAdapter, WreckfestAdapter,
    wrc_kylotonn::WrcKylotonnVariant,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

/// Wrap an `assert_debug_snapshot!` call with a filter that scrubs the
/// non-deterministic `Instant` timestamp field from Debug output.
macro_rules! debug_snap {
    ($name:expr, $value:expr) => {
        insta::with_settings!({
            filters => vec![
                (r"timestamp: Instant \{[^}]*\}", "timestamp: [timestamp]"),
            ]
        }, {
            insta::assert_debug_snapshot!($name, $value);
        });
    };
}

// ─── Forza (Sled format, 232 bytes) ──────────────────────────────────────────

fn make_forza_sled() -> Vec<u8> {
    let mut data = vec![0u8; 232];
    write_i32(&mut data, 0, 1); // is_race_on = 1
    write_f32_le(&mut data, 8, 8000.0); // engine_max_rpm
    write_f32_le(&mut data, 16, 6000.0); // current_rpm
    write_f32_le(&mut data, 32, 40.0); // vel_x → speed
    data
}

#[test]
fn debug_forza_sled() -> TestResult {
    let normalized = ForzaAdapter::new().normalize(&make_forza_sled())?;
    debug_snap!("debug_forza_sled", &normalized);
    Ok(())
}

// ─── BeamNG (OutGauge, 96 bytes) ─────────────────────────────────────────────

fn make_beamng() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    data[10] = 4; // gear_raw 4 → gear 3
    write_f32_le(&mut data, 12, 30.0); // speed_ms
    write_f32_le(&mut data, 16, 6000.0); // rpm
    write_f32_le(&mut data, 48, 0.8); // throttle
    write_f32_le(&mut data, 52, 0.0); // brake
    data
}

#[test]
fn debug_beamng() -> TestResult {
    let normalized = BeamNGAdapter::new().normalize(&make_beamng())?;
    debug_snap!("debug_beamng", &normalized);
    Ok(())
}

// ─── ETS2 (SCS shared memory, 512 bytes) ─────────────────────────────────────

fn make_ets2() -> Vec<u8> {
    let mut data = vec![0u8; 512];
    write_u32(&mut data, 0, 1); // version = 1
    write_f32_le(&mut data, 4, 25.0); // speed_ms
    write_f32_le(&mut data, 8, 1800.0); // engine_rpm
    write_i32(&mut data, 12, 6); // gear = 6
    write_f32_le(&mut data, 16, 0.7); // fuel_ratio
    write_f32_le(&mut data, 20, 0.55); // engine_load
    write_f32_le(&mut data, 24, 0.65); // throttle
    write_f32_le(&mut data, 28, 0.0); // brake
    write_f32_le(&mut data, 32, 0.0); // clutch
    write_f32_le(&mut data, 36, 0.15); // steering (slight right)
    write_f32_le(&mut data, 40, 91.0); // engine_temp_c
    write_f32_le(&mut data, 44, 2300.0); // max_rpm
    data
}

#[test]
fn debug_ets2() -> TestResult {
    let normalized = Ets2Adapter::new().normalize(&make_ets2())?;
    debug_snap!("debug_ets2", &normalized);
    Ok(())
}

// ─── PCars2 (custom binary, 46 bytes) ────────────────────────────────────────

fn make_pcars2() -> Vec<u8> {
    let mut data = vec![0u8; 46];
    data[44] = (0.1f32 * 127.0) as i8 as u8; // steering i8
    data[30] = (0.75f32 * 255.0) as u8; // throttle u8
    data[29] = 0; // brake u8
    write_f32_le(&mut data, 36, 45.0); // speed f32 m/s
    data[40..42].copy_from_slice(&7000u16.to_le_bytes()); // rpm u16
    data[42..44].copy_from_slice(&8500u16.to_le_bytes()); // max_rpm u16
    data[45] = 3 | (6 << 4); // gear=3, num_gears=6
    data
}

#[test]
fn debug_pcars2() -> TestResult {
    let normalized = PCars2Adapter::new().normalize(&make_pcars2())?;
    debug_snap!("debug_pcars2", &normalized);
    Ok(())
}

// ─── SimHub (JSON) ───────────────────────────────────────────────────────────

#[test]
fn debug_simhub() -> TestResult {
    let json = br#"{"SpeedMs":35.0,"Rpms":6000.0,"MaxRpms":8500.0,"Gear":"4","Throttle":80.0,"Brake":5.0,"Clutch":0.0,"SteeringAngle":-60.0,"FuelPercent":55.0,"LateralGForce":0.9,"LongitudinalGForce":0.3,"FFBValue":0.25,"IsRunning":true,"IsInPit":false}"#;
    let normalized = SimHubAdapter::new().normalize(json)?;
    debug_snap!("debug_simhub", &normalized);
    Ok(())
}

// ─── Dirt3 (Codemasters Mode 1, 264 bytes) ───────────────────────────────────

fn make_dirt3() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 100, 28.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 28.0); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 29.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 29.0); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.7); // throttle
    write_f32_le(&mut buf, 120, -0.1); // steer
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 3.0); // gear
    write_f32_le(&mut buf, 148, 5500.0); // rpm
    write_f32_le(&mut buf, 252, 8000.0); // max_rpm
    buf
}

#[test]
fn debug_dirt3() -> TestResult {
    let normalized = Dirt3Adapter::new().normalize(&make_dirt3())?;
    debug_snap!("debug_dirt3", &normalized);
    Ok(())
}

// ─── Dirt5 (CustomUdpSpec mode 1, 60 bytes) ──────────────────────────────────

fn make_dirt5() -> Vec<u8> {
    let mut buf = Vec::with_capacity(60);
    buf.extend_from_slice(&35.0f32.to_le_bytes()); // speed
    buf.extend_from_slice(&524.0f32.to_le_bytes()); // engine_rate (rad/s ≈ 5000 RPM)
    buf.extend_from_slice(&3i32.to_le_bytes()); // gear
    buf.extend_from_slice(&0.1f32.to_le_bytes()); // steering_input
    buf.extend_from_slice(&0.7f32.to_le_bytes()); // throttle_input
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // brake_input
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input
    for _ in 0..4 {
        buf.extend_from_slice(&35.0f32.to_le_bytes()); // wheel_patch_speed
    }
    for _ in 0..4 {
        buf.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position
    }
    buf
}

#[test]
fn debug_dirt5() -> TestResult {
    let normalized = Dirt5Adapter::new().normalize(&make_dirt5())?;
    debug_snap!("debug_dirt5", &normalized);
    Ok(())
}

// ─── Wreckfest (magic + binary, 28 bytes) ────────────────────────────────────

fn make_wreckfest() -> Vec<u8> {
    let mut buf = vec![0u8; 28];
    buf[0..4].copy_from_slice(b"WRKF");
    write_f32_le(&mut buf, 8, 38.0); // speed_ms
    write_f32_le(&mut buf, 12, 5000.0); // rpm
    buf[16] = 3; // gear
    write_f32_le(&mut buf, 20, 0.8); // lateral_g
    write_f32_le(&mut buf, 24, 0.3); // longitudinal_g
    buf
}

#[test]
fn debug_wreckfest() -> TestResult {
    let normalized = WreckfestAdapter::new().normalize(&make_wreckfest())?;
    debug_snap!("debug_wreckfest", &normalized);
    Ok(())
}

// ─── Dakar Desert Rally (magic + binary, 40 bytes) ───────────────────────────

fn make_dakar() -> Vec<u8> {
    let mut data = vec![0u8; 40];
    data[0..4].copy_from_slice(b"DAKR");
    write_f32_le(&mut data, 8, 32.0); // speed_ms
    write_f32_le(&mut data, 12, 4500.0); // rpm
    data[16] = 3; // gear
    write_f32_le(&mut data, 20, 0.3); // lateral_g
    write_f32_le(&mut data, 24, 0.2); // longitudinal_g
    write_f32_le(&mut data, 28, 0.7); // throttle
    write_f32_le(&mut data, 32, 0.0); // brake
    write_f32_le(&mut data, 36, 0.15); // steering_angle
    data
}

#[test]
fn debug_dakar() -> TestResult {
    let normalized = DakarDesertRallyAdapter::new().normalize(&make_dakar())?;
    debug_snap!("debug_dakar", &normalized);
    Ok(())
}

// ─── FlatOut (magic + binary, 36 bytes) ──────────────────────────────────────

fn make_flatout() -> Vec<u8> {
    let mut data = vec![0u8; 36];
    data[0..4].copy_from_slice(b"FOTC");
    write_f32_le(&mut data, 8, 28.0); // speed_ms
    write_f32_le(&mut data, 12, 5000.0); // rpm
    data[16] = 3; // gear
    write_f32_le(&mut data, 20, 0.0); // lateral_g
    write_f32_le(&mut data, 24, 0.0); // longitudinal_g
    write_f32_le(&mut data, 28, 0.8); // throttle
    write_f32_le(&mut data, 32, 0.0); // brake
    data
}

#[test]
fn debug_flatout() -> TestResult {
    let normalized = FlatOutAdapter::new().normalize(&make_flatout())?;
    debug_snap!("debug_flatout", &normalized);
    Ok(())
}

// ─── Trackmania (JSON) ───────────────────────────────────────────────────────

#[test]
fn debug_trackmania() -> TestResult {
    let json = br#"{"speed":50.0,"gear":4,"rpm":7000.0,"throttle":0.9,"brake":0.0,"steerAngle":0.1,"engineRunning":true}"#;
    let normalized = TrackmaniaAdapter::new().normalize(json)?;
    debug_snap!("debug_trackmania", &normalized);
    Ok(())
}

// ─── RBR (custom binary, 184 bytes) ──────────────────────────────────────────

fn make_rbr() -> Vec<u8> {
    let mut buf = vec![0u8; 184];
    write_f32_le(&mut buf, 12, 25.0); // speed_ms
    write_f32_le(&mut buf, 52, 0.75); // throttle
    write_f32_le(&mut buf, 56, 0.0); // brake
    write_f32_le(&mut buf, 64, 3.0); // gear
    write_f32_le(&mut buf, 68, -0.2); // steering
    write_f32_le(&mut buf, 116, 5500.0); // rpm
    buf
}

#[test]
fn debug_rbr() -> TestResult {
    let normalized = RBRAdapter::new().normalize(&make_rbr())?;
    debug_snap!("debug_rbr", &normalized);
    Ok(())
}

// ─── Rennsport (identifier byte + binary, 24 bytes) ─────────────────────────

fn make_rennsport() -> Vec<u8> {
    let mut buf = vec![0u8; 24];
    buf[0] = 0x52; // identifier 'R'
    write_f32_le(&mut buf, 4, 200.0); // speed_kmh
    write_f32_le(&mut buf, 8, 7500.0); // rpm
    buf[12] = 4; // gear
    write_f32_le(&mut buf, 16, 0.5); // ffb_scalar
    write_f32_le(&mut buf, 20, 0.1); // slip_ratio
    buf
}

#[test]
fn debug_rennsport() -> TestResult {
    let normalized = RennsportAdapter::new().normalize(&make_rennsport())?;
    debug_snap!("debug_rennsport", &normalized);
    Ok(())
}

// ─── NASCAR (Papyrus UDP, 92 bytes) ──────────────────────────────────────────

fn make_nascar() -> Vec<u8> {
    let mut data = vec![0u8; 92];
    write_f32_le(&mut data, 16, 55.0); // speed_ms
    write_f32_le(&mut data, 68, 4.0); // gear (float)
    write_f32_le(&mut data, 72, 7500.0); // rpm
    write_f32_le(&mut data, 80, 0.85); // throttle
    write_f32_le(&mut data, 84, 0.0); // brake
    write_f32_le(&mut data, 88, -0.1); // steer
    data
}

#[test]
fn debug_nascar() -> TestResult {
    let normalized = NascarAdapter::new().normalize(&make_nascar())?;
    debug_snap!("debug_nascar", &normalized);
    Ok(())
}

// ─── WRC Kylotonn (binary, 96 bytes) ─────────────────────────────────────────

fn make_wrc_kylotonn() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    write_f32_le(&mut data, 0, 0.5); // stage_progress
    write_f32_le(&mut data, 4, 30.0); // road_speed_ms
    write_f32_le(&mut data, 8, -0.2); // steering
    write_f32_le(&mut data, 12, 0.8); // throttle
    write_f32_le(&mut data, 16, 0.0); // brake
    write_u32(&mut data, 28, 3); // gear
    write_f32_le(&mut data, 32, 5500.0); // rpm
    write_f32_le(&mut data, 36, 8000.0); // max_rpm
    data
}

#[test]
fn debug_wrc_kylotonn() -> TestResult {
    let normalized =
        WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9).normalize(&make_wrc_kylotonn())?;
    debug_snap!("debug_wrc_kylotonn", &normalized);
    Ok(())
}

// ─── V-Rally 4 (binary, 96 bytes) ───────────────────────────────────────────

fn make_vrally4() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    write_f32_le(&mut data, 4, 25.0); // speed_ms
    write_f32_le(&mut data, 8, 0.1); // steering
    write_f32_le(&mut data, 12, 0.65); // throttle
    write_f32_le(&mut data, 16, 0.0); // brake
    write_u32(&mut data, 28, 3); // gear
    write_f32_le(&mut data, 32, 5000.0); // rpm
    write_f32_le(&mut data, 36, 7500.0); // max_rpm
    data
}

#[test]
fn debug_vrally4() -> TestResult {
    let normalized = VRally4Adapter::new().normalize(&make_vrally4())?;
    debug_snap!("debug_vrally4", &normalized);
    Ok(())
}

// ─── RaceRoom (shared memory, 4096 bytes) ────────────────────────────────────

fn make_raceroom() -> Vec<u8> {
    let mut buf = vec![0u8; 4096];
    write_i32(&mut buf, 0, 3); // version_major = 3
    write_i32(&mut buf, 20, 0); // game_paused
    write_i32(&mut buf, 24, 0); // game_in_menus
    let rps = 6000.0f32 * std::f32::consts::PI / 30.0;
    let max_rps = 8500.0f32 * std::f32::consts::PI / 30.0;
    write_f32_le(&mut buf, 1396, rps); // engine_rps
    write_f32_le(&mut buf, 1400, max_rps); // max_engine_rps
    write_f32_le(&mut buf, 1392, 48.0); // car_speed m/s
    write_f32_le(&mut buf, 1500, 0.8); // throttle
    write_f32_le(&mut buf, 1508, 0.0); // brake
    write_i32(&mut buf, 1408, 4); // gear
    buf
}

#[test]
fn debug_raceroom() -> TestResult {
    let normalized = RaceRoomAdapter::new().normalize(&make_raceroom())?;
    debug_snap!("debug_raceroom", &normalized);
    Ok(())
}

// ─── MotoGP (SimHub JSON) ────────────────────────────────────────────────────

#[test]
fn debug_motogp() -> TestResult {
    let json = br#"{"SpeedMs":40.0,"Rpms":10000.0,"MaxRpms":14000.0,"Gear":"5","Throttle":90.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":15.0,"FuelPercent":50.0,"LateralGForce":1.0,"LongitudinalGForce":0.5,"FFBValue":0.3,"IsRunning":true,"IsInPit":false}"#;
    let normalized = MotoGPAdapter::new().normalize(json)?;
    debug_snap!("debug_motogp", &normalized);
    Ok(())
}

// ─── AC Rally (probe packet) ─────────────────────────────────────────────────

#[test]
fn debug_ac_rally() -> TestResult {
    let raw = b"\x01\x04AC Rally probe";
    let normalized = ACRallyAdapter::new().normalize(raw)?;
    debug_snap!("debug_ac_rally", &normalized);
    Ok(())
}
