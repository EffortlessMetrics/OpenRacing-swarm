//! Integration tests for the `racing-wheel-telemetry-ams2` crate.
//!
//! AMS2 uses Windows shared memory, so most behavioral tests are adapter-level
//! (game_id, update_rate, etc.). Shared memory tests require a running AMS2 instance.

use openracing_telemetry_adapters::ams2::{AMS2SharedMemory, DrsState, HighestFlag, PitMode};
use racing_wheel_telemetry_ams2::{AMS2Adapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Helper: serialize an `AMS2SharedMemory` to its raw byte representation.
fn shared_memory_to_bytes(data: &AMS2SharedMemory) -> Vec<u8> {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let ptr = data as *const AMS2SharedMemory as *const u8;
    // SAFETY: AMS2SharedMemory is repr(C) and fully initialized via Default.
    unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec()
}

/// Helper: create a default AMS2SharedMemory (avoids private-field issues with
/// struct update syntax).
fn default_shared_memory() -> AMS2SharedMemory {
    AMS2SharedMemory::default()
}

#[test]
fn test_game_id() {
    let adapter = AMS2Adapter::new();
    assert_eq!(adapter.game_id(), "ams2");
}

#[test]
fn test_default_update_rate() {
    let adapter = AMS2Adapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        Duration::from_millis(16),
        "AMS2 default update rate should be ~60Hz (16ms)"
    );
}

#[test]
fn test_adapter_is_default() {
    let a = AMS2Adapter::new();
    let b = AMS2Adapter::default();
    // Both should have the same game_id and update rate.
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
}

/// Normalizing an empty slice must return an error, not panic.
#[test]
fn test_normalize_empty_returns_error() {
    let adapter = AMS2Adapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty raw data must return error"
    );
}

/// Normalizing arbitrary bytes must not panic.
#[test]
fn test_normalize_arbitrary_bytes_no_panic() {
    let adapter = AMS2Adapter::new();
    // Fill with junk data — result is unspecified but must not panic.
    let _ = adapter.normalize(&vec![0xAB; 2048]);
}

#[tokio::test]
async fn test_is_game_running_returns_result() -> TestResult {
    let adapter = AMS2Adapter::new();
    // Should return Ok(bool) regardless of whether AMS2 is actually running.
    let _ = adapter.is_game_running().await?;
    Ok(())
}

#[tokio::test]
async fn test_stop_monitoring_is_safe() -> TestResult {
    let adapter = AMS2Adapter::new();
    // stop_monitoring should always succeed (no-op when not started).
    adapter.stop_monitoring().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Struct size / layout verification
// ---------------------------------------------------------------------------

/// The AMS2SharedMemory struct must be large enough for the normalize path to
/// accept it without error when serialized to bytes.
#[test]
fn test_shared_memory_struct_size_nonzero() {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    assert!(
        size > 256,
        "AMS2SharedMemory should be a large struct, got {size}"
    );
}

/// Normalizing raw bytes of exactly `size_of::<AMS2SharedMemory>()` must succeed.
#[test]
fn test_normalize_exact_struct_size_succeeds() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = AMS2SharedMemory::default();
    let raw = shared_memory_to_bytes(&data);
    assert_eq!(raw.len(), std::mem::size_of::<AMS2SharedMemory>());
    let _result = adapter.normalize(&raw)?;
    Ok(())
}

/// One byte less than the struct size must return an error.
#[test]
fn test_normalize_one_byte_short_returns_error() {
    let adapter = AMS2Adapter::new();
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let truncated = vec![0u8; size - 1];
    assert!(
        adapter.normalize(&truncated).is_err(),
        "one byte short must return error"
    );
}

// ---------------------------------------------------------------------------
// Byte-level parsing round-trip
// ---------------------------------------------------------------------------

/// Speed and RPM values survive the serialize→normalize round-trip.
#[test]
fn test_normalize_preserves_speed_and_rpm() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.speed = 72.5;
    data.rpm = 8500.0;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(
        (result.speed_ms - 72.5).abs() < 0.01,
        "speed mismatch: {}",
        result.speed_ms
    );
    assert!(
        (result.rpm - 8500.0).abs() < 0.01,
        "rpm mismatch: {}",
        result.rpm
    );
    Ok(())
}

/// Gear value is preserved through normalization.
#[test]
fn test_normalize_gear_passthrough() -> TestResult {
    let adapter = AMS2Adapter::new();
    for gear in [-1i8, 0, 1, 3, 6] {
        let mut data = default_shared_memory();
        data.gear = gear;
        let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
        assert_eq!(result.gear, gear, "gear mismatch for input {gear}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Flag handling
// ---------------------------------------------------------------------------

/// Yellow flag is set when highest_flag == Yellow.
#[test]
fn test_normalize_yellow_flag() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.highest_flag = HighestFlag::Yellow as u32;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.flags.yellow_flag, "yellow_flag should be set");
    assert!(!result.flags.green_flag, "green_flag should not be set");
    Ok(())
}

/// Green flag is set when highest_flag == Green.
#[test]
fn test_normalize_green_flag() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.highest_flag = HighestFlag::Green as u32;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.flags.green_flag, "green_flag should be set");
    Ok(())
}

/// Checkered flag is set when highest_flag == Chequered.
#[test]
fn test_normalize_checkered_flag() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.highest_flag = HighestFlag::Chequered as u32;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.flags.checkered_flag, "checkered_flag should be set");
    Ok(())
}

// ---------------------------------------------------------------------------
// Pit mode and DRS
// ---------------------------------------------------------------------------

/// in_pits flag is set for any non-None pit mode.
#[test]
fn test_normalize_pit_mode_in_pit() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.pit_mode = PitMode::InPit as u32;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.flags.in_pits, "in_pits flag should be set");
    Ok(())
}

/// pit_limiter flag is set when pit_mode == InPitlane.
#[test]
fn test_normalize_pit_limiter() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.pit_mode = PitMode::InPitlane as u32;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.flags.pit_limiter, "pit_limiter should be set");
    Ok(())
}

/// DRS available and active flags.
#[test]
fn test_normalize_drs_available_vs_active() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data_avail = default_shared_memory();
    data_avail.drs_state = DrsState::Available as u32;
    let r = adapter.normalize(&shared_memory_to_bytes(&data_avail))?;
    assert!(r.flags.drs_available);
    assert!(!r.flags.drs_active);

    let mut data_active = default_shared_memory();
    data_active.drs_state = DrsState::Active as u32;
    let r = adapter.normalize(&shared_memory_to_bytes(&data_active))?;
    assert!(r.flags.drs_active);
    Ok(())
}

// ---------------------------------------------------------------------------
// Slip ratio edge cases
// ---------------------------------------------------------------------------

/// Slip ratio is zero when speed ≤ 1.0 m/s.
#[test]
fn test_normalize_slip_zero_at_low_speed() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.speed = 0.5;
    data.tyre_slip = [0.8, 0.8, 0.8, 0.8];
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(
        result.slip_ratio.abs() < 0.001,
        "slip_ratio should be 0 at low speed, got {}",
        result.slip_ratio
    );
    Ok(())
}

/// Slip ratio is clamped to 1.0 even with extreme tyre_slip values.
#[test]
fn test_normalize_slip_clamped_to_one() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.speed = 50.0;
    data.tyre_slip = [5.0, 5.0, 5.0, 5.0];
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(
        (result.slip_ratio - 1.0).abs() < 0.001,
        "slip_ratio should be clamped to 1.0, got {}",
        result.slip_ratio
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// FFB scalar clamping
// ---------------------------------------------------------------------------

/// Steering values beyond ±1.0 are clamped for the FFB scalar.
#[test]
fn test_normalize_ffb_scalar_clamped() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data_high = default_shared_memory();
    data_high.steering = 2.5;
    let r = adapter.normalize(&shared_memory_to_bytes(&data_high))?;
    assert!(
        (r.ffb_scalar - 1.0).abs() < 0.001,
        "ffb_scalar should clamp to 1.0, got {}",
        r.ffb_scalar
    );

    let mut data_low = default_shared_memory();
    data_low.steering = -3.0;
    let r = adapter.normalize(&shared_memory_to_bytes(&data_low))?;
    assert!(
        (r.ffb_scalar - (-1.0)).abs() < 0.001,
        "ffb_scalar should clamp to -1.0, got {}",
        r.ffb_scalar
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Extended data and string extraction
// ---------------------------------------------------------------------------

/// Typed fields (throttle, brake) and extended data keys (fuel_level_l, lap) are populated.
#[test]
fn test_normalize_extended_data_keys() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.throttle = 0.6;
    data.brake = 0.3;
    data.fuel_level = 42.0;
    data.fuel_capacity = 100.0;
    data.laps_completed = 7;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;

    // throttle and brake are now first-class typed fields
    assert!((result.throttle - 0.6).abs() < 0.001);
    assert!((result.brake - 0.3).abs() < 0.001);

    // fuel is now a typed field (percent) + extended (liters)
    assert!((result.fuel_percent - 0.42).abs() < 0.001);
    assert!(result.extended.contains_key("fuel_level_l"));

    // lap is a typed field
    assert_eq!(result.lap, 7);

    Ok(())
}

/// Car name and track location are extracted from byte arrays.
#[test]
fn test_normalize_car_and_track_names() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    let car = b"formula_v10";
    let track = b"spa_francorchamps";
    data.car_name[..car.len()].copy_from_slice(car);
    data.track_location[..track.len()].copy_from_slice(track);
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert_eq!(result.car_id.as_deref(), Some("formula_v10"));
    assert_eq!(result.track_id.as_deref(), Some("spa_francorchamps"));
    Ok(())
}

/// TC and ABS flags are set when their settings are non-zero.
#[test]
fn test_normalize_tc_and_abs_flags() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_shared_memory();
    data.tc_setting = 3;
    data.abs_setting = 2;
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(
        result.flags.traction_control,
        "traction_control should be set"
    );
    assert!(result.flags.abs_active, "abs_active should be set");
    Ok(())
}

/// With TC and ABS at zero, the flags should be false.
#[test]
fn test_normalize_tc_and_abs_off() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = AMS2SharedMemory::default();
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(
        !result.flags.traction_control,
        "traction_control should be off"
    );
    assert!(!result.flags.abs_active, "abs_active should be off");
    Ok(())
}

/// All-zero default data normalizes with sensible defaults.
#[test]
fn test_normalize_default_data_is_sane() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = AMS2SharedMemory::default();
    let result = adapter.normalize(&shared_memory_to_bytes(&data))?;
    assert!(result.speed_ms.abs() < 0.001);
    assert!(result.rpm.abs() < 0.001);
    assert_eq!(result.gear, 0);
    assert!(result.slip_ratio.abs() < 0.001);
    assert!(!result.flags.yellow_flag);
    assert!(!result.flags.red_flag);
    Ok(())
}
