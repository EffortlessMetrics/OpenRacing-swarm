//! Insta snapshot tests for the AMS2 telemetry adapter.
//!
//! These tests lock down the normalized output format so that any change to
//! the adapter's output is caught as a snapshot diff.

use openracing_telemetry_adapters::ams2::{AMS2SharedMemory, HighestFlag, PitMode, SessionState};
use racing_wheel_telemetry_ams2::{AMS2Adapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn adapter() -> AMS2Adapter {
    AMS2Adapter::new()
}

fn shared_memory_to_bytes(data: &AMS2SharedMemory) -> Vec<u8> {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let ptr = data as *const AMS2SharedMemory as *const u8;
    // SAFETY: AMS2SharedMemory is repr(C) and fully initialized via Default.
    unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec()
}

fn default_shared_memory() -> AMS2SharedMemory {
    AMS2SharedMemory::default()
}

fn set_car_name(data: &mut AMS2SharedMemory, name: &[u8]) {
    data.car_name[..name.len()].copy_from_slice(name);
}

fn set_track_location(data: &mut AMS2SharedMemory, name: &[u8]) {
    data.track_location[..name.len()].copy_from_slice(name);
}

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

/// Snapshot: normal GT3 race lap at ~200 km/h.
#[test]
fn snapshot_ams2_gt3_race() -> TestResult {
    let mut data = default_shared_memory();

    // Session / race state
    data.game_state = 2; // InGamePlaying
    data.session_state = SessionState::Race as u32;
    data.race_state = 2; // Racing
    data.highest_flag = HighestFlag::Green as u32;

    // Car identity
    set_car_name(&mut data, b"mclaren_720s_gt3");
    set_track_location(&mut data, b"interlagos");

    // Motion
    data.speed = 55.5; // ~200 km/h
    data.rpm = 7200.0;
    data.max_rpm = 8500.0;
    data.gear = 5;
    data.num_gears = 6;
    data.steering = 0.12;

    // Controls
    data.throttle = 0.85;
    data.brake = 0.0;
    data.clutch = 0.0;

    // Tyres — warm, normal grip
    data.tyre_temp = [92.0, 94.0, 88.0, 90.0];
    data.tyre_grip = [1.0, 1.0, 0.98, 0.99];
    data.tyre_slip = [0.03, 0.04, 0.05, 0.04];
    data.tyre_wear = [0.05, 0.06, 0.03, 0.04];

    // Electronics
    data.tc_setting = 3;
    data.abs_setting = 2;

    // Fuel / laps
    data.fuel_level = 42.0;
    data.fuel_capacity = 110.0;
    data.laps_completed = 8;
    data.laps_in_event = 25;

    let norm = adapter().normalize(&shared_memory_to_bytes(&data))?;
    insta::assert_yaml_snapshot!("ams2_gt3_race_frame", norm);
    Ok(())
}

/// Snapshot: wet conditions with reduced grip and yellow flag.
#[test]
fn snapshot_ams2_wet_conditions() -> TestResult {
    let mut data = default_shared_memory();

    // Session / race state
    data.game_state = 2; // InGamePlaying
    data.session_state = SessionState::Race as u32;
    data.race_state = 2; // Racing
    data.highest_flag = HighestFlag::Yellow as u32;

    // Car identity
    set_car_name(&mut data, b"porsche_911_gt3_r");
    set_track_location(&mut data, b"spa_francorchamps");

    // Motion — slower in the wet
    data.speed = 34.7; // ~125 km/h
    data.rpm = 5500.0;
    data.max_rpm = 8200.0;
    data.gear = 3;
    data.num_gears = 6;
    data.steering = -0.38;

    // Controls — cautious inputs
    data.throttle = 0.55;
    data.brake = 0.10;
    data.clutch = 0.0;

    // Tyres — lower grip, higher slip in wet
    data.tyre_temp = [68.0, 70.0, 64.0, 66.0];
    data.tyre_grip = [0.72, 0.70, 0.68, 0.69];
    data.tyre_slip = [0.18, 0.20, 0.22, 0.21];
    data.tyre_wear = [0.12, 0.14, 0.08, 0.10];

    // Electronics — higher TC for wet
    data.tc_setting = 6;
    data.abs_setting = 4;

    // Fuel / laps
    data.fuel_level = 65.0;
    data.fuel_capacity = 110.0;
    data.laps_completed = 3;
    data.laps_in_event = 20;

    let norm = adapter().normalize(&shared_memory_to_bytes(&data))?;
    insta::assert_yaml_snapshot!("ams2_wet_conditions_frame", norm);
    Ok(())
}

/// Snapshot: practice session, car stationary in pits.
#[test]
fn snapshot_ams2_practice_session() -> TestResult {
    let mut data = default_shared_memory();

    // Session / race state
    data.game_state = 2; // InGamePlaying
    data.session_state = SessionState::Practice as u32;
    data.race_state = 2; // Racing (active in practice)
    data.highest_flag = HighestFlag::None as u32;
    data.pit_mode = PitMode::InGarage as u32;

    // Car identity
    set_car_name(&mut data, b"ferrari_488_gt3_evo");
    set_track_location(&mut data, b"nurburgring_gp");

    // Motion — stationary
    data.speed = 0.0;
    data.rpm = 850.0;
    data.max_rpm = 8000.0;
    data.gear = 0; // neutral
    data.num_gears = 6;
    data.steering = 0.0;

    // Controls — idle
    data.throttle = 0.0;
    data.brake = 0.0;
    data.clutch = 0.0;

    // Tyres — cold, no wear
    data.tyre_temp = [28.0, 28.0, 27.0, 27.0];
    data.tyre_grip = [0.85, 0.85, 0.85, 0.85];
    data.tyre_slip = [0.0, 0.0, 0.0, 0.0];
    data.tyre_wear = [0.0, 0.0, 0.0, 0.0];

    // Electronics
    data.tc_setting = 0;
    data.abs_setting = 0;

    // Fuel / laps
    data.fuel_level = 110.0;
    data.fuel_capacity = 110.0;
    data.laps_completed = 0;
    data.laps_in_event = 0;

    let norm = adapter().normalize(&shared_memory_to_bytes(&data))?;
    insta::assert_yaml_snapshot!("ams2_practice_session_frame", norm);
    Ok(())
}
