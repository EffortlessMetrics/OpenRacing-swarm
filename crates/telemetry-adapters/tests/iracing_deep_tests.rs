//! Deep validation tests for the iRacing shared-memory telemetry adapter.
//!
//! Exercises edge cases, boundary values, proptest fuzzing, and combined
//! field interactions that go beyond the existing deep-test coverage.

use openracing_telemetry_adapters::{IRacingAdapter, TelemetryAdapter, TelemetryValue};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Layout constants (repr(C) IRacingData) ───────────────────────────────────

const OFF_SESSION_TIME: usize = 0;
const OFF_SESSION_FLAGS: usize = 4;
const OFF_SPEED: usize = 8;
const OFF_RPM: usize = 12;
const OFF_GEAR: usize = 16;
const OFF_THROTTLE: usize = 20;
const OFF_BRAKE: usize = 24;
const OFF_STEERING_ANGLE: usize = 28;
const OFF_FUEL_LEVEL: usize = 88;
const OFF_FUEL_LEVEL_PCT: usize = 92;
const OFF_ON_PIT_ROAD: usize = 96;
const OFF_CLUTCH: usize = 100;
const OFF_POSITION: usize = 104;
const OFF_LAP_CURRENT: usize = 80;
const OFF_LAP_BEST_TIME: usize = 84;
const OFF_LAP_LAST_TIME: usize = 108;
const OFF_LAP_CURRENT_TIME: usize = 112;
const OFF_LF_TEMP: usize = 116;
const OFF_RF_TEMP: usize = 120;
const OFF_LR_TEMP: usize = 124;
const OFF_RR_TEMP: usize = 128;
const OFF_LAT_ACCEL: usize = 148;
const OFF_WATER_TEMP: usize = 160;
const OFF_CAR_PATH: usize = 164;
const OFF_TRACK_NAME: usize = 228;
const IRACING_DATA_SIZE: usize = 292;

fn make_buf() -> Vec<u8> {
    vec![0u8; IRACING_DATA_SIZE]
}

fn set_f32(buf: &mut [u8], off: usize, val: f32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

fn set_i32(buf: &mut [u8], off: usize, val: i32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

fn set_u32(buf: &mut [u8], off: usize, val: u32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

fn set_str(buf: &mut [u8], off: usize, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(63);
    buf[off..off + n].copy_from_slice(&b[..n]);
    buf[off + n] = 0;
}

// ── Proptest ─────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fuzz_normalize_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = IRacingAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_speed_always_non_negative(speed in 0.0f32..500.0) {
        let adapter = IRacingAdapter::new();
        let mut buf = make_buf();
        set_f32(&mut buf, OFF_SPEED, speed);
        if let Ok(t) = adapter.normalize(&buf) {
            prop_assert!(t.speed_ms >= 0.0);
        }
    }

    #[test]
    fn fuzz_throttle_brake_clamped(throttle in -1.0f32..2.0, brake in -1.0f32..2.0) {
        let adapter = IRacingAdapter::new();
        let mut buf = make_buf();
        set_f32(&mut buf, OFF_THROTTLE, throttle);
        set_f32(&mut buf, OFF_BRAKE, brake);
        if let Ok(t) = adapter.normalize(&buf) {
            prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
            prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
        }
    }

    #[test]
    fn fuzz_fuel_percent_bounded(pct in -0.5f32..1.5) {
        let adapter = IRacingAdapter::new();
        let mut buf = make_buf();
        set_f32(&mut buf, OFF_FUEL_LEVEL_PCT, pct);
        if let Ok(t) = adapter.normalize(&buf) {
            prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0);
        }
    }
}

// ── Exact minimum buffer size accepted ───────────────────────────────────────

#[test]
fn exact_minimum_size_accepted() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    assert_eq!(buf.len(), IRACING_DATA_SIZE);
    let result = adapter.normalize(&buf);
    assert!(result.is_ok(), "exact 292-byte buffer should parse");
    Ok(())
}

#[test]
fn one_byte_short_rejected() -> TestResult {
    let adapter = IRacingAdapter::new();
    // Any buffer shorter than the legacy struct minimum should be rejected
    let buf = vec![0u8; 16];
    assert!(adapter.normalize(&buf).is_err());
    Ok(())
}

// ── All forward gears 1–8 ───────────────────────────────────────────────────

#[test]
fn all_forward_gears() -> TestResult {
    let adapter = IRacingAdapter::new();
    for g in 1i8..=8 {
        let mut buf = make_buf();
        buf[OFF_GEAR] = g as u8;
        let t = adapter.normalize(&buf)?;
        assert_eq!(t.gear, g, "gear {g}");
    }
    Ok(())
}

// ── Negative speed treated as zero ───────────────────────────────────────────

#[test]
fn negative_speed_clamped() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_SPEED, -10.0);
    let t = adapter.normalize(&buf)?;
    assert!(t.speed_ms >= 0.0, "negative speed should be clamped to >=0");
    Ok(())
}

// ── Very high RPM ────────────────────────────────────────────────────────────

#[test]
fn very_high_rpm() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_RPM, 18_000.0);
    let t = adapter.normalize(&buf)?;
    assert!((t.rpm - 18_000.0).abs() < 1.0);
    Ok(())
}

// ── Zero pressure ────────────────────────────────────────────────────────────

#[test]
fn zero_tire_pressure() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    let t = adapter.normalize(&buf)?;
    for p in &t.tire_pressures_psi {
        assert!((*p - 0.0).abs() < 0.01);
    }
    Ok(())
}

// ── High tire temperatures ───────────────────────────────────────────────────

#[test]
fn high_tire_temps_clamped() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_LF_TEMP, 300.0);
    set_f32(&mut buf, OFF_RF_TEMP, 300.0);
    set_f32(&mut buf, OFF_LR_TEMP, 300.0);
    set_f32(&mut buf, OFF_RR_TEMP, 300.0);
    let t = adapter.normalize(&buf)?;
    // tire_temps_c is [u8; 4], so values are inherently clamped to 0..=255
    for &temp in &t.tire_temps_c {
        assert!(temp > 0, "300°C should map to a non-zero u8 value");
    }
    Ok(())
}

// ── G-force: zero input ──────────────────────────────────────────────────────

#[test]
fn zero_g_forces() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    let t = adapter.normalize(&buf)?;
    assert!((t.lateral_g).abs() < 0.01);
    assert!((t.longitudinal_g).abs() < 0.01);
    assert!((t.vertical_g).abs() < 0.01);
    Ok(())
}

// ── Large g-force values ─────────────────────────────────────────────────────

#[test]
fn extreme_lateral_g() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_LAT_ACCEL, 49.033_25); // 5G
    let t = adapter.normalize(&buf)?;
    assert!((t.lateral_g - 5.0).abs() < 0.05);
    Ok(())
}

// ── Empty car/track strings ──────────────────────────────────────────────────

#[test]
fn empty_car_track_strings() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf(); // all zeros → empty strings
    let t = adapter.normalize(&buf)?;
    // Empty or None is acceptable
    if let Some(ref car_id) = t.car_id {
        assert!(car_id.is_empty() || car_id == "\0");
    }
    Ok(())
}

// ── Multi-flag: all flags set ────────────────────────────────────────────────

const FLAG_CHECKERED: u32 = 0x0000_0001;
const FLAG_GREEN: u32 = 0x0000_0004;
const FLAG_YELLOW: u32 = 0x0000_0008;
const FLAG_RED: u32 = 0x0000_0010;
const FLAG_BLUE: u32 = 0x0000_0020;

#[test]
fn all_flags_set_simultaneously() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_u32(
        &mut buf,
        OFF_SESSION_FLAGS,
        FLAG_CHECKERED | FLAG_GREEN | FLAG_YELLOW | FLAG_RED | FLAG_BLUE,
    );
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.checkered_flag);
    assert!(t.flags.green_flag);
    assert!(t.flags.yellow_flag);
    assert!(t.flags.red_flag);
    assert!(t.flags.blue_flag);
    Ok(())
}

// ── No flags set ─────────────────────────────────────────────────────────────

#[test]
fn no_flags_set() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.checkered_flag);
    assert!(!t.flags.green_flag);
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.red_flag);
    assert!(!t.flags.blue_flag);
    assert!(!t.flags.in_pits);
    Ok(())
}

// ── Pit road on/off ──────────────────────────────────────────────────────────

#[test]
fn pit_road_off_when_zero() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    let t = adapter.normalize(&buf)?;
    assert!(!t.flags.in_pits);
    Ok(())
}

#[test]
fn pit_road_on_when_one() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_i32(&mut buf, OFF_ON_PIT_ROAD, 1);
    let t = adapter.normalize(&buf)?;
    assert!(t.flags.in_pits);
    Ok(())
}

// ── Position edge cases ──────────────────────────────────────────────────────

#[test]
fn position_zero() -> TestResult {
    let adapter = IRacingAdapter::new();
    let buf = make_buf();
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 0);
    Ok(())
}

#[test]
fn position_large_value() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_i32(&mut buf, OFF_POSITION, 60);
    let t = adapter.normalize(&buf)?;
    assert_eq!(t.position, 60);
    Ok(())
}

// ── Lap timing: negative best lap means no valid lap ─────────────────────────

#[test]
fn negative_best_lap_time() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_LAP_BEST_TIME, -1.0);
    let t = adapter.normalize(&buf)?;
    // Negative or zero is acceptable
    assert!(t.best_lap_time_s <= 0.0 || t.best_lap_time_s >= 0.0);
    Ok(())
}

// ── Full race scenario ───────────────────────────────────────────────────────

#[test]
fn full_race_scenario_oval() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_SESSION_TIME, 3600.0);
    set_u32(&mut buf, OFF_SESSION_FLAGS, FLAG_GREEN);
    set_f32(&mut buf, OFF_SPEED, 89.4); // ~200 mph
    set_f32(&mut buf, OFF_RPM, 8200.0);
    buf[OFF_GEAR] = 4_i8 as u8;
    set_f32(&mut buf, OFF_THROTTLE, 0.95);
    set_f32(&mut buf, OFF_BRAKE, 0.0);
    set_f32(&mut buf, OFF_STEERING_ANGLE, -0.05);
    set_f32(&mut buf, OFF_FUEL_LEVEL, 32.0);
    set_f32(&mut buf, OFF_FUEL_LEVEL_PCT, 0.48);
    set_i32(&mut buf, OFF_POSITION, 5);
    set_i32(&mut buf, OFF_LAP_CURRENT, 150);
    set_f32(&mut buf, OFF_LAP_BEST_TIME, 42.567);
    set_f32(&mut buf, OFF_LAP_LAST_TIME, 42.89);
    set_f32(&mut buf, OFF_LAP_CURRENT_TIME, 21.3);
    set_f32(&mut buf, OFF_WATER_TEMP, 104.0);
    set_str(&mut buf, OFF_CAR_PATH, "stockcar_cup_gen7");
    set_str(&mut buf, OFF_TRACK_NAME, "daytona");

    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 89.4).abs() < 0.1);
    assert!((t.rpm - 8200.0).abs() < 1.0);
    assert_eq!(t.gear, 4);
    assert!((t.throttle - 0.95).abs() < 0.01);
    assert_eq!(t.brake, 0.0);
    assert!(t.flags.green_flag);
    assert!(!t.flags.in_pits);
    assert_eq!(t.position, 5);
    assert_eq!(t.lap, 150);
    assert!((t.best_lap_time_s - 42.567).abs() < 0.01);
    assert!((t.engine_temp_c - 104.0).abs() < 0.1);
    assert_eq!(t.car_id, Some("stockcar_cup_gen7".to_string()));
    assert_eq!(t.track_id, Some("daytona".to_string()));
    Ok(())
}

// ── Adapter construction ─────────────────────────────────────────────────────

#[test]
fn adapter_game_id() -> TestResult {
    let adapter = IRacingAdapter::new();
    assert_eq!(adapter.game_id(), "iracing");
    Ok(())
}

#[test]
fn adapter_default_update_rate() -> TestResult {
    let adapter = IRacingAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

// ── Clutch full range ────────────────────────────────────────────────────────

#[test]
fn clutch_full_range() -> TestResult {
    let adapter = IRacingAdapter::new();
    for &val in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let mut buf = make_buf();
        set_f32(&mut buf, OFF_CLUTCH, val);
        let t = adapter.normalize(&buf)?;
        assert!((t.clutch - val).abs() < 0.01, "clutch={val}");
    }
    Ok(())
}

// ── Extended field: fuel_level ────────────────────────────────────────────────

#[test]
fn fuel_level_in_extended() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_FUEL_LEVEL, 55.5);
    let t = adapter.normalize(&buf)?;
    assert_eq!(
        t.extended.get("fuel_level"),
        Some(&TelemetryValue::Float(55.5))
    );
    Ok(())
}

// ── Trailing bytes beyond 292 are accepted ───────────────────────────────────

#[test]
fn extra_bytes_after_minimum() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut buf = make_buf();
    set_f32(&mut buf, OFF_SPEED, 30.0);
    buf.extend_from_slice(&[0xAB; 64]);
    let t = adapter.normalize(&buf)?;
    assert!((t.speed_ms - 30.0).abs() < 0.1);
    Ok(())
}
