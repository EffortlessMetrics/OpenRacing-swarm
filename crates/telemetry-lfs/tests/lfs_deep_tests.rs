//! Deep tests for Live for Speed (LFS) OutGauge / InSim telemetry adapter.
//!
//! Covers OutGauge protocol packet layout, field parsing, gear encoding,
//! dashboard-light bitmask flags, extended telemetry fields, connection
//! management (port builder), and edge-case handling.

use openracing_telemetry::TelemetryValue;
use racing_wheel_telemetry_lfs::{LFSAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const OUTGAUGE_SIZE: usize = 92;

// OutGauge byte offsets
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

// Dashboard light bitmask flags
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

// ═══════════════════════════════════════════════════════════════════════════════
// OutGauge protocol structure validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_packet_size_constant() -> TestResult {
    assert_eq!(
        OUTGAUGE_SIZE, 92,
        "OutGauge minimum is 92 bytes (without optional ID)"
    );
    Ok(())
}

#[test]
fn protocol_all_offsets_within_packet() -> TestResult {
    let offsets = [
        (OFF_GEAR, 1usize),
        (OFF_SPEED, 4),
        (OFF_RPM, 4),
        (OFF_TURBO, 4),
        (OFF_ENG_TEMP, 4),
        (OFF_FUEL, 4),
        (OFF_OIL_PRESSURE, 4),
        (OFF_OIL_TEMP, 4),
        (OFF_SHOW_LIGHTS, 4),
        (OFF_THROTTLE, 4),
        (OFF_BRAKE, 4),
        (OFF_CLUTCH, 4),
    ];
    for (off, size) in offsets {
        assert!(
            off + size <= OUTGAUGE_SIZE,
            "offset {off} + {size} exceeds packet size"
        );
    }
    Ok(())
}

#[test]
fn protocol_field_no_overlap() -> TestResult {
    // Input group: throttle, brake, clutch are sequential 4-byte fields.
    assert_eq!(OFF_THROTTLE + 4, OFF_BRAKE, "throttle → brake contiguous");
    assert_eq!(OFF_BRAKE + 4, OFF_CLUTCH, "brake → clutch contiguous");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Packet parsing: acceptance and rejection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_exact_92_bytes_accepted() -> TestResult {
    let adapter = LFSAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn parse_91_bytes_rejected() -> TestResult {
    let adapter = LFSAdapter::new();
    assert!(adapter.normalize(&[0u8; 91]).is_err());
    Ok(())
}

#[test]
fn parse_one_byte_rejected() -> TestResult {
    let adapter = LFSAdapter::new();
    assert!(adapter.normalize(&[0x42]).is_err());
    Ok(())
}

#[test]
fn parse_oversized_200_bytes_accepted() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = vec![0u8; 200];
    write_f32(&mut buf, OFF_SPEED, 33.3);
    write_f32(&mut buf, OFF_RPM, 5500.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 33.3).abs() < 0.01);
    assert!((t.rpm - 5500.0).abs() < 0.1);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gear encoding: OutGauge → normalized
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn gear_all_values_0_through_8() -> TestResult {
    let adapter = LFSAdapter::new();
    let expected: [(u8, i8); 9] = [
        (0, -1), // Reverse
        (1, 0),  // Neutral
        (2, 1),  // 1st
        (3, 2),  // 2nd
        (4, 3),  // 3rd
        (5, 4),  // 4th
        (6, 5),  // 5th
        (7, 6),  // 6th
        (8, 7),  // 7th
    ];
    for (raw, norm) in expected {
        let mut buf = make_packet();
        buf[OFF_GEAR] = raw;
        let t = adapter.normalize(&buf)?;
        assert_eq!(t.gear, norm, "raw gear {raw} → normalized {norm}");
    }
    Ok(())
}

#[test]
fn gear_high_raw_value() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 15; // Unusual but possible
    let t = adapter.normalize(&buf)?;
    // 15 - 1 = 14
    assert_eq!(t.gear, 14);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Input fields: throttle, brake, clutch
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn inputs_full_throttle_zero_brake() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 1.0);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_CLUTCH, 0.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    Ok(())
}

#[test]
fn inputs_partial_brake_and_clutch() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 0.0);
    write_f32(&mut buf, OFF_BRAKE, 0.6);
    write_f32(&mut buf, OFF_CLUTCH, 0.75);
    let t = adapter.normalize(&buf)?;
    assert!((t.brake - 0.6).abs() < 0.001);
    assert!((t.clutch - 0.75).abs() < 0.001);
    Ok(())
}

#[test]
fn inputs_simultaneous_throttle_and_brake() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_THROTTLE, 0.5);
    write_f32(&mut buf, OFF_BRAKE, 0.8);
    let t = adapter.normalize(&buf)?;
    assert!((t.throttle - 0.5).abs() < 0.001);
    assert!((t.brake - 0.8).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Engine data
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn engine_high_rpm() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, 11500.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 11500.0).abs() < 0.1);
    Ok(())
}

#[test]
fn engine_temp_passthrough() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_ENG_TEMP, 105.5);
    let t = adapter.normalize(&buf)?;
    assert!((t.engine_temp_c - 105.5).abs() < 0.1);
    Ok(())
}

#[test]
fn engine_fuel_passthrough() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL, 0.42);
    let t = adapter.normalize(&buf)?;
    assert!((t.fuel_percent - 0.42).abs() < 0.001);
    Ok(())
}

#[test]
fn engine_fuel_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL, 0.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dashboard light flags (bitmask)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn flags_no_lights_active() -> TestResult {
    let adapter = LFSAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.pit_limiter);
    assert!(!t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

#[test]
fn flags_individual_pit_limiter() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_PITSPEED);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.pit_limiter);
    assert!(!t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

#[test]
fn flags_individual_tc() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC);
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.pit_limiter);
    assert!(t.flags.traction_control);
    assert!(!t.flags.abs_active);
    Ok(())
}

#[test]
fn flags_individual_abs() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_ABS);
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.pit_limiter);
    assert!(!t.flags.traction_control);
    assert!(t.flags.abs_active);
    Ok(())
}

#[test]
fn flags_all_combined() -> TestResult {
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

#[test]
fn flags_dash_lights_raw_in_extended() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    let raw_val = DL_TC | DL_ABS;
    write_u32(&mut buf, OFF_SHOW_LIGHTS, raw_val);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("dash_lights_raw"),
        Some(&TelemetryValue::Integer(raw_val as i32))
    );
    Ok(())
}

#[test]
fn flags_zero_show_lights_no_raw_extended() -> TestResult {
    let adapter = LFSAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    // When show_lights == 0, dash_lights_raw is not added.
    assert!(
        !t.extended.contains_key("dash_lights_raw"),
        "zero lights → no raw key"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extended telemetry fields
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn extended_turbo_bar() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_TURBO, 2.5);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("turbo_bar"),
        Some(&TelemetryValue::Float(2.5))
    );
    Ok(())
}

#[test]
fn extended_oil_pressure_and_temp() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_OIL_PRESSURE, 5.2);
    write_f32(&mut buf, OFF_OIL_TEMP, 120.0);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("oil_pressure_bar"),
        Some(&TelemetryValue::Float(5.2))
    );
    assert_eq!(
        t.extended.get("oil_temp_c"),
        Some(&TelemetryValue::Float(120.0))
    );
    Ok(())
}

#[test]
fn extended_shift_light_off() -> TestResult {
    let adapter = LFSAdapter::new();
    let buf = make_packet();
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("shift_light"),
        Some(&TelemetryValue::Boolean(false))
    );
    Ok(())
}

#[test]
fn extended_shift_light_on() -> TestResult {
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

// ═══════════════════════════════════════════════════════════════════════════════
// Connection management (port builder)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn connection_default_port() -> TestResult {
    let adapter = LFSAdapter::new();
    // Default port is 30000; we can't read it directly from the public API,
    // but game_id should still work.
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

#[test]
fn connection_custom_port_does_not_affect_id() -> TestResult {
    let adapter = LFSAdapter::new().with_port(12345);
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

#[test]
fn connection_update_rate_60hz() -> TestResult {
    let adapter = LFSAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

#[test]
fn connection_default_trait() -> TestResult {
    let adapter = LFSAdapter::default();
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NaN/Inf resilience
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn nan_speed_gives_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_SPEED, f32::NAN);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms.is_finite());
    Ok(())
}

#[test]
fn inf_rpm_gives_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_RPM, f32::INFINITY);
    let t = adapter.normalize(&buf)?;
    assert!(t.rpm.is_finite());
    Ok(())
}

#[test]
fn nan_fuel_gives_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    write_f32(&mut buf, OFF_FUEL, f32::NAN);
    let t = adapter.normalize(&buf)?;
    assert!(t.fuel_percent.is_finite());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Full race scenario
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_hot_lap() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 5; // 4th gear
    write_f32(&mut buf, OFF_SPEED, 55.0);
    write_f32(&mut buf, OFF_RPM, 8200.0);
    write_f32(&mut buf, OFF_TURBO, 1.8);
    write_f32(&mut buf, OFF_ENG_TEMP, 97.0);
    write_f32(&mut buf, OFF_FUEL, 0.35);
    write_f32(&mut buf, OFF_OIL_PRESSURE, 4.5);
    write_f32(&mut buf, OFF_OIL_TEMP, 115.0);
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_TC | DL_SHIFT);
    write_f32(&mut buf, OFF_THROTTLE, 0.95);
    write_f32(&mut buf, OFF_BRAKE, 0.0);
    write_f32(&mut buf, OFF_CLUTCH, 0.0);

    let t = adapter.normalize(&buf)?;

    assert_eq!(t.gear, 4);
    assert!((t.speed_ms - 55.0).abs() < 0.01);
    assert!((t.rpm - 8200.0).abs() < 0.1);
    assert!((t.throttle - 0.95).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    assert!((t.engine_temp_c - 97.0).abs() < 0.1);
    assert!((t.fuel_percent - 0.35).abs() < 0.001);
    assert!(t.flags.traction_control);
    assert!(!t.flags.abs_active);
    assert_eq!(
        t.extended.get("turbo_bar"),
        Some(&TelemetryValue::Float(1.8))
    );
    assert_eq!(
        t.extended.get("shift_light"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

#[test]
fn scenario_pit_lane_entry() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 2; // 1st gear
    write_f32(&mut buf, OFF_SPEED, 16.5);
    write_f32(&mut buf, OFF_RPM, 3000.0);
    write_f32(&mut buf, OFF_FUEL, 0.12);
    write_u32(&mut buf, OFF_SHOW_LIGHTS, DL_PITSPEED);
    write_f32(&mut buf, OFF_THROTTLE, 0.2);
    write_f32(&mut buf, OFF_BRAKE, 0.0);

    let t = adapter.normalize(&buf)?;

    assert_eq!(t.gear, 1);
    assert!(t.flags.pit_limiter);
    assert!((t.fuel_percent - 0.12).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Determinism
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn deterministic_output() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut buf = make_packet();
    buf[OFF_GEAR] = 4;
    write_f32(&mut buf, OFF_SPEED, 40.0);
    write_f32(&mut buf, OFF_RPM, 6500.0);
    write_f32(&mut buf, OFF_THROTTLE, 0.7);
    write_f32(&mut buf, OFF_BRAKE, 0.15);

    let t1 = adapter.normalize(&buf)?;
    let t2 = adapter.normalize(&buf)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.brake, t2.brake);
    Ok(())
}
