//! Snapshot tests for telemetry adapters with realistic driving data (v8).
//!
//! Covers adapters NOT in v7: ACC, Assetto Corsa (original), BeamNG, LFS,
//! F1 25, SimHub, WRC Generations, Project CARS 2.

use openracing_telemetry_adapters::{
    ACCAdapter, AssettoCorsaAdapter, BeamNGAdapter, F1_25Adapter, LFSAdapter, PCars2Adapter,
    SimHubAdapter, TelemetryAdapter, WrcGenerationsAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn write_i32_le(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

/// Push a minimal ACC lap-time block (no splits).
fn push_acc_lap(buf: &mut Vec<u8>, lap_time_ms: i32) {
    buf.extend_from_slice(&lap_time_ms.to_le_bytes()); // lap_time_ms
    buf.extend_from_slice(&1u16.to_le_bytes()); // car_index
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(0); // split_count = 0
    buf.push(0); // is_invalid
    buf.push(1); // is_valid_for_best
    buf.push(0); // is_outlap
    buf.push(0); // is_inlap
}

// ─── ACC (Assetto Corsa Competizione) ─────────────────────────────────────────
// ACC normalize() parses a RealtimeCarUpdate message (type 3) from the
// ACC broadcast protocol.

fn make_acc_realtime_car_update() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(128);
    // Message type = 3 (MSG_REALTIME_CAR_UPDATE)
    buf.push(3);
    // car_index: u16
    buf.extend_from_slice(&7u16.to_le_bytes());
    // driver_index: u16
    buf.extend_from_slice(&0u16.to_le_bytes());
    // driver_count: u8
    buf.push(1);
    // gear_raw: u8 (gear = raw - 2; for 4th gear we need raw = 6)
    buf.push(6);
    // world_pos_x: f32
    buf.extend_from_slice(&100.0f32.to_le_bytes());
    // world_pos_y: f32
    buf.extend_from_slice(&50.0f32.to_le_bytes());
    // yaw: f32
    buf.extend_from_slice(&1.5f32.to_le_bytes());
    // car_location: u8 (1 = on track)
    buf.push(1);
    // speed_kmh: u16 (180 km/h)
    buf.extend_from_slice(&180u16.to_le_bytes());
    // position: u16
    buf.extend_from_slice(&3u16.to_le_bytes());
    // cup_position: u16
    buf.extend_from_slice(&3u16.to_le_bytes());
    // track_position: u16
    buf.extend_from_slice(&5u16.to_le_bytes());
    // spline_position: f32
    buf.extend_from_slice(&0.45f32.to_le_bytes());
    // laps: u16
    buf.extend_from_slice(&7u16.to_le_bytes());
    // delta_ms: i32
    buf.extend_from_slice(&(-350i32).to_le_bytes());
    // 3 lap time blocks (best, last, current)
    push_acc_lap(&mut buf, 98_500);
    push_acc_lap(&mut buf, 99_200);
    push_acc_lap(&mut buf, 45_000);
    buf
}

#[test]
fn acc_realistic_snapshot() -> TestResult {
    let adapter = ACCAdapter::new();
    let raw = make_acc_realtime_car_update();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Assetto Corsa (original) ─────────────────────────────────────────────────
// RTCarInfo: 328-byte UDP packet. Key offsets:
//   speed_Ms@16(f32), gas@56(f32), brake@60(f32), clutch@64(f32),
//   rpm@68(f32), steer@72(f32), gear@76(i32: 0=R,1=N,2=1st,...)

fn make_assetto_corsa_data() -> Vec<u8> {
    let mut buf = vec![0u8; 328];
    write_f32_le(&mut buf, 16, 50.0); // speed_Ms ≈ 180 km/h
    write_f32_le(&mut buf, 56, 0.80); // gas (throttle)
    write_f32_le(&mut buf, 60, 0.10); // brake
    write_f32_le(&mut buf, 64, 0.0); // clutch
    write_f32_le(&mut buf, 68, 8000.0); // rpm
    write_f32_le(&mut buf, 72, 0.12); // steer (slight right)
    // gear: AC encoding 6 → 5th gear (0=R, 1=N, 2=1st, 3=2nd, 4=3rd, 5=4th, 6=5th)
    write_i32_le(&mut buf, 76, 6);
    buf
}

#[test]
fn assetto_corsa_realistic_snapshot() -> TestResult {
    let adapter = AssettoCorsaAdapter::new();
    let normalized = adapter.normalize(&make_assetto_corsa_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── BeamNG ───────────────────────────────────────────────────────────────────
// OutGauge 96-byte packet. Key offsets:
//   gear@10(u8: 0=R,1=N,2=1st,...), speed@12(f32 m/s), rpm@16(f32),
//   throttle@48(f32), brake@52(f32), clutch@56(f32)

fn make_beamng_data() -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    buf[10] = 5; // gear raw 5 → 4th gear (5 - 1 = 4)
    write_f32_le(&mut buf, 12, 50.0); // speed m/s ≈ 180 km/h
    write_f32_le(&mut buf, 16, 8000.0); // rpm
    write_f32_le(&mut buf, 48, 0.80); // throttle
    write_f32_le(&mut buf, 52, 0.10); // brake
    write_f32_le(&mut buf, 56, 0.0); // clutch
    buf
}

#[test]
fn beamng_realistic_snapshot() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let normalized = adapter.normalize(&make_beamng_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── LFS (Live for Speed) ────────────────────────────────────────────────────
// OutGauge 92-byte packet (same layout as BeamNG), plus fuel@28(f32 0..1).

fn make_lfs_data() -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    buf[10] = 5; // gear raw 5 → 4th gear
    write_f32_le(&mut buf, 12, 50.0); // speed m/s ≈ 180 km/h
    write_f32_le(&mut buf, 16, 8000.0); // rpm
    write_f32_le(&mut buf, 28, 0.65); // fuel (0..1)
    write_f32_le(&mut buf, 48, 0.80); // throttle
    write_f32_le(&mut buf, 52, 0.10); // brake
    write_f32_le(&mut buf, 56, 0.0); // clutch
    buf
}

#[test]
fn lfs_realistic_snapshot() -> TestResult {
    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&make_lfs_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── F1 25 ───────────────────────────────────────────────────────────────────
// Uses the public builder to construct a Car Telemetry packet (ID 6).
// normalize() accepts a telemetry packet and uses default status values.

#[test]
fn f1_25_realistic_snapshot() -> TestResult {
    use openracing_telemetry_adapters::f1_25::build_car_telemetry_packet;

    let adapter = F1_25Adapter::new();
    let raw = build_car_telemetry_packet(
        0,                        // player_index
        280,                      // speed_kmh
        4,                        // gear (4th)
        11500,                    // engine_rpm
        0.80,                     // throttle
        0.10,                     // brake
        0,                        // DRS off
        [23.5, 23.8, 22.9, 23.1], // tyre pressures PSI [RL, RR, FL, FR]
    );
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── SimHub ──────────────────────────────────────────────────────────────────
// JSON UDP packet. Throttle/Brake/Clutch are 0–100, SteeringAngle in degrees.

fn make_simhub_data() -> Vec<u8> {
    let json = r#"{
        "SpeedMs": 50.0,
        "Rpms": 8000.0,
        "MaxRpms": 9500.0,
        "Gear": "4",
        "Throttle": 80.0,
        "Brake": 10.0,
        "Clutch": 0.0,
        "SteeringAngle": -45.0,
        "FuelPercent": 62.5,
        "LateralGForce": 1.1,
        "LongitudinalGForce": -0.3,
        "FFBValue": 0.42,
        "IsRunning": true,
        "IsInPit": false
    }"#;
    json.as_bytes().to_vec()
}

#[test]
fn simhub_realistic_snapshot() -> TestResult {
    let adapter = SimHubAdapter::new();
    let normalized = adapter.normalize(&make_simhub_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC Generations ─────────────────────────────────────────────────────────
// Codemasters Mode 1 / RallyEngine 264-byte binary packet.
// Wheel speeds, throttle, steer, brake, gear, RPM etc. at known offsets.

fn make_wrc_generations_data() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    // Body velocity (m/s) for slip ratio derivation
    write_f32_le(&mut buf, 32, 49.0); // velocity_x
    write_f32_le(&mut buf, 36, 1.0); // velocity_y
    write_f32_le(&mut buf, 40, 10.0); // velocity_z
    // Wheel speeds (m/s) → speed = average
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

#[test]
fn wrc_generations_realistic_snapshot() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let normalized = adapter.normalize(&make_wrc_generations_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Project CARS 2 ──────────────────────────────────────────────────────────
// Simplified UDP/shared-memory packet, min 84 bytes.
// steering@40, throttle@44, brake@48, speed@52, rpm@56, max_rpm@60, gear@80(u32).

fn make_pcars2_data() -> Vec<u8> {
    let mut buf = vec![0u8; 46];
    buf[44] = (0.12f32 * 127.0) as i8 as u8; // steering i8 [-127,+127]
    buf[30] = (0.80f32 * 255.0) as u8; // throttle u8 [0-255]
    buf[29] = (0.10f32 * 255.0) as u8; // brake u8 [0-255]
    write_f32_le(&mut buf, 36, 50.0); // speed f32 m/s
    buf[40..42].copy_from_slice(&8000u16.to_le_bytes()); // rpm u16
    buf[42..44].copy_from_slice(&9500u16.to_le_bytes()); // max_rpm u16
    buf[45] = 4 | (6 << 4); // gear=4, num_gears=6
    buf
}

#[test]
fn pcars2_realistic_snapshot() -> TestResult {
    let adapter = PCars2Adapter::new();
    let normalized = adapter.normalize(&make_pcars2_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
