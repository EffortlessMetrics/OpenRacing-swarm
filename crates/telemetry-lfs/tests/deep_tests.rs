//! Deep tests for the Live for Speed (LFS) OutGauge telemetry adapter.
//!
//! Covers packet construction, field parsing, gear encoding,
//! dashboard light flags, extended fields, and edge cases.

use openracing_telemetry::TelemetryValue;
use racing_wheel_telemetry_lfs::{LFSAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const OUTGAUGE_SIZE: usize = 92;

// OutGauge offsets
const OFF_GEAR: usize = 10;
const OFF_SPEED: usize = 12;
const OFF_RPM: usize = 16;
const OFF_TURBO: usize = 20;
const OFF_ENG_TEMP: usize = 24;
const OFF_FUEL: usize = 28;
const OFF_OIL_PRESSURE: usize = 32;
const OFF_OIL_TEMP: usize = 36;
const OFF_SHOW_LIGHTS: usize = 44;
const OFF_THROTTLE: usize = 48;
const OFF_BRAKE: usize = 52;
const OFF_CLUTCH: usize = 56;

// Dashboard light flags
const DL_SHIFT: u32 = 0x0001;
const DL_PITSPEED: u32 = 0x0008;
const DL_TC: u32 = 0x0010;
const DL_ABS: u32 = 0x0400;

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn make_packet() -> Vec<u8> {
    vec![0u8; OUTGAUGE_SIZE]
}

// ── Adapter identity ─────────────────────────────────────────────────────────

#[test]
fn deep_game_id() -> TestResult {
    let adapter = LFSAdapter::new();
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

#[test]
fn deep_update_rate() -> TestResult {
    let adapter = LFSAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

#[test]
fn deep_with_port() -> TestResult {
    let adapter = LFSAdapter::new().with_port(31000);
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

// ── Packet rejection ─────────────────────────────────────────────────────────

#[test]
fn deep_rejects_empty() -> TestResult {
    let adapter = LFSAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_rejects_short_packet() -> TestResult {
    let adapter = LFSAdapter::new();
    assert!(adapter.normalize(&[0u8; OUTGAUGE_SIZE - 1]).is_err());
    Ok(())
}

#[test]
fn deep_accepts_oversized_packet() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = vec![0u8; OUTGAUGE_SIZE + 64];
    write_f32(&mut buf, OFF_SPEED, 25.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 25.0).abs() < 0.01);
    Ok(())
}

// ── Speed and RPM ────────────────────────────────────────────────────────────

#[test]
fn deep_speed_passthrough() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_SPEED, 42.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 42.5).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_rpm_passthrough() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 7200.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 7200.0).abs() < 0.01, "rpm={}", t.rpm);
    Ok(())
}

// ── Gear encoding ────────────────────────────────────────────────────────────

#[test]
fn deep_gear_reverse() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 0; // OutGauge 0 = Reverse → -1
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, -1, "gear 0 → -1 (reverse)");
    Ok(())
}

#[test]
fn deep_gear_neutral() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 1; // OutGauge 1 = Neutral → 0
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.gear, 0, "gear 1 → 0 (neutral)");
    Ok(())
}

#[test]
fn deep_gear_forward_range() -> TestResult {
    let adapter = LFSAdapter::new();
    for raw in 2u8..=8 {
        let mut buf = make_packet();
        buf[OFF_GEAR] = raw;
        let t = adapter.normalize(&buf)?;
        let expected = (raw - 1) as i8;
        assert_eq!(t.gear, expected, "raw={raw} expected={expected}");
    }
    Ok(())
}

// ── Throttle, brake, clutch ──────────────────────────────────────────────────

#[test]
fn deep_throttle_brake_clutch() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 0.85);
    write_f32(&mut buf, OFF_BRAKE, 0.42);
    write_f32(&mut buf, OFF_CLUTCH, 0.15);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 0.85).abs() < 0.001);
    assert!((t.brake - 0.42).abs() < 0.001);
    assert!((t.clutch - 0.15).abs() < 0.001);
    Ok(())
}

// ── Engine temp and fuel ─────────────────────────────────────────────────────

#[test]
fn deep_engine_temp() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_ENG_TEMP, 92.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.engine_temp_c - 92.5).abs() < 0.1);
    Ok(())
}

#[test]
fn deep_fuel_percent() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL, 0.65);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 0.65).abs() < 0.001);
    Ok(())
}

// ── Dashboard light flags ────────────────────────────────────────────────────

#[test]
fn deep_pit_limiter_flag() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_PITSPEED);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.pit_limiter, "pit limiter should be set");
    assert!(!t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

#[test]
fn deep_tc_abs_flags() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC | DL_ABS);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.traction_control, "TC should be set");
    assert!(t.flags.abs_active, "ABS should be set");
    assert!(!t.flags.pit_limiter);
    Ok(())
}

#[test]
fn deep_shift_light_in_extended() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_SHIFT);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("shift_light"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

#[test]
fn deep_all_flags_combined() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(
        &mut buf,
        OFF_SHOW_LIGHTS,
        DL_SHIFT | DL_PITSPEED | DL_TC | DL_ABS,
    );
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.pit_limiter);
    assert!(t.flags.traction_control);
    assert!(t.flags.abs_active);
    assert_eq!(
        t.extended.get("shift_light"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

// ── Extended data ────────────────────────────────────────────────────────────

#[test]
fn deep_turbo_and_oil_extended() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_TURBO, 1.5);
    write_f32(&mut buf, OFF_OIL_PRESSURE, 4.0);
    write_f32(&mut buf, OFF_OIL_TEMP, 110.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("turbo_bar"),
        Some(&TelemetryValue::Float(1.5))
    );
    assert_eq!(
        t.extended.get("oil_pressure_bar"),
        Some(&TelemetryValue::Float(4.0))
    );
    assert_eq!(
        t.extended.get("oil_temp_c"),
        Some(&TelemetryValue::Float(110.0))
    );
    Ok(())
}

// ── Zero packet defaults ────────────────────────────────────────────────────

#[test]
fn deep_all_zeros_packet() -> TestResult {
    let adapter = LFSAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    // gear raw 0 → reverse (-1) for LFS OutGauge
    assert_eq!(t.gear, -1);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    Ok(())
}
