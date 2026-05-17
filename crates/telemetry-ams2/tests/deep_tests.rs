//! Deep individual tests for the AMS2 telemetry adapter.
//!
//! Covers packet parsing, normalization edge cases, flag handling,
//! tire data, fuel calculations, and game-specific AMS2 features.

use openracing_telemetry_adapters::ams2::{AMS2SharedMemory, DrsState, HighestFlag, PitMode};
use racing_wheel_telemetry_ams2::{AMS2Adapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Serialize an `AMS2SharedMemory` to raw bytes for the normalize() API.
fn to_bytes(data: &AMS2SharedMemory) -> Vec<u8> {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let ptr = data as *const AMS2SharedMemory as *const u8;
    // SAFETY: AMS2SharedMemory is repr(C) and fully initialized.
    unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec()
}

fn write_str(buf: &mut [u8; 64], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(63);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
}

/// Create a default AMS2SharedMemory (avoids private-field issues).
fn default_mem() -> AMS2SharedMemory {
    AMS2SharedMemory::default()
}

// ── Packet parsing tests ─────────────────────────────────────────────────────

#[test]
fn deep_parse_default_shared_memory() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let raw = to_bytes(&data);
    let t = adapter.normalize(&raw)?;
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    Ok(())
}

#[test]
fn deep_parse_undersized_packet_rejected() -> TestResult {
    let adapter = AMS2Adapter::new();
    let short = vec![0u8; std::mem::size_of::<AMS2SharedMemory>() - 1];
    assert!(adapter.normalize(&short).is_err());
    Ok(())
}

#[test]
fn deep_parse_zero_length_rejected() -> TestResult {
    let adapter = AMS2Adapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_parse_oversized_packet_accepted() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let mut raw = to_bytes(&data);
    raw.extend_from_slice(&[0xFFu8; 256]);
    let t = adapter.normalize(&raw)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

// ── Normalization tests ──────────────────────────────────────────────────────

#[test]
fn deep_speed_preserved_exactly() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 72.5;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!((t.speed_ms - 72.5).abs() < 0.001, "speed_ms={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_gear_reverse_neutral_forward() -> TestResult {
    let adapter = AMS2Adapter::new();
    for (gear_in, gear_expected) in [(-1i8, -1i8), (0, 0), (1, 1), (6, 6)] {
        let mut data = default_mem();
        data.gear = gear_in;
        let t = adapter.normalize(&to_bytes(&data))?;
        assert_eq!(t.gear, gear_expected, "gear input={gear_in}");
    }
    Ok(())
}

#[test]
fn deep_throttle_brake_clutch_passthrough() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.throttle = 0.73;
    data.brake = 0.42;
    data.clutch = 0.15;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!((t.throttle - 0.73).abs() < 0.001);
    assert!((t.brake - 0.42).abs() < 0.001);
    assert!((t.clutch - 0.15).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_steering_clamped_to_ffb_range() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data_high = default_mem();
    data_high.steering = 2.5;
    let t = adapter.normalize(&to_bytes(&data_high))?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.001, "high clamp");

    let mut data_low = default_mem();
    data_low.steering = -3.0;
    let t = adapter.normalize(&to_bytes(&data_low))?;
    assert!((t.ffb_scalar - (-1.0)).abs() < 0.001, "low clamp");
    Ok(())
}

// ── Fuel calculation tests ───────────────────────────────────────────────────

#[test]
fn deep_fuel_percent_normal() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.fuel_level = 25.0;
    data.fuel_capacity = 100.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!((t.fuel_percent - 0.25).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_fuel_percent_zero_capacity() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.fuel_level = 50.0;
    data.fuel_capacity = 0.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.fuel_percent, 0.0, "zero capacity → 0% fuel");
    Ok(())
}

#[test]
fn deep_fuel_percent_overfull_clamped() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.fuel_level = 120.0;
    data.fuel_capacity = 100.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(
        (t.fuel_percent - 1.0).abs() < 0.001,
        "overfull clamped to 1.0"
    );
    Ok(())
}

// ── Slip ratio calculation tests ─────────────────────────────────────────────

#[test]
fn deep_slip_ratio_at_speed() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 30.0;
    data.tyre_slip = [0.2, 0.3, 0.1, 0.2];
    let t = adapter.normalize(&to_bytes(&data))?;
    // Average: (0.2 + 0.3 + 0.1 + 0.2) / 4 = 0.2
    assert!((t.slip_ratio - 0.2).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_slip_ratio_zero_at_low_speed() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 0.5; // below 1.0 threshold
    data.tyre_slip = [1.0, 1.0, 1.0, 1.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.slip_ratio, 0.0, "slip=0 when speed<=1");
    Ok(())
}

#[test]
fn deep_slip_ratio_negative_tyres_use_abs() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.speed = 50.0;
    data.tyre_slip = [-0.3, -0.3, -0.3, -0.3];
    let t = adapter.normalize(&to_bytes(&data))?;
    // abs(-0.3) average = 0.3
    assert!((t.slip_ratio - 0.3).abs() < 0.001);
    Ok(())
}

// ── G-force calculation tests ────────────────────────────────────────────────

#[test]
fn deep_g_forces_conversion() -> TestResult {
    let adapter = AMS2Adapter::new();
    let g = 9.80665_f32;
    let mut data = default_mem();
    // X=right (lateral), Y=up (vertical), Z=forward (longitudinal)
    data.local_acceleration = [2.0 * g, 1.5 * g, -0.5 * g];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!((t.lateral_g - 2.0).abs() < 0.01, "lateral_g");
    assert!((t.vertical_g - 1.5).abs() < 0.01, "vertical_g");
    assert!((t.longitudinal_g - (-0.5)).abs() < 0.01, "longitudinal_g");
    Ok(())
}

// ── Flag tests ───────────────────────────────────────────────────────────────

#[test]
fn deep_all_flag_variants() -> TestResult {
    let adapter = AMS2Adapter::new();
    let flag_checks: Vec<(u32, &str)> = vec![
        (HighestFlag::Green as u32, "green"),
        (HighestFlag::Yellow as u32, "yellow"),
        (HighestFlag::Red as u32, "red"),
        (HighestFlag::Blue as u32, "blue"),
        (HighestFlag::Chequered as u32, "checkered"),
    ];

    for (flag_value, label) in &flag_checks {
        let mut data = default_mem();
        data.highest_flag = *flag_value;
        let t = adapter.normalize(&to_bytes(&data))?;
        match *label {
            "green" => assert!(t.flags.green_flag, "green_flag should be set"),
            "yellow" => assert!(t.flags.yellow_flag, "yellow_flag should be set"),
            "red" => assert!(t.flags.red_flag, "red_flag should be set"),
            "blue" => assert!(t.flags.blue_flag, "blue_flag should be set"),
            "checkered" => assert!(t.flags.checkered_flag, "checkered_flag should be set"),
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn deep_pit_mode_driving_into_pits() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.pit_mode = PitMode::DrivingIntoPits as u32;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(t.flags.in_pits, "DrivingIntoPits should set in_pits");
    Ok(())
}

#[test]
fn deep_drs_installed_neither_available_nor_active() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.drs_state = DrsState::Installed as u32;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(!t.flags.drs_available, "Installed ≠ available");
    assert!(!t.flags.drs_active, "Installed ≠ active");
    Ok(())
}

// ── Tire data tests ──────────────────────────────────────────────────────────

#[test]
fn deep_tire_temps_conversion() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.tyre_temp = [85.0, 90.0, 95.0, 100.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.tire_temps_c, [85, 90, 95, 100]);
    Ok(())
}

#[test]
fn deep_tire_pressures_kpa_to_psi() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    // 200 kPa ≈ 29.007 PSI
    data.air_pressure = [200.0, 200.0, 200.0, 200.0];
    let t = adapter.normalize(&to_bytes(&data))?;
    for &psi in &t.tire_pressures_psi {
        assert!((psi - 29.007).abs() < 0.1, "pressure_psi={psi}");
    }
    Ok(())
}

// ── String extraction tests ──────────────────────────────────────────────────

#[test]
fn deep_car_and_track_names() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    write_str(&mut data.car_name, "mcl_720s_gt3");
    write_str(&mut data.track_location, "spa_francorchamps");
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.car_id.as_deref(), Some("mcl_720s_gt3"));
    assert_eq!(t.track_id.as_deref(), Some("spa_francorchamps"));
    Ok(())
}

#[test]
fn deep_empty_car_name_produces_car_id() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let t = adapter.normalize(&to_bytes(&data))?;
    // All-zero car_name → empty string → builder may set None or Some("")
    let id = t.car_id.as_deref().unwrap_or("");
    assert!(id.is_empty(), "expected empty car_id, got '{id}'");
    Ok(())
}

// ── Timing / lap data tests ──────────────────────────────────────────────────

#[test]
fn deep_timing_fields_populated() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.laps_completed = 10;
    data.current_time = 75.3;
    data.best_lap_time = 68.2;
    data.last_lap_time = 70.1;
    data.split_time_ahead = 1.5;
    data.split_time_behind = 2.3;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.lap, 10);
    assert!((t.current_lap_time_s - 75.3).abs() < 0.01);
    assert!((t.best_lap_time_s - 68.2).abs() < 0.01);
    assert!((t.last_lap_time_s - 70.1).abs() < 0.01);
    assert!((t.delta_ahead_s - 1.5).abs() < 0.01);
    assert!((t.delta_behind_s - 2.3).abs() < 0.01);
    Ok(())
}

#[test]
fn deep_zero_timing_fields_remain_zero() -> TestResult {
    let adapter = AMS2Adapter::new();
    let data = default_mem();
    let t = adapter.normalize(&to_bytes(&data))?;
    assert_eq!(t.lap, 0);
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    Ok(())
}

// ── Electronics tests ────────────────────────────────────────────────────────

#[test]
fn deep_tc_abs_flags() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.tc_setting = 5;
    data.abs_setting = 3;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(
        t.flags.traction_control,
        "TC should be active when setting>0"
    );
    assert!(t.flags.abs_active, "ABS should be active when setting>0");

    let data_off = default_mem();
    let t2 = adapter.normalize(&to_bytes(&data_off))?;
    assert!(!t2.flags.traction_control);
    assert!(!t2.flags.abs_active);
    Ok(())
}

// ── Engine temp and extended data tests ──────────────────────────────────────

#[test]
fn deep_water_temp_to_engine_temp() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.water_temp_celsius = 92.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!((t.engine_temp_c - 92.0).abs() < 0.1);
    Ok(())
}

#[test]
fn deep_extended_oil_pressure() -> TestResult {
    let adapter = AMS2Adapter::new();
    let mut data = default_mem();
    data.oil_pressure_kpa = 350.0;
    data.oil_temp_celsius = 105.0;
    data.water_pressure_kpa = 120.0;
    let t = adapter.normalize(&to_bytes(&data))?;
    assert!(t.extended.contains_key("oil_pressure_kpa"));
    assert!(t.extended.contains_key("oil_temp_c"));
    assert!(t.extended.contains_key("water_pressure_kpa"));
    Ok(())
}
