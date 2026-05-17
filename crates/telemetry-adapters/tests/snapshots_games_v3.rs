//! Snapshot tests for additional telemetry adapters (v3).
//!
//! Covers: Assetto Corsa, Automobilista 1, DiRT 3, DiRT 4, DiRT 5,
//! DiRT Rally 2.0, Dirt Showdown, EA WRC, F1, Gran Turismo 7, GRID 2019,
//! GRID Legends, KartKraft, Le Mans Ultimate, NASCAR 21, Race Driver GRID.

use openracing_telemetry_adapters::{
    AssettoCorsaAdapter, Automobilista1Adapter, Dirt3Adapter, Dirt4Adapter, Dirt5Adapter,
    DirtRally2Adapter, DirtShowdownAdapter, EAWRCAdapter, F1Adapter, Grid2019Adapter,
    GridLegendsAdapter, KartKraftAdapter, LeMansUltimateAdapter, Nascar21Adapter,
    RaceDriverGridAdapter, TelemetryAdapter, gran_turismo_7,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_f64(buf: &mut [u8], offset: usize, val: f64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

// ─── Assetto Corsa ───────────────────────────────────────────────────────────

fn make_assetto_corsa_packet() -> Vec<u8> {
    let mut data = vec![0u8; 328]; // RTCarInfo struct size
    write_f32_le(&mut data, 8, 100.0); // speed_Kmh at offset 8
    write_f32_le(&mut data, 16, 100.0 / 3.6); // speed_Ms at offset 16 (~27.78)
    write_f32_le(&mut data, 56, 0.75); // gas at offset 56
    write_f32_le(&mut data, 60, 0.0); // brake at offset 60
    write_f32_le(&mut data, 68, 5500.0); // engineRPM at offset 68
    write_f32_le(&mut data, 72, 0.30); // steer at offset 72
    write_i32(&mut data, 76, 3); // gear at offset 76 (AC: 3 = 2nd)
    data
}

#[test]
fn assetto_corsa_snapshot() -> TestResult {
    let packet = make_assetto_corsa_packet();
    let normalized = AssettoCorsaAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Automobilista 1 ─────────────────────────────────────────────────────────

fn make_automobilista_packet() -> Vec<u8> {
    let mut data = vec![0u8; 532];
    write_f64(&mut data, 216, 9.81 * 0.4); // lateral accel (0.4 G in m/s²)
    write_f64(&mut data, 232, 9.81 * 0.2); // longitudinal accel (0.2 G)
    write_i32(&mut data, 360, 3); // gear (3rd)
    write_f64(&mut data, 368, 5800.0); // engine_rpm
    write_f64(&mut data, 384, 8000.0); // engine_max_rpm
    data[457] = 60u8; // fuel_capacity (litres, u8)
    write_f32_le(&mut data, 460, 42.0); // fuel_in_tank
    write_f32_le(&mut data, 492, 0.70); // throttle
    write_f32_le(&mut data, 496, 0.0); // brake
    write_f32_le(&mut data, 500, -0.15); // steering
    write_f32_le(&mut data, 528, 32.0); // speed_ms
    data
}

#[test]
fn automobilista_snapshot() -> TestResult {
    let packet = make_automobilista_packet();
    let normalized = Automobilista1Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Codemasters Mode 1 shared packet builder ─────────────────────────────────
// Used by: DiRT 3, DiRT 4, DiRT Rally 2.0, Dirt Showdown, GRID 2019,
//          GRID Legends, Race Driver GRID

fn make_codemasters_mode1_packet() -> Vec<u8> {
    let mut data = vec![0u8; 264];
    write_f32_le(&mut data, 100, 25.0); // wheel speed RL (m/s)
    write_f32_le(&mut data, 104, 25.0); // wheel speed RR
    write_f32_le(&mut data, 108, 25.0); // wheel speed FL
    write_f32_le(&mut data, 112, 25.0); // wheel speed FR
    write_f32_le(&mut data, 116, 0.85); // throttle
    write_f32_le(&mut data, 120, 0.10); // steer
    write_f32_le(&mut data, 124, 0.0); // brake
    write_f32_le(&mut data, 132, 3.0); // gear (f32: 3 = 3rd)
    write_f32_le(&mut data, 136, 0.25); // gforce_lat
    write_f32_le(&mut data, 140, 0.50); // gforce_lon
    write_f32_le(&mut data, 148, 4800.0); // rpm
    write_f32_le(&mut data, 180, 35.0); // fuel_in_tank
    write_f32_le(&mut data, 184, 55.0); // fuel_capacity
    write_f32_le(&mut data, 252, 7200.0); // max_rpm
    data
}

// ─── DiRT 3 ──────────────────────────────────────────────────────────────────

#[test]
fn dirt3_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = Dirt3Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── DiRT 4 ──────────────────────────────────────────────────────────────────

#[test]
fn dirt4_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = Dirt4Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── DiRT 5 ──────────────────────────────────────────────────────────────────
// Uses CustomUdpSpec mode 1: base(7) + mode1-extra(8) = 15 fields × 4 bytes = 60 bytes.
// Base: speed, engine_rate, gear(i32), steering_input, throttle_input, brake_input, clutch_input
// Mode 1 extra: wheel_patch_speed_fl/fr/rl/rr, suspension_position_fl/fr/rl/rr

fn make_dirt5_packet() -> Vec<u8> {
    let mut data = Vec::with_capacity(60);
    data.extend_from_slice(&30.0f32.to_le_bytes()); // speed (m/s)
    data.extend_from_slice(&419.0f32.to_le_bytes()); // engine_rate (rad/s ≈ 4000 RPM)
    data.extend_from_slice(&3i32.to_le_bytes()); // gear
    data.extend_from_slice(&0.10f32.to_le_bytes()); // steering_input
    data.extend_from_slice(&0.80f32.to_le_bytes()); // throttle_input
    data.extend_from_slice(&0.0f32.to_le_bytes()); // brake_input
    data.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input
    data.extend_from_slice(&29.0f32.to_le_bytes()); // wheel_patch_speed_fl
    data.extend_from_slice(&29.0f32.to_le_bytes()); // wheel_patch_speed_fr
    data.extend_from_slice(&28.5f32.to_le_bytes()); // wheel_patch_speed_rl
    data.extend_from_slice(&28.5f32.to_le_bytes()); // wheel_patch_speed_rr
    data.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_fl
    data.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_fr
    data.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_rl
    data.extend_from_slice(&0.01f32.to_le_bytes()); // suspension_position_rr
    data
}

#[test]
fn dirt5_snapshot() -> TestResult {
    let packet = make_dirt5_packet();
    let normalized = Dirt5Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── DiRT Rally 2.0 ──────────────────────────────────────────────────────────

#[test]
fn dirt_rally_2_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = DirtRally2Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Dirt Showdown ───────────────────────────────────────────────────────────

#[test]
fn dirt_showdown_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = DirtShowdownAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── EA WRC ──────────────────────────────────────────────────────────────────
// Requires filesystem JSON config; written to a tempdir to avoid touching the
// real EA WRC telemetry directory.

#[test]
fn eawrc_snapshot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let readme_dir = dir.path().join("readme");
    let udp_dir = dir.path().join("udp");
    std::fs::create_dir_all(&readme_dir)?;
    std::fs::create_dir_all(&udp_dir)?;

    let channels_json = serde_json::json!({
        "versions": { "schema": 1, "data": 1 },
        "channels": [
            { "id": "packet_uid",    "type": "fourCC" },
            { "id": "ffb_scalar",    "type": "f32"    },
            { "id": "engine_rpm",    "type": "f32"    },
            { "id": "vehicle_speed", "type": "f32"    },
            { "id": "gear",          "type": "i8"     }
        ]
    });
    let structure_json = serde_json::json!({
        "id": "openracing",
        "packets": [{
            "id": "session_update",
            "header": { "channels": ["packet_uid"] },
            "channels": ["ffb_scalar", "engine_rpm", "vehicle_speed", "gear"]
        }]
    });

    std::fs::write(
        readme_dir.join("channels.json"),
        serde_json::to_string(&channels_json)?,
    )?;
    std::fs::write(
        udp_dir.join("openracing.json"),
        serde_json::to_string(&structure_json)?,
    )?;

    // Packet layout (matches session_update): fourCC + f32 + f32 + f32 + i8
    let mut packet = Vec::new();
    packet.extend_from_slice(b"SU01"); // packet_uid (fourCC, 4 bytes)
    packet.extend_from_slice(&0.60f32.to_le_bytes()); // ffb_scalar
    packet.extend_from_slice(&6400.0f32.to_le_bytes()); // engine_rpm
    packet.extend_from_slice(&51.0f32.to_le_bytes()); // vehicle_speed (m/s)
    packet.push(4i8.to_le_bytes()[0]); // gear

    let adapter = EAWRCAdapter::with_telemetry_dir(dir.path().to_owned());
    let normalized = adapter.normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── F1 ──────────────────────────────────────────────────────────────────────
// Uses CustomUdpSpec mode 3: base(7) + mode3-extra(15) = 22 fields × 4 bytes = 88 bytes.
// Mode 3 extra: wheel_patch_speed_fl/fr/rl/rr, suspension_velocity_fl/fr/rl/rr,
//               suspension_position_fl/fr/rl/rr, long_accel, lat_accel, vert_accel

fn make_f1_packet() -> Vec<u8> {
    let mut data = Vec::with_capacity(88);
    // base fields
    data.extend_from_slice(&55.0f32.to_le_bytes()); // speed (m/s)
    data.extend_from_slice(&700.0f32.to_le_bytes()); // engine_rate (rad/s ≈ 6685 RPM)
    data.extend_from_slice(&5i32.to_le_bytes()); // gear
    data.extend_from_slice(&(-0.05f32).to_le_bytes()); // steering_input
    data.extend_from_slice(&0.90f32.to_le_bytes()); // throttle_input
    data.extend_from_slice(&0.0f32.to_le_bytes()); // brake_input
    data.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input
    // mode 3 extra: wheel_patch_speed fl/fr/rl/rr
    for _ in 0..4 {
        data.extend_from_slice(&55.0f32.to_le_bytes());
    }
    // suspension_velocity fl/fr/rl/rr
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // suspension_position fl/fr/rl/rr
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    data.extend_from_slice(&0.50f32.to_le_bytes()); // long_accel
    data.extend_from_slice(&(-0.30f32).to_le_bytes()); // lat_accel
    data.extend_from_slice(&0.0f32.to_le_bytes()); // vert_accel
    data
}

#[test]
fn f1_snapshot() -> TestResult {
    let packet = make_f1_packet();
    let normalized = F1Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo 7 ──────────────────────────────────────────────────────────
// GranTurismo7Adapter::normalize() requires Salsa20-encrypted input; test the
// parse step directly via gran_turismo_7::parse_decrypted with a 296-byte buffer.

fn make_gt7_decrypted_packet() -> [u8; gran_turismo_7::PACKET_SIZE] {
    let mut buf = [0u8; gran_turismo_7::PACKET_SIZE];
    write_u32(&mut buf, gran_turismo_7::OFF_MAGIC, gran_turismo_7::MAGIC);
    write_f32_le(&mut buf, 0x3C, 8500.0); // engine_rpm
    write_f32_le(&mut buf, 0x44, 35.0); // fuel_level
    write_f32_le(&mut buf, 0x48, 50.0); // fuel_capacity
    write_f32_le(&mut buf, 0x4C, 80.0); // speed_ms
    write_f32_le(&mut buf, 0x58, 92.0); // water_temp_c
    write_f32_le(&mut buf, 0x60, 85.0); // tire_temp_fl
    write_f32_le(&mut buf, 0x64, 87.0); // tire_temp_fr
    write_f32_le(&mut buf, 0x68, 83.0); // tire_temp_rl
    write_f32_le(&mut buf, 0x6C, 84.0); // tire_temp_rr
    write_u16(&mut buf, 0x74, 7); // lap_count (i16)
    write_i32(&mut buf, 0x78, 85_000); // best_lap_ms
    write_i32(&mut buf, 0x7C, 87_500); // last_lap_ms
    write_u16(&mut buf, 0x8A, 9200); // max_alert_rpm (i16)
    write_u16(&mut buf, 0x8E, 0); // flags (i16)
    buf[0x90] = 5u8; // gear_byte (5th gear, low nibble)
    buf[0x91] = (0.70f32 * 255.0) as u8; // throttle
    buf[0x92] = 0u8; // brake
    write_i32(&mut buf, 0x124, 4444); // car_code
    buf
}

#[test]
fn gran_turismo_7_snapshot() -> TestResult {
    let buf = make_gt7_decrypted_packet();
    let normalized = gran_turismo_7::parse_decrypted(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── GRID 2019 ───────────────────────────────────────────────────────────────

#[test]
fn grid_2019_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = Grid2019Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── GRID Legends ────────────────────────────────────────────────────────────

#[test]
fn grid_legends_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = GridLegendsAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── KartKraft ───────────────────────────────────────────────────────────────
// KartKraft uses FlatBuffers binary format; build a minimal valid packet that
// mirrors the test helper in kartkraft.rs (root_offset + "KKFB" identifier,
// Frame table → Dashboard table).

fn make_kartkraft_packet(
    speed: f32,
    rpm: f32,
    steer_deg: f32,
    throttle: f32,
    brake: f32,
    gear: i8,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();

    let push_u16 = |buf: &mut Vec<u8>, v: u16| buf.extend_from_slice(&v.to_le_bytes());
    let push_i32 = |buf: &mut Vec<u8>, v: i32| buf.extend_from_slice(&v.to_le_bytes());
    let push_u32 = |buf: &mut Vec<u8>, v: u32| buf.extend_from_slice(&v.to_le_bytes());
    let push_f32 = |buf: &mut Vec<u8>, v: f32| buf.extend_from_slice(&v.to_le_bytes());

    // Reserve root_offset placeholder + file identifier.
    push_u32(&mut buf, 0); // placeholder
    buf.extend_from_slice(b"KKFB");

    // Frame vtable (timestamp and motion absent; dash present at field offset 4).
    let vt_frame_start = buf.len();
    push_u16(&mut buf, 10); // vtable_size
    push_u16(&mut buf, 12); // object_size
    push_u16(&mut buf, 0); // field 0 (timestamp) absent
    push_u16(&mut buf, 0); // field 1 (motion) absent
    push_u16(&mut buf, 4); // field 2 (dash) at byte offset 4 from frame_table

    // Frame table.
    let frame_table_pos = buf.len();
    push_i32(&mut buf, (frame_table_pos - vt_frame_start) as i32); // soffset
    push_u32(&mut buf, 0); // dash UOffset placeholder (patched below)
    push_u32(&mut buf, 0); // padding

    // Patch root_offset.
    let root_val = frame_table_pos as u32;
    buf[0..4].copy_from_slice(&root_val.to_le_bytes());

    // Dashboard vtable (6 scalar fields: speed, rpm, steer, throttle, brake, gear).
    let vt_dash_start = buf.len();
    push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
    push_u16(&mut buf, 28); // object_size = 4 + 6*4
    push_u16(&mut buf, 4); // speed
    push_u16(&mut buf, 8); // rpm
    push_u16(&mut buf, 12); // steer
    push_u16(&mut buf, 16); // throttle
    push_u16(&mut buf, 20); // brake
    push_u16(&mut buf, 24); // gear

    // Dashboard table.
    let dash_table_pos = buf.len();
    push_i32(&mut buf, (dash_table_pos - vt_dash_start) as i32);
    push_f32(&mut buf, speed);
    push_f32(&mut buf, rpm);
    push_f32(&mut buf, steer_deg);
    push_f32(&mut buf, throttle);
    push_f32(&mut buf, brake);
    buf.push(gear as u8);
    buf.push(0);
    buf.push(0);
    buf.push(0);

    // Patch dash UOffset: ref_pos = frame_table_pos + 4; uoffset = dash_table_pos - ref_pos.
    let ref_pos = frame_table_pos + 4;
    let dash_uoffset = (dash_table_pos - ref_pos) as u32;
    buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

    buf
}

#[test]
fn kartkraft_snapshot() -> TestResult {
    let packet = make_kartkraft_packet(25.0, 8500.0, 15.0, 0.9, 0.0, 3);
    let normalized = KartKraftAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Le Mans Ultimate ────────────────────────────────────────────────────────

fn make_le_mans_ultimate_packet() -> Vec<u8> {
    let mut data = vec![0u8; 20];
    write_f32_le(&mut data, 0, 45.0); // speed_ms
    write_f32_le(&mut data, 4, 7200.0); // rpm
    write_f32_le(&mut data, 8, 4.0); // gear (4th)
    write_f32_le(&mut data, 12, 0.60); // throttle
    write_f32_le(&mut data, 16, 0.0); // brake
    data
}

#[test]
fn le_mans_ultimate_snapshot() -> TestResult {
    let packet = make_le_mans_ultimate_packet();
    let normalized = LeMansUltimateAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── NASCAR 21: Ignition ─────────────────────────────────────────────────────
// Papyrus UDP format (identical to NASCAR Heat series), 92 bytes minimum.

fn make_nascar21_packet() -> Vec<u8> {
    let mut data = vec![0u8; 92];
    write_f32_le(&mut data, 16, 60.0); // speed_ms
    write_f32_le(&mut data, 32, 4.9); // acc_x  (longitudinal, m/s²)
    write_f32_le(&mut data, 36, 9.8); // acc_y  (lateral, m/s²)
    write_f32_le(&mut data, 68, 3.0); // gear   (f32, 3 = 3rd)
    write_f32_le(&mut data, 72, 5500.0); // rpm
    write_f32_le(&mut data, 80, 0.90); // throttle
    write_f32_le(&mut data, 84, 0.0); // brake
    write_f32_le(&mut data, 88, 0.20); // steer
    data
}

#[test]
fn nascar_21_snapshot() -> TestResult {
    let packet = make_nascar21_packet();
    let normalized = Nascar21Adapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Race Driver GRID ────────────────────────────────────────────────────────

#[test]
fn race_driver_grid_snapshot() -> TestResult {
    let packet = make_codemasters_mode1_packet();
    let normalized = RaceDriverGridAdapter::new().normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
