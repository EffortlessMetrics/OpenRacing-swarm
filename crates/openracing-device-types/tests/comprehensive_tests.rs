//! Comprehensive tests for openracing-device-types.
//!
//! Covers: DeviceInputs construction, button set/get, hat directions,
//! builder methods, edge cases, and property tests.

use openracing_device_types::{DeviceInputs, HatDirection, TelemetryData};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// DeviceInputs basics
// ---------------------------------------------------------------------------

mod device_inputs_construction {
    use super::*;

    #[test]
    fn default_all_zeros() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.tick, 0);
        assert_eq!(inputs.buttons, [0u8; 16]);
        assert_eq!(inputs.hat, 0);
        assert!(inputs.steering.is_none());
        assert!(inputs.throttle.is_none());
        assert!(inputs.brake.is_none());
        assert!(inputs.clutch_left.is_none());
        assert!(inputs.clutch_right.is_none());
        assert!(inputs.clutch_combined.is_none());
        assert!(inputs.clutch_left_button.is_none());
        assert!(inputs.clutch_right_button.is_none());
        assert!(inputs.handbrake.is_none());
        assert_eq!(inputs.rotaries, [0i16; 8]);
    }

    #[test]
    fn new_equals_default() {
        let a = DeviceInputs::new();
        let b = DeviceInputs::default();
        assert_eq!(a.tick, b.tick);
        assert_eq!(a.buttons, b.buttons);
    }
}

// ---------------------------------------------------------------------------
// Button set/get
// ---------------------------------------------------------------------------

mod button_tests {
    use super::*;

    #[test]
    fn set_and_get_all_16_buttons() {
        let mut inputs = DeviceInputs::default();
        for i in 0..16 {
            assert!(!inputs.button(i), "Button {i} should start unset");
            inputs.set_button(i, true);
            assert!(inputs.button(i), "Button {i} should be set");
        }
        // Verify all are set
        for i in 0..16 {
            assert!(inputs.button(i));
        }
    }

    #[test]
    fn unset_button() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(3, true);
        assert!(inputs.button(3));
        inputs.set_button(3, false);
        assert!(!inputs.button(3));
    }

    #[test]
    fn button_independence() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(0, true);
        inputs.set_button(7, true);
        inputs.set_button(0, false);
        assert!(!inputs.button(0));
        assert!(inputs.button(7));
    }

    #[test]
    fn out_of_range_button_returns_false() {
        let inputs = DeviceInputs::default();
        assert!(!inputs.button(128));
        assert!(!inputs.button(200));
        assert!(!inputs.button(usize::MAX));
    }

    #[test]
    fn out_of_range_set_is_noop() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(128, true); // should not panic or change anything
        inputs.set_button(999, true);
        // Nothing in range was set, so the bitfield is untouched.
        assert_eq!(inputs.buttons, [0u8; 16]);
    }

    #[test]
    fn button_byte_boundary() {
        // Button 7 is the last bit of byte 0, button 8 is the first bit of byte 1
        let mut inputs = DeviceInputs::default();
        inputs.set_button(7, true);
        inputs.set_button(8, true);
        assert!(inputs.button(7));
        assert!(inputs.button(8));
        assert!(!inputs.button(6));
        assert!(!inputs.button(9));
    }
}

// ---------------------------------------------------------------------------
// Hat direction
// ---------------------------------------------------------------------------

mod hat_direction_tests {
    use super::*;

    #[test]
    fn all_8_directions() {
        let expected = [
            (0, HatDirection::Up),
            (1, HatDirection::UpRight),
            (2, HatDirection::Right),
            (3, HatDirection::DownRight),
            (4, HatDirection::Down),
            (5, HatDirection::DownLeft),
            (6, HatDirection::Left),
            (7, HatDirection::UpLeft),
        ];
        for (val, dir) in &expected {
            let inputs = DeviceInputs::new().with_hat(*val);
            assert_eq!(inputs.hat_direction(), *dir);
        }
    }

    #[test]
    fn values_above_7_are_neutral() {
        for val in [8u8, 9, 15, 128, 255] {
            let inputs = DeviceInputs::new().with_hat(val);
            assert_eq!(inputs.hat_direction(), HatDirection::Neutral);
        }
    }

    #[test]
    fn default_hat_direction() {
        assert_eq!(HatDirection::default(), HatDirection::Neutral);
    }

    #[test]
    fn hat_direction_is_copy_and_eq() {
        let d = HatDirection::Up;
        let d2 = d;
        assert_eq!(d, d2);
    }
}

// ---------------------------------------------------------------------------
// Builder methods
// ---------------------------------------------------------------------------

mod builder_tests {
    use super::*;

    #[test]
    fn with_steering() {
        let inputs = DeviceInputs::new().with_steering(32768);
        assert_eq!(inputs.steering, Some(32768));
    }

    #[test]
    fn with_pedals() {
        let inputs = DeviceInputs::new().with_pedals(1000, 2000, 500);
        assert_eq!(inputs.throttle, Some(1000));
        assert_eq!(inputs.brake, Some(2000));
        assert_eq!(inputs.clutch_combined, Some(500));
    }

    #[test]
    fn with_handbrake() {
        let inputs = DeviceInputs::new().with_handbrake(4096);
        assert_eq!(inputs.handbrake, Some(4096));
    }

    #[test]
    fn with_rotaries() {
        let rotaries = [10, -20, 30, -40, 50, -60, 70, -80];
        let inputs = DeviceInputs::new().with_rotaries(rotaries);
        assert_eq!(inputs.rotaries, rotaries);
    }

    #[test]
    fn chained_builders() {
        let inputs = DeviceInputs::new()
            .with_steering(16384)
            .with_pedals(100, 200, 300)
            .with_handbrake(0)
            .with_hat(2)
            .with_rotaries([1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(inputs.steering, Some(16384));
        assert_eq!(inputs.throttle, Some(100));
        assert_eq!(inputs.brake, Some(200));
        assert_eq!(inputs.clutch_combined, Some(300));
        assert_eq!(inputs.handbrake, Some(0));
        assert_eq!(inputs.hat, 2);
        assert_eq!(inputs.rotaries[0], 1);
        assert_eq!(inputs.rotaries[7], 8);
    }
}

// ---------------------------------------------------------------------------
// Rotary access
// ---------------------------------------------------------------------------

mod rotary_tests {
    use super::*;

    #[test]
    fn rotary_in_range() {
        let inputs = DeviceInputs::new().with_rotaries([10, 20, 30, 40, 50, 60, 70, 80]);
        for i in 0..8 {
            assert_eq!(inputs.rotary(i), (i as i16 + 1) * 10);
        }
    }

    #[test]
    fn rotary_out_of_range_returns_zero() {
        let inputs = DeviceInputs::new().with_rotaries([100; 8]);
        assert_eq!(inputs.rotary(8), 0);
        assert_eq!(inputs.rotary(100), 0);
    }

    #[test]
    fn rotary_negative_values() {
        let inputs =
            DeviceInputs::new().with_rotaries([-128, -1, 0, 1, 127, i16::MIN, i16::MAX, 0]);
        assert_eq!(inputs.rotary(0), -128);
        assert_eq!(inputs.rotary(1), -1);
        assert_eq!(inputs.rotary(4), 127);
        assert_eq!(inputs.rotary(5), i16::MIN);
        assert_eq!(inputs.rotary(6), i16::MAX);
    }
}

// ---------------------------------------------------------------------------
// Clutch pedal separation
// ---------------------------------------------------------------------------

mod clutch_tests {
    use super::*;

    #[test]
    fn separate_clutch_pedals() {
        let inputs = DeviceInputs {
            clutch_left: Some(100),
            clutch_right: Some(200),
            clutch_combined: Some(150),
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            ..Default::default()
        };
        assert_eq!(inputs.clutch_left, Some(100));
        assert_eq!(inputs.clutch_right, Some(200));
        assert_eq!(inputs.clutch_combined, Some(150));
        assert_eq!(inputs.clutch_left_button, Some(true));
        assert_eq!(inputs.clutch_right_button, Some(false));
    }
}

// ---------------------------------------------------------------------------
// TelemetryData
// ---------------------------------------------------------------------------

mod telemetry_tests {
    use super::*;

    #[test]
    fn telemetry_data_construction() {
        let td = TelemetryData {
            wheel_angle_deg: -90.0,
            wheel_speed_rad_s: 5.0,
            temperature_c: 65,
            fault_flags: 0x03,
            hands_on: false,
        };
        assert!((td.wheel_angle_deg - (-90.0)).abs() < f32::EPSILON);
        assert_eq!(td.temperature_c, 65);
        assert_eq!(td.fault_flags, 0x03);
        assert!(!td.hands_on);
    }

    #[test]
    fn telemetry_data_clone() {
        let td = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 10.0,
            temperature_c: 50,
            fault_flags: 0,
            hands_on: true,
        };
        let cloned = td.clone();
        assert!((cloned.wheel_angle_deg - 45.0).abs() < f32::EPSILON);
        assert!(cloned.hands_on);
    }
}

// ---------------------------------------------------------------------------
// DeviceInputs is Copy + Clone
// ---------------------------------------------------------------------------

mod trait_tests {
    use super::*;

    #[test]
    fn device_inputs_is_copy() {
        let a = DeviceInputs::new().with_steering(100);
        let b = a; // Copy
        assert_eq!(a.steering, b.steering);
    }

    #[test]
    fn device_inputs_is_clone() {
        let a = DeviceInputs::new().with_steering(100);
        #[allow(clippy::clone_on_copy)]
        let b = a.clone();
        assert_eq!(a.steering, b.steering);
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn button_set_get_roundtrip(idx in 0usize..128) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(idx, true);
        prop_assert!(inputs.button(idx));
        inputs.set_button(idx, false);
        prop_assert!(!inputs.button(idx));
    }

    #[test]
    fn hat_direction_valid_or_neutral(hat in any::<u8>()) {
        let inputs = DeviceInputs::new().with_hat(hat);
        let dir = inputs.hat_direction();
        if hat < 8 {
            prop_assert_ne!(dir, HatDirection::Neutral);
        } else {
            prop_assert_eq!(dir, HatDirection::Neutral);
        }
    }

    #[test]
    fn rotary_out_of_bounds_zero(idx in 8usize..1000) {
        let inputs = DeviceInputs::new().with_rotaries([42; 8]);
        prop_assert_eq!(inputs.rotary(idx), 0);
    }

    #[test]
    fn rotary_in_bounds_correct(idx in 0usize..8, values in proptest::array::uniform8(any::<i16>())) {
        let inputs = DeviceInputs::new().with_rotaries(values);
        prop_assert_eq!(inputs.rotary(idx), values[idx]);
    }

    #[test]
    fn button_out_of_range_always_false(idx in 128usize..10000) {
        let mut inputs = DeviceInputs::default();
        // Indices at or beyond the 128-button contract are out of range:
        // setting is a no-op and reading is always false.
        inputs.set_button(idx, true);
        prop_assert!(!inputs.button(idx));
    }
}
