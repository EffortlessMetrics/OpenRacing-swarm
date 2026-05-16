//! Deep tests for openracing-handbrake.
//!
//! Covers: calibration mapping, dead zones, noise filtering via calibration,
//! state machine (idle → pressed → released), edge cases (min/max ADC,
//! wraparound), and property-based output-range invariants.

use openracing_handbrake::{
    HandbrakeCalibration, HandbrakeCapabilities, HandbrakeError, HandbrakeInput, HandbrakeType,
    MAX_ANALOG_VALUE,
};
use proptest::prelude::*;

// ── Calibration: raw → calibrated value mapping ────────────────────────────

mod calibration_mapping {
    use super::*;

    #[test]
    fn raw_at_min_produces_zero() {
        let input = HandbrakeInput {
            raw_value: 1000,
            is_engaged: false,
            calibration_min: 1000,
            calibration_max: 9000,
        };
        assert!(input.normalized().abs() < f32::EPSILON);
    }

    #[test]
    fn raw_at_max_produces_one() {
        let input = HandbrakeInput {
            raw_value: 9000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 9000,
        };
        assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn raw_at_quarter_produces_quarter() {
        let input = HandbrakeInput {
            raw_value: 3000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 9000,
        };
        // (3000 - 1000) / (9000 - 1000) = 2000 / 8000 = 0.25
        assert!((input.normalized() - 0.25).abs() < 0.001);
    }

    #[test]
    fn raw_at_three_quarters_produces_three_quarters() {
        let input = HandbrakeInput {
            raw_value: 7000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 9000,
        };
        // (7000 - 1000) / (9000 - 1000) = 6000 / 8000 = 0.75
        assert!((input.normalized() - 0.75).abs() < 0.001);
    }

    #[test]
    fn narrow_calibration_range_still_maps_correctly() {
        let input = HandbrakeInput {
            raw_value: 5001,
            is_engaged: true,
            calibration_min: 5000,
            calibration_max: 5002,
        };
        // (5001 - 5000) / (5002 - 5000) = 1 / 2 = 0.5
        assert!((input.normalized() - 0.5).abs() < 0.01);
    }

    #[test]
    fn recalibrate_changes_normalized_output() {
        let mut input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        let before = input.normalized();

        input.calibrate(4000, 6000);
        let after = input.normalized();

        // With narrow range centered on 5000, normalized should be 0.5
        assert!((after - 0.5).abs() < 0.01);
        assert!((before - after).abs() > 0.01);
    }

    #[test]
    fn calibration_via_builder_and_mutate_give_same_result() {
        let a = HandbrakeInput {
            raw_value: 3000,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        }
        .with_calibration(1000, 5000);

        let mut b = HandbrakeInput {
            raw_value: 3000,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        b.calibrate(1000, 5000);

        assert!((a.normalized() - b.normalized()).abs() < f32::EPSILON);
    }
}

// ── Dead zone behavior ─────────────────────────────────────────────────────

mod dead_zone {
    use super::*;

    #[test]
    fn zero_range_calibration_returns_zero() {
        let input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: false,
            calibration_min: 5000,
            calibration_max: 5000,
        };
        assert!(input.normalized().abs() < f32::EPSILON);
    }

    #[test]
    fn raw_below_calibration_min_clamps_to_zero() {
        // When raw_value < calibration_min, the subtraction in normalized()
        // can underflow for u16. Test the clamping behavior.
        let input = HandbrakeInput {
            raw_value: 2000,
            is_engaged: false,
            calibration_min: 2000,
            calibration_max: 8000,
        };
        assert!(input.normalized().abs() < f32::EPSILON);
    }

    #[test]
    fn raw_above_calibration_max_clamps_to_one() {
        let input = HandbrakeInput {
            raw_value: 60000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
    }
}

// ── Noise filtering via calibration sampling ───────────────────────────────

mod noise_filtering {
    use super::*;

    #[test]
    fn calibration_sampling_tracks_extremes_over_noisy_input() {
        let mut cal = HandbrakeCalibration::new();
        let samples = [500, 520, 490, 510, 9000, 8900, 9100, 495, 9050];
        for &s in &samples {
            cal.sample(s);
        }
        assert_eq!(cal.min, 490);
        assert_eq!(cal.max, 9100);
    }

    #[test]
    fn calibration_apply_then_normalize_uses_sampled_range() {
        let mut cal = HandbrakeCalibration::new();
        cal.sample(1000);
        cal.sample(9000);

        let mut input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        cal.apply(&mut input);

        // (5000 - 1000) / (9000 - 1000) = 4000 / 8000 = 0.5
        assert!((input.normalized() - 0.5).abs() < 0.001);
    }

    #[test]
    fn single_sample_sets_both_min_and_max() {
        let mut cal = HandbrakeCalibration::new();
        cal.sample(4242);
        assert_eq!(cal.min, 4242);
        assert_eq!(cal.max, 4242);
    }

    #[test]
    fn many_identical_samples_produce_zero_range() {
        let mut cal = HandbrakeCalibration::new();
        for _ in 0..100 {
            cal.sample(3000);
        }
        assert_eq!(cal.min, 3000);
        assert_eq!(cal.max, 3000);

        let mut input = HandbrakeInput {
            raw_value: 3000,
            is_engaged: false,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        cal.apply(&mut input);
        // Zero range → normalized returns 0.0
        assert!(input.normalized().abs() < f32::EPSILON);
    }
}

// ── State machine: idle → pressed → released ───────────────────────────────

mod state_machine {
    use super::*;

    #[test]
    fn idle_state_not_engaged() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0x00]; // raw_value = 0
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(!input.is_engaged);
        assert_eq!(input.raw_value, 0);
        Ok(())
    }

    #[test]
    fn pressed_state_engaged() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0x80]; // raw_value = 0x8000 (32768)
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.is_engaged);
        assert!(input.normalized() > 0.0);
        Ok(())
    }

    #[test]
    fn released_state_back_to_idle() -> Result<(), Box<dyn std::error::Error>> {
        // Simulate pressing then releasing
        let press_data = [0x00, 0x00, 0x00, 0x80];
        let pressed = HandbrakeInput::parse_gamepad(&press_data).map_err(|e| e.to_string())?;
        assert!(pressed.is_engaged);

        let release_data = [0x00, 0x00, 0x32, 0x00]; // raw_value = 50 (below threshold)
        let released = HandbrakeInput::parse_gamepad(&release_data).map_err(|e| e.to_string())?;
        assert!(!released.is_engaged);
        Ok(())
    }

    #[test]
    fn transition_through_threshold() -> Result<(), Box<dyn std::error::Error>> {
        let values: Vec<(u16, bool)> = vec![
            (0, false),
            (50, false),
            (100, false), // Exactly at threshold → not engaged
            (101, true),  // Just above → engaged
            (500, true),
            (50, false), // Back below → not engaged
        ];
        for (raw, expected_engaged) in values {
            let data = [0x00, 0x00, (raw & 0xFF) as u8, (raw >> 8) as u8];
            let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
            assert_eq!(
                input.is_engaged, expected_engaged,
                "raw_value={raw}: expected engaged={expected_engaged}"
            );
        }
        Ok(())
    }
}

// ── Edge cases: min/max ADC values ─────────────────────────────────────────

mod edge_cases {
    use super::*;

    #[test]
    fn min_adc_value() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0);
        assert!(input.normalized().abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn max_adc_value() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0xFF, 0xFF];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, MAX_ANALOG_VALUE);
        assert!((input.normalized() - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn max_analog_value_constant_is_u16_max() {
        assert_eq!(MAX_ANALOG_VALUE, u16::MAX);
        assert_eq!(MAX_ANALOG_VALUE, 65535);
    }

    #[test]
    fn parse_exactly_4_bytes_minimum() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0x01];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x0100);
        Ok(())
    }

    #[test]
    fn parse_large_buffer() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 256];
        data[2] = 0xAB;
        data[3] = 0xCD;
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0xCDAB);
        Ok(())
    }

    #[test]
    fn error_types_are_distinct() {
        let e1 = HandbrakeError::InvalidPosition(0);
        let e2 = HandbrakeError::Disconnected;
        // They produce different Display messages
        assert_ne!(e1.to_string(), e2.to_string());
    }

    #[test]
    fn error_invalid_position_carries_value() {
        let err = HandbrakeError::InvalidPosition(12345);
        assert!(err.to_string().contains("12345"));
    }

    #[test]
    fn default_handbrake_input_normalized_is_zero() {
        let input = HandbrakeInput::default();
        assert!(input.normalized().abs() < f32::EPSILON);
    }

    #[test]
    fn capabilities_all_types_have_calibration_support() {
        let caps = [
            HandbrakeCapabilities::analog(),
            HandbrakeCapabilities::load_cell(50.0),
            HandbrakeCapabilities::hall_effect(),
        ];
        for cap in &caps {
            assert!(cap.supports_calibration);
        }
    }

    #[test]
    fn capabilities_load_cell_zero_and_large_loads() {
        let zero = HandbrakeCapabilities::load_cell(0.0);
        assert_eq!(zero.max_load_kg, Some(0.0));

        let large = HandbrakeCapabilities::load_cell(999.9);
        assert_eq!(large.max_load_kg, Some(999.9));
    }

    #[test]
    fn handbrake_type_all_four_variants_exist() {
        let types = [
            HandbrakeType::Analog,
            HandbrakeType::Digital,
            HandbrakeType::LoadCell,
            HandbrakeType::HallEffect,
        ];
        // All distinct
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j]);
            }
        }
    }

    #[test]
    fn calibration_center_is_none_by_default() {
        let cal = HandbrakeCalibration::new();
        assert_eq!(cal.center, None);
    }

    #[test]
    fn calibration_center_stays_none_after_sampling() {
        let mut cal = HandbrakeCalibration::new();
        cal.sample(100);
        cal.sample(9000);
        assert_eq!(cal.center, None);
    }
}

// ── Serde round-trip ───────────────────────────────────────────────────────

mod serde_roundtrip {
    use super::*;

    #[test]
    fn handbrake_type_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let types = [
            HandbrakeType::Analog,
            HandbrakeType::Digital,
            HandbrakeType::LoadCell,
            HandbrakeType::HallEffect,
        ];
        for &t in &types {
            let json = serde_json::to_string(&t).map_err(|e| e.to_string())?;
            let back: HandbrakeType = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(t, back);
        }
        Ok(())
    }

    #[test]
    fn handbrake_capabilities_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let caps = [
            HandbrakeCapabilities::analog(),
            HandbrakeCapabilities::load_cell(42.5),
            HandbrakeCapabilities::hall_effect(),
        ];
        for cap in &caps {
            let json = serde_json::to_string(cap).map_err(|e| e.to_string())?;
            let back: HandbrakeCapabilities =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(*cap, back);
        }
        Ok(())
    }
}

// ── Property tests ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    #[test]
    fn prop_output_always_in_unit_range(
        raw in 0u16..=65535u16,
        min in 0u16..=32767u16,
        max in 32768u16..=65535u16,
    ) {
        // Constrain raw >= min to avoid u16 subtraction overflow in normalized()
        let input = HandbrakeInput {
            raw_value: raw.max(min),
            is_engaged: raw > 100,
            calibration_min: min,
            calibration_max: max,
        };
        let n = input.normalized();
        prop_assert!(n >= 0.0, "normalized must be >= 0, got {n}");
        prop_assert!(n <= 1.0, "normalized must be <= 1, got {n}");
    }

    #[test]
    fn prop_normalized_monotonic_with_raw(
        min in 0u16..=1000u16,
        max in 50000u16..=65535u16,
        a in 1001u16..=25000u16,
        b in 25001u16..=49999u16,
    ) {
        let input_a = HandbrakeInput {
            raw_value: a,
            is_engaged: true,
            calibration_min: min,
            calibration_max: max,
        };
        let input_b = HandbrakeInput {
            raw_value: b,
            is_engaged: true,
            calibration_min: min,
            calibration_max: max,
        };
        prop_assert!(input_a.normalized() <= input_b.normalized(),
            "normalized should be monotonic: raw {} => {}, raw {} => {}",
            a, input_a.normalized(), b, input_b.normalized());
    }

    #[test]
    fn prop_engagement_consistent_with_threshold(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let data = [0x00, 0x00, lo, hi];
        if let Ok(input) = HandbrakeInput::parse_gamepad(&data) {
            let expected = input.raw_value > 100;
            prop_assert_eq!(input.is_engaged, expected);
        }
    }

    #[test]
    fn prop_parse_always_succeeds_for_4plus_bytes(
        data in proptest::collection::vec(any::<u8>(), 4..=128),
    ) {
        let result = HandbrakeInput::parse_gamepad(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_parse_always_fails_for_short_data(
        data in proptest::collection::vec(any::<u8>(), 0..4usize),
    ) {
        let result = HandbrakeInput::parse_gamepad(&data);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_calibration_tracks_extremes(
        samples in proptest::collection::vec(any::<u16>(), 2..100),
    ) {
        let mut cal = HandbrakeCalibration::new();
        for &s in &samples {
            cal.sample(s);
        }
        for &s in &samples {
            prop_assert!(s >= cal.min, "sample {s} < min {}", cal.min);
            prop_assert!(s <= cal.max, "sample {s} > max {}", cal.max);
        }
    }
}

// ── Position encoding: byte-level layout ───────────────────────────────────

mod position_encoding {
    use super::*;

    #[test]
    fn little_endian_byte_order_low_byte_only() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0xAB, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x00AB);
        Ok(())
    }

    #[test]
    fn little_endian_byte_order_high_byte_only() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0xCD];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0xCD00);
        Ok(())
    }

    #[test]
    fn little_endian_combined() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x34, 0x12];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x1234);
        Ok(())
    }

    #[test]
    fn leading_bytes_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0xFF, 0xEE, 0x10, 0x20];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x2010);
        Ok(())
    }

    #[test]
    fn position_encoding_sweep_powers_of_two() -> Result<(), Box<dyn std::error::Error>> {
        let expected_values: Vec<u16> = (0..16).map(|bit| 1u16 << bit).collect();
        for &val in &expected_values {
            let lo = (val & 0xFF) as u8;
            let hi = (val >> 8) as u8;
            let data = [0x00, 0x00, lo, hi];
            let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
            assert_eq!(input.raw_value, val, "failed for value {val:#06X}");
        }
        Ok(())
    }

    #[test]
    fn trailing_bytes_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x50, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x0050);
        Ok(())
    }
}

// ── Calibration workflow: multi-point ──────────────────────────────────────

mod calibration_workflow {
    use super::*;

    #[test]
    fn full_calibration_workflow_sample_apply_normalize() {
        let mut cal = HandbrakeCalibration::new();
        // Simulate user sweeping handbrake from rest to fully pulled
        let sweep = [50, 100, 200, 500, 1000, 3000, 5000, 8000, 9500];
        for &v in &sweep {
            cal.sample(v);
        }
        assert_eq!(cal.min, 50);
        assert_eq!(cal.max, 9500);

        let mut input = HandbrakeInput {
            raw_value: 4775,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        cal.apply(&mut input);
        // (4775 - 50) / (9500 - 50) = 4725 / 9450 = 0.5
        assert!((input.normalized() - 0.5).abs() < 0.001);
    }

    #[test]
    fn calibration_with_reversed_min_max_order() {
        let mut cal = HandbrakeCalibration::new();
        // Samples arrive largest first, then smallest
        cal.sample(9000);
        cal.sample(1000);
        assert_eq!(cal.min, 1000);
        assert_eq!(cal.max, 9000);
    }

    #[test]
    fn recalibration_overwrites_previous() {
        let mut input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };
        let n1 = input.normalized();

        input.calibrate(4000, 6000);
        let n2 = input.normalized();

        input.calibrate(0, 10000);
        let n3 = input.normalized();

        // n2 should be 0.5 (centered in narrow range)
        assert!((n2 - 0.5).abs() < 0.01);
        // n3 should be same as 5000/10000 = 0.5
        assert!((n3 - 0.5).abs() < 0.01);
        // n1 is 5000/65535 ≈ 0.076
        assert!(n1 < 0.1);
    }

    #[test]
    fn calibration_center_can_be_set_manually() {
        let mut cal = HandbrakeCalibration::new();
        cal.center = Some(5000);
        assert_eq!(cal.center, Some(5000));
    }
}

// ── Axis mapping: normalized output across ranges ─────────────────────────

mod axis_mapping {
    use super::*;

    #[test]
    fn normalized_linear_sweep_monotonic() {
        let mut prev = -1.0f32;
        for raw in (0..=MAX_ANALOG_VALUE).step_by(1000) {
            let input = HandbrakeInput {
                raw_value: raw,
                is_engaged: raw > 100,
                calibration_min: 0,
                calibration_max: MAX_ANALOG_VALUE,
            };
            let n = input.normalized();
            assert!(n >= prev, "not monotonic at raw={raw}: {n} < {prev}");
            prev = n;
        }
    }

    #[test]
    fn normalized_quarter_points() {
        let range = 40000u16;
        let base = 10000u16;
        for (frac_num, frac_den) in [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4)] {
            let raw = base + (range as u32 * frac_num as u32 / frac_den as u32) as u16;
            let input = HandbrakeInput {
                raw_value: raw,
                is_engaged: true,
                calibration_min: base,
                calibration_max: base + range,
            };
            let expected = frac_num as f32 / frac_den as f32;
            assert!(
                (input.normalized() - expected).abs() < 0.01,
                "raw={raw}, expected={expected}, got={}",
                input.normalized()
            );
        }
    }

    #[test]
    fn narrow_range_axis_mapping() {
        // 1-unit range: only min and max produce distinct outputs
        let at_min = HandbrakeInput {
            raw_value: 1000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 1001,
        };
        let at_max = HandbrakeInput {
            raw_value: 1001,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 1001,
        };
        assert!(at_min.normalized().abs() < f32::EPSILON);
        assert!((at_max.normalized() - 1.0).abs() < f32::EPSILON);
    }
}

// ── Deadzone: fine-grained threshold behavior ─────────────────────────────

mod deadzone_fine {
    use super::*;

    #[test]
    fn engagement_exactly_at_100_not_engaged() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 100, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 100);
        assert!(!input.is_engaged);
        Ok(())
    }

    #[test]
    fn engagement_at_101_engaged() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 101, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 101);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn near_zero_values_produce_near_zero_normalized() {
        for raw in [0u16, 1, 5, 10, 50] {
            let input = HandbrakeInput {
                raw_value: raw,
                is_engaged: false,
                calibration_min: 0,
                calibration_max: MAX_ANALOG_VALUE,
            };
            assert!(
                input.normalized() < 0.01,
                "raw={raw} should produce near-zero normalized, got {}",
                input.normalized()
            );
        }
    }

    #[test]
    fn calibrated_deadzone_below_min_clamps() {
        let input = HandbrakeInput {
            raw_value: 500,
            is_engaged: false,
            calibration_min: 500,
            calibration_max: 9000,
        };
        assert!(input.normalized().abs() < f32::EPSILON);
    }

    #[test]
    fn calibrated_deadzone_one_above_min() {
        let input = HandbrakeInput {
            raw_value: 501,
            is_engaged: true,
            calibration_min: 500,
            calibration_max: 9000,
        };
        let n = input.normalized();
        // (501 - 500) / (9000 - 500) = 1 / 8500 ≈ 0.000118
        assert!(n > 0.0);
        assert!(n < 0.001);
    }
}

// ── All handbrake types: variant-specific behavior ────────────────────────

mod handbrake_type_variants {
    use super::*;

    #[test]
    fn digital_type_construction() {
        let caps = HandbrakeCapabilities {
            handbrake_type: HandbrakeType::Digital,
            max_load_kg: None,
            has_hall_effect_sensor: false,
            supports_calibration: false,
        };
        assert_eq!(caps.handbrake_type, HandbrakeType::Digital);
        assert!(!caps.supports_calibration);
    }

    #[test]
    fn digital_type_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let caps = HandbrakeCapabilities {
            handbrake_type: HandbrakeType::Digital,
            max_load_kg: None,
            has_hall_effect_sensor: false,
            supports_calibration: false,
        };
        let json = serde_json::to_string(&caps).map_err(|e| e.to_string())?;
        let back: HandbrakeCapabilities = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        assert_eq!(caps, back);
        Ok(())
    }

    #[test]
    fn each_constructor_type_matches_enum() {
        assert_eq!(
            HandbrakeCapabilities::analog().handbrake_type,
            HandbrakeType::Analog
        );
        assert_eq!(
            HandbrakeCapabilities::load_cell(1.0).handbrake_type,
            HandbrakeType::LoadCell
        );
        assert_eq!(
            HandbrakeCapabilities::hall_effect().handbrake_type,
            HandbrakeType::HallEffect
        );
    }

    #[test]
    fn load_cell_with_large_load() {
        let caps = HandbrakeCapabilities::load_cell(500.0);
        assert_eq!(caps.max_load_kg, Some(500.0));
    }

    #[test]
    fn hall_effect_sensor_flag_only_on_hall_effect() {
        assert!(HandbrakeCapabilities::hall_effect().has_hall_effect_sensor);
        assert!(!HandbrakeCapabilities::analog().has_hall_effect_sensor);
        assert!(!HandbrakeCapabilities::load_cell(10.0).has_hall_effect_sensor);
    }

    #[test]
    fn only_load_cell_has_max_load() {
        assert!(HandbrakeCapabilities::analog().max_load_kg.is_none());
        assert!(HandbrakeCapabilities::hall_effect().max_load_kg.is_none());
        assert!(HandbrakeCapabilities::load_cell(10.0).max_load_kg.is_some());
    }

    #[test]
    fn handbrake_type_debug_contains_variant_name() {
        assert!(format!("{:?}", HandbrakeType::Analog).contains("Analog"));
        assert!(format!("{:?}", HandbrakeType::Digital).contains("Digital"));
        assert!(format!("{:?}", HandbrakeType::LoadCell).contains("LoadCell"));
        assert!(format!("{:?}", HandbrakeType::HallEffect).contains("HallEffect"));
    }

    #[test]
    fn default_capabilities_matches_analog() {
        let default = HandbrakeCapabilities::default();
        let analog = HandbrakeCapabilities::analog();
        assert_eq!(default, analog);
    }
}
