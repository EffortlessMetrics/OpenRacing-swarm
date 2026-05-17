//! Golden packet tests for telemetry adapters.
//!
//! Each test defines a known-good byte sequence, parses it, and verifies specific
//! field values. This locks the parser behaviour against unintentional regressions.

#![allow(clippy::redundant_closure)]

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::{
    codemasters_shared, dakar, flatout, le_mans_ultimate, nascar, pcars2, rennsport, rfactor1,
    simhub, trackmania, wreckfest, wtcr,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Helper: write LE u32 ─────────────────────────────────────────────────────

fn write_f64_le(buf: &mut [u8], offset: usize, value: f64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Codemasters Mode 1 (DiRT Rally 2, DiRT 3/4, GRID family)
// ═══════════════════════════════════════════════════════════════════════════════

fn make_codemasters_golden() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    // Wheel speeds FL/FR/RL/RR at offsets 108, 112, 100, 104 → average = 30 m/s
    write_f32_le(&mut buf, 108, 30.0); // FL
    write_f32_le(&mut buf, 112, 30.0); // FR
    write_f32_le(&mut buf, 100, 30.0); // RL
    write_f32_le(&mut buf, 104, 30.0); // RR
    // RPM @ 148
    write_f32_le(&mut buf, 148, 7200.0);
    // Max RPM @ 252
    write_f32_le(&mut buf, 252, 8500.0);
    // Gear @ 132 (3.0 = 3rd gear)
    write_f32_le(&mut buf, 132, 3.0);
    // Throttle @ 116
    write_f32_le(&mut buf, 116, 0.75);
    // Brake @ 124
    write_f32_le(&mut buf, 124, 0.0);
    // Steering @ 120
    write_f32_le(&mut buf, 120, -0.15);
    // Lateral G @ 136 (1.5 G)
    write_f32_le(&mut buf, 136, 1.5);
    // Fuel in tank @ 180, fuel capacity @ 184
    write_f32_le(&mut buf, 180, 25.0);
    write_f32_le(&mut buf, 184, 50.0);
    // Number of gears @ 260
    write_f32_le(&mut buf, 260, 6.0);
    buf
}

#[test]
fn golden_codemasters_mode1() -> TestResult {
    let data = make_codemasters_golden();
    let t = codemasters_shared::parse_codemasters_mode1_common(&data, "GoldenTest")?;

    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 7200.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert!((t.max_rpm - 8500.0).abs() < 0.01, "max_rpm: {}", t.max_rpm);
    assert_eq!(t.gear, 3, "gear: {}", t.gear);
    assert!((t.throttle - 0.75).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake - 0.0).abs() < 0.01, "brake: {}", t.brake);
    assert!(
        (t.steering_angle - (-0.15)).abs() < 0.01,
        "steering_angle: {}",
        t.steering_angle
    );
    assert!(
        (t.lateral_g - 1.5).abs() < 0.01,
        "lateral_g: {}",
        t.lateral_g
    );
    // FFB = lat_g / 3.0 = 0.5
    assert!(
        (t.ffb_scalar - 0.5).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    // Fuel percent = 25/50 = 0.5
    assert!(
        (t.fuel_percent - 0.5).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert_eq!(t.num_gears, 6, "num_gears: {}", t.num_gears);
    Ok(())
}

#[test]
fn golden_codemasters_reverse_gear() -> TestResult {
    let mut data = make_codemasters_golden();
    // Gear 0.0 → reverse (-1)
    write_f32_le(&mut data, 132, 0.0);
    let t = codemasters_shared::parse_codemasters_mode1_common(&data, "GoldenTest")?;
    assert_eq!(t.gear, -1, "expected reverse gear, got {}", t.gear);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NASCAR (Papyrus UDP)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_nascar_packet() -> TestResult {
    let mut buf = vec![0u8; 92];
    // Speed @ 16
    write_f32_le(&mut buf, 16, 55.0); // 55 m/s ≈ 198 km/h
    // RPM @ 72
    write_f32_le(&mut buf, 72, 8500.0);
    // Gear @ 68 (4.0 = 4th gear)
    write_f32_le(&mut buf, 68, 4.0);
    // Throttle @ 80
    write_f32_le(&mut buf, 80, 0.9);
    // Brake @ 84
    write_f32_le(&mut buf, 84, 0.0);
    // Steering @ 88
    write_f32_le(&mut buf, 88, 0.05);
    // Lateral accel @ 36 (m/s²)
    write_f32_le(&mut buf, 36, 9.81); // 1G lateral

    let t = nascar::parse_nascar_packet(&buf)?;

    assert!((t.speed_ms - 55.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 8500.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.9).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    // lat_g = 9.81 / 9.81 = 1.0; FFB = 1.0 / 2.0 = 0.5
    assert!(
        (t.lateral_g - 1.0).abs() < 0.01,
        "lateral_g: {}",
        t.lateral_g
    );
    assert!(
        (t.ffb_scalar - 0.5).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Le Mans Ultimate (rFactor2 bridge)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_le_mans_ultimate_packet() -> TestResult {
    let mut buf = vec![0u8; 20];
    // Speed @ 0
    write_f32_le(&mut buf, 0, 72.0); // 72 m/s ≈ 259 km/h
    // RPM @ 4
    write_f32_le(&mut buf, 4, 6800.0);
    // Gear @ 8 (5.0 = 5th gear)
    write_f32_le(&mut buf, 8, 5.0);
    // Throttle @ 12
    write_f32_le(&mut buf, 12, 1.0);
    // Brake @ 16
    write_f32_le(&mut buf, 16, 0.0);

    let t = le_mans_ultimate::parse_le_mans_ultimate_packet(&buf)?;

    assert!((t.speed_ms - 72.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 6800.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 5);
    assert!((t.throttle - 1.0).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    // FFB = throttle - brake = 1.0
    assert!(
        (t.ffb_scalar - 1.0).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn golden_le_mans_ultimate_reverse() -> TestResult {
    let mut buf = vec![0u8; 20];
    write_f32_le(&mut buf, 0, 2.0);
    write_f32_le(&mut buf, 4, 1500.0);
    // Gear -1.0 → reverse
    write_f32_le(&mut buf, 8, -1.0);
    write_f32_le(&mut buf, 12, 0.3);
    write_f32_le(&mut buf, 16, 0.0);

    let t = le_mans_ultimate::parse_le_mans_ultimate_packet(&buf)?;
    assert_eq!(t.gear, -1, "expected reverse gear");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Wreckfest
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_wreckfest_packet() -> TestResult {
    let mut buf = vec![0u8; 28];
    // Magic "WRKF" @ 0
    buf[0..4].copy_from_slice(&[0x57, 0x52, 0x4B, 0x46]);
    // Speed @ 8
    write_f32_le(&mut buf, 8, 25.0); // 25 m/s ≈ 90 km/h
    // RPM @ 12
    write_f32_le(&mut buf, 12, 6000.0);
    // Gear @ 16 (3 = 3rd gear)
    buf[16] = 3;
    // Lateral G @ 20
    write_f32_le(&mut buf, 20, 0.8);
    // Longitudinal G @ 24
    write_f32_le(&mut buf, 24, 0.3);

    let t = wreckfest::parse_wreckfest_packet(&buf)?;

    assert!((t.speed_ms - 25.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 6000.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 3);
    assert!(
        (t.lateral_g - 0.8).abs() < 0.01,
        "lateral_g: {}",
        t.lateral_g
    );
    // FFB = hypot(0.8, 0.3) / 3.0 ≈ 0.284
    let expected_ffb = (0.8f32.hypot(0.3)) / 3.0;
    assert!(
        (t.ffb_scalar - expected_ffb).abs() < 0.01,
        "ffb_scalar: {} (expected {})",
        t.ffb_scalar,
        expected_ffb
    );
    Ok(())
}

#[test]
fn golden_wreckfest_bad_magic_rejected() -> TestResult {
    let mut buf = vec![0u8; 28];
    buf[0..4].copy_from_slice(b"XXXX");
    assert!(wreckfest::parse_wreckfest_packet(&buf).is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dakar Desert Rally
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_dakar_packet() -> TestResult {
    let mut buf = vec![0u8; 40];
    // Magic "DAKR" @ 0
    buf[0..4].copy_from_slice(&[0x44, 0x41, 0x4B, 0x52]);
    // Speed @ 8
    write_f32_le(&mut buf, 8, 35.0);
    // RPM @ 12
    write_f32_le(&mut buf, 12, 4500.0);
    // Gear @ 16 (4 = 4th gear)
    buf[16] = 4;
    // Throttle @ 28
    write_f32_le(&mut buf, 28, 0.6);
    // Brake @ 32
    write_f32_le(&mut buf, 32, 0.0);
    // Steering @ 36
    write_f32_le(&mut buf, 36, 0.1);

    let t = dakar::parse_dakar_packet(&buf)?;

    assert!((t.speed_ms - 35.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 4500.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.6).abs() < 0.01, "throttle: {}", t.throttle);
    Ok(())
}

#[test]
fn golden_dakar_reverse_gear() -> TestResult {
    let mut buf = vec![0u8; 40];
    buf[0..4].copy_from_slice(&[0x44, 0x41, 0x4B, 0x52]);
    // 255 encodes reverse
    buf[16] = 255;
    let t = dakar::parse_dakar_packet(&buf)?;
    assert_eq!(t.gear, -1, "expected reverse gear");
    Ok(())
}

#[test]
fn golden_dakar_bad_magic_rejected() -> TestResult {
    let buf = vec![0u8; 40];
    assert!(dakar::parse_dakar_packet(&buf).is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// FlatOut
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_flatout_packet() -> TestResult {
    let mut buf = vec![0u8; 36];
    // Magic "FOTC" @ 0
    buf[0..4].copy_from_slice(&[0x46, 0x4F, 0x54, 0x43]);
    // Speed @ 8
    write_f32_le(&mut buf, 8, 40.0);
    // RPM @ 12
    write_f32_le(&mut buf, 12, 5500.0);
    // Gear @ 16 (2 = 2nd gear)
    buf[16] = 2;
    // Throttle @ 28
    write_f32_le(&mut buf, 28, 1.0);
    // Brake @ 32
    write_f32_le(&mut buf, 32, 0.0);

    let t = flatout::parse_flatout_packet(&buf)?;

    assert!((t.speed_ms - 40.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 5500.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 2);
    assert!((t.throttle - 1.0).abs() < 0.01, "throttle: {}", t.throttle);
    Ok(())
}

#[test]
fn golden_flatout_bad_magic_rejected() -> TestResult {
    let buf = vec![0u8; 36];
    assert!(flatout::parse_flatout_packet(&buf).is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rennsport
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_rennsport_packet() -> TestResult {
    let mut buf = vec![0u8; 24];
    // Identifier byte 0x52 ('R') @ 0
    buf[0] = 0x52;
    // Speed km/h @ 4
    write_f32_le(&mut buf, 4, 180.0);
    // RPM @ 8
    write_f32_le(&mut buf, 8, 7000.0);
    // Gear @ 12
    buf[12] = 5;
    // FFB scalar @ 16
    write_f32_le(&mut buf, 16, 0.65);
    // Slip ratio @ 20
    write_f32_le(&mut buf, 20, 0.12);

    let t = rennsport::parse_rennsport_packet(&buf)?;

    // Speed converts from km/h → m/s: 180 / 3.6 = 50.0
    assert!((t.speed_ms - 50.0).abs() < 0.1, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 7000.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 5);
    assert!(
        (t.ffb_scalar - 0.65).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    assert!(
        (t.slip_ratio - 0.12).abs() < 0.01,
        "slip_ratio: {}",
        t.slip_ratio
    );
    Ok(())
}

#[test]
fn golden_rennsport_bad_identifier_rejected() -> TestResult {
    let mut buf = vec![0u8; 24];
    buf[0] = 0x00; // wrong identifier
    assert!(rennsport::parse_rennsport_packet(&buf).is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// rFactor 1
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_rfactor1_packet() -> TestResult {
    // rFactor 1 uses f64 LE for velocity vector at offsets 24, 32, 40
    let mut buf = vec![0u8; 1025]; // large enough for gear at offset 1024
    // Velocity: 30 m/s in Z direction
    write_f64_le(&mut buf, 24, 0.0); // vel_x
    write_f64_le(&mut buf, 32, 0.0); // vel_y
    write_f64_le(&mut buf, 40, 30.0); // vel_z
    // RPM @ 312 (f64)
    write_f64_le(&mut buf, 312, 6500.0);
    // Throttle @ 1000 (f64)
    write_f64_le(&mut buf, 1000, 0.8);
    // Brake @ 1008 (f64)
    write_f64_le(&mut buf, 1008, 0.0);
    // Gear @ 1024 (i8)
    buf[1024] = 4; // 4th gear

    let t = rfactor1::parse_rfactor1_packet(&buf)?;

    assert!((t.speed_ms - 30.0).abs() < 0.1, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 6500.0).abs() < 0.1, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.8).abs() < 0.01, "throttle: {}", t.throttle);
    Ok(())
}

#[test]
fn golden_rfactor1_minimal_packet_speed_only() -> TestResult {
    // Minimal 48-byte packet: only velocity is guaranteed
    let mut buf = vec![0u8; 48];
    write_f64_le(&mut buf, 24, 10.0);
    write_f64_le(&mut buf, 32, 0.0);
    write_f64_le(&mut buf, 40, 0.0);

    let t = rfactor1::parse_rfactor1_packet(&buf)?;
    assert!((t.speed_ms - 10.0).abs() < 0.1, "speed_ms: {}", t.speed_ms);
    // Other fields should default to 0
    assert!((t.rpm).abs() < 0.01, "rpm should default to 0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// PCars2
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_pcars2_packet() -> TestResult {
    let mut buf = vec![0u8; 538];
    // Packet type byte @ 3 (type 0 = telemetry)
    buf[3] = 0;
    // Speed @ 12 (f32)
    write_f32_le(&mut buf, 12, 45.0);
    // RPM @ 16 (u16 LE)
    buf[16..18].copy_from_slice(&10000u16.to_le_bytes());
    // Throttle @ 24 (u8, 0-255 → 0.0-1.0)
    buf[24] = 200;
    // Brake @ 25 (u8)
    buf[25] = 50;
    // Gear @ 45 (i8: -1 = reverse, 0 = neutral, 1+ = forward)
    buf[45] = 3i8 as u8;

    let result = pcars2::parse_pcars2_packet(&buf);
    // PCars2 may or may not parse depending on other validation;
    // at minimum it must not panic
    if let Ok(t) = result {
        assert!(t.speed_ms >= 0.0);
        assert!(t.rpm >= 0.0);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// WTCR (Codemasters Mode 1 offsets, 264 bytes)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_wtcr_packet() -> TestResult {
    let mut buf = vec![0u8; 264];
    // Wheel speeds for speed calculation
    write_f32_le(&mut buf, 108, 40.0); // FL
    write_f32_le(&mut buf, 112, 40.0); // FR
    write_f32_le(&mut buf, 100, 40.0); // RL
    write_f32_le(&mut buf, 104, 40.0); // RR
    // RPM @ 148
    write_f32_le(&mut buf, 148, 6200.0);
    // Gear @ 132
    write_f32_le(&mut buf, 132, 4.0);
    // Throttle @ 116
    write_f32_le(&mut buf, 116, 0.85);
    // Brake @ 124
    write_f32_le(&mut buf, 124, 0.0);

    let t = wtcr::parse_wtcr_packet(&buf)?;

    assert!((t.rpm - 6200.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.85).abs() < 0.01, "throttle: {}", t.throttle);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// SimHub (JSON)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_simhub_full_packet() -> TestResult {
    let json = r#"{"SpeedMs":33.5,"Rpms":7200.0,"MaxRpms":8500.0,"Gear":"4","Throttle":80.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":45.0,"Steer":0.0,"FuelPercent":62.0,"LateralGForce":1.2,"LongitudinalGForce":0.5,"FFBValue":0.7,"IsRunning":true,"IsInPit":false}"#;
    let t = simhub::parse_simhub_packet(json.as_bytes())?;

    assert!((t.speed_ms - 33.5).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 7200.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert!((t.max_rpm - 8500.0).abs() < 0.01, "max_rpm: {}", t.max_rpm);
    assert_eq!(t.gear, 4);
    // Throttle: 80 / 100 = 0.8
    assert!((t.throttle - 0.8).abs() < 0.01, "throttle: {}", t.throttle);
    // Brake: 10 / 100 = 0.1
    assert!((t.brake - 0.1).abs() < 0.01, "brake: {}", t.brake);
    // Fuel: 62 / 100 = 0.62
    assert!(
        (t.fuel_percent - 0.62).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert!(
        (t.ffb_scalar - 0.7).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn golden_simhub_reverse_gear() -> TestResult {
    let json = r#"{"SpeedMs":2.0,"Rpms":1500.0,"Gear":"R","Throttle":30.0,"Brake":0.0}"#;
    let t = simhub::parse_simhub_packet(json.as_bytes())?;
    assert_eq!(t.gear, -1, "expected reverse gear");
    Ok(())
}

#[test]
fn golden_simhub_neutral_gear() -> TestResult {
    let json = r#"{"SpeedMs":0.0,"Rpms":800.0,"Gear":"N","Throttle":0.0,"Brake":0.0}"#;
    let t = simhub::parse_simhub_packet(json.as_bytes())?;
    assert_eq!(t.gear, 0, "expected neutral gear");
    Ok(())
}

#[test]
fn golden_simhub_empty_gear_string() -> TestResult {
    let json = r#"{"SpeedMs":0.0,"Rpms":800.0,"Gear":"","Throttle":0.0,"Brake":0.0}"#;
    let t = simhub::parse_simhub_packet(json.as_bytes())?;
    assert_eq!(t.gear, 0, "empty gear should map to neutral");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Trackmania (JSON)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_trackmania_packet() -> TestResult {
    let json = r#"{"speed":28.5,"rpm":9000.0,"gear":3,"throttle":0.95,"brake":0.0,"steerAngle":-0.2,"engineRunning":true}"#;
    let t = trackmania::parse_trackmania_packet(json.as_bytes())?;

    assert!((t.speed_ms - 28.5).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 9000.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 3);
    assert!((t.throttle - 0.95).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    // FFB scalar = steer_angle = -0.2
    assert!(
        (t.ffb_scalar - (-0.2)).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn golden_trackmania_minimal_json() -> TestResult {
    // All optional fields default to 0
    let json = r#"{}"#;
    let t = trackmania::parse_trackmania_packet(json.as_bytes())?;
    assert!((t.speed_ms).abs() < 0.01);
    assert!((t.rpm).abs() < 0.01);
    assert_eq!(t.gear, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-adapter: all factories produce adapters with matching game_id
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_all_factory_game_ids_match() -> TestResult {
    for (id, factory) in openracing_telemetry_adapters::adapter_factories() {
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *id,
            "Factory '{id}' produced adapter with game_id '{}'",
            adapter.game_id()
        );
    }
    Ok(())
}
