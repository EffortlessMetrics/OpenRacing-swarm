//! Extended golden packet tests for telemetry adapters that were not covered in
//! `golden_packets.rs`.
//!
//! Covers: iRacing, ACC, BeamNG.drive, DiRT Rally 2.0, ETS2, and ATS.

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::{
    TelemetryAdapter,
    acc::ACCAdapter,
    beamng::BeamNGAdapter,
    dirt_rally_2::DirtRally2Adapter,
    ets2::{self, Ets2Adapter, Ets2Variant},
    iracing::IRacingAdapter,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Shared helpers ───────────────────────────────────────────────────────────

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ═══════════════════════════════════════════════════════════════════════════════
// iRacing — repr(C) struct layout (IRacingData)
// ═══════════════════════════════════════════════════════════════════════════════

// IRacingData field offsets (repr(C), i8 gear with 3 bytes padding):
const IR_OFF_SESSION_TIME: usize = 0; // f32
// IR_OFF_SESSION_FLAGS = 4 (not used in these golden tests)
const IR_OFF_SPEED: usize = 8; // f32, m/s
const IR_OFF_RPM: usize = 12; // f32
const IR_OFF_GEAR: usize = 16; // i8  (padding 17..20)
const IR_OFF_THROTTLE: usize = 20; // f32
const IR_OFF_BRAKE: usize = 24; // f32
const IR_OFF_STEER_ANGLE: usize = 28; // f32
// steer torque/force/limiter fields: offsets 32..48 (not used in these golden tests)
// slip ratio + tire rps fields: offsets 48..80 (not used in these golden tests)
const IR_OFF_LAP_CURRENT: usize = 80; // i32
const IR_OFF_LAP_BEST_TIME: usize = 84; // f32
// IR_OFF_FUEL_LEVEL = 88 (not used in these golden tests)
const IR_OFF_FUEL_LEVEL_PCT: usize = 92; // f32
const IR_OFF_ON_PIT_ROAD: usize = 96; // i32
const IR_OFF_CLUTCH: usize = 100; // f32
// IR_OFF_CAR_POSITION = 104 (not used in these golden tests)
const IR_OFF_LAP_LAST_TIME: usize = 108; // f32
const IR_OFF_LAP_CURRENT_TIME: usize = 112; // f32
const IR_OFF_LF_TEMP: usize = 116; // f32
const IR_OFF_RF_TEMP: usize = 120; // f32
const IR_OFF_LR_TEMP: usize = 124; // f32
const IR_OFF_RR_TEMP: usize = 128; // f32
// tire pressure fields: offsets 132..148 (not used in these golden tests)
const IR_OFF_LAT_ACCEL: usize = 148; // f32
const IR_OFF_LONG_ACCEL: usize = 152; // f32
// IR_OFF_VERT_ACCEL = 156 (not used in these golden tests)
const IR_OFF_WATER_TEMP: usize = 160; // f32
const IR_OFF_CAR_PATH: usize = 164; // [u8; 64]
const IR_OFF_TRACK_NAME: usize = 228; // [u8; 64]
const IR_DATA_SIZE: usize = 292;

fn make_iracing_golden() -> Vec<u8> {
    let mut buf = vec![0u8; IR_DATA_SIZE];
    write_f32_le(&mut buf, IR_OFF_SESSION_TIME, 120.5);
    write_f32_le(&mut buf, IR_OFF_SPEED, 55.0);
    write_f32_le(&mut buf, IR_OFF_RPM, 7200.0);
    buf[IR_OFF_GEAR] = 4_i8 as u8;
    write_f32_le(&mut buf, IR_OFF_THROTTLE, 0.85);
    write_f32_le(&mut buf, IR_OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, IR_OFF_STEER_ANGLE, -0.25);
    write_f32_le(&mut buf, IR_OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, IR_OFF_FUEL_LEVEL_PCT, 0.62);
    write_f32_le(&mut buf, IR_OFF_LAP_BEST_TIME, 91.234);
    write_f32_le(&mut buf, IR_OFF_LAP_LAST_TIME, 92.100);
    write_f32_le(&mut buf, IR_OFF_LAP_CURRENT_TIME, 45.5);
    write_i32_le(&mut buf, IR_OFF_LAP_CURRENT, 5);
    write_f32_le(&mut buf, IR_OFF_LAT_ACCEL, 9.81); // ~1G lateral
    write_f32_le(&mut buf, IR_OFF_LONG_ACCEL, 4.905); // ~0.5G
    write_f32_le(&mut buf, IR_OFF_WATER_TEMP, 92.0);
    write_f32_le(&mut buf, IR_OFF_LF_TEMP, 85.0);
    write_f32_le(&mut buf, IR_OFF_RF_TEMP, 87.0);
    write_f32_le(&mut buf, IR_OFF_LR_TEMP, 83.0);
    write_f32_le(&mut buf, IR_OFF_RR_TEMP, 84.0);
    // Car name: "gt3_mclaren\0"
    let car = b"gt3_mclaren\0";
    buf[IR_OFF_CAR_PATH..IR_OFF_CAR_PATH + car.len()].copy_from_slice(car);
    // Track name: "spa\0"
    let track = b"spa\0";
    buf[IR_OFF_TRACK_NAME..IR_OFF_TRACK_NAME + track.len()].copy_from_slice(track);
    buf
}

#[test]
fn golden_iracing_full_packet() -> TestResult {
    let adapter = IRacingAdapter::new();
    let data = make_iracing_golden();
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 55.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 7200.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 4, "gear: {}", t.gear);
    assert!((t.throttle - 0.85).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    assert!(
        (t.steering_angle - (-0.25)).abs() < 0.01,
        "steering_angle: {}",
        t.steering_angle
    );
    assert!(
        (t.fuel_percent - 0.62).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert_eq!(t.car_id.as_deref(), Some("gt3_mclaren"));
    assert_eq!(t.track_id.as_deref(), Some("spa"));
    // lat_accel * (1/9.80665) ≈ 1.0 G
    assert!(
        (t.lateral_g - 1.0).abs() < 0.02,
        "lateral_g: {}",
        t.lateral_g
    );
    assert!(
        (t.engine_temp_c - 92.0).abs() < 0.1,
        "engine_temp_c: {}",
        t.engine_temp_c
    );
    Ok(())
}

#[test]
fn golden_iracing_reverse_gear() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut data = vec![0u8; IR_DATA_SIZE];
    // gear = -1 as i8
    data[IR_OFF_GEAR] = (-1_i8) as u8;
    write_f32_le(&mut data, IR_OFF_RPM, 1500.0);
    write_f32_le(&mut data, IR_OFF_SPEED, 2.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "expected reverse gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_iracing_pit_flag() -> TestResult {
    let adapter = IRacingAdapter::new();
    let mut data = vec![0u8; IR_DATA_SIZE];
    write_i32_le(&mut data, IR_OFF_ON_PIT_ROAD, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.flags.in_pits, "expected in_pits flag");
    Ok(())
}

#[test]
fn golden_iracing_all_zeros() -> TestResult {
    let adapter = IRacingAdapter::new();
    let data = vec![0u8; IR_DATA_SIZE];
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms).abs() < 0.01);
    assert!((t.rpm).abs() < 0.01);
    assert_eq!(t.gear, 0);
    assert!((t.throttle).abs() < 0.01);
    Ok(())
}

#[test]
fn golden_iracing_short_packet_rejected() -> TestResult {
    let adapter = IRacingAdapter::new();
    // A buffer smaller than IRacingLegacyData should be rejected
    let data = vec![0u8; 50];
    assert!(
        adapter.normalize(&data).is_err(),
        "expected error for short iRacing packet"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ACC (Assetto Corsa Competizione) — Broadcasting protocol v4
// ═══════════════════════════════════════════════════════════════════════════════

/// Write a minimal ACC LapInfo sub-struct (0 splits).
fn push_acc_lap(buf: &mut Vec<u8>, lap_time_ms: i32) {
    buf.extend_from_slice(&lap_time_ms.to_le_bytes()); // lap_time_ms
    buf.extend_from_slice(&1u16.to_le_bytes()); // car_index
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(0); // split_count = 0
    buf.push(0); // is_invalid
    buf.push(1); // is_valid_for_best
    buf.push(0); // is_outlap
    buf.push(0); // is_inlap
}

fn make_acc_realtime_car_update(
    car_index: u16,
    gear_raw: u8,
    speed_kmh: u16,
    position: u16,
    laps: u16,
    best_lap_ms: i32,
    last_lap_ms: i32,
    current_lap_ms: i32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.push(3); // MSG_REALTIME_CAR_UPDATE
    buf.extend_from_slice(&car_index.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(1); // driver_count
    buf.push(gear_raw);
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_x
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_y
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
    buf.push(1); // car_location = 1 (on track)
    buf.extend_from_slice(&speed_kmh.to_le_bytes());
    buf.extend_from_slice(&position.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // cup_position
    buf.extend_from_slice(&0u16.to_le_bytes()); // track_position
    buf.extend_from_slice(&0.45f32.to_le_bytes()); // spline_position
    buf.extend_from_slice(&laps.to_le_bytes());
    buf.extend_from_slice(&(-200i32).to_le_bytes()); // delta_ms
    push_acc_lap(&mut buf, best_lap_ms);
    push_acc_lap(&mut buf, last_lap_ms);
    push_acc_lap(&mut buf, current_lap_ms);
    buf
}

#[test]
fn golden_acc_realtime_car_update() -> TestResult {
    let adapter = ACCAdapter::new();
    // gear_raw=5 → 5-1=4th gear; speed 180 km/h → 50 m/s
    let packet = make_acc_realtime_car_update(3, 5, 180, 2, 8, 92_345, 93_100, 41_000);
    let t = adapter.normalize(&packet)?;

    assert!(
        (t.speed_ms - 50.0).abs() < 0.1,
        "speed_ms: {} (expected ~50.0)",
        t.speed_ms
    );
    assert_eq!(t.gear, 4, "gear: {}", t.gear);
    assert_eq!(t.position, 2, "position: {}", t.position);
    assert_eq!(t.lap, 8, "lap: {}", t.lap);
    // best_lap_time_s = 92345 / 1000 = 92.345
    assert!(
        (t.best_lap_time_s - 92.345).abs() < 0.01,
        "best_lap_time_s: {}",
        t.best_lap_time_s
    );
    assert!(
        (t.last_lap_time_s - 93.1).abs() < 0.01,
        "last_lap_time_s: {}",
        t.last_lap_time_s
    );
    assert_eq!(t.car_id.as_deref(), Some("car_3"));
    Ok(())
}

#[test]
fn golden_acc_reverse_gear() -> TestResult {
    let adapter = ACCAdapter::new();
    // gear_raw=0 → 0-1=-1 (reverse)
    let packet = make_acc_realtime_car_update(0, 0, 5, 1, 0, -1, -1, 0);
    let t = adapter.normalize(&packet)?;
    assert_eq!(t.gear, -1, "expected reverse gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_acc_neutral_gear() -> TestResult {
    let adapter = ACCAdapter::new();
    // gear_raw=1 → 1-1=0 (neutral)
    let packet = make_acc_realtime_car_update(0, 1, 0, 1, 0, -1, -1, 0);
    let t = adapter.normalize(&packet)?;
    assert_eq!(t.gear, 0, "expected neutral gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_acc_pit_lane_flag() -> TestResult {
    let adapter = ACCAdapter::new();
    let mut packet = make_acc_realtime_car_update(0, 3, 60, 5, 3, 95_000, 96_000, 10_000);
    // car_location is at a fixed offset: 1(msg_type) + 2 + 2 + 1 + 1 + 4 + 4 + 4 = 19
    packet[19] = 2; // pitlane
    let t = adapter.normalize(&packet)?;
    assert!(t.flags.in_pits, "expected in_pits flag for car_location=2");
    assert!(
        t.flags.pit_limiter,
        "expected pit_limiter flag for car_location=2"
    );
    Ok(())
}

#[test]
fn golden_acc_short_packet_rejected() -> TestResult {
    let adapter = ACCAdapter::new();
    let packet = vec![3u8, 0, 0]; // too short for car update
    assert!(
        adapter.normalize(&packet).is_err(),
        "expected error for short ACC packet"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// BeamNG.drive — OutGauge UDP (92 bytes)
// ═══════════════════════════════════════════════════════════════════════════════

// OutGauge byte offsets
const OG_OFF_GEAR: usize = 10;
const OG_OFF_SPEED: usize = 12;
const OG_OFF_RPM: usize = 16;
const OG_OFF_TURBO: usize = 20;
const OG_OFF_ENG_TEMP: usize = 24;
const OG_OFF_FUEL: usize = 28;
const OG_OFF_SHOW_LIGHTS: usize = 44;
const OG_OFF_THROTTLE: usize = 48;
const OG_OFF_BRAKE: usize = 52;
const OG_OFF_CLUTCH: usize = 56;
const OG_PACKET_SIZE: usize = 92;

fn make_beamng_golden() -> Vec<u8> {
    let mut buf = vec![0u8; OG_PACKET_SIZE];
    write_f32_le(&mut buf, OG_OFF_SPEED, 25.0); // 25 m/s ≈ 90 km/h
    write_f32_le(&mut buf, OG_OFF_RPM, 4500.0);
    buf[OG_OFF_GEAR] = 4; // OutGauge: 4 = 3rd gear (normalised as 3)
    write_f32_le(&mut buf, OG_OFF_THROTTLE, 0.7);
    write_f32_le(&mut buf, OG_OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, OG_OFF_CLUTCH, 0.05);
    write_f32_le(&mut buf, OG_OFF_FUEL, 0.65);
    write_f32_le(&mut buf, OG_OFF_ENG_TEMP, 88.0);
    write_f32_le(&mut buf, OG_OFF_TURBO, 0.8);
    buf
}

#[test]
fn golden_beamng_full_packet() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let data = make_beamng_golden();
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 25.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 4500.0).abs() < 0.01, "rpm: {}", t.rpm);
    // OutGauge gear 4 → normalised 3 (4-1=3)
    assert_eq!(t.gear, 3, "gear: {}", t.gear);
    assert!((t.throttle - 0.7).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    assert!((t.clutch - 0.05).abs() < 0.01, "clutch: {}", t.clutch);
    assert!(
        (t.fuel_percent - 0.65).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert!(
        (t.engine_temp_c - 88.0).abs() < 0.1,
        "engine_temp_c: {}",
        t.engine_temp_c
    );
    Ok(())
}

#[test]
fn golden_beamng_reverse_gear() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let mut data = vec![0u8; OG_PACKET_SIZE];
    data[OG_OFF_GEAR] = 0; // OutGauge: 0 = Reverse
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "expected reverse gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_beamng_neutral_gear() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let mut data = vec![0u8; OG_PACKET_SIZE];
    data[OG_OFF_GEAR] = 1; // OutGauge: 1 = Neutral
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0, "expected neutral gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_beamng_dashboard_flags() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let mut data = vec![0u8; OG_PACKET_SIZE];
    data[OG_OFF_GEAR] = 3; // 2nd gear
    // DL_TC=0x0010, DL_ABS=0x0400
    write_u32_le(&mut data, OG_OFF_SHOW_LIGHTS, 0x0010 | 0x0400);
    let t = adapter.normalize(&data)?;
    assert!(t.flags.traction_control, "expected TC flag");
    assert!(t.flags.abs_active, "expected ABS flag");
    assert!(!t.flags.pit_limiter, "pit_limiter should be false");
    Ok(())
}

#[test]
fn golden_beamng_all_zeros() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let data = vec![0u8; OG_PACKET_SIZE];
    let t = adapter.normalize(&data)?;
    // gear 0 → Reverse in OutGauge
    assert_eq!(t.gear, -1, "all-zeros OutGauge gear=0 → reverse");
    assert!((t.speed_ms).abs() < 0.01);
    assert!((t.rpm).abs() < 0.01);
    Ok(())
}

#[test]
fn golden_beamng_short_packet_rejected() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let data = vec![0u8; 50];
    assert!(
        adapter.normalize(&data).is_err(),
        "expected error for short BeamNG packet"
    );
    Ok(())
}

#[test]
fn golden_beamng_96_byte_packet_accepted() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let mut data = vec![0u8; 96]; // 92 + 4 byte optional id field
    data[OG_OFF_GEAR] = 3; // 2nd gear
    write_f32_le(&mut data, OG_OFF_SPEED, 30.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 2);
    assert!((t.speed_ms - 30.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// DiRT Rally 2.0 — Codemasters Mode 1 (264 bytes)
// ═══════════════════════════════════════════════════════════════════════════════

const CM_PACKET_SIZE: usize = 264;
const CM_OFF_WHEEL_SPEED_RL: usize = 100;
const CM_OFF_WHEEL_SPEED_RR: usize = 104;
const CM_OFF_WHEEL_SPEED_FL: usize = 108;
const CM_OFF_WHEEL_SPEED_FR: usize = 112;
const CM_OFF_THROTTLE: usize = 116;
const CM_OFF_STEER: usize = 120;
const CM_OFF_BRAKE: usize = 124;
const CM_OFF_GEAR: usize = 132;
const CM_OFF_GFORCE_LAT: usize = 136;
const CM_OFF_RPM: usize = 148;
const CM_OFF_FUEL_IN_TANK: usize = 180;
const CM_OFF_FUEL_CAPACITY: usize = 184;
const CM_OFF_MAX_RPM: usize = 252;
const CM_OFF_MAX_GEARS: usize = 260;

fn make_dirt_rally2_golden() -> Vec<u8> {
    let mut buf = vec![0u8; CM_PACKET_SIZE];
    // Speed via all 4 wheel speeds = 35 m/s
    write_f32_le(&mut buf, CM_OFF_WHEEL_SPEED_FL, 35.0);
    write_f32_le(&mut buf, CM_OFF_WHEEL_SPEED_FR, 35.0);
    write_f32_le(&mut buf, CM_OFF_WHEEL_SPEED_RL, 35.0);
    write_f32_le(&mut buf, CM_OFF_WHEEL_SPEED_RR, 35.0);
    write_f32_le(&mut buf, CM_OFF_RPM, 6500.0);
    write_f32_le(&mut buf, CM_OFF_MAX_RPM, 8000.0);
    write_f32_le(&mut buf, CM_OFF_GEAR, 4.0); // 4th gear
    write_f32_le(&mut buf, CM_OFF_THROTTLE, 0.9);
    write_f32_le(&mut buf, CM_OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, CM_OFF_STEER, 0.1);
    write_f32_le(&mut buf, CM_OFF_GFORCE_LAT, 0.6);
    write_f32_le(&mut buf, CM_OFF_FUEL_IN_TANK, 30.0);
    write_f32_le(&mut buf, CM_OFF_FUEL_CAPACITY, 60.0);
    write_f32_le(&mut buf, CM_OFF_MAX_GEARS, 6.0);
    buf
}

#[test]
fn golden_dirt_rally2_full_packet() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let data = make_dirt_rally2_golden();
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 35.0).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 6500.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert!((t.max_rpm - 8000.0).abs() < 0.01, "max_rpm: {}", t.max_rpm);
    assert_eq!(t.gear, 4, "gear: {}", t.gear);
    assert!((t.throttle - 0.9).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    assert!(
        (t.steering_angle - 0.1).abs() < 0.01,
        "steering_angle: {}",
        t.steering_angle
    );
    // FFB = lat_g / 3.0 = 0.6 / 3.0 = 0.2
    assert!(
        (t.ffb_scalar - 0.2).abs() < 0.01,
        "ffb_scalar: {}",
        t.ffb_scalar
    );
    // fuel_percent = 30/60 = 0.5
    assert!(
        (t.fuel_percent - 0.5).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert_eq!(t.num_gears, 6, "num_gears: {}", t.num_gears);
    Ok(())
}

#[test]
fn golden_dirt_rally2_reverse_gear() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut data = vec![0u8; CM_PACKET_SIZE];
    // Gear 0.0 → reverse
    write_f32_le(&mut data, CM_OFF_GEAR, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "expected reverse gear, got {}", t.gear);
    Ok(())
}

#[test]
fn golden_dirt_rally2_all_zeros() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let data = vec![0u8; CM_PACKET_SIZE];
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms).abs() < 0.01);
    assert!((t.rpm).abs() < 0.01);
    // zero gear maps to reverse in codemasters format
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn golden_dirt_rally2_short_packet_rejected() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let data = vec![0u8; CM_PACKET_SIZE - 1];
    assert!(
        adapter.normalize(&data).is_err(),
        "expected error for short Dirt Rally 2 packet"
    );
    Ok(())
}

#[test]
fn golden_dirt_rally2_max_lateral_g() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut data = vec![0u8; CM_PACKET_SIZE];
    // Very high lateral G → FFB clamped to [-1, 1]
    write_f32_le(&mut data, CM_OFF_GFORCE_LAT, 10.0);
    let t = adapter.normalize(&data)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar out of [-1,1]: {}",
        t.ffb_scalar
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ETS2 (Euro Truck Simulator 2) — SCS Telemetry SDK shared memory
// ═══════════════════════════════════════════════════════════════════════════════

const SCS_OFF_VERSION: usize = 0;
const SCS_OFF_SPEED_MS: usize = 4;
const SCS_OFF_ENGINE_RPM: usize = 8;
const SCS_OFF_GEAR: usize = 12;
const SCS_OFF_FUEL_RATIO: usize = 16;
const SCS_OFF_ENGINE_LOAD: usize = 20;
const SCS_OFF_THROTTLE: usize = 24;
const SCS_OFF_BRAKE: usize = 28;
const SCS_OFF_CLUTCH: usize = 32;
const SCS_OFF_STEERING: usize = 36;
const SCS_OFF_ENGINE_TEMP_C: usize = 40;
const SCS_OFF_MAX_RPM: usize = 44;
const SCS_MEMORY_SIZE: usize = 512;

fn make_ets2_golden() -> Vec<u8> {
    let mut buf = vec![0u8; SCS_MEMORY_SIZE];
    write_u32_le(&mut buf, SCS_OFF_VERSION, 1); // expected version
    write_f32_le(&mut buf, SCS_OFF_SPEED_MS, 22.2); // ~80 km/h
    write_f32_le(&mut buf, SCS_OFF_ENGINE_RPM, 1800.0);
    write_i32_le(&mut buf, SCS_OFF_GEAR, 8); // 8th gear (truck has many gears)
    write_f32_le(&mut buf, SCS_OFF_FUEL_RATIO, 0.72);
    write_f32_le(&mut buf, SCS_OFF_ENGINE_LOAD, 0.55);
    write_f32_le(&mut buf, SCS_OFF_THROTTLE, 0.6);
    write_f32_le(&mut buf, SCS_OFF_BRAKE, 0.0);
    write_f32_le(&mut buf, SCS_OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, SCS_OFF_STEERING, 0.15);
    write_f32_le(&mut buf, SCS_OFF_ENGINE_TEMP_C, 88.0);
    write_f32_le(&mut buf, SCS_OFF_MAX_RPM, 2500.0);
    buf
}

#[test]
fn golden_ets2_full_packet() -> TestResult {
    let data = make_ets2_golden();
    let t = ets2::parse_scs_packet(&data)?;

    assert!((t.speed_ms - 22.2).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 1800.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 8, "gear: {}", t.gear);
    assert!(
        (t.fuel_percent - 0.72).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    assert!((t.throttle - 0.6).abs() < 0.01, "throttle: {}", t.throttle);
    assert!((t.brake).abs() < 0.01, "brake: {}", t.brake);
    assert!(
        (t.engine_temp_c - 88.0).abs() < 0.1,
        "engine_temp_c: {}",
        t.engine_temp_c
    );
    assert!((t.max_rpm - 2500.0).abs() < 0.01, "max_rpm: {}", t.max_rpm);
    // Steering: 0.15 * 0.6109 ≈ 0.0916
    assert!(
        (t.steering_angle - 0.0916).abs() < 0.01,
        "steering_angle: {}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn golden_ets2_reverse_gear() -> TestResult {
    let mut data = make_ets2_golden();
    write_i32_le(&mut data, SCS_OFF_GEAR, -1);
    let t = ets2::parse_scs_packet(&data)?;
    assert_eq!(t.gear, -1, "expected reverse gear");
    Ok(())
}

#[test]
fn golden_ets2_neutral_gear() -> TestResult {
    let mut data = make_ets2_golden();
    write_i32_le(&mut data, SCS_OFF_GEAR, 0);
    let t = ets2::parse_scs_packet(&data)?;
    assert_eq!(t.gear, 0, "expected neutral gear");
    Ok(())
}

#[test]
fn golden_ets2_wrong_version_rejected() -> TestResult {
    let mut data = make_ets2_golden();
    write_u32_le(&mut data, SCS_OFF_VERSION, 99);
    assert!(
        ets2::parse_scs_packet(&data).is_err(),
        "expected error for wrong SCS version"
    );
    Ok(())
}

#[test]
fn golden_ets2_short_buffer_rejected() -> TestResult {
    let data = vec![0u8; 10];
    assert!(
        ets2::parse_scs_packet(&data).is_err(),
        "expected error for short SCS buffer"
    );
    Ok(())
}

#[test]
fn golden_ets2_ffb_scalar_in_range() -> TestResult {
    let data = make_ets2_golden();
    let t = ets2::parse_scs_packet(&data)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar out of [-1,1]: {}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn golden_ets2_all_zeros_with_valid_version() -> TestResult {
    let mut data = vec![0u8; SCS_MEMORY_SIZE];
    write_u32_le(&mut data, SCS_OFF_VERSION, 1);
    let t = ets2::parse_scs_packet(&data)?;
    assert!((t.speed_ms).abs() < 0.01);
    assert!((t.rpm).abs() < 0.01);
    assert_eq!(t.gear, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ATS (American Truck Simulator) — same SCS format as ETS2
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_ats_adapter_game_id() -> TestResult {
    let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
    assert_eq!(adapter.game_id(), "ats");
    Ok(())
}

#[test]
fn golden_ats_normalize_delegates_to_scs() -> TestResult {
    let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
    let data = make_ets2_golden();
    let t = adapter.normalize(&data)?;

    // ATS uses same parser as ETS2; verify key fields
    assert!((t.speed_ms - 22.2).abs() < 0.01, "speed_ms: {}", t.speed_ms);
    assert!((t.rpm - 1800.0).abs() < 0.01, "rpm: {}", t.rpm);
    assert_eq!(t.gear, 8, "gear: {}", t.gear);
    assert!(
        (t.fuel_percent - 0.72).abs() < 0.01,
        "fuel_percent: {}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn golden_ats_reverse_gear() -> TestResult {
    let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
    let mut data = make_ets2_golden();
    write_i32_le(&mut data, SCS_OFF_GEAR, -1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "expected reverse gear");
    Ok(())
}

#[test]
fn golden_ats_short_buffer_rejected() -> TestResult {
    let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
    let data = vec![0u8; 10];
    assert!(
        adapter.normalize(&data).is_err(),
        "expected error for short ATS buffer"
    );
    Ok(())
}
