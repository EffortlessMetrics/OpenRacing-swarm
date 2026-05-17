//! Deep tests for Wreckfest (and related) telemetry adapters.

use openracing_telemetry_adapters::wreckfest::parse_wreckfest_packet;
use openracing_telemetry_adapters::{BeamNGAdapter, TelemetryAdapter, WreckfestAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Wreckfest packet layout constants ───────────────────────────────────

const WRECKFEST_MIN_PACKET_SIZE: usize = 28;
const WRECKFEST_MAGIC: [u8; 4] = [0x57, 0x52, 0x4B, 0x46]; // "WRKF"
const OFF_MAGIC: usize = 0;
const OFF_SPEED: usize = 8;
const OFF_RPM: usize = 12;
const OFF_GEAR: usize = 16;
const OFF_LATERAL_G: usize = 20;
const OFF_LONGITUDINAL_G: usize = 24;

fn make_wreckfest_packet(
    speed: f32,
    rpm: f32,
    gear: u8,
    lateral_g: f32,
    longitudinal_g: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; WRECKFEST_MIN_PACKET_SIZE];
    data[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&WRECKFEST_MAGIC);
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear;
    data[OFF_LATERAL_G..OFF_LATERAL_G + 4].copy_from_slice(&lateral_g.to_le_bytes());
    data[OFF_LONGITUDINAL_G..OFF_LONGITUDINAL_G + 4].copy_from_slice(&longitudinal_g.to_le_bytes());
    data
}

// ── Wreckfest parsing tests ─────────────────────────────────────────────

#[test]
fn wreckfest_parse_basic_packet() -> TestResult {
    let data = make_wreckfest_packet(35.0, 5000.0, 4, 0.3, 0.1);
    let result = parse_wreckfest_packet(&data)?;
    assert!((result.speed_ms - 35.0).abs() < 0.01);
    assert!((result.rpm - 5000.0).abs() < 0.1);
    assert_eq!(result.gear, 4);
    assert!((result.lateral_g - 0.3).abs() < 0.01);
    assert!((result.longitudinal_g - 0.1).abs() < 0.01);
    Ok(())
}

#[test]
fn wreckfest_reject_bad_magic() {
    let mut data = make_wreckfest_packet(10.0, 2000.0, 1, 0.0, 0.0);
    data[0] = 0x00;
    assert!(parse_wreckfest_packet(&data).is_err());
}

#[test]
fn wreckfest_reject_short_packet() {
    let data = vec![0u8; 10];
    assert!(parse_wreckfest_packet(&data).is_err());
}

#[test]
fn wreckfest_reject_empty_packet() {
    assert!(parse_wreckfest_packet(&[]).is_err());
}

#[test]
fn wreckfest_neutral_gear() -> TestResult {
    let data = make_wreckfest_packet(0.0, 800.0, 0, 0.0, 0.0);
    let result = parse_wreckfest_packet(&data)?;
    assert_eq!(result.gear, 0);
    Ok(())
}

#[test]
fn wreckfest_ffb_scalar_clamped() -> TestResult {
    // High G-forces → FFB scalar clamped to [-1, 1]
    let data = make_wreckfest_packet(80.0, 7000.0, 5, 5.0, 5.0);
    let result = parse_wreckfest_packet(&data)?;
    assert!(
        result.ffb_scalar >= -1.0 && result.ffb_scalar <= 1.0,
        "ffb_scalar {} out of range",
        result.ffb_scalar
    );
    Ok(())
}

#[test]
fn wreckfest_ffb_scalar_from_gforces() -> TestResult {
    let data = make_wreckfest_packet(40.0, 4000.0, 3, 1.5, 2.0);
    let result = parse_wreckfest_packet(&data)?;
    // combined_g = hypot(1.5, 2.0) = 2.5, ffb_scalar = 2.5/3.0 ≈ 0.833
    let expected = 1.5f32.hypot(2.0) / 3.0;
    assert!((result.ffb_scalar - expected).abs() < 0.01);
    Ok(())
}

#[test]
fn wreckfest_speed_nonnegative() -> TestResult {
    let data = make_wreckfest_packet(50.0, 6000.0, 4, 0.1, 0.2);
    let result = parse_wreckfest_packet(&data)?;
    assert!(result.speed_ms >= 0.0);
    Ok(())
}

#[test]
fn wreckfest_high_gear_capped() -> TestResult {
    let data = make_wreckfest_packet(20.0, 3000.0, 15, 0.0, 0.0);
    let result = parse_wreckfest_packet(&data)?;
    // gear_raw.min(12) as i8 → capped at 12
    assert!(result.gear <= 12);
    Ok(())
}

#[test]
fn wreckfest_adapter_game_id_and_rate() {
    let adapter = WreckfestAdapter::new();
    assert_eq!(adapter.game_id(), "wreckfest");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn wreckfest_adapter_normalize_delegates() -> TestResult {
    let adapter = WreckfestAdapter::new();
    let data = make_wreckfest_packet(25.0, 3500.0, 2, 0.2, 0.1);
    let result = adapter.normalize(&data)?;
    assert!((result.speed_ms - 25.0).abs() < 0.01);
    assert!((result.rpm - 3500.0).abs() < 0.1);
    Ok(())
}

// ── BeamNG adapter basic tests ──────────────────────────────────────────

#[test]
fn beamng_adapter_game_id_and_rate() {
    let adapter = BeamNGAdapter::new();
    assert_eq!(adapter.game_id(), "beamng_drive");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn beamng_adapter_reject_undersized_packet() {
    let adapter = BeamNGAdapter::new();
    let data = vec![0u8; 10];
    assert!(adapter.normalize(&data).is_err());
}
