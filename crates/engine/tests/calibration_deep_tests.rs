//! Deep tests for calibration workflows in the engine context.
//!
//! Covers:
//! - Wheel center detection
//! - Rotation range calibration
//! - Force feedback strength calibration
//! - Dead zone configuration
//! - Linearity curve adjustment
//! - Pedal calibration (min/max/curve)
//! - Calibration save/load/apply cycle
//! - Calibration reset to defaults
//! - Calibration with different device types
//! - Invalid calibration data rejection

use openracing_calibration::{
    AxisCalibration, CalibrationError, CalibrationPoint, DeviceCalibration, JoystickCalibrator,
    PedalCalibrator, calibrate_joystick_axis, create_pedal_calibration,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TOL: f32 = 0.02;

fn assert_near(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() < TOL,
        "{label}: expected {expected}, got {actual}"
    );
}

// ===========================================================================
// Wheel center detection
// ===========================================================================

mod wheel_center_tests {
    use super::*;

    #[test]
    fn center_detected_at_exact_midpoint() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (32768, 0.5), (65535, 1.0)])?;
        assert_eq!(axis.center, Some(32768));
        Ok(())
    }

    #[test]
    fn center_detected_near_half_within_tolerance() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (31000, 0.47), (65535, 1.0)])?;
        assert_eq!(axis.center, Some(31000), "0.47 is within ±0.1 of 0.5");
        Ok(())
    }

    #[test]
    fn center_not_detected_when_far_from_half() -> TestResult {
        let axis = calibrate_joystick_axis(&[(0, 0.0), (10000, 0.15), (65535, 1.0)])?;
        assert!(axis.center.is_none(), "0.15 is too far from 0.5");
        Ok(())
    }

    #[test]
    fn center_uses_first_matching_sample() -> TestResult {
        let axis =
            calibrate_joystick_axis(&[(0, 0.0), (30000, 0.48), (33000, 0.50), (65535, 1.0)])?;
        // First sample within ±0.1 of 0.5 is (30000, 0.48)
        assert_eq!(axis.center, Some(30000));
        Ok(())
    }

    #[test]
    fn center_with_asymmetric_range() -> TestResult {
        let axis = calibrate_joystick_axis(&[(5000, 0.0), (35000, 0.5), (60000, 1.0)])?;
        assert_eq!(axis.center, Some(35000));
        assert_eq!(axis.min, 5000);
        assert_eq!(axis.max, 60000);
        Ok(())
    }
}

// ===========================================================================
// Rotation range calibration
// ===========================================================================

mod rotation_range_tests {
    use super::*;

    #[test]
    fn full_range_calibration_sweep() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        for i in 0..=20 {
            let raw = (i as u32 * 65535 / 20) as u16;
            let norm = i as f32 / 20.0;
            cal.add_sample(raw, norm);
        }
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        Ok(())
    }

    #[test]
    fn partial_range_captures_extremes() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(8000, 0.0);
        cal.add_sample(30000, 0.5);
        cal.add_sample(55000, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 8000);
        assert_eq!(axis.max, 55000);
        Ok(())
    }

    #[test]
    fn progressive_refinement_widens_range() -> TestResult {
        let mut cal1 = JoystickCalibrator::new(0);
        cal1.add_sample(2000, 0.0);
        cal1.add_sample(60000, 1.0);
        let first = cal1.calibrate()?;

        let mut cal2 = JoystickCalibrator::new(0);
        cal2.add_sample(1000, 0.0);
        cal2.add_sample(64000, 1.0);
        let second = cal2.calibrate()?;

        assert!(second.min <= first.min, "refined min should be <= original");
        assert!(second.max >= first.max, "refined max should be >= original");
        Ok(())
    }
}

// ===========================================================================
// Force feedback strength calibration
// ===========================================================================

mod ffb_strength_tests {
    use super::*;

    #[test]
    fn ffb_strength_maps_linearly() {
        let axis = AxisCalibration::new(0, 65535);
        // 25% input -> 25% output
        assert_near(axis.apply(16384), 0.25, "25% FFB");
        // 50% input -> 50% output
        assert_near(axis.apply(32768), 0.5, "50% FFB");
        // 75% input -> 75% output
        assert_near(axis.apply(49152), 0.75, "75% FFB");
    }

    #[test]
    fn ffb_strength_clamped_at_boundaries() {
        let axis = AxisCalibration::new(0, 65535);
        // At min maps to 0
        assert_near(axis.apply(0), 0.0, "at min");
        // At max maps to 1
        assert_near(axis.apply(65535), 1.0, "at max");
    }

    #[test]
    fn ffb_strength_with_custom_range() {
        // deadzone bounds share the same raw coordinate space as min/max,
        // so matching them to the axis range is a no-op.
        let axis = AxisCalibration::new(10000, 50000).with_deadzone(10000, 50000);
        let mid = 30000;
        assert_near(axis.apply(mid), 0.5, "mid-range FFB");
    }
}

// ===========================================================================
// Dead zone configuration
// ===========================================================================

mod deadzone_tests {
    use super::*;

    #[test]
    fn narrow_deadzone_filters_noise() {
        let axis = AxisCalibration::new(0, 65535).with_deadzone(500, 65000);
        // Small values below deadzone min map to 0
        assert_near(axis.apply(200), 0.0, "below narrow dz");
        // Above deadzone max maps to 1
        assert_near(axis.apply(65535), 1.0, "above narrow dz");
    }

    #[test]
    fn wide_deadzone_compresses_active_range() {
        let axis = AxisCalibration::new(0, 65535).with_deadzone(20000, 45000);
        // Well below dz -> 0
        assert_near(axis.apply(0), 0.0, "below wide dz");
        // Well above dz -> 1
        assert_near(axis.apply(65535), 1.0, "above wide dz");
        // Midpoint of active range -> ~0.5
        let mid = (20000 + 45000) / 2;
        let result = axis.apply(mid);
        assert!(
            (result - 0.5).abs() < 0.05,
            "mid active range expected ~0.5, got {result}"
        );
    }

    #[test]
    fn deadzone_at_zero_min_passes_all() {
        let axis = AxisCalibration::new(0, 65535).with_deadzone(0, 65535);
        // With deadzone matching full range, output is linear
        assert_near(axis.apply(0), 0.0, "dz min");
        assert_near(axis.apply(32768), 0.5, "dz mid");
        assert_near(axis.apply(65535), 1.0, "dz max");
    }

    #[test]
    fn deadzone_boundaries_are_preserved_in_serde() -> TestResult {
        let axis = AxisCalibration::new(0, 65535).with_deadzone(1500, 63000);
        let json = serde_json::to_string(&axis)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;
        assert_eq!(restored.deadzone_min, 1500);
        assert_eq!(restored.deadzone_max, 63000);
        Ok(())
    }
}

// ===========================================================================
// Linearity curve adjustment
// ===========================================================================

mod linearity_tests {
    use super::*;

    #[test]
    fn linear_output_monotonically_increasing() {
        let axis = AxisCalibration::new(0, 65535);
        let mut prev = axis.apply(0);
        for raw in (1000..=65535).step_by(1000) {
            let current = axis.apply(raw);
            assert!(
                current >= prev,
                "non-monotonic at raw={raw}: prev={prev}, current={current}"
            );
            prev = current;
        }
    }

    #[test]
    fn non_linear_samples_still_produce_valid_calibration() -> TestResult {
        // Sensor with non-linear distribution
        let samples: Vec<(u16, f32)> = vec![
            (0, 0.0),
            (5000, 0.1),
            (15000, 0.25),
            (30000, 0.45),
            (35000, 0.55),
            (50000, 0.75),
            (60000, 0.9),
            (65535, 1.0),
        ];
        let axis = calibrate_joystick_axis(&samples)?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        // Center should be detected from (30000, 0.45) which is within ±0.1 of 0.5
        assert!(axis.center.is_some());
        Ok(())
    }

    #[test]
    fn equal_min_max_returns_half_for_all_inputs() {
        let axis = AxisCalibration::new(500, 500);
        assert_near(axis.apply(0), 0.5, "equal range low input");
        assert_near(axis.apply(500), 0.5, "equal range at value");
        assert_near(axis.apply(65535), 0.5, "equal range high input");
    }
}

// ===========================================================================
// Pedal calibration
// ===========================================================================

mod pedal_tests {
    use super::*;

    #[test]
    fn throttle_full_range() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        Ok(())
    }

    #[test]
    fn brake_with_offset_range() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[5000, 60000], &[0, 65535])?;
        assert_eq!(axes[1].min, 5000);
        assert_eq!(axes[1].max, 60000);
        Ok(())
    }

    #[test]
    fn clutch_partial_range() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[10000, 50000])?;
        assert_eq!(axes[2].min, 10000);
        assert_eq!(axes[2].max, 50000);
        Ok(())
    }

    #[test]
    fn pedal_calibrator_incremental_sampling() -> TestResult {
        let mut cal = PedalCalibrator::new();
        for i in 0..=10 {
            let raw = (i as u32 * 65535 / 10) as u16;
            cal.add_throttle(raw);
            cal.add_brake(raw);
            cal.add_clutch(raw);
        }
        let axes = cal.calibrate()?;
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        assert_eq!(axes.len(), 3);
        Ok(())
    }

    #[test]
    fn missing_brake_samples_fails() {
        let result = create_pedal_calibration(&[0, 65535], &[], &[0, 65535]);
        assert!(result.is_err());
    }

    #[test]
    fn single_sample_per_axis_produces_equal_min_max() -> TestResult {
        let axes = create_pedal_calibration(&[32768], &[32768], &[32768])?;
        assert_eq!(axes[0].min, axes[0].max);
        assert_eq!(axes[1].min, axes[1].max);
        assert_eq!(axes[2].min, axes[2].max);
        Ok(())
    }
}

// ===========================================================================
// Calibration save/load/apply cycle
// ===========================================================================

mod save_load_tests {
    use super::*;

    #[test]
    fn json_round_trip_preserves_all_fields() -> TestResult {
        let mut device = DeviceCalibration::new("Fanatec CSL DD", 3);
        if let Some(steering) = device.axis(0) {
            *steering = AxisCalibration::new(100, 65000)
                .with_center(32500)
                .with_deadzone(500, 64500);
        }
        if let Some(throttle) = device.axis(1) {
            *throttle = AxisCalibration::new(200, 60000);
        }
        if let Some(brake) = device.axis(2) {
            *brake = AxisCalibration::new(1000, 55000).with_deadzone(1500, 54000);
        }

        let json = serde_json::to_string_pretty(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(&*restored.name, "Fanatec CSL DD");
        assert_eq!(restored.axes.len(), 3);
        assert_eq!(restored.version, 1);
        assert_eq!(restored.axes[0].min, 100);
        assert_eq!(restored.axes[0].max, 65000);
        assert_eq!(restored.axes[0].center, Some(32500));
        assert_eq!(restored.axes[0].deadzone_min, 500);
        assert_eq!(restored.axes[1].min, 200);
        assert_eq!(restored.axes[2].deadzone_max, 54000);
        Ok(())
    }

    #[test]
    fn apply_loaded_calibration_to_input() -> TestResult {
        let axis = AxisCalibration::new(0, 65535).with_center(32768);
        let json = serde_json::to_string(&axis)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;

        // Apply the restored calibration
        assert_near(restored.apply(0), 0.0, "restored min");
        assert_near(restored.apply(32768), 0.5, "restored mid");
        assert_near(restored.apply(65535), 1.0, "restored max");
        Ok(())
    }

    #[test]
    fn multi_device_save_load() -> TestResult {
        let devices = vec![
            DeviceCalibration::new("Wheel A", 1),
            DeviceCalibration::new("Pedals B", 3),
            DeviceCalibration::new("Handbrake C", 1),
        ];
        let json = serde_json::to_string(&devices)?;
        let restored: Vec<DeviceCalibration> = serde_json::from_str(&json)?;
        assert_eq!(restored.len(), 3);
        assert_eq!(&*restored[0].name, "Wheel A");
        assert_eq!(&*restored[1].name, "Pedals B");
        assert_eq!(&*restored[2].name, "Handbrake C");
        assert_eq!(restored[1].axes.len(), 3);
        Ok(())
    }

    #[test]
    fn calibration_point_serde_round_trip() -> TestResult {
        let point = CalibrationPoint::new(12345, 0.37);
        let json = serde_json::to_string(&point)?;
        let restored: CalibrationPoint = serde_json::from_str(&json)?;
        assert_eq!(restored.raw, 12345);
        assert_near(restored.normalized, 0.37, "normalized");
        Ok(())
    }
}

// ===========================================================================
// Calibration reset to defaults
// ===========================================================================

mod reset_tests {
    use super::*;

    #[test]
    fn reset_clears_all_samples() {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        cal.reset();
        assert!(
            cal.calibrate().is_err(),
            "after reset, calibrate should fail"
        );
    }

    #[test]
    fn pedal_reset_clears_all_axes() {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);
        cal.reset();
        assert!(
            cal.calibrate().is_err(),
            "after reset, all axes should be empty"
        );
    }

    #[test]
    fn pedal_reset_and_recalibrate_with_new_range() -> TestResult {
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
        cal.add_throttle(5000);
        cal.add_throttle(55000);
        cal.add_brake(5000);
        cal.add_brake(55000);
        cal.add_clutch(5000);
        cal.add_clutch(55000);
        let second = cal.calibrate()?;
        assert_eq!(second[0].min, 5000);
        assert_eq!(second[0].max, 55000);
        Ok(())
    }

    #[test]
    fn default_axis_calibration_is_full_range() {
        let axis = AxisCalibration::default();
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 0xFFFF);
        assert!(axis.center.is_none());
        assert_eq!(axis.deadzone_min, 0);
        assert_eq!(axis.deadzone_max, 0xFFFF);
    }

    #[test]
    fn default_device_calibration_is_empty() {
        let device = DeviceCalibration::default();
        assert!(device.name.is_empty());
        assert!(device.axes.is_empty());
        assert_eq!(device.version, 1);
    }
}

// ===========================================================================
// Calibration with different device types
// ===========================================================================

mod device_type_tests {
    use super::*;

    #[test]
    fn wheel_device_single_axis_with_center() -> TestResult {
        let mut device = DeviceCalibration::new("Racing Wheel", 1);
        if let Some(steering) = device.axis(0) {
            *steering = AxisCalibration::new(0, 65535).with_center(32768);
        }
        assert_eq!(&*device.name, "Racing Wheel");
        assert_eq!(device.axes.len(), 1);
        assert_eq!(device.axes[0].center, Some(32768));
        Ok(())
    }

    #[test]
    fn pedal_device_three_axes_no_center() {
        let mut device = DeviceCalibration::new("Load Cell Pedals", 3);
        if let Some(throttle) = device.axis(0) {
            *throttle = AxisCalibration::new(500, 60000);
        }
        if let Some(brake) = device.axis(1) {
            *brake = AxisCalibration::new(1000, 55000);
        }
        if let Some(clutch) = device.axis(2) {
            *clutch = AxisCalibration::new(200, 58000);
        }
        assert!(device.axes[0].center.is_none());
        assert!(device.axes[1].center.is_none());
        assert!(device.axes[2].center.is_none());
    }

    #[test]
    fn handbrake_device_single_axis() {
        let mut device = DeviceCalibration::new("USB Handbrake", 1);
        if let Some(axis) = device.axis(0) {
            *axis = AxisCalibration::new(0, 4095).with_deadzone(0, 4095); // 12-bit ADC
        }
        assert_eq!(device.axes[0].max, 4095);
        assert_near(device.axes[0].apply(2048), 0.5, "12-bit midpoint");
    }

    #[test]
    fn multi_axis_controller_five_axes() {
        let device = DeviceCalibration::new("Multi-Axis Rig", 5);
        assert_eq!(device.axes.len(), 5);
        // All axes should start with default calibration
        for (i, axis) in device.axes.iter().enumerate() {
            assert_eq!(axis.min, 0, "axis {i} default min");
            assert_eq!(axis.max, 0xFFFF, "axis {i} default max");
        }
    }

    #[test]
    fn out_of_bounds_axis_returns_none() {
        let mut device = DeviceCalibration::new("Wheel", 2);
        assert!(device.axis(0).is_some());
        assert!(device.axis(1).is_some());
        assert!(device.axis(2).is_none());
        assert!(device.axis(100).is_none());
    }
}

// ===========================================================================
// Invalid calibration data rejection
// ===========================================================================

mod invalid_data_tests {
    use super::*;

    #[test]
    fn empty_joystick_samples_fails() {
        let cal = JoystickCalibrator::new(0);
        let result = cal.calibrate();
        assert!(result.is_err());
        assert!(matches!(result, Err(CalibrationError::NotComplete)));
    }

    #[test]
    fn empty_pedal_calibration_fails() {
        let cal = PedalCalibrator::new();
        let result = cal.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn pedal_missing_throttle_fails() {
        let result = create_pedal_calibration(&[], &[0, 65535], &[0, 65535]);
        assert!(result.is_err());
    }

    #[test]
    fn pedal_missing_clutch_fails() {
        let result = create_pedal_calibration(&[0, 65535], &[0, 65535], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn calibration_error_display_messages() {
        let err = CalibrationError::InvalidData;
        assert_eq!(format!("{err}"), "Invalid calibration data");

        let err = CalibrationError::NotComplete;
        assert_eq!(format!("{err}"), "Calibration not complete");

        let err = CalibrationError::DeviceError("timeout".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn joystick_calibrate_convenience_empty_fails() {
        let result = calibrate_joystick_axis(&[]);
        assert!(result.is_err());
    }
}
