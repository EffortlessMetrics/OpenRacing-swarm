//! Integration tests for the `racing-wheel-telemetry-f1` crate.
//!
//! Covers F1 2023/2024 packet parsing, struct layout verification,
//! normalization, process_packet state machine, and edge cases.

use openracing_telemetry::TelemetryValue;
use openracing_telemetry_adapters::f1_25::{
    CAR_TELEMETRY_ENTRY_SIZE, CarTelemetryData, ERS_MAX_STORE_ENERGY_J,
    MIN_CAR_TELEMETRY_PACKET_SIZE, SessionData, parse_car_telemetry, parse_header,
    parse_session_data,
};
use openracing_telemetry_adapters::f1_native::{
    CAR_STATUS_2023_ENTRY_SIZE, CAR_STATUS_2024_ENTRY_SIZE, F1NativeAdapter, F1NativeCarStatusData,
    F1NativeState, MIN_CAR_STATUS_2023_PACKET_SIZE, MIN_CAR_STATUS_2024_PACKET_SIZE,
    PACKET_FORMAT_2023, PACKET_FORMAT_2024, build_car_status_packet_f23,
    build_car_status_packet_f24, build_car_telemetry_packet_native, build_f1_native_header_bytes,
    normalize, parse_car_status_2023, parse_car_status_2024,
};
use racing_wheel_telemetry_f1::TelemetryAdapter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Constants / struct layout verification ────────────────────────────────────

const HEADER_SIZE: usize = 29;
const NUM_CARS: usize = 22;

#[test]
fn header_size_is_29_bytes() -> TestResult {
    let header = build_f1_native_header_bytes(2023, 6, 0);
    assert_eq!(header.len(), HEADER_SIZE, "header must be exactly 29 bytes");
    Ok(())
}

#[test]
fn car_telemetry_entry_size_is_60() {
    assert_eq!(CAR_TELEMETRY_ENTRY_SIZE, 60);
}

#[test]
fn car_status_2023_entry_size_is_47() {
    assert_eq!(CAR_STATUS_2023_ENTRY_SIZE, 47);
}

#[test]
fn car_status_2024_entry_size_is_55() {
    assert_eq!(CAR_STATUS_2024_ENTRY_SIZE, 55);
}

#[test]
fn min_car_status_2023_packet_size_matches_formula() {
    assert_eq!(
        MIN_CAR_STATUS_2023_PACKET_SIZE,
        HEADER_SIZE + NUM_CARS * CAR_STATUS_2023_ENTRY_SIZE
    );
}

#[test]
fn min_car_status_2024_packet_size_matches_formula() {
    assert_eq!(
        MIN_CAR_STATUS_2024_PACKET_SIZE,
        HEADER_SIZE + NUM_CARS * CAR_STATUS_2024_ENTRY_SIZE
    );
}

#[test]
fn min_car_telemetry_packet_size_includes_trailer() {
    // 29 header + 22*60 car data + 3 trailer
    assert_eq!(
        MIN_CAR_TELEMETRY_PACKET_SIZE,
        HEADER_SIZE + NUM_CARS * 60 + 3
    );
}

#[test]
fn ers_max_store_is_4mj() {
    assert!((ERS_MAX_STORE_ENERGY_J - 4_000_000.0).abs() < 1.0);
}

// ── Header parsing ───────────────────────────────────────────────────────────

#[test]
fn parse_header_format_2023() -> TestResult {
    let raw = build_f1_native_header_bytes(2023, 6, 0);
    let h = parse_header(&raw)?;
    assert_eq!(h.packet_format, 2023);
    assert_eq!(h.packet_id, 6);
    assert_eq!(h.player_car_index, 0);
    Ok(())
}

#[test]
fn parse_header_format_2024() -> TestResult {
    let raw = build_f1_native_header_bytes(2024, 7, 5);
    let h = parse_header(&raw)?;
    assert_eq!(h.packet_format, 2024);
    assert_eq!(h.packet_id, 7);
    assert_eq!(h.player_car_index, 5);
    Ok(())
}

#[test]
fn parse_header_player_index_max_valid() -> TestResult {
    let raw = build_f1_native_header_bytes(2023, 1, 21);
    let h = parse_header(&raw)?;
    assert_eq!(h.player_car_index, 21);
    Ok(())
}

#[test]
fn parse_header_too_short_errors() {
    let result = parse_header(&[0u8; 10]);
    assert!(result.is_err());
}

#[test]
fn parse_header_empty_packet_errors() {
    let result = parse_header(&[]);
    assert!(result.is_err());
}

// ── Car Status F1 23 ─────────────────────────────────────────────────────────

#[test]
fn car_status_2023_parses_fuel_ers_drs() -> TestResult {
    let raw = build_car_status_packet_f23(0, 30.0, 2_500_000.0, 1, 0, 12, 15_000);
    let status = parse_car_status_2023(&raw, 0)?;
    assert!((status.fuel_in_tank - 30.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 2_500_000.0).abs() < 1.0);
    assert_eq!(status.drs_allowed, 1);
    assert_eq!(status.pit_limiter_status, 0);
    assert_eq!(status.actual_tyre_compound, 12);
    assert_eq!(status.max_rpm, 15_000);
    Ok(())
}

#[test]
fn car_status_2023_engine_power_always_zero() -> TestResult {
    let raw = build_car_status_packet_f23(0, 10.0, 1_000_000.0, 0, 0, 13, 12000);
    let status = parse_car_status_2023(&raw, 0)?;
    assert_eq!(status.engine_power_ice, 0.0);
    assert_eq!(status.engine_power_mguk, 0.0);
    Ok(())
}

#[test]
fn car_status_2023_pit_limiter_on() -> TestResult {
    let raw = build_car_status_packet_f23(1, 5.0, 0.0, 0, 1, 14, 10000);
    let status = parse_car_status_2023(&raw, 1)?;
    assert_eq!(status.pit_limiter_status, 1);
    Ok(())
}

#[test]
fn car_status_2023_rejects_truncated_packet() {
    let result = parse_car_status_2023(&[0u8; 100], 0);
    assert!(result.is_err());
}

#[test]
fn car_status_2023_rejects_player_index_22() -> TestResult {
    let raw = build_car_status_packet_f23(0, 10.0, 1_000_000.0, 0, 0, 13, 12000);
    let result = parse_car_status_2023(&raw, 22);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn car_status_2023_rejects_player_index_255() -> TestResult {
    let raw = build_car_status_packet_f23(0, 10.0, 1_000_000.0, 0, 0, 13, 12000);
    let result = parse_car_status_2023(&raw, 255);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn car_status_2023_non_zero_player_index() -> TestResult {
    // Build packet with player at index 5 and parse at index 5
    let raw = build_car_status_packet_f23(5, 42.0, 3_000_000.0, 0, 0, 11, 13000);
    let status = parse_car_status_2023(&raw, 5)?;
    assert!((status.fuel_in_tank - 42.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 3_000_000.0).abs() < 1.0);
    Ok(())
}

#[test]
fn car_status_2023_zero_fuel_zero_ers() -> TestResult {
    let raw = build_car_status_packet_f23(0, 0.0, 0.0, 0, 0, 14, 12000);
    let status = parse_car_status_2023(&raw, 0)?;
    assert_eq!(status.fuel_in_tank, 0.0);
    assert_eq!(status.ers_store_energy, 0.0);
    assert_eq!(status.drs_allowed, 0);
    Ok(())
}

// ── Car Status F1 24 ─────────────────────────────────────────────────────────

#[test]
fn car_status_2024_parses_fuel_ers() -> TestResult {
    let raw = build_car_status_packet_f24(0, 28.5, 3_000_000.0, 1, 0, 13, 14_500);
    let status = parse_car_status_2024(&raw, 0)?;
    assert!((status.fuel_in_tank - 28.5).abs() < 1e-5);
    assert!((status.ers_store_energy - 3_000_000.0).abs() < 1.0);
    assert_eq!(status.drs_allowed, 1);
    assert_eq!(status.actual_tyre_compound, 13);
    assert_eq!(status.max_rpm, 14_500);
    Ok(())
}

#[test]
fn car_status_2024_engine_power_defaults_to_zero() -> TestResult {
    // Builder sets engine_power_ice/mguk to zero; verify parsed correctly
    let raw = build_car_status_packet_f24(0, 10.0, 1_000_000.0, 0, 0, 14, 12000);
    let status = parse_car_status_2024(&raw, 0)?;
    assert_eq!(status.engine_power_ice, 0.0);
    assert_eq!(status.engine_power_mguk, 0.0);
    Ok(())
}

#[test]
fn car_status_2024_rejects_truncated_packet() {
    let result = parse_car_status_2024(&[0u8; 200], 0);
    assert!(result.is_err());
}

#[test]
fn car_status_2024_rejects_player_index_out_of_range() -> TestResult {
    let raw = build_car_status_packet_f24(0, 10.0, 0.0, 0, 0, 14, 12000);
    let result = parse_car_status_2024(&raw, 22);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn car_status_2024_non_zero_player_index() -> TestResult {
    let raw = build_car_status_packet_f24(10, 55.0, 4_000_000.0, 1, 1, 16, 15000);
    let status = parse_car_status_2024(&raw, 10)?;
    assert!((status.fuel_in_tank - 55.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 4_000_000.0).abs() < 1.0);
    assert_eq!(status.pit_limiter_status, 1);
    Ok(())
}

// ── Car Telemetry parsing (shared format between F1 23 and F1 24) ────────────

#[test]
fn car_telemetry_2023_round_trip() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        180,
        5,
        12000,
        0.75,
        0.0,
        -0.1,
        0,
        [23.0, 23.0, 22.5, 22.5],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.speed_kmh, 180);
    assert_eq!(telem.gear, 5);
    assert_eq!(telem.engine_rpm, 12000);
    assert!((telem.throttle - 0.75).abs() < 1e-5);
    assert!((telem.steer - (-0.1)).abs() < 1e-5);
    assert_eq!(telem.brake, 0.0);
    assert_eq!(telem.drs, 0);
    assert!((telem.tyres_pressure[0] - 23.0).abs() < 1e-4);
    assert!((telem.tyres_pressure[2] - 22.5).abs() < 1e-4);
    Ok(())
}

#[test]
fn car_telemetry_2024_drs_active() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        250,
        8,
        14500,
        0.95,
        0.0,
        0.05,
        1,
        [24.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.drs, 1);
    assert_eq!(telem.speed_kmh, 250);
    assert_eq!(telem.gear, 8);
    Ok(())
}

#[test]
fn car_telemetry_reverse_gear() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        5,
        -1,
        2000,
        0.0,
        0.0,
        0.0,
        0,
        [20.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.gear, -1);
    Ok(())
}

#[test]
fn car_telemetry_neutral_gear() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        0,
        0,
        800,
        0.0,
        0.0,
        0.0,
        0,
        [20.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.gear, 0);
    assert_eq!(telem.speed_kmh, 0);
    Ok(())
}

#[test]
fn car_telemetry_full_brake_full_throttle() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        100,
        3,
        8000,
        1.0,
        1.0,
        0.0,
        0,
        [22.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert!((telem.throttle - 1.0).abs() < 1e-5);
    assert!((telem.brake - 1.0).abs() < 1e-5);
    Ok(())
}

#[test]
fn car_telemetry_full_left_steer() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        80,
        2,
        6000,
        0.3,
        0.0,
        -1.0,
        0,
        [21.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert!((telem.steer - (-1.0)).abs() < 1e-5);
    Ok(())
}

#[test]
fn car_telemetry_non_zero_player_index() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        15,
        200,
        6,
        11000,
        0.8,
        0.1,
        0.0,
        0,
        [23.5; 4],
    );
    let telem = parse_car_telemetry(&raw, 15)?;
    assert_eq!(telem.speed_kmh, 200);
    assert_eq!(telem.gear, 6);
    Ok(())
}

#[test]
fn car_telemetry_truncated_packet_errors() {
    let result = parse_car_telemetry(&[0u8; 50], 0);
    assert!(result.is_err());
}

#[test]
fn car_telemetry_rejects_player_index_22() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let result = parse_car_telemetry(&raw, 22);
    assert!(result.is_err());
    Ok(())
}

// ── Session data parsing ─────────────────────────────────────────────────────

fn build_session_packet(
    format: u16,
    track_temp: i8,
    air_temp: i8,
    session_type: u8,
    track_id: i8,
) -> Vec<u8> {
    let mut buf = build_f1_native_header_bytes(format, 1, 0);
    buf.push(0); // weather
    buf.push(track_temp as u8); // trackTemperature
    buf.push(air_temp as u8); // airTemperature
    buf.push(50); // totalLaps
    buf.extend_from_slice(&5326u16.to_le_bytes()); // trackLength
    buf.push(session_type); // sessionType
    buf.push(track_id as u8); // trackId
    buf
}

#[test]
fn session_data_parse_monza_race() -> TestResult {
    let raw = build_session_packet(PACKET_FORMAT_2023, 32, 26, 6, 11);
    let session = parse_session_data(&raw)?;
    assert_eq!(session.track_temperature, 32);
    assert_eq!(session.air_temperature, 26);
    assert_eq!(session.session_type, 6);
    assert_eq!(session.track_id, 11);
    Ok(())
}

#[test]
fn session_data_parse_bahrain_qualifying() -> TestResult {
    let raw = build_session_packet(PACKET_FORMAT_2024, 45, 35, 5, 3);
    let session = parse_session_data(&raw)?;
    assert_eq!(session.track_temperature, 45);
    assert_eq!(session.air_temperature, 35);
    assert_eq!(session.session_type, 5);
    assert_eq!(session.track_id, 3);
    Ok(())
}

#[test]
fn session_data_truncated_packet_errors() {
    // Header only (29 bytes), no session data
    let raw = build_f1_native_header_bytes(2023, 1, 0);
    let result = parse_session_data(&raw);
    assert!(result.is_err());
}

#[test]
fn session_data_negative_temperatures() -> TestResult {
    // Cold track: -5°C track, -10°C air
    let raw = build_session_packet(PACKET_FORMAT_2023, -5, -10, 1, 0);
    let session = parse_session_data(&raw)?;
    assert_eq!(session.track_temperature, -5);
    assert_eq!(session.air_temperature, -10);
    Ok(())
}

// ── process_packet format rejection ──────────────────────────────────────────

#[test]
fn process_packet_rejects_format_2025() {
    let raw = build_f1_native_header_bytes(2025, 6, 0);
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &raw);
    assert!(result.is_err());
}

#[test]
fn process_packet_rejects_format_2022() {
    let raw = build_f1_native_header_bytes(2022, 6, 0);
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &raw);
    assert!(result.is_err());
}

#[test]
fn process_packet_rejects_format_zero() {
    let raw = build_f1_native_header_bytes(0, 6, 0);
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &raw);
    assert!(result.is_err());
}

// ── process_packet state machine ─────────────────────────────────────────────

#[test]
fn process_packet_needs_both_telem_and_status_to_emit_f23() -> TestResult {
    let mut state = F1NativeState::default();

    // Telemetry alone → None
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        180,
        5,
        12000,
        0.7,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let result = F1NativeAdapter::process_packet(&mut state, &telem)?;
    assert!(result.is_none(), "telemetry alone must not emit");

    // Status completes the pair → Some
    let status = build_car_status_packet_f23(0, 25.0, 2_000_000.0, 1, 0, 12, 13000);
    let normalized = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(normalized.is_some(), "status after telemetry must emit");

    let norm = normalized.ok_or("expected Some")?;
    let expected_speed = 180.0_f32 / 3.6;
    assert!((norm.speed_ms - expected_speed).abs() < 0.01);
    assert_eq!(norm.gear, 5);
    assert!((norm.rpm - 12000.0).abs() < 0.1);
    assert!(norm.flags.drs_available);
    assert!(!norm.flags.drs_active);
    assert!(!norm.flags.pit_limiter);
    Ok(())
}

#[test]
fn process_packet_needs_both_telem_and_status_to_emit_f24() -> TestResult {
    let mut state = F1NativeState::default();

    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        300,
        8,
        14000,
        1.0,
        0.0,
        0.0,
        1,
        [24.0; 4],
    );
    F1NativeAdapter::process_packet(&mut state, &telem)?;

    let status = build_car_status_packet_f24(0, 10.0, 3_500_000.0, 1, 0, 14, 15000);
    let normalized = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(normalized.is_some());

    let norm = normalized.ok_or("expected Some")?;
    assert!((norm.speed_ms - 300.0 / 3.6).abs() < 0.01);
    assert_eq!(norm.gear, 8);
    assert!(norm.flags.drs_active);
    assert!(norm.flags.ers_available);
    Ok(())
}

#[test]
fn process_packet_status_first_then_telemetry_emits() -> TestResult {
    let mut state = F1NativeState::default();

    // Status alone → None
    let status = build_car_status_packet_f23(0, 20.0, 1_000_000.0, 0, 0, 13, 12000);
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(result.is_none(), "status alone must not emit");

    // Telemetry completes the pair → Some
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        120,
        4,
        9000,
        0.6,
        0.1,
        0.0,
        0,
        [21.0; 4],
    );
    let result = F1NativeAdapter::process_packet(&mut state, &telem)?;
    assert!(result.is_some(), "telemetry after status must emit");
    Ok(())
}

#[test]
fn process_packet_session_does_not_emit() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_session_packet(PACKET_FORMAT_2023, 32, 26, 6, 11);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    assert_eq!(state.session.track_id, 11);
    Ok(())
}

#[test]
fn process_packet_unknown_packet_id_returns_none() -> TestResult {
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 99, 0);
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

#[test]
fn process_packet_all_valid_ignored_ids_return_none() -> TestResult {
    // Packet IDs 0, 2, 3, 4, 5, 8..13 are not session/telem/status
    let mut state = F1NativeState::default();
    for id in [0u8, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13] {
        let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, id, 0);
        let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
        assert!(result.is_none(), "packet id {} should be ignored", id);
    }
    Ok(())
}

#[test]
fn process_packet_subsequent_updates_overwrite_state() -> TestResult {
    let mut state = F1NativeState::default();

    // First pair: 100 km/h
    let telem1 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let status1 = build_car_status_packet_f23(0, 20.0, 1_000_000.0, 0, 0, 13, 12000);
    F1NativeAdapter::process_packet(&mut state, &telem1)?;
    let norm1 =
        F1NativeAdapter::process_packet(&mut state, &status1)?.ok_or("expected first emission")?;

    // Second telemetry: 200 km/h — should emit immediately with old status
    let telem2 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        6,
        11000,
        0.9,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let norm2 =
        F1NativeAdapter::process_packet(&mut state, &telem2)?.ok_or("expected second emission")?;

    assert!((norm1.speed_ms - 100.0 / 3.6).abs() < 0.01);
    assert!((norm2.speed_ms - 200.0 / 3.6).abs() < 0.01);
    assert_eq!(norm2.gear, 6);
    Ok(())
}

// ── adapter.normalize() single-packet API ────────────────────────────────────

#[test]
fn adapter_normalize_car_telemetry_works() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        144,
        4,
        9500,
        0.5,
        0.2,
        0.0,
        0,
        [21.0; 4],
    );
    let norm = adapter.normalize(&raw)?;
    assert!((norm.speed_ms - 144.0 / 3.6).abs() < 0.01);
    assert_eq!(norm.gear, 4);
    assert!((norm.rpm - 9500.0).abs() < 0.1);
    Ok(())
}

#[test]
fn adapter_normalize_car_status_alone_errors() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_status_packet_f23(0, 20.0, 1_000_000.0, 0, 0, 13, 12000);
    assert!(adapter.normalize(&raw).is_err());
    Ok(())
}

#[test]
fn adapter_normalize_session_packet_errors() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_session_packet(PACKET_FORMAT_2023, 30, 25, 6, 11);
    assert!(adapter.normalize(&raw).is_err());
    Ok(())
}

#[test]
fn adapter_normalize_rejects_unsupported_format() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2025, 6, 0);
    assert!(adapter.normalize(&raw).is_err());
}

#[test]
fn adapter_normalize_rejects_unknown_packet_id() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2023, 42, 0);
    assert!(adapter.normalize(&raw).is_err());
}

#[test]
fn adapter_normalize_includes_decoder_type() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let norm = adapter.normalize(&raw)?;
    assert_eq!(
        norm.extended.get("decoder_type"),
        Some(&TelemetryValue::String("f1_native_udp".to_string()))
    );
    Ok(())
}

// ── game_id ──────────────────────────────────────────────────────────────────

#[test]
fn adapter_game_id() {
    let adapter = F1NativeAdapter::new();
    assert_eq!(adapter.game_id(), "f1_native");
}

#[test]
fn adapter_default_trait() {
    let adapter = F1NativeAdapter::default();
    assert_eq!(adapter.game_id(), "f1_native");
}

// ── Normalization correctness ────────────────────────────────────────────────

#[test]
fn normalize_speed_kmh_to_ms() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 360,
        throttle: 1.0,
        steer: 0.0,
        brake: 0.0,
        gear: 8,
        engine_rpm: 14000,
        drs: 0,
        brakes_temperature: [500, 500, 500, 500],
        tyres_surface_temperature: [90, 90, 90, 90],
        tyres_inner_temperature: [100, 100, 100, 100],
        engine_temperature: 110,
        tyres_pressure: [24.0; 4],
    };
    let status = F1NativeCarStatusData {
        max_rpm: 15000,
        ers_store_energy: 2_000_000.0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!((norm.speed_ms - 100.0).abs() < 0.01, "360 km/h = 100 m/s");
    assert_eq!(norm.gear, 8);
    assert!((norm.rpm - 14000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn normalize_zero_speed() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 0,
        throttle: 0.0,
        steer: 0.0,
        brake: 0.0,
        gear: 0,
        engine_rpm: 800,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 90,
        tyres_pressure: [22.0; 4],
    };
    let status = F1NativeCarStatusData::default();
    let norm = normalize(&telem, &status, &SessionData::default());
    assert_eq!(norm.speed_ms, 0.0);
    assert_eq!(norm.gear, 0);
    Ok(())
}

#[test]
fn normalize_drs_flags() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 300,
        throttle: 1.0,
        steer: 0.0,
        brake: 0.0,
        gear: 8,
        engine_rpm: 14000,
        drs: 1,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        drs_allowed: 1,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(norm.flags.drs_active);
    assert!(norm.flags.drs_available);
    Ok(())
}

#[test]
fn normalize_drs_not_active_not_available() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 200,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 5,
        engine_rpm: 10000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        drs_allowed: 0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(!norm.flags.drs_active);
    assert!(!norm.flags.drs_available);
    Ok(())
}

#[test]
fn normalize_pit_limiter_flag() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 60,
        throttle: 0.3,
        steer: 0.0,
        brake: 0.0,
        gear: 2,
        engine_rpm: 5000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        pit_limiter_status: 1,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(norm.flags.pit_limiter);
    assert!(norm.flags.in_pits);
    Ok(())
}

#[test]
fn normalize_ers_fraction_clamped_to_1() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 0,
        throttle: 0.0,
        steer: 0.0,
        brake: 0.0,
        gear: 0,
        engine_rpm: 0,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    // Overflow: 2x max ERS
    let status = F1NativeCarStatusData {
        ers_store_energy: ERS_MAX_STORE_ENERGY_J * 2.0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    if let Some(TelemetryValue::Float(frac)) = norm.extended.get("ers_store_fraction") {
        assert!(*frac <= 1.0, "fraction must not exceed 1.0");
        assert!(*frac >= 0.0, "fraction must not be negative");
    } else {
        return Err("ers_store_fraction not found".into());
    }
    Ok(())
}

#[test]
fn normalize_ers_available_when_energy_positive() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 100,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 3,
        engine_rpm: 8000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        ers_store_energy: 1_000_000.0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(norm.flags.ers_available);
    Ok(())
}

#[test]
fn normalize_ers_not_available_when_zero() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 100,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 3,
        engine_rpm: 8000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        ers_store_energy: 0.0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(!norm.flags.ers_available);
    Ok(())
}

#[test]
fn normalize_rpm_fraction_calculation() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 200,
        throttle: 1.0,
        steer: 0.0,
        brake: 0.0,
        gear: 5,
        engine_rpm: 10000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        max_rpm: 15000,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    let expected_frac = 10000.0_f32 / 15000.0;
    if let Some(TelemetryValue::Float(frac)) = norm.extended.get("rpm_fraction") {
        assert!((*frac - expected_frac).abs() < 1e-4);
    } else {
        return Err("rpm_fraction not found".into());
    }
    Ok(())
}

#[test]
fn normalize_rpm_fraction_zero_max_rpm() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 100,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 3,
        engine_rpm: 8000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        max_rpm: 0, // edge case: zero max RPM
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    if let Some(TelemetryValue::Float(frac)) = norm.extended.get("rpm_fraction") {
        assert_eq!(*frac, 0.0, "rpm_fraction must be 0 when max_rpm is 0");
    } else {
        return Err("rpm_fraction not found".into());
    }
    Ok(())
}

#[test]
fn normalize_extended_fields_present() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 180,
        throttle: 0.8,
        steer: -0.3,
        brake: 0.1,
        gear: 5,
        engine_rpm: 11000,
        drs: 0,
        brakes_temperature: [400, 410, 420, 430],
        tyres_surface_temperature: [85, 86, 87, 88],
        tyres_inner_temperature: [95, 96, 97, 98],
        engine_temperature: 105,
        tyres_pressure: [23.0, 23.5, 22.0, 22.5],
    };
    let status = F1NativeCarStatusData {
        fuel_in_tank: 25.0,
        fuel_remaining_laps: 8.5,
        actual_tyre_compound: 12,
        tyre_age_laps: 15,
        ers_store_energy: 2_000_000.0,
        ers_deploy_mode: 2,
        ers_harvested_mguk: 500_000.0,
        ers_harvested_mguh: 300_000.0,
        ers_deployed: 1_000_000.0,
        engine_power_ice: 550_000.0,
        engine_power_mguk: 120_000.0,
        ..F1NativeCarStatusData::default()
    };
    let session = SessionData {
        track_id: 11,
        session_type: 6,
        track_temperature: 32,
        air_temperature: 26,
    };
    let norm = normalize(&telem, &status, &session);

    // Throttle/brake/steer (now typed fields)
    assert!((norm.throttle - 0.8).abs() < f32::EPSILON);
    assert!((norm.brake - 0.1).abs() < f32::EPSILON);
    assert!((norm.steering_angle - (-0.3)).abs() < f32::EPSILON);

    // Engine temp (now typed)
    assert!((norm.engine_temp_c - 105.0).abs() < f32::EPSILON);

    // Max RPM (now typed, from status.max_rpm which defaults to 0)
    assert!((norm.max_rpm).abs() < f32::EPSILON);

    // Tire pressures (now typed [FL, FR, RL, RR]; F1 data [RL, RR, FL, FR] reordered)
    // Input: [23.0, 23.5, 22.0, 22.5] => reordered: [22.0, 22.5, 23.0, 23.5]
    assert!((norm.tire_pressures_psi[0] - 22.0).abs() < f32::EPSILON); // FL
    assert!((norm.tire_pressures_psi[1] - 22.5).abs() < f32::EPSILON); // FR
    assert!((norm.tire_pressures_psi[2] - 23.0).abs() < f32::EPSILON); // RL
    assert!((norm.tire_pressures_psi[3] - 23.5).abs() < f32::EPSILON); // RR

    // Tire surface temps (now typed [FL, FR, RL, RR]; F1 data [RL, RR, FL, FR] reordered)
    // Input: [85, 86, 87, 88] => reordered: [87, 88, 85, 86]
    assert_eq!(norm.tire_temps_c[0], 87); // FL
    assert_eq!(norm.tire_temps_c[1], 88); // FR
    assert_eq!(norm.tire_temps_c[2], 85); // RL
    assert_eq!(norm.tire_temps_c[3], 86); // RR

    // Fuel
    assert_eq!(
        norm.extended.get("fuel_remaining_kg"),
        Some(&TelemetryValue::Float(25.0))
    );
    assert_eq!(
        norm.extended.get("fuel_remaining_laps"),
        Some(&TelemetryValue::Float(8.5))
    );

    // Tyres
    assert_eq!(
        norm.extended.get("tyre_compound"),
        Some(&TelemetryValue::Integer(12))
    );
    assert_eq!(
        norm.extended.get("tyre_compound_name"),
        Some(&TelemetryValue::String("Soft".to_string()))
    );
    assert_eq!(
        norm.extended.get("tyre_age_laps"),
        Some(&TelemetryValue::Integer(15))
    );

    // Tyre pressures (now typed; see above)
    // Individual pressure extended keys removed — validated above via tire_pressures_psi

    // Engine power
    assert_eq!(
        norm.extended.get("engine_power_ice_w"),
        Some(&TelemetryValue::Float(550_000.0))
    );
    assert_eq!(
        norm.extended.get("engine_power_mguk_w"),
        Some(&TelemetryValue::Float(120_000.0))
    );

    // ERS
    assert_eq!(
        norm.extended.get("ers_store_energy_j"),
        Some(&TelemetryValue::Float(2_000_000.0))
    );
    assert_eq!(
        norm.extended.get("ers_deploy_mode"),
        Some(&TelemetryValue::Integer(2))
    );
    assert_eq!(
        norm.extended.get("ers_harvested_mguk_j"),
        Some(&TelemetryValue::Float(500_000.0))
    );
    assert_eq!(
        norm.extended.get("ers_deployed_j"),
        Some(&TelemetryValue::Float(1_000_000.0))
    );

    // Engine temperature (now typed; see above)
    // engine_temperature_c extended key removed — validated above via engine_temp_c

    // Session
    assert_eq!(
        norm.extended.get("session_type"),
        Some(&TelemetryValue::Integer(6))
    );
    assert_eq!(
        norm.extended.get("track_temperature_c"),
        Some(&TelemetryValue::Integer(32))
    );
    assert_eq!(
        norm.extended.get("air_temperature_c"),
        Some(&TelemetryValue::Integer(26))
    );

    // Brake/tyre temps (inner temps remain in extended; surface temps now typed above)
    assert_eq!(
        norm.extended.get("brake_temp_rl_c"),
        Some(&TelemetryValue::Integer(400))
    );
    // tyre surface temps validated above via tire_temps_c typed field
    assert_eq!(
        norm.extended.get("tyre_inner_temp_rr_c"),
        Some(&TelemetryValue::Integer(96))
    );

    // Track name
    assert_eq!(norm.track_id, Some("Monza".to_string()));
    Ok(())
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn empty_data_errors() {
    let adapter = F1NativeAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
}

#[test]
fn single_byte_errors() {
    let adapter = F1NativeAdapter::new();
    assert!(adapter.normalize(&[0x42]).is_err());
}

#[test]
fn header_only_telemetry_packet_errors() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2023, 6, 0);
    // Has valid header but no car data
    assert!(adapter.normalize(&raw).is_err());
}

#[test]
fn header_only_status_packet_errors() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2024, 7, 0);
    assert!(adapter.normalize(&raw).is_err());
}

#[test]
fn process_packet_with_zero_length_data() {
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &[]);
    assert!(result.is_err());
}

#[test]
fn process_packet_with_partial_header() {
    let mut state = F1NativeState::default();
    let result = F1NativeAdapter::process_packet(&mut state, &[0xE7, 0x07]); // 2023 in LE
    assert!(result.is_err());
}

#[test]
fn car_status_2023_exactly_minimum_size_works() -> TestResult {
    // Build the exact minimum-size packet
    let raw = build_car_status_packet_f23(0, 10.0, 1_000_000.0, 0, 0, 13, 12000);
    assert!(raw.len() >= MIN_CAR_STATUS_2023_PACKET_SIZE);
    let status = parse_car_status_2023(&raw, 0)?;
    assert!((status.fuel_in_tank - 10.0).abs() < 1e-5);
    Ok(())
}

#[test]
fn car_status_2024_exactly_minimum_size_works() -> TestResult {
    let raw = build_car_status_packet_f24(0, 15.0, 2_000_000.0, 1, 0, 14, 13000);
    assert!(raw.len() >= MIN_CAR_STATUS_2024_PACKET_SIZE);
    let status = parse_car_status_2024(&raw, 0)?;
    assert!((status.fuel_in_tank - 15.0).abs() < 1e-5);
    Ok(())
}

#[test]
fn car_status_2023_one_byte_short_errors() {
    let short = vec![0u8; MIN_CAR_STATUS_2023_PACKET_SIZE - 1];
    let result = parse_car_status_2023(&short, 0);
    assert!(result.is_err());
}

#[test]
fn car_status_2024_one_byte_short_errors() {
    let short = vec![0u8; MIN_CAR_STATUS_2024_PACKET_SIZE - 1];
    let result = parse_car_status_2024(&short, 0);
    assert!(result.is_err());
}

#[test]
fn car_telemetry_one_byte_short_errors() {
    let short = vec![0u8; MIN_CAR_TELEMETRY_PACKET_SIZE - 1];
    let result = parse_car_telemetry(&short, 0);
    assert!(result.is_err());
}

// ── Last player index (21) ───────────────────────────────────────────────────

#[test]
fn car_status_2023_last_player_index() -> TestResult {
    let raw = build_car_status_packet_f23(21, 18.0, 1_500_000.0, 1, 0, 12, 13500);
    let status = parse_car_status_2023(&raw, 21)?;
    assert!((status.fuel_in_tank - 18.0).abs() < 1e-5);
    assert!((status.ers_store_energy - 1_500_000.0).abs() < 1.0);
    Ok(())
}

#[test]
fn car_status_2024_last_player_index() -> TestResult {
    let raw = build_car_status_packet_f24(21, 22.0, 3_500_000.0, 0, 1, 15, 14000);
    let status = parse_car_status_2024(&raw, 21)?;
    assert!((status.fuel_in_tank - 22.0).abs() < 1e-5);
    assert_eq!(status.pit_limiter_status, 1);
    Ok(())
}

#[test]
fn car_telemetry_last_player_index() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        21,
        280,
        7,
        13500,
        0.9,
        0.0,
        0.1,
        1,
        [24.5; 4],
    );
    let telem = parse_car_telemetry(&raw, 21)?;
    assert_eq!(telem.speed_kmh, 280);
    assert_eq!(telem.gear, 7);
    Ok(())
}

// ── Cross-format consistency ─────────────────────────────────────────────────

#[test]
fn telemetry_format_2023_and_2024_identical_layout() -> TestResult {
    // Car Telemetry packets should produce the same data for both formats
    let params = (
        0u8,
        200u16,
        6i8,
        12000u16,
        0.8f32,
        0.1f32,
        -0.2f32,
        1u8,
        [23.0f32; 4],
    );

    let raw_23 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        params.0,
        params.1,
        params.2,
        params.3,
        params.4,
        params.5,
        params.6,
        params.7,
        params.8,
    );
    let raw_24 = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        params.0,
        params.1,
        params.2,
        params.3,
        params.4,
        params.5,
        params.6,
        params.7,
        params.8,
    );

    let telem_23 = parse_car_telemetry(&raw_23, 0)?;
    let telem_24 = parse_car_telemetry(&raw_24, 0)?;

    assert_eq!(telem_23.speed_kmh, telem_24.speed_kmh);
    assert_eq!(telem_23.gear, telem_24.gear);
    assert_eq!(telem_23.engine_rpm, telem_24.engine_rpm);
    assert!((telem_23.throttle - telem_24.throttle).abs() < 1e-6);
    assert!((telem_23.brake - telem_24.brake).abs() < 1e-6);
    assert!((telem_23.steer - telem_24.steer).abs() < 1e-6);
    assert_eq!(telem_23.drs, telem_24.drs);
    Ok(())
}

// ── F1NativeCarStatusData default ────────────────────────────────────────────

#[test]
fn car_status_data_default_is_zeroed() {
    let d = F1NativeCarStatusData::default();
    assert_eq!(d.traction_control, 0);
    assert_eq!(d.anti_lock_brakes, 0);
    assert_eq!(d.pit_limiter_status, 0);
    assert_eq!(d.fuel_in_tank, 0.0);
    assert_eq!(d.fuel_remaining_laps, 0.0);
    assert_eq!(d.max_rpm, 0);
    assert_eq!(d.drs_allowed, 0);
    assert_eq!(d.actual_tyre_compound, 0);
    assert_eq!(d.tyre_age_laps, 0);
    assert_eq!(d.engine_power_ice, 0.0);
    assert_eq!(d.engine_power_mguk, 0.0);
    assert_eq!(d.ers_store_energy, 0.0);
    assert_eq!(d.ers_deploy_mode, 0);
    assert_eq!(d.ers_harvested_mguk, 0.0);
    assert_eq!(d.ers_harvested_mguh, 0.0);
    assert_eq!(d.ers_deployed, 0.0);
}

// ── F1NativeState default ────────────────────────────────────────────────────

#[test]
fn f1_native_state_default_has_no_data() {
    let state = F1NativeState::default();
    assert!(state.latest_telemetry.is_none());
    assert!(state.latest_status.is_none());
    assert_eq!(state.session.track_id, 0);
    assert_eq!(state.session.session_type, 0);
}

// ── Tyre compound names ──────────────────────────────────────────────────────

#[test]
fn normalize_known_tyre_compound_names() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 100,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 3,
        engine_rpm: 8000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };

    let compounds = [
        (12u8, "Soft"),
        (13, "Medium"),
        (14, "Hard"),
        (7, "Intermediate"),
        (16, "C5"),
        (17, "C4"),
        (18, "C3"),
        (19, "C2"),
        (20, "C1"),
    ];

    for (compound, expected_name) in compounds {
        let status = F1NativeCarStatusData {
            actual_tyre_compound: compound,
            ..F1NativeCarStatusData::default()
        };
        let norm = normalize(&telem, &status, &SessionData::default());
        assert_eq!(
            norm.extended.get("tyre_compound_name"),
            Some(&TelemetryValue::String(expected_name.to_string())),
            "compound {} should be named '{}'",
            compound,
            expected_name
        );
    }
    Ok(())
}

#[test]
fn normalize_unknown_tyre_compound_returns_unknown() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 100,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 3,
        engine_rpm: 8000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        actual_tyre_compound: 255, // invalid
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert_eq!(
        norm.extended.get("tyre_compound_name"),
        Some(&TelemetryValue::String("Unknown".to_string()))
    );
    Ok(())
}

// ── Full round-trip: build → process_packet → check all normalized fields ────

#[test]
fn full_round_trip_f23_telem_and_status() -> TestResult {
    let mut state = F1NativeState::default();

    // Session packet
    let session_raw = build_session_packet(PACKET_FORMAT_2023, 35, 28, 10, 5);
    F1NativeAdapter::process_packet(&mut state, &session_raw)?;

    // Car telemetry
    let telem_raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        3,
        220,
        6,
        12500,
        0.85,
        0.05,
        -0.15,
        0,
        [23.0, 23.5, 22.0, 22.5],
    );
    F1NativeAdapter::process_packet(&mut state, &telem_raw)?;

    // Car status
    let status_raw = build_car_status_packet_f23(3, 35.0, 2_800_000.0, 1, 0, 13, 14000);
    let norm =
        F1NativeAdapter::process_packet(&mut state, &status_raw)?.ok_or("expected emission")?;

    // Verify speed conversion
    assert!((norm.speed_ms - 220.0 / 3.6).abs() < 0.01);
    assert_eq!(norm.gear, 6);
    assert!((norm.rpm - 12500.0).abs() < 0.1);
    assert!(norm.flags.drs_available);
    assert!(!norm.flags.drs_active);
    assert!(!norm.flags.pit_limiter);

    // Track from session
    assert_eq!(norm.track_id, Some("Monaco".to_string()));

    // Session temps
    assert_eq!(
        norm.extended.get("track_temperature_c"),
        Some(&TelemetryValue::Integer(35))
    );
    assert_eq!(
        norm.extended.get("air_temperature_c"),
        Some(&TelemetryValue::Integer(28))
    );
    Ok(())
}

#[test]
fn full_round_trip_f24_telem_and_status() -> TestResult {
    let mut state = F1NativeState::default();

    let session_raw = build_session_packet(PACKET_FORMAT_2024, 40, 30, 6, 30);
    F1NativeAdapter::process_packet(&mut state, &session_raw)?;

    let telem_raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        310,
        8,
        14500,
        1.0,
        0.0,
        0.0,
        1,
        [25.0; 4],
    );
    F1NativeAdapter::process_packet(&mut state, &telem_raw)?;

    let status_raw = build_car_status_packet_f24(0, 8.0, 3_900_000.0, 1, 0, 16, 15000);
    let norm =
        F1NativeAdapter::process_packet(&mut state, &status_raw)?.ok_or("expected emission")?;

    assert!((norm.speed_ms - 310.0 / 3.6).abs() < 0.01);
    assert_eq!(norm.gear, 8);
    assert!(norm.flags.drs_active);
    assert!(norm.flags.drs_available);
    assert!(norm.flags.ers_available);
    assert_eq!(norm.track_id, Some("Miami".to_string()));
    assert_eq!(
        norm.extended.get("tyre_compound_name"),
        Some(&TelemetryValue::String("C5".to_string()))
    );
    Ok(())
}

// ── Field extraction accuracy: hand-crafted byte patterns ────────────────────

/// Verify header fields are at the exact byte offsets specified by the protocol.
#[test]
fn header_field_offsets_verified_by_hand() -> TestResult {
    let mut raw = vec![0u8; 29];
    // packetFormat at bytes 0-1 (little-endian)
    raw[0] = 0xE7; // 2023 = 0x07E7
    raw[1] = 0x07;
    // gameYear(2), major(3), minor(4), packetVersion(5)
    raw[2] = 23;
    raw[3] = 1;
    raw[4] = 0;
    raw[5] = 1;
    // packetId at byte 6
    raw[6] = 6;
    // sessionUID at bytes 7-14
    // sessionTime at bytes 15-18
    // frameIdentifier at bytes 19-22
    // overallFrameIdentifier at bytes 23-26
    // playerCarIndex at byte 27
    raw[27] = 19;
    // secondaryPlayerCarIndex at byte 28
    raw[28] = 255;

    let header = parse_header(&raw)?;
    assert_eq!(header.packet_format, 2023);
    assert_eq!(header.packet_id, 6);
    assert_eq!(header.player_car_index, 19);
    Ok(())
}

/// Verify car telemetry entry offsets by writing known values at exact positions.
#[test]
fn car_telemetry_field_offsets_verified_by_hand() -> TestResult {
    let mut raw = vec![0u8; MIN_CAR_TELEMETRY_PACKET_SIZE];
    // Write header: format=2024, packetId=6, playerIndex=0
    raw[0..2].copy_from_slice(&2024u16.to_le_bytes());
    raw[6] = 6;
    raw[27] = 0;

    let car_base = 29; // HEADER_SIZE
    // speed at car_base+0..2
    raw[car_base..car_base + 2].copy_from_slice(&275u16.to_le_bytes());
    // throttle at car_base+2..6
    raw[car_base + 2..car_base + 6].copy_from_slice(&0.77f32.to_le_bytes());
    // steer at car_base+6..10
    raw[car_base + 6..car_base + 10].copy_from_slice(&(-0.33f32).to_le_bytes());
    // brake at car_base+10..14
    raw[car_base + 10..car_base + 14].copy_from_slice(&0.15f32.to_le_bytes());
    // clutch at car_base+14
    raw[car_base + 14] = 0;
    // gear at car_base+15 (i8)
    raw[car_base + 15] = 6u8;
    // engine_rpm at car_base+16..18
    raw[car_base + 16..car_base + 18].copy_from_slice(&11234u16.to_le_bytes());
    // drs at car_base+18
    raw[car_base + 18] = 1;
    // brakes_temperature at car_base+22..30 (4x u16)
    raw[car_base + 22..car_base + 24].copy_from_slice(&501u16.to_le_bytes());
    raw[car_base + 24..car_base + 26].copy_from_slice(&502u16.to_le_bytes());
    raw[car_base + 26..car_base + 28].copy_from_slice(&503u16.to_le_bytes());
    raw[car_base + 28..car_base + 30].copy_from_slice(&504u16.to_le_bytes());
    // tyres_surface_temperature at car_base+30..34
    raw[car_base + 30] = 91;
    raw[car_base + 31] = 92;
    raw[car_base + 32] = 93;
    raw[car_base + 33] = 94;
    // tyres_inner_temperature at car_base+34..38
    raw[car_base + 34] = 101;
    raw[car_base + 35] = 102;
    raw[car_base + 36] = 103;
    raw[car_base + 37] = 104;
    // engine_temperature at car_base+38..40
    raw[car_base + 38..car_base + 40].copy_from_slice(&109u16.to_le_bytes());
    // tyres_pressure at car_base+40..56 (4x f32)
    raw[car_base + 40..car_base + 44].copy_from_slice(&23.1f32.to_le_bytes());
    raw[car_base + 44..car_base + 48].copy_from_slice(&23.2f32.to_le_bytes());
    raw[car_base + 48..car_base + 52].copy_from_slice(&22.1f32.to_le_bytes());
    raw[car_base + 52..car_base + 56].copy_from_slice(&22.2f32.to_le_bytes());

    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.speed_kmh, 275);
    assert!((telem.throttle - 0.77).abs() < 1e-5);
    assert!((telem.steer - (-0.33)).abs() < 1e-5);
    assert!((telem.brake - 0.15).abs() < 1e-5);
    assert_eq!(telem.gear, 6);
    assert_eq!(telem.engine_rpm, 11234);
    assert_eq!(telem.drs, 1);
    assert_eq!(telem.brakes_temperature, [501, 502, 503, 504]);
    assert_eq!(telem.tyres_surface_temperature, [91, 92, 93, 94]);
    assert_eq!(telem.tyres_inner_temperature, [101, 102, 103, 104]);
    assert_eq!(telem.engine_temperature, 109);
    assert!((telem.tyres_pressure[0] - 23.1).abs() < 1e-4);
    assert!((telem.tyres_pressure[1] - 23.2).abs() < 1e-4);
    assert!((telem.tyres_pressure[2] - 22.1).abs() < 1e-4);
    assert!((telem.tyres_pressure[3] - 22.2).abs() < 1e-4);
    Ok(())
}

/// Verify F1 23 car status entry field offsets with hand-crafted bytes.
#[test]
fn car_status_2023_field_offsets_verified_by_hand() -> TestResult {
    let mut raw = vec![0u8; MIN_CAR_STATUS_2023_PACKET_SIZE];
    // Header: format=2023, packetId=7, playerIndex=0
    raw[0..2].copy_from_slice(&2023u16.to_le_bytes());
    raw[6] = 7;
    raw[27] = 0;

    let base = 29; // car 0 starts at HEADER_SIZE
    raw[base] = 2; // tractionControl
    raw[base + 1] = 1; // antiLockBrakes
    raw[base + 4] = 1; // pitLimiterStatus
    raw[base + 5..base + 9].copy_from_slice(&42.5f32.to_le_bytes()); // fuelInTank
    raw[base + 13..base + 17].copy_from_slice(&11.2f32.to_le_bytes()); // fuelRemainingLaps
    raw[base + 17..base + 19].copy_from_slice(&13500u16.to_le_bytes()); // maxRPM
    raw[base + 22] = 1; // drsAllowed
    raw[base + 25] = 14; // actualTyreCompound (Hard)
    raw[base + 27] = 5; // tyreAgeLaps
    raw[base + 29..base + 33].copy_from_slice(&1_500_000.0f32.to_le_bytes()); // ersStoreEnergy
    raw[base + 33] = 2; // ersDeployMode
    raw[base + 34..base + 38].copy_from_slice(&300_000.0f32.to_le_bytes()); // ersHarvestedMGUK
    raw[base + 38..base + 42].copy_from_slice(&200_000.0f32.to_le_bytes()); // ersHarvestedMGUH
    raw[base + 42..base + 46].copy_from_slice(&800_000.0f32.to_le_bytes()); // ersDeployed

    let status = parse_car_status_2023(&raw, 0)?;
    assert_eq!(status.traction_control, 2);
    assert_eq!(status.anti_lock_brakes, 1);
    assert_eq!(status.pit_limiter_status, 1);
    assert!((status.fuel_in_tank - 42.5).abs() < 1e-5);
    assert!((status.fuel_remaining_laps - 11.2).abs() < 1e-5);
    assert_eq!(status.max_rpm, 13500);
    assert_eq!(status.drs_allowed, 1);
    assert_eq!(status.actual_tyre_compound, 14);
    assert_eq!(status.tyre_age_laps, 5);
    assert_eq!(status.engine_power_ice, 0.0); // not present in F1 23
    assert_eq!(status.engine_power_mguk, 0.0);
    assert!((status.ers_store_energy - 1_500_000.0).abs() < 1.0);
    assert_eq!(status.ers_deploy_mode, 2);
    assert!((status.ers_harvested_mguk - 300_000.0).abs() < 1.0);
    assert!((status.ers_harvested_mguh - 200_000.0).abs() < 1.0);
    assert!((status.ers_deployed - 800_000.0).abs() < 1.0);
    Ok(())
}

/// Verify F1 24 car status entry field offsets, including the new engine power fields.
#[test]
fn car_status_2024_field_offsets_verified_by_hand() -> TestResult {
    let mut raw = vec![0u8; MIN_CAR_STATUS_2024_PACKET_SIZE];
    raw[0..2].copy_from_slice(&2024u16.to_le_bytes());
    raw[6] = 7;
    raw[27] = 0;

    let base = 29;
    raw[base] = 1; // tractionControl
    raw[base + 1] = 0; // antiLockBrakes
    raw[base + 4] = 0; // pitLimiterStatus
    raw[base + 5..base + 9].copy_from_slice(&38.0f32.to_le_bytes()); // fuelInTank
    raw[base + 13..base + 17].copy_from_slice(&15.5f32.to_le_bytes()); // fuelRemainingLaps
    raw[base + 17..base + 19].copy_from_slice(&14000u16.to_le_bytes()); // maxRPM
    raw[base + 22] = 0; // drsAllowed
    raw[base + 25] = 16; // actualTyreCompound (C5)
    raw[base + 27] = 12; // tyreAgeLaps
    // F1 24 new fields:
    raw[base + 29..base + 33].copy_from_slice(&560_000.0f32.to_le_bytes()); // enginePowerICE
    raw[base + 33..base + 37].copy_from_slice(&120_000.0f32.to_le_bytes()); // enginePowerMGUK
    raw[base + 37..base + 41].copy_from_slice(&3_200_000.0f32.to_le_bytes()); // ersStoreEnergy
    raw[base + 41] = 3; // ersDeployMode
    raw[base + 42..base + 46].copy_from_slice(&700_000.0f32.to_le_bytes()); // ersHarvestedMGUK
    raw[base + 46..base + 50].copy_from_slice(&450_000.0f32.to_le_bytes()); // ersHarvestedMGUH
    raw[base + 50..base + 54].copy_from_slice(&1_100_000.0f32.to_le_bytes()); // ersDeployed

    let status = parse_car_status_2024(&raw, 0)?;
    assert_eq!(status.traction_control, 1);
    assert_eq!(status.anti_lock_brakes, 0);
    assert!((status.fuel_in_tank - 38.0).abs() < 1e-5);
    assert!((status.fuel_remaining_laps - 15.5).abs() < 1e-5);
    assert_eq!(status.max_rpm, 14000);
    assert_eq!(status.actual_tyre_compound, 16);
    assert_eq!(status.tyre_age_laps, 12);
    assert!((status.engine_power_ice - 560_000.0).abs() < 1.0);
    assert!((status.engine_power_mguk - 120_000.0).abs() < 1.0);
    assert!((status.ers_store_energy - 3_200_000.0).abs() < 1.0);
    assert_eq!(status.ers_deploy_mode, 3);
    assert!((status.ers_harvested_mguk - 700_000.0).abs() < 1.0);
    assert!((status.ers_harvested_mguh - 450_000.0).abs() < 1.0);
    assert!((status.ers_deployed - 1_100_000.0).abs() < 1.0);
    Ok(())
}

/// Session data field offsets verified by hand-crafted bytes.
#[test]
fn session_data_field_offsets_verified_by_hand() -> TestResult {
    let mut raw = vec![0u8; 37]; // header(29) + 8 session fields
    raw[0..2].copy_from_slice(&2024u16.to_le_bytes());
    raw[6] = 1; // session packet ID
    raw[27] = 0;

    // Session data starts at offset 29
    raw[29] = 0; // weather
    raw[30] = 42u8; // trackTemperature (i8=42)
    raw[31] = 30u8; // airTemperature (i8=30)
    raw[32] = 57; // totalLaps
    raw[33..35].copy_from_slice(&5891u16.to_le_bytes()); // trackLength
    raw[35] = 10; // sessionType (race)
    raw[36] = 11; // trackId (Monza)

    let session = parse_session_data(&raw)?;
    assert_eq!(session.track_temperature, 42);
    assert_eq!(session.air_temperature, 30);
    assert_eq!(session.session_type, 10);
    assert_eq!(session.track_id, 11);
    Ok(())
}

// ── Edge cases: truncated, wrong IDs, version mismatches ─────────────────────

/// Packet with NaN float values should be sanitized to 0.0 by ByteReader.
#[test]
fn car_telemetry_nan_float_sanitized() -> TestResult {
    let mut raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    // Overwrite throttle (car_base+2..6) with NaN
    let car_base = 29;
    let nan_bytes = f32::NAN.to_le_bytes();
    raw[car_base + 2..car_base + 6].copy_from_slice(&nan_bytes);

    let telem = parse_car_telemetry(&raw, 0)?;
    // ByteReader::f32_le returns 0.0 for non-finite values
    assert_eq!(telem.throttle, 0.0);
    Ok(())
}

/// Packet with Infinity float values should be sanitized to 0.0.
#[test]
fn car_telemetry_infinity_float_sanitized() -> TestResult {
    let mut raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let car_base = 29;
    let inf_bytes = f32::INFINITY.to_le_bytes();
    raw[car_base + 10..car_base + 14].copy_from_slice(&inf_bytes); // brake field

    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.brake, 0.0);
    Ok(())
}

/// Negative infinity is also sanitized.
#[test]
fn car_telemetry_neg_infinity_float_sanitized() -> TestResult {
    let mut raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        100,
        3,
        8000,
        0.5,
        0.0,
        0.0,
        0,
        [22.0; 4],
    );
    let car_base = 29;
    let neg_inf_bytes = f32::NEG_INFINITY.to_le_bytes();
    raw[car_base + 6..car_base + 10].copy_from_slice(&neg_inf_bytes); // steer field

    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.steer, 0.0);
    Ok(())
}

/// Oversized packets (extra trailing bytes) should still parse correctly.
#[test]
fn car_telemetry_oversized_packet_parses() -> TestResult {
    let mut raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        200,
        6,
        11000,
        0.8,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    // Append 100 extra bytes
    raw.extend_from_slice(&[0xFFu8; 100]);
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.speed_kmh, 200);
    assert_eq!(telem.gear, 6);
    Ok(())
}

/// Oversized status packets parse correctly.
#[test]
fn car_status_2024_oversized_packet_parses() -> TestResult {
    let mut raw = build_car_status_packet_f24(0, 25.0, 2_000_000.0, 1, 0, 13, 13500);
    raw.extend_from_slice(&[0xFFu8; 100]);
    let status = parse_car_status_2024(&raw, 0)?;
    assert!((status.fuel_in_tank - 25.0).abs() < 1e-5);
    Ok(())
}

/// Maximum u16 speed value parses correctly.
#[test]
fn car_telemetry_max_speed_u16() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        u16::MAX,
        8,
        u16::MAX,
        1.0,
        0.0,
        0.0,
        1,
        [30.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 0)?;
    assert_eq!(telem.speed_kmh, u16::MAX);
    assert_eq!(telem.engine_rpm, u16::MAX);
    Ok(())
}

/// Verify u16::MAX speed normalizes to expected m/s value.
#[test]
fn normalize_max_u16_speed() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: u16::MAX,
        throttle: 1.0,
        steer: 0.0,
        brake: 0.0,
        gear: 8,
        engine_rpm: 15000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData::default();
    let norm = normalize(&telem, &status, &SessionData::default());
    let expected = f32::from(u16::MAX) / 3.6;
    assert!((norm.speed_ms - expected).abs() < 0.01);
    Ok(())
}

/// Packet with format 2023 header but Car Status data that is 2024-sized:
/// process_packet should parse using the 2023 parser (47-byte entries).
#[test]
fn process_packet_f23_header_oversized_data_uses_f23_parser() -> TestResult {
    // Build an F1 23 status packet (47-byte entries) with extra padding
    let mut raw = build_car_status_packet_f23(0, 30.0, 2_000_000.0, 1, 0, 12, 13000);
    raw.extend_from_slice(&[0u8; 200]); // extra data
    let mut state = F1NativeState::default();
    // Need telemetry first
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        180,
        5,
        12000,
        0.7,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_some());
    let norm = result.ok_or("expected emission")?;
    assert!((norm.speed_ms - 180.0 / 3.6).abs() < 0.01);
    Ok(())
}

/// Truncated at various sizes within the header.
#[test]
fn parse_header_truncated_at_various_sizes() {
    for size in 0..29 {
        let raw = vec![0u8; size];
        assert!(parse_header(&raw).is_err(), "size {} should fail", size);
    }
}

/// Truncated car telemetry at various sizes near the minimum.
#[test]
fn car_telemetry_truncated_at_various_sizes() {
    for size in 0..MIN_CAR_TELEMETRY_PACKET_SIZE {
        let raw = vec![0u8; size];
        assert!(
            parse_car_telemetry(&raw, 0).is_err(),
            "size {} should fail",
            size
        );
    }
}

/// Truncated F1 23 car status at various sizes near the minimum.
#[test]
fn car_status_2023_truncated_at_various_sizes() {
    for size in 0..MIN_CAR_STATUS_2023_PACKET_SIZE {
        let raw = vec![0u8; size];
        assert!(
            parse_car_status_2023(&raw, 0).is_err(),
            "size {} should fail",
            size
        );
    }
}

/// Truncated F1 24 car status at various sizes near the minimum.
#[test]
fn car_status_2024_truncated_at_various_sizes() {
    for size in 0..MIN_CAR_STATUS_2024_PACKET_SIZE {
        let raw = vec![0u8; size];
        assert!(
            parse_car_status_2024(&raw, 0).is_err(),
            "size {} should fail",
            size
        );
    }
}

/// All non-supported packet IDs in 0..=255 are either handled or silently ignored.
#[test]
fn process_packet_all_packet_ids_no_panic() -> TestResult {
    let mut state = F1NativeState::default();
    for id in 0..=255u8 {
        let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, id, 0);
        // Should not panic; may return Ok(None) or Err for malformed data
        let _ = F1NativeAdapter::process_packet(&mut state, &raw);
    }
    Ok(())
}

/// Motion packets (ID 0) are silently ignored by process_packet.
#[test]
fn process_packet_motion_packet_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, 0, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Lap data packets (ID 2) are silently ignored.
#[test]
fn process_packet_lap_data_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 2, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Event packets (ID 3) are silently ignored.
#[test]
fn process_packet_event_packet_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, 3, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Participants packets (ID 4) are silently ignored.
#[test]
fn process_packet_participants_packet_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 4, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Car setups packets (ID 5) are silently ignored.
#[test]
fn process_packet_car_setups_packet_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, 5, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Final classification packets (ID 8) are silently ignored.
#[test]
fn process_packet_final_classification_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 8, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Lobby info packets (ID 9) are silently ignored.
#[test]
fn process_packet_lobby_info_ignored() -> TestResult {
    let mut state = F1NativeState::default();
    let raw = build_f1_native_header_bytes(PACKET_FORMAT_2024, 9, 0);
    let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
    assert!(result.is_none());
    Ok(())
}

/// Session updates overwrite previous session state.
#[test]
fn process_packet_session_updates_overwrite() -> TestResult {
    let mut state = F1NativeState::default();
    let raw1 = build_session_packet(PACKET_FORMAT_2023, 30, 22, 5, 11);
    F1NativeAdapter::process_packet(&mut state, &raw1)?;
    assert_eq!(state.session.track_id, 11);

    let raw2 = build_session_packet(PACKET_FORMAT_2023, 35, 25, 10, 5);
    F1NativeAdapter::process_packet(&mut state, &raw2)?;
    assert_eq!(state.session.track_id, 5);
    assert_eq!(state.session.session_type, 10);
    assert_eq!(state.session.track_temperature, 35);
    Ok(())
}

/// Verify that the traction_control and anti_lock_brakes flags are parsed
/// from car status packets and appear in normalized output.
#[test]
fn normalize_traction_control_and_abs_flags() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 150,
        throttle: 0.6,
        steer: 0.0,
        brake: 0.0,
        gear: 4,
        engine_rpm: 9000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        traction_control: 2,
        anti_lock_brakes: 1,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(norm.flags.traction_control);
    assert!(norm.flags.abs_active);
    Ok(())
}

/// Verify TC/ABS flags are false when set to 0.
#[test]
fn normalize_tc_abs_off() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 150,
        throttle: 0.6,
        steer: 0.0,
        brake: 0.0,
        gear: 4,
        engine_rpm: 9000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData {
        traction_control: 0,
        anti_lock_brakes: 0,
        ..F1NativeCarStatusData::default()
    };
    let norm = normalize(&telem, &status, &SessionData::default());
    assert!(!norm.flags.traction_control);
    assert!(!norm.flags.abs_active);
    Ok(())
}

/// Track names resolve correctly for known IDs.
#[test]
fn track_name_lookup_known_ids() -> TestResult {
    use openracing_telemetry_adapters::f1_25::track_name_from_id;

    let known = [
        (0, "Melbourne"),
        (5, "Monaco"),
        (7, "Silverstone"),
        (11, "Monza"),
        (14, "Abu Dhabi"),
        (30, "Miami"),
        (31, "Las Vegas"),
        (32, "Losail"),
    ];
    for (id, expected) in known {
        assert_eq!(
            track_name_from_id(id),
            expected,
            "track id {} should be '{}'",
            id,
            expected
        );
    }
    Ok(())
}

/// Unknown track IDs return "Unknown".
#[test]
fn track_name_lookup_unknown_ids() -> TestResult {
    use openracing_telemetry_adapters::f1_25::track_name_from_id;
    assert_eq!(track_name_from_id(33), "Unknown");
    assert_eq!(track_name_from_id(127), "Unknown");
    // Negative IDs are clamped to 0 by max(0), so -1 maps to Melbourne (index 0)
    assert_eq!(track_name_from_id(-1), "Melbourne");
    Ok(())
}

/// Wet tyre compound name.
#[test]
fn tyre_compound_wet() -> TestResult {
    use openracing_telemetry_adapters::f1_25::tyre_compound_name;
    assert_eq!(tyre_compound_name(8), "Wet");
    assert_eq!(tyre_compound_name(15), "Wet");
    Ok(())
}

/// Intermediate tyre compound name.
#[test]
fn tyre_compound_intermediate() -> TestResult {
    use openracing_telemetry_adapters::f1_25::tyre_compound_name;
    assert_eq!(tyre_compound_name(7), "Intermediate");
    Ok(())
}

/// Tire pressure reordering: F1 data [RL, RR, FL, FR] → normalized [FL, FR, RL, RR].
#[test]
fn normalize_tire_pressure_reorder() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 200,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 5,
        engine_rpm: 10000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [0; 4],
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [21.0, 21.5, 22.0, 22.5], // [RL, RR, FL, FR]
    };
    let status = F1NativeCarStatusData::default();
    let norm = normalize(&telem, &status, &SessionData::default());
    // Normalized order: [FL, FR, RL, RR]
    assert!((norm.tire_pressures_psi[0] - 22.0).abs() < 1e-4); // FL
    assert!((norm.tire_pressures_psi[1] - 22.5).abs() < 1e-4); // FR
    assert!((norm.tire_pressures_psi[2] - 21.0).abs() < 1e-4); // RL
    assert!((norm.tire_pressures_psi[3] - 21.5).abs() < 1e-4); // RR
    Ok(())
}

/// Tire surface temps reordering: F1 data [RL, RR, FL, FR] → normalized [FL, FR, RL, RR].
#[test]
fn normalize_tire_temps_reorder() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 200,
        throttle: 0.5,
        steer: 0.0,
        brake: 0.0,
        gear: 5,
        engine_rpm: 10000,
        drs: 0,
        brakes_temperature: [0; 4],
        tyres_surface_temperature: [81, 82, 83, 84], // [RL, RR, FL, FR]
        tyres_inner_temperature: [0; 4],
        engine_temperature: 0,
        tyres_pressure: [0.0; 4],
    };
    let status = F1NativeCarStatusData::default();
    let norm = normalize(&telem, &status, &SessionData::default());
    assert_eq!(norm.tire_temps_c[0], 83); // FL
    assert_eq!(norm.tire_temps_c[1], 84); // FR
    assert_eq!(norm.tire_temps_c[2], 81); // RL
    assert_eq!(norm.tire_temps_c[3], 82); // RR
    Ok(())
}

/// Car telemetry for player at middle index (index 11).
#[test]
fn car_telemetry_middle_player_index() -> TestResult {
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        11,
        260,
        7,
        13000,
        0.9,
        0.0,
        0.05,
        1,
        [24.0; 4],
    );
    let telem = parse_car_telemetry(&raw, 11)?;
    assert_eq!(telem.speed_kmh, 260);
    assert_eq!(telem.gear, 7);
    assert_eq!(telem.engine_rpm, 13000);
    Ok(())
}

/// NaN in car status ERS field is sanitized to 0.0.
#[test]
fn car_status_2024_nan_ers_sanitized() -> TestResult {
    let mut raw = build_car_status_packet_f24(0, 20.0, 1_000_000.0, 0, 0, 13, 12000);
    // Overwrite ERS store energy (at offset base+37..41) with NaN
    let base = 29; // car 0
    let nan_bytes = f32::NAN.to_le_bytes();
    raw[base + 37..base + 41].copy_from_slice(&nan_bytes);
    let status = parse_car_status_2024(&raw, 0)?;
    assert_eq!(status.ers_store_energy, 0.0);
    Ok(())
}

/// Adapter normalize() rejects format 0.
#[test]
fn adapter_normalize_rejects_format_0() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(0, 6, 0);
    assert!(adapter.normalize(&raw).is_err());
}

/// Adapter normalize() rejects format u16::MAX.
#[test]
fn adapter_normalize_rejects_format_max() {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(u16::MAX, 6, 0);
    assert!(adapter.normalize(&raw).is_err());
}

/// process_packet with a session packet then a status-only does not emit.
#[test]
fn process_packet_session_then_status_no_emit() -> TestResult {
    let mut state = F1NativeState::default();
    let session = build_session_packet(PACKET_FORMAT_2024, 30, 25, 6, 11);
    F1NativeAdapter::process_packet(&mut state, &session)?;

    let status = build_car_status_packet_f24(0, 25.0, 2_000_000.0, 1, 0, 13, 13500);
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(result.is_none(), "status without telemetry must not emit");
    Ok(())
}

/// Verify decoder_type is set in process_packet emissions.
#[test]
fn process_packet_emission_has_decoder_type() -> TestResult {
    let mut state = F1NativeState::default();
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        200,
        6,
        11000,
        0.8,
        0.0,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f24(0, 25.0, 2_000_000.0, 1, 0, 13, 13500);
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let norm = F1NativeAdapter::process_packet(&mut state, &status)?.ok_or("expected emission")?;
    assert_eq!(
        norm.extended.get("decoder_type"),
        Some(&TelemetryValue::String("f1_native_udp".to_string()))
    );
    Ok(())
}
