//! Snapshot tests for sim racing adapters without dedicated snapshot coverage.
//!
//! Covers: PCars2, PCars3, Automobilista 1, RaceRoom, Gran Turismo 7,
//! Gran Turismo Sport, Trackmania, and AC Rally.

use openracing_telemetry_adapters::{
    ACRallyAdapter, Automobilista1Adapter, PCars2Adapter, PCars3Adapter, RaceRoomAdapter,
    TelemetryAdapter, TrackmaniaAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Shared byte-write helpers ────────────────────────────────────────────────

fn write_u16_le(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_i16_le(buf: &mut [u8], offset: usize, value: i16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_f64_le(buf: &mut [u8], offset: usize, value: f64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. PCars2 — mid-race snapshot (full 538-byte UDP sTelemetryData)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: lap 4 of a GT3 race, 5th gear at ~190 km/h, partial throttle,
// light trail-braking, moderate lateral G from a long sweeper.

// SMS sTelemetryData UDP offsets (verified against CrewChiefV4)
const PCARS2_PACKET_SIZE: usize = 538;
const PCARS2_OFF_CAR_FLAGS: usize = 17;
const PCARS2_OFF_OIL_TEMP: usize = 18;
const PCARS2_OFF_OIL_PRESSURE: usize = 20;
const PCARS2_OFF_WATER_TEMP: usize = 22;
const PCARS2_OFF_WATER_PRESSURE: usize = 24;
const PCARS2_OFF_FUEL_PRESSURE: usize = 26;
const PCARS2_OFF_FUEL_CAPACITY: usize = 28;
const PCARS2_OFF_BRAKE: usize = 29;
const PCARS2_OFF_THROTTLE: usize = 30;
const PCARS2_OFF_CLUTCH: usize = 31;
const PCARS2_OFF_FUEL_LEVEL: usize = 32;
const PCARS2_OFF_SPEED: usize = 36;
const PCARS2_OFF_RPM: usize = 40;
const PCARS2_OFF_MAX_RPM: usize = 42;
const PCARS2_OFF_STEERING: usize = 44;
const PCARS2_OFF_GEAR_NUM_GEARS: usize = 45;
const PCARS2_OFF_BOOST: usize = 46;
const PCARS2_OFF_ODOMETER: usize = 48;
const PCARS2_OFF_LOCAL_ACCEL_X: usize = 100;
const PCARS2_OFF_LOCAL_ACCEL_Y: usize = 104;
const PCARS2_OFF_LOCAL_ACCEL_Z: usize = 108;
const PCARS2_OFF_TYRE_TEMP: usize = 176;
const PCARS2_OFF_AIR_PRESSURE: usize = 352;

fn make_pcars2_race_packet() -> Vec<u8> {
    let mut buf = vec![0u8; PCARS2_PACKET_SIZE];
    // Inputs
    buf[PCARS2_OFF_THROTTLE] = 191; // ~75%
    buf[PCARS2_OFF_BRAKE] = 38; // ~15% trail-brake
    buf[PCARS2_OFF_CLUTCH] = 0;
    buf[PCARS2_OFF_STEERING] = 20_i8 as u8; // slight right
    // 5th gear, 6 total: low nibble=5, high nibble=6
    buf[PCARS2_OFF_GEAR_NUM_GEARS] = (6 << 4) | 5;
    // Speed & RPM
    write_f32_le(&mut buf, PCARS2_OFF_SPEED, 52.8); // ~190 km/h
    write_u16_le(&mut buf, PCARS2_OFF_RPM, 7200);
    write_u16_le(&mut buf, PCARS2_OFF_MAX_RPM, 8500);
    // Fuel
    buf[PCARS2_OFF_FUEL_CAPACITY] = 110;
    write_f32_le(&mut buf, PCARS2_OFF_FUEL_LEVEL, 0.62);
    // Temperatures
    write_i16_le(&mut buf, PCARS2_OFF_OIL_TEMP, 115);
    write_i16_le(&mut buf, PCARS2_OFF_WATER_TEMP, 92);
    write_u16_le(&mut buf, PCARS2_OFF_OIL_PRESSURE, 380);
    write_u16_le(&mut buf, PCARS2_OFF_WATER_PRESSURE, 140);
    write_u16_le(&mut buf, PCARS2_OFF_FUEL_PRESSURE, 350);
    // Car flags: ABS active (bit 4)
    buf[PCARS2_OFF_CAR_FLAGS] = 0x10;
    // Boost & odometer
    buf[PCARS2_OFF_BOOST] = 12;
    write_f32_le(&mut buf, PCARS2_OFF_ODOMETER, 245.7);
    // G-forces (m/s²): ~1.2G lateral, ~0.5G longitudinal, ~1G vertical
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_X, 11.77);
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_Y, 9.81);
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_Z, 4.9);
    // Tyre temps (u8 °C): FL, FR, RL, RR
    buf[PCARS2_OFF_TYRE_TEMP] = 88;
    buf[PCARS2_OFF_TYRE_TEMP + 1] = 91;
    buf[PCARS2_OFF_TYRE_TEMP + 2] = 85;
    buf[PCARS2_OFF_TYRE_TEMP + 3] = 87;
    // Tyre pressures (u16, kPa): ~170 kPa each
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE, 172);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 2, 174);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 4, 168);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 6, 170);
    buf
}

#[test]
fn pcars2_race_snapshot() -> TestResult {
    let buf = make_pcars2_race_packet();
    let adapter = PCars2Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. PCars3 — mid-race snapshot (same UDP format as PCars2)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: lap 2, 3rd gear at ~120 km/h, heavy acceleration out of a
// hairpin, full throttle, no braking.

fn make_pcars3_race_packet() -> Vec<u8> {
    let mut buf = vec![0u8; PCARS2_PACKET_SIZE];
    buf[PCARS2_OFF_THROTTLE] = 255; // full throttle
    buf[PCARS2_OFF_BRAKE] = 0;
    buf[PCARS2_OFF_CLUTCH] = 0;
    buf[PCARS2_OFF_STEERING] = (-10_i8) as u8; // slight left
    // 3rd gear, 7 total
    buf[PCARS2_OFF_GEAR_NUM_GEARS] = (7 << 4) | 3;
    write_f32_le(&mut buf, PCARS2_OFF_SPEED, 33.3); // ~120 km/h
    write_u16_le(&mut buf, PCARS2_OFF_RPM, 6800);
    write_u16_le(&mut buf, PCARS2_OFF_MAX_RPM, 9000);
    buf[PCARS2_OFF_FUEL_CAPACITY] = 90;
    write_f32_le(&mut buf, PCARS2_OFF_FUEL_LEVEL, 0.85);
    write_i16_le(&mut buf, PCARS2_OFF_OIL_TEMP, 105);
    write_i16_le(&mut buf, PCARS2_OFF_WATER_TEMP, 88);
    write_u16_le(&mut buf, PCARS2_OFF_OIL_PRESSURE, 400);
    write_u16_le(&mut buf, PCARS2_OFF_WATER_PRESSURE, 130);
    write_u16_le(&mut buf, PCARS2_OFF_FUEL_PRESSURE, 340);
    buf[PCARS2_OFF_CAR_FLAGS] = 0;
    buf[PCARS2_OFF_BOOST] = 0;
    write_f32_le(&mut buf, PCARS2_OFF_ODOMETER, 112.3);
    // Acceleration out of hairpin: minimal lateral, strong longitudinal
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_X, 1.5);
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_Y, 9.81);
    write_f32_le(&mut buf, PCARS2_OFF_LOCAL_ACCEL_Z, 7.85);
    buf[PCARS2_OFF_TYRE_TEMP] = 82;
    buf[PCARS2_OFF_TYRE_TEMP + 1] = 84;
    buf[PCARS2_OFF_TYRE_TEMP + 2] = 80;
    buf[PCARS2_OFF_TYRE_TEMP + 3] = 81;
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE, 165);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 2, 167);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 4, 163);
    write_u16_le(&mut buf, PCARS2_OFF_AIR_PRESSURE + 6, 164);
    buf
}

#[test]
fn pcars3_race_snapshot() -> TestResult {
    let buf = make_pcars3_race_packet();
    let adapter = PCars3Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Automobilista 1 — race snapshot (ISI rFactor 1 shared memory, 532 bytes)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: mid-race at Interlagos, 4th gear at ~160 km/h, moderate cornering.

const AMS1_MIN_SIZE: usize = 532;
const AMS1_OFF_LOCAL_ACCEL_X: usize = 216;
const AMS1_OFF_LOCAL_ACCEL_Z: usize = 232;
const AMS1_OFF_GEAR: usize = 360;
const AMS1_OFF_ENGINE_RPM: usize = 368;
const AMS1_OFF_ENGINE_MAX_RPM: usize = 384;
const AMS1_OFF_FUEL_CAPACITY: usize = 457;
const AMS1_OFF_FUEL: usize = 460;
const AMS1_OFF_FILTERED_THROTTLE: usize = 492;
const AMS1_OFF_FILTERED_BRAKE: usize = 496;
const AMS1_OFF_FILTERED_STEERING: usize = 500;
const AMS1_OFF_SPEED: usize = 528;

#[test]
fn automobilista_race_snapshot() -> TestResult {
    let mut buf = vec![0u8; AMS1_MIN_SIZE];
    write_f32_le(&mut buf, AMS1_OFF_SPEED, 44.4); // ~160 km/h
    write_f64_le(&mut buf, AMS1_OFF_ENGINE_RPM, 7500.0);
    write_f64_le(&mut buf, AMS1_OFF_ENGINE_MAX_RPM, 9500.0);
    write_i32_le(&mut buf, AMS1_OFF_GEAR, 4);
    write_f32_le(&mut buf, AMS1_OFF_FILTERED_THROTTLE, 0.70);
    write_f32_le(&mut buf, AMS1_OFF_FILTERED_BRAKE, 0.05);
    write_f32_le(&mut buf, AMS1_OFF_FILTERED_STEERING, -0.18);
    // Lateral accel ~1.4G, longitudinal ~0.3G
    write_f64_le(&mut buf, AMS1_OFF_LOCAL_ACCEL_X, 13.73);
    write_f64_le(&mut buf, AMS1_OFF_LOCAL_ACCEL_Z, 2.94);
    // Fuel: 35L in a 65L tank
    buf[AMS1_OFF_FUEL_CAPACITY] = 65;
    write_f32_le(&mut buf, AMS1_OFF_FUEL, 35.0);

    let adapter = Automobilista1Adapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. RaceRoom — race snapshot (R3E shared memory, 4096 bytes)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: P3, lap 6, 4th gear at ~165 km/h, moderate braking into a chicane,
// yellow flag, ABS active.

const R3E_VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;
const R3E_OFF_VERSION_MAJOR: usize = 0;
const R3E_OFF_GAME_PAUSED: usize = 20;
const R3E_OFF_GAME_IN_MENUS: usize = 24;
const R3E_OFF_SPEED: usize = 1392;
const R3E_OFF_ENGINE_RPS: usize = 1396;
const R3E_OFF_MAX_ENGINE_RPS: usize = 1400;
const R3E_OFF_GEAR: usize = 1408;
const R3E_OFF_NUM_GEARS: usize = 1412;
const R3E_OFF_FUEL_LEFT: usize = 1456;
const R3E_OFF_FUEL_CAPACITY: usize = 1460;
const R3E_OFF_ENGINE_TEMP: usize = 1480;
const R3E_OFF_THROTTLE: usize = 1500;
const R3E_OFF_BRAKE: usize = 1508;
const R3E_OFF_CLUTCH: usize = 1516;
const R3E_OFF_STEER_INPUT: usize = 1524;
const R3E_OFF_LOCAL_ACCEL_X: usize = 1440;
const R3E_OFF_LOCAL_ACCEL_Y: usize = 1444;
const R3E_OFF_LOCAL_ACCEL_Z: usize = 1448;
const R3E_OFF_POSITION: usize = 988;
const R3E_OFF_COMPLETED_LAPS: usize = 1028;
const R3E_OFF_LAP_TIME_CURRENT: usize = 1100;
const R3E_OFF_LAP_TIME_BEST: usize = 1068;
const R3E_OFF_LAP_TIME_PREVIOUS: usize = 1084;
const R3E_OFF_DELTA_FRONT: usize = 1124;
const R3E_OFF_DELTA_BEHIND: usize = 1128;
const R3E_OFF_FLAG_YELLOW: usize = 932;
const R3E_OFF_FLAG_GREEN: usize = 972;
#[allow(dead_code)]
const R3E_OFF_IN_PITLANE: usize = 848;
const R3E_OFF_AID_ABS: usize = 1536;
const R3E_OFF_TIRE_TEMP_FL_CENTER: usize = 1748;
const R3E_OFF_TIRE_TEMP_FR_CENTER: usize = 1772;
const R3E_OFF_TIRE_TEMP_RL_CENTER: usize = 1796;
const R3E_OFF_TIRE_TEMP_RR_CENTER: usize = 1820;
const R3E_OFF_TIRE_PRESSURE_FL: usize = 1712;
const R3E_OFF_TIRE_PRESSURE_FR: usize = 1716;
const R3E_OFF_TIRE_PRESSURE_RL: usize = 1720;
const R3E_OFF_TIRE_PRESSURE_RR: usize = 1724;

#[test]
fn raceroom_race_snapshot() -> TestResult {
    let mut buf = vec![0u8; R3E_VIEW_SIZE];
    // Header: version 3, not paused, not in menus
    write_i32_le(&mut buf, R3E_OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_i32_le(&mut buf, R3E_OFF_GAME_PAUSED, 0);
    write_i32_le(&mut buf, R3E_OFF_GAME_IN_MENUS, 0);
    // Speed & engine (rps = RPM * π/30)
    write_f32_le(&mut buf, R3E_OFF_SPEED, 45.8);
    let rpm_6500_rps = 6500.0f32 * (std::f32::consts::PI / 30.0);
    let rpm_8000_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
    write_f32_le(&mut buf, R3E_OFF_ENGINE_RPS, rpm_6500_rps);
    write_f32_le(&mut buf, R3E_OFF_MAX_ENGINE_RPS, rpm_8000_rps);
    write_i32_le(&mut buf, R3E_OFF_GEAR, 4);
    write_i32_le(&mut buf, R3E_OFF_NUM_GEARS, 6);
    // Inputs
    write_f32_le(&mut buf, R3E_OFF_THROTTLE, 0.20);
    write_f32_le(&mut buf, R3E_OFF_BRAKE, 0.65);
    write_f32_le(&mut buf, R3E_OFF_CLUTCH, 0.0);
    write_f32_le(&mut buf, R3E_OFF_STEER_INPUT, 0.35);
    // Fuel: 25L of 60L
    write_f32_le(&mut buf, R3E_OFF_FUEL_LEFT, 25.0);
    write_f32_le(&mut buf, R3E_OFF_FUEL_CAPACITY, 60.0);
    write_f32_le(&mut buf, R3E_OFF_ENGINE_TEMP, 97.0);
    // G-forces (R3E: +X=left, +Y=up, +Z=back)
    write_f32_le(&mut buf, R3E_OFF_LOCAL_ACCEL_X, 8.5); // ~0.87G left
    write_f32_le(&mut buf, R3E_OFF_LOCAL_ACCEL_Y, 9.81); // ~1G vertical
    write_f32_le(&mut buf, R3E_OFF_LOCAL_ACCEL_Z, 6.0); // ~0.61G braking
    // Scoring
    write_i32_le(&mut buf, R3E_OFF_POSITION, 3);
    write_i32_le(&mut buf, R3E_OFF_COMPLETED_LAPS, 6);
    write_f32_le(&mut buf, R3E_OFF_LAP_TIME_CURRENT, 55.2);
    write_f32_le(&mut buf, R3E_OFF_LAP_TIME_BEST, 51.8);
    write_f32_le(&mut buf, R3E_OFF_LAP_TIME_PREVIOUS, 52.3);
    write_f32_le(&mut buf, R3E_OFF_DELTA_FRONT, 1.5);
    write_f32_le(&mut buf, R3E_OFF_DELTA_BEHIND, 0.7);
    // Flags: yellow + green
    write_i32_le(&mut buf, R3E_OFF_FLAG_YELLOW, 1);
    write_i32_le(&mut buf, R3E_OFF_FLAG_GREEN, 1);
    // ABS active (value 5 = active in R3E)
    write_i32_le(&mut buf, R3E_OFF_AID_ABS, 5);
    // Tire temps (~90°C)
    write_f32_le(&mut buf, R3E_OFF_TIRE_TEMP_FL_CENTER, 89.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_TEMP_FR_CENTER, 93.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_TEMP_RL_CENTER, 86.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_TEMP_RR_CENTER, 90.0);
    // Tire pressures (~170 KPa)
    write_f32_le(&mut buf, R3E_OFF_TIRE_PRESSURE_FL, 171.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_PRESSURE_FR, 173.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_PRESSURE_RL, 169.0);
    write_f32_le(&mut buf, R3E_OFF_TIRE_PRESSURE_RR, 170.0);

    let adapter = RaceRoomAdapter::new();
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Gran Turismo 7 — race snapshot (decrypted 296-byte packet)
// ═══════════════════════════════════════════════════════════════════════════════
//
// normalize() expects encrypted data; use parse_decrypted directly with
// a plaintext buffer containing the correct magic.
//
// Scenario: Sport mode race, lap 3, 4th gear at ~170 km/h, mid-corner.

const GT7_PACKET_SIZE: usize = 296;
const GT7_OFF_MAGIC: usize = 0x00;
const GT7_MAGIC: u32 = 0x4737_5330;
const GT7_OFF_ENGINE_RPM: usize = 0x3C;
const GT7_OFF_FUEL_LEVEL: usize = 0x44;
const GT7_OFF_FUEL_CAPACITY: usize = 0x48;
const GT7_OFF_SPEED_MS: usize = 0x4C;
const GT7_OFF_WATER_TEMP: usize = 0x58;
const GT7_OFF_TIRE_TEMP_FL: usize = 0x60;
const GT7_OFF_TIRE_TEMP_FR: usize = 0x64;
const GT7_OFF_TIRE_TEMP_RL: usize = 0x68;
const GT7_OFF_TIRE_TEMP_RR: usize = 0x6C;
const GT7_OFF_LAP_COUNT: usize = 0x74;
const GT7_OFF_BEST_LAP_MS: usize = 0x78;
const GT7_OFF_LAST_LAP_MS: usize = 0x7C;
const GT7_OFF_MAX_ALERT_RPM: usize = 0x8A;
const GT7_OFF_FLAGS: usize = 0x8E;
const GT7_OFF_GEAR_BYTE: usize = 0x90;
const GT7_OFF_THROTTLE: usize = 0x91;
const GT7_OFF_BRAKE: usize = 0x92;
const GT7_OFF_CAR_CODE: usize = 0x124;

fn make_gt7_decrypted_buf() -> [u8; GT7_PACKET_SIZE] {
    let mut buf = [0u8; GT7_PACKET_SIZE];
    buf[GT7_OFF_MAGIC..GT7_OFF_MAGIC + 4].copy_from_slice(&GT7_MAGIC.to_le_bytes());
    buf
}

#[test]
fn gran_turismo_7_race_snapshot() -> TestResult {
    let mut buf = make_gt7_decrypted_buf();
    // Engine
    buf[GT7_OFF_ENGINE_RPM..GT7_OFF_ENGINE_RPM + 4].copy_from_slice(&6200.0f32.to_le_bytes());
    write_u16_le(&mut buf, GT7_OFF_MAX_ALERT_RPM, 7800);
    // Speed: ~170 km/h ≈ 47.2 m/s
    buf[GT7_OFF_SPEED_MS..GT7_OFF_SPEED_MS + 4].copy_from_slice(&47.2f32.to_le_bytes());
    // Inputs
    buf[GT7_OFF_THROTTLE] = 204; // ~80%
    buf[GT7_OFF_BRAKE] = 0;
    // 4th gear, suggested 5th
    buf[GT7_OFF_GEAR_BYTE] = (5 << 4) | 4;
    // Fuel: 30L of 65L
    buf[GT7_OFF_FUEL_LEVEL..GT7_OFF_FUEL_LEVEL + 4].copy_from_slice(&30.0f32.to_le_bytes());
    buf[GT7_OFF_FUEL_CAPACITY..GT7_OFF_FUEL_CAPACITY + 4].copy_from_slice(&65.0f32.to_le_bytes());
    // Water temp
    buf[GT7_OFF_WATER_TEMP..GT7_OFF_WATER_TEMP + 4].copy_from_slice(&91.0f32.to_le_bytes());
    // Tire temps
    buf[GT7_OFF_TIRE_TEMP_FL..GT7_OFF_TIRE_TEMP_FL + 4].copy_from_slice(&78.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_FR..GT7_OFF_TIRE_TEMP_FR + 4].copy_from_slice(&81.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_RL..GT7_OFF_TIRE_TEMP_RL + 4].copy_from_slice(&75.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_RR..GT7_OFF_TIRE_TEMP_RR + 4].copy_from_slice(&77.0f32.to_le_bytes());
    // Laps
    write_u16_le(&mut buf, GT7_OFF_LAP_COUNT, 3);
    write_i32_le(&mut buf, GT7_OFF_BEST_LAP_MS, 92_340);
    write_i32_le(&mut buf, GT7_OFF_LAST_LAP_MS, 93_100);
    // Car code
    write_i32_le(&mut buf, GT7_OFF_CAR_CODE, 1234);
    // Flags: TCS active (bit 11)
    let flags: u16 = 1 << 11;
    write_u16_le(&mut buf, GT7_OFF_FLAGS, flags);

    let normalized = openracing_telemetry_adapters::gran_turismo_7::parse_decrypted(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Gran Turismo Sport — race snapshot (same SimulatorInterface format as GT7)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: Daily Race B, lap 5, 3rd gear at ~130 km/h, heavy braking zone.

#[test]
fn gran_turismo_sport_race_snapshot() -> TestResult {
    let mut buf = make_gt7_decrypted_buf();
    buf[GT7_OFF_ENGINE_RPM..GT7_OFF_ENGINE_RPM + 4].copy_from_slice(&5400.0f32.to_le_bytes());
    write_u16_le(&mut buf, GT7_OFF_MAX_ALERT_RPM, 7200);
    buf[GT7_OFF_SPEED_MS..GT7_OFF_SPEED_MS + 4].copy_from_slice(&36.1f32.to_le_bytes()); // ~130 km/h
    buf[GT7_OFF_THROTTLE] = 0;
    buf[GT7_OFF_BRAKE] = 217; // ~85%
    buf[GT7_OFF_GEAR_BYTE] = (3 << 4) | 3; // 3rd gear
    buf[GT7_OFF_FUEL_LEVEL..GT7_OFF_FUEL_LEVEL + 4].copy_from_slice(&18.0f32.to_le_bytes());
    buf[GT7_OFF_FUEL_CAPACITY..GT7_OFF_FUEL_CAPACITY + 4].copy_from_slice(&50.0f32.to_le_bytes());
    buf[GT7_OFF_WATER_TEMP..GT7_OFF_WATER_TEMP + 4].copy_from_slice(&95.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_FL..GT7_OFF_TIRE_TEMP_FL + 4].copy_from_slice(&85.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_FR..GT7_OFF_TIRE_TEMP_FR + 4].copy_from_slice(&88.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_RL..GT7_OFF_TIRE_TEMP_RL + 4].copy_from_slice(&82.0f32.to_le_bytes());
    buf[GT7_OFF_TIRE_TEMP_RR..GT7_OFF_TIRE_TEMP_RR + 4].copy_from_slice(&84.0f32.to_le_bytes());
    write_u16_le(&mut buf, GT7_OFF_LAP_COUNT, 5);
    write_i32_le(&mut buf, GT7_OFF_BEST_LAP_MS, 78_500);
    write_i32_le(&mut buf, GT7_OFF_LAST_LAP_MS, 79_200);
    write_i32_le(&mut buf, GT7_OFF_CAR_CODE, 567);
    // No special flags
    write_u16_le(&mut buf, GT7_OFF_FLAGS, 0);

    // GT Sport shares the same packet format; use parse_decrypted directly.
    let normalized = openracing_telemetry_adapters::gran_turismo_7::parse_decrypted(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Trackmania — time trial snapshot (JSON over UDP)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Scenario: Trackmania 2020 time trial, 4th gear at ~200 km/h (~55.6 m/s),
// full throttle through a drift section.

#[test]
fn trackmania_time_trial_snapshot() -> TestResult {
    let json = br#"{"speed":55.6,"gear":4,"rpm":7200.0,"throttle":1.0,"brake":0.0,"steerAngle":-0.22,"engineRunning":true}"#;
    let adapter = TrackmaniaAdapter::new();
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. AC Rally — probe snapshot (discovery adapter, raw packet)
// ═══════════════════════════════════════════════════════════════════════════════
//
// AC Rally is a discovery-first adapter. normalize() interprets raw bytes as
// a probe packet and emits diagnostic extended fields.

#[test]
fn ac_rally_probe_snapshot() -> TestResult {
    // Simulate a raw probe response packet (16 bytes of arbitrary telemetry-like data).
    let raw: &[u8] = &[
        0x01, 0x04, 0x00, 0x00, // registration result header
        0x2A, 0x00, 0x00, 0x00, // connection_id = 42
        0x01, 0x00, // success=true, readonly=false
        0x02, 0x00, 0x6F, 0x6B, // acc-string "ok" (length 2 + "ok")
        0xFF, 0xFE, // trailing bytes
    ];
    let adapter = ACRallyAdapter::new();
    let normalized = adapter.normalize(raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
