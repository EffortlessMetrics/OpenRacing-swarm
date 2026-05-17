//! Deep protocol-level tests for the Richard Burns Rally (RBR) LiveData UDP adapter.
//!
//! Exercises packet parsing, endianness, field extraction, boundary values,
//! and corrupted-packet handling against the RBR LiveData UDP plugin protocol
//! (128-byte legacy and 184-byte current formats, port 6776).

use openracing_telemetry_adapters::{RBRAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── RBR LiveData byte offsets (all little-endian f32) ────────────────────────

const OFF_SPEED_MS: usize = 12;
const OFF_THROTTLE: usize = 52;
const OFF_BRAKE: usize = 56;
const OFF_CLUTCH: usize = 60;
const OFF_GEAR: usize = 64;
const OFF_STEERING: usize = 68;
const OFF_HANDBRAKE: usize = 112;
const OFF_RPM: usize = 116;

const MIN_PACKET_SIZE: usize = 128;
const FULL_PACKET_SIZE: usize = 184;

fn make_packet(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

fn set_f32(buf: &mut [u8], offset: usize, val: f32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn adapter() -> RBRAdapter {
    RBRAdapter::new()
}

// ── 1. Adapter metadata ─────────────────────────────────────────────────────

#[test]
fn rbr_game_id_is_rbr() -> TestResult {
    assert_eq!(adapter().game_id(), "rbr");
    Ok(())
}

#[test]
fn rbr_update_rate_approx_60hz() -> TestResult {
    let rate = adapter().expected_update_rate();
    assert_eq!(rate, std::time::Duration::from_millis(17));
    Ok(())
}

// ── 2. Packet size validation ───────────────────────────────────────────────

#[test]
fn rbr_empty_packet_rejected() -> TestResult {
    let a = adapter();
    assert!(a.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn rbr_one_byte_short_rejected() -> TestResult {
    let a = adapter();
    let buf = make_packet(MIN_PACKET_SIZE - 1);
    assert!(a.normalize(&buf).is_err());
    Ok(())
}

#[test]
fn rbr_exact_128_bytes_accepted() -> TestResult {
    let a = adapter();
    let buf = make_packet(MIN_PACKET_SIZE);
    let t = a.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn rbr_184_byte_packet_accepted() -> TestResult {
    let a = adapter();
    let buf = make_packet(FULL_PACKET_SIZE);
    let t = a.normalize(&buf)?;
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn rbr_oversized_packet_accepted() -> TestResult {
    let a = adapter();
    let buf = make_packet(256);
    let t = a.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

// ── 3. Endianness correctness (all fields little-endian f32) ────────────────

#[test]
fn rbr_speed_little_endian_round_trip() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    let expected = 42.5f32;
    set_f32(&mut buf, OFF_SPEED_MS, expected);
    let t = a.normalize(&buf)?;
    assert!((t.speed_ms - expected).abs() < 0.001);
    Ok(())
}

#[test]
fn rbr_rpm_little_endian_round_trip() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_RPM, 7250.0);
    let t = a.normalize(&buf)?;
    assert!((t.rpm - 7250.0).abs() < 0.01);
    Ok(())
}

// ── 4. Field extraction accuracy ────────────────────────────────────────────

#[test]
fn rbr_full_driving_scenario() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_SPEED_MS, 28.5);
    set_f32(&mut buf, OFF_RPM, 6200.0);
    set_f32(&mut buf, OFF_GEAR, 4.0);
    set_f32(&mut buf, OFF_THROTTLE, 0.82);
    set_f32(&mut buf, OFF_BRAKE, 0.0);
    set_f32(&mut buf, OFF_CLUTCH, 0.05);
    set_f32(&mut buf, OFF_STEERING, -0.15);

    let t = a.normalize(&buf)?;
    assert!((t.speed_ms - 28.5).abs() < 0.01);
    assert!((t.rpm - 6200.0).abs() < 0.1);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.82).abs() < 0.01);
    assert!(t.brake.abs() < 0.001);
    assert!((t.clutch - 0.05).abs() < 0.01);
    assert!((t.steering_angle - (-0.15)).abs() < 0.01);
    Ok(())
}

#[test]
fn rbr_heavy_braking_scenario() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_SPEED_MS, 55.0);
    set_f32(&mut buf, OFF_RPM, 4800.0);
    set_f32(&mut buf, OFF_GEAR, 3.0);
    set_f32(&mut buf, OFF_THROTTLE, 0.0);
    set_f32(&mut buf, OFF_BRAKE, 0.95);
    set_f32(&mut buf, OFF_STEERING, 0.4);

    let t = a.normalize(&buf)?;
    assert!((t.throttle).abs() < 0.001);
    assert!((t.brake - 0.95).abs() < 0.01);
    // FFB scalar = throttle - brake = -0.95
    assert!((t.ffb_scalar - (-0.95)).abs() < 0.01);
    Ok(())
}

// ── 5. Gear encoding ────────────────────────────────────────────────────────

#[test]
fn rbr_gear_zero_is_reverse() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_GEAR, 0.0);
    let t = a.normalize(&buf)?;
    assert_eq!(t.gear, -1, "gear 0 should be reverse (-1)");
    Ok(())
}

#[test]
fn rbr_gear_fractional_below_half_is_reverse() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_GEAR, 0.4);
    let t = a.normalize(&buf)?;
    assert_eq!(t.gear, -1, "gear 0.4 should be reverse (-1)");
    Ok(())
}

#[test]
fn rbr_gear_1_through_6() -> TestResult {
    let a = adapter();
    for expected_gear in 1i8..=6 {
        let mut buf = make_packet(FULL_PACKET_SIZE);
        set_f32(&mut buf, OFF_GEAR, expected_gear as f32);
        let t = a.normalize(&buf)?;
        assert_eq!(t.gear, expected_gear, "gear {expected_gear} mismatch");
    }
    Ok(())
}

// ── 6. FFB scalar = throttle − brake ────────────────────────────────────────

#[test]
fn rbr_ffb_scalar_full_throttle() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_THROTTLE, 1.0);
    set_f32(&mut buf, OFF_BRAKE, 0.0);
    let t = a.normalize(&buf)?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn rbr_ffb_scalar_full_brake() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_THROTTLE, 0.0);
    set_f32(&mut buf, OFF_BRAKE, 1.0);
    let t = a.normalize(&buf)?;
    assert!((t.ffb_scalar - (-1.0)).abs() < 0.001);
    Ok(())
}

#[test]
fn rbr_ffb_scalar_trail_braking() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_THROTTLE, 0.3);
    set_f32(&mut buf, OFF_BRAKE, 0.6);
    let t = a.normalize(&buf)?;
    assert!((t.ffb_scalar - (-0.3)).abs() < 0.01);
    Ok(())
}

// ── 7. Handbrake flag ───────────────────────────────────────────────────────

#[test]
fn rbr_handbrake_above_threshold_sets_flag() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_HANDBRAKE, 0.8);
    let t = a.normalize(&buf)?;
    assert!(t.flags.session_paused, "handbrake > 0.5 → session_paused");
    Ok(())
}

#[test]
fn rbr_handbrake_below_threshold_clears_flag() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_HANDBRAKE, 0.3);
    let t = a.normalize(&buf)?;
    assert!(!t.flags.session_paused, "handbrake ≤ 0.5 → no flag");
    Ok(())
}

// ── 8. Non-finite / corrupted values ────────────────────────────────────────

#[test]
fn rbr_nan_speed_becomes_zero() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_SPEED_MS, f32::NAN);
    let t = a.normalize(&buf)?;
    assert_eq!(t.speed_ms, 0.0, "NaN speed should default to 0.0");
    Ok(())
}

#[test]
fn rbr_infinity_rpm_becomes_zero() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_RPM, f32::INFINITY);
    let t = a.normalize(&buf)?;
    assert_eq!(t.rpm, 0.0, "Inf RPM should default to 0.0");
    Ok(())
}

#[test]
fn rbr_neg_infinity_throttle_becomes_zero() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_THROTTLE, f32::NEG_INFINITY);
    let t = a.normalize(&buf)?;
    assert_eq!(
        t.throttle, 0.0,
        "NEG_INFINITY throttle should default to 0.0"
    );
    Ok(())
}

// ── 9. Deterministic results ────────────────────────────────────────────────

#[test]
fn rbr_same_input_same_output() -> TestResult {
    let a = adapter();
    let mut buf = make_packet(FULL_PACKET_SIZE);
    set_f32(&mut buf, OFF_SPEED_MS, 35.0);
    set_f32(&mut buf, OFF_RPM, 5000.0);
    set_f32(&mut buf, OFF_GEAR, 3.0);

    let t1 = a.normalize(&buf)?;
    let t2 = a.normalize(&buf)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    Ok(())
}
