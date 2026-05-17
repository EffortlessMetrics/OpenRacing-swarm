//! Property-based and edge-case validation tests for the F1 2023/2024 native
//! UDP telemetry adapter.
//!
//! Complements `f1_deep_tests.rs` with proptest fuzzing, boundary arithmetic,
//! and additional state-machine scenarios.

use openracing_telemetry_adapters::f1_native::{
    F1NativeAdapter, F1NativeState, PACKET_FORMAT_2023, PACKET_FORMAT_2024,
    build_car_status_packet_f23, build_car_status_packet_f24, build_car_telemetry_packet_native,
    build_f1_native_header_bytes, parse_car_status_2023, parse_car_status_2024,
};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use racing_wheel_telemetry_f1::TelemetryAdapter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Proptest: arbitrary telemetry packets never panic ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fuzz_normalize_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let adapter = F1NativeAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_process_packet_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let mut state = F1NativeState::default();
        let _ = F1NativeAdapter::process_packet(&mut state, &data);
    }

    #[test]
    fn fuzz_f23_telemetry_valid_range(
        speed_kmh in 0u16..400,
        gear in 0i8..9,
        rpm in 0u16..16000,
        throttle in 0.0f32..=1.0,
        brake in 0.0f32..=1.0,
    ) {
        let adapter = F1NativeAdapter::new();
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2023, 0, speed_kmh, gear, rpm, throttle, brake, 0.0, 0, [22.0; 4],
        );
        let t = adapter.normalize(&raw).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let expected_ms = f32::from(speed_kmh) / 3.6;
        prop_assert!((t.speed_ms - expected_ms).abs() < 0.2);
        prop_assert_eq!(t.gear, gear);
        prop_assert!((t.rpm - f32::from(rpm)).abs() < 1.0);
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
    }

    #[test]
    fn fuzz_f24_telemetry_valid_range(
        speed_kmh in 0u16..400,
        gear in 0i8..9,
        rpm in 0u16..16000,
    ) {
        let adapter = F1NativeAdapter::new();
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2024, 0, speed_kmh, gear, rpm, 0.5, 0.2, 0.0, 0, [23.0; 4],
        );
        let t = adapter.normalize(&raw).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let expected_ms = f32::from(speed_kmh) / 3.6;
        prop_assert!((t.speed_ms - expected_ms).abs() < 0.2);
    }

    #[test]
    fn fuzz_car_status_f23_fuel_bounded(fuel in 0.0f32..110.0, ers in 0.0f32..4_000_001.0) {
        let raw = build_car_status_packet_f23(0, fuel, ers, 0, 0, 16, 12000);
        let status = parse_car_status_2023(&raw, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!((status.fuel_in_tank - fuel).abs() < 1.0);
        prop_assert!((status.ers_store_energy - ers).abs() < 1.0);
    }

    #[test]
    fn fuzz_car_status_f24_fuel_bounded(fuel in 0.0f32..110.0, ers in 0.0f32..4_000_001.0) {
        let raw = build_car_status_packet_f24(0, fuel, ers, 0, 0, 16, 12000);
        let status = parse_car_status_2024(&raw, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!((status.fuel_in_tank - fuel).abs() < 1.0);
        prop_assert!((status.ers_store_energy - ers).abs() < 1.0);
    }

    #[test]
    fn fuzz_player_index_bounds(idx in 0u8..22) {
        let raw = build_car_status_packet_f23(idx, 30.0, 1_000_000.0, 0, 0, 16, 12000);
        let result = parse_car_status_2023(&raw, usize::from(idx));
        prop_assert!(result.is_ok());
    }
}

// ── Tire pressure ordering across both formats ───────────────────────────────

#[test]
fn tire_pressures_distinct_values_f23() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        9000,
        0.7,
        0.1,
        0.0,
        0,
        [18.0, 19.5, 21.0, 22.5], // wire: RL, RR, FL, FR
    );
    let t = adapter.normalize(&raw)?;
    // Normalized: [FL, FR, RL, RR]
    assert!((t.tire_pressures_psi[0] - 21.0).abs() < 0.1, "FL");
    assert!((t.tire_pressures_psi[1] - 22.5).abs() < 0.1, "FR");
    assert!((t.tire_pressures_psi[2] - 18.0).abs() < 0.1, "RL");
    assert!((t.tire_pressures_psi[3] - 19.5).abs() < 0.1, "RR");
    Ok(())
}

#[test]
fn tire_pressures_distinct_values_f24() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        200,
        5,
        9000,
        0.7,
        0.1,
        0.0,
        0,
        [18.0, 19.5, 21.0, 22.5],
    );
    let t = adapter.normalize(&raw)?;
    assert!((t.tire_pressures_psi[0] - 21.0).abs() < 0.1, "FL");
    assert!((t.tire_pressures_psi[1] - 22.5).abs() < 0.1, "FR");
    assert!((t.tire_pressures_psi[2] - 18.0).abs() < 0.1, "RL");
    assert!((t.tire_pressures_psi[3] - 19.5).abs() < 0.1, "RR");
    Ok(())
}

// ── Full state machine: telemetry → status → telemetry → status ──────────────

#[test]
fn sequential_frames_update_correctly() -> TestResult {
    let telem1 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        100,
        3,
        5000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let status1 = build_car_status_packet_f23(0, 40.0, 2_000_000.0, 0, 0, 16, 11000);
    let telem2 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        250,
        6,
        10500,
        1.0,
        0.0,
        -0.3,
        1,
        [24.0; 4],
    );
    let status2 = build_car_status_packet_f23(0, 38.5, 1_800_000.0, 1, 0, 16, 11000);

    let mut state = F1NativeState::default();

    F1NativeAdapter::process_packet(&mut state, &telem1)?;
    let r1 = F1NativeAdapter::process_packet(&mut state, &status1)?;
    let t1 = r1.ok_or("expected first frame")?;
    assert!((t1.speed_ms - 100.0 / 3.6).abs() < 0.2);
    assert_eq!(t1.gear, 3);

    F1NativeAdapter::process_packet(&mut state, &telem2)?;
    let r2 = F1NativeAdapter::process_packet(&mut state, &status2)?;
    let t2 = r2.ok_or("expected second frame")?;
    assert!((t2.speed_ms - 250.0 / 3.6).abs() < 0.2);
    assert_eq!(t2.gear, 6);
    assert!(t2.flags.drs_active);
    Ok(())
}

// ── Edge case: all-zero telemetry packet ─────────────────────────────────────

#[test]
fn all_zero_telemetry_packet_values() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        0,
        0,
        0,
        0.0,
        0.0,
        0.0,
        0,
        [0.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert!(!t.flags.drs_active);
    Ok(())
}

// ── Edge case: maximum practical values ──────────────────────────────────────

#[test]
fn max_practical_values() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        370,
        8,
        15000,
        1.0,
        1.0,
        -1.0,
        1,
        [30.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!((t.speed_ms - 370.0 / 3.6).abs() < 0.2);
    assert_eq!(t.gear, 8);
    assert!((t.rpm - 15000.0).abs() < 1.0);
    assert!((t.throttle - 1.0).abs() < 0.01);
    assert!((t.brake - 1.0).abs() < 0.01);
    Ok(())
}

// ── Steering angle sign ──────────────────────────────────────────────────────

#[test]
fn steering_negative_angle() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        150,
        4,
        7000,
        0.5,
        0.0,
        -0.8,
        0,
        [22.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!(
        t.steering_angle < 0.0,
        "negative steer should yield negative angle"
    );
    Ok(())
}

#[test]
fn steering_positive_angle() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        150,
        4,
        7000,
        0.5,
        0.0,
        0.8,
        0,
        [22.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!(
        t.steering_angle > 0.0,
        "positive steer should yield positive angle"
    );
    Ok(())
}

// ── Header-only packets are rejected by normalize ────────────────────────────

#[test]
fn header_only_packet_rejected() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 6, 0);
    let result = adapter.normalize(&raw);
    assert!(result.is_err(), "header-only (no car data) should fail");
    Ok(())
}

// ── Car status player index 0..21 all parse correctly ────────────────────────

#[test]
fn all_valid_player_indices_f23() -> TestResult {
    for idx in 0u8..22 {
        let raw = build_car_status_packet_f23(idx, 30.0, 1_500_000.0, 0, 0, 16, 12000);
        let status = parse_car_status_2023(&raw, usize::from(idx))?;
        assert!((status.fuel_in_tank - 30.0).abs() < 0.1, "idx={idx}");
    }
    Ok(())
}

#[test]
fn all_valid_player_indices_f24() -> TestResult {
    for idx in 0u8..22 {
        let raw = build_car_status_packet_f24(idx, 30.0, 1_500_000.0, 0, 0, 18, 13500);
        let status = parse_car_status_2024(&raw, usize::from(idx))?;
        assert!((status.fuel_in_tank - 30.0).abs() < 0.1, "idx={idx}");
    }
    Ok(())
}

// ── ERS fraction: full, half, zero ───────────────────────────────────────────

#[test]
fn ers_full_store() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 4_000_000.0, 0, 0, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(t.extended.contains_key("ers_store_fraction"));
    Ok(())
}

// ── Tyre compound codes ──────────────────────────────────────────────────────

#[test]
fn tyre_compound_stored_in_extended() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        200,
        5,
        9000,
        0.6,
        0.1,
        0.0,
        0,
        [23.0; 4],
    );
    // compound 16=Soft, 17=Medium, 18=Hard
    let status = build_car_status_packet_f24(0, 35.0, 2_000_000.0, 0, 0, 17, 13000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(t.extended.contains_key("tyre_compound"));
    assert!(t.extended.contains_key("tyre_compound_name"));
    Ok(())
}

// ── DRS combinations ─────────────────────────────────────────────────────────

#[test]
fn drs_not_allowed_not_active() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.7,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 0, 0, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(!t.flags.drs_active);
    assert!(!t.flags.drs_available);
    Ok(())
}

#[test]
fn drs_allowed_but_not_active() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.7,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(!t.flags.drs_active);
    assert!(t.flags.drs_available);
    Ok(())
}
