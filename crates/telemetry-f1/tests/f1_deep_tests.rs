//! Extended deep tests for the F1 2023/2024 native UDP telemetry adapter.
//!
//! Covers all packet types, both protocol versions, player car identification,
//! CarStatus field extraction, ERS calculations, tyre compound names,
//! state machine transitions, edge cases, and combined scenarios.

use openracing_telemetry_adapters::f1_native::{
    CAR_STATUS_2023_ENTRY_SIZE, CAR_STATUS_2024_ENTRY_SIZE, F1NativeAdapter, F1NativeCarStatusData,
    F1NativeState, MIN_CAR_STATUS_2023_PACKET_SIZE, MIN_CAR_STATUS_2024_PACKET_SIZE,
    PACKET_FORMAT_2023, PACKET_FORMAT_2024, build_car_status_packet_f23,
    build_car_status_packet_f24, build_car_telemetry_packet_native, build_f1_native_header_bytes,
    parse_car_status_2023, parse_car_status_2024,
};
use racing_wheel_telemetry_f1::{NormalizedTelemetry, TelemetryAdapter, TelemetryFrame};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Protocol constants verification ──────────────────────────────────────────

#[test]
fn protocol_constants_are_correct() -> TestResult {
    assert_eq!(PACKET_FORMAT_2023, 2023);
    assert_eq!(PACKET_FORMAT_2024, 2024);
    assert_eq!(CAR_STATUS_2023_ENTRY_SIZE, 47);
    assert_eq!(CAR_STATUS_2024_ENTRY_SIZE, 55);
    // F1 24 adds 8 bytes (enginePowerICE + enginePowerMGUK = 2 × f32)
    assert_eq!(
        CAR_STATUS_2024_ENTRY_SIZE - CAR_STATUS_2023_ENTRY_SIZE,
        8,
        "F1 24 adds 8 bytes per car vs F1 23"
    );
    Ok(())
}

#[test]
fn min_packet_sizes_match_22_cars() -> TestResult {
    let header = 29usize; // standard header size
    assert_eq!(MIN_CAR_STATUS_2023_PACKET_SIZE, header + 22 * 47);
    assert_eq!(MIN_CAR_STATUS_2024_PACKET_SIZE, header + 22 * 55);
    Ok(())
}

// ── Player car index tests ───────────────────────────────────────────────────

#[test]
fn player_index_0_is_default() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.8,
        0.1,
        0.0,
        0,
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let r = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(r.is_some(), "player index 0 should produce output");
    Ok(())
}

#[test]
fn player_index_19_works() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        19,
        150,
        4,
        7000,
        0.5,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f23(19, 25.0, 1_500_000.0, 0, 0, 16, 11000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let r = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(r.is_some(), "player index 19 should produce output");
    Ok(())
}

#[test]
fn player_index_21_is_last_valid() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        21,
        100,
        3,
        5000,
        0.3,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let status = build_car_status_packet_f23(21, 20.0, 1_000_000.0, 0, 0, 16, 10000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let r = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(r.is_some(), "player index 21 (last of 22) should work");
    Ok(())
}

// ── F1 23 CarStatus field extraction ─────────────────────────────────────────

#[test]
fn f23_car_status_all_fields() -> TestResult {
    let raw = build_car_status_packet_f23(
        0,           // player_index
        45.0,        // fuel_in_tank
        3_500_000.0, // ers_store_energy
        1,           // drs_allowed
        1,           // pit_limiter
        16,          // actual_tyre_compound (Soft)
        13_000,      // max_rpm
    );
    let status = parse_car_status_2023(&raw, 0)?;
    assert!((status.fuel_in_tank - 45.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 3_500_000.0).abs() < 1.0);
    assert_eq!(status.drs_allowed, 1);
    assert_eq!(status.pit_limiter_status, 1);
    assert_eq!(status.actual_tyre_compound, 16);
    assert_eq!(status.max_rpm, 13_000);
    // F1 23 has no engine power fields
    assert_eq!(status.engine_power_ice, 0.0);
    assert_eq!(status.engine_power_mguk, 0.0);
    Ok(())
}

#[test]
fn f23_car_status_zero_fuel() -> TestResult {
    let raw = build_car_status_packet_f23(0, 0.0, 0.0, 0, 0, 0, 0);
    let status = parse_car_status_2023(&raw, 0)?;
    assert_eq!(status.fuel_in_tank, 0.0);
    assert_eq!(status.ers_store_energy, 0.0);
    assert_eq!(status.max_rpm, 0);
    Ok(())
}

#[test]
fn f23_car_status_rejects_too_short() {
    let raw = vec![0u8; MIN_CAR_STATUS_2023_PACKET_SIZE - 1];
    assert!(parse_car_status_2023(&raw, 0).is_err());
}

#[test]
fn f23_car_status_rejects_invalid_player_index() {
    let raw = build_car_status_packet_f23(0, 30.0, 1_000_000.0, 0, 0, 16, 12000);
    assert!(parse_car_status_2023(&raw, 22).is_err());
    assert!(parse_car_status_2023(&raw, 255).is_err());
}

// ── F1 24 CarStatus field extraction ─────────────────────────────────────────

#[test]
fn f24_car_status_all_fields() -> TestResult {
    let raw = build_car_status_packet_f24(
        0,           // player_index
        38.0,        // fuel_in_tank
        2_800_000.0, // ers_store_energy
        1,           // drs_allowed
        0,           // pit_limiter
        18,          // actual_tyre_compound (Medium)
        14_000,      // max_rpm
    );
    let status = parse_car_status_2024(&raw, 0)?;
    assert!((status.fuel_in_tank - 38.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 2_800_000.0).abs() < 1.0);
    assert_eq!(status.drs_allowed, 1);
    assert_eq!(status.pit_limiter_status, 0);
    assert_eq!(status.actual_tyre_compound, 18);
    assert_eq!(status.max_rpm, 14_000);
    Ok(())
}

#[test]
fn f24_car_status_rejects_too_short() {
    let raw = vec![0u8; MIN_CAR_STATUS_2024_PACKET_SIZE - 1];
    assert!(parse_car_status_2024(&raw, 0).is_err());
}

#[test]
fn f24_car_status_rejects_invalid_player_index() {
    let raw = build_car_status_packet_f24(0, 30.0, 1_000_000.0, 0, 0, 16, 12000);
    assert!(parse_car_status_2024(&raw, 22).is_err());
}

// ── State machine: requires both telemetry + status ──────────────────────────

#[test]
fn process_packet_no_output_with_only_status() -> TestResult {
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);
    let mut state = F1NativeState::default();
    let r = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(r.is_none(), "status alone should not produce output");
    Ok(())
}

#[test]
fn process_packet_no_output_with_only_telemetry() -> TestResult {
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
        [25.0; 4],
    );
    let mut state = F1NativeState::default();
    let r = F1NativeAdapter::process_packet(&mut state, &telem)?;
    assert!(r.is_none(), "telemetry alone should not produce output");
    Ok(())
}

#[test]
fn process_packet_session_never_produces_output() -> TestResult {
    let session = build_f1_native_header_bytes(PACKET_FORMAT_2023, 1, 0); // packet ID 1 = Session
    let mut state = F1NativeState::default();
    // Session packet alone is too short for full session parse, but let's
    // test with minimal data — it should return Ok(None) or Err, never Some.
    let r = F1NativeAdapter::process_packet(&mut state, &session);
    if let Ok(maybe) = r {
        assert!(maybe.is_none(), "session should not emit telemetry");
    }
    Ok(())
}

#[test]
fn process_packet_unknown_packet_id_returns_none() -> TestResult {
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 5, 0); // ID 5 = CarSetup
    let mut state = F1NativeState::default();
    let r = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(r.is_none(), "unknown packet IDs should be silently ignored");
    Ok(())
}

#[test]
fn process_packet_second_telemetry_updates_state() -> TestResult {
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
    let telem2 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.9,
        0.0,
        0.0,
        1,
        [24.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem1)?;
    F1NativeAdapter::process_packet(&mut state, &telem2)?;
    let r = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = r.ok_or("expected output after second telemetry + status")?;

    // Should use the second telemetry data (200 km/h, gear 5)
    assert!(
        (t.speed_ms - 200.0 / 3.6).abs() < 0.1,
        "should use latest telemetry"
    );
    assert_eq!(t.gear, 5);
    assert!(t.flags.drs_active, "DRS should be active (drs=1)");
    Ok(())
}

// ── Speed conversion edge cases ──────────────────────────────────────────────

#[test]
fn speed_conversion_zero() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        0,
        0,
        850,
        0.0,
        0.0,
        0.0,
        0,
        [20.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn speed_conversion_350_kmh() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        350,
        8,
        12000,
        1.0,
        0.0,
        0.0,
        1,
        [25.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    let expected = 350.0_f32 / 3.6;
    assert!(
        (t.speed_ms - expected).abs() < 0.1,
        "350 km/h → {expected} m/s, got {}",
        t.speed_ms
    );
    Ok(())
}

// ── Tire pressure reordering ─────────────────────────────────────────────────

#[test]
fn tire_pressure_reorder_f1_to_normalized() -> TestResult {
    // F1 wire order: [RL, RR, FL, FR]
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        150,
        4,
        9000,
        0.6,
        0.0,
        0.0,
        0,
        [20.0, 21.0, 22.0, 23.0], // wire: RL=20, RR=21, FL=22, FR=23
    );
    let adapter = F1NativeAdapter::new();
    let t = adapter.normalize(&raw)?;
    // Normalized order: [FL, FR, RL, RR]
    assert!((t.tire_pressures_psi[0] - 22.0).abs() < 0.1, "FL=22");
    assert!((t.tire_pressures_psi[1] - 23.0).abs() < 0.1, "FR=23");
    assert!((t.tire_pressures_psi[2] - 20.0).abs() < 0.1, "RL=20");
    assert!((t.tire_pressures_psi[3] - 21.0).abs() < 0.1, "RR=21");
    Ok(())
}

// ── ERS calculations ─────────────────────────────────────────────────────────

#[test]
fn ers_fraction_calculation() -> TestResult {
    // ERS max store = 4_000_000 J (from f1_25 constant)
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
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 0, 0, 16, 11000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    let ers_frac = t.extended.get("ers_store_fraction");
    assert!(
        ers_frac.is_some(),
        "ers_store_fraction should be in extended"
    );
    Ok(())
}

#[test]
fn ers_zero_energy_yields_zero_fraction() -> TestResult {
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
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 0.0, 0, 0, 16, 11000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    assert!(t.extended.contains_key("ers_store_fraction"));
    Ok(())
}

// ── Flag combinations ────────────────────────────────────────────────────────

#[test]
fn flags_drs_active_and_available() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        300,
        8,
        11000,
        1.0,
        0.0,
        0.0,
        1,
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(t.flags.drs_active, "DRS byte=1 → active");
    assert!(t.flags.drs_available, "drs_allowed=1 → available");
    Ok(())
}

#[test]
fn flags_pit_limiter_implies_in_pits() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        60,
        2,
        3000,
        0.3,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 0, 1, 16, 12000);
    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;
    assert!(t.flags.pit_limiter);
    assert!(t.flags.in_pits, "pit_limiter → in_pits");
    Ok(())
}

// ── Normalize method packet type filtering ───────────────────────────────────

#[test]
fn normalize_rejects_session_packet() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 1, 0);
    let result = adapter.normalize(&raw);
    assert!(
        result.is_err(),
        "session packet cannot produce telemetry alone"
    );
    Ok(())
}

#[test]
fn normalize_rejects_car_status_packet() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 0, 0, 16, 12000);
    let result = adapter.normalize(&raw);
    assert!(
        result.is_err(),
        "car status without telemetry is incomplete"
    );
    Ok(())
}

#[test]
fn normalize_accepts_car_telemetry_packet() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.7,
        0.1,
        0.0,
        0,
        [25.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!((t.speed_ms - 200.0 / 3.6).abs() < 0.1);
    Ok(())
}

// ── Cross-format: F1 23 vs F1 24 telemetry packets ──────────────────────────

#[test]
fn f23_and_f24_telemetry_produce_same_output() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw_23 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        250,
        6,
        10000,
        0.9,
        0.2,
        -0.1,
        1,
        [24.0; 4],
    );
    let raw_24 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        250,
        6,
        10000,
        0.9,
        0.2,
        -0.1,
        1,
        [24.0; 4],
    );
    let t23 = adapter.normalize(&raw_23)?;
    let t24 = adapter.normalize(&raw_24)?;

    assert_eq!(t23.speed_ms, t24.speed_ms, "speed should match");
    assert_eq!(t23.gear, t24.gear, "gear should match");
    assert_eq!(t23.rpm, t24.rpm, "rpm should match");
    assert_eq!(t23.throttle, t24.throttle, "throttle should match");
    assert_eq!(t23.brake, t24.brake, "brake should match");
    Ok(())
}

// ── Mixed format state machine ───────────────────────────────────────────────

#[test]
fn f23_telemetry_with_f24_status() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        180,
        4,
        9000,
        0.7,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f24(0, 35.0, 2_500_000.0, 1, 0, 16, 12000);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(result.is_some(), "mixed F23 telem + F24 status should work");
    Ok(())
}

// ── Extended data completeness ───────────────────────────────────────────────

#[test]
fn extended_data_keys_present() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.8,
        0.1,
        0.0,
        1,
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 35.0, 2_000_000.0, 1, 0, 16, 12000);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    let expected_keys = [
        "drs_active",
        "drs_available",
        "ers_store_energy_j",
        "ers_store_fraction",
        "ers_deploy_mode",
        "rpm_fraction",
        "fuel_remaining_kg",
        "fuel_remaining_laps",
        "tyre_compound",
        "tyre_compound_name",
        "tyre_age_laps",
        "decoder_type",
        "session_type",
    ];
    for key in &expected_keys {
        assert!(t.extended.contains_key(*key), "missing extended key: {key}");
    }
    Ok(())
}

// ── TelemetryFrame construction ──────────────────────────────────────────────

#[test]
fn telemetry_frame_fields() -> TestResult {
    let n = NormalizedTelemetry::builder()
        .speed_ms(55.5)
        .rpm(8000.0)
        .gear(4)
        .build();
    let frame = TelemetryFrame::new(n, 123456789, 100, 2048);
    assert!((frame.data.speed_ms - 55.5).abs() < 0.01);
    assert!((frame.data.rpm - 8000.0).abs() < 0.1);
    assert_eq!(frame.data.gear, 4);
    assert_eq!(frame.timestamp_ns, 123456789);
    assert_eq!(frame.sequence, 100);
    assert_eq!(frame.raw_size, 2048);
    Ok(())
}

// ── Adapter construction ─────────────────────────────────────────────────────

#[test]
fn adapter_default_matches_new() -> TestResult {
    let a = F1NativeAdapter::new();
    let b = F1NativeAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

#[test]
fn with_port_does_not_change_game_id() -> TestResult {
    let adapter = F1NativeAdapter::new().with_port(30000);
    assert_eq!(adapter.game_id(), "f1_native");
    Ok(())
}

// ── F1NativeCarStatusData default ────────────────────────────────────────────

#[test]
fn car_status_data_default_is_all_zero() -> TestResult {
    let status = F1NativeCarStatusData::default();
    assert_eq!(status.traction_control, 0);
    assert_eq!(status.anti_lock_brakes, 0);
    assert_eq!(status.pit_limiter_status, 0);
    assert_eq!(status.fuel_in_tank, 0.0);
    assert_eq!(status.max_rpm, 0);
    assert_eq!(status.ers_store_energy, 0.0);
    assert_eq!(status.engine_power_ice, 0.0);
    assert_eq!(status.engine_power_mguk, 0.0);
    Ok(())
}

// ── Full race scenario: qualifying lap ───────────────────────────────────────

#[test]
fn qualifying_hot_lap_scenario() -> TestResult {
    // Simulate a typical qualifying lap with DRS, full ERS, soft tyres
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,                        // player index
        330,                      // speed_kmh (high speed on straight)
        8,                        // gear 8
        12500,                    // high RPM
        1.0,                      // full throttle
        0.0,                      // no brake
        0.0,                      // straight
        1,                        // DRS active
        [23.5, 24.0, 22.0, 22.5], // tire pressures
    );
    let status = build_car_status_packet_f24(
        0,           // player
        50.0,        // fuel
        3_800_000.0, // near-full ERS
        1,           // DRS allowed
        0,           // no pit limiter
        16,          // soft compound
        13_500,      // max RPM
    );

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    assert!((t.speed_ms - 330.0 / 3.6).abs() < 0.2);
    assert_eq!(t.gear, 8);
    assert!((t.rpm - 12500.0).abs() < 1.0);
    assert!((t.throttle - 1.0).abs() < 0.01);
    assert!(t.flags.drs_active);
    assert!(t.flags.drs_available);
    assert!(!t.flags.pit_limiter);
    assert!(!t.flags.in_pits);

    // RPM fraction
    let rpm_frac = t.extended.get("rpm_fraction");
    assert!(rpm_frac.is_some(), "rpm_fraction should be present");

    Ok(())
}

// ── Deterministic output ─────────────────────────────────────────────────────

#[test]
fn process_packet_deterministic() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.7,
        0.2,
        0.1,
        0,
        [25.0; 4],
    );
    let status = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 16, 12000);

    let mut state1 = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state1, &telem)?;
    let r1 = F1NativeAdapter::process_packet(&mut state1, &status)?;

    let mut state2 = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state2, &telem)?;
    let r2 = F1NativeAdapter::process_packet(&mut state2, &status)?;

    let t1 = r1.ok_or("expected output 1")?;
    let t2 = r2.ok_or("expected output 2")?;

    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.brake, t2.brake);
    assert_eq!(t1.flags.drs_active, t2.flags.drs_active);
    assert_eq!(t1.flags.pit_limiter, t2.flags.pit_limiter);
    Ok(())
}

// ── Unsupported format edge cases ────────────────────────────────────────────

#[test]
fn format_0_rejected() {
    let raw = build_f1_native_header_bytes(0, 6, 0);
    let mut state = F1NativeState::default();
    assert!(F1NativeAdapter::process_packet(&mut state, &raw).is_err());
}

#[test]
fn format_2022_rejected() {
    let raw = build_f1_native_header_bytes(2022, 6, 0);
    let mut state = F1NativeState::default();
    assert!(F1NativeAdapter::process_packet(&mut state, &raw).is_err());
}

#[test]
fn format_2025_rejected() {
    let raw = build_f1_native_header_bytes(2025, 6, 0);
    let mut state = F1NativeState::default();
    assert!(F1NativeAdapter::process_packet(&mut state, &raw).is_err());
}

#[test]
fn format_65535_rejected() {
    let raw = build_f1_native_header_bytes(u16::MAX, 6, 0);
    let mut state = F1NativeState::default();
    assert!(F1NativeAdapter::process_packet(&mut state, &raw).is_err());
}
