//! Deep tests for the Project CARS 2 / Project CARS 3 telemetry adapter.

use openracing_telemetry_adapters::pcars2::{
    PACKET_TYPE_TIMINGS, merge_timing_fields, parse_pcars2_packet, parse_pcars2_timings_packet,
    pcars2_packet_type,
};
use openracing_telemetry_adapters::{NormalizedTelemetry, PCars2Adapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Packet construction helpers ─────────────────────────────────────────

const PCARS2_UDP_MIN_SIZE: usize = 46;
const MAX_PACKET_SIZE: usize = 1500;
const OFF_STEERING: usize = 44;
const OFF_THROTTLE: usize = 30;
const OFF_BRAKE: usize = 29;
const OFF_CLUTCH: usize = 31;
const OFF_SPEED: usize = 36;
const OFF_RPM: usize = 40;
const OFF_MAX_RPM: usize = 42;
const OFF_GEAR_NUM_GEARS: usize = 45;
const OFF_FUEL_LEVEL: usize = 32;
const OFF_CAR_FLAGS: usize = 17;
const OFF_WATER_TEMP: usize = 22;
const OFF_PACKET_TYPE: usize = 10;
const OFF_LOCAL_ACCEL_X: usize = 100;
const OFF_LOCAL_ACCEL_Y: usize = 104;
const OFF_LOCAL_ACCEL_Z: usize = 108;
const OFF_TYRE_TEMP: usize = 176;
const OFF_AIR_PRESSURE: usize = 352;

// Timing packet offsets
const TIMINGS_OFF_NUM_PARTICIPANTS: usize = 12;
const TIMINGS_OFF_PARTICIPANTS: usize = 29;
const PARTICIPANT_ENTRY_SIZE: usize = 28;
const PART_OFF_RACE_POSITION: usize = 8;
const PART_OFF_CURRENT_LAP: usize = 10;
const PART_OFF_BEST_LAP_TIME: usize = 16;
const PART_OFF_LAST_LAP_TIME: usize = 20;
const PART_OFF_CURRENT_TIME: usize = 24;

fn make_pcars2_packet(
    steering: f32,
    throttle: f32,
    brake: f32,
    speed: f32,
    rpm: f32,
    max_rpm: f32,
    gear: u32,
) -> Vec<u8> {
    let mut data = vec![0u8; PCARS2_UDP_MIN_SIZE];
    data[OFF_STEERING] = (steering.clamp(-1.0, 1.0) * 127.0) as i8 as u8;
    data[OFF_THROTTLE] = (throttle.clamp(0.0, 1.0) * 255.0) as u8;
    data[OFF_BRAKE] = (brake.clamp(0.0, 1.0) * 255.0) as u8;
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 2].copy_from_slice(&(rpm as u16).to_le_bytes());
    data[OFF_MAX_RPM..OFF_MAX_RPM + 2].copy_from_slice(&(max_rpm as u16).to_le_bytes());
    let gear_val: u8 = if gear > 14 { 15 } else { gear as u8 };
    data[OFF_GEAR_NUM_GEARS] = gear_val;
    data
}

fn make_full_pcars2_packet(
    steering: f32,
    throttle: f32,
    brake: f32,
    speed: f32,
    rpm: f32,
    max_rpm: f32,
    gear: u32,
) -> Vec<u8> {
    let mut data = vec![0u8; MAX_PACKET_SIZE.min(538)];
    data[OFF_STEERING] = (steering.clamp(-1.0, 1.0) * 127.0) as i8 as u8;
    data[OFF_THROTTLE] = (throttle.clamp(0.0, 1.0) * 255.0) as u8;
    data[OFF_BRAKE] = (brake.clamp(0.0, 1.0) * 255.0) as u8;
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 2].copy_from_slice(&(rpm as u16).to_le_bytes());
    data[OFF_MAX_RPM..OFF_MAX_RPM + 2].copy_from_slice(&(max_rpm as u16).to_le_bytes());
    let gear_val: u8 = if gear > 14 { 15 } else { gear as u8 };
    data[OFF_GEAR_NUM_GEARS] = gear_val;
    data
}

fn make_timings_packet(
    num_participants: u8,
    entries: &[(u8, u8, f32, f32, f32)], // (position, lap, best, last, current)
) -> Vec<u8> {
    let size = TIMINGS_OFF_PARTICIPANTS + entries.len().max(1) * PARTICIPANT_ENTRY_SIZE;
    let mut data = vec![0u8; size];
    data[OFF_PACKET_TYPE] = PACKET_TYPE_TIMINGS;
    data[TIMINGS_OFF_NUM_PARTICIPANTS] = num_participants;
    for (i, &(pos, lap, best, last, current)) in entries.iter().enumerate() {
        let base = TIMINGS_OFF_PARTICIPANTS + i * PARTICIPANT_ENTRY_SIZE;
        data[base + PART_OFF_RACE_POSITION] = pos;
        data[base + PART_OFF_CURRENT_LAP] = lap;
        data[base + PART_OFF_BEST_LAP_TIME..base + PART_OFF_BEST_LAP_TIME + 4]
            .copy_from_slice(&best.to_le_bytes());
        data[base + PART_OFF_LAST_LAP_TIME..base + PART_OFF_LAST_LAP_TIME + 4]
            .copy_from_slice(&last.to_le_bytes());
        data[base + PART_OFF_CURRENT_TIME..base + PART_OFF_CURRENT_TIME + 4]
            .copy_from_slice(&current.to_le_bytes());
    }
    data
}

// ── Shared memory parsing tests ─────────────────────────────────────────

#[test]
fn pcars2_parse_basic_telemetry() -> TestResult {
    let data = make_pcars2_packet(0.5, 0.9, 0.1, 60.0, 7000.0, 9000.0, 4);
    let result = parse_pcars2_packet(&data)?;
    assert!((result.speed_ms - 60.0).abs() < 0.01);
    assert!((result.rpm - 7000.0).abs() < 1.0);
    assert_eq!(result.gear, 4);
    Ok(())
}

#[test]
fn pcars2_parse_reverse_gear() -> TestResult {
    let data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 800.0, 8000.0, 15);
    let result = parse_pcars2_packet(&data)?;
    assert_eq!(result.gear, -1, "gear nibble 15 should map to reverse (-1)");
    Ok(())
}

#[test]
fn pcars2_parse_neutral_gear() -> TestResult {
    let data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 800.0, 8000.0, 0);
    let result = parse_pcars2_packet(&data)?;
    assert_eq!(result.gear, 0, "gear nibble 0 should map to neutral (0)");
    Ok(())
}

#[test]
fn pcars2_num_gears_from_high_nibble() -> TestResult {
    let mut data = make_pcars2_packet(0.0, 0.5, 0.0, 30.0, 4000.0, 8000.0, 3);
    // High nibble = 6 (6 forward gears), low nibble = 3 (3rd gear)
    data[OFF_GEAR_NUM_GEARS] = (6 << 4) | 3;
    let result = parse_pcars2_packet(&data)?;
    assert_eq!(result.gear, 3);
    assert_eq!(result.num_gears, 6);
    Ok(())
}

// ── Participant data extraction tests ───────────────────────────────────

#[test]
fn pcars2_timings_single_participant() -> TestResult {
    let data = make_timings_packet(1, &[(3, 5, 62.5, 63.1, 30.0)]);
    let result = parse_pcars2_timings_packet(&data, 0)?;
    assert_eq!(result.position, 3);
    assert_eq!(result.lap, 5);
    assert!((result.best_lap_time_s - 62.5).abs() < 0.01);
    assert!((result.last_lap_time_s - 63.1).abs() < 0.01);
    assert!((result.current_lap_time_s - 30.0).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_timings_multiple_participants() -> TestResult {
    let entries = vec![
        (1, 8, 58.0, 59.2, 25.0),
        (2, 7, 59.5, 60.1, 22.0),
        (3, 7, 60.0, 61.0, 28.0),
    ];
    let data = make_timings_packet(3, &entries);
    // Request participant index 1
    let result = parse_pcars2_timings_packet(&data, 1)?;
    assert_eq!(result.position, 2);
    assert_eq!(result.lap, 7);
    assert!((result.best_lap_time_s - 59.5).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_timings_out_of_range_participant_falls_back_to_zero() -> TestResult {
    let data = make_timings_packet(1, &[(5, 3, 70.0, 71.0, 15.0)]);
    // Request participant 10 but only 1 exists → fall back to index 0
    let result = parse_pcars2_timings_packet(&data, 10)?;
    assert_eq!(result.position, 5);
    Ok(())
}

#[test]
fn pcars2_timings_invalid_times_default_to_zero() -> TestResult {
    let data = make_timings_packet(1, &[(1, 1, -1.0, -1.0, -1.0)]);
    let result = parse_pcars2_timings_packet(&data, 0)?;
    assert_eq!(result.best_lap_time_s, 0.0);
    assert_eq!(result.last_lap_time_s, 0.0);
    assert_eq!(result.current_lap_time_s, 0.0);
    Ok(())
}

// ── Timing and scoring tests ────────────────────────────────────────────

#[test]
fn pcars2_merge_timing_preserves_telemetry_fields() -> TestResult {
    let mut telemetry =
        parse_pcars2_packet(&make_pcars2_packet(0.0, 0.7, 0.0, 45.0, 5000.0, 8000.0, 3))?;
    let timing_data = make_timings_packet(1, &[(2, 6, 61.0, 62.0, 20.0)]);
    let timing = parse_pcars2_timings_packet(&timing_data, 0)?;
    merge_timing_fields(&mut telemetry, &timing);

    // Timing fields merged
    assert_eq!(telemetry.position, 2);
    assert_eq!(telemetry.lap, 6);
    assert!((telemetry.best_lap_time_s - 61.0).abs() < 0.01);
    // Original fields preserved
    assert!((telemetry.speed_ms - 45.0).abs() < 0.01);
    assert!((telemetry.rpm - 5000.0).abs() < 1.0);
    assert_eq!(telemetry.gear, 3);
    Ok(())
}

#[test]
fn pcars2_merge_timing_does_not_overwrite_with_defaults() -> TestResult {
    let mut telemetry = NormalizedTelemetry::builder()
        .position(5)
        .lap(10_u16)
        .best_lap_time_s(55.0)
        .build();
    // Timing with all-default values (position=0, lap=0, times=0.0)
    let timing = NormalizedTelemetry::default();
    merge_timing_fields(&mut telemetry, &timing);

    assert_eq!(
        telemetry.position, 5,
        "should keep original position when timing has 0"
    );
    assert_eq!(
        telemetry.lap, 10,
        "should keep original lap when timing has 0"
    );
    assert!((telemetry.best_lap_time_s - 55.0).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_packet_type_detection() {
    let mut telem_pkt = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
    telem_pkt[OFF_PACKET_TYPE] = 0;
    assert_eq!(pcars2_packet_type(&telem_pkt), Some(0));

    let timing_pkt = make_timings_packet(1, &[(1, 1, 60.0, 61.0, 10.0)]);
    assert_eq!(pcars2_packet_type(&timing_pkt), Some(PACKET_TYPE_TIMINGS));

    assert_eq!(pcars2_packet_type(&[]), None);
}

#[test]
fn pcars2_full_packet_gforces() -> TestResult {
    let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
    let g: f32 = 9.80665;
    data[OFF_LOCAL_ACCEL_X..OFF_LOCAL_ACCEL_X + 4].copy_from_slice(&g.to_le_bytes());
    data[OFF_LOCAL_ACCEL_Y..OFF_LOCAL_ACCEL_Y + 4].copy_from_slice(&(-g).to_le_bytes());
    data[OFF_LOCAL_ACCEL_Z..OFF_LOCAL_ACCEL_Z + 4].copy_from_slice(&(0.5 * g).to_le_bytes());

    let result = parse_pcars2_packet(&data)?;
    assert!((result.lateral_g - 1.0).abs() < 0.01);
    assert!((result.vertical_g - (-1.0)).abs() < 0.01);
    assert!((result.longitudinal_g - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_full_packet_tyre_temps() -> TestResult {
    let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
    data[OFF_TYRE_TEMP] = 85;
    data[OFF_TYRE_TEMP + 1] = 92;
    data[OFF_TYRE_TEMP + 2] = 78;
    data[OFF_TYRE_TEMP + 3] = 89;

    let result = parse_pcars2_packet(&data)?;
    assert_eq!(result.tire_temps_c, [85, 92, 78, 89]);
    Ok(())
}

#[test]
fn pcars2_full_packet_tyre_pressures() -> TestResult {
    let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
    let kpa: u16 = 200;
    for i in 0..4 {
        data[OFF_AIR_PRESSURE + i * 2..OFF_AIR_PRESSURE + i * 2 + 2]
            .copy_from_slice(&kpa.to_le_bytes());
    }

    let result = parse_pcars2_packet(&data)?;
    let expected_psi = 200.0 * 0.145_038;
    for &p in &result.tire_pressures_psi {
        assert!((p - expected_psi).abs() < 0.01);
    }
    Ok(())
}

#[test]
fn pcars2_car_flags_pit_limiter_and_abs() -> TestResult {
    let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
    data[OFF_CAR_FLAGS] = 0x18; // bit 3 = pit limiter, bit 4 = ABS
    let result = parse_pcars2_packet(&data)?;
    assert!(result.flags.pit_limiter);
    assert!(result.flags.abs_active);
    Ok(())
}

#[test]
fn pcars2_reject_empty_packet() {
    assert!(parse_pcars2_packet(&[]).is_err());
}

#[test]
fn pcars2_reject_undersized_packet() {
    let data = vec![0u8; PCARS2_UDP_MIN_SIZE - 1];
    assert!(parse_pcars2_packet(&data).is_err());
}

#[test]
fn pcars2_adapter_game_id_and_rate() {
    let adapter = PCars2Adapter::new();
    assert_eq!(adapter.game_id(), "project_cars_2");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(10));
}

#[test]
fn pcars2_adapter_normalize_dispatches_telemetry() -> TestResult {
    let adapter = PCars2Adapter::new();
    let data = make_pcars2_packet(0.0, 0.6, 0.0, 40.0, 4000.0, 8000.0, 3);
    let result = adapter.normalize(&data)?;
    assert!((result.rpm - 4000.0).abs() < 1.0);
    assert_eq!(result.gear, 3);
    Ok(())
}

#[test]
fn pcars2_adapter_normalize_dispatches_timing() -> TestResult {
    let adapter = PCars2Adapter::new();
    let data = make_timings_packet(1, &[(4, 7, 59.0, 60.0, 22.0)]);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.position, 4);
    assert_eq!(result.lap, 7);
    Ok(())
}

#[test]
fn pcars2_fuel_level_parsing() -> TestResult {
    let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
    let fuel: f32 = 0.75;
    data[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&fuel.to_le_bytes());
    let result = parse_pcars2_packet(&data)?;
    assert!((result.fuel_percent - 0.75).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_water_temp_parsing() -> TestResult {
    let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
    data[OFF_WATER_TEMP..OFF_WATER_TEMP + 2].copy_from_slice(&95i16.to_le_bytes());
    let result = parse_pcars2_packet(&data)?;
    assert!((result.engine_temp_c - 95.0).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_clutch_parsing() -> TestResult {
    let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
    data[OFF_CLUTCH] = 128; // ~50%
    let result = parse_pcars2_packet(&data)?;
    assert!((result.clutch - 128.0 / 255.0).abs() < 0.01);
    Ok(())
}

#[test]
fn pcars2_timings_position_mask_top_bit() -> TestResult {
    // Top bit indicates "active" participant; position is lower 7 bits
    let mut data = make_timings_packet(1, &[(5, 2, 60.0, 61.0, 10.0)]);
    let base = TIMINGS_OFF_PARTICIPANTS;
    data[base + PART_OFF_RACE_POSITION] = 0x80 | 5; // active=true, position=5
    let result = parse_pcars2_timings_packet(&data, 0)?;
    assert_eq!(result.position, 5);
    Ok(())
}

#[test]
fn pcars2_timings_too_short_rejected() {
    let data = vec![0u8; 20];
    assert!(parse_pcars2_timings_packet(&data, 0).is_err());
}
