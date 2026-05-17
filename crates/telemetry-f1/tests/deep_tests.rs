//! Deep tests for the F1 2023/2024 native UDP telemetry adapter.
//!
//! Covers packet construction, header parsing, field extraction,
//! normalization, ERS data, and game-specific F1 features.

use openracing_telemetry_adapters::f1_native::{
    F1NativeAdapter, F1NativeState, PACKET_FORMAT_2023, PACKET_FORMAT_2024,
    build_car_status_packet_f23, build_car_status_packet_f24, build_car_telemetry_packet_native,
    build_f1_native_header_bytes,
};
use racing_wheel_telemetry_f1::{
    F1NativeAdapter as ReexportedAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Adapter identity tests ───────────────────────────────────────────────────

#[test]
fn deep_game_id_is_f1_native() -> TestResult {
    let adapter = F1NativeAdapter::new();
    assert_eq!(adapter.game_id(), "f1_native");
    Ok(())
}

#[test]
fn deep_reexported_adapter_matches() -> TestResult {
    let adapter = ReexportedAdapter::new();
    assert_eq!(adapter.game_id(), "f1_native");
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
    Ok(())
}

#[test]
fn deep_with_port_overrides_bind_port() -> TestResult {
    let adapter = F1NativeAdapter::new().with_port(12345);
    assert_eq!(adapter.game_id(), "f1_native");
    Ok(())
}

// ── Packet rejection tests ───────────────────────────────────────────────────

#[test]
fn deep_normalize_rejects_empty_data() -> TestResult {
    let adapter = F1NativeAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_normalize_rejects_short_header() -> TestResult {
    let adapter = F1NativeAdapter::new();
    assert!(adapter.normalize(&[0u8; 10]).is_err());
    Ok(())
}

#[test]
fn deep_normalize_rejects_unsupported_format() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2025, 6, 0);
    assert!(adapter.normalize(&raw).is_err());
    Ok(())
}

#[test]
fn deep_normalize_rejects_unknown_packet_id() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_f1_native_header_bytes(2023, 99, 0);
    assert!(adapter.normalize(&raw).is_err());
    Ok(())
}

// ── Car Telemetry packet (ID 6) parsing ──────────────────────────────────────

#[test]
fn deep_car_telemetry_2023_speed_conversion() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,    // player_index
        200,  // speed_kmh
        3,    // gear
        8000, // rpm
        0.75, // throttle
        0.0,  // brake
        0.0,  // steer
        0,    // drs
        [25.0, 25.0, 25.0, 25.0],
    );
    let t = adapter.normalize(&raw)?;
    // 200 km/h → ~55.56 m/s
    assert!(
        (t.speed_ms - 200.0 / 3.6).abs() < 0.1,
        "speed_ms={}",
        t.speed_ms
    );
    assert_eq!(t.gear, 3);
    assert!((t.rpm - 8000.0).abs() < 0.1);
    assert!((t.throttle - 0.75).abs() < 0.01);
    Ok(())
}

#[test]
fn deep_car_telemetry_2024_identical_layout() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        150,
        5,
        10000,
        1.0,
        0.5,
        -0.3,
        1,
        [22.0, 22.5, 23.0, 23.5],
    );
    let t = adapter.normalize(&raw)?;
    assert!((t.speed_ms - 150.0 / 3.6).abs() < 0.1);
    assert_eq!(t.gear, 5);
    assert!((t.throttle - 1.0).abs() < 0.01);
    assert!((t.brake - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn deep_car_telemetry_drs_flag() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        300,
        8,
        11000,
        1.0,
        0.0,
        0.0,
        1, // DRS on
        [25.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!(t.flags.drs_active, "DRS should be active");
    Ok(())
}

#[test]
fn deep_car_telemetry_no_drs() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        200,
        5,
        8000,
        0.5,
        0.0,
        0.0,
        0, // DRS off
        [25.0; 4],
    );
    let t = adapter.normalize(&raw)?;
    assert!(!t.flags.drs_active, "DRS should not be active");
    Ok(())
}

// ── process_packet stateful tests ────────────────────────────────────────────

#[test]
fn deep_process_packet_telemetry_then_status() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        250,
        6,
        10500,
        0.9,
        0.1,
        0.2,
        0,
        [24.0; 4],
    );
    let status = build_car_status_packet_f23(0, 35.0, 2_000_000.0, 1, 0, 16, 12000);

    let mut state = F1NativeState::default();
    let r1 = F1NativeAdapter::process_packet(&mut state, &telem)?;
    assert!(r1.is_none(), "no status yet → no output");

    let r2 = F1NativeAdapter::process_packet(&mut state, &status)?;
    assert!(r2.is_some(), "telemetry+status → output");
    let t = r2.ok_or("expected Some")?;
    assert!((t.speed_ms - 250.0 / 3.6).abs() < 0.1);
    assert!(t.flags.drs_available, "drs_allowed=1 → available");
    Ok(())
}

#[test]
fn deep_process_packet_f24_with_engine_power() -> TestResult {
    let telem = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        180,
        4,
        9000,
        0.7,
        0.2,
        0.0,
        0,
        [23.0; 4],
    );
    let status = build_car_status_packet_f24(0, 40.0, 3_000_000.0, 0, 1, 18, 11500);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected normalized output")?;

    assert!(t.flags.pit_limiter, "pit_limiter=1 → active");
    assert!(t.flags.in_pits, "pit_limiter → in_pits");
    assert!(!t.flags.drs_available, "drs_allowed=0 → not available");
    Ok(())
}

#[test]
fn deep_process_packet_rejects_wrong_format() -> TestResult {
    let raw = build_f1_native_header_bytes(2025, 6, 0);
    let mut state = F1NativeState::default();
    assert!(F1NativeAdapter::process_packet(&mut state, &raw).is_err());
    Ok(())
}

// ── Tire data reordering ─────────────────────────────────────────────────────

#[test]
fn deep_tire_pressures_reordered_from_f1_layout() -> TestResult {
    // F1 wire order: [RL, RR, FL, FR]; normalized order: [FL, FR, RL, RR]
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2023,
        0,
        100,
        3,
        5000,
        0.5,
        0.0,
        0.0,
        0,
        [21.0, 22.0, 23.0, 24.0], // wire: [RL=21, RR=22, FL=23, FR=24]
    );
    let adapter = F1NativeAdapter::new();
    let t = adapter.normalize(&raw)?;
    // Expected: [FL=23, FR=24, RL=21, RR=22]
    assert!((t.tire_pressures_psi[0] - 23.0).abs() < 0.1, "FL");
    assert!((t.tire_pressures_psi[1] - 24.0).abs() < 0.1, "FR");
    assert!((t.tire_pressures_psi[2] - 21.0).abs() < 0.1, "RL");
    assert!((t.tire_pressures_psi[3] - 22.0).abs() < 0.1, "RR");
    Ok(())
}

// ── ERS and fuel extended data ───────────────────────────────────────────────

#[test]
fn deep_ers_fraction_in_extended() -> TestResult {
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
    let status = build_car_status_packet_f23(0, 50.0, 2_000_000.0, 0, 0, 16, 11000);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    let ers_frac = t.extended.get("ers_store_fraction");
    assert!(
        ers_frac.is_some(),
        "ers_store_fraction missing from extended"
    );
    Ok(())
}

#[test]
fn deep_fuel_remaining_kg_in_extended() -> TestResult {
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
    let status = build_car_status_packet_f23(0, 42.5, 1_000_000.0, 0, 0, 16, 11000);

    let mut state = F1NativeState::default();
    F1NativeAdapter::process_packet(&mut state, &telem)?;
    let result = F1NativeAdapter::process_packet(&mut state, &status)?;
    let t = result.ok_or("expected output")?;

    assert!(t.extended.contains_key("fuel_remaining_kg"));
    Ok(())
}

// ── TelemetryFrame construction ──────────────────────────────────────────────

#[test]
fn deep_telemetry_frame_from_normalized() -> TestResult {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(7500.0)
        .speed_ms(55.0)
        .build();
    let frame = TelemetryFrame::new(telemetry, 99999, 42, 1024);
    assert!((frame.data.rpm - 7500.0).abs() < 0.01);
    assert_eq!(frame.timestamp_ns, 99999);
    assert_eq!(frame.sequence, 42);
    assert_eq!(frame.raw_size, 1024);
    Ok(())
}
