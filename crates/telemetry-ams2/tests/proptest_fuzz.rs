//! Property-based fuzz tests for AMS2/PCars2 shared memory parsing.
//!
//! Ensures the parser never panics on arbitrary or random input,
//! verifies invariants on valid parsed output, tests boundary conditions,
//! and checks NaN/Inf handling.

use openracing_telemetry_adapters::ams2::AMS2SharedMemory;
use proptest::prelude::*;
use racing_wheel_telemetry_ams2::{AMS2Adapter, NormalizedTelemetry, TelemetryAdapter};

const AMS2_MEM_SIZE: usize = std::mem::size_of::<AMS2SharedMemory>();

// ── Helpers ─────────────────────────────────────────────────────────────────

fn make_ams2_bytes(mem: &AMS2SharedMemory) -> Vec<u8> {
    let ptr = mem as *const AMS2SharedMemory as *const u8;
    unsafe { std::slice::from_raw_parts(ptr, AMS2_MEM_SIZE) }.to_vec()
}

fn assert_telemetry_invariants(t: &NormalizedTelemetry) {
    assert!(
        t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
        "speed_ms invalid: {}",
        t.speed_ms
    );
    assert!(t.rpm >= 0.0 && t.rpm.is_finite(), "rpm invalid: {}", t.rpm);
    assert!(
        t.throttle >= 0.0 && t.throttle <= 1.0,
        "throttle out of 0.0..=1.0: {}",
        t.throttle
    );
    assert!(
        t.brake >= 0.0 && t.brake <= 1.0,
        "brake out of 0.0..=1.0: {}",
        t.brake
    );
    assert!(
        t.clutch >= 0.0 && t.clutch <= 1.0,
        "clutch out of 0.0..=1.0: {}",
        t.clutch
    );
    assert!(
        t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
        "fuel_percent out of 0.0..=1.0: {}",
        t.fuel_percent
    );
}

// ── 1. Random byte arrays → parse never panics ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = AMS2Adapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly `size_of::<AMS2SharedMemory>()` bytes filled with
    /// random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(
            any::<u8>(),
            AMS2_MEM_SIZE..=AMS2_MEM_SIZE
        )
    ) {
        let adapter = AMS2Adapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ── 2. Invariant verification on valid-size parsed output ───────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Valid AMS2 shared memory with realistic physics must produce correct invariants.
    #[test]
    fn prop_valid_data_invariants(
        rpm in 0.0f32..18000.0,
        max_rpm in 100.0f32..25000.0,
        speed in 0.0f32..120.0,
        throttle in 0.0f32..1.0,
        brake in 0.0f32..1.0,
        clutch in 0.0f32..1.0,
        steering in -1.0f32..1.0,
        gear in -1i8..=8i8,
        fuel_level in 0.0f32..100.0,
        fuel_capacity in 1.0f32..120.0,
    ) {
        let mut mem = AMS2SharedMemory::default();
        mem.rpm = rpm;
        mem.max_rpm = max_rpm;
        mem.speed = speed;
        mem.throttle = throttle;
        mem.brake = brake;
        mem.clutch = clutch;
        mem.steering = steering;
        mem.gear = gear;
        mem.fuel_level = fuel_level;
        mem.fuel_capacity = fuel_capacity;

        let data = make_ams2_bytes(&mem);
        let adapter = AMS2Adapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid AMS2 data must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        assert_telemetry_invariants(&t);

        // RPM preserved (builder clamps negative to 0)
        let expected_rpm = rpm.max(0.0);
        prop_assert!((t.rpm - expected_rpm).abs() < 0.1,
            "rpm mismatch: {} vs {}", t.rpm, expected_rpm);

        // Speed preserved (builder clamps negative to 0)
        let expected_speed = speed.max(0.0);
        prop_assert!((t.speed_ms - expected_speed).abs() < 0.1,
            "speed_ms mismatch: {} vs {}", t.speed_ms, expected_speed);

        // Fuel percentage = fuel_level / fuel_capacity, clamped to [0, 1]
        let expected_fuel = (fuel_level / fuel_capacity).clamp(0.0, 1.0);
        prop_assert!((t.fuel_percent - expected_fuel).abs() < 0.01,
            "fuel_percent mismatch: {} vs {}", t.fuel_percent, expected_fuel);

        // Gear preserved
        prop_assert_eq!(t.gear, gear, "gear mismatch");
    }

    /// Throttle/brake/clutch round-trip: input → NormalizedTelemetry preserves values.
    #[test]
    fn prop_controls_round_trip(
        throttle in 0.0f32..1.0,
        brake in 0.0f32..1.0,
        clutch in 0.0f32..1.0,
    ) {
        let mut mem = AMS2SharedMemory::default();
        mem.throttle = throttle;
        mem.brake = brake;
        mem.clutch = clutch;
        mem.speed = 50.0;
        mem.rpm = 5000.0;
        mem.max_rpm = 10000.0;

        let data = make_ams2_bytes(&mem);
        let adapter = AMS2Adapter::new();
        let t = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!((t.throttle - throttle).abs() < 0.01,
            "throttle {} vs expected {}", t.throttle, throttle);
        prop_assert!((t.brake - brake).abs() < 0.01,
            "brake {} vs expected {}", t.brake, brake);
        prop_assert!((t.clutch - clutch).abs() < 0.01,
            "clutch {} vs expected {}", t.clutch, clutch);
    }
}

// ── 3. Truncated packet → graceful error ────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Any buffer shorter than AMS2SharedMemory size must be rejected.
    #[test]
    fn prop_truncated_rejected(len in 0usize..AMS2_MEM_SIZE) {
        let data = vec![0u8; len];
        let adapter = AMS2Adapter::new();
        prop_assert!(adapter.normalize(&data).is_err(),
            "packet of len {} should be rejected (need {})", len, AMS2_MEM_SIZE);
    }

    /// Off-by-one below the required size must be rejected.
    #[test]
    fn prop_off_by_one_below(
        data in proptest::collection::vec(any::<u8>(), (AMS2_MEM_SIZE - 1)..=AMS2_MEM_SIZE - 1)
    ) {
        let adapter = AMS2Adapter::new();
        prop_assert!(adapter.normalize(&data).is_err());
    }
}

// ── 4. Oversized buffers → still parse correctly ────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Buffers larger than AMS2SharedMemory must still parse (extra bytes ignored).
    #[test]
    fn prop_oversized_parses(extra in 1usize..256) {
        let mut mem = AMS2SharedMemory::default();
        mem.rpm = 5000.0;
        mem.max_rpm = 10000.0;
        mem.speed = 30.0;

        let mut data = make_ams2_bytes(&mem);
        data.extend(vec![0xFFu8; extra]);

        let adapter = AMS2Adapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "oversized buffer must still parse");
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((t.rpm - 5000.0).abs() < 0.1);
        prop_assert!((t.speed_ms - 30.0).abs() < 0.1);
    }
}

// ── 5. NaN / Infinity handling ──────────────────────────────────────────────

#[test]
fn nan_in_all_float_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut mem = AMS2SharedMemory::default();
    mem.rpm = f32::NAN;
    mem.max_rpm = f32::NAN;
    mem.speed = f32::NAN;
    mem.throttle = f32::NAN;
    mem.brake = f32::NAN;
    mem.clutch = f32::NAN;
    mem.steering = f32::NAN;
    mem.fuel_level = f32::NAN;
    mem.fuel_capacity = f32::NAN;

    let data = make_ams2_bytes(&mem);
    let adapter = AMS2Adapter::new();
    // Must not panic — builder sanitizes NaN → 0
    let result = adapter.normalize(&data);
    assert!(result.is_ok(), "NaN input must not cause error");
    let t = result?;
    // Builder clamps NaN to 0 for numeric fields
    assert!(t.speed_ms.is_finite(), "speed_ms must be finite after NaN");
    assert!(t.rpm.is_finite(), "rpm must be finite after NaN");
    assert!(t.throttle.is_finite(), "throttle must be finite after NaN");
    Ok(())
}

#[test]
fn infinity_in_all_float_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut mem = AMS2SharedMemory::default();
    mem.rpm = f32::INFINITY;
    mem.max_rpm = f32::INFINITY;
    mem.speed = f32::INFINITY;
    mem.throttle = f32::INFINITY;
    mem.brake = f32::NEG_INFINITY;
    mem.clutch = f32::INFINITY;
    mem.fuel_level = f32::INFINITY;
    mem.fuel_capacity = f32::INFINITY;

    let data = make_ams2_bytes(&mem);
    let adapter = AMS2Adapter::new();
    // Must not panic
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn neg_infinity_speed_rpm() -> Result<(), Box<dyn std::error::Error>> {
    let mut mem = AMS2SharedMemory::default();
    mem.rpm = f32::NEG_INFINITY;
    mem.speed = f32::NEG_INFINITY;
    mem.max_rpm = 10000.0;

    let data = make_ams2_bytes(&mem);
    let adapter = AMS2Adapter::new();
    let result = adapter.normalize(&data);
    assert!(result.is_ok());
    let t = result?;
    // Builder clamps negative to 0
    assert!(t.speed_ms >= 0.0, "neg inf speed should be clamped to >= 0");
    assert!(t.rpm >= 0.0, "neg inf rpm should be clamped to >= 0");
    Ok(())
}

// ── 6. Deterministic normalization ──────────────────────────────────────────

#[test]
fn deterministic_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let mut mem = AMS2SharedMemory::default();
    mem.rpm = 7500.0;
    mem.max_rpm = 12000.0;
    mem.speed = 45.0;
    mem.throttle = 0.8;
    mem.brake = 0.2;
    mem.clutch = 0.0;
    mem.gear = 4;
    mem.steering = 0.15;

    let data = make_ams2_bytes(&mem);
    let adapter = AMS2Adapter::new();
    let a = adapter.normalize(&data)?;
    let b = adapter.normalize(&data)?;
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.throttle - b.throttle).abs() < f32::EPSILON);
    assert!((a.brake - b.brake).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    Ok(())
}

// ── 7. Zero fuel capacity → zero fuel percent ──────────────────────────────

#[test]
fn zero_fuel_capacity_no_division_by_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut mem = AMS2SharedMemory::default();
    mem.fuel_level = 50.0;
    mem.fuel_capacity = 0.0;
    mem.rpm = 3000.0;
    mem.max_rpm = 8000.0;
    mem.speed = 20.0;

    let data = make_ams2_bytes(&mem);
    let adapter = AMS2Adapter::new();
    let t = adapter.normalize(&data)?;
    assert!(
        t.fuel_percent >= 0.0 && t.fuel_percent.is_finite(),
        "fuel_percent must be valid when capacity=0: {}",
        t.fuel_percent
    );
    Ok(())
}

// ── 8. G-force direction mapping ────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// G-forces must remain finite for any acceleration values.
    #[test]
    fn prop_gforce_finite(
        ax in -50.0f32..50.0,
        ay in -50.0f32..50.0,
        az in -50.0f32..50.0,
    ) {
        let mut mem = AMS2SharedMemory::default();
        mem.local_acceleration = [ax, ay, az];
        mem.rpm = 3000.0;
        mem.max_rpm = 8000.0;
        mem.speed = 20.0;

        let data = make_ams2_bytes(&mem);
        let adapter = AMS2Adapter::new();
        let t = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.lateral_g.is_finite(), "lateral_g not finite");
        prop_assert!(t.longitudinal_g.is_finite(), "longitudinal_g not finite");
        prop_assert!(t.vertical_g.is_finite(), "vertical_g not finite");
    }
}

// ── 9. Tire data invariants ─────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Tire temperatures and pressures must be non-negative after normalization.
    #[test]
    fn prop_tire_data_invariants(
        temp_fl in -50.0f32..300.0,
        temp_fr in -50.0f32..300.0,
        temp_rl in -50.0f32..300.0,
        temp_rr in -50.0f32..300.0,
        press_fl in -10.0f32..400.0,
        press_fr in -10.0f32..400.0,
        press_rl in -10.0f32..400.0,
        press_rr in -10.0f32..400.0,
    ) {
        let mut mem = AMS2SharedMemory::default();
        mem.tyre_temp = [temp_fl, temp_fr, temp_rl, temp_rr];
        mem.air_pressure = [press_fl, press_fr, press_rl, press_rr];
        mem.rpm = 3000.0;
        mem.max_rpm = 8000.0;
        mem.speed = 20.0;

        let data = make_ams2_bytes(&mem);
        let adapter = AMS2Adapter::new();
        let t = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        for &_temp in &t.tire_temps_c {
            // u8 type already constrains to 0..=255
        }
        for &psi in &t.tire_pressures_psi {
            prop_assert!(psi >= 0.0 && psi.is_finite(),
                "tire pressure must be >= 0 and finite: {}", psi);
        }
    }
}
