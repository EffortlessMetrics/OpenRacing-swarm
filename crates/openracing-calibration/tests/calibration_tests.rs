//! Calibration hardening tests
//!
//! Comprehensive tests for calibration data creation, axis mapping,
//! dead-zone handling, device calibration, pedal and joystick calibrators,
//! serialization, and property-based fuzzing.

use openracing_calibration::{
    AxisCalibration, CalibrationError, CalibrationPoint, DeviceCalibration, JoystickCalibrator,
    PedalCalibrator, calibrate_joystick_axis, create_pedal_calibration,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// AxisCalibration
// ---------------------------------------------------------------------------

mod axis_calibration {
    use super::*;

    #[test]
    fn full_range_maps_correctly() {
        let cal = AxisCalibration::new(0, 65535);
        assert!((cal.apply(0) - 0.0).abs() < 0.01);
        assert!((cal.apply(32768) - 0.5).abs() < 0.01);
        assert!((cal.apply(65535) - 1.0).abs() < 0.01);
    }

    #[test]
    fn partial_range_min_maps_to_zero() {
        // With default deadzone (0..65535), a partial-range axis maps min → 0
        let cal = AxisCalibration::new(1000, 2000);
        assert!((cal.apply(1000) - 0.0).abs() < 0.01);
    }

    #[test]
    fn partial_range_with_matching_deadzone_maps_full_span() {
        // deadzone values live in the same raw coordinate space as min/max,
        // so a deadzone matching the axis range is a no-op.
        let cal = AxisCalibration::new(1000, 2000).with_deadzone(1000, 2000);
        assert!((cal.apply(1000) - 0.0).abs() < 0.01);
        assert!((cal.apply(1500) - 0.5).abs() < 0.01);
        assert!((cal.apply(2000) - 1.0).abs() < 0.01);
    }

    #[test]
    fn zero_range_returns_midpoint() {
        let cal = AxisCalibration::new(500, 500);
        let result = cal.apply(500);
        assert!(
            (result - 0.5).abs() < 0.01,
            "zero-range axis must return 0.5, got {}",
            result
        );
    }

    #[test]
    fn deadzone_clamps_below_min() {
        let cal = AxisCalibration::new(0, 65535).with_deadzone(5000, 60000);
        assert!(
            (cal.apply(0) - 0.0).abs() < 0.01,
            "below deadzone must map to 0.0"
        );
    }

    #[test]
    fn deadzone_clamps_above_max() {
        let cal = AxisCalibration::new(0, 65535).with_deadzone(5000, 60000);
        assert!(
            (cal.apply(65535) - 1.0).abs() < 0.01,
            "above deadzone must map to 1.0"
        );
    }

    #[test]
    fn center_point_is_stored() {
        let cal = AxisCalibration::new(0, 65535).with_center(32768);
        assert_eq!(cal.center, Some(32768));
    }

    #[test]
    fn default_calibration_is_full_range() {
        let cal = AxisCalibration::default();
        assert_eq!(cal.min, 0);
        assert_eq!(cal.max, 0xFFFF);
        assert!(cal.center.is_none());
    }

    #[test]
    fn apply_within_range_is_clamped_to_zero_one() {
        let cal = AxisCalibration::new(100, 200);
        let below_min = cal.apply(50);
        let at_min = cal.apply(100);
        let above = cal.apply(200);
        assert!(
            (below_min - 0.0).abs() < 0.01,
            "raw below min must saturate to 0.0, got {}",
            below_min
        );
        assert!(at_min >= 0.0, "output must be >= 0.0, got {}", at_min);
        assert!(above <= 1.0, "output must be <= 1.0, got {}", above);
    }

    #[test]
    fn serialization_roundtrip() -> TestResult {
        let original = AxisCalibration::new(100, 900)
            .with_center(500)
            .with_deadzone(120, 880);

        let json = serde_json::to_string(&original)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.min, 100);
        assert_eq!(restored.max, 900);
        assert_eq!(restored.center, Some(500));
        assert_eq!(restored.deadzone_min, 120);
        assert_eq!(restored.deadzone_max, 880);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CalibrationPoint
// ---------------------------------------------------------------------------

mod calibration_point {
    use super::*;

    #[test]
    fn new_stores_values() {
        let point = CalibrationPoint::new(32768, 0.5);
        assert_eq!(point.raw, 32768);
        assert!((point.normalized - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn serialization_roundtrip() -> TestResult {
        let original = CalibrationPoint::new(1000, 0.25);
        let json = serde_json::to_string(&original)?;
        let restored: CalibrationPoint = serde_json::from_str(&json)?;

        assert_eq!(restored.raw, 1000);
        assert!((restored.normalized - 0.25).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DeviceCalibration
// ---------------------------------------------------------------------------

mod device_calibration {
    use super::*;

    #[test]
    fn new_creates_correct_axis_count() {
        let device = DeviceCalibration::new("Test", 3);
        assert_eq!(device.axes.len(), 3);
        assert_eq!(device.name, "Test");
        assert_eq!(device.version, 1);
    }

    #[test]
    fn zero_axes_is_valid() {
        let device = DeviceCalibration::new("No Axes", 0);
        assert!(device.axes.is_empty());
    }

    #[test]
    fn axis_access_in_bounds() {
        let mut device = DeviceCalibration::new("Dev", 2);
        assert!(device.axis(0).is_some());
        assert!(device.axis(1).is_some());
    }

    #[test]
    fn axis_access_out_of_bounds_returns_none() {
        let mut device = DeviceCalibration::new("Dev", 2);
        assert!(device.axis(2).is_none());
        assert!(device.axis(100).is_none());
    }

    #[test]
    fn axis_mutation_persists() {
        let mut device = DeviceCalibration::new("Mutate", 1);
        if let Some(axis) = device.axis(0) {
            *axis = AxisCalibration::new(100, 900).with_center(500);
        }

        assert_eq!(device.axes[0].min, 100);
        assert_eq!(device.axes[0].max, 900);
        assert_eq!(device.axes[0].center, Some(500));
    }

    #[test]
    fn default_device_has_empty_axes() {
        let device = DeviceCalibration::default();
        assert!(device.axes.is_empty());
        assert!(device.name.is_empty());
        assert_eq!(device.version, 1);
    }

    #[test]
    fn serialization_roundtrip() -> TestResult {
        let mut device = DeviceCalibration::new("Fanatec CSL DD", 3);
        if let Some(axis) = device.axis(0) {
            *axis = AxisCalibration::new(0, 65535).with_center(32768);
        }
        if let Some(axis) = device.axis(1) {
            *axis = AxisCalibration::new(0, 65535).with_deadzone(500, 65000);
        }

        let json = serde_json::to_string_pretty(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.name, "Fanatec CSL DD");
        assert_eq!(restored.axes.len(), 3);
        assert_eq!(restored.axes[0].center, Some(32768));
        assert_eq!(restored.axes[1].deadzone_min, 500);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PedalCalibrator
// ---------------------------------------------------------------------------

mod pedal_calibrator {
    use super::*;

    #[test]
    fn empty_calibrator_fails() {
        let cal = PedalCalibrator::new();
        let result = cal.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn missing_one_axis_fails() {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        // Missing clutch
        let result = cal.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn full_calibration_succeeds() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(100);
        cal.add_throttle(900);
        cal.add_brake(200);
        cal.add_brake(800);
        cal.add_clutch(50);
        cal.add_clutch(950);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 100);
        assert_eq!(axes[0].max, 900);
        assert_eq!(axes[1].min, 200);
        assert_eq!(axes[1].max, 800);
        assert_eq!(axes[2].min, 50);
        assert_eq!(axes[2].max, 950);
        Ok(())
    }

    #[test]
    fn single_sample_per_axis_succeeds() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(500);
        cal.add_brake(500);
        cal.add_clutch(500);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        // Single sample: min == max
        assert_eq!(axes[0].min, 500);
        assert_eq!(axes[0].max, 500);
        Ok(())
    }

    #[test]
    fn reset_clears_all_samples() {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_brake(0);
        cal.add_clutch(0);
        cal.reset();
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn many_samples_uses_min_max() -> TestResult {
        let mut cal = PedalCalibrator::new();
        for v in [100, 200, 300, 400, 500, 600, 700, 800, 900] {
            cal.add_throttle(v);
            cal.add_brake(v);
            cal.add_clutch(v);
        }

        let axes = cal.calibrate()?;
        assert_eq!(axes[0].min, 100);
        assert_eq!(axes[0].max, 900);
        Ok(())
    }

    #[test]
    fn default_creates_empty() {
        let cal = PedalCalibrator::default();
        assert!(cal.calibrate().is_err());
    }
}

// ---------------------------------------------------------------------------
// create_pedal_calibration convenience function
// ---------------------------------------------------------------------------

mod pedal_convenience {
    use super::*;

    #[test]
    fn create_from_slices() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        Ok(())
    }

    #[test]
    fn empty_slices_fail() {
        let result = create_pedal_calibration(&[], &[0], &[0]);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// JoystickCalibrator
// ---------------------------------------------------------------------------

mod joystick_calibrator {
    use super::*;

    #[test]
    fn empty_calibrator_fails() {
        let cal = JoystickCalibrator::new(0);
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn full_range_calibration() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(32768, 0.5);
        cal.add_sample(65535, 1.0);

        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        assert!(
            axis.center.is_some(),
            "center should be detected from 0.5 sample"
        );
        Ok(())
    }

    #[test]
    fn no_center_when_no_midpoint_sample() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);

        let axis = cal.calibrate()?;
        assert!(
            axis.center.is_none(),
            "center should not be detected without a ~0.5 sample"
        );
        Ok(())
    }

    #[test]
    fn reset_clears_samples() {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        cal.reset();
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn single_sample_calibration() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(500, 0.5);

        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 500);
        assert_eq!(axis.max, 500);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// calibrate_joystick_axis convenience function
// ---------------------------------------------------------------------------

mod joystick_convenience {
    use super::*;

    #[test]
    fn from_samples_slice() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (32768, 0.5), (65535, 1.0)])?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        assert!(axis.center.is_some());
        Ok(())
    }

    #[test]
    fn empty_samples_fail() {
        let result = calibrate_joystick_axis(&[]);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

mod error_types {
    use super::*;

    #[test]
    fn invalid_data_display() {
        let err = CalibrationError::InvalidData;
        assert_eq!(format!("{}", err), "Invalid calibration data");
    }

    #[test]
    fn not_complete_display() {
        let err = CalibrationError::NotComplete;
        assert_eq!(format!("{}", err), "Calibration not complete");
    }

    #[test]
    fn device_error_display() {
        let err = CalibrationError::DeviceError("USB timeout".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("USB timeout"));
    }
}

// ---------------------------------------------------------------------------
// Proptest: fuzz axis calibration
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;

    proptest! {
        #[test]
        fn apply_output_is_between_zero_and_one(
            min in 0u16..=32767,
            raw_offset in 0u16..=32767u16,
        ) {
            // Inputs may be outside the calibrated span; apply() must remain
            // bounded and finite by saturating to the span endpoints.
            let max = min.saturating_add(1).max(min + 1);
            let raw = min.saturating_add(raw_offset);
            let cal = AxisCalibration::new(min, max);
            let result = cal.apply(raw);
            prop_assert!(result >= 0.0, "apply must be >= 0.0, got {}", result);
            prop_assert!(result <= 1.0, "apply must be <= 1.0, got {}", result);
        }

        #[test]
        fn axis_serialization_roundtrip(
            min in 0u16..=65534,
            max_offset in 1u16..=65535u16,
        ) {
            let max = min.saturating_add(max_offset);
            let cal = AxisCalibration::new(min, max);
            let json = serde_json::to_string(&cal).map_err(|e| {
                TestCaseError::fail(format!("serialize failed: {e}"))
            })?;
            let restored: AxisCalibration = serde_json::from_str(&json).map_err(|e| {
                TestCaseError::fail(format!("deserialize failed: {e}"))
            })?;
            prop_assert_eq!(restored.min, min);
            prop_assert_eq!(restored.max, max);
        }

        #[test]
        fn device_calibration_roundtrip(axis_count in 0usize..=10) {
            let device = DeviceCalibration::new("PropTest Device", axis_count);
            let json = serde_json::to_string(&device).map_err(|e| {
                TestCaseError::fail(format!("serialize failed: {e}"))
            })?;
            let restored: DeviceCalibration = serde_json::from_str(&json).map_err(|e| {
                TestCaseError::fail(format!("deserialize failed: {e}"))
            })?;
            prop_assert_eq!(restored.axes.len(), axis_count);
            prop_assert_eq!(restored.name, "PropTest Device");
        }
    }
}
