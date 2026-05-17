//! Deep protocol-level tests for the Live For Speed (LFS) OutGauge UDP adapter.
//!
//! Exercises packet parsing, endianness, gear encoding, dashboard light flags,
//! boundary values, and corrupted-packet handling against the 92-byte OutGauge
//! protocol (port 30000).  With optional OutGauge ID the packet is 96 bytes.

use openracing_telemetry_adapters::{LFSAdapter, TelemetryAdapter, TelemetryValue};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── LFS OutGauge byte offsets ────────────────────────────────────────────────

const OFF_GEAR: usize = 10; // u8
const OFF_SPEED: usize = 12; // f32, m/s
const OFF_RPM: usize = 16; // f32
const OFF_TURBO: usize = 20; // f32, BAR
const OFF_ENG_TEMP: usize = 24; // f32, °C
const OFF_FUEL: usize = 28; // f32, 0..1
const OFF_OIL_PRESSURE: usize = 32; // f32, BAR
const OFF_OIL_TEMP: usize = 36; // f32, °C
const OFF_SHOW_LIGHTS: usize = 44; // u32
const OFF_THROTTLE: usize = 48; // f32, 0..1
const OFF_BRAKE: usize = 52; // f32, 0..1
const OFF_CLUTCH: usize = 56; // f32, 0..1

const OUTGAUGE_PACKET_SIZE: usize = 92;

// Dashboard light bitmasks
const DL_SHIFT: u32 = 0x0001;
const DL_PITSPEED: u32 = 0x0008;
const DL_TC: u32 = 0x0010;
const DL_ABS: u32 = 0x0400;

fn make_packet() -> Vec<u8> {
    vec![0u8; OUTGAUGE_PACKET_SIZE]
}

fn set_f32(buf: &mut [u8], offset: usize, val: f32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn set_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn adapter() -> LFSAdapter {
    LFSAdapter::new()
}

// ── 1. Adapter metadata ─────────────────────────────────────────────────────

#[test]
fn lfs_game_id() -> TestResult {
    assert_eq!(adapter().game_id(), "live_for_speed");
    Ok(())
}

#[test]
fn lfs_update_rate_60hz() -> TestResult {
    assert_eq!(adapter().expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

#[test]
fn lfs_custom_port() -> TestResult {
    let a = LFSAdapter::new().with_port(31000);
    // Verify the adapter was created without error (port is private but
    // construction should succeed).
    assert_eq!(a.game_id(), "live_for_speed");
    Ok(())
}

// ── 2. Packet size validation ───────────────────────────────────────────────

#[test]
fn lfs_empty_packet_rejected() -> TestResult {
    assert!(adapter().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn lfs_one_byte_short_rejected() -> TestResult {
    let buf = vec![0u8; OUTGAUGE_PACKET_SIZE - 1];
    assert!(adapter().normalize(&buf).is_err());
    Ok(())
}

#[test]
fn lfs_exact_92_bytes_accepted() -> TestResult {
    let buf = make_packet();
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn lfs_oversized_packet_accepted() -> TestResult {
    let buf = vec![0u8; 200];
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

// ── 3. Endianness correctness ───────────────────────────────────────────────

#[test]
fn lfs_speed_little_endian() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_SPEED, 42.5);
    let t = adapter().normalize(&buf)?;
    assert!((t.speed_ms - 42.5).abs() < 0.001);
    Ok(())
}

#[test]
fn lfs_rpm_little_endian() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_RPM, 6800.0);
    let t = adapter().normalize(&buf)?;
    assert!((t.rpm - 6800.0).abs() < 0.1);
    Ok(())
}

// ── 4. Gear encoding ────────────────────────────────────────────────────────

#[test]
fn lfs_gear_0_is_reverse() -> TestResult {
    let mut buf = make_packet();
    buf[OFF_GEAR] = 0;
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn lfs_gear_1_is_neutral() -> TestResult {
    let mut buf = make_packet();
    buf[OFF_GEAR] = 1;
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn lfs_gear_2_through_8_are_forward() -> TestResult {
    for raw in 2u8..=8 {
        let mut buf = make_packet();
        buf[OFF_GEAR] = raw;
        let t = adapter().normalize(&buf)?;
        let expected = (raw - 1) as i8;
        assert_eq!(t.gear, expected, "raw gear {raw} → expected {expected}");
    }
    Ok(())
}

// ── 5. Field extraction accuracy ────────────────────────────────────────────

#[test]
fn lfs_full_driving_scenario() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_SPEED, 55.0);
    set_f32(&mut buf, OFF_RPM, 7200.0);
    buf[OFF_GEAR] = 4; // 3rd gear
    set_f32(&mut buf, OFF_THROTTLE, 0.9);
    set_f32(&mut buf, OFF_BRAKE, 0.0);
    set_f32(&mut buf, OFF_CLUTCH, 0.05);
    set_f32(&mut buf, OFF_FUEL, 0.62);
    set_f32(&mut buf, OFF_ENG_TEMP, 92.0);

    let t = adapter().normalize(&buf)?;
    assert!((t.speed_ms - 55.0).abs() < 0.01);
    assert!((t.rpm - 7200.0).abs() < 0.1);
    assert_eq!(t.gear, 3);
    assert!((t.throttle - 0.9).abs() < 0.01);
    assert!(t.brake.abs() < 0.001);
    assert!((t.clutch - 0.05).abs() < 0.01);
    assert!((t.fuel_percent - 0.62).abs() < 0.01);
    assert!((t.engine_temp_c - 92.0).abs() < 0.1);
    Ok(())
}

#[test]
fn lfs_heavy_braking_high_speed() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_SPEED, 80.0);
    set_f32(&mut buf, OFF_RPM, 4500.0);
    buf[OFF_GEAR] = 5; // 4th gear
    set_f32(&mut buf, OFF_THROTTLE, 0.0);
    set_f32(&mut buf, OFF_BRAKE, 0.98);

    let t = adapter().normalize(&buf)?;
    assert!((t.brake - 0.98).abs() < 0.01);
    assert!(t.throttle.abs() < 0.001);
    Ok(())
}

// ── 6. Dashboard light flags ────────────────────────────────────────────────

#[test]
fn lfs_pit_limiter_flag() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_PITSPEED);
    let t = adapter().normalize(&buf)?;
    assert!(t.flags.pit_limiter);
    assert!(!t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

#[test]
fn lfs_tc_flag() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC);
    let t = adapter().normalize(&buf)?;
    assert!(t.flags.traction_control);
    assert!(!t.flags.pit_limiter);
    Ok(())
}

#[test]
fn lfs_abs_flag() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_ABS);
    let t = adapter().normalize(&buf)?;
    assert!(t.flags.abs_active);
    Ok(())
}

#[test]
fn lfs_combined_flags() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC | DL_ABS | DL_PITSPEED);
    let t = adapter().normalize(&buf)?;
    assert!(t.flags.traction_control);
    assert!(t.flags.abs_active);
    assert!(t.flags.pit_limiter);
    Ok(())
}

#[test]
fn lfs_no_flags_when_lights_zero() -> TestResult {
    let buf = make_packet();
    let t = adapter().normalize(&buf)?;
    assert!(!t.flags.pit_limiter);
    assert!(!t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

// ── 7. Extended fields ──────────────────────────────────────────────────────

#[test]
fn lfs_turbo_in_extended() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_TURBO, 1.5);
    let t = adapter().normalize(&buf)?;
    assert_eq!(
        t.extended.get("turbo_bar"),
        Some(&TelemetryValue::Float(1.5))
    );
    Ok(())
}

#[test]
fn lfs_oil_pressure_in_extended() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_OIL_PRESSURE, 3.8);
    let t = adapter().normalize(&buf)?;
    assert_eq!(
        t.extended.get("oil_pressure_bar"),
        Some(&TelemetryValue::Float(3.8))
    );
    Ok(())
}

#[test]
fn lfs_oil_temp_in_extended() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_OIL_TEMP, 110.0);
    let t = adapter().normalize(&buf)?;
    assert_eq!(
        t.extended.get("oil_temp_c"),
        Some(&TelemetryValue::Float(110.0))
    );
    Ok(())
}

#[test]
fn lfs_shift_light_in_extended() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_SHIFT);
    let t = adapter().normalize(&buf)?;
    assert_eq!(
        t.extended.get("shift_light"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

#[test]
fn lfs_dash_lights_raw_present_when_nonzero() -> TestResult {
    let mut buf = make_packet();
    set_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC | DL_ABS);
    let t = adapter().normalize(&buf)?;
    assert!(
        t.extended.contains_key("dash_lights_raw"),
        "dash_lights_raw should be present when lights are on"
    );
    Ok(())
}

// ── 8. Non-finite values ────────────────────────────────────────────────────

#[test]
fn lfs_nan_speed_becomes_zero() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_SPEED, f32::NAN);
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn lfs_infinity_rpm_becomes_zero() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_RPM, f32::INFINITY);
    let t = adapter().normalize(&buf)?;
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

// ── 9. Value range validation ───────────────────────────────────────────────

#[test]
fn lfs_throttle_clamped_above_one() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_THROTTLE, 1.5);
    let t = adapter().normalize(&buf)?;
    assert!(t.throttle <= 1.0, "throttle should be clamped to 1.0");
    Ok(())
}

#[test]
fn lfs_brake_clamped_above_one() -> TestResult {
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_BRAKE, 2.0);
    let t = adapter().normalize(&buf)?;
    assert!(t.brake <= 1.0, "brake should be clamped to 1.0");
    Ok(())
}

// ── 10. Deterministic output ────────────────────────────────────────────────

#[test]
fn lfs_same_input_same_output() -> TestResult {
    let a = adapter();
    let mut buf = make_packet();
    set_f32(&mut buf, OFF_SPEED, 35.0);
    set_f32(&mut buf, OFF_RPM, 5200.0);
    buf[OFF_GEAR] = 3;

    let t1 = a.normalize(&buf)?;
    let t2 = a.normalize(&buf)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    Ok(())
}
