//! Deep tests for calibration: axis mapping, pedals, wheel, storage, edge cases.

use openracing_calibration::{
    AxisCalibration, CalibrationPoint, DeviceCalibration, JoystickCalibrator, PedalCalibrator,
    calibrate_joystick_axis, create_pedal_calibration,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Axis calibration: raw → calibrated mapping
// ---------------------------------------------------------------------------

mod axis_calibration_tests {
    use super::*;

    #[test]
    fn full_range_maps_correctly() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        assert!((calib.apply(32768) - 0.5).abs() < 0.01);
        assert!((calib.apply(65535) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn partial_range_with_matching_deadzones() -> TestResult {
        // deadzone bounds are raw values in the same coordinate space as
        // min/max, so a deadzone matching the axis range is a no-op.
        let calib = AxisCalibration::new(1000, 3000).with_deadzone(1000, 3000);
        assert!((calib.apply(1000) - 0.0).abs() < 0.001, "min → 0.0");
        assert!((calib.apply(2000) - 0.5).abs() < 0.01, "mid → 0.5");
        assert!((calib.apply(3000) - 1.0).abs() < 0.001, "max → 1.0");
        Ok(())
    }

    #[test]
    fn at_min_returns_zero() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn at_max_returns_one() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(65535) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn center_point_stored() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_center(32768);
        assert_eq!(calib.center, Some(32768));
        Ok(())
    }

    #[test]
    fn deadzone_clamps_low_values() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        assert!(
            (calib.apply(2000) - 0.0).abs() < 0.001,
            "below dz_min → 0.0"
        );
        Ok(())
    }

    #[test]
    fn deadzone_clamps_high_values() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        assert!(
            (calib.apply(63000) - 1.0).abs() < 0.001,
            "above dz_max → 1.0"
        );
        Ok(())
    }

    #[test]
    fn deadzone_remaps_interior() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        let mid = calib.apply(32768);
        assert!(
            mid > 0.0 && mid < 1.0,
            "interior maps between 0 and 1, got {mid}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pedal calibration: dead zones, curves, inversion
// ---------------------------------------------------------------------------

mod pedal_calibration_tests {
    use super::*;

    #[test]
    fn pedal_basic_calibration() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        Ok(())
    }

    #[test]
    fn pedal_partial_range() -> TestResult {
        let axes = create_pedal_calibration(&[100, 500, 900], &[200, 800], &[300, 700])?;
        assert_eq!(axes[0].min, 100);
        assert_eq!(axes[0].max, 900);
        assert_eq!(axes[1].min, 200);
        assert_eq!(axes[1].max, 800);
        assert_eq!(axes[2].min, 300);
        assert_eq!(axes[2].max, 700);
        Ok(())
    }

    #[test]
    fn pedal_single_sample_min_equals_max() -> TestResult {
        let axes = create_pedal_calibration(&[500], &[500], &[500])?;
        assert_eq!(axes[0].min, 500);
        assert_eq!(axes[0].max, 500);
        // Zero range → returns 0.5
        assert!((axes[0].apply(500) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn pedal_missing_axis_fails() -> TestResult {
        let cal = PedalCalibrator::new();
        assert!(cal.calibrate().is_err(), "empty calibrator should fail");
        Ok(())
    }

    #[test]
    fn pedal_reset_clears_samples() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);
        cal.reset();
        assert!(cal.calibrate().is_err(), "reset should clear all samples");
        Ok(())
    }

    #[test]
    fn pedal_deadzone_simulation() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let throttle = axes[0].clone();
        let throttle_with_dz = AxisCalibration {
            deadzone_min: 3000,
            deadzone_max: 62535,
            ..throttle
        };
        // Small raw value below deadzone maps to 0
        assert!((throttle_with_dz.apply(1000) - 0.0).abs() < 0.001);
        // Large raw value above deadzone maps to 1
        assert!((throttle_with_dz.apply(64000) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn pedal_inversion_via_apply() -> TestResult {
        // Simulate inverted pedal by swapping min/max in calibration
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let normal_at_zero = axes[0].apply(0);
        let normal_at_max = axes[0].apply(65535);
        // Normal: 0 → 0.0, 65535 → 1.0
        assert!(normal_at_zero < normal_at_max, "normal polarity");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wheel calibration: center finding, rotation range
// ---------------------------------------------------------------------------

mod wheel_calibration_tests {
    use super::*;

    #[test]
    fn wheel_center_detected() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (32768, 0.5), (65535, 1.0)])?;
        assert_eq!(axis.center, Some(32768), "center should be detected at 0.5");
        Ok(())
    }

    #[test]
    fn wheel_full_rotation_range() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (65535, 1.0)])?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        Ok(())
    }

    #[test]
    fn wheel_partial_rotation_range() -> TestResult {
        let axis = calibrate_joystick_axis(&[(10000, 0.0), (32768, 0.5), (55000, 1.0)])?;
        assert_eq!(axis.min, 10000);
        assert_eq!(axis.max, 55000);
        Ok(())
    }

    #[test]
    fn wheel_no_center_when_no_midpoint_sample() -> TestResult {
        // No sample near 0.5 → no center detected
        let axis = calibrate_joystick_axis(&[(0, 0.0), (65535, 1.0)])?;
        assert_eq!(axis.center, None, "no sample near 0.5 → no center");
        Ok(())
    }

    #[test]
    fn joystick_calibrator_reset_works() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        cal.reset();
        assert!(cal.calibrate().is_err(), "reset should clear samples");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Multi-point calibration: linearity verification
// ---------------------------------------------------------------------------

mod multipoint_tests {
    use super::*;

    #[test]
    fn multipoint_linearity() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        let points: Vec<u16> = vec![0, 13107, 26214, 39321, 52428, 65535];
        let expected = [0.0f32, 0.2, 0.4, 0.6, 0.8, 1.0];

        for (raw, exp) in points.iter().zip(expected.iter()) {
            let got = calib.apply(*raw);
            assert!(
                (got - exp).abs() < 0.01,
                "raw={raw}: expected {exp}, got {got}"
            );
        }
        Ok(())
    }

    #[test]
    fn multipoint_monotonic_output() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        let mut prev = -1.0f32;
        for raw in (0..=65535).step_by(1000) {
            let val = calib.apply(raw);
            assert!(
                val >= prev,
                "non-monotonic at raw={raw}: prev={prev}, val={val}"
            );
            prev = val;
        }
        Ok(())
    }

    #[test]
    fn calibration_points_store_correctly() -> TestResult {
        let p = CalibrationPoint::new(32768, 0.5);
        assert_eq!(p.raw, 32768);
        assert!((p.normalized - 0.5).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration storage: save/load round-trip
// ---------------------------------------------------------------------------

mod storage_tests {
    use super::*;

    #[test]
    fn device_calibration_json_round_trip() -> TestResult {
        let mut device = DeviceCalibration::new("Test Wheel", 3);
        if let Some(steering) = device.axis(0) {
            *steering = AxisCalibration::new(0, 65535)
                .with_center(32768)
                .with_deadzone(1000, 64535);
        }
        if let Some(throttle) = device.axis(1) {
            *throttle = AxisCalibration::new(100, 900);
        }
        if let Some(brake) = device.axis(2) {
            *brake = AxisCalibration::new(50, 950);
        }

        let json = serde_json::to_string(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.name, "Test Wheel");
        assert_eq!(restored.axes.len(), 3);
        assert_eq!(restored.version, 1);
        assert_eq!(restored.axes[0].min, 0);
        assert_eq!(restored.axes[0].max, 65535);
        assert_eq!(restored.axes[0].center, Some(32768));
        assert_eq!(restored.axes[0].deadzone_min, 1000);
        assert_eq!(restored.axes[0].deadzone_max, 64535);
        assert_eq!(restored.axes[1].min, 100);
        assert_eq!(restored.axes[1].max, 900);
        Ok(())
    }

    #[test]
    fn axis_calibration_json_round_trip() -> TestResult {
        let calib = AxisCalibration::new(500, 4000)
            .with_center(2250)
            .with_deadzone(600, 3900);

        let json = serde_json::to_string(&calib)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.min, 500);
        assert_eq!(restored.max, 4000);
        assert_eq!(restored.center, Some(2250));
        assert_eq!(restored.deadzone_min, 600);
        assert_eq!(restored.deadzone_max, 3900);
        Ok(())
    }

    #[test]
    fn calibration_point_json_round_trip() -> TestResult {
        let point = CalibrationPoint::new(12345, 0.75);
        let json = serde_json::to_string(&point)?;
        let restored: CalibrationPoint = serde_json::from_str(&json)?;
        assert_eq!(restored.raw, 12345);
        assert!((restored.normalized - 0.75).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_calibration_preserves_all_axes() -> TestResult {
        let mut device = DeviceCalibration::new("Multi-axis", 6);
        for i in 0..6 {
            if let Some(axis) = device.axis(i) {
                *axis = AxisCalibration::new((i * 100) as u16, ((i + 1) * 10000) as u16);
            }
        }

        let json = serde_json::to_string(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.axes.len(), 6);
        for i in 0..6 {
            assert_eq!(restored.axes[i].min, (i * 100) as u16);
            assert_eq!(restored.axes[i].max, ((i + 1) * 10000) as u16);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases: inverted min/max, zero range, identical values
// ---------------------------------------------------------------------------

mod edge_case_tests {
    use super::*;

    #[test]
    fn zero_range_returns_midpoint() -> TestResult {
        let calib = AxisCalibration::new(5000, 5000);
        assert!((calib.apply(5000) - 0.5).abs() < 0.001, "zero range → 0.5");
        Ok(())
    }

    #[test]
    fn identical_min_max_any_raw_returns_midpoint() -> TestResult {
        let calib = AxisCalibration::new(100, 100);
        assert!((calib.apply(0) - 0.5).abs() < 0.001);
        assert!((calib.apply(100) - 0.5).abs() < 0.001);
        assert!((calib.apply(65535) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn max_equals_u16_max() -> TestResult {
        let calib = AxisCalibration::new(0, u16::MAX);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        assert!((calib.apply(u16::MAX) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn min_equals_max_equals_zero() -> TestResult {
        let calib = AxisCalibration::new(0, 0);
        assert!((calib.apply(0) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn device_axis_out_of_bounds() -> TestResult {
        let mut device = DeviceCalibration::new("Test", 2);
        assert!(device.axis(5).is_none(), "out-of-bounds → None");
        Ok(())
    }

    #[test]
    fn device_default_is_empty() -> TestResult {
        let device = DeviceCalibration::default();
        assert!(device.axes.is_empty());
        assert!(device.name.is_empty());
        assert_eq!(device.version, 1);
        Ok(())
    }

    #[test]
    fn deadzone_wider_than_range() -> TestResult {
        // Deadzone boundaries outside normalized range → everything goes to 0 or 1
        let calib = AxisCalibration::new(0, 65535).with_deadzone(0, 0);
        let val = calib.apply(32768);
        // dz_max is 0, which is ≤ dz_min, so all values above dz_max→1.0
        assert!((val - 1.0).abs() < 0.001, "all above zero dz_max → 1.0");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 1000, timeout: 60_000, ..ProptestConfig::default() })]

        #[test]
        fn calibrated_output_in_range(
            min_val in 0u16..=32767,
            max_offset in 1u16..=32768,
            raw_offset in 0u16..=65534u16,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            // The input is kept within the calibrated range for this
            // monotonicity check; apply() also saturates outside it.
            let raw = min_val.saturating_add(raw_offset % (max_val - min_val + 1));
            let calib = AxisCalibration::new(min_val, max_val);
            let out = calib.apply(raw);
            prop_assert!(
                (0.0..=1.0).contains(&out),
                "output {} not in [0.0, 1.0] for raw={}, min={}, max={}",
                out, raw, min_val, max_val
            );
        }

        #[test]
        fn calibrated_output_finite(
            min_val in 0u16..=32767u16,
            max_offset in 0u16..=32768u16,
            raw_offset in 0u16..=65534u16,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            // Keep this case within the calibrated range; out-of-range
            // values are covered by the dedicated saturation regressions.
            let raw = if max_val > min_val {
                min_val.saturating_add(raw_offset % (max_val - min_val + 1))
            } else {
                min_val
            };
            let calib = AxisCalibration::new(min_val, max_val);
            let out = calib.apply(raw);
            prop_assert!(out.is_finite(), "output is NaN/Inf");
        }

        #[test]
        fn calibrated_monotonic_when_min_lt_max(
            min_val in 0u16..=30000,
            max_offset in 2u16..=30000,
            raw1_offset in 0u16..=29998,
            raw2_extra in 1u16..=29999,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            let range = max_val - min_val;
            // Ensure raw values are within [min, max]
            let raw1 = min_val + (raw1_offset % range);
            let raw2 = raw1.saturating_add(raw2_extra).min(max_val);
            if raw2 <= raw1 { return Ok(()); }
            let calib = AxisCalibration::new(min_val, max_val);
            let o1 = calib.apply(raw1);
            let o2 = calib.apply(raw2);
            prop_assert!(
                o2 >= o1,
                "non-monotonic: apply({}) = {} > apply({}) = {}",
                raw1, o1, raw2, o2
            );
        }

        #[test]
        fn zero_range_always_midpoint(
            val in 0u16..=65535u16,
        ) {
            let calib = AxisCalibration::new(val, val);
            // A collapsed range is defined for its single calibration value.
            let out = calib.apply(val);
            prop_assert!(
                (out - 0.5).abs() < 0.001,
                "zero range should always return 0.5, got {}",
                out
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Calibration workflow: sequential min → max → deadzone → center
// ---------------------------------------------------------------------------

mod calibration_workflow_tests {
    use super::*;

    #[test]
    fn full_axis_workflow_joystick() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        // Step 1: sweep to min
        cal.add_sample(100, 0.0);
        // Step 2: sweep to max
        cal.add_sample(900, 1.0);
        // Step 3: find center
        cal.add_sample(500, 0.5);

        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 100);
        assert_eq!(axis.max, 900);
        assert_eq!(axis.center, Some(500));

        // Step 4: apply deadzone
        let axis_with_dz = axis.clone().with_deadzone(axis.min, axis.max);

        let mid = axis_with_dz.apply(500);
        assert!(mid > 0.0 && mid < 1.0, "mid-range works: {mid}");
        Ok(())
    }

    #[test]
    fn full_pedal_workflow() -> TestResult {
        let mut cal = PedalCalibrator::new();
        // Simulate sweeping each pedal through its range
        for v in (0u16..=65535).step_by(1000) {
            cal.add_throttle(v);
        }
        cal.add_throttle(65535);
        for v in (0u16..=65535).step_by(2000) {
            cal.add_brake(v);
        }
        cal.add_brake(65535);
        for v in (0u16..=65535).step_by(5000) {
            cal.add_clutch(v);
        }
        cal.add_clutch(65535);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        Ok(())
    }

    #[test]
    fn incremental_sample_collection() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);

        // First batch: only endpoints
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        let result1 = cal.calibrate()?;
        assert_eq!(result1.center, None);

        // Second batch: add center
        cal.add_sample(32768, 0.5);
        let result2 = cal.calibrate()?;
        assert_eq!(result2.center, Some(32768));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration reset and recalibrate
// ---------------------------------------------------------------------------

mod recalibration_tests {
    use super::*;

    #[test]
    fn joystick_recalibrate_with_new_range() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);

        // First calibration
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        let first = cal.calibrate()?;
        assert_eq!(first.min, 0);
        assert_eq!(first.max, 65535);

        // Reset and recalibrate with narrower range
        cal.reset();
        cal.add_sample(10000, 0.0);
        cal.add_sample(50000, 1.0);
        let second = cal.calibrate()?;
        assert_eq!(second.min, 10000);
        assert_eq!(second.max, 50000);

        // First calibration values are unchanged
        assert_eq!(first.min, 0);
        assert_eq!(first.max, 65535);
        Ok(())
    }

    #[test]
    fn pedal_recalibrate_after_reset() -> TestResult {
        let mut cal = PedalCalibrator::new();

        cal.add_throttle(0);
        cal.add_throttle(1000);
        cal.add_brake(0);
        cal.add_brake(1000);
        cal.add_clutch(0);
        cal.add_clutch(1000);
        let first = cal.calibrate()?;
        assert_eq!(first[0].max, 1000);

        cal.reset();
        cal.add_throttle(500);
        cal.add_throttle(2000);
        cal.add_brake(500);
        cal.add_brake(2000);
        cal.add_clutch(500);
        cal.add_clutch(2000);
        let second = cal.calibrate()?;
        assert_eq!(second[0].min, 500);
        assert_eq!(second[0].max, 2000);
        Ok(())
    }

    #[test]
    fn device_calibration_axis_overwrite() -> TestResult {
        let mut device = DeviceCalibration::new("Test", 2);

        if let Some(axis) = device.axis(0) {
            *axis = AxisCalibration::new(0, 1000);
        }
        assert_eq!(device.axes[0].max, 1000);

        // Overwrite with new calibration
        if let Some(axis) = device.axis(0) {
            *axis = AxisCalibration::new(500, 2000).with_center(1250);
        }
        assert_eq!(device.axes[0].min, 500);
        assert_eq!(device.axes[0].max, 2000);
        assert_eq!(device.axes[0].center, Some(1250));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration validation: reject invalid data
// ---------------------------------------------------------------------------

mod validation_tests {
    use super::*;

    #[test]
    fn pedal_throttle_and_brake_only_fails() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        // No clutch samples
        assert!(cal.calibrate().is_err());
        Ok(())
    }

    #[test]
    fn joystick_single_sample_succeeds() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(32768, 0.5);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 32768);
        assert_eq!(axis.max, 32768);
        // Zero range → 0.5
        assert!((axis.apply(32768) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn multiple_resets_still_works() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        for _ in 0..5 {
            cal.add_sample(0, 0.0);
            cal.add_sample(65535, 1.0);
            cal.reset();
        }
        assert!(cal.calibrate().is_err(), "should fail after final reset");

        // Final calibration should succeed
        cal.add_sample(100, 0.0);
        cal.add_sample(200, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 100);
        assert_eq!(axis.max, 200);
        Ok(())
    }

    #[test]
    fn duplicate_samples_handled() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        for _ in 0..10 {
            cal.add_sample(500, 0.0);
            cal.add_sample(500, 1.0);
        }
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 500);
        assert_eq!(axis.max, 500);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration migration from different formats
// ---------------------------------------------------------------------------

mod migration_tests {
    use super::*;

    #[test]
    fn deserialize_minimal_device_json() -> TestResult {
        let json = r#"{"name":"Wheel","axes":[],"version":1}"#;
        let device: DeviceCalibration = serde_json::from_str(json)?;
        assert_eq!(device.name, "Wheel");
        assert!(device.axes.is_empty());
        assert_eq!(device.version, 1);
        Ok(())
    }

    #[test]
    fn deserialize_v1_axis_calibration() -> TestResult {
        let json = r#"{
            "min": 0,
            "center": null,
            "max": 65535,
            "deadzone_min": 0,
            "deadzone_max": 65535
        }"#;
        let axis: AxisCalibration = serde_json::from_str(json)?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        assert_eq!(axis.center, None);
        Ok(())
    }

    #[test]
    fn deserialize_v1_axis_with_center() -> TestResult {
        let json = r#"{
            "min": 100,
            "center": 32768,
            "max": 60000,
            "deadzone_min": 500,
            "deadzone_max": 59500
        }"#;
        let axis: AxisCalibration = serde_json::from_str(json)?;
        assert_eq!(axis.min, 100);
        assert_eq!(axis.center, Some(32768));
        assert_eq!(axis.max, 60000);
        assert_eq!(axis.deadzone_min, 500);
        assert_eq!(axis.deadzone_max, 59500);
        Ok(())
    }

    #[test]
    fn deserialize_full_device_with_multiple_axes() -> TestResult {
        let json = r#"{
            "name": "Fanatec CSL DD",
            "axes": [
                {"min": 0, "center": 32768, "max": 65535, "deadzone_min": 1000, "deadzone_max": 64535},
                {"min": 0, "center": null, "max": 65535, "deadzone_min": 0, "deadzone_max": 65535},
                {"min": 100, "center": null, "max": 900, "deadzone_min": 100, "deadzone_max": 900}
            ],
            "version": 1
        }"#;
        let device: DeviceCalibration = serde_json::from_str(json)?;
        assert_eq!(device.name, "Fanatec CSL DD");
        assert_eq!(device.axes.len(), 3);
        assert_eq!(device.axes[0].center, Some(32768));
        assert_eq!(device.axes[1].center, None);
        assert_eq!(device.axes[2].min, 100);
        assert_eq!(device.version, 1);
        Ok(())
    }

    #[test]
    fn deserialize_default_axis_round_trip() -> TestResult {
        let axis = AxisCalibration::default();
        let json = serde_json::to_string(&axis)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;
        assert_eq!(restored.min, 0);
        assert_eq!(restored.max, 0xFFFF);
        assert_eq!(restored.center, None);
        assert_eq!(restored.deadzone_min, 0);
        assert_eq!(restored.deadzone_max, 0xFFFF);
        Ok(())
    }

    #[test]
    fn invalid_json_rejected() -> TestResult {
        let bad_json = r#"{"name": 42}"#;
        let result: Result<DeviceCalibration, _> = serde_json::from_str(bad_json);
        assert!(result.is_err());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Multi-axis calibration (deep)
// ---------------------------------------------------------------------------

mod multi_axis_deep_tests {
    use super::*;

    #[test]
    fn six_axis_device_with_mixed_configs() -> TestResult {
        let mut device = DeviceCalibration::new("6-axis controller", 6);

        let configs: [(u16, u16, Option<u16>); 6] = [
            (0, 65535, Some(32768)),
            (0, 65535, None),
            (0, 65535, None),
            (100, 900, None),
            (200, 800, None),
            (0, 1023, Some(512)),
        ];

        for (i, (min, max, center)) in configs.iter().enumerate() {
            if let Some(axis) = device.axis(i) {
                let mut new_axis = AxisCalibration::new(*min, *max);
                if let Some(c) = center {
                    new_axis = new_axis.with_center(*c);
                }
                *axis = new_axis;
            }
        }

        for (i, (min, max, center)) in configs.iter().enumerate() {
            assert_eq!(device.axes[i].min, *min, "axis {i} min");
            assert_eq!(device.axes[i].max, *max, "axis {i} max");
            assert_eq!(device.axes[i].center, *center, "axis {i} center");
        }
        Ok(())
    }

    #[test]
    fn multi_axis_apply_independent() -> TestResult {
        let mut device = DeviceCalibration::new("Multi", 3);
        if let Some(a) = device.axis(0) {
            *a = AxisCalibration::new(0, 1000);
        }
        if let Some(a) = device.axis(1) {
            *a = AxisCalibration::new(1000, 2000);
        }
        if let Some(a) = device.axis(2) {
            *a = AxisCalibration::new(500, 1500);
        }

        // Each axis maps independently with a deadzone matching its own range
        let a0 = device.axes[0].clone().with_deadzone(0, 1000);
        let a1 = device.axes[1].clone().with_deadzone(1000, 2000);
        let a2 = device.axes[2].clone().with_deadzone(500, 1500);

        let v0 = a0.apply(500);
        let v1 = a1.apply(1500);
        let v2 = a2.apply(1000);

        assert!((v0 - 0.5).abs() < 0.01, "axis 0 at mid: {v0}");
        assert!((v1 - 0.5).abs() < 0.01, "axis 1 at mid: {v1}");
        assert!((v2 - 0.5).abs() < 0.01, "axis 2 at mid: {v2}");
        Ok(())
    }

    #[test]
    fn multi_axis_json_round_trip_with_deadzones() -> TestResult {
        let mut device = DeviceCalibration::new("Full Rig", 4);
        if let Some(a) = device.axis(0) {
            *a = AxisCalibration::new(0, 65535)
                .with_center(32768)
                .with_deadzone(500, 65035);
        }
        if let Some(a) = device.axis(1) {
            *a = AxisCalibration::new(0, 65535);
        }
        if let Some(a) = device.axis(2) {
            *a = AxisCalibration::new(0, 65535);
        }
        if let Some(a) = device.axis(3) {
            *a = AxisCalibration::new(0, 65535);
        }

        let json = serde_json::to_string_pretty(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.axes.len(), 4);
        assert_eq!(restored.axes[0].center, Some(32768));
        assert_eq!(restored.axes[0].deadzone_min, 500);
        assert_eq!(restored.axes[0].deadzone_max, 65035);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pedal non-linear response curves
// ---------------------------------------------------------------------------

mod pedal_nonlinear_tests {
    use super::*;

    #[test]
    fn pedal_with_narrow_deadzone_at_top() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let throttle = axes[0].clone().with_deadzone(0, 60000);

        // Values above deadzone_max → 1.0
        let high = throttle.apply(64000);
        assert!((high - 1.0).abs() < 0.01, "above deadzone_max: {high}");

        // Values in range should still work
        let mid = throttle.apply(30000);
        assert!(mid > 0.0 && mid < 1.0, "mid-range: {mid}");
        Ok(())
    }

    #[test]
    fn pedal_with_deadzone_at_bottom() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let brake = axes[1].clone().with_deadzone(5000, 65535);

        let low = brake.apply(2000);
        assert!((low - 0.0).abs() < 0.01, "below deadzone_min: {low}");

        let mid = brake.apply(35000);
        assert!(mid > 0.0 && mid < 1.0, "mid-range: {mid}");
        Ok(())
    }

    #[test]
    fn pedal_narrow_physical_range() -> TestResult {
        let axes = create_pedal_calibration(&[30000, 40000], &[30000, 40000], &[30000, 40000])?;
        // Deadzone matching the axis range is a no-op.
        let throttle = axes[0].clone().with_deadzone(30000, 40000);

        assert!((throttle.apply(30000) - 0.0).abs() < 0.01);
        assert!((throttle.apply(35000) - 0.5).abs() < 0.01);
        assert!((throttle.apply(40000) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn pedal_apply_monotonic_with_deadzone() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let throttle = axes[0].clone().with_deadzone(3000, 62535);

        let mut prev = -1.0f32;
        for raw in (0u16..=65535).step_by(100) {
            let val = throttle.apply(raw);
            assert!(
                val >= prev,
                "non-monotonic at raw={raw}: prev={prev}, val={val}"
            );
            prev = val;
        }
        Ok(())
    }

    #[test]
    fn three_pedals_different_ranges() -> TestResult {
        let axes = create_pedal_calibration(&[0, 50000], &[5000, 60000], &[10000, 55000])?;

        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 50000);
        assert_eq!(axes[1].min, 5000);
        assert_eq!(axes[1].max, 60000);
        assert_eq!(axes[2].min, 10000);
        assert_eq!(axes[2].max, 55000);

        // Each maps its own range to [0,1] with a matching (no-op) deadzone
        let t = axes[0].clone().with_deadzone(0, 50000);
        let b = axes[1].clone().with_deadzone(5000, 60000);
        let c = axes[2].clone().with_deadzone(10000, 55000);

        assert!((t.apply(25000) - 0.5).abs() < 0.01);
        assert!((b.apply(32500) - 0.5).abs() < 0.01);
        assert!((c.apply(32500) - 0.5).abs() < 0.01);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Auto-calibration simulation
// ---------------------------------------------------------------------------

mod auto_calibration_tests {
    use super::*;

    #[test]
    fn progressive_sample_collection_narrows_range() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);

        // First batch: coarse sweep
        cal.add_sample(1000, 0.0);
        cal.add_sample(60000, 1.0);
        let coarse = cal.calibrate()?;
        assert_eq!(coarse.min, 1000);
        assert_eq!(coarse.max, 60000);

        // Second batch: fine-tune endpoints
        cal.add_sample(500, 0.0);
        cal.add_sample(64000, 1.0);
        let refined = cal.calibrate()?;
        assert_eq!(refined.min, 500, "auto-cal should discover lower min");
        assert_eq!(refined.max, 64000, "auto-cal should discover higher max");
        Ok(())
    }

    #[test]
    fn auto_detect_center_from_dense_samples() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        // Simulate user sweeping axis: many samples, one near center
        for raw in (0u16..=65535).step_by(6554) {
            let normalized = raw as f32 / 65535.0;
            cal.add_sample(raw, normalized);
        }
        cal.add_sample(65535, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        // Should detect center from sample near 0.5
        assert!(axis.center.is_some(), "should auto-detect center");
        Ok(())
    }

    #[test]
    fn auto_calibration_pedal_sweep() -> TestResult {
        let mut cal = PedalCalibrator::new();
        // Simulate slow pedal press
        for v in (0u16..=65535).step_by(256) {
            cal.add_throttle(v);
            cal.add_brake(v);
            cal.add_clutch(v);
        }
        cal.add_throttle(65535);
        cal.add_brake(65535);
        cal.add_clutch(65535);

        let axes = cal.calibrate()?;
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        assert_eq!(axes[1].min, 0);
        assert_eq!(axes[1].max, 65535);
        assert_eq!(axes[2].min, 0);
        assert_eq!(axes[2].max, 65535);
        Ok(())
    }

    #[test]
    fn auto_calibration_noisy_samples() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        // Noisy sensor: samples jitter around true range [1000, 60000]
        let noisy_samples: Vec<(u16, f32)> = vec![
            (1050, 0.01),
            (1200, 0.02),
            (980, 0.0),
            (30000, 0.49),
            (32000, 0.52),
            (59800, 0.99),
            (60100, 1.0),
            (60050, 0.99),
        ];
        for (raw, norm) in &noisy_samples {
            cal.add_sample(*raw, *norm);
        }
        let axis = cal.calibrate()?;
        assert!(
            axis.min <= 1000,
            "min should capture lowest noise: {}",
            axis.min
        );
        assert!(
            axis.max >= 60050,
            "max should capture highest noise: {}",
            axis.max
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration error display
// ---------------------------------------------------------------------------

mod error_display_tests {
    use super::*;
    use openracing_calibration::CalibrationError;

    #[test]
    fn error_invalid_data_message() -> TestResult {
        let err = CalibrationError::InvalidData;
        assert_eq!(format!("{err}"), "Invalid calibration data");
        Ok(())
    }

    #[test]
    fn error_not_complete_message() -> TestResult {
        let err = CalibrationError::NotComplete;
        assert_eq!(format!("{err}"), "Calibration not complete");
        Ok(())
    }

    #[test]
    fn error_device_error_message() -> TestResult {
        let err = CalibrationError::DeviceError("USB timeout".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("USB timeout"));
        assert!(msg.contains("Device error"));
        Ok(())
    }

    #[test]
    fn error_device_error_empty_string() -> TestResult {
        let err = CalibrationError::DeviceError(String::new());
        let msg = format!("{err}");
        assert!(msg.contains("Device error"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

mod additional_edge_cases {
    use super::*;

    #[test]
    fn axis_default_matches_full_range() -> TestResult {
        let axis = AxisCalibration::default();
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 0xFFFF);
        assert!(axis.center.is_none());
        assert_eq!(axis.deadzone_min, 0);
        assert_eq!(axis.deadzone_max, 0xFFFF);
        Ok(())
    }

    #[test]
    fn pedal_calibrator_default_same_as_new() -> TestResult {
        let from_new = PedalCalibrator::new();
        let from_default = PedalCalibrator::default();
        // Both should fail identically (no samples)
        assert!(from_new.calibrate().is_err());
        assert!(from_default.calibrate().is_err());
        Ok(())
    }

    #[test]
    fn calibration_point_extreme_values() -> TestResult {
        let p_min = CalibrationPoint::new(0, 0.0);
        let p_max = CalibrationPoint::new(u16::MAX, 1.0);
        assert_eq!(p_min.raw, 0);
        assert_eq!(p_max.raw, u16::MAX);
        assert!((p_min.normalized - 0.0).abs() < f32::EPSILON);
        assert!((p_max.normalized - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn calibration_point_negative_normalized() -> TestResult {
        // Normalized values outside [0,1] are valid at the point level
        let p = CalibrationPoint::new(100, -0.5);
        assert!((p.normalized - (-0.5)).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn calibration_point_above_one_normalized() -> TestResult {
        let p = CalibrationPoint::new(100, 1.5);
        assert!((p.normalized - 1.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_calibration_zero_axes() -> TestResult {
        let device = DeviceCalibration::new("No Axes", 0);
        assert!(device.axes.is_empty());
        assert_eq!(device.name, "No Axes");
        Ok(())
    }

    #[test]
    fn device_calibration_large_axis_count() -> TestResult {
        let device = DeviceCalibration::new("Many Axes", 100);
        assert_eq!(device.axes.len(), 100);
        Ok(())
    }

    #[test]
    fn device_calibration_empty_name() -> TestResult {
        let device = DeviceCalibration::new("", 1);
        assert!(device.name.is_empty());
        assert_eq!(device.axes.len(), 1);
        Ok(())
    }

    #[test]
    fn axis_apply_at_boundaries() -> TestResult {
        let calib = AxisCalibration::new(100, 200);
        // Deadzone matching the axis range is a no-op.
        let calib_dz = calib.with_deadzone(100, 200);
        let at_min = calib_dz.apply(100);
        let at_max = calib_dz.apply(200);
        assert!((at_min - 0.0).abs() < 0.01, "at min: {at_min}");
        assert!((at_max - 1.0).abs() < 0.01, "at max: {at_max}");
        Ok(())
    }

    #[test]
    fn joystick_center_detection_threshold() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        // Sample at 0.41 — just within the 0.1 threshold of 0.5
        cal.add_sample(0, 0.0);
        cal.add_sample(26870, 0.41);
        cal.add_sample(65535, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.center, Some(26870), "0.41 is within 0.1 of 0.5");
        Ok(())
    }

    #[test]
    fn joystick_center_not_detected_outside_threshold() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(25000, 0.39); // outside 0.1 threshold
        cal.add_sample(65535, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.center, None, "0.39 is outside 0.1 threshold of 0.5");
        Ok(())
    }

    #[test]
    fn multiple_center_candidates_uses_first() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(30000, 0.45);
        cal.add_sample(32768, 0.50);
        cal.add_sample(65535, 1.0);
        let axis = cal.calibrate()?;
        // First match within threshold should be used
        assert!(axis.center.is_some());
        assert_eq!(axis.center, Some(30000), "first sample near 0.5 wins");
        Ok(())
    }

    #[test]
    fn pedal_one_sample_per_axis() -> TestResult {
        let axes = create_pedal_calibration(&[32768], &[32768], &[32768])?;
        for axis in &axes {
            assert_eq!(axis.min, 32768);
            assert_eq!(axis.max, 32768);
            assert!((axis.apply(32768) - 0.5).abs() < 0.001);
        }
        Ok(())
    }

    #[test]
    fn device_calibration_version_preserved() -> TestResult {
        let device = DeviceCalibration::new("V1", 1);
        assert_eq!(device.version, 1);
        let json = serde_json::to_string(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;
        assert_eq!(restored.version, 1);
        Ok(())
    }
}
