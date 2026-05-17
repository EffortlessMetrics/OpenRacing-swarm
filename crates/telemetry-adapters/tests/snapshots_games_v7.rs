//! Snapshot tests for telemetry adapters with realistic driving data (v7).
//!
//! The existing zeroed-buffer snapshots in `snapshots_extended.rs` verify that
//! the adapters survive all-zero input.  These tests populate fields with
//! representative driving values so the snapshot captures meaningful output.
//!
//! Covers: AMS2, rFactor2, iRacing, RaceRoom, ETS2, Gran Turismo 7,
//! DiRT Rally 2.0, GRID Legends, Wreckfest, KartKraft, Rennsport,
//! Dakar Desert Rally, Forza Motorsport (CarDash).

use openracing_telemetry_adapters::{
    AMS2Adapter, DakarDesertRallyAdapter, DirtRally2Adapter, Ets2Adapter, ForzaAdapter,
    GridLegendsAdapter, IRacingAdapter, KartKraftAdapter, RFactor2Adapter, RaceRoomAdapter,
    RennsportAdapter, TelemetryAdapter, WreckfestAdapter,
    ams2::AMS2SharedMemory,
    gran_turismo_7,
    rfactor2::{RF2VehicleTelemetry, RF2WheelTelemetry},
};
use std::mem;
use std::ptr;
mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a `#[repr(C)]` value to its raw byte representation.
fn struct_to_bytes<T: Copy>(val: &T) -> Vec<u8> {
    let size = mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    // SAFETY: T is Copy + repr(C), buf is exactly size_of::<T>() bytes.
    unsafe {
        ptr::copy_nonoverlapping(val as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    buf
}

/// Write a UTF-8 string into a fixed-size byte buffer (null-terminated).
fn write_string(dst: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&bytes[..len]);
    dst[len] = 0;
}

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

// ─── AMS2 ─────────────────────────────────────────────────────────────────────

fn make_ams2_data() -> AMS2SharedMemory {
    let mut data = AMS2SharedMemory::default();

    // Session / state
    data.version = 12;
    data.game_state = 2; // InGamePlaying
    data.session_state = 5; // Race
    data.race_state = 2; // Racing
    data.laps_completed = 3;
    data.laps_in_event = 15;

    // Car dynamics
    data.speed = 45.0; // 45 m/s ≈ 162 km/h
    data.rpm = 7200.0;
    data.max_rpm = 8500.0;
    data.gear = 4;
    data.num_gears = 6;
    data.fuel_level = 32.5;
    data.fuel_capacity = 60.0;

    // Controls & FFB
    data.throttle = 0.75;
    data.brake = 0.0;
    data.clutch = 0.0;
    data.steering = 0.15; // slight right input

    // Electronics
    data.tc_setting = 2;
    data.abs_setting = 1;

    // Tyre slip (non-zero so slip_ratio is computed; speed > 1.0)
    data.tyre_slip = [0.04, 0.05, 0.08, 0.07];

    // Flags: green flag
    data.highest_flag = 1; // Green

    // Car / track names
    write_string(&mut data.car_name, "Formula_Trainer");
    write_string(&mut data.track_location, "Interlagos");

    data
}

#[test]
fn ams2_normalized_snapshot() -> TestResult {
    let adapter = AMS2Adapter::new();
    let raw = struct_to_bytes(&make_ams2_data());
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor2 ─────────────────────────────────────────────────────────────────

fn make_rf2_vehicle() -> RF2VehicleTelemetry {
    // Wheel lateral patch velocity (used for slip ratio; speed ≥ 1.0)
    let base_wheel = RF2WheelTelemetry {
        lateral_patch_vel: 3.3,
        ..RF2WheelTelemetry::default()
    };

    let mut vehicle = RF2VehicleTelemetry {
        id: 1,
        lap_number: 5,
        local_vel: [55.0, 0.0, 0.0], // 55 m/s ≈ 198 km/h forward
        gear: 5,
        engine_rpm: 9500.0,
        engine_max_rpm: 11_000.0,
        engine_water_temp: 92.0,
        engine_oil_temp: 105.0,
        fuel: 28.0,
        unfiltered_throttle: 0.85,
        unfiltered_brake: 0.0,
        unfiltered_steering: 0.1,
        unfiltered_clutch: 0.0,
        // FFB via steering shaft torque (≤ 1.5 → clamped directly)
        steering_shaft_torque: 0.75,
        wheels: [
            RF2WheelTelemetry {
                lateral_patch_vel: 2.2,
                ..base_wheel
            },
            RF2WheelTelemetry {
                lateral_patch_vel: 2.75,
                ..base_wheel
            },
            RF2WheelTelemetry {
                lateral_patch_vel: 4.4,
                ..base_wheel
            },
            RF2WheelTelemetry {
                lateral_patch_vel: 3.85,
                ..base_wheel
            },
        ],
        ..Default::default()
    };

    // Car / track names
    write_string(&mut vehicle.vehicle_name, "Dallara_IR18");
    write_string(&mut vehicle.track_name, "Spa-Francorchamps");

    vehicle
}

#[test]
fn rfactor2_normalized_snapshot() -> TestResult {
    let adapter = RFactor2Adapter::new();
    let raw = struct_to_bytes(&make_rf2_vehicle());
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── iRacing ──────────────────────────────────────────────────────────────────
// IRacingData is private (#[repr(C)]); construct raw bytes matching its layout.
// Offsets: session_time@0(f32), session_flags@4(u32), speed@8(f32), rpm@12(f32),
//   gear@16(i8+3pad), throttle@20(f32), brake@24(f32), steering_wheel_angle@28(f32),
//   steering_wheel_torque@32(f32), pct_torque_sign@36(f32), max_force_nm@40(f32),
//   limiter@44(f32), lf/rf/lr/rr_tire_slip_ratio@48..64(f32×4),
//   lf/rf/lr/rr_tire_rps@64..80(f32×4), lap_current@80(i32), lap_best_time@84(f32),
//   fuel_level@88(f32), on_pit_road@92(i32), car_path@96([u8;64]),
//   track_name@160([u8;64]).  Total: 224 bytes.

fn make_iracing_data() -> Vec<u8> {
    let mut buf = vec![0u8; 320]; // sized for expanded IRacingData
    write_f32_le(&mut buf, 0, 120.5); // session_time
    write_u32(&mut buf, 4, 0x04); // session_flags = green
    write_f32_le(&mut buf, 8, 62.0); // speed (m/s) ≈ 223 km/h
    write_f32_le(&mut buf, 12, 7800.0); // rpm
    buf[16] = 4i8 as u8; // gear (4th)
    write_f32_le(&mut buf, 20, 0.92); // throttle
    write_f32_le(&mut buf, 24, 0.0); // brake
    write_f32_le(&mut buf, 28, -0.08); // steering_wheel_angle (rad)
    write_f32_le(&mut buf, 32, 12.0); // steering_wheel_torque (N·m)
    write_f32_le(&mut buf, 36, 0.0); // pct_torque_sign
    write_f32_le(&mut buf, 40, 0.0); // max_force_nm
    write_f32_le(&mut buf, 44, 0.0); // limiter
    write_f32_le(&mut buf, 48, 0.02); // lf_tire_slip_ratio
    write_f32_le(&mut buf, 52, 0.03); // rf_tire_slip_ratio
    write_f32_le(&mut buf, 56, 0.01); // lr_tire_slip_ratio
    write_f32_le(&mut buf, 60, 0.01); // rr_tire_slip_ratio
    write_f32_le(&mut buf, 64, 35.0); // lf_tire_rps
    write_f32_le(&mut buf, 68, 35.0); // rf_tire_rps
    write_f32_le(&mut buf, 72, 35.0); // lr_tire_rps
    write_f32_le(&mut buf, 76, 35.0); // rr_tire_rps
    write_i32(&mut buf, 80, 8); // lap_current
    write_f32_le(&mut buf, 84, 82.5); // lap_best_time (seconds)
    write_f32_le(&mut buf, 88, 15.0); // fuel_level (litres)
    write_f32_le(&mut buf, 92, 0.42); // fuel_level_pct
    write_i32(&mut buf, 96, 1); // on_pit_road = true
    write_f32_le(&mut buf, 100, 0.15); // clutch
    write_i32(&mut buf, 104, 3); // player_car_position
    write_f32_le(&mut buf, 108, 83.2); // lap_last_time (seconds)
    write_f32_le(&mut buf, 112, 41.7); // lap_current_time (seconds)
    write_f32_le(&mut buf, 116, 92.0); // lf_temp_cl (°C)
    write_f32_le(&mut buf, 120, 94.0); // rf_temp_cl (°C)
    write_f32_le(&mut buf, 124, 88.0); // lr_temp_cl (°C)
    write_f32_le(&mut buf, 128, 90.0); // rr_temp_cl (°C)
    write_f32_le(&mut buf, 132, 172.0); // lf_pressure (kPa)
    write_f32_le(&mut buf, 136, 172.0); // rf_pressure (kPa)
    write_f32_le(&mut buf, 140, 165.0); // lr_pressure (kPa)
    write_f32_le(&mut buf, 144, 165.0); // rr_pressure (kPa)
    write_f32_le(&mut buf, 148, 4.9); // lat_accel (m/s²)
    write_f32_le(&mut buf, 152, -2.1); // long_accel (m/s²)
    write_f32_le(&mut buf, 156, 9.81); // vert_accel (m/s²)
    write_f32_le(&mut buf, 160, 85.0); // water_temp (°C)
    write_string(&mut buf[164..228], "dallarair18");
    write_string(&mut buf[228..292], "indianapolis");
    buf
}

#[test]
fn iracing_realistic_snapshot() -> TestResult {
    let adapter = IRacingAdapter::new();
    let normalized = adapter.normalize(&make_iracing_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── RaceRoom ─────────────────────────────────────────────────────────────────
// R3E shared memory: R3E_VIEW_SIZE = 4096, version_major@0 = 3 (SDK v3).

fn make_raceroom_data() -> Vec<u8> {
    let mut buf = vec![0u8; 4096];
    write_i32(&mut buf, 0, 3); // version_major = 3
    write_i32(&mut buf, 20, 0); // game_paused = 0
    write_i32(&mut buf, 24, 0); // game_in_menus = 0
    // engine_rps in rad/s: RPM * π / 30
    let rps_6800 = 6800.0f32 * std::f32::consts::PI / 30.0;
    let rps_9000 = 9000.0f32 * std::f32::consts::PI / 30.0;
    write_f32_le(&mut buf, 1396, rps_6800); // engine_rps
    write_f32_le(&mut buf, 1400, rps_9000); // max_engine_rps
    write_f32_le(&mut buf, 1456, 25.0); // fuel_left (f32, litres)
    write_f32_le(&mut buf, 1460, 50.0); // fuel_capacity (f32, litres)
    write_f32_le(&mut buf, 1392, 52.0); // car_speed m/s ≈ 187 km/h
    write_f32_le(&mut buf, 1524, -0.12); // steer_input_raw
    write_f32_le(&mut buf, 1500, 0.80); // throttle
    write_f32_le(&mut buf, 1508, 0.0); // brake
    write_f32_le(&mut buf, 1516, 0.0); // clutch
    write_i32(&mut buf, 1408, 5); // gear (5th)
    buf
}

#[test]
fn raceroom_realistic_snapshot() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let normalized = adapter.normalize(&make_raceroom_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── ETS2 ─────────────────────────────────────────────────────────────────────
// SCS Telemetry SDK shared memory: 512 bytes, version@0 = 1.

fn make_ets2_data() -> Vec<u8> {
    let mut buf = vec![0u8; 512];
    write_u32(&mut buf, 0, 1); // version = 1
    write_f32_le(&mut buf, 4, 25.0); // speed_ms (90 km/h)
    write_f32_le(&mut buf, 8, 1400.0); // engine_rpm
    write_i32(&mut buf, 12, 8); // gear = 8th
    write_f32_le(&mut buf, 16, 0.65); // fuel_ratio
    write_f32_le(&mut buf, 20, 0.55); // engine_load
    write_f32_le(&mut buf, 24, 0.7); // throttle
    write_f32_le(&mut buf, 28, 0.0); // brake
    write_f32_le(&mut buf, 32, 0.0); // clutch
    write_f32_le(&mut buf, 36, -0.05); // steering (slight left)
    write_f32_le(&mut buf, 40, 87.0); // engine_temp_c
    write_f32_le(&mut buf, 44, 2200.0); // max_rpm
    buf
}

#[test]
fn ets2_realistic_snapshot() -> TestResult {
    let adapter = Ets2Adapter::new();
    let normalized = adapter.normalize(&make_ets2_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo 7 ───────────────────────────────────────────────────────────
// Uses parse_decrypted (public) with a 296-byte buffer containing the GT7 magic.

fn make_gt7_data() -> [u8; gran_turismo_7::PACKET_SIZE] {
    let mut buf = [0u8; gran_turismo_7::PACKET_SIZE];
    write_u32(&mut buf, gran_turismo_7::OFF_MAGIC, gran_turismo_7::MAGIC);
    write_f32_le(&mut buf, 0x3C, 9200.0); // engine_rpm
    write_f32_le(&mut buf, 0x44, 42.0); // fuel_level
    write_f32_le(&mut buf, 0x48, 60.0); // fuel_capacity
    write_f32_le(&mut buf, 0x4C, 68.0); // speed_ms ≈ 245 km/h
    write_f32_le(&mut buf, 0x58, 95.0); // water_temp_c
    write_f32_le(&mut buf, 0x60, 90.0); // tire_temp_fl
    write_f32_le(&mut buf, 0x64, 92.0); // tire_temp_fr
    write_f32_le(&mut buf, 0x68, 88.0); // tire_temp_rl
    write_f32_le(&mut buf, 0x6C, 89.0); // tire_temp_rr
    write_u16(&mut buf, 0x74, 12); // lap_count (i16)
    write_i32(&mut buf, 0x78, 78_000); // best_lap_ms
    write_i32(&mut buf, 0x7C, 79_200); // last_lap_ms
    write_u16(&mut buf, 0x8A, 9500); // max_alert_rpm
    write_u16(&mut buf, 0x8E, 0); // flags
    buf[0x90] = 6u8; // gear_byte (6th gear, low nibble)
    buf[0x91] = (0.85f32 * 255.0) as u8; // throttle
    buf[0x92] = 0u8; // brake
    write_i32(&mut buf, 0x124, 3333); // car_code
    buf
}

#[test]
fn gran_turismo_7_realistic_snapshot() -> TestResult {
    let buf = make_gt7_data();
    let normalized = gran_turismo_7::parse_decrypted(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo 7 – Type2 extended (316 bytes) ─────────────────────────────
// PacketType2 adds wheel rotation and motion data (sway, heave, surge).

fn make_gt7_type2_data() -> Vec<u8> {
    let mut buf = vec![0u8; gran_turismo_7::PACKET_SIZE_TYPE2];
    write_u32(&mut buf, gran_turismo_7::OFF_MAGIC, gran_turismo_7::MAGIC);
    write_f32_le(&mut buf, 0x3C, 7400.0); // engine_rpm
    write_f32_le(&mut buf, 0x44, 28.0); // fuel_level
    write_f32_le(&mut buf, 0x48, 65.0); // fuel_capacity
    write_f32_le(&mut buf, 0x4C, 52.0); // speed_ms ≈ 187 km/h
    write_f32_le(&mut buf, 0x58, 93.0); // water_temp_c
    write_f32_le(&mut buf, 0x60, 91.0); // tire_temp_fl
    write_f32_le(&mut buf, 0x64, 93.0); // tire_temp_fr
    write_f32_le(&mut buf, 0x68, 87.0); // tire_temp_rl
    write_f32_le(&mut buf, 0x6C, 88.0); // tire_temp_rr
    write_u16(&mut buf, 0x74, 5); // lap_count
    write_i32(&mut buf, 0x78, 91_200); // best_lap_ms
    write_i32(&mut buf, 0x7C, 92_800); // last_lap_ms
    write_u16(&mut buf, 0x8A, 8800); // max_alert_rpm
    write_u16(&mut buf, 0x8E, 0); // flags (none)
    buf[0x90] = 5u8; // gear_byte (5th gear)
    buf[0x91] = (0.72f32 * 255.0) as u8; // throttle
    buf[0x92] = 0u8; // brake
    write_i32(&mut buf, 0x124, 2750); // car_code
    // Type2 extended fields
    write_f32_le(&mut buf, 0x128, -0.35); // wheel_rotation (rad, slight left)
    write_f32_le(&mut buf, 0x130, 0.22); // sway (lateral)
    write_f32_le(&mut buf, 0x134, -0.08); // heave (vertical)
    write_f32_le(&mut buf, 0x138, 0.65); // surge (longitudinal)
    buf
}

#[test]
fn gran_turismo_7_type2_extended_snapshot() -> TestResult {
    let buf = make_gt7_type2_data();
    let normalized = gran_turismo_7::parse_decrypted_ext(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo 7 – Type3 full (344 bytes) ────────────────────────────────
// PacketType3 adds energy recovery and car-type indicator atop Type2 fields.

fn make_gt7_type3_data() -> Vec<u8> {
    let mut buf = vec![0u8; gran_turismo_7::PACKET_SIZE_TYPE3];
    write_u32(&mut buf, gran_turismo_7::OFF_MAGIC, gran_turismo_7::MAGIC);
    write_f32_le(&mut buf, 0x3C, 5800.0); // engine_rpm
    write_f32_le(&mut buf, 0x44, 18.0); // fuel_level
    write_f32_le(&mut buf, 0x48, 55.0); // fuel_capacity
    write_f32_le(&mut buf, 0x4C, 38.0); // speed_ms ≈ 137 km/h
    write_f32_le(&mut buf, 0x58, 88.0); // water_temp_c
    write_f32_le(&mut buf, 0x60, 78.0); // tire_temp_fl
    write_f32_le(&mut buf, 0x64, 80.0); // tire_temp_fr
    write_f32_le(&mut buf, 0x68, 76.0); // tire_temp_rl
    write_f32_le(&mut buf, 0x6C, 77.0); // tire_temp_rr
    write_u16(&mut buf, 0x74, 8); // lap_count
    write_i32(&mut buf, 0x78, 102_500); // best_lap_ms
    write_i32(&mut buf, 0x7C, 104_100); // last_lap_ms
    write_u16(&mut buf, 0x8A, 7500); // max_alert_rpm
    write_u16(&mut buf, 0x8E, 0); // flags
    buf[0x90] = 3u8; // gear_byte (3rd gear)
    buf[0x91] = (0.45f32 * 255.0) as u8; // throttle
    buf[0x92] = 0u8; // brake
    write_i32(&mut buf, 0x124, 5100); // car_code
    // Type2 extended fields
    write_f32_le(&mut buf, 0x128, 0.12); // wheel_rotation (rad)
    write_f32_le(&mut buf, 0x130, -0.10); // sway
    write_f32_le(&mut buf, 0x134, 0.02); // heave
    write_f32_le(&mut buf, 0x138, 0.40); // surge
    // Type3 extended fields
    buf[0x13E] = 4u8; // car_type_byte3 (4 = electric)
    write_f32_le(&mut buf, 0x150, 62.5); // energy_recovery
    buf
}

#[test]
fn gran_turismo_7_type3_full_snapshot() -> TestResult {
    let buf = make_gt7_type3_data();
    let normalized = gran_turismo_7::parse_decrypted_ext(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Gran Turismo 7 – heavy braking with flags ──────────────────────────────
// Scenario: hard braking zone with TCS + ASM active and rev limiter hit.

fn make_gt7_braking_data() -> [u8; gran_turismo_7::PACKET_SIZE] {
    let mut buf = [0u8; gran_turismo_7::PACKET_SIZE];
    write_u32(&mut buf, gran_turismo_7::OFF_MAGIC, gran_turismo_7::MAGIC);
    write_f32_le(&mut buf, 0x3C, 4200.0); // engine_rpm (decelerating)
    write_f32_le(&mut buf, 0x44, 22.0); // fuel_level
    write_f32_le(&mut buf, 0x48, 60.0); // fuel_capacity
    write_f32_le(&mut buf, 0x4C, 25.0); // speed_ms ≈ 90 km/h (slowing down)
    write_f32_le(&mut buf, 0x58, 97.0); // water_temp_c (hot from hard driving)
    write_f32_le(&mut buf, 0x60, 105.0); // tire_temp_fl (hot from braking)
    write_f32_le(&mut buf, 0x64, 107.0); // tire_temp_fr
    write_f32_le(&mut buf, 0x68, 95.0); // tire_temp_rl
    write_f32_le(&mut buf, 0x6C, 96.0); // tire_temp_rr
    write_u16(&mut buf, 0x74, 10); // lap_count
    write_i32(&mut buf, 0x78, 82_000); // best_lap_ms
    write_i32(&mut buf, 0x7C, 83_500); // last_lap_ms
    write_u16(&mut buf, 0x8A, 9000); // max_alert_rpm
    // FLAGS: TCS(1<<11) | ASM(1<<10) | REV_LIMIT(1<<5)
    write_u16(&mut buf, 0x8E, (1u16 << 11) | (1u16 << 10) | (1u16 << 5));
    buf[0x90] = 2u8; // gear_byte (2nd gear, downshifted)
    buf[0x91] = 0u8; // throttle (off)
    buf[0x92] = (0.95f32 * 255.0) as u8; // brake (heavy braking)
    write_i32(&mut buf, 0x124, 1887); // car_code
    buf
}

#[test]
fn gran_turismo_7_heavy_braking_snapshot() -> TestResult {
    let buf = make_gt7_braking_data();
    let normalized = gran_turismo_7::parse_decrypted(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── DiRT Rally 2.0 ──────────────────────────────────────────────────────────
// Codemasters Mode 1 packet (264 bytes).

fn make_dirt_rally_2_data() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 100, 22.0); // wheel_speed_rl (m/s)
    write_f32_le(&mut buf, 104, 22.5); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 23.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 23.5); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.70); // throttle
    write_f32_le(&mut buf, 120, -0.35); // steer (left)
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 3.0); // gear (3rd)
    write_f32_le(&mut buf, 136, 0.60); // gforce_lat
    write_f32_le(&mut buf, 140, 0.30); // gforce_lon
    write_f32_le(&mut buf, 148, 5200.0); // rpm
    write_f32_le(&mut buf, 180, 28.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 45.0); // fuel_capacity
    write_f32_le(&mut buf, 252, 7000.0); // max_rpm
    buf
}

#[test]
fn dirt_rally_2_realistic_snapshot() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let normalized = adapter.normalize(&make_dirt_rally_2_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── GRID Legends ────────────────────────────────────────────────────────────
// Codemasters Mode 1 packet (264 bytes).

fn make_grid_legends_data() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 100, 40.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 40.0); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 41.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 41.0); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.95); // throttle
    write_f32_le(&mut buf, 120, 0.05); // steer (slight right)
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 5.0); // gear (5th)
    write_f32_le(&mut buf, 136, 1.80); // gforce_lat
    write_f32_le(&mut buf, 140, 0.45); // gforce_lon
    write_f32_le(&mut buf, 148, 7500.0); // rpm
    write_f32_le(&mut buf, 180, 40.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 65.0); // fuel_capacity
    write_f32_le(&mut buf, 252, 9000.0); // max_rpm
    buf
}

#[test]
fn grid_legends_realistic_snapshot() -> TestResult {
    let adapter = GridLegendsAdapter::new();
    let normalized = adapter.normalize(&make_grid_legends_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Wreckfest ────────────────────────────────────────────────────────────────
// WRKF magic at offset 0; min packet 28 bytes.

fn make_wreckfest_data() -> Vec<u8> {
    let mut buf = vec![0u8; 28];
    buf[0..4].copy_from_slice(b"WRKF"); // magic
    write_f32_le(&mut buf, 8, 42.0); // speed_ms ≈ 151 km/h
    write_f32_le(&mut buf, 12, 5800.0); // rpm
    buf[16] = 4u8; // gear (4th)
    write_f32_le(&mut buf, 20, 1.2); // lateral_g
    write_f32_le(&mut buf, 24, 0.5); // longitudinal_g
    buf
}

#[test]
fn wreckfest_realistic_snapshot() -> TestResult {
    let adapter = WreckfestAdapter::new();
    let normalized = adapter.normalize(&make_wreckfest_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── KartKraft ───────────────────────────────────────────────────────────────
// FlatBuffers binary format: root_offset + "KKFB" identifier, Frame → Dashboard.

fn make_kartkraft_data() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();

    let push_u16 = |buf: &mut Vec<u8>, v: u16| buf.extend_from_slice(&v.to_le_bytes());
    let push_i32 = |buf: &mut Vec<u8>, v: i32| buf.extend_from_slice(&v.to_le_bytes());
    let push_u32 = |buf: &mut Vec<u8>, v: u32| buf.extend_from_slice(&v.to_le_bytes());
    let push_f32 = |buf: &mut Vec<u8>, v: f32| buf.extend_from_slice(&v.to_le_bytes());

    // Root offset placeholder + file identifier.
    push_u32(&mut buf, 0); // placeholder
    buf.extend_from_slice(b"KKFB");

    // Frame vtable (dash present at field offset 4).
    let vt_frame_start = buf.len();
    push_u16(&mut buf, 10); // vtable_size
    push_u16(&mut buf, 12); // object_size
    push_u16(&mut buf, 0); // field 0 (timestamp) absent
    push_u16(&mut buf, 0); // field 1 (motion) absent
    push_u16(&mut buf, 4); // field 2 (dash) at byte offset 4

    // Frame table.
    let frame_table_pos = buf.len();
    push_i32(&mut buf, (frame_table_pos - vt_frame_start) as i32);
    push_u32(&mut buf, 0); // dash UOffset placeholder
    push_u32(&mut buf, 0); // padding

    // Patch root_offset.
    buf[0..4].copy_from_slice(&(frame_table_pos as u32).to_le_bytes());

    // Dashboard vtable (6 scalar fields).
    let vt_dash_start = buf.len();
    push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
    push_u16(&mut buf, 28); // object_size = 4 + 6*4
    push_u16(&mut buf, 4); // speed
    push_u16(&mut buf, 8); // rpm
    push_u16(&mut buf, 12); // steer
    push_u16(&mut buf, 16); // throttle
    push_u16(&mut buf, 20); // brake
    push_u16(&mut buf, 24); // gear

    // Dashboard table.
    let dash_table_pos = buf.len();
    push_i32(&mut buf, (dash_table_pos - vt_dash_start) as i32);
    push_f32(&mut buf, 18.0); // speed (m/s) ≈ 65 km/h
    push_f32(&mut buf, 11_000.0); // rpm
    push_f32(&mut buf, -8.0); // steer (degrees, left)
    push_f32(&mut buf, 0.95); // throttle
    push_f32(&mut buf, 0.0); // brake
    buf.push(4u8); // gear (4th)
    buf.extend_from_slice(&[0, 0, 0]); // padding

    // Patch dash UOffset.
    let ref_pos = frame_table_pos + 4;
    let dash_uoffset = (dash_table_pos - ref_pos) as u32;
    buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

    buf
}

#[test]
fn kartkraft_realistic_snapshot() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let normalized = adapter.normalize(&make_kartkraft_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Rennsport ────────────────────────────────────────────────────────────────
// Identifier byte 0x52 ('R'); min packet 24 bytes.

fn make_rennsport_data() -> Vec<u8> {
    let mut buf = vec![0u8; 24];
    buf[0] = 0x52; // identifier 'R'
    write_f32_le(&mut buf, 4, 216.0); // speed_kmh → 60.0 m/s
    write_f32_le(&mut buf, 8, 8200.0); // rpm
    buf[12] = 4u8; // gear (4th)
    write_f32_le(&mut buf, 16, 0.45); // ffb_scalar
    write_f32_le(&mut buf, 20, 0.12); // slip_ratio
    buf
}

#[test]
fn rennsport_realistic_snapshot() -> TestResult {
    let adapter = RennsportAdapter::new();
    let normalized = adapter.normalize(&make_rennsport_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Dakar Desert Rally ──────────────────────────────────────────────────────
// DAKR magic at offset 0; min packet 40 bytes.

fn make_dakar_data() -> Vec<u8> {
    let mut buf = vec![0u8; 40];
    buf[0..4].copy_from_slice(b"DAKR"); // magic
    write_f32_le(&mut buf, 8, 38.0); // speed_ms ≈ 137 km/h
    write_f32_le(&mut buf, 12, 5500.0); // rpm
    buf[16] = 4; // gear (4th)
    write_f32_le(&mut buf, 20, 0.45); // lateral_g
    write_f32_le(&mut buf, 24, 0.20); // longitudinal_g
    write_f32_le(&mut buf, 28, 0.65); // throttle
    write_f32_le(&mut buf, 32, 0.10); // brake
    write_f32_le(&mut buf, 36, -0.20); // steering_angle
    buf
}

#[test]
fn dakar_realistic_snapshot() -> TestResult {
    let adapter = DakarDesertRallyAdapter::new();
    let normalized = adapter.normalize(&make_dakar_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Forza Motorsport (CarDash) ──────────────────────────────────────────────
// CarDash packet: 311 bytes, is_race_on@0 = 1.

fn make_forza_cardash_data() -> Vec<u8> {
    let mut buf = vec![0u8; 311];
    write_i32(&mut buf, 0, 1); // is_race_on = 1
    write_f32_le(&mut buf, 8, 9000.0); // engine_max_rpm
    write_f32_le(&mut buf, 16, 7200.0); // current_rpm
    // Velocity vector → speed magnitude
    write_f32_le(&mut buf, 32, 35.0); // vel_x
    write_f32_le(&mut buf, 36, 0.0); // vel_y
    write_f32_le(&mut buf, 40, 10.0); // vel_z
    // Wheel rotation speeds (rad/s)
    write_f32_le(&mut buf, 100, 110.0); // fl
    write_f32_le(&mut buf, 104, 110.0); // fr
    write_f32_le(&mut buf, 108, 108.0); // rl
    write_f32_le(&mut buf, 112, 108.0); // rr
    // CarDash extension
    write_f32_le(&mut buf, 244, 36.4); // dash_speed (m/s, more accurate)
    buf[303] = 220; // dash_accel (throttle): 220/255 ≈ 0.863
    buf[304] = 0; // dash_brake
    buf[307] = 5; // dash_gear: 5 → gear 4
    buf[308] = (-10i8) as u8; // dash_steer: -10/127 ≈ -0.079 (slight left)
    buf
}

#[test]
fn forza_cardash_realistic_snapshot() -> TestResult {
    let adapter = ForzaAdapter::new();
    let normalized = adapter.normalize(&make_forza_cardash_data())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
