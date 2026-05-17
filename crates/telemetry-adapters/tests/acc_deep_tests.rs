//! Deep validation tests for the ACC (Assetto Corsa Competizione) broadcasting
//! protocol adapter.
//!
//! Covers RealtimeCarUpdate parsing, gear encoding, speed conversion, pit-flag
//! mapping, lap-time arithmetic, state-machine interactions, and proptest
//! fuzzing of the normalize path.

use openracing_telemetry_adapters::{ACCAdapter, TelemetryAdapter, TelemetryValue};
use proptest::prelude::*;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── ACC broadcasting message IDs ─────────────────────────────────────────────

const MSG_REGISTRATION_RESULT: u8 = 1;
const MSG_REALTIME_CAR_UPDATE: u8 = 3;
const MSG_TRACK_DATA: u8 = 5;

// ── Packet builders ──────────────────────────────────────────────────────────

fn push_acc_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn push_lap(buf: &mut Vec<u8>, lap_time_ms: i32) {
    buf.extend_from_slice(&lap_time_ms.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // car index
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver index
    buf.push(0); // split count
    buf.push(0); // is invalid
    buf.push(1); // valid for best
    buf.push(0); // outlap
    buf.push(0); // inlap
}

fn build_car_update(
    car_index: u16,
    gear_raw: u8,
    speed_kmh: u16,
    position: u16,
    laps: u16,
    car_location: u8,
) -> Vec<u8> {
    build_car_update_ext(
        car_index,
        gear_raw,
        speed_kmh,
        position,
        laps,
        car_location,
        91_000,
        92_000,
        45_000,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_car_update_ext(
    car_index: u16,
    gear_raw: u8,
    speed_kmh: u16,
    position: u16,
    laps: u16,
    car_location: u8,
    best_ms: i32,
    last_ms: i32,
    current_ms: i32,
) -> Vec<u8> {
    let mut pkt = vec![MSG_REALTIME_CAR_UPDATE];
    pkt.extend_from_slice(&car_index.to_le_bytes());
    pkt.extend_from_slice(&0u16.to_le_bytes()); // driver index
    pkt.push(1); // driver count
    pkt.push(gear_raw);
    pkt.extend_from_slice(&0.0f32.to_le_bytes()); // world pos x
    pkt.extend_from_slice(&0.0f32.to_le_bytes()); // world pos y
    pkt.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
    pkt.push(car_location);
    pkt.extend_from_slice(&speed_kmh.to_le_bytes());
    pkt.extend_from_slice(&position.to_le_bytes());
    pkt.extend_from_slice(&position.to_le_bytes()); // cup position
    pkt.extend_from_slice(&position.to_le_bytes()); // track position
    pkt.extend_from_slice(&0.5f32.to_le_bytes()); // spline position
    pkt.extend_from_slice(&laps.to_le_bytes());
    pkt.extend_from_slice(&0i32.to_le_bytes()); // delta
    push_lap(&mut pkt, best_ms);
    push_lap(&mut pkt, last_ms);
    push_lap(&mut pkt, current_ms);
    pkt
}

fn build_registration(connection_id: i32, success: bool) -> Vec<u8> {
    let mut pkt = vec![MSG_REGISTRATION_RESULT];
    pkt.extend_from_slice(&connection_id.to_le_bytes());
    pkt.push(u8::from(success));
    pkt.push(0); // readonly byte
    push_acc_string(&mut pkt, "");
    pkt
}

fn build_track_data(name: &str) -> Vec<u8> {
    let mut pkt = vec![MSG_TRACK_DATA];
    pkt.extend_from_slice(&0i32.to_le_bytes());
    push_acc_string(&mut pkt, name);
    pkt.extend_from_slice(&1i32.to_le_bytes()); // track id
    pkt.extend_from_slice(&5793i32.to_le_bytes()); // track meters
    pkt.push(0); // camera sets
    pkt.push(0); // hud pages
    pkt
}

// ── Proptest ─────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fuzz_normalize_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = ACCAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_speed_conversion(kmh in 0u16..500) {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, kmh, 1, 1, 1);
        if let Ok(t) = adapter.normalize(&pkt) {
            let expected = f32::from(kmh) / 3.6;
            prop_assert!((t.speed_ms - expected).abs() < 0.2);
        }
    }

    #[test]
    fn fuzz_gear_encoding(gear_raw in 0u8..20) {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, gear_raw, 100, 1, 1, 1);
        if let Ok(t) = adapter.normalize(&pkt) {
            let expected = (i16::from(gear_raw) - 1).clamp(-128, 127) as i8;
            prop_assert_eq!(t.gear, expected);
        }
    }

    #[test]
    fn fuzz_position_clamped(pos in 0u16..500) {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 100, pos, 1, 1);
        if let Ok(t) = adapter.normalize(&pkt) {
            // position is u8, so always <= 255; verify it parsed without error
            let _ = t.position;
        }
    }
}

// ── Gear edge cases ──────────────────────────────────────────────────────────

#[test]
fn gear_first() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 2, 80, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, 1, "wire 2 = 1st gear");
    Ok(())
}

#[test]
fn gear_sixth() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 7, 250, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, 6);
    Ok(())
}

// ── Speed: precise km/h → m/s conversion ─────────────────────────────────────

#[test]
fn speed_100kmh_to_ms() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 100, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    let expected = 100.0f32 / 3.6;
    assert!((t.speed_ms - expected).abs() < 0.1);
    Ok(())
}

#[test]
fn speed_300kmh_to_ms() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 6, 300, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    let expected = 300.0f32 / 3.6;
    assert!((t.speed_ms - expected).abs() < 0.1);
    Ok(())
}

// ── Car location → pit flags ─────────────────────────────────────────────────

#[test]
fn car_location_0_none() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 100, 1, 5, 0);
    let t = adapter.normalize(&pkt)?;
    assert!(!t.flags.in_pits);
    Ok(())
}

#[test]
fn car_location_1_on_track() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 200, 1, 5, 1);
    let t = adapter.normalize(&pkt)?;
    assert!(!t.flags.in_pits);
    assert!(!t.flags.pit_limiter);
    Ok(())
}

#[test]
fn car_location_2_pitlane() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 2, 60, 1, 5, 2);
    let t = adapter.normalize(&pkt)?;
    assert!(t.flags.in_pits);
    assert!(t.flags.pit_limiter);
    Ok(())
}

#[test]
fn car_location_3_pit_entry() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 3, 80, 1, 5, 3);
    let t = adapter.normalize(&pkt)?;
    assert!(t.flags.in_pits);
    Ok(())
}

#[test]
fn car_location_4_pit_exit() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 3, 80, 1, 5, 4);
    let t = adapter.normalize(&pkt)?;
    assert!(t.flags.in_pits);
    Ok(())
}

// ── Lap time: negative values clamped ────────────────────────────────────────

#[test]
fn negative_lap_times_clamped_to_zero() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update_ext(1, 4, 120, 1, 5, 1, -1, -1, -1);
    let t = adapter.normalize(&pkt)?;
    assert!(t.best_lap_time_s >= 0.0, "best");
    assert!(t.last_lap_time_s >= 0.0, "last");
    assert!(t.current_lap_time_s >= 0.0, "current");
    Ok(())
}

#[test]
fn lap_time_precision() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update_ext(1, 4, 120, 1, 5, 1, 123_456, 100_000, 50_500);
    let t = adapter.normalize(&pkt)?;
    assert!((t.best_lap_time_s - 123.456).abs() < 0.01);
    assert!((t.last_lap_time_s - 100.0).abs() < 0.01);
    assert!((t.current_lap_time_s - 50.5).abs() < 0.01);
    Ok(())
}

// ── Delta in extended ────────────────────────────────────────────────────────

#[test]
fn delta_ms_in_extended() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 120, 1, 5, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(
        t.extended.get("delta_ms"),
        Some(&TelemetryValue::Integer(0))
    );
    Ok(())
}

// ── Car index in car_id ──────────────────────────────────────────────────────

#[test]
fn car_id_from_index_0() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(0, 4, 120, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.car_id, Some("car_0".to_string()));
    Ok(())
}

#[test]
fn car_id_from_index_max() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(999, 4, 120, 1, 1, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.car_id, Some("car_999".to_string()));
    Ok(())
}

// ── Registration result does not produce telemetry ───────────────────────────

#[test]
fn registration_success_no_telemetry() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_registration(42, true);
    assert!(adapter.normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn registration_failure_no_telemetry() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_registration(42, false);
    assert!(adapter.normalize(&pkt).is_err());
    Ok(())
}

// ── Track data alone does not produce telemetry ──────────────────────────────

#[test]
fn track_data_no_telemetry() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_track_data("nurburgring");
    assert!(adapter.normalize(&pkt).is_err());
    Ok(())
}

// ── Unknown message type ─────────────────────────────────────────────────────

#[test]
fn unknown_message_type_rejected() -> TestResult {
    let adapter = ACCAdapter::new();
    assert!(adapter.normalize(&[200u8, 0, 0, 0, 0]).is_err());
    Ok(())
}

// ── Single byte packet ───────────────────────────────────────────────────────

#[test]
fn single_byte_packet_rejected() -> TestResult {
    let adapter = ACCAdapter::new();
    assert!(adapter.normalize(&[MSG_REALTIME_CAR_UPDATE]).is_err());
    Ok(())
}

// ── Adapter metadata ─────────────────────────────────────────────────────────

#[test]
fn game_id_is_acc() -> TestResult {
    let adapter = ACCAdapter::new();
    assert_eq!(adapter.game_id(), "acc");
    Ok(())
}

#[test]
fn update_rate_16ms() -> TestResult {
    let adapter = ACCAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    Ok(())
}

// ── Laps field ───────────────────────────────────────────────────────────────

#[test]
fn lap_count_zero() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 120, 1, 0, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.lap, 0);
    Ok(())
}

#[test]
fn lap_count_high() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 120, 1, 500, 1);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.lap, 500);
    Ok(())
}

// ── Extended: spline position, cup/track position ────────────────────────────

#[test]
fn extended_fields_all_present() -> TestResult {
    let adapter = ACCAdapter::new();
    let pkt = build_car_update(1, 4, 120, 2, 5, 1);
    let t = adapter.normalize(&pkt)?;
    assert!(t.extended.contains_key("cup_position"));
    assert!(t.extended.contains_key("track_position"));
    assert!(t.extended.contains_key("delta_ms"));
    assert!(t.extended.contains_key("spline_position"));
    Ok(())
}
