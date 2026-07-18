//! Hardening tests for `openracing-device-types`.
//!
//! Covers:
//! - Device type enumeration completeness (HatDirection variants)
//! - Device capability queries (button, hat, rotary, pedal accessors)
//! - Device identification (builder patterns, field access)
//! - Serialization roundtrips (Copy, Clone, Debug)
//! - Unknown / out-of-range device handling
//! - Property-based tests

use openracing_device_types::{DeviceInputs, HatDirection, TelemetryData};
use proptest::prelude::*;

// ===========================================================================
// 1. Device Type Enumeration Completeness
// ===========================================================================

mod hat_direction_completeness {
    use super::*;

    #[test]
    fn all_eight_directions_map_from_consecutive_values() {
        let expected = [
            (0u8, HatDirection::Up),
            (1, HatDirection::UpRight),
            (2, HatDirection::Right),
            (3, HatDirection::DownRight),
            (4, HatDirection::Down),
            (5, HatDirection::DownLeft),
            (6, HatDirection::Left),
            (7, HatDirection::UpLeft),
        ];

        for (val, expected_dir) in &expected {
            let inputs = DeviceInputs::new().with_hat(*val);
            assert_eq!(
                inputs.hat_direction(),
                *expected_dir,
                "Hat value {} should map to {:?}",
                val,
                expected_dir
            );
        }
    }

    #[test]
    fn hat_values_8_through_255_are_neutral() {
        for val in 8u8..=255 {
            let inputs = DeviceInputs::new().with_hat(val);
            assert_eq!(
                inputs.hat_direction(),
                HatDirection::Neutral,
                "Hat value {} should be Neutral",
                val
            );
        }
    }

    #[test]
    fn hat_direction_default_is_neutral() {
        assert_eq!(HatDirection::default(), HatDirection::Neutral);
    }

    #[test]
    fn hat_direction_equality_is_symmetric() {
        assert_eq!(HatDirection::Up, HatDirection::Up);
        assert_ne!(HatDirection::Up, HatDirection::Down);
        assert_ne!(HatDirection::Left, HatDirection::Right);
    }

    #[test]
    fn hat_direction_all_variants_are_distinct() {
        let variants = [
            HatDirection::Up,
            HatDirection::UpRight,
            HatDirection::Right,
            HatDirection::DownRight,
            HatDirection::Down,
            HatDirection::DownLeft,
            HatDirection::Left,
            HatDirection::UpLeft,
            HatDirection::Neutral,
        ];

        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Variants at index {} and {} should differ", i, j);
                }
            }
        }
    }

    #[test]
    fn hat_direction_is_copy() {
        let d = HatDirection::UpLeft;
        let d2 = d;
        assert_eq!(d, d2);
    }

    #[test]
    fn hat_direction_debug_format() {
        let dbg = format!("{:?}", HatDirection::DownRight);
        assert!(
            dbg.contains("DownRight"),
            "Debug output should contain variant name"
        );
    }
}

// ===========================================================================
// 2. Device Capability Queries
// ===========================================================================

mod capability_queries {
    use super::*;

    #[test]
    fn button_get_individual_bits() {
        let mut inputs = DeviceInputs::default();

        // All buttons off initially
        for i in 0..16 {
            assert!(!inputs.button(i), "Button {} should be off", i);
        }

        // Set each button and verify
        for i in 0..16 {
            inputs.set_button(i, true);
            assert!(inputs.button(i), "Button {} should now be on", i);
        }

        // All buttons on
        for i in 0..16 {
            assert!(inputs.button(i), "Button {} should still be on", i);
        }
    }

    #[test]
    fn button_clear_individual_bits() {
        let mut inputs = DeviceInputs::default();

        // Set all
        for i in 0..16 {
            inputs.set_button(i, true);
        }

        // Clear even buttons
        for i in (0..16).step_by(2) {
            inputs.set_button(i, false);
        }

        // Even buttons off, odd on
        for i in 0..16 {
            if i % 2 == 0 {
                assert!(!inputs.button(i), "Even button {} should be off", i);
            } else {
                assert!(inputs.button(i), "Odd button {} should be on", i);
            }
        }
    }

    #[test]
    fn button_byte_boundary_crossing() {
        let mut inputs = DeviceInputs::default();

        // Button 7 = last bit of byte 0
        inputs.set_button(7, true);
        // Button 8 = first bit of byte 1
        inputs.set_button(8, true);

        assert!(inputs.button(7));
        assert!(inputs.button(8));
        assert!(!inputs.button(6));
        assert!(!inputs.button(9));
    }

    #[test]
    fn rotary_in_range_returns_correct_values() {
        let rotaries = [100, -50, 0, i16::MAX, i16::MIN, 1, -1, 32000];
        let inputs = DeviceInputs::new().with_rotaries(rotaries);

        for (i, expected) in rotaries.iter().enumerate() {
            assert_eq!(inputs.rotary(i), *expected, "Rotary {} mismatch", i);
        }
    }

    #[test]
    fn rotary_out_of_range_returns_zero() {
        let inputs = DeviceInputs::new().with_rotaries([42; 8]);
        assert_eq!(inputs.rotary(8), 0);
        assert_eq!(inputs.rotary(100), 0);
        assert_eq!(inputs.rotary(usize::MAX), 0);
    }

    #[test]
    fn steering_pedals_and_clutch_accessors() {
        let inputs = DeviceInputs {
            steering: Some(32768),
            throttle: Some(65535),
            brake: Some(0),
            clutch_left: Some(1000),
            clutch_right: Some(2000),
            clutch_combined: Some(1500),
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            ..Default::default()
        };

        assert_eq!(inputs.steering, Some(32768));
        assert_eq!(inputs.throttle, Some(65535));
        assert_eq!(inputs.brake, Some(0));
        assert_eq!(inputs.clutch_left, Some(1000));
        assert_eq!(inputs.clutch_right, Some(2000));
        assert_eq!(inputs.clutch_combined, Some(1500));
        assert_eq!(inputs.clutch_left_button, Some(true));
        assert_eq!(inputs.clutch_right_button, Some(false));
    }

    #[test]
    fn handbrake_accessor() {
        let inputs = DeviceInputs::new().with_handbrake(4096);
        assert_eq!(inputs.handbrake, Some(4096));
    }

    #[test]
    fn all_optionals_none_by_default() {
        let inputs = DeviceInputs::default();
        assert!(inputs.steering.is_none());
        assert!(inputs.throttle.is_none());
        assert!(inputs.brake.is_none());
        assert!(inputs.clutch_left.is_none());
        assert!(inputs.clutch_right.is_none());
        assert!(inputs.clutch_combined.is_none());
        assert!(inputs.clutch_left_button.is_none());
        assert!(inputs.clutch_right_button.is_none());
        assert!(inputs.handbrake.is_none());
    }
}

// ===========================================================================
// 3. Device Identification (Builder API)
// ===========================================================================

mod builder_identification {
    use super::*;

    #[test]
    fn builder_with_steering() {
        let inputs = DeviceInputs::new().with_steering(16384);
        assert_eq!(inputs.steering, Some(16384));
        // Other fields unchanged
        assert_eq!(inputs.tick, 0);
        assert!(inputs.throttle.is_none());
    }

    #[test]
    fn builder_with_pedals() {
        let inputs = DeviceInputs::new().with_pedals(100, 200, 300);
        assert_eq!(inputs.throttle, Some(100));
        assert_eq!(inputs.brake, Some(200));
        assert_eq!(inputs.clutch_combined, Some(300));
    }

    #[test]
    fn builder_with_buttons() {
        let buttons = [0xFF; 16];
        let inputs = DeviceInputs::new().with_buttons(buttons);
        assert_eq!(inputs.buttons, buttons);
    }

    #[test]
    fn builder_with_hat() {
        let inputs = DeviceInputs::new().with_hat(4);
        assert_eq!(inputs.hat, 4);
        assert_eq!(inputs.hat_direction(), HatDirection::Down);
    }

    #[test]
    fn builder_with_handbrake() {
        let inputs = DeviceInputs::new().with_handbrake(u16::MAX);
        assert_eq!(inputs.handbrake, Some(u16::MAX));
    }

    #[test]
    fn builder_with_rotaries() {
        let rot = [-100, 100, 0, 0, i16::MIN, i16::MAX, -1, 1];
        let inputs = DeviceInputs::new().with_rotaries(rot);
        assert_eq!(inputs.rotaries, rot);
    }

    #[test]
    fn builder_chaining_preserves_all_fields() {
        let inputs = DeviceInputs::new()
            .with_steering(1000)
            .with_pedals(2000, 3000, 4000)
            .with_handbrake(5000)
            .with_hat(6)
            .with_buttons([0x01; 16])
            .with_rotaries([10; 8]);

        assert_eq!(inputs.steering, Some(1000));
        assert_eq!(inputs.throttle, Some(2000));
        assert_eq!(inputs.brake, Some(3000));
        assert_eq!(inputs.clutch_combined, Some(4000));
        assert_eq!(inputs.handbrake, Some(5000));
        assert_eq!(inputs.hat, 6);
        assert_eq!(inputs.hat_direction(), HatDirection::Left);
        assert_eq!(inputs.buttons, [0x01; 16]);
        assert_eq!(inputs.rotaries, [10; 8]);
    }

    #[test]
    fn builder_last_value_wins() {
        let inputs = DeviceInputs::new()
            .with_steering(100)
            .with_steering(200)
            .with_steering(300);
        assert_eq!(inputs.steering, Some(300));
    }
}

// ===========================================================================
// 4. Serialization / Trait Roundtrips
// ===========================================================================

mod serialization_roundtrips {
    use super::*;

    #[test]
    fn device_inputs_is_copy() {
        let a = DeviceInputs::new().with_steering(500).with_hat(3);
        let b = a; // Copy
        assert_eq!(a.steering, b.steering);
        assert_eq!(a.hat, b.hat);
    }

    #[test]
    fn device_inputs_is_clone() {
        let a = DeviceInputs::new().with_pedals(100, 200, 300);
        #[allow(clippy::clone_on_copy)]
        let b = a.clone();
        assert_eq!(a.throttle, b.throttle);
        assert_eq!(a.brake, b.brake);
        assert_eq!(a.clutch_combined, b.clutch_combined);
    }

    #[test]
    fn device_inputs_debug_format() {
        let inputs = DeviceInputs::new().with_steering(12345);
        let dbg = format!("{:?}", inputs);
        assert!(dbg.contains("DeviceInputs"));
        assert!(dbg.contains("12345"));
    }

    #[test]
    fn telemetry_data_is_clone() {
        let td = TelemetryData {
            wheel_angle_deg: 90.0,
            wheel_speed_rad_s: 15.0,
            temperature_c: 70,
            fault_flags: 0xFF,
            hands_on: true,
        };
        let cloned = td.clone();
        assert!((cloned.wheel_angle_deg - 90.0).abs() < f32::EPSILON);
        assert_eq!(cloned.temperature_c, 70);
        assert_eq!(cloned.fault_flags, 0xFF);
        assert!(cloned.hands_on);
    }

    #[test]
    fn telemetry_data_debug_format() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        let dbg = format!("{:?}", td);
        assert!(dbg.contains("TelemetryData"));
    }

    #[test]
    fn hat_direction_debug_all_variants() {
        let variants = [
            HatDirection::Up,
            HatDirection::UpRight,
            HatDirection::Right,
            HatDirection::DownRight,
            HatDirection::Down,
            HatDirection::DownLeft,
            HatDirection::Left,
            HatDirection::UpLeft,
            HatDirection::Neutral,
        ];

        for v in &variants {
            let dbg = format!("{:?}", v);
            assert!(!dbg.is_empty());
        }
    }
}

// ===========================================================================
// 5. Unknown / Out-of-Range Device Handling
// ===========================================================================

mod unknown_handling {
    use super::*;

    #[test]
    fn out_of_range_button_read_returns_false() {
        let inputs = DeviceInputs::default();
        assert!(!inputs.button(128));
        assert!(!inputs.button(200));
        assert!(!inputs.button(255));
        assert!(!inputs.button(usize::MAX));
    }

    #[test]
    fn out_of_range_button_set_is_noop() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(128, true);
        inputs.set_button(999, true);
        inputs.set_button(usize::MAX, true);

        // No buttons should be set
        for i in 0..128 {
            assert!(!inputs.button(i), "Button {} should be unset", i);
        }
    }

    #[test]
    fn out_of_range_rotary_returns_zero() {
        let inputs = DeviceInputs::new().with_rotaries([i16::MAX; 8]);
        assert_eq!(inputs.rotary(8), 0);
        assert_eq!(inputs.rotary(9), 0);
        assert_eq!(inputs.rotary(usize::MAX), 0);
    }

    #[test]
    fn hat_max_u8_is_neutral() {
        let inputs = DeviceInputs::new().with_hat(u8::MAX);
        assert_eq!(inputs.hat_direction(), HatDirection::Neutral);
    }

    #[test]
    fn default_tick_is_zero() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.tick, 0);
    }

    #[test]
    fn tick_can_be_set_directly() {
        let inputs = DeviceInputs {
            tick: u32::MAX,
            ..Default::default()
        };
        assert_eq!(inputs.tick, u32::MAX);
    }
}

// ===========================================================================
// 6. Edge Cases
// ===========================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn u16_max_steering_value() {
        let inputs = DeviceInputs::new().with_steering(u16::MAX);
        assert_eq!(inputs.steering, Some(u16::MAX));
    }

    #[test]
    fn u16_zero_steering_value() {
        let inputs = DeviceInputs::new().with_steering(0);
        assert_eq!(inputs.steering, Some(0));
    }

    #[test]
    fn u16_max_pedal_values() {
        let inputs = DeviceInputs::new().with_pedals(u16::MAX, u16::MAX, u16::MAX);
        assert_eq!(inputs.throttle, Some(u16::MAX));
        assert_eq!(inputs.brake, Some(u16::MAX));
        assert_eq!(inputs.clutch_combined, Some(u16::MAX));
    }

    #[test]
    fn i16_extreme_rotary_values() {
        let inputs = DeviceInputs::new().with_rotaries([
            i16::MIN,
            i16::MAX,
            0,
            -1,
            1,
            i16::MIN,
            i16::MAX,
            0,
        ]);
        assert_eq!(inputs.rotary(0), i16::MIN);
        assert_eq!(inputs.rotary(1), i16::MAX);
        assert_eq!(inputs.rotary(2), 0);
    }

    #[test]
    fn all_buttons_on_via_raw_bytes() {
        let inputs = DeviceInputs::new().with_buttons([0xFF; 16]);
        for i in 0..16 {
            assert!(inputs.button(i), "All buttons should be set");
        }
    }

    #[test]
    fn all_buttons_off_via_raw_bytes() {
        let inputs = DeviceInputs::new().with_buttons([0x00; 16]);
        for i in 0..16 {
            assert!(!inputs.button(i), "All buttons should be unset");
        }
    }

    #[test]
    fn telemetry_extreme_values() {
        let td = TelemetryData {
            wheel_angle_deg: f32::MAX,
            wheel_speed_rad_s: f32::MIN,
            temperature_c: u8::MAX,
            fault_flags: u8::MAX,
            hands_on: true,
        };
        assert_eq!(td.wheel_angle_deg, f32::MAX);
        assert_eq!(td.wheel_speed_rad_s, f32::MIN);
        assert_eq!(td.temperature_c, 255);
        assert_eq!(td.fault_flags, 255);
    }

    #[test]
    fn telemetry_nan_angle() {
        let td = TelemetryData {
            wheel_angle_deg: f32::NAN,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert!(td.wheel_angle_deg.is_nan());
    }

    #[test]
    fn telemetry_infinity_speed() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: f32::INFINITY,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert!(td.wheel_speed_rad_s.is_infinite());
    }
}

// ===========================================================================
// 7. Property-Based Tests
// ===========================================================================

proptest! {
    #[test]
    fn prop_button_set_get_roundtrip(idx in 0usize..128) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(idx, true);
        prop_assert!(inputs.button(idx), "Button {} should be set", idx);
        inputs.set_button(idx, false);
        prop_assert!(!inputs.button(idx), "Button {} should be cleared", idx);
    }

    #[test]
    fn prop_button_set_does_not_affect_others(
        target in 0usize..128,
        other in 0usize..128
    ) {
        if target != other {
            let mut inputs = DeviceInputs::default();
            inputs.set_button(target, true);
            prop_assert!(!inputs.button(other), "Button {} should be unaffected", other);
        }
    }

    #[test]
    fn prop_hat_valid_or_neutral(hat in any::<u8>()) {
        let inputs = DeviceInputs::new().with_hat(hat);
        let dir = inputs.hat_direction();
        if hat < 8 {
            prop_assert_ne!(dir, HatDirection::Neutral);
        } else {
            prop_assert_eq!(dir, HatDirection::Neutral);
        }
    }

    #[test]
    fn prop_rotary_in_bounds(idx in 0usize..8, values in proptest::array::uniform8(any::<i16>())) {
        let inputs = DeviceInputs::new().with_rotaries(values);
        prop_assert_eq!(inputs.rotary(idx), values[idx]);
    }

    #[test]
    fn prop_rotary_out_of_bounds(idx in 8usize..10000) {
        let inputs = DeviceInputs::new().with_rotaries([99; 8]);
        prop_assert_eq!(inputs.rotary(idx), 0);
    }

    #[test]
    fn prop_out_of_range_button_always_false(idx in 128usize..10000) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(idx, true);
        prop_assert!(!inputs.button(idx));
    }

    #[test]
    fn prop_device_inputs_copy_identity(
        steering in any::<Option<u16>>(),
        throttle in any::<Option<u16>>(),
        hat in any::<u8>()
    ) {
        let inputs = DeviceInputs {
            steering,
            throttle,
            hat,
            ..Default::default()
        };
        let copy = inputs;
        prop_assert_eq!(copy.steering, inputs.steering);
        prop_assert_eq!(copy.throttle, inputs.throttle);
        prop_assert_eq!(copy.hat, inputs.hat);
    }

    #[test]
    fn prop_with_steering_sets_value(val in any::<u16>()) {
        let inputs = DeviceInputs::new().with_steering(val);
        prop_assert_eq!(inputs.steering, Some(val));
    }

    #[test]
    fn prop_with_pedals_sets_all_three(t in any::<u16>(), b in any::<u16>(), c in any::<u16>()) {
        let inputs = DeviceInputs::new().with_pedals(t, b, c);
        prop_assert_eq!(inputs.throttle, Some(t));
        prop_assert_eq!(inputs.brake, Some(b));
        prop_assert_eq!(inputs.clutch_combined, Some(c));
    }
}
