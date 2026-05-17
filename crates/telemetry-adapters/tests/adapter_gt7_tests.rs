//! Deep tests for the Gran Turismo 7 telemetry adapter (Salsa20 encrypted UDP).

use openracing_telemetry_adapters::gran_turismo_7::{
    Gt7PacketType, MAGIC, PACKET_SIZE, PACKET_SIZE_TYPE2, PACKET_SIZE_TYPE3, parse_decrypted,
    parse_decrypted_ext,
};
use openracing_telemetry_adapters::{GranTurismo7Adapter, TelemetryAdapter, TelemetryValue};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Field offsets (from the adapter source)
const OFF_MAGIC: usize = 0x00;
const OFF_ENGINE_RPM: usize = 0x3C;
const OFF_FUEL_LEVEL: usize = 0x44;
const OFF_FUEL_CAPACITY: usize = 0x48;
const OFF_SPEED_MS: usize = 0x4C;
const OFF_WATER_TEMP: usize = 0x58;
const OFF_TIRE_TEMP_FL: usize = 0x60;
const OFF_TIRE_TEMP_FR: usize = 0x64;
const OFF_TIRE_TEMP_RL: usize = 0x68;
const OFF_TIRE_TEMP_RR: usize = 0x6C;
const OFF_LAP_COUNT: usize = 0x74;
const OFF_BEST_LAP_MS: usize = 0x78;
const OFF_LAST_LAP_MS: usize = 0x7C;
const OFF_CURRENT_LAP_MS: usize = 0x80;
const OFF_POSITION: usize = 0x84;
#[allow(dead_code)]
const OFF_MAX_ALERT_RPM: usize = 0x8A;
const OFF_FLAGS: usize = 0x8E;
const OFF_GEAR_BYTE: usize = 0x90;
const OFF_THROTTLE: usize = 0x91;
const OFF_BRAKE: usize = 0x92;
const OFF_CAR_CODE: usize = 0x124;
const OFF_WHEEL_ROTATION: usize = 0x128;
const OFF_SWAY: usize = 0x130;
const OFF_HEAVE: usize = 0x134;
const OFF_SURGE: usize = 0x138;
const OFF_CAR_TYPE_BYTE3: usize = 0x13E;
const OFF_ENERGY_RECOVERY: usize = 0x150;

fn buf_with_magic() -> [u8; PACKET_SIZE] {
    let mut buf = [0u8; PACKET_SIZE];
    buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
    buf
}

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_i16(buf: &mut [u8], offset: usize, value: i16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

// ── Decryption handling tests ───────────────────────────────────────────

#[test]
fn gt7_adapter_rejects_short_packet() {
    let adapter = GranTurismo7Adapter::new();
    let data = vec![0u8; PACKET_SIZE - 1];
    assert!(adapter.normalize(&data).is_err());
}

#[test]
fn gt7_adapter_rejects_empty_packet() {
    let adapter = GranTurismo7Adapter::new();
    assert!(adapter.normalize(&[]).is_err());
}

#[test]
fn gt7_parse_decrypted_rejects_too_short_buffer() {
    let buf = [0u8; PACKET_SIZE - 1];
    assert!(parse_decrypted_ext(&buf).is_err());
}

// ── Basic field parsing tests ───────────────────────────────────────────

#[test]
fn gt7_parse_rpm_and_speed() -> TestResult {
    let mut buf = buf_with_magic();
    write_f32(&mut buf, OFF_ENGINE_RPM, 7500.0);
    write_f32(&mut buf, OFF_SPEED_MS, 55.0);
    let result = parse_decrypted(&buf)?;
    assert!((result.rpm - 7500.0).abs() < 0.01);
    assert!((result.speed_ms - 55.0).abs() < 0.01);
    Ok(())
}

#[test]
fn gt7_parse_throttle_and_brake() -> TestResult {
    let mut buf = buf_with_magic();
    buf[OFF_THROTTLE] = 200; // ~78.4%
    buf[OFF_BRAKE] = 128; // ~50.2%
    let result = parse_decrypted(&buf)?;
    assert!((result.throttle - 200.0 / 255.0).abs() < 0.01);
    assert!((result.brake - 128.0 / 255.0).abs() < 0.01);
    Ok(())
}

#[test]
fn gt7_parse_gear_encoding() -> TestResult {
    let mut buf = buf_with_magic();
    // Low nibble = current gear, high nibble = suggested gear
    buf[OFF_GEAR_BYTE] = 0x34; // suggested=3, current=4
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.gear, 4);
    Ok(())
}

#[test]
fn gt7_parse_neutral_gear() -> TestResult {
    let mut buf = buf_with_magic();
    buf[OFF_GEAR_BYTE] = 0x00;
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.gear, 0);
    Ok(())
}

#[test]
fn gt7_parse_tire_temperatures() -> TestResult {
    let mut buf = buf_with_magic();
    write_f32(&mut buf, OFF_TIRE_TEMP_FL, 85.5);
    write_f32(&mut buf, OFF_TIRE_TEMP_FR, 90.0);
    write_f32(&mut buf, OFF_TIRE_TEMP_RL, 78.3);
    write_f32(&mut buf, OFF_TIRE_TEMP_RR, 88.9);
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.tire_temps_c[0], 85); // truncated to u8
    assert_eq!(result.tire_temps_c[1], 90);
    assert_eq!(result.tire_temps_c[2], 78);
    assert_eq!(result.tire_temps_c[3], 88);
    Ok(())
}

#[test]
fn gt7_parse_fuel_percent() -> TestResult {
    let mut buf = buf_with_magic();
    write_f32(&mut buf, OFF_FUEL_LEVEL, 25.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 100.0);
    let result = parse_decrypted(&buf)?;
    assert!((result.fuel_percent - 0.25).abs() < 0.01);
    Ok(())
}

#[test]
fn gt7_parse_lap_timing() -> TestResult {
    let mut buf = buf_with_magic();
    write_i32(&mut buf, OFF_BEST_LAP_MS, 62_500); // 62.5s
    write_i32(&mut buf, OFF_LAST_LAP_MS, 63_100); // 63.1s
    write_i32(&mut buf, OFF_CURRENT_LAP_MS, 30_000); // 30.0s
    let result = parse_decrypted(&buf)?;
    assert!((result.best_lap_time_s - 62.5).abs() < 0.01);
    assert!((result.last_lap_time_s - 63.1).abs() < 0.01);
    assert!((result.current_lap_time_s - 30.0).abs() < 0.01);
    Ok(())
}

#[test]
fn gt7_parse_position_and_laps() -> TestResult {
    let mut buf = buf_with_magic();
    write_i16(&mut buf, OFF_POSITION, 5);
    write_u16(&mut buf, OFF_LAP_COUNT, 12);
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.position, 5);
    assert_eq!(result.lap, 12);
    Ok(())
}

#[test]
fn gt7_parse_flags() -> TestResult {
    let mut buf = buf_with_magic();
    // TCS (bit 11) | ASM (bit 10) | REV_LIMIT (bit 5)
    let flags: u16 = (1 << 11) | (1 << 10) | (1 << 5);
    write_u16(&mut buf, OFF_FLAGS, flags);
    let result = parse_decrypted(&buf)?;
    assert!(result.flags.traction_control);
    assert!(result.flags.abs_active);
    assert!(result.flags.engine_limiter);
    Ok(())
}

#[test]
fn gt7_parse_car_code_extended() -> TestResult {
    let mut buf = buf_with_magic();
    write_i32(&mut buf, OFF_CAR_CODE, 42);
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.car_id, Some("gt7_42".to_string()));
    Ok(())
}

// ── Extended packet tests (Type2/Type3) ─────────────────────────────────

#[test]
fn gt7_type2_extended_motion_fields() -> TestResult {
    let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
    buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
    write_f32(&mut buf, OFF_WHEEL_ROTATION, 0.35);
    write_f32(&mut buf, OFF_SWAY, 1.2);
    write_f32(&mut buf, OFF_HEAVE, 0.5);
    write_f32(&mut buf, OFF_SURGE, -0.8);

    let result = parse_decrypted_ext(&buf)?;
    assert!((result.steering_angle - 0.35).abs() < 0.01);
    assert!((result.lateral_g - 1.2).abs() < 0.01);
    assert!((result.vertical_g - 0.5).abs() < 0.01);
    assert!((result.longitudinal_g - (-0.8)).abs() < 0.01);
    assert_eq!(
        result.get_extended("gt7_sway"),
        Some(&TelemetryValue::Float(1.2))
    );
    Ok(())
}

#[test]
fn gt7_type3_energy_recovery() -> TestResult {
    let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
    buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[OFF_CAR_TYPE_BYTE3] = 4; // electric
    write_f32(&mut buf, OFF_ENERGY_RECOVERY, 0.85);

    let result = parse_decrypted_ext(&buf)?;
    assert_eq!(
        result.get_extended("gt7_car_type"),
        Some(&TelemetryValue::Integer(4))
    );
    assert_eq!(
        result.get_extended("gt7_energy_recovery"),
        Some(&TelemetryValue::Float(0.85))
    );
    Ok(())
}

#[test]
fn gt7_packet_type_detection() {
    assert_eq!(Gt7PacketType::Type1.expected_size(), PACKET_SIZE);
    assert_eq!(Gt7PacketType::Type2.expected_size(), PACKET_SIZE_TYPE2);
    assert_eq!(Gt7PacketType::Type3.expected_size(), PACKET_SIZE_TYPE3);
}

#[test]
fn gt7_packet_type_heartbeats() {
    assert_eq!(Gt7PacketType::Type1.heartbeat(), b"A");
    assert_eq!(Gt7PacketType::Type2.heartbeat(), b"B");
    assert_eq!(Gt7PacketType::Type3.heartbeat(), b"~");
}

#[test]
fn gt7_packet_type_xor_keys() {
    assert_eq!(Gt7PacketType::Type1.xor_key(), 0xDEAD_BEAF);
    assert_eq!(Gt7PacketType::Type2.xor_key(), 0xDEAD_BEEF);
    assert_eq!(Gt7PacketType::Type3.xor_key(), 0x55FA_BB4F);
}

#[test]
fn gt7_adapter_game_id_and_rate() {
    let adapter = GranTurismo7Adapter::new();
    assert_eq!(adapter.game_id(), "gran_turismo_7");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
}

#[test]
fn gt7_zero_fuel_capacity_gives_zero_percent() -> TestResult {
    let mut buf = buf_with_magic();
    write_f32(&mut buf, OFF_FUEL_LEVEL, 10.0);
    write_f32(&mut buf, OFF_FUEL_CAPACITY, 0.0);
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn gt7_negative_lap_times_default_to_zero() -> TestResult {
    let mut buf = buf_with_magic();
    write_i32(&mut buf, OFF_BEST_LAP_MS, -1);
    write_i32(&mut buf, OFF_LAST_LAP_MS, -1);
    write_i32(&mut buf, OFF_CURRENT_LAP_MS, -1);
    let result = parse_decrypted(&buf)?;
    assert_eq!(result.best_lap_time_s, 0.0);
    assert_eq!(result.last_lap_time_s, 0.0);
    assert_eq!(result.current_lap_time_s, 0.0);
    Ok(())
}

#[test]
fn gt7_water_temp_parsing() -> TestResult {
    let mut buf = buf_with_magic();
    write_f32(&mut buf, OFF_WATER_TEMP, 92.5);
    let result = parse_decrypted(&buf)?;
    assert!((result.engine_temp_c - 92.5).abs() < 0.01);
    Ok(())
}

#[test]
fn gt7_type3_includes_type2_fields() -> TestResult {
    let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
    buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
    write_f32(&mut buf, OFF_WHEEL_ROTATION, 0.5);
    write_f32(&mut buf, OFF_SWAY, 0.3);
    buf[OFF_CAR_TYPE_BYTE3] = 2;

    let result = parse_decrypted_ext(&buf)?;
    // Type2 fields are parsed
    assert!((result.steering_angle - 0.5).abs() < 0.01);
    assert!(result.get_extended("gt7_sway").is_some());
    // Type3 fields are also present
    assert!(result.get_extended("gt7_car_type").is_some());
    assert!(result.get_extended("gt7_energy_recovery").is_some());
    Ok(())
}
