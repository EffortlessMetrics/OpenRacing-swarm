#![allow(clippy::redundant_closure)]

use openracing_handbrake::{
    HandbrakeCalibration, HandbrakeCapabilities, HandbrakeError, HandbrakeInput, HandbrakeType,
    MAX_ANALOG_VALUE,
};

// ── Position Parsing ────────────────────────────────────────────────────────

#[test]
fn parse_gamepad_little_endian_byte_order() -> Result<(), Box<dyn std::error::Error>> {
    // bytes[2]=0x34, bytes[3]=0x12 → raw_value=0x1234
    let data = [0x00, 0x00, 0x34, 0x12];
    let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.raw_value, 0x1234);
    Ok(())
}

#[test]
fn parse_gamepad_ignores_leading_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xAA, 0xBB, 0x00, 0x01];
    let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.raw_value, 0x0100);
    Ok(())
}

#[test]
fn parse_gamepad_accepts_extra_trailing_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x50, 0x00, 0xFF, 0xFF, 0xFF];
    let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.raw_value, 0x0050);
    Ok(())
}

#[test]
fn parse_gamepad_short_data_returns_error() {
    for len in 0..4 {
        let data = vec![0u8; len];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err(), "data of length {len} should fail");
    }
}

#[test]
fn parse_gamepad_engagement_boundary() -> Result<(), Box<dyn std::error::Error>> {
    // raw_value=100 → not engaged (threshold is >100)
    let data = [0x00, 0x00, 100, 0x00];
    let not_engaged = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(!not_engaged.is_engaged);

    // raw_value=101 → engaged
    let data = [0x00, 0x00, 101, 0x00];
    let engaged = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(engaged.is_engaged);
    Ok(())
}

#[test]
fn parse_gamepad_default_calibration() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0xFF, 0x00];
    let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.calibration_min, 0);
    assert_eq!(input.calibration_max, MAX_ANALOG_VALUE);
    Ok(())
}

// ── Calibration / Range Mapping ─────────────────────────────────────────────

#[test]
fn normalized_full_range() {
    let input = HandbrakeInput {
        raw_value: MAX_ANALOG_VALUE,
        is_engaged: true,
        calibration_min: 0,
        calibration_max: MAX_ANALOG_VALUE,
    };
    assert!((input.normalized() - 1.0).abs() < 0.001);
}

#[test]
fn normalized_midpoint() {
    let input = HandbrakeInput {
        raw_value: MAX_ANALOG_VALUE / 2,
        is_engaged: false,
        calibration_min: 0,
        calibration_max: MAX_ANALOG_VALUE,
    };
    assert!((input.normalized() - 0.5).abs() < 0.001);
}

#[test]
fn normalized_custom_calibration_range() {
    let input = HandbrakeInput {
        raw_value: 3000,
        is_engaged: true,
        calibration_min: 1000,
        calibration_max: 5000,
    };
    // (3000-1000)/(5000-1000) = 0.5
    assert!((input.normalized() - 0.5).abs() < 0.001);
}

#[test]
fn normalized_clamps_above_max() {
    let input = HandbrakeInput {
        raw_value: 10000,
        is_engaged: true,
        calibration_min: 1000,
        calibration_max: 5000,
    };
    assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn normalized_at_calibration_min_is_zero() {
    let input = HandbrakeInput {
        raw_value: 1000,
        is_engaged: false,
        calibration_min: 1000,
        calibration_max: 5000,
    };
    assert!(input.normalized().abs() < f32::EPSILON);
}

#[test]
fn normalized_zero_range_returns_zero() {
    let input = HandbrakeInput {
        raw_value: 5000,
        is_engaged: false,
        calibration_min: 5000,
        calibration_max: 5000,
    };
    assert!(input.normalized().abs() < f32::EPSILON);
}

#[test]
fn with_calibration_builder_sets_range() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0xE8, 0x03]; // raw_value=1000
    let input = HandbrakeInput::parse_gamepad(&data)
        .map_err(|e| e.to_string())?
        .with_calibration(500, 1500);
    assert_eq!(input.calibration_min, 500);
    assert_eq!(input.calibration_max, 1500);
    // (1000-500)/(1500-500) = 0.5
    assert!((input.normalized() - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn calibrate_mutates_range() {
    let mut input = HandbrakeInput::default();
    input.calibrate(200, 8000);
    assert_eq!(input.calibration_min, 200);
    assert_eq!(input.calibration_max, 8000);
}

#[test]
fn calibration_tracks_extremes_over_multiple_samples() {
    let mut cal = HandbrakeCalibration::new();
    cal.sample(500);
    cal.sample(100);
    cal.sample(900);
    cal.sample(300);
    assert_eq!(cal.min, 100);
    assert_eq!(cal.max, 900);
}

#[test]
fn calibration_apply_sets_input_range() {
    let mut cal = HandbrakeCalibration::new();
    cal.sample(200);
    cal.sample(8000);
    let mut input = HandbrakeInput::default();
    cal.apply(&mut input);
    assert_eq!(input.calibration_min, 200);
    assert_eq!(input.calibration_max, 8000);
}

#[test]
fn calibration_default_matches_new() {
    let a = HandbrakeCalibration::new();
    let b = HandbrakeCalibration::default();
    assert_eq!(a.min, b.min);
    assert_eq!(a.max, b.max);
    assert_eq!(a.center, b.center);
}

#[test]
fn handbrake_input_default_values() {
    let input = HandbrakeInput::default();
    assert_eq!(input.raw_value, 0);
    assert!(!input.is_engaged);
    assert_eq!(input.calibration_min, 0);
    assert_eq!(input.calibration_max, MAX_ANALOG_VALUE);
}

// ── Device Identification ───────────────────────────────────────────────────

#[test]
fn handbrake_type_default_is_analog() {
    assert_eq!(HandbrakeType::default(), HandbrakeType::Analog);
}

#[test]
fn capabilities_analog_properties() {
    let caps = HandbrakeCapabilities::analog();
    assert_eq!(caps.handbrake_type, HandbrakeType::Analog);
    assert_eq!(caps.max_load_kg, None);
    assert!(!caps.has_hall_effect_sensor);
    assert!(caps.supports_calibration);
}

#[test]
fn capabilities_load_cell_has_max_load() {
    let caps = HandbrakeCapabilities::load_cell(80.0);
    assert_eq!(caps.handbrake_type, HandbrakeType::LoadCell);
    assert_eq!(caps.max_load_kg, Some(80.0));
    assert!(!caps.has_hall_effect_sensor);
}

#[test]
fn capabilities_hall_effect_has_sensor_flag() {
    let caps = HandbrakeCapabilities::hall_effect();
    assert_eq!(caps.handbrake_type, HandbrakeType::HallEffect);
    assert!(caps.has_hall_effect_sensor);
    assert!(caps.supports_calibration);
}

#[test]
fn capabilities_default_matches_analog() {
    let default = HandbrakeCapabilities::default();
    let analog = HandbrakeCapabilities::analog();
    assert_eq!(default, analog);
}

#[test]
fn all_handbrake_types_are_distinct() {
    let types = [
        HandbrakeType::Analog,
        HandbrakeType::Digital,
        HandbrakeType::LoadCell,
        HandbrakeType::HallEffect,
    ];
    for (i, a) in types.iter().enumerate() {
        for (j, b) in types.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "types at index {i} and {j} should differ");
            }
        }
    }
}

#[test]
fn error_display_messages() {
    let err = HandbrakeError::InvalidPosition(42);
    assert!(err.to_string().contains("42"));

    let err = HandbrakeError::Disconnected;
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("disconnected"));
}

// ── Proptest ────────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_normalized_always_in_unit_range(
        raw in 0u16..=65535u16,
        min in 0u16..=32767u16,
        max in 32768u16..=65535u16,
    ) {
        let input = HandbrakeInput {
            raw_value: raw.max(min),
            is_engaged: raw > 100,
            calibration_min: min,
            calibration_max: max,
        };
        let n = input.normalized();
        prop_assert!((0.0..=1.0).contains(&n), "normalized={n}");
    }

    #[test]
    fn prop_parse_roundtrip_preserves_raw_value(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let data = [0x00, 0x00, lo, hi];
        if let Ok(input) = HandbrakeInput::parse_gamepad(&data) {
            let expected = u16::from(lo) | (u16::from(hi) << 8);
            prop_assert_eq!(input.raw_value, expected);
        }
    }

    #[test]
    fn prop_calibration_sample_tracks_min_max(
        samples in proptest::collection::vec(any::<u16>(), 1..50),
    ) {
        let mut cal = HandbrakeCalibration::new();
        for &s in &samples {
            cal.sample(s);
        }
        let Some(expected_min) = samples.iter().min().copied() else { return Ok(()); };
        let Some(expected_max) = samples.iter().max().copied() else { return Ok(()); };
        prop_assert_eq!(cal.min, expected_min);
        prop_assert_eq!(cal.max, expected_max);
    }
}
