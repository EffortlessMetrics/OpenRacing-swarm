//! Integration tests for game telemetry adapters.
//!
//! These tests verify the `normalize()` method of each adapter using
//! hand-crafted packets constructed from the documented byte offsets.
//! Tests cover:
//!   - Short/empty packets → Err
//!   - Valid packets → expected field values
//!   - Game-paused / not-racing states → empty telemetry

use openracing_telemetry_adapters::{
    AssettoCorsaAdapter, BeamNGAdapter, ForzaAdapter, IRacingAdapter, PCars2Adapter,
    RFactor2Adapter, RaceRoomAdapter, TelemetryAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helper ──────────────────────────────────────────────────────────────────

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    let bytes = value.to_le_bytes();
    buf[offset..offset + 4].copy_from_slice(&bytes);
}

#[allow(dead_code)]
fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    let bytes = value.to_le_bytes();
    buf[offset..offset + 4].copy_from_slice(&bytes);
}

// ─── Forza ───────────────────────────────────────────────────────────────────

#[test]
fn forza_empty_packet_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn forza_sled_not_racing_returns_empty_telemetry() -> TestResult {
    // is_race_on = 0 → empty NormalizedTelemetry
    let mut pkt = vec![0u8; 232];
    write_i32_le(&mut pkt, 0, 0); // is_race_on = 0

    let adapter = ForzaAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.rpm, 0.0, "not-racing packet must produce zero rpm");
    Ok(())
}

#[test]
fn forza_sled_valid_packet_parses_rpm_and_speed() -> TestResult {
    let mut pkt = vec![0u8; 232];
    write_i32_le(&mut pkt, 0, 1); // is_race_on = 1
    write_f32_le(&mut pkt, 8, 9000.0); // engine_max_rpm
    write_f32_le(&mut pkt, 16, 6000.0); // current_rpm
    write_f32_le(&mut pkt, 32, 30.0); // vel_x (OFF_VEL_X=32) → speed ~30 m/s

    let adapter = ForzaAdapter::new();
    let t = adapter.normalize(&pkt)?;

    assert!(
        (t.rpm - 6000.0).abs() < 1.0,
        "rpm must be ~6000, got {}",
        t.rpm
    );
    assert!(
        (t.speed_ms - 30.0).abs() < 0.1,
        "speed_ms must be ~30, got {}",
        t.speed_ms
    );
    Ok(())
}

#[test]
fn forza_sled_speed_from_negative_velocity() -> TestResult {
    // Sled format has no gear field; verify speed_ms is non-negative for
    // negative velocity (moving backwards).
    let mut pkt = vec![0u8; 232];
    write_i32_le(&mut pkt, 0, 1); // is_race_on
    write_f32_le(&mut pkt, 32, -20.0); // vel_x = -20 m/s (OFF_VEL_X=32)

    let adapter = ForzaAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert!(t.speed_ms >= 0.0, "speed_ms must be non-negative");
    assert!(
        (t.speed_ms - 20.0).abs() < 0.1,
        "speed_ms must be ~20 for vel_x=-20, got {}",
        t.speed_ms
    );
    Ok(())
}

#[test]
fn forza_cardash_packet_parses() -> TestResult {
    // CarDash is 311 bytes; same layout as Sled for first 232 bytes
    let mut pkt = vec![0u8; 311];
    write_i32_le(&mut pkt, 0, 1);
    write_f32_le(&mut pkt, 16, 5500.0); // rpm

    let adapter = ForzaAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert!(
        (t.rpm - 5500.0).abs() < 1.0,
        "CardDash rpm must be ~5500, got {}",
        t.rpm
    );
    Ok(())
}

#[test]
fn forza_unknown_packet_size_returns_error() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 100]).is_err(),
        "unknown size must return Err"
    );
    Ok(())
}

// ─── BeamNG ──────────────────────────────────────────────────────────────────

#[test]
fn beamng_empty_packet_returns_error() -> TestResult {
    let adapter = BeamNGAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn beamng_short_packet_returns_error() -> TestResult {
    let adapter = BeamNGAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 50]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn beamng_valid_outgauge_packet_parses_fields() -> TestResult {
    let mut pkt = vec![0u8; 96];
    write_f32_le(&mut pkt, 12, 25.0); // speed m/s
    write_f32_le(&mut pkt, 16, 5000.0); // rpm
    pkt[10] = 3; // gear=3 → 3-1 = 2nd gear normalized
    write_f32_le(&mut pkt, 48, 0.7); // throttle

    let adapter = BeamNGAdapter::new();
    let t = adapter.normalize(&pkt)?;

    assert!(
        (t.rpm - 5000.0).abs() < 1.0,
        "rpm must be ~5000, got {}",
        t.rpm
    );
    assert!(
        (t.speed_ms - 25.0).abs() < 0.01,
        "speed must be ~25 m/s, got {}",
        t.speed_ms
    );
    assert_eq!(t.gear, 2, "OutGauge gear 3 must map to normalized gear 2");
    assert!((t.throttle - 0.7).abs() < 0.01, "throttle must be ~0.7");
    Ok(())
}

#[test]
fn beamng_reverse_gear_maps_to_minus_one() -> TestResult {
    let mut pkt = vec![0u8; 96];
    pkt[10] = 0; // gear=0 → Reverse

    let adapter = BeamNGAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, -1, "OutGauge gear 0 must map to Reverse (-1)");
    Ok(())
}

#[test]
fn beamng_neutral_gear_maps_to_zero() -> TestResult {
    let mut pkt = vec![0u8; 96];
    pkt[10] = 1; // gear=1 → Neutral

    let adapter = BeamNGAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, 0, "OutGauge gear 1 must map to Neutral (0)");
    Ok(())
}

// ─── Assetto Corsa ───────────────────────────────────────────────────────────

#[test]
fn assetto_corsa_empty_packet_returns_error() -> TestResult {
    let adapter = AssettoCorsaAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn assetto_corsa_short_packet_returns_error() -> TestResult {
    let adapter = AssettoCorsaAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 50]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn assetto_corsa_valid_packet_parses_fields() -> TestResult {
    let mut pkt = vec![0u8; 328]; // RTCarInfo struct size
    write_f32_le(&mut pkt, 16, 30.0); // speed_Ms at offset 16
    write_f32_le(&mut pkt, 56, 0.8); // gas at offset 56
    write_f32_le(&mut pkt, 60, 0.0); // brake at offset 60
    write_f32_le(&mut pkt, 68, 5500.0); // engineRPM at offset 68
    write_f32_le(&mut pkt, 72, 0.3); // steer at offset 72
    write_i32_le(&mut pkt, 76, 3); // gear at offset 76 (AC: 3 = 2nd)

    let adapter = AssettoCorsaAdapter::new();
    let t = adapter.normalize(&pkt)?;

    assert!(
        (t.rpm - 5500.0).abs() < 1.0,
        "rpm must be ~5500, got {}",
        t.rpm
    );
    assert!(
        (t.speed_ms - 30.0).abs() < 0.2,
        "speed must be ~30 m/s, got {}",
        t.speed_ms
    );
    assert_eq!(t.gear, 2, "gear must be 2 (AC raw 3 → normalized 2)");
    assert!((t.throttle - 0.8).abs() < 0.01, "throttle must be ~0.8");
    Ok(())
}

// ─── Project CARS 2 ──────────────────────────────────────────────────────────

#[test]
fn pcars2_empty_packet_returns_error() -> TestResult {
    let adapter = PCars2Adapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn pcars2_short_packet_returns_error() -> TestResult {
    let adapter = PCars2Adapter::new();
    assert!(
        adapter.normalize(&[0u8; 30]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn pcars2_valid_packet_parses_fields() -> TestResult {
    let mut pkt = vec![0u8; 46];
    pkt[44] = (-0.15f32 * 127.0) as i8 as u8; // steering i8
    pkt[30] = (0.9f32 * 255.0) as u8; // throttle u8
    pkt[29] = 0; // brake u8
    write_f32_le(&mut pkt, 36, 45.0); // speed f32 m/s
    pkt[40..42].copy_from_slice(&7000u16.to_le_bytes()); // rpm u16
    pkt[42..44].copy_from_slice(&9000u16.to_le_bytes()); // max_rpm u16
    pkt[45] = 4; // gear=4

    let adapter = PCars2Adapter::new();
    let t = adapter.normalize(&pkt)?;

    assert!(
        (t.rpm - 7000.0).abs() < 1.0,
        "rpm must be ~7000, got {}",
        t.rpm
    );
    assert!((t.speed_ms - 45.0).abs() < 0.01, "speed must be ~45 m/s");
    assert_eq!(t.gear, 4, "gear must be 4");
    // u8 round-trip: (0.9 * 255) as u8 = 229, 229/255 ≈ 0.898
    assert!(
        (t.throttle - 229.0 / 255.0).abs() < 0.01,
        "throttle must be ~0.9"
    );
    Ok(())
}

// ─── RaceRoom ────────────────────────────────────────────────────────────────

#[test]
fn raceroom_empty_packet_returns_error() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn raceroom_short_packet_returns_error() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 500]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn raceroom_wrong_version_returns_error() -> TestResult {
    let mut pkt = vec![0u8; 4096];
    write_i32_le(&mut pkt, 0, 99); // version_major = 99 (wrong)

    let adapter = RaceRoomAdapter::new();
    assert!(
        adapter.normalize(&pkt).is_err(),
        "wrong version_major must return Err"
    );
    Ok(())
}

#[test]
fn raceroom_game_paused_returns_empty_telemetry() -> TestResult {
    let mut pkt = vec![0u8; 4096];
    write_i32_le(&mut pkt, 0, 3); // version_major = 3 (R3E SDK v3)
    write_i32_le(&mut pkt, 20, 1); // game_paused = 1

    let adapter = RaceRoomAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.rpm, 0.0, "paused game must produce zero rpm");
    Ok(())
}

#[test]
fn raceroom_game_in_menus_returns_empty_telemetry() -> TestResult {
    let mut pkt = vec![0u8; 4096];
    write_i32_le(&mut pkt, 0, 3); // version_major = 3
    write_i32_le(&mut pkt, 20, 0); // game_paused = 0
    write_i32_le(&mut pkt, 24, 1); // game_in_menus = 1

    let adapter = RaceRoomAdapter::new();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.rpm, 0.0, "in-menus state must produce zero rpm");
    Ok(())
}

#[test]
fn raceroom_valid_packet_parses_fields() -> TestResult {
    let mut pkt = vec![0u8; 4096];
    write_i32_le(&mut pkt, 0, 3); // version_major = 3
    write_i32_le(&mut pkt, 20, 0); // game_paused = 0
    write_i32_le(&mut pkt, 24, 0); // game_in_menus = 0
    // engine_rps in rad/s: 5000 RPM = 5000 * π / 30 ≈ 523.6 rad/s
    let rps_5000 = 5000.0f32 * std::f32::consts::PI / 30.0;
    let rps_8000 = 8000.0f32 * std::f32::consts::PI / 30.0;
    write_f32_le(&mut pkt, 1396, rps_5000); // engine_rps
    write_f32_le(&mut pkt, 1400, rps_8000); // max_engine_rps
    write_f32_le(&mut pkt, 1392, 50.0); // car_speed m/s
    write_f32_le(&mut pkt, 1524, 0.1); // steer_input_raw
    write_f32_le(&mut pkt, 1500, 0.6); // throttle
    write_i32_le(&mut pkt, 1408, 3); // gear

    let adapter = RaceRoomAdapter::new();
    let t = adapter.normalize(&pkt)?;

    assert!(
        (t.rpm - 5000.0).abs() < 1.0,
        "rpm must be ~5000, got {}",
        t.rpm
    );
    assert!((t.speed_ms - 50.0).abs() < 0.01, "speed must be ~50 m/s");
    assert_eq!(t.gear, 3, "gear must be 3");
    assert!((t.throttle - 0.6).abs() < 0.01, "throttle must be ~0.6");
    Ok(())
}

// ─── rFactor 2 ───────────────────────────────────────────────────────────────

#[test]
fn rfactor2_empty_packet_returns_error() -> TestResult {
    let adapter = RFactor2Adapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must return Err"
    );
    Ok(())
}

#[test]
fn rfactor2_short_packet_returns_error() -> TestResult {
    let adapter = RFactor2Adapter::new();
    assert!(
        adapter.normalize(&[0u8; 16]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn rfactor2_large_zeroed_buffer_returns_ok() -> TestResult {
    // A zeroed 8 KiB buffer is large enough for any RF2VehicleTelemetry struct.
    let adapter = RFactor2Adapter::new();
    let result = adapter.normalize(&[0u8; 8192]);
    assert!(
        result.is_ok(),
        "zero-filled large buffer must parse without error"
    );
    Ok(())
}

// ─── iRacing ─────────────────────────────────────────────────────────────────

#[test]
fn iracing_empty_packet_returns_error() -> TestResult {
    let adapter = IRacingAdapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must return Err"
    );
    Ok(())
}

#[test]
fn iracing_short_packet_returns_error() -> TestResult {
    let adapter = IRacingAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 16]).is_err(),
        "short packet must return Err"
    );
    Ok(())
}

#[test]
fn iracing_large_zeroed_buffer_returns_ok() -> TestResult {
    // A zeroed 8 KiB buffer covers both IRacingLegacyData and IRacingData.
    let adapter = IRacingAdapter::new();
    let result = adapter.normalize(&[0u8; 8192]);
    assert!(
        result.is_ok(),
        "zero-filled large buffer must parse without error"
    );
    Ok(())
}

// ─── Adapter registry ────────────────────────────────────────────────────────

#[test]
fn adapter_factories_registry_contains_all_expected_games() -> TestResult {
    use openracing_telemetry_adapters::adapter_factories;

    let factories = adapter_factories();
    let game_ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    for expected in &[
        "forza_motorsport",
        "beamng_drive",
        "assetto_corsa",
        "project_cars_2",
        "raceroom",
        "rfactor2",
        "iracing",
        "acc",
    ] {
        assert!(
            game_ids.contains(expected),
            "adapter registry must contain '{}'; found: {:?}",
            expected,
            game_ids
        );
    }

    Ok(())
}

#[test]
fn adapter_game_ids_are_unique() -> TestResult {
    use openracing_telemetry_adapters::adapter_factories;
    use std::collections::HashSet;

    let factories = adapter_factories();
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();
    let unique: HashSet<&&str> = ids.iter().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "all adapter game IDs must be unique"
    );
    Ok(())
}
