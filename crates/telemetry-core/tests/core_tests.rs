//! Integration tests for the openracing-telemetry crate.
//!
//! Covers: builder pattern, default values, extended map, TelemetryFlags,
//! conversion utilities, and re-exported types.

use std::time::Instant;

use openracing_telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── 1. Default values ─────────────────────────────────────────────────────

#[test]
fn normalized_telemetry_default_has_zero_numeric_fields() -> TestResult {
    let t = NormalizedTelemetry::default();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.num_gears, 0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.slip_angle_fl, 0.0);
    assert_eq!(t.slip_angle_fr, 0.0);
    assert_eq!(t.slip_angle_rl, 0.0);
    assert_eq!(t.slip_angle_rr, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.ffb_torque_nm, 0.0);
    assert_eq!(t.position, 0);
    assert_eq!(t.lap, 0);
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    assert_eq!(t.delta_ahead_s, 0.0);
    assert_eq!(t.delta_behind_s, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.engine_temp_c, 0.0);
    assert_eq!(t.sequence, 0);
    assert_eq!(t.tire_temps_c, [0; 4]);
    assert_eq!(t.tire_pressures_psi, [0.0; 4]);
    Ok(())
}

#[test]
fn normalized_telemetry_default_has_none_option_fields() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    Ok(())
}

#[test]
fn normalized_telemetry_default_has_empty_extended_map() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.extended.is_empty());
    Ok(())
}

#[test]
fn normalized_telemetry_new_equals_default() -> TestResult {
    let a = NormalizedTelemetry::new();
    let b = NormalizedTelemetry::default();
    assert_eq!(a.speed_ms, b.speed_ms);
    assert_eq!(a.rpm, b.rpm);
    assert_eq!(a.gear, b.gear);
    assert_eq!(a.sequence, b.sequence);
    Ok(())
}

// ── 2. Builder pattern – typed fields ─────────────────────────────────────

#[test]
fn builder_sets_motion_fields() -> TestResult {
    let ts = Instant::now();
    let t = NormalizedTelemetry::builder()
        .speed_ms(45.5)
        .steering_angle(-0.3)
        .throttle(0.9)
        .brake(0.2)
        .clutch(0.5)
        .timestamp(ts)
        .build();

    assert_eq!(t.speed_ms, 45.5);
    assert_eq!(t.steering_angle, -0.3);
    assert_eq!(t.throttle, 0.9);
    assert_eq!(t.brake, 0.2);
    assert_eq!(t.clutch, 0.5);
    Ok(())
}

#[test]
fn builder_sets_engine_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(7200.0)
        .max_rpm(8500.0)
        .gear(4)
        .num_gears(6)
        .build();

    assert_eq!(t.rpm, 7200.0);
    assert_eq!(t.max_rpm, 8500.0);
    assert_eq!(t.gear, 4);
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn builder_sets_g_force_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .lateral_g(1.2)
        .longitudinal_g(-0.8)
        .vertical_g(0.1)
        .build();

    assert_eq!(t.lateral_g, 1.2);
    assert_eq!(t.longitudinal_g, -0.8);
    assert_eq!(t.vertical_g, 0.1);
    Ok(())
}

#[test]
fn builder_sets_slip_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .slip_ratio(0.35)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .build();

    assert_eq!(t.slip_ratio, 0.35);
    assert_eq!(t.slip_angle_fl, 0.01);
    assert_eq!(t.slip_angle_fr, 0.02);
    assert_eq!(t.slip_angle_rl, 0.03);
    assert_eq!(t.slip_angle_rr, 0.04);
    Ok(())
}

#[test]
fn builder_sets_tire_arrays() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .tire_temps_c([85, 90, 80, 88])
        .tire_pressures_psi([26.0, 26.5, 25.5, 26.0])
        .build();

    assert_eq!(t.tire_temps_c, [85, 90, 80, 88]);
    assert_eq!(t.tire_pressures_psi, [26.0, 26.5, 25.5, 26.0]);
    Ok(())
}

#[test]
fn builder_sets_ffb_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .ffb_scalar(0.75)
        .ffb_torque_nm(12.5)
        .build();

    assert_eq!(t.ffb_scalar, 0.75);
    assert_eq!(t.ffb_torque_nm, 12.5);
    Ok(())
}

#[test]
fn builder_sets_context_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .car_id("porsche_911_gt3")
        .track_id("spa_francorchamps")
        .session_id("race_001")
        .position(3)
        .lap(12)
        .build();

    assert_eq!(t.car_id.as_deref(), Some("porsche_911_gt3"));
    assert_eq!(t.track_id.as_deref(), Some("spa_francorchamps"));
    assert_eq!(t.session_id.as_deref(), Some("race_001"));
    assert_eq!(t.position, 3);
    assert_eq!(t.lap, 12);
    Ok(())
}

#[test]
fn builder_sets_lap_time_and_delta_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .current_lap_time_s(42.5)
        .best_lap_time_s(41.2)
        .last_lap_time_s(42.0)
        .delta_ahead_s(-1.3)
        .delta_behind_s(0.8)
        .build();

    assert_eq!(t.current_lap_time_s, 42.5);
    assert_eq!(t.best_lap_time_s, 41.2);
    assert_eq!(t.last_lap_time_s, 42.0);
    assert_eq!(t.delta_ahead_s, -1.3);
    assert_eq!(t.delta_behind_s, 0.8);
    Ok(())
}

#[test]
fn builder_sets_fuel_and_engine_temp() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .fuel_percent(0.65)
        .engine_temp_c(92.3)
        .build();

    assert_eq!(t.fuel_percent, 0.65);
    assert_eq!(t.engine_temp_c, 92.3);
    Ok(())
}

#[test]
fn builder_sets_sequence() -> TestResult {
    let t = NormalizedTelemetry::builder().sequence(42).build();
    assert_eq!(t.sequence, 42);
    Ok(())
}

#[test]
fn builder_clamps_throttle_and_brake() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .throttle(1.5)
        .brake(-0.3)
        .clutch(2.0)
        .build();

    assert_eq!(t.throttle, 1.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 1.0);
    Ok(())
}

#[test]
fn builder_clamps_ffb_scalar_range() -> TestResult {
    let t = NormalizedTelemetry::builder().ffb_scalar(-2.0).build();
    assert_eq!(t.ffb_scalar, -1.0);

    let t2 = NormalizedTelemetry::builder().ffb_scalar(5.0).build();
    assert_eq!(t2.ffb_scalar, 1.0);
    Ok(())
}

#[test]
fn builder_clamps_slip_ratio_range() -> TestResult {
    let under = NormalizedTelemetry::builder().slip_ratio(-0.5).build();
    assert_eq!(under.slip_ratio, 0.0);

    let over = NormalizedTelemetry::builder().slip_ratio(1.5).build();
    assert_eq!(over.slip_ratio, 1.0);
    Ok(())
}

#[test]
fn builder_clamps_fuel_percent_range() -> TestResult {
    let under = NormalizedTelemetry::builder().fuel_percent(-0.1).build();
    assert_eq!(under.fuel_percent, 0.0);

    let over = NormalizedTelemetry::builder().fuel_percent(1.5).build();
    assert_eq!(over.fuel_percent, 1.0);
    Ok(())
}

#[test]
fn builder_ignores_nan_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::NAN)
        .rpm(f32::NAN)
        .throttle(f32::NAN)
        .lateral_g(f32::NAN)
        .ffb_scalar(f32::NAN)
        .build();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    Ok(())
}

#[test]
fn builder_ignores_infinite_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::INFINITY)
        .rpm(f32::NEG_INFINITY)
        .build();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    Ok(())
}

#[test]
fn builder_ignores_negative_speed_and_rpm() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(-10.0)
        .rpm(-500.0)
        .max_rpm(-100.0)
        .build();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    Ok(())
}

#[test]
fn builder_empty_string_ids_produce_none() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .car_id("")
        .track_id("")
        .session_id("")
        .build();

    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    Ok(())
}

// ── 3. Extended map ───────────────────────────────────────────────────────

#[test]
fn extended_map_insert_and_retrieve_float() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("water_temp", TelemetryValue::Float(85.5))
        .build();

    let val = t.get_extended("water_temp");
    assert_eq!(val, Some(&TelemetryValue::Float(85.5)));
    Ok(())
}

#[test]
fn extended_map_insert_and_retrieve_integer() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("lap_sector", TelemetryValue::Integer(2))
        .build();

    assert_eq!(
        t.get_extended("lap_sector"),
        Some(&TelemetryValue::Integer(2))
    );
    Ok(())
}

#[test]
fn extended_map_insert_and_retrieve_boolean() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("headlights_on", TelemetryValue::Boolean(true))
        .build();

    assert_eq!(
        t.get_extended("headlights_on"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

#[test]
fn extended_map_insert_and_retrieve_string() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended(
            "driver_name",
            TelemetryValue::String("Max Verstappen".to_string()),
        )
        .build();

    assert_eq!(
        t.get_extended("driver_name"),
        Some(&TelemetryValue::String("Max Verstappen".to_string()))
    );
    Ok(())
}

#[test]
fn extended_map_missing_key_returns_none() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.get_extended("nonexistent").is_none());
    Ok(())
}

#[test]
fn extended_map_multiple_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("oil_temp", TelemetryValue::Float(110.0))
        .extended("rain_intensity", TelemetryValue::Integer(3))
        .extended("wipers_on", TelemetryValue::Boolean(true))
        .build();

    assert_eq!(t.extended.len(), 3);
    assert_eq!(
        t.get_extended("oil_temp"),
        Some(&TelemetryValue::Float(110.0))
    );
    assert_eq!(
        t.get_extended("rain_intensity"),
        Some(&TelemetryValue::Integer(3))
    );
    assert_eq!(
        t.get_extended("wipers_on"),
        Some(&TelemetryValue::Boolean(true))
    );
    Ok(())
}

#[test]
fn with_extended_on_instance_adds_value() -> TestResult {
    let t = NormalizedTelemetry::default().with_extended("turbo_psi", TelemetryValue::Float(14.7));

    assert_eq!(
        t.get_extended("turbo_psi"),
        Some(&TelemetryValue::Float(14.7))
    );
    Ok(())
}

#[test]
fn extended_map_overwrite_replaces_value() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("sector", TelemetryValue::Integer(1))
        .extended("sector", TelemetryValue::Integer(2))
        .build();

    assert_eq!(t.get_extended("sector"), Some(&TelemetryValue::Integer(2)));
    assert_eq!(t.extended.len(), 1);
    Ok(())
}

// ── 4. TelemetryFlags ────────────────────────────────────────────────────

#[test]
fn telemetry_flags_default_green_flag_true() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag);
    Ok(())
}

#[test]
fn telemetry_flags_default_all_hazard_flags_false() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
    assert!(!flags.checkered_flag);
    Ok(())
}

#[test]
fn telemetry_flags_default_pit_flags_false() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(!flags.pit_limiter);
    assert!(!flags.in_pits);
    Ok(())
}

#[test]
fn telemetry_flags_default_electronics_false() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(!flags.drs_available);
    assert!(!flags.drs_active);
    assert!(!flags.ers_available);
    assert!(!flags.ers_active);
    assert!(!flags.launch_control);
    assert!(!flags.traction_control);
    assert!(!flags.abs_active);
    assert!(!flags.engine_limiter);
    Ok(())
}

#[test]
fn telemetry_flags_default_session_flags_false() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(!flags.safety_car);
    assert!(!flags.formation_lap);
    assert!(!flags.session_paused);
    Ok(())
}

#[test]
fn telemetry_flags_custom_combination() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        safety_car: true,
        drs_available: true,
        abs_active: true,
        ..Default::default()
    };

    assert!(flags.yellow_flag);
    assert!(flags.safety_car);
    assert!(flags.drs_available);
    assert!(flags.abs_active);
    // defaults still hold
    assert!(flags.green_flag);
    assert!(!flags.red_flag);
    assert!(!flags.in_pits);
    Ok(())
}

#[test]
fn telemetry_flags_equality() -> TestResult {
    let a = TelemetryFlags::default();
    let b = TelemetryFlags::default();
    assert_eq!(a, b);

    let c = TelemetryFlags {
        yellow_flag: true,
        ..Default::default()
    };
    assert_ne!(a, c);
    Ok(())
}

#[test]
fn builder_sets_flags() -> TestResult {
    let flags = TelemetryFlags {
        blue_flag: true,
        in_pits: true,
        pit_limiter: true,
        ..Default::default()
    };
    let t = NormalizedTelemetry::builder().flags(flags.clone()).build();

    assert!(t.flags.blue_flag);
    assert!(t.flags.in_pits);
    assert!(t.flags.pit_limiter);
    assert_eq!(t.flags, flags);
    Ok(())
}

#[test]
fn has_active_flags_detects_hazard_flags() -> TestResult {
    let none_active = NormalizedTelemetry::default();
    assert!(!none_active.has_active_flags());

    let yellow = NormalizedTelemetry::builder()
        .flags(TelemetryFlags {
            yellow_flag: true,
            ..Default::default()
        })
        .build();
    assert!(yellow.has_active_flags());

    let checkered = NormalizedTelemetry::builder()
        .flags(TelemetryFlags {
            checkered_flag: true,
            ..Default::default()
        })
        .build();
    assert!(checkered.has_active_flags());
    Ok(())
}

// ── 5. Utility / conversion functions ─────────────────────────────────────

#[test]
fn speed_conversion_kmh() -> TestResult {
    let t = NormalizedTelemetry::builder().speed_ms(27.78).build();
    assert!((t.speed_kmh() - 100.008).abs() < 0.1);
    Ok(())
}

#[test]
fn speed_conversion_mph() -> TestResult {
    let t = NormalizedTelemetry::builder().speed_ms(27.78).build();
    assert!((t.speed_mph() - 62.15).abs() < 0.1);
    Ok(())
}

#[test]
fn slip_angle_averages() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.03)
        .slip_angle_rl(0.05)
        .slip_angle_rr(0.07)
        .build();

    assert!((t.average_slip_angle() - 0.04).abs() < 0.001);
    assert!((t.front_slip_angle() - 0.02).abs() < 0.001);
    assert!((t.rear_slip_angle() - 0.06).abs() < 0.001);
    Ok(())
}

#[test]
fn is_stationary_threshold() -> TestResult {
    let stopped = NormalizedTelemetry::builder().speed_ms(0.0).build();
    assert!(stopped.is_stationary());

    let crawling = NormalizedTelemetry::builder().speed_ms(0.49).build();
    assert!(crawling.is_stationary());

    let moving = NormalizedTelemetry::builder().speed_ms(0.5).build();
    assert!(!moving.is_stationary());
    Ok(())
}

#[test]
fn total_g_pythagorean() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .lateral_g(3.0)
        .longitudinal_g(4.0)
        .build();
    assert!((t.total_g() - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn has_ffb_data_checks_both_fields() -> TestResult {
    let no_ffb = NormalizedTelemetry::default();
    assert!(!no_ffb.has_ffb_data());

    let scalar_only = NormalizedTelemetry::builder().ffb_scalar(0.5).build();
    assert!(scalar_only.has_ffb_data());

    let torque_only = NormalizedTelemetry::builder().ffb_torque_nm(10.0).build();
    assert!(torque_only.has_ffb_data());
    Ok(())
}

#[test]
fn has_rpm_data_and_display_data() -> TestResult {
    let no_rpm = NormalizedTelemetry::default();
    assert!(!no_rpm.has_rpm_data());
    assert!(!no_rpm.has_rpm_display_data());

    let rpm_only = NormalizedTelemetry::builder().rpm(3000.0).build();
    assert!(rpm_only.has_rpm_data());
    assert!(!rpm_only.has_rpm_display_data());

    let both = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .max_rpm(8000.0)
        .build();
    assert!(both.has_rpm_data());
    assert!(both.has_rpm_display_data());
    Ok(())
}

#[test]
fn rpm_fraction_calculation() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .max_rpm(8000.0)
        .build();
    assert!((t.rpm_fraction() - 0.75).abs() < 0.001);

    let no_max = NormalizedTelemetry::builder().rpm(6000.0).build();
    assert_eq!(no_max.rpm_fraction(), 0.0);

    let over = NormalizedTelemetry::builder()
        .rpm(9000.0)
        .max_rpm(8000.0)
        .build();
    assert_eq!(over.rpm_fraction(), 1.0);
    Ok(())
}

#[test]
fn with_timestamp_mut_and_with_sequence() -> TestResult {
    let ts = Instant::now();
    let t = NormalizedTelemetry::default()
        .with_timestamp_mut(ts)
        .with_sequence(99);

    assert_eq!(t.sequence, 99);
    Ok(())
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn validated_clamps_nan_and_out_of_range() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.speed_ms = f32::NAN;
    t.throttle = 1.5;
    t.brake = -0.1;
    t.ffb_scalar = 2.0;
    t.fuel_percent = f32::INFINITY;
    t.slip_ratio = -0.5;

    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.throttle, 1.0);
    assert_eq!(v.brake, 0.0);
    assert_eq!(v.ffb_scalar, 1.0);
    assert_eq!(v.fuel_percent, 0.0);
    assert_eq!(v.slip_ratio, 0.0);
    Ok(())
}

// ── 6. TelemetryFrame ────────────────────────────────────────────────────

#[test]
fn telemetry_frame_new_stores_fields() -> TestResult {
    let data = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::new(data, 123_456, 7, 512);

    assert_eq!(frame.data.rpm, 5000.0);
    assert_eq!(frame.timestamp_ns, 123_456);
    assert_eq!(frame.sequence, 7);
    assert_eq!(frame.raw_size, 512);
    Ok(())
}

#[test]
fn telemetry_frame_from_telemetry_auto_timestamps() -> TestResult {
    let data = NormalizedTelemetry::builder().speed_ms(30.0).build();
    let frame = TelemetryFrame::from_telemetry(data, 1, 256);

    assert!(frame.timestamp_ns > 0);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, 256);
    assert_eq!(frame.data.speed_ms, 30.0);
    Ok(())
}

// ── 7. TelemetryValue variants ───────────────────────────────────────────

#[test]
fn telemetry_value_equality() -> TestResult {
    assert_eq!(TelemetryValue::Float(1.0), TelemetryValue::Float(1.0));
    assert_eq!(TelemetryValue::Integer(42), TelemetryValue::Integer(42));
    assert_eq!(TelemetryValue::Boolean(true), TelemetryValue::Boolean(true));
    assert_eq!(
        TelemetryValue::String("a".into()),
        TelemetryValue::String("a".into())
    );
    assert_ne!(TelemetryValue::Float(1.0), TelemetryValue::Integer(1));
    Ok(())
}

#[test]
fn telemetry_value_clone() -> TestResult {
    let original = TelemetryValue::String("test".to_string());
    let cloned = original.clone();
    assert_eq!(original, cloned);
    Ok(())
}

// ── 8. Serde round-trip for NormalizedTelemetry ──────────────────────────

#[test]
fn normalized_telemetry_serde_roundtrip() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(33.0)
        .rpm(5500.0)
        .gear(3)
        .throttle(0.7)
        .flags(TelemetryFlags {
            yellow_flag: true,
            ..Default::default()
        })
        .car_id("ferrari_488")
        .extended("boost", TelemetryValue::Float(1.2))
        .build();

    let json = serde_json::to_string(&t)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(deserialized.speed_ms, 33.0);
    assert_eq!(deserialized.rpm, 5500.0);
    assert_eq!(deserialized.gear, 3);
    assert_eq!(deserialized.throttle, 0.7);
    assert!(deserialized.flags.yellow_flag);
    assert_eq!(deserialized.car_id.as_deref(), Some("ferrari_488"));
    assert_eq!(
        deserialized.get_extended("boost"),
        Some(&TelemetryValue::Float(1.2))
    );
    Ok(())
}

#[test]
fn telemetry_flags_serde_roundtrip() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        drs_active: true,
        abs_active: true,
        safety_car: true,
        ..Default::default()
    };

    let json = serde_json::to_string(&flags)?;
    let deserialized: TelemetryFlags = serde_json::from_str(&json)?;
    assert_eq!(flags, deserialized);
    Ok(())
}

#[test]
fn telemetry_flags_deserialize_missing_fields_use_defaults() -> TestResult {
    let json = r#"{}"#;
    let flags: TelemetryFlags = serde_json::from_str(json)?;
    assert!(flags.green_flag);
    assert!(!flags.yellow_flag);
    Ok(())
}

#[test]
fn telemetry_value_serde_roundtrip() -> TestResult {
    let values = vec![
        TelemetryValue::Float(std::f32::consts::PI),
        TelemetryValue::Integer(-7),
        TelemetryValue::Boolean(false),
        TelemetryValue::String("hello".into()),
    ];

    for v in &values {
        let json = serde_json::to_string(v)?;
        let back: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(&back, v);
    }
    Ok(())
}

// ── 9. Extended map via BTreeMap ordering ────────────────────────────────

#[test]
fn extended_map_maintains_sorted_order() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("z_key", TelemetryValue::Integer(1))
        .extended("a_key", TelemetryValue::Integer(2))
        .extended("m_key", TelemetryValue::Integer(3))
        .build();

    let keys: Vec<&String> = t.extended.keys().collect();
    assert_eq!(keys, vec!["a_key", "m_key", "z_key"]);
    Ok(())
}

// ── 10. NormalizedTelemetry with_timestamp constructor ───────────────────

#[test]
fn with_timestamp_creates_zeroed_with_given_time() -> TestResult {
    let ts = Instant::now();
    let t = NormalizedTelemetry::with_timestamp(ts);

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert!(t.extended.is_empty());
    Ok(())
}

// ── 11. TelemetryFieldCoverage & FlagCoverage ────────────────────────────

#[test]
fn telemetry_field_coverage_serde_roundtrip() -> TestResult {
    use openracing_telemetry::contracts::{FlagCoverage, TelemetryFieldCoverage};

    let coverage = TelemetryFieldCoverage {
        game_id: "acc".to_string(),
        game_version: "1.9".to_string(),
        ffb_scalar: true,
        rpm: true,
        speed: true,
        slip_ratio: false,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: true,
            red_flag: true,
            blue_flag: true,
            checkered_flag: true,
            green_flag: true,
            pit_limiter: true,
            in_pits: true,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: true,
            abs_active: true,
        },
        car_id: true,
        track_id: true,
        extended_fields: vec!["water_temp".to_string(), "oil_temp".to_string()],
    };

    let json = serde_json::to_string(&coverage)?;
    let back: TelemetryFieldCoverage = serde_json::from_str(&json)?;
    assert_eq!(back.game_id, "acc");
    assert!(back.flags.yellow_flag);
    assert!(!back.flags.drs_available);
    assert_eq!(back.extended_fields.len(), 2);
    Ok(())
}
