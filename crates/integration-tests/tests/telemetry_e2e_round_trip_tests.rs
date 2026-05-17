//! End-to-end telemetry round-trip tests for high-priority games.
//!
//! Each test constructs a realistic raw byte buffer with known field values,
//! feeds it through the adapter's parse/normalize pipeline, and asserts that
//! the resulting `NormalizedTelemetry` fields match expectations.

use openracing_telemetry_adapters::{TelemetryAdapter, adapter_factories};
use racing_wheel_schemas::telemetry::TelemetryValue;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u16_le(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

/// Assert a float is within tolerance of an expected value.
fn assert_f32_near(actual: f32, expected: f32, tol: f32, label: &str) {
    assert!(
        (actual - expected).abs() < tol,
        "{label}: expected ~{expected}, got {actual} (tol={tol})"
    );
}

/// Extract a float from the NormalizedTelemetry extended map.
fn extended_float(
    extended: &std::collections::BTreeMap<String, TelemetryValue>,
    key: &str,
) -> Option<f32> {
    match extended.get(key) {
        Some(TelemetryValue::Float(v)) => Some(*v),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Assetto Corsa Competizione (ACC) — Broadcasting SDK RealtimeCarUpdate
// ═══════════════════════════════════════════════════════════════════════════════

/// Build an ACC RealtimeCarUpdate packet (message type 3).
///
/// Layout follows `parse_realtime_car_update` in the ACC adapter:
///   u8  msg_type | u16 car_index | u16 driver_index | u8 driver_count |
///   u8  gear_raw | f32 world_pos_x | f32 world_pos_y | f32 yaw |
///   u8  car_location | u16 speed_kmh | u16 position | u16 cup_position |
///   u16 track_position | f32 spline_position | u16 laps | i32 delta_ms |
///   3 × lap_time_ms entries (each: i32 + u16 + u16 + u8(0 splits) + 4×u8 flags)
fn build_acc_realtime_car_update(
    speed_kmh: u16,
    gear_raw: u8,
    position: u16,
    laps: u16,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(80);

    buf.push(3); // MSG_REALTIME_CAR_UPDATE
    buf.extend_from_slice(&0u16.to_le_bytes()); // car_index = 0
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(1); // driver_count
    buf.push(gear_raw); // gear (0=R, 1=N, 2=1st, ...)
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_x
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_y
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
    buf.push(0); // car_location = track
    buf.extend_from_slice(&speed_kmh.to_le_bytes());
    buf.extend_from_slice(&position.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // cup_position
    buf.extend_from_slice(&0u16.to_le_bytes()); // track_position
    buf.extend_from_slice(&0.5f32.to_le_bytes()); // spline_position
    buf.extend_from_slice(&laps.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes()); // delta_ms

    // 3 × lap_time_ms entries (0 splits each)
    for _ in 0..3 {
        buf.extend_from_slice(&(-1i32).to_le_bytes()); // lap_time_ms = -1 (no time)
        buf.extend_from_slice(&0u16.to_le_bytes()); // car_index
        buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
        buf.push(0); // split_count = 0
        buf.push(0); // is_invalid
        buf.push(0); // is_valid_for_best
        buf.push(0); // is_outlap
        buf.push(0); // is_inlap
    }

    buf
}

#[test]
fn acc_realtime_car_update_round_trips_speed_and_gear() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("acc")?;

    // ACC gear encoding: 0=Reverse, 1=Neutral, 2=1st, 5=4th
    // Normalized: gear_raw - 1 → -1, 0, 1, 4
    let packet = build_acc_realtime_car_update(180, 5, 3, 7);
    let telem = adapter.normalize(&packet)?;

    // speed_kmh=180 → speed_ms = 180/3.6 = 50.0
    assert_f32_near(telem.speed_ms, 50.0, 0.5, "ACC speed");
    assert_eq!(telem.gear, 4, "ACC gear_raw=5 should map to 4th gear");

    Ok(())
}

#[test]
fn acc_reverse_gear_maps_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("acc")?;

    let packet = build_acc_realtime_car_update(10, 0, 1, 0);
    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, -1, "ACC gear_raw=0 should map to reverse (-1)");
    assert_f32_near(telem.speed_ms, 10.0 / 3.6, 0.5, "ACC speed in reverse");

    Ok(())
}

#[test]
fn acc_neutral_gear_maps_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("acc")?;

    let packet = build_acc_realtime_car_update(0, 1, 1, 0);
    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, 0, "ACC gear_raw=1 should map to neutral (0)");

    Ok(())
}

#[test]
fn acc_rejects_empty_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("acc")?;
    let result = adapter.normalize(&[]);
    assert!(result.is_err(), "ACC must reject empty packets");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// iRacing — shared-memory struct (IRacingData / IRacingLegacyData repr(C))
// ═══════════════════════════════════════════════════════════════════════════════
//
// IRacingData layout (repr(C)):
//   offset  0: session_time    f32
//   offset  4: session_flags   u32
//   offset  8: speed           f32
//   offset 12: rpm             f32
//   offset 16: gear            i8  (+ 3 bytes padding)
//   offset 20: throttle        f32
//   offset 24: brake           f32
//   offset 28: steering_wheel_angle       f32
//   offset 32: steering_wheel_torque      f32
//   offset 36: steering_wheel_pct_torque_sign  f32
//   offset 40: steering_wheel_max_force_nm     f32
//   offset 44: steering_wheel_limiter          f32
//   offset 48..64: tire slip ratios (4×f32)
//   offset 64..80: tire rps (4×f32)
//   offset 80: lap_current     i32
//   offset 84: lap_best_time   f32
//   offset 88: fuel_level      f32
//   offset 92: on_pit_road     i32
//   offset 96..160: car_path   [u8; 64]
//   offset 160..224: track_name [u8; 64]
// Total: 224 bytes

const IRACING_DATA_SIZE: usize = 224;

fn build_iracing_packet(speed: f32, rpm: f32, gear: i8, throttle: f32, brake: f32) -> Vec<u8> {
    let mut buf = vec![0u8; IRACING_DATA_SIZE];

    write_f32_le(&mut buf, 0, 120.0); // session_time
    write_u32_le(&mut buf, 4, 0); // session_flags (no flags)
    write_f32_le(&mut buf, 8, speed);
    write_f32_le(&mut buf, 12, rpm);
    buf[16] = gear as u8;
    write_f32_le(&mut buf, 20, throttle);
    write_f32_le(&mut buf, 24, brake);
    write_f32_le(&mut buf, 28, 0.1); // steering_wheel_angle
    write_i32_le(&mut buf, 80, 5); // lap_current
    write_f32_le(&mut buf, 84, 92.5); // lap_best_time
    write_f32_le(&mut buf, 88, 30.0); // fuel_level

    buf
}

#[test]
fn iracing_packet_round_trips_speed_rpm_gear() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("iracing")?;

    let packet = build_iracing_packet(55.0, 7200.0, 4, 0.85, 0.0);
    let telem = adapter.normalize(&packet)?;

    assert_f32_near(telem.speed_ms, 55.0, 0.5, "iRacing speed");
    assert_f32_near(telem.rpm, 7200.0, 1.0, "iRacing RPM");
    assert_eq!(telem.gear, 4, "iRacing gear should be 4th");

    Ok(())
}

#[test]
fn iracing_packet_round_trips_controls() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("iracing")?;

    let packet = build_iracing_packet(0.0, 850.0, 0, 0.0, 1.0);
    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, 0, "iRacing gear=0 should be neutral");
    // iRacing stores throttle/brake in extended map
    let ext_throttle = extended_float(&telem.extended, "throttle").unwrap_or(telem.throttle);
    let ext_brake = extended_float(&telem.extended, "brake").unwrap_or(telem.brake);
    assert_f32_near(ext_throttle, 0.0, 0.01, "iRacing zero throttle");
    assert_f32_near(ext_brake, 1.0, 0.01, "iRacing full brake");

    Ok(())
}

#[test]
fn iracing_reverse_gear_maps_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("iracing")?;

    // iRacing: gear = -1 (0xFF as i8) for reverse
    let packet = build_iracing_packet(5.0, 2000.0, -1, 0.3, 0.0);
    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, -1, "iRacing gear=-1 should be reverse");

    Ok(())
}

#[test]
fn iracing_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("iracing")?;
    let short = [0u8; 50];
    let result = adapter.normalize(&short);
    assert!(result.is_err(), "iRacing must reject undersized packets");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gran Turismo 7 — 296-byte decrypted packet parsed by parse_decrypted()
// ═══════════════════════════════════════════════════════════════════════════════
//
// The adapter's normalize() performs Salsa20 decryption first, so we test
// parse_decrypted() directly to verify the field extraction logic on known
// plaintext payloads (as requested: "test decrypted payload parsing").

use openracing_telemetry_adapters::gran_turismo_7::{
    MAGIC, OFF_MAGIC, PACKET_SIZE, parse_decrypted,
};

fn build_gt7_decrypted_packet(
    rpm: f32,
    speed_ms: f32,
    gear: u8,
    throttle: u8,
    brake: u8,
) -> [u8; PACKET_SIZE] {
    let mut buf = [0u8; PACKET_SIZE];

    // Magic bytes at offset 0x00
    buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());

    // Engine RPM at 0x3C
    write_f32_le(&mut buf, 0x3C, rpm);
    // Fuel level at 0x44
    write_f32_le(&mut buf, 0x44, 30.0);
    // Fuel capacity at 0x48
    write_f32_le(&mut buf, 0x48, 60.0);
    // Speed m/s at 0x4C
    write_f32_le(&mut buf, 0x4C, speed_ms);
    // Water temp at 0x58
    write_f32_le(&mut buf, 0x58, 92.0);
    // Tire temps (FL/FR/RL/RR) at 0x60..0x6C
    write_f32_le(&mut buf, 0x60, 85.0);
    write_f32_le(&mut buf, 0x64, 87.0);
    write_f32_le(&mut buf, 0x68, 80.0);
    write_f32_le(&mut buf, 0x6C, 82.0);
    // Lap count at 0x74 (i16 LE)
    buf[0x74] = 3;
    buf[0x75] = 0;
    // Best lap ms at 0x78
    write_i32_le(&mut buf, 0x78, 92_500); // 92.5 seconds
    // Last lap ms at 0x7C
    write_i32_le(&mut buf, 0x7C, 93_100); // 93.1 seconds
    // Max alert RPM at 0x8A (i16 LE)
    write_u16_le(&mut buf, 0x8A, 8500);
    // Gear byte at 0x90 (low nibble = gear)
    buf[0x90] = gear;
    // Throttle at 0x91, Brake at 0x92
    buf[0x91] = throttle;
    buf[0x92] = brake;
    // Car code at 0x124
    write_i32_le(&mut buf, 0x124, 42);

    buf
}

#[test]
fn gt7_decrypted_packet_round_trips_core_fields() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_gt7_decrypted_packet(7200.0, 55.5, 4, 200, 0);
    let telem = parse_decrypted(&buf)?;

    assert_f32_near(telem.rpm, 7200.0, 1.0, "GT7 RPM");
    assert_f32_near(telem.max_rpm, 8500.0, 1.0, "GT7 max RPM");
    assert_f32_near(telem.speed_ms, 55.5, 0.1, "GT7 speed");
    assert_eq!(telem.gear, 4, "GT7 gear nibble=4 should be 4th");

    // Throttle 200/255 ≈ 0.784
    assert_f32_near(telem.throttle, 200.0 / 255.0, 0.01, "GT7 throttle");
    assert_f32_near(telem.brake, 0.0, 0.01, "GT7 brake off");

    Ok(())
}

#[test]
fn gt7_decrypted_packet_round_trips_temperatures_and_laps() -> Result<(), Box<dyn std::error::Error>>
{
    let buf = build_gt7_decrypted_packet(5000.0, 30.0, 3, 128, 64);
    let telem = parse_decrypted(&buf)?;

    // Tire temps (f32 → u8 clamped) FL=85, FR=87, RL=80, RR=82
    assert_eq!(telem.tire_temps_c, [85, 87, 80, 82], "GT7 tire temps");

    // Fuel: 30/60 = 0.5
    assert_f32_near(telem.fuel_percent, 0.5, 0.01, "GT7 fuel percent");
    // Engine temp
    assert_f32_near(telem.engine_temp_c, 92.0, 0.5, "GT7 water temp");

    assert_eq!(telem.lap, 3, "GT7 lap count");
    assert_f32_near(telem.best_lap_time_s, 92.5, 0.01, "GT7 best lap");
    assert_f32_near(telem.last_lap_time_s, 93.1, 0.01, "GT7 last lap");

    Ok(())
}

#[test]
fn gt7_decrypted_packet_neutral_gear() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_gt7_decrypted_packet(800.0, 0.0, 0, 0, 0);
    let telem = parse_decrypted(&buf)?;

    assert_eq!(telem.gear, 0, "GT7 gear=0 should be neutral");
    assert_f32_near(telem.speed_ms, 0.0, 0.01, "GT7 speed at standstill");

    Ok(())
}

#[test]
fn gt7_adapter_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("gran_turismo_7")?;
    let short = [0u8; 100];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "GT7 adapter must reject packets shorter than 296 bytes"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// F1 25 / EA F1 — CarTelemetry packet (packet ID 6)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Header layout (29 bytes):
//   0..2:  packet_format (u16 LE) = 2025
//   2..6:  gameYear, majorVersion, minorVersion, packetVersion
//   6:     packet_id = 6 (CAR_TELEMETRY)
//   7..15: sessionUID
//  15..19: sessionTime
//  19..23: frameIdentifier
//  23..27: overallFrameIdentifier
//  27:    player_car_index
//  28:    secondaryPlayerCarIndex
//
// Per-car telemetry entry (60 bytes) starting at HEADER + car_index*60:
//   0..2:  speed_kmh       (u16 LE)
//   2..6:  throttle        (f32 LE)
//   6..10: steer           (f32 LE)
//  10..14: brake           (f32 LE)
//  14:    clutch           (u8)
//  15:    gear             (i8)
//  16..18: engine_rpm      (u16 LE)
//  18:    drs              (u8)
//  19..22: rev lights etc. (skip)
//  22..30: brake temps     (4×u16 LE)
//  30..34: tyre surf temps (4×u8)
//  34..38: tyre inner temps(4×u8)
//  38..40: engine temp     (u16 LE)
//  40..56: tyre pressures  (4×f32 LE)
//  56..60: surfaceType     (4×u8)
//
// Minimum total = 29 + 22*60 + 3 = 1352 bytes

const F1_25_HEADER_SIZE: usize = 29;
const F1_25_NUM_CARS: usize = 22;
const F1_25_CAR_ENTRY_SIZE: usize = 60;
const F1_25_MIN_PACKET: usize = F1_25_HEADER_SIZE + F1_25_NUM_CARS * F1_25_CAR_ENTRY_SIZE + 3;

fn build_f1_25_car_telemetry_packet(
    player_index: u8,
    speed_kmh: u16,
    throttle: f32,
    brake: f32,
    gear: i8,
    engine_rpm: u16,
) -> Vec<u8> {
    let mut buf = vec![0u8; F1_25_MIN_PACKET];

    // Header
    write_u16_le(&mut buf, 0, 2025); // packet_format
    buf[6] = 6; // packet_id = CAR_TELEMETRY
    buf[27] = player_index;

    // Player car telemetry entry
    let car_off = F1_25_HEADER_SIZE + (player_index as usize) * F1_25_CAR_ENTRY_SIZE;
    write_u16_le(&mut buf, car_off, speed_kmh);
    write_f32_le(&mut buf, car_off + 2, throttle);
    write_f32_le(&mut buf, car_off + 6, 0.0); // steer
    write_f32_le(&mut buf, car_off + 10, brake);
    buf[car_off + 14] = 0; // clutch
    buf[car_off + 15] = gear as u8;
    write_u16_le(&mut buf, car_off + 16, engine_rpm);
    buf[car_off + 18] = 0; // DRS off
    // Engine temperature at car_off + 38
    write_u16_le(&mut buf, car_off + 38, 95);
    // Tyre pressures at car_off + 40 (4×f32)
    write_f32_le(&mut buf, car_off + 40, 23.5);
    write_f32_le(&mut buf, car_off + 44, 23.5);
    write_f32_le(&mut buf, car_off + 48, 22.0);
    write_f32_le(&mut buf, car_off + 52, 22.0);

    buf
}

#[test]
fn f1_25_car_telemetry_round_trips_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("f1_25")?;

    let packet = build_f1_25_car_telemetry_packet(0, 310, 1.0, 0.0, 7, 12_000);
    let telem = adapter.normalize(&packet)?;

    // speed_kmh=310 → m/s = 310/3.6 ≈ 86.11
    assert_f32_near(telem.speed_ms, 310.0 / 3.6, 1.0, "F1 25 speed");
    assert_f32_near(telem.rpm, 12_000.0, 1.0, "F1 25 RPM");
    assert_eq!(telem.gear, 7, "F1 25 gear should be 7th");

    Ok(())
}

#[test]
fn f1_25_car_telemetry_round_trips_controls() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("f1_25")?;

    let packet = build_f1_25_car_telemetry_packet(0, 0, 0.0, 0.95, 0, 4_000);
    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, 0, "F1 25 gear=0 should be neutral");
    // F1 25 stores throttle/brake in extended map
    let ext_throttle = extended_float(&telem.extended, "throttle").unwrap_or(telem.throttle);
    let ext_brake = extended_float(&telem.extended, "brake").unwrap_or(telem.brake);
    assert_f32_near(ext_throttle, 0.0, 0.01, "F1 25 zero throttle");
    assert_f32_near(ext_brake, 0.95, 0.02, "F1 25 heavy braking");

    Ok(())
}

#[test]
fn f1_25_different_player_index() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("f1_25")?;

    // Player car at index 5
    let packet = build_f1_25_car_telemetry_packet(5, 280, 0.75, 0.0, 6, 11_500);
    let telem = adapter.normalize(&packet)?;

    assert_f32_near(telem.speed_ms, 280.0 / 3.6, 1.0, "F1 25 speed at index 5");
    assert_eq!(telem.gear, 6, "F1 25 gear at index 5");

    Ok(())
}

#[test]
fn f1_25_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("f1_25")?;
    let short = [0u8; 50];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "F1 25 must reject packets shorter than the header"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// rFactor 2 — RF2VehicleTelemetry struct (pub, repr(C))
// ═══════════════════════════════════════════════════════════════════════════════
//
// Rather than manually computing byte offsets in a complex repr(C) struct,
// we construct the public `RF2VehicleTelemetry` directly, then cast to bytes.

use openracing_telemetry_adapters::rfactor2::RF2VehicleTelemetry;

#[test]
fn rfactor2_vehicle_telemetry_round_trips_engine_data() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("rfactor2")?;

    let vehicle = RF2VehicleTelemetry {
        engine_rpm: 7200.0,
        engine_max_rpm: 8500.0,
        gear: 4,
        unfiltered_throttle: 0.85,
        unfiltered_brake: 0.0,
        unfiltered_clutch: 0.0,
        unfiltered_steering: 0.1,
        fuel: 42.0,
        local_vel: [20.0, 0.0, 0.0],
        lap_number: 7,
        ..RF2VehicleTelemetry::default()
    };

    let raw: &[u8] = unsafe {
        std::slice::from_raw_parts(
            &vehicle as *const RF2VehicleTelemetry as *const u8,
            std::mem::size_of::<RF2VehicleTelemetry>(),
        )
    };
    let telem = adapter.normalize(raw)?;

    assert_f32_near(telem.rpm, 7200.0, 1.0, "rF2 RPM");
    assert_f32_near(telem.speed_ms, 20.0, 0.5, "rF2 speed from local_vel");
    assert_eq!(telem.gear, 4, "rF2 gear should be 4th");

    Ok(())
}

#[test]
fn rfactor2_speed_from_3d_velocity() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("rfactor2")?;

    let vehicle = RF2VehicleTelemetry {
        engine_rpm: 5000.0,
        gear: 3,
        // Speed = sqrt(3^2 + 4^2 + 0^2) = 5.0 m/s
        local_vel: [3.0, 4.0, 0.0],
        ..RF2VehicleTelemetry::default()
    };

    let raw: &[u8] = unsafe {
        std::slice::from_raw_parts(
            &vehicle as *const RF2VehicleTelemetry as *const u8,
            std::mem::size_of::<RF2VehicleTelemetry>(),
        )
    };
    let telem = adapter.normalize(raw)?;

    assert_f32_near(telem.speed_ms, 5.0, 0.1, "rF2 speed sqrt(3²+4²)");
    assert_eq!(telem.gear, 3, "rF2 gear should be 3rd");

    Ok(())
}

#[test]
fn rfactor2_reverse_gear() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("rfactor2")?;

    let vehicle = RF2VehicleTelemetry {
        engine_rpm: 2000.0,
        gear: -1,
        local_vel: [2.0, 0.0, 0.0],
        ..RF2VehicleTelemetry::default()
    };

    let raw: &[u8] = unsafe {
        std::slice::from_raw_parts(
            &vehicle as *const RF2VehicleTelemetry as *const u8,
            std::mem::size_of::<RF2VehicleTelemetry>(),
        )
    };
    let telem = adapter.normalize(raw)?;

    assert_eq!(telem.gear, -1, "rF2 gear=-1 should be reverse");

    Ok(())
}

#[test]
fn rfactor2_extracts_vehicle_and_track_name() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("rfactor2")?;

    let mut vehicle_name = [0u8; 64];
    let car_name = b"Porsche 911 GT3 R\0";
    vehicle_name[..car_name.len()].copy_from_slice(car_name);

    let mut track_name_buf = [0u8; 64];
    let track = b"Spa-Francorchamps\0";
    track_name_buf[..track.len()].copy_from_slice(track);

    let vehicle = RF2VehicleTelemetry {
        engine_rpm: 3000.0,
        gear: 2,
        local_vel: [10.0, 0.0, 0.0],
        vehicle_name,
        track_name: track_name_buf,
        ..RF2VehicleTelemetry::default()
    };

    let raw: &[u8] = unsafe {
        std::slice::from_raw_parts(
            &vehicle as *const RF2VehicleTelemetry as *const u8,
            std::mem::size_of::<RF2VehicleTelemetry>(),
        )
    };
    let telem = adapter.normalize(raw)?;

    assert_eq!(
        telem.car_id.as_deref(),
        Some("Porsche 911 GT3 R"),
        "rF2 car name"
    );
    assert_eq!(
        telem.track_id.as_deref(),
        Some("Spa-Francorchamps"),
        "rF2 track name"
    );

    Ok(())
}

#[test]
fn rfactor2_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("rfactor2")?;
    let short = [0u8; 32];
    let result = adapter.normalize(&short);
    assert!(result.is_err(), "rFactor 2 must reject undersized packets");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-adapter: verify all five high-priority adapters are registered
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn high_priority_adapters_registered() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    let required = ["acc", "iracing", "gran_turismo_7", "f1_25", "rfactor2"];
    for game in &required {
        assert!(
            ids.contains(game),
            "adapter registry must include '{game}', available: {ids:?}"
        );
    }

    Ok(())
}
