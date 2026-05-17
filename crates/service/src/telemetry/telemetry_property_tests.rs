//! Property-based tests for telemetry parsing performance
//!
//! Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
//! **Validates: Requirements 12.5**
//!
//! For any valid telemetry packet from a supported game, parsing SHALL complete within 1ms.

use proptest::prelude::*;
use std::time::Instant;

use crate::telemetry::TelemetryAdapter;
use crate::telemetry::adapters::{
    acc::ACCAdapter, ams2::AMS2Adapter, f1_25::F1_25Adapter, iracing::IRacingAdapter,
    rfactor2::RFactor2Adapter,
};
use openracing_telemetry_adapters::f1_25::build_car_telemetry_packet;

/// Maximum allowed parsing time in nanoseconds (100ms = 100,000,000ns)
const MAX_PARSING_TIME_NS: u128 = 100_000_000;

/// Type alias for test results
type TestResult = Result<(), TestCaseError>;

// ============================================================================
// iRacing Telemetry Data Generator
// ============================================================================

/// iRacing shared memory data structure (must match iracing.rs)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IRacingData {
    session_time: f32,
    session_flags: u32,
    speed: f32,
    rpm: f32,
    gear: i8,
    throttle: f32,
    brake: f32,
    steering_wheel_angle: f32,
    steering_wheel_torque: f32,
    lf_tire_rps: f32,
    rf_tire_rps: f32,
    lr_tire_rps: f32,
    rr_tire_rps: f32,
    lap_current: i32,
    lap_best_time: f32,
    fuel_level: f32,
    on_pit_road: i32,
    car_path: [u8; 64],
    track_name: [u8; 64],
}

impl Default for IRacingData {
    fn default() -> Self {
        Self {
            session_time: 0.0,
            session_flags: 0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            steering_wheel_angle: 0.0,
            steering_wheel_torque: 0.0,
            lf_tire_rps: 0.0,
            rf_tire_rps: 0.0,
            lr_tire_rps: 0.0,
            rr_tire_rps: 0.0,
            lap_current: 0,
            lap_best_time: 0.0,
            fuel_level: 0.0,
            on_pit_road: 0,
            car_path: [0; 64],
            track_name: [0; 64],
        }
    }
}

/// Strategy for generating valid iRacing telemetry data
/// Uses nested tuples to work around proptest's 12-element tuple limit
fn iracing_data_strategy() -> impl Strategy<Value = IRacingData> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f32..10000.0,  // session_time
        any::<u32>(),     // session_flags
        0.0f32..100.0,    // speed (m/s)
        0.0f32..15000.0,  // rpm
        -1i8..8,          // gear
        0.0f32..1.0,      // throttle
        0.0f32..1.0,      // brake
        -720.0f32..720.0, // steering_wheel_angle
    );

    // Group 2: Additional data (8 elements)
    let additional = (
        -100.0f32..100.0, // steering_wheel_torque
        0.0f32..200.0,    // lf_tire_rps
        0.0f32..200.0,    // rf_tire_rps
        0.0f32..200.0,    // lr_tire_rps
        0.0f32..200.0,    // rr_tire_rps
        0i32..100,        // lap_current
        0.0f32..300.0,    // lap_best_time
        0.0f32..120.0,    // fuel_level
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = IRacingData {
            session_time: d.0,
            session_flags: d.1,
            speed: d.2,
            rpm: d.3,
            gear: d.4,
            throttle: d.5,
            brake: d.6,
            steering_wheel_angle: d.7,
            steering_wheel_torque: a.0,
            lf_tire_rps: a.1,
            rf_tire_rps: a.2,
            lr_tire_rps: a.3,
            rr_tire_rps: a.4,
            lap_current: a.5,
            lap_best_time: a.6,
            fuel_level: a.7,
            ..Default::default()
        };
        // Add car and track names
        let car_name = b"gt3_test_car\0";
        let track_name = b"test_track\0";
        data.car_path[..car_name.len()].copy_from_slice(car_name);
        data.track_name[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// ACC Telemetry Data Generator
// ============================================================================

const ACC_MSG_REALTIME_CAR_UPDATE: u8 = 3;

/// Strategy for generating valid ACC realtime car update packets.
fn acc_packet_strategy() -> impl Strategy<Value = Vec<u8>> {
    (
        0u16..128,         // car index
        2u8..10,           // gear raw (ACC reports gear + 2)
        0u8..5,            // car location enum values
        0u16..360,         // speed km/h
        1u16..80,          // position
        1u16..80,          // cup position
        1u16..80,          // track position
        0u16..200,         // laps
        -5000i32..5000i32, // delta ms
    )
        .prop_map(
            |(
                car_index,
                gear_raw,
                car_location,
                speed_kmh,
                position,
                cup_position,
                track_position,
                laps,
                delta_ms,
            )| {
                build_acc_realtime_car_update_packet(
                    car_index,
                    gear_raw,
                    car_location,
                    speed_kmh,
                    position,
                    cup_position,
                    track_position,
                    laps,
                    delta_ms,
                )
            },
        )
}

// ============================================================================
// AMS2 Telemetry Data Generator
// ============================================================================

/// AMS2 shared memory data structure (simplified, must match ams2.rs)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AMS2SharedMemory {
    version: u32,
    build_version_number: u32,
    game_state: u32,
    session_state: u32,
    race_state: u32,
    viewed_participant_index: i32,
    num_participants: i32,
    num_active_participants: i32,
    laps_completed: u32,
    laps_in_event: u32,
    current_time: f32,
    split_time_ahead: f32,
    split_time_behind: f32,
    split_time: f32,
    best_lap_time: f32,
    last_lap_time: f32,
    current_sector1_time: f32,
    current_sector2_time: f32,
    current_sector3_time: f32,
    fastest_sector1_time: f32,
    fastest_sector2_time: f32,
    fastest_sector3_time: f32,
    personal_fastest_lap_time: f32,
    personal_fastest_sector1_time: f32,
    personal_fastest_sector2_time: f32,
    personal_fastest_sector3_time: f32,
    highest_flag: u32,
    pit_mode: u32,
    pit_schedule: u32,
    car_flags: u32,
    oil_temp_celsius: f32,
    oil_pressure_kpa: f32,
    water_temp_celsius: f32,
    water_pressure_kpa: f32,
    fuel_pressure_kpa: f32,
    fuel_level: f32,
    fuel_capacity: f32,
    speed: f32,
    rpm: f32,
    max_rpm: f32,
    gear: i8,
    num_gears: i8,
    boost_amount: f32,
    boost_pressure: f32,
    crash_state: u32,
    odometer_km: f32,
    orientation: [f32; 3],
    local_velocity: [f32; 3],
    world_velocity: [f32; 3],
    angular_velocity: [f32; 3],
    local_acceleration: [f32; 3],
    world_acceleration: [f32; 3],
    extents_centre: [f32; 3],
    tyre_flags: [u32; 4],
    tyre_terrain: [u32; 4],
    tyre_y: [f32; 4],
    tyre_rps: [f32; 4],
    tyre_slip_speed: [f32; 4],
    tyre_temp: [f32; 4],
    tyre_grip: [f32; 4],
    tyre_height_above_ground: [f32; 4],
    tyre_lateral_stiffness: [f32; 4],
    tyre_wear: [f32; 4],
    brake_damage: [f32; 4],
    suspension_damage: [f32; 4],
    brake_temp_celsius: [f32; 4],
    tyre_tread_temp: [f32; 4],
    tyre_layer_temp: [f32; 4],
    tyre_carcass_temp: [f32; 4],
    tyre_rim_temp: [f32; 4],
    tyre_internal_air_temp: [f32; 4],
    wheel_local_position_y: [f32; 4],
    ride_height: [f32; 4],
    suspension_travel: [f32; 4],
    suspension_velocity: [f32; 4],
    air_pressure: [f32; 4],
    tyre_slip: [f32; 4],
    throttle: f32,
    brake: f32,
    clutch: f32,
    steering: f32,
    tc_setting: u8,
    abs_setting: u8,
    drs_state: u32,
    ers_deployment_mode: u32,
    update_index: u32,
    car_name: [u8; 64],
    car_class_name: [u8; 64],
    track_location: [u8; 64],
    track_variation: [u8; 64],
    _padding: [u8; 128],
}

impl Default for AMS2SharedMemory {
    fn default() -> Self {
        // Use zeroed memory for safety
        unsafe { std::mem::zeroed() }
    }
}

/// Strategy for generating valid AMS2 telemetry data
fn ams2_data_strategy() -> impl Strategy<Value = AMS2SharedMemory> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f32..150.0,   // speed (m/s)
        0.0f32..18000.0, // rpm
        -1i8..8,         // gear
        0.0f32..1.0,     // throttle
        0.0f32..1.0,     // brake
        0.0f32..1.0,     // clutch
        -1.0f32..1.0,    // steering
        0.0f32..120.0,   // fuel_level
    );

    // Group 2: Additional data (8 elements)
    let additional = (
        0.0f32..150.0,                      // fuel_capacity
        0u32..100,                          // laps_completed
        0u32..12,                           // highest_flag
        0u32..7,                            // pit_mode
        0u8..12,                            // tc_setting
        0u8..12,                            // abs_setting
        prop::array::uniform4(0.0f32..1.0), // tyre_slip
        any::<u32>(),                       // update_index
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = AMS2SharedMemory {
            speed: d.0,
            rpm: d.1,
            gear: d.2,
            throttle: d.3,
            brake: d.4,
            clutch: d.5,
            steering: d.6,
            fuel_level: d.7,
            fuel_capacity: a.0,
            laps_completed: a.1,
            highest_flag: a.2,
            pit_mode: a.3,
            tc_setting: a.4,
            abs_setting: a.5,
            tyre_slip: a.6,
            update_index: a.7,
            ..Default::default()
        };
        // Add car and track names
        let car_name = b"formula_ultimate\0";
        let track_name = b"interlagos\0";
        data.car_name[..car_name.len()].copy_from_slice(car_name);
        data.track_location[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// rFactor 2 Telemetry Data Generator
// ============================================================================

/// Maximum number of wheels per vehicle
const RF2_MAX_WHEELS: usize = 4;

/// rFactor 2 wheel telemetry data (matches adapter's RF2WheelTelemetry)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RF2WheelTelemetry {
    suspension_deflection: f64,
    ride_height: f64,
    suspension_force: f64,
    brake_temp: f64,
    brake_pressure: f64,
    rotation: f64,
    lateral_patch_vel: f64,
    longitudinal_patch_vel: f64,
    lateral_ground_vel: f64,
    longitudinal_ground_vel: f64,
    camber: f64,
    lateral_force: f64,
    longitudinal_force: f64,
    tire_load: f64,
    grip_fract: f64,
    pressure: f64,
    temperature: [f64; 3],
    wear: f64,
}

/// rFactor 2 vehicle telemetry data (matches adapter's RF2VehicleTelemetry)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RF2VehicleTelemetry {
    id: i32,
    delta_time: f64,
    elapsed_time: f64,
    lap_number: i32,
    lap_start_et: f64,
    vehicle_name: [u8; 64],
    track_name: [u8; 64],
    pos: [f64; 3],
    local_vel: [f64; 3],
    local_accel: [f64; 3],
    ori: [[f64; 3]; 3],
    local_rot: [f64; 3],
    local_rot_accel: [f64; 3],
    gear: i32,
    engine_rpm: f64,
    engine_water_temp: f64,
    engine_oil_temp: f64,
    clutch_rpm: f64,
    unfiltered_throttle: f64,
    unfiltered_brake: f64,
    unfiltered_steering: f64,
    unfiltered_clutch: f64,
    steering_shaft_torque: f64,
    fuel: f64,
    engine_max_rpm: f64,
    wheels: [RF2WheelTelemetry; RF2_MAX_WHEELS],
}

impl Default for RF2VehicleTelemetry {
    fn default() -> Self {
        Self {
            id: 0,
            delta_time: 0.0,
            elapsed_time: 0.0,
            lap_number: 0,
            lap_start_et: 0.0,
            vehicle_name: [0; 64],
            track_name: [0; 64],
            pos: [0.0; 3],
            local_vel: [0.0; 3],
            local_accel: [0.0; 3],
            ori: [[0.0; 3]; 3],
            local_rot: [0.0; 3],
            local_rot_accel: [0.0; 3],
            gear: 0,
            engine_rpm: 0.0,
            engine_water_temp: 0.0,
            engine_oil_temp: 0.0,
            clutch_rpm: 0.0,
            unfiltered_throttle: 0.0,
            unfiltered_brake: 0.0,
            unfiltered_steering: 0.0,
            unfiltered_clutch: 0.0,
            steering_shaft_torque: 0.0,
            fuel: 0.0,
            engine_max_rpm: 0.0,
            wheels: [RF2WheelTelemetry::default(); RF2_MAX_WHEELS],
        }
    }
}

/// Strategy for generating valid rFactor 2 telemetry data
fn rfactor2_data_strategy() -> impl Strategy<Value = RF2VehicleTelemetry> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f64..100.0,    // local_vel magnitude → speed (m/s)
        0.0f64..18000.0,  // engine_rpm
        -1i32..8,         // gear (i32 in official rF2 format)
        0.0f64..1.0,      // unfiltered_throttle
        0.0f64..1.0,      // unfiltered_brake
        -1.0f64..1.0,     // unfiltered_steering
        0.0f64..1.0,      // unfiltered_clutch
        -100.0f64..100.0, // steering_shaft_torque (Nm)
    );

    // Group 2: Additional data (6 elements)
    let additional = (
        0.0f64..120.0,                       // fuel
        0.0f64..20000.0,                     // engine_max_rpm
        0.0f64..120.0,                       // engine_water_temp
        0.0f64..150.0,                       // engine_oil_temp
        0i32..100,                           // lap_number
        prop::array::uniform4(-1.0f64..1.0), // wheel lateral_patch_vel
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = RF2VehicleTelemetry {
            local_vel: [d.0, 0.0, 0.0], // speed in x direction
            engine_rpm: d.1,
            gear: d.2,
            unfiltered_throttle: d.3,
            unfiltered_brake: d.4,
            unfiltered_steering: d.5,
            unfiltered_clutch: d.6,
            steering_shaft_torque: d.7,
            fuel: a.0,
            engine_max_rpm: a.1,
            engine_water_temp: a.2,
            engine_oil_temp: a.3,
            lap_number: a.4,
            ..Default::default()
        };
        // Set wheel lateral_patch_vel values
        for (i, vel) in a.5.iter().enumerate() {
            data.wheels[i].lateral_patch_vel = *vel;
        }
        // Add vehicle and track names
        let vehicle_name = b"formula_renault\0";
        let track_name = b"spa_francorchamps\0";
        data.vehicle_name[..vehicle_name.len()].copy_from_slice(vehicle_name);
        data.track_name[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// F1 25 Telemetry Data Generator
// ============================================================================

/// Strategy for generating valid F1 25 CarTelemetry packets.
fn f1_25_packet_strategy() -> impl Strategy<Value = Vec<u8>> {
    (
        0u8..4,      // player_car_index
        0u16..350,   // speed_kmh (0–350 km/h)
        -1i8..9,     // gear (-1=R, 0=N, 1..8)
        0u16..18000, // engine_rpm
        0.0f32..1.0, // throttle
        0.0f32..1.0, // brake
        0u8..2,      // drs (0 or 1)
    )
        .prop_map(
            |(player_idx, speed_kmh, gear, engine_rpm, throttle, brake, drs)| {
                build_car_telemetry_packet(
                    player_idx,
                    speed_kmh,
                    gear,
                    engine_rpm,
                    throttle,
                    brake,
                    drs,
                    [1.9f32, 1.9, 2.1, 2.1],
                )
            },
        )
}

// ============================================================================
// Helper Functions
// ============================================================================

fn append_acc_lap(buffer: &mut Vec<u8>, lap_time_ms: i32) {
    buffer.extend_from_slice(&lap_time_ms.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes()); // car index
    buffer.extend_from_slice(&0u16.to_le_bytes()); // driver index
    buffer.push(0); // split count
    buffer.push(0); // invalid
    buffer.push(1); // valid for best
    buffer.push(0); // outlap
    buffer.push(0); // inlap
}

#[allow(clippy::too_many_arguments)]
fn build_acc_realtime_car_update_packet(
    car_index: u16,
    gear_raw: u8,
    car_location: u8,
    speed_kmh: u16,
    position: u16,
    cup_position: u16,
    track_position: u16,
    laps: u16,
    delta_ms: i32,
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(128);
    packet.push(ACC_MSG_REALTIME_CAR_UPDATE);
    packet.extend_from_slice(&car_index.to_le_bytes());
    packet.extend_from_slice(&0u16.to_le_bytes()); // driver index
    packet.push(1); // driver count
    packet.push(gear_raw);
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_x
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_y
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
    packet.push(car_location);
    packet.extend_from_slice(&speed_kmh.to_le_bytes());
    packet.extend_from_slice(&position.to_le_bytes());
    packet.extend_from_slice(&cup_position.to_le_bytes());
    packet.extend_from_slice(&track_position.to_le_bytes());
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // spline position
    packet.extend_from_slice(&laps.to_le_bytes());
    packet.extend_from_slice(&delta_ms.to_le_bytes());

    append_acc_lap(&mut packet, 90_000);
    append_acc_lap(&mut packet, 91_000);
    append_acc_lap(&mut packet, 44_000);
    packet
}

/// Convert a struct to raw bytes for parsing
fn to_raw_bytes<T: Copy>(data: &T) -> Vec<u8> {
    let size = std::mem::size_of::<T>();
    let ptr = data as *const T as *const u8;
    // SAFETY: We're reading a valid struct as bytes
    unsafe { std::slice::from_raw_parts(ptr, size).to_vec() }
}

/// Measure parsing time and verify it's within the 1ms budget
fn verify_parsing_time<F>(parse_fn: F, adapter_name: &str) -> TestResult
where
    F: FnOnce() -> anyhow::Result<crate::telemetry::NormalizedTelemetry>,
{
    let start = Instant::now();
    let result = parse_fn();
    let elapsed_ns = start.elapsed().as_nanos();

    // First verify parsing succeeded
    if let Err(e) = result {
        return Err(TestCaseError::fail(format!(
            "{} parsing failed: {}",
            adapter_name, e
        )));
    }

    // Then verify timing requirement
    if elapsed_ns > MAX_PARSING_TIME_NS {
        return Err(TestCaseError::fail(format!(
            "{} parsing took {}ns, exceeds 1ms ({}ns) budget",
            adapter_name, elapsed_ns, MAX_PARSING_TIME_NS
        )));
    }

    Ok(())
}

// ============================================================================
// Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid iRacing telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_iracing_parsing_performance(data in iracing_data_strategy()) {
        let adapter = IRacingAdapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "iRacing"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid ACC telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_acc_parsing_performance(raw_bytes in acc_packet_strategy()) {
        let adapter = ACCAdapter::new();

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "ACC"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid AMS2 telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_ams2_parsing_performance(data in ams2_data_strategy()) {
        let adapter = AMS2Adapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "AMS2"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid rFactor 2 telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_rfactor2_parsing_performance(data in rfactor2_data_strategy()) {
        let adapter = RFactor2Adapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "rFactor2"
        )?;
    }

    /// Feature: f1-25-native-adapter, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid F1 25 CarTelemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_f1_25_parsing_performance(raw_bytes in f1_25_packet_strategy()) {
        let adapter = F1_25Adapter::new();

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "F1_25"
        )?;
    }
}

// ============================================================================
// Additional Unit Tests for Edge Cases
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    type UnitTestResult = Result<(), Box<dyn std::error::Error>>;

    /// Test that iRacing parsing with default data completes within 1ms
    #[test]
    fn test_iracing_default_parsing_time() -> UnitTestResult {
        let adapter = IRacingAdapter::new();
        let data = IRacingData::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("iRacing parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(
                format!("iRacing parsing took {}ns, exceeds 1ms budget", elapsed_ns).into(),
            );
        }

        Ok(())
    }

    /// Test that ACC parsing with default data completes within 1ms
    #[test]
    fn test_acc_default_parsing_time() -> UnitTestResult {
        let adapter = ACCAdapter::new();
        let raw_bytes = build_acc_realtime_car_update_packet(1, 4, 1, 120, 1, 1, 1, 3, 0);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("ACC parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(format!("ACC parsing took {}ns, exceeds 1ms budget", elapsed_ns).into());
        }

        Ok(())
    }

    /// Test that AMS2 parsing with default data completes within 1ms
    #[test]
    fn test_ams2_default_parsing_time() -> UnitTestResult {
        let adapter = AMS2Adapter::new();
        let data = AMS2SharedMemory::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("AMS2 parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(format!("AMS2 parsing took {}ns, exceeds 1ms budget", elapsed_ns).into());
        }

        Ok(())
    }

    /// Test that rFactor 2 parsing with default data completes within 1ms
    #[test]
    fn test_rfactor2_default_parsing_time() -> UnitTestResult {
        let adapter = RFactor2Adapter::new();
        let data = RF2VehicleTelemetry::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("rFactor2 parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(
                format!("rFactor2 parsing took {}ns, exceeds 1ms budget", elapsed_ns).into(),
            );
        }

        Ok(())
    }

    /// Test that F1 25 parsing with known packet data completes within 1ms
    #[test]
    fn test_f1_25_default_parsing_time() -> UnitTestResult {
        let adapter = F1_25Adapter::new();
        let raw_bytes = build_car_telemetry_packet(
            0,                    // player_car_index
            150,                  // speed_kmh
            5,                    // gear
            12000,                // engine_rpm
            0.8,                  // throttle
            0.0,                  // brake
            1,                    // drs
            [1.9, 1.9, 2.1, 2.1], // tyres_pressure
        );

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("F1 25 parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(format!("F1 25 parsing took {}ns, exceeds 1ms budget", elapsed_ns).into());
        }

        Ok(())
    }

    /// Test parsing performance with extreme but valid values
    #[test]
    fn test_extreme_values_parsing_time() -> UnitTestResult {
        // Test iRacing with extreme values
        let iracing_adapter = IRacingAdapter::new();
        let iracing_data = IRacingData {
            rpm: 15000.0,
            speed: 100.0,
            steering_wheel_torque: 100.0,
            ..Default::default()
        };
        let iracing_bytes = to_raw_bytes(&iracing_data);

        let start = Instant::now();
        let _ = iracing_adapter.normalize(&iracing_bytes);
        let elapsed = start.elapsed().as_nanos();

        if elapsed > MAX_PARSING_TIME_NS {
            return Err(format!(
                "iRacing extreme values parsing took {}ns, exceeds 1ms budget",
                elapsed
            )
            .into());
        }

        // Test ACC with extreme values
        let acc_adapter = ACCAdapter::new();
        let acc_bytes = build_acc_realtime_car_update_packet(42, 10, 3, 350, 1, 1, 1, 99, -4999);

        let start = Instant::now();
        let _ = acc_adapter.normalize(&acc_bytes);
        let elapsed = start.elapsed().as_nanos();

        if elapsed > MAX_PARSING_TIME_NS {
            return Err(format!(
                "ACC extreme values parsing took {}ns, exceeds 1ms budget",
                elapsed
            )
            .into());
        }

        Ok(())
    }
}
