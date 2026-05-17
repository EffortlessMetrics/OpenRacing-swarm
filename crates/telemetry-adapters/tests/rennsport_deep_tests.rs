//! Deep protocol-level tests for the Rennsport UDP telemetry adapter.
//!
//! Exercises packet parsing, identifier validation, speed conversion (km/h → m/s),
//! FFB/slip-ratio clamping, gear encoding, and corrupted-packet handling against
//! the 24-byte Rennsport UDP protocol (port 9000, identifier byte 0x52 'R').

use openracing_telemetry_adapters::{RennsportAdapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Rennsport UDP byte offsets ───────────────────────────────────────────────

const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

const RENNSPORT_MIN_PACKET_SIZE: usize = 24;
const RENNSPORT_IDENTIFIER: u8 = 0x52; // 'R'

fn make_packet(speed_kmh: f32, rpm: f32, gear: i8, ffb: f32, slip: f32) -> Vec<u8> {
    let mut data = vec![0u8; RENNSPORT_MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear as u8;
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip.to_le_bytes());
    data
}

fn make_base_packet() -> Vec<u8> {
    make_packet(100.0, 5000.0, 3, 0.5, 0.1)
}

fn adapter() -> RennsportAdapter {
    RennsportAdapter::new()
}

// ── 1. Adapter metadata ─────────────────────────────────────────────────────

#[test]
fn rennsport_game_id() -> TestResult {
    assert_eq!(adapter().game_id(), "rennsport");
    Ok(())
}

#[test]
fn rennsport_update_rate_60hz() -> TestResult {
    assert_eq!(adapter().expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

#[test]
fn rennsport_custom_port() -> TestResult {
    let a = RennsportAdapter::new().with_port(9001);
    assert_eq!(a.game_id(), "rennsport");
    Ok(())
}

// ── 2. Identifier validation ────────────────────────────────────────────────

#[test]
fn rennsport_correct_identifier_accepted() -> TestResult {
    let data = make_base_packet();
    let t = adapter().normalize(&data)?;
    assert!(t.speed_ms > 0.0);
    Ok(())
}

#[test]
fn rennsport_wrong_identifier_rejected() -> TestResult {
    let mut data = make_base_packet();
    data[OFF_IDENTIFIER] = 0x41; // 'A'
    assert!(adapter().normalize(&data).is_err());
    Ok(())
}

#[test]
fn rennsport_zero_identifier_rejected() -> TestResult {
    let mut data = make_base_packet();
    data[OFF_IDENTIFIER] = 0x00;
    assert!(adapter().normalize(&data).is_err());
    Ok(())
}

// ── 3. Packet size validation ───────────────────────────────────────────────

#[test]
fn rennsport_empty_packet_rejected() -> TestResult {
    assert!(adapter().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn rennsport_one_byte_short_rejected() -> TestResult {
    let buf = vec![0u8; RENNSPORT_MIN_PACKET_SIZE - 1];
    assert!(adapter().normalize(&buf).is_err());
    Ok(())
}

#[test]
fn rennsport_exact_24_bytes_accepted() -> TestResult {
    let data = make_base_packet();
    assert_eq!(data.len(), 24);
    assert!(adapter().normalize(&data).is_ok());
    Ok(())
}

#[test]
fn rennsport_oversized_packet_accepted() -> TestResult {
    let mut data = vec![0u8; 512];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    let t = adapter().normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

// ── 4. Speed conversion (km/h → m/s) ───────────────────────────────────────

#[test]
fn rennsport_speed_kmh_to_ms_conversion() -> TestResult {
    // 180 km/h = 50 m/s
    let data = make_packet(180.0, 7500.0, 4, 0.5, 0.1);
    let t = adapter().normalize(&data)?;
    assert!((t.speed_ms - 50.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn rennsport_speed_zero() -> TestResult {
    let data = make_packet(0.0, 800.0, 0, 0.0, 0.0);
    let t = adapter().normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001);
    Ok(())
}

#[test]
fn rennsport_negative_speed_clamped_to_zero() -> TestResult {
    let data = make_packet(-10.0, 1000.0, 1, 0.0, 0.0);
    let t = adapter().normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "speed_ms={} should be ≥ 0", t.speed_ms);
    Ok(())
}

// ── 5. RPM extraction ───────────────────────────────────────────────────────

#[test]
fn rennsport_rpm_extraction() -> TestResult {
    let data = make_packet(100.0, 8200.0, 5, 0.3, 0.05);
    let t = adapter().normalize(&data)?;
    assert!((t.rpm - 8200.0).abs() < 0.1);
    Ok(())
}

#[test]
fn rennsport_negative_rpm_clamped_to_zero() -> TestResult {
    let data = make_packet(0.0, -500.0, 0, 0.0, 0.0);
    let t = adapter().normalize(&data)?;
    assert!(t.rpm >= 0.0, "rpm={} should be ≥ 0", t.rpm);
    Ok(())
}

// ── 6. Gear encoding ────────────────────────────────────────────────────────

#[test]
fn rennsport_gear_reverse() -> TestResult {
    let data = make_packet(0.0, 1000.0, -1, 0.0, 0.0);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn rennsport_gear_neutral() -> TestResult {
    let data = make_packet(0.0, 800.0, 0, 0.0, 0.0);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn rennsport_gear_forward_range() -> TestResult {
    for g in 1i8..=7 {
        let data = make_packet(50.0, 5000.0, g, 0.3, 0.05);
        let t = adapter().normalize(&data)?;
        assert_eq!(t.gear, g, "gear {g} mismatch");
    }
    Ok(())
}

// ── 7. FFB scalar clamping ──────────────────────────────────────────────────

#[test]
fn rennsport_ffb_normal_value() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, 0.6, 0.1);
    let t = adapter().normalize(&data)?;
    assert!((t.ffb_scalar - 0.6).abs() < 0.001);
    Ok(())
}

#[test]
fn rennsport_ffb_clamped_above_one() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, 5.0, 0.1);
    let t = adapter().normalize(&data)?;
    assert!(
        t.ffb_scalar <= 1.0,
        "ffb_scalar={} should be ≤ 1.0",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn rennsport_ffb_clamped_below_neg_one() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, -5.0, 0.1);
    let t = adapter().normalize(&data)?;
    assert!(
        t.ffb_scalar >= -1.0,
        "ffb_scalar={} should be ≥ -1.0",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn rennsport_ffb_negative_value() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, -0.7, 0.1);
    let t = adapter().normalize(&data)?;
    assert!((t.ffb_scalar - (-0.7)).abs() < 0.001);
    Ok(())
}

// ── 8. Slip ratio clamping ──────────────────────────────────────────────────

#[test]
fn rennsport_slip_ratio_normal_value() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, 0.5, 0.15);
    let t = adapter().normalize(&data)?;
    assert!((t.slip_ratio - 0.15).abs() < 0.001);
    Ok(())
}

#[test]
fn rennsport_slip_ratio_clamped_above_one() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, 0.5, 2.5);
    let t = adapter().normalize(&data)?;
    assert!(
        t.slip_ratio <= 1.0,
        "slip_ratio={} should be ≤ 1.0",
        t.slip_ratio
    );
    Ok(())
}

#[test]
fn rennsport_slip_ratio_clamped_below_zero() -> TestResult {
    let data = make_packet(100.0, 6000.0, 3, 0.5, -0.5);
    let t = adapter().normalize(&data)?;
    assert!(
        t.slip_ratio >= 0.0,
        "slip_ratio={} should be ≥ 0.0",
        t.slip_ratio
    );
    Ok(())
}

// ── 9. Non-finite / corrupted values ────────────────────────────────────────

#[test]
fn rennsport_nan_speed_becomes_zero() -> TestResult {
    let data = make_packet(f32::NAN, 5000.0, 3, 0.5, 0.1);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0, "NaN speed → 0.0");
    Ok(())
}

#[test]
fn rennsport_infinity_rpm_becomes_zero() -> TestResult {
    let data = make_packet(100.0, f32::INFINITY, 3, 0.5, 0.1);
    let t = adapter().normalize(&data)?;
    assert_eq!(t.rpm, 0.0, "Inf RPM → 0.0");
    Ok(())
}

// ── 10. Endianness round-trip ───────────────────────────────────────────────

#[test]
fn rennsport_endianness_round_trip() -> TestResult {
    let speed_kmh = 144.0f32;
    let rpm = 6500.0f32;
    let ffb = -0.42f32;
    let data = make_packet(speed_kmh, rpm, 4, ffb, 0.08);
    let t = adapter().normalize(&data)?;
    assert!((t.speed_ms - 40.0).abs() < 0.01, "144 km/h = 40 m/s");
    assert!((t.rpm - 6500.0).abs() < 0.1);
    assert!((t.ffb_scalar - (-0.42)).abs() < 0.001);
    assert!((t.slip_ratio - 0.08).abs() < 0.001);
    Ok(())
}

// ── 11. Deterministic output ────────────────────────────────────────────────

#[test]
fn rennsport_same_input_same_output() -> TestResult {
    let a = adapter();
    let data = make_base_packet();
    let t1 = a.normalize(&data)?;
    let t2 = a.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.ffb_scalar, t2.ffb_scalar);
    assert_eq!(t1.slip_ratio, t2.slip_ratio);
    Ok(())
}
