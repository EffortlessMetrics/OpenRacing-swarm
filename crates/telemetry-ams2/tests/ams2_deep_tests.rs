//! Extended deep tests for the AMS2 telemetry adapter.
//!
//! Covers session types, game states, all pit mode transitions, multi-flag
//! exclusivity, weather/tire edge cases, boost pressure, crash state,
//! extended data completeness, and shared memory struct integrity.

use openracing_telemetry_adapters::ams2::{
    AMS2Adapter, AMS2SharedMemory, DrsState, GameState, HighestFlag, PitMode, RaceState,
    SessionState,
};
use racing_wheel_telemetry_ams2::TelemetryAdapter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn to_bytes(data: &AMS2SharedMemory) -> Vec<u8> {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let ptr = data as *const AMS2SharedMemory as *const u8;
    // SAFETY: AMS2SharedMemory is repr(C) and fully initialized via Default.
    unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec()
}

fn write_str(buf: &mut [u8; 64], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(63);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
}

fn default_mem() -> AMS2SharedMemory {
    AMS2SharedMemory::default()
}

// ── Session state enum coverage ──────────────────────────────────────────────

#[test]
fn session_state_discriminants_are_distinct() -> TestResult {
    let variants = [
        SessionState::Invalid,
        SessionState::Practice,
        SessionState::Test,
        SessionState::Qualify,
        SessionState::FormationLap,
        SessionState::Race,
        SessionState::TimeAttack,
    ];
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(*a as u32, *b as u32, "{a:?} vs {b:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn session_state_values_match_sdk() -> TestResult {
    assert_eq!(SessionState::Invalid as u32, 0);
    assert_eq!(SessionState::Practice as u32, 1);
    assert_eq!(SessionState::Test as u32, 2);
    assert_eq!(SessionState::Qualify as u32, 3);
    assert_eq!(SessionState::FormationLap as u32, 4);
    assert_eq!(SessionState::Race as u32, 5);
    assert_eq!(SessionState::TimeAttack as u32, 6);
    Ok(())
}

// ── Game state enum coverage ─────────────────────────────────────────────────

#[test]
fn game_state_discriminants_match_ams2_v9_sdk() -> TestResult {
    assert_eq!(GameState::Exited as u32, 0);
    assert_eq!(GameState::FrontEnd as u32, 1);
    assert_eq!(GameState::InGamePlaying as u32, 2);
    assert_eq!(GameState::InGamePaused as u32, 3);
    assert_eq!(GameState::InGameInMenuTimeTicking as u32, 4);
    assert_eq!(GameState::InGameRestarting as u32, 5);
    assert_eq!(GameState::InGameReplay as u32, 6);
    assert_eq!(GameState::FrontEndReplay as u32, 7);
    Ok(())
}

#[test]
fn game_state_struct_field_round_trip() -> TestResult {
    let adapter = AMS2Adapter::new();
    for state_val in 0..=7u32 {
        let mut data = default_mem();
        data.game_state = state_val;
        let raw = to_bytes(&data);
        // Normalization should succeed regardless of game state value.
        let _t = adapter.normalize(&raw)?;
    }
    Ok(())
}

// ── Race state enum coverage ─────────────────────────────────────────────────

#[test]
fn race_state_discriminants_match_sdk() -> TestResult {
    assert_eq!(RaceState::Invalid as u32, 0);
    assert_eq!(RaceState::NotStarted as u32, 1);
    assert_eq!(RaceState::Racing as u32, 2);
    assert_eq!(RaceState::Finished as u32, 3);
    assert_eq!(RaceState::Disqualified as u32, 4);
    assert_eq!(RaceState::Retired as u32, 5);
    assert_eq!(RaceState::DnsDidNotStart as u32, 6);
    Ok(())
}

#[test]
fn race_state_round_trip_through_normalize() -> TestResult {
    let adapter = AMS2Adapter::new();
    for state_val in 0..=6u32 {
        let mut data = default_mem();
        data.race_state = state_val;
        let raw = to_bytes(&data);
        let _t = adapter.normalize(&raw)?;
    }
    Ok(())
}

// ── Pit mode exhaustive transitions ──────────────────────────────────────────

#[test]
fn all_pit_modes_set_in_pits_correctly() -> TestResult {
    let adapter = AMS2Adapter::new();
    let pit_modes = [
        (PitMode::None, false),
        (PitMode::DrivingIntoPits, true),
        (PitMode::InPit, true),
        (PitMode::DrivingOutOfPits, true),
        (PitMode::InGarage, true),
        (PitMode::DrivingOutOfGarage, true),
        (PitMode::InPitlane, true),
    ];
    for (mode, expected_in_pits) in &pit_modes {
        let mut data = default_mem();
        data.pit_mode = *mode as u32;
        let t = adapter.normalize(&to_bytes(&data))?;
        assert_eq!(
            t.flags.in_pits, *expected_in_pits,
            "PitMode::{mode:?} in_pits mismatch"
        );
    }
    Ok(())
}

#[test]
fn pit_limiter_only_for_in_pitlane() -> TestResult {
    let adapter = AMS2Adapter::new();
    let pit_modes = [
        (PitMode::None, false),
        (PitMode::DrivingIntoPits, false),
        (PitMode::InPit, false),
        (PitMode::InPitlane, true),
    ];
    for (mode, expected_limiter) in &pit_modes {
        let mut data = default_mem();
        data.pit_mode = *mode as u32;
        let t = adapter.normalize(&to_bytes(&data))?;
        assert_eq!(
            t.flags.pit_limiter, *expected_limiter,
            "PitMode::{mode:?} pit_limiter mismatch"
        );
    }
    Ok(())
}

// ── Flag mutual exclusivity ─────────────────────────────────────────────────

#[test]
fn flags_are_mutually_exclusive() -> TestResult {
    let adapter = AMS2Adapter::new();
    let flag_values = [
        HighestFlag::None,
        HighestFlag::Green,
        HighestFlag::Blue,
        HighestFlag::WhiteSlowCar,
        HighestFlag::WhiteFinalLap,
        HighestFlag::Red,
        HighestFlag::Yellow,
        HighestFlag::DoubleYellow,
        HighestFlag::BlackAndWhite,
        HighestFlag::BlackOrangeCircle,
        HighestFlag::Black,
        HighestFlag::Chequered,
    ];
    for flag in &flag_values {
        let mut data = default_mem();
        data.highest_flag = *flag as u32;
        let t = adapter.normalize(&to_bytes(&data))?;
        let active_count = [
            t.flags.green_flag,
            t.flags.yellow_flag,
            t.flags.red_flag,
            t.flags.blue_flag,
            t.flags.checkered_flag,
        ]
        .iter()
        .filter(|&&f| f)
        .count();
        assert!(
            active_count <= 1,
            "HighestFlag::{flag:?} set {active_count} flags (expected 0 or 1)"
        );
    }
    Ok(())
}

#[test]
fn no_flags_when_highest_flag_is_none() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.highest_flag = HighestFlag::None as u32;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(!t.flags.green_flag);
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.red_flag);
    assert!(!t.flags.blue_flag);
    assert!(!t.flags.checkered_flag);
    Ok(())
}

// ── DRS state exhaustive ─────────────────────────────────────────────────────

#[test]
fn drs_all_states_coverage() -> TestResult {
    let adapter = AMS2Adapter::new();
    let cases = [
        (DrsState::Installed, false, false),
        (DrsState::Available, true, false),
        (DrsState::Active, false, true),
    ];
    for (state, expect_avail, expect_active) in &cases {
        let mut data = default_mem();
        data.drs_state = *state as u32;
        let t = adapter.normalize(&to_bytes(&data))?;
        assert_eq!(
            t.flags.drs_available, *expect_avail,
            "DrsState::{state:?} available"
        );
        assert_eq!(
            t.flags.drs_active, *expect_active,
            "DrsState::{state:?} active"
        );
    }
    Ok(())
}

// ── Slip ratio edge cases ────────────────────────────────────────────────────

#[test]
fn slip_ratio_clamped_to_one() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 50.0;
    data.tyre_slip = [5.0, 5.0, 5.0, 5.0]; // extreme slip
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(
        t.slip_ratio <= 1.0,
        "slip should be clamped to 1.0, got {}",
        t.slip_ratio
    );
    Ok(())
}

#[test]
fn slip_ratio_boundary_at_speed_one() -> TestResult {
    let adapter = AMS2Adapter::new();
    // Speed exactly 1.0 is not > 1.0 → slip should be 0.
    let mut data = default_mem();
    data.speed = 1.0;
    data.tyre_slip = [0.5, 0.5, 0.5, 0.5];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.slip_ratio, 0.0, "speed=1.0 is at boundary");

    // Speed just above 1.0
    let mut data2 = default_mem();
    data2.speed = 1.001;
    data2.tyre_slip = [0.5, 0.5, 0.5, 0.5];
    let t2 = adapter.normalize(&to_bytes(&data2))?;
    assert!(t2.slip_ratio > 0.0, "speed>1.0 should yield non-zero slip");
    Ok(())
}

// ── Tire data edge cases ─────────────────────────────────────────────────────

#[test]
fn tire_temps_negative_clamp_to_zero() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.tyre_temp = [-10.0, 0.0, 255.0, 300.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.tire_temps_c[0], 0, "negative temp → 0");
    assert_eq!(t.tire_temps_c[1], 0, "zero temp");
    assert_eq!(t.tire_temps_c[2], 255, "max u8 temp");
    assert_eq!(t.tire_temps_c[3], 255, ">255 clamped to 255");
    Ok(())
}

#[test]
fn tire_pressures_zero_kpa_yields_zero_psi() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.air_pressure = [0.0, 0.0, 0.0, 0.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    for &psi in &t.tire_pressures_psi {
        assert_eq!(psi, 0.0, "zero kPa → zero PSI");
    }
    Ok(())
}

#[test]
fn tire_pressures_negative_kpa_yields_zero_psi() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.air_pressure = [-50.0, -1.0, 0.0, 100.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.tire_pressures_psi[0], 0.0, "negative kPa → 0 PSI");
    assert_eq!(t.tire_pressures_psi[1], 0.0, "negative kPa → 0 PSI");
    assert!(t.tire_pressures_psi[3] > 0.0, "positive kPa → positive PSI");
    Ok(())
}

// ── Boost pressure and crash state ──────────────────────────────────────────

#[test]
fn boost_pressure_in_extended() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.boost_pressure = 1.5;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(
        t.extended.contains_key("boost_pressure"),
        "boost_pressure should be in extended"
    );
    Ok(())
}

#[test]
fn zero_boost_pressure_omitted_from_extended() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem(); // boost_pressure defaults to 0.0
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(
        !t.extended.contains_key("boost_pressure"),
        "zero boost_pressure should be omitted"
    );
    Ok(())
}

// ── Fuel capacity and level extended ─────────────────────────────────────────

#[test]
fn fuel_level_and_capacity_in_extended() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.fuel_level = 35.0;
    data.fuel_capacity = 100.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(t.extended.contains_key("fuel_level_l"));
    assert!(t.extended.contains_key("fuel_capacity_l"));
    Ok(())
}

#[test]
fn zero_fuel_level_omitted_from_extended() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(!t.extended.contains_key("fuel_level_l"));
    assert!(!t.extended.contains_key("fuel_capacity_l"));
    Ok(())
}

// ── String extraction edge cases ─────────────────────────────────────────────

#[test]
fn car_name_max_length_no_null() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    // Fill entire car_name with non-zero bytes (no null terminator)
    data.car_name = [b'A'; 64];
    let t = adapter.normalize(&to_bytes(&data))?;
    let car_id = t.car_id.as_deref().unwrap_or("");
    assert_eq!(car_id.len(), 64, "full 64-byte string without null");
    Ok(())
}

#[test]
fn track_name_with_utf8_safe_bytes() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    write_str(&mut data.track_location, "Nurburgring");
    let t = adapter.normalize(&to_bytes(&data))?;
    let track = t.track_id.as_deref().unwrap_or("");
    assert!(
        track.contains("rburgring"),
        "track should contain partial match"
    );
    Ok(())
}

// ── Lap count edge cases ─────────────────────────────────────────────────────

#[test]
fn lap_count_u16_overflow_clamped() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.laps_completed = 100_000; // > u16::MAX
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.lap, u16::MAX, "laps clamped to u16::MAX");
    Ok(())
}

#[test]
fn zero_laps_remain_zero() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.lap, 0);
    Ok(())
}

// ── Num gears ────────────────────────────────────────────────────────────────

#[test]
fn num_gears_populated_when_positive() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.num_gears = 6;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn num_gears_zero_remains_zero() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.num_gears, 0);
    Ok(())
}

// ── Engine temp edge cases ───────────────────────────────────────────────────

#[test]
fn zero_water_temp_not_written() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem(); // water_temp_celsius = 0.0
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(
        t.engine_temp_c, 0.0,
        "zero water temp → engine_temp_c stays 0"
    );
    Ok(())
}

// ── Combined scenario: full race lap ─────────────────────────────────────────

#[test]
fn full_race_lap_scenario() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut car_name = [0u8; 64];
    let mut track_name = [0u8; 64];
    write_str(&mut car_name, "porsche_911_gt3_r");
    write_str(&mut track_name, "bathurst");

    let mut data = AMS2SharedMemory::default();
    data.version = 9;
    data.game_state = GameState::InGamePlaying as u32;
    data.session_state = SessionState::Race as u32;
    data.race_state = RaceState::Racing as u32;
    data.speed = 65.0;
    data.rpm = 8500.0;
    data.max_rpm = 9200.0;
    data.gear = 4;
    data.num_gears = 6;
    data.throttle = 0.95;
    data.brake = 0.0;
    data.clutch = 0.0;
    data.steering = -0.15;
    data.fuel_level = 40.0;
    data.fuel_capacity = 110.0;
    data.water_temp_celsius = 88.0;
    data.oil_temp_celsius = 105.0;
    data.oil_pressure_kpa = 350.0;
    data.laps_completed = 8;
    data.current_time = 135.2;
    data.best_lap_time = 132.0;
    data.last_lap_time = 133.5;
    data.split_time_ahead = 0.5;
    data.split_time_behind = 1.2;
    data.highest_flag = HighestFlag::Green as u32;
    data.pit_mode = PitMode::None as u32;
    data.tc_setting = 2;
    data.abs_setting = 1;
    data.drs_state = DrsState::Installed as u32;
    data.tyre_temp = [92.0, 94.0, 88.0, 90.0];
    data.air_pressure = [185.0, 187.0, 180.0, 182.0];
    data.tyre_slip = [0.05, 0.06, 0.03, 0.04];
    data.local_acceleration = [9.80665 * 1.2, 9.80665 * 1.0, 9.80665 * 0.4];
    data.car_name = car_name;
    data.track_location = track_name;

    let t = adapter.normalize(&to_bytes(&data))?;

    // Core telemetry
    assert!((t.speed_ms - 65.0).abs() < 0.01);
    assert!((t.rpm - 8500.0).abs() < 0.1);
    assert_eq!(t.gear, 4);
    assert_eq!(t.num_gears, 6);
    assert!((t.throttle - 0.95).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    assert!((t.steering_angle - (-0.15)).abs() < 0.001);

    // Fuel
    let expected_fuel_pct = 40.0 / 110.0;
    assert!((t.fuel_percent - expected_fuel_pct).abs() < 0.001);

    // Timing
    assert_eq!(t.lap, 8);
    assert!((t.current_lap_time_s - 135.2).abs() < 0.01);
    assert!((t.best_lap_time_s - 132.0).abs() < 0.01);
    assert!((t.last_lap_time_s - 133.5).abs() < 0.01);

    // Flags
    assert!(t.flags.green_flag);
    assert!(!t.flags.in_pits);
    assert!(t.flags.traction_control);
    assert!(t.flags.abs_active);
    assert!(!t.flags.drs_available);
    assert!(!t.flags.drs_active);

    // G-forces
    assert!((t.lateral_g - 1.2).abs() < 0.01);
    assert!((t.longitudinal_g - 0.4).abs() < 0.01);

    // Car/Track
    assert_eq!(t.car_id.as_deref(), Some("porsche_911_gt3_r"));
    assert_eq!(t.track_id.as_deref(), Some("bathurst"));

    // Tire data
    assert_eq!(t.tire_temps_c, [92, 94, 88, 90]);
    assert!(t.tire_pressures_psi[0] > 0.0);

    // Extended
    assert!(t.extended.contains_key("oil_temp_c"));
    assert!(t.extended.contains_key("oil_pressure_kpa"));
    assert!(t.extended.contains_key("fuel_level_l"));

    Ok(())
}

// ── Determinism across multiple normalizations ──────────────────────────────

#[test]
fn deterministic_across_calls() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 42.0;
    data.rpm = 6000.0;
    data.gear = 3;
    data.throttle = 0.5;
    data.fuel_level = 30.0;
    data.fuel_capacity = 80.0;
    let raw = to_bytes(&data);

    let t1 = adapter.normalize(&raw)?;
    let t2 = adapter.normalize(&raw)?;
    let t3 = adapter.normalize(&raw)?;

    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t2.speed_ms, t3.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.fuel_percent, t2.fuel_percent);
    assert_eq!(t1.flags.green_flag, t2.flags.green_flag);
    Ok(())
}

// ── Shared memory struct size ────────────────────────────────────────────────

#[test]
fn shared_memory_struct_has_consistent_size() -> TestResult {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    // Verify struct size is stable and non-zero.
    assert!(size > 0, "struct should be non-zero size");
    // Verify default can be serialized to bytes at this size.
    let data = default_mem();
    let raw = to_bytes(&data);
    assert_eq!(raw.len(), size);
    Ok(())
}

// ── Adapter Default trait ────────────────────────────────────────────────────

#[test]
fn adapter_default_matches_new() -> TestResult {
    let a = AMS2Adapter::new();
    let b = AMS2Adapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

// ── Weather scenario: wet conditions ─────────────────────────────────────────

#[test]
fn wet_conditions_cold_tires_high_slip() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = AMS2SharedMemory::default();
    data.speed = 40.0;
    data.rpm = 5000.0;
    data.gear = 3;
    data.throttle = 0.6;
    data.tyre_temp = [50.0, 52.0, 48.0, 51.0];
    data.tyre_slip = [0.4, 0.5, 0.3, 0.35];
    data.highest_flag = HighestFlag::Yellow as u32;
    let t = adapter.normalize(&to_bytes(&data))?;

    assert!(t.flags.yellow_flag);
    assert!(t.slip_ratio > 0.0, "should have non-zero slip on wet");
    assert_eq!(t.tire_temps_c, [50, 52, 48, 51]);
    Ok(())
}

// ── Pit entry/exit scenario ──────────────────────────────────────────────────

#[test]
fn pit_entry_driving_into_pits_then_in_pit() -> TestResult {
    let adapter = AMS2Adapter::new();

    // Phase 1: driving into pits
    let mut data1 = default_mem();
    data1.speed = 20.0;
    data1.pit_mode = PitMode::DrivingIntoPits as u32;
    let t1 = adapter.normalize(&to_bytes(&data1))?;
    assert!(t1.flags.in_pits);
    assert!(!t1.flags.pit_limiter);

    // Phase 2: in pit (stopped)
    let mut data2 = default_mem();
    data2.speed = 0.0;
    data2.pit_mode = PitMode::InPit as u32;
    let t2 = adapter.normalize(&to_bytes(&data2))?;
    assert!(t2.flags.in_pits);
    assert!(!t2.flags.pit_limiter);

    // Phase 3: in pitlane (driving out)
    let mut data3 = default_mem();
    data3.speed = 15.0;
    data3.pit_mode = PitMode::InPitlane as u32;
    let t3 = adapter.normalize(&to_bytes(&data3))?;
    assert!(t3.flags.in_pits);
    assert!(t3.flags.pit_limiter);

    Ok(())
}
