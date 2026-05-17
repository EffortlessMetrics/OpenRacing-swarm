//! Snapshot tests for rally / offroad-themed telemetry adapters.
//!
//! Covers: EA WRC, WRC Generations, WRC Kylotonn (WRC 10), Seb Loeb Rally,
//! V-Rally 4, Dakar Desert Rally, Gravel, and FlatOut.

use openracing_telemetry_adapters::{
    DakarDesertRallyAdapter, EAWRCAdapter, FlatOutAdapter, GravelAdapter, SebLoebRallyAdapter,
    TelemetryAdapter, VRally4Adapter, WrcGenerationsAdapter,
    wrc_kylotonn::{WrcKylotonnAdapter, WrcKylotonnVariant},
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

// ─── EA WRC ──────────────────────────────────────────────────────────────────
// Schema-driven adapter: needs temp config files on disk. Simulates a rally
// stage packet with steering, throttle, brake, RPM, speed, and gear channels.

#[test]
fn eawrc_rally_stage_snapshot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let readme_dir = dir.path().join("readme");
    let udp_dir = dir.path().join("udp");
    std::fs::create_dir_all(&readme_dir)?;
    std::fs::create_dir_all(&udp_dir)?;

    let channels_json = serde_json::json!({
        "versions": { "schema": 1, "data": 1 },
        "channels": [
            { "id": "packet_uid",    "type": "fourCC" },
            { "id": "ffb_scalar",    "type": "f32" },
            { "id": "engine_rpm",    "type": "f32" },
            { "id": "vehicle_speed", "type": "f32" },
            { "id": "gear",          "type": "i8"  },
            { "id": "slip_ratio",    "type": "f32" }
        ]
    });
    let structure_json = serde_json::json!({
        "id": "openracing",
        "packets": [{
            "id": "session_update",
            "header": { "channels": ["packet_uid"] },
            "channels": ["ffb_scalar", "engine_rpm", "vehicle_speed", "gear", "slip_ratio"]
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

    // fourCC + f32 + f32 + f32 + i8 + f32
    let mut packet = Vec::new();
    packet.extend_from_slice(b"SU01"); // packet_uid
    packet.extend_from_slice(&0.55f32.to_le_bytes()); // ffb_scalar
    packet.extend_from_slice(&5800.0f32.to_le_bytes()); // engine_rpm (gravel stage)
    packet.extend_from_slice(&28.0f32.to_le_bytes()); // vehicle_speed m/s (~100 km/h)
    packet.push(3i8.to_le_bytes()[0]); // gear (3rd)
    packet.extend_from_slice(&0.12f32.to_le_bytes()); // slip_ratio (loose gravel)

    let adapter = EAWRCAdapter::with_telemetry_dir(dir.path().to_owned());
    let normalized = adapter.normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC Generations ─────────────────────────────────────────────────────────
// Codemasters Mode 1 / RallyEngine 264-byte binary packet.

fn make_wrc_generations_rally_data() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 32, 26.0); // velocity_x (m/s)
    write_f32_le(&mut buf, 36, 2.0); // velocity_y
    write_f32_le(&mut buf, 40, 5.0); // velocity_z
    write_f32_le(&mut buf, 100, 27.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 27.0); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 28.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 28.0); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.90); // throttle (aggressive stage)
    write_f32_le(&mut buf, 120, 0.25); // steer (left turn)
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 3.0); // gear (3rd)
    write_f32_le(&mut buf, 136, 1.20); // gforce_lat (sliding)
    write_f32_le(&mut buf, 140, 0.40); // gforce_lon
    write_f32_le(&mut buf, 148, 6200.0); // rpm
    write_f32_le(&mut buf, 180, 35.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 55.0); // fuel_capacity
    write_f32_le(&mut buf, 252, 8000.0); // max_rpm
    write_f32_le(&mut buf, 260, 6.0); // max_gears
    buf
}

#[test]
fn wrc_generations_rally_snapshot() -> TestResult {
    let adapter = WrcGenerationsAdapter::new();
    let normalized = adapter.normalize(&make_wrc_generations_rally_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WRC Kylotonn (WRC 10) ──────────────────────────────────────────────────
// Custom 96-byte binary packet: speed, steering, throttle, brake, clutch, gear
// (u32), rpm, max_rpm, suspension×4, pos, orientation, wheel_speed×4.

fn make_wrc_kylotonn_rally_data() -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    write_f32_le(&mut buf, 0, 0.42); // stage_progress (42 %)
    write_f32_le(&mut buf, 4, 22.0); // road_speed_ms (~79 km/h)
    write_f32_le(&mut buf, 8, -0.30); // steering (left)
    write_f32_le(&mut buf, 12, 0.85); // throttle
    write_f32_le(&mut buf, 16, 0.0); // brake
    write_f32_le(&mut buf, 20, 0.0); // hand_brake
    write_f32_le(&mut buf, 24, 0.0); // clutch
    write_u32_le(&mut buf, 28, 3); // gear (3rd)
    write_f32_le(&mut buf, 32, 5500.0); // rpm
    write_f32_le(&mut buf, 36, 7500.0); // max_rpm
    write_f32_le(&mut buf, 40, 0.02); // suspension_fl
    write_f32_le(&mut buf, 44, 0.01); // suspension_fr
    write_f32_le(&mut buf, 48, 0.03); // suspension_rl
    write_f32_le(&mut buf, 52, 0.02); // suspension_rr
    write_f32_le(&mut buf, 56, 120.0); // pos_x
    write_f32_le(&mut buf, 60, 45.0); // pos_y
    write_f32_le(&mut buf, 64, -30.0); // pos_z
    write_f32_le(&mut buf, 68, 0.05); // roll
    write_f32_le(&mut buf, 72, 0.02); // pitch
    write_f32_le(&mut buf, 76, 1.10); // yaw
    write_f32_le(&mut buf, 80, 22.5); // wheel_speed_fl
    write_f32_le(&mut buf, 84, 22.3); // wheel_speed_fr
    write_f32_le(&mut buf, 88, 21.8); // wheel_speed_rl
    write_f32_le(&mut buf, 92, 22.0); // wheel_speed_rr
    buf
}

#[test]
fn wrc_kylotonn_rally_snapshot() -> TestResult {
    let adapter = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
    let normalized = adapter.normalize(&make_wrc_kylotonn_rally_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Seb Loeb Rally ─────────────────────────────────────────────────────────
// Stub adapter — normalize always returns defaults regardless of input.

#[test]
fn seb_loeb_rally_snapshot() -> TestResult {
    let adapter = SebLoebRallyAdapter::new();
    let normalized = adapter.normalize(&[])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── V-Rally 4 ──────────────────────────────────────────────────────────────
// Kylotonn format, 96 bytes. Offsets shared with WRC Kylotonn (speed at 4,
// steering at 8, throttle at 12, brake at 16, gear u32 at 28, rpm at 32, etc.)

fn make_v_rally_4_data() -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    write_f32_le(&mut buf, 4, 35.0); // speed_ms (~126 km/h)
    write_f32_le(&mut buf, 8, 0.10); // steering (slight right)
    write_f32_le(&mut buf, 12, 0.75); // throttle
    write_f32_le(&mut buf, 16, 0.05); // brake (trail-braking)
    write_f32_le(&mut buf, 24, 0.0); // clutch
    write_u32_le(&mut buf, 28, 4); // gear (4th)
    write_f32_le(&mut buf, 32, 6800.0); // rpm
    write_f32_le(&mut buf, 36, 8500.0); // max_rpm
    write_f32_le(&mut buf, 56, 200.0); // pos_x
    write_f32_le(&mut buf, 60, 80.0); // pos_y
    write_f32_le(&mut buf, 64, -15.0); // pos_z
    write_f32_le(&mut buf, 80, 12.0); // vel_x
    write_f32_le(&mut buf, 84, 1.0); // vel_y
    write_f32_le(&mut buf, 88, 33.0); // vel_z
    buf
}

#[test]
fn v_rally_4_rally_snapshot() -> TestResult {
    let adapter = VRally4Adapter::new();
    let normalized = adapter.normalize(&make_v_rally_4_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Dakar Desert Rally ──────────────────────────────────────────────────────
// Bridge UDP, 40-byte minimum, starts with "DAKR" magic. Gear 255 = reverse.

fn make_dakar_desert_stage_data() -> Vec<u8> {
    let mut buf = vec![0u8; 40];
    buf[0..4].copy_from_slice(&[0x44, 0x41, 0x4B, 0x52]); // "DAKR"
    buf[4..8].copy_from_slice(&42u32.to_le_bytes()); // sequence
    write_f32_le(&mut buf, 8, 33.0); // speed_ms (~119 km/h desert stage)
    write_f32_le(&mut buf, 12, 4200.0); // rpm
    buf[16] = 4; // gear (4th)
    write_f32_le(&mut buf, 20, 0.60); // lateral_g (dune drift)
    write_f32_le(&mut buf, 24, 0.35); // longitudinal_g
    write_f32_le(&mut buf, 28, 0.95); // throttle (full send)
    write_f32_le(&mut buf, 32, 0.0); // brake
    write_f32_le(&mut buf, 36, -0.08); // steering_angle (slight left)
    buf
}

#[test]
fn dakar_desert_stage_snapshot() -> TestResult {
    let adapter = DakarDesertRallyAdapter::new();
    let normalized = adapter.normalize(&make_dakar_desert_stage_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gravel ──────────────────────────────────────────────────────────────────
// SimHub JSON UDP bridge. Throttle/Brake/Clutch are 0–100, SteeringAngle in
// degrees.

fn make_gravel_rallycross_data() -> Vec<u8> {
    let json = r#"{
        "SpeedMs": 30.0,
        "Rpms": 6500.0,
        "MaxRpms": 8500.0,
        "Gear": "3",
        "Throttle": 85.0,
        "Brake": 5.0,
        "Clutch": 0.0,
        "SteeringAngle": -60.0,
        "FuelPercent": 55.0,
        "LateralGForce": 1.4,
        "LongitudinalGForce": 0.3,
        "FFBValue": 0.50,
        "IsRunning": true,
        "IsInPit": false
    }"#;
    json.as_bytes().to_vec()
}

#[test]
fn gravel_rallycross_snapshot() -> TestResult {
    let adapter = GravelAdapter::new();
    let normalized = adapter.normalize(&make_gravel_rallycross_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── FlatOut ─────────────────────────────────────────────────────────────────
// Bridge UDP, 36-byte minimum, starts with "FOTC" magic.

fn make_flatout_demolition_data() -> Vec<u8> {
    let mut buf = vec![0u8; 36];
    buf[0..4].copy_from_slice(&[0x46, 0x4F, 0x54, 0x43]); // "FOTC"
    buf[4..8].copy_from_slice(&99u32.to_le_bytes()); // sequence
    write_f32_le(&mut buf, 8, 18.0); // speed_ms (~65 km/h demolition derby)
    write_f32_le(&mut buf, 12, 5200.0); // rpm
    buf[16] = 2; // gear (2nd)
    write_f32_le(&mut buf, 20, 1.80); // lateral_g (hard impact)
    write_f32_le(&mut buf, 24, -0.90); // longitudinal_g (braking impact)
    write_f32_le(&mut buf, 28, 1.0); // throttle (floored)
    write_f32_le(&mut buf, 32, 0.0); // brake
    buf
}

#[test]
fn flatout_demolition_race_snapshot() -> TestResult {
    let adapter = FlatOutAdapter::new();
    let normalized = adapter.normalize(&make_flatout_demolition_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
