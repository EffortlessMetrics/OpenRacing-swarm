//! Deep tests for openracing-device-types.
//!
//! Covers: DeviceInputs construction and builders, button set/get with
//! boundary and independence checks, hat direction enumeration, rotary
//! access, clutch pedal separation, TelemetryData, Display/Debug
//! formatting, trait verification (Copy, Clone, Default), and
//! property-based invariants.

use openracing_device_types::{DeviceInputs, HatDirection, TelemetryData};
use proptest::prelude::*;

// ── Device type enumeration ────────────────────────────────────────────────

mod hat_direction_completeness {
    use super::*;

    #[test]
    fn all_nine_hat_directions_exist() {
        let _dirs = [
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
    }

    #[test]
    fn hat_values_0_through_7_map_to_unique_directions() {
        let mut seen = Vec::new();
        for val in 0u8..8 {
            let dir = DeviceInputs::new().with_hat(val).hat_direction();
            assert!(
                !seen.contains(&dir),
                "hat value {val} produced duplicate direction {dir:?}"
            );
            seen.push(dir);
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn hat_values_8_and_above_are_all_neutral() {
        for val in [8u8, 9, 15, 16, 100, 128, 254, 255] {
            let dir = DeviceInputs::new().with_hat(val).hat_direction();
            assert_eq!(
                dir,
                HatDirection::Neutral,
                "hat value {val} should be neutral"
            );
        }
    }

    #[test]
    fn hat_direction_default_is_neutral() {
        assert_eq!(HatDirection::default(), HatDirection::Neutral);
    }

    #[test]
    fn hat_direction_mapping_table() {
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
        for (val, dir) in &expected {
            assert_eq!(
                DeviceInputs::new().with_hat(*val).hat_direction(),
                *dir,
                "hat {val} mismatch"
            );
        }
    }
}

// ── Device matching: builder methods ───────────────────────────────────────

mod device_builders {
    use super::*;

    #[test]
    fn with_steering_sets_value() {
        let inputs = DeviceInputs::new().with_steering(32768);
        assert_eq!(inputs.steering, Some(32768));
    }

    #[test]
    fn with_steering_zero() {
        let inputs = DeviceInputs::new().with_steering(0);
        assert_eq!(inputs.steering, Some(0));
    }

    #[test]
    fn with_steering_max() {
        let inputs = DeviceInputs::new().with_steering(u16::MAX);
        assert_eq!(inputs.steering, Some(u16::MAX));
    }

    #[test]
    fn with_pedals_sets_all_three() {
        let inputs = DeviceInputs::new().with_pedals(1000, 2000, 3000);
        assert_eq!(inputs.throttle, Some(1000));
        assert_eq!(inputs.brake, Some(2000));
        assert_eq!(inputs.clutch_combined, Some(3000));
    }

    #[test]
    fn with_handbrake_sets_value() {
        let inputs = DeviceInputs::new().with_handbrake(4096);
        assert_eq!(inputs.handbrake, Some(4096));
    }

    #[test]
    fn with_hat_sets_value() {
        let inputs = DeviceInputs::new().with_hat(5);
        assert_eq!(inputs.hat, 5);
    }

    #[test]
    fn with_rotaries_sets_all() {
        let rotaries = [1, -2, 3, -4, 5, -6, 7, -8];
        let inputs = DeviceInputs::new().with_rotaries(rotaries);
        assert_eq!(inputs.rotaries, rotaries);
    }

    #[test]
    fn chained_builders_all_fields() {
        let buttons = [0xFF; 16];
        let rotaries = [100i16; 8];
        let inputs = DeviceInputs::new()
            .with_buttons(buttons)
            .with_steering(50000)
            .with_pedals(10, 20, 30)
            .with_handbrake(40)
            .with_hat(3)
            .with_rotaries(rotaries);

        assert_eq!(inputs.buttons, buttons);
        assert_eq!(inputs.steering, Some(50000));
        assert_eq!(inputs.throttle, Some(10));
        assert_eq!(inputs.brake, Some(20));
        assert_eq!(inputs.clutch_combined, Some(30));
        assert_eq!(inputs.handbrake, Some(40));
        assert_eq!(inputs.hat, 3);
        assert_eq!(inputs.rotaries, rotaries);
    }

    #[test]
    fn with_buttons_sets_array() {
        let mut buttons = [0u8; 16];
        buttons[0] = 0xFF;
        buttons[15] = 0x01;
        let inputs = DeviceInputs::new().with_buttons(buttons);
        assert_eq!(inputs.buttons[0], 0xFF);
        assert_eq!(inputs.buttons[15], 0x01);
    }
}

// ── Device capabilities: button access ─────────────────────────────────────

mod button_capabilities {
    use super::*;

    #[test]
    fn set_and_get_every_button_independently() {
        for i in 0..16 {
            let mut inputs = DeviceInputs::default();
            inputs.set_button(i, true);
            for j in 0..16 {
                if j == i {
                    assert!(inputs.button(j), "button {j} should be set");
                } else {
                    assert!(
                        !inputs.button(j),
                        "button {j} should not be set when only {i} is set"
                    );
                }
            }
        }
    }

    #[test]
    fn set_all_then_clear_one() {
        let mut inputs = DeviceInputs::new().with_buttons([0xFF; 16]);
        // All 128 buttons (0..128) should be true
        for i in 0..128 {
            assert!(inputs.button(i));
        }
        inputs.set_button(5, false);
        assert!(!inputs.button(5));
        assert!(inputs.button(4));
        assert!(inputs.button(6));
    }

    #[test]
    fn button_out_of_range_returns_false() {
        let inputs = DeviceInputs::new().with_buttons([0xFF; 16]);
        assert!(!inputs.button(128));
        assert!(!inputs.button(200));
        assert!(!inputs.button(usize::MAX));
    }

    #[test]
    fn set_button_out_of_range_is_noop() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(128, true);
        inputs.set_button(999, true);
        // No button in range should be set
        for i in 0..128 {
            assert!(!inputs.button(i));
        }
    }

    #[test]
    fn button_at_byte_boundary() {
        let mut inputs = DeviceInputs::default();
        // Button 7 = last bit of byte 0, button 8 = first bit of byte 1
        inputs.set_button(7, true);
        inputs.set_button(8, true);
        assert!(inputs.button(7));
        assert!(inputs.button(8));
        assert!(!inputs.button(6));
        assert!(!inputs.button(9));
    }

    #[test]
    fn button_toggle_roundtrip() {
        let mut inputs = DeviceInputs::default();
        for i in 0..16 {
            inputs.set_button(i, true);
            assert!(inputs.button(i));
            inputs.set_button(i, false);
            assert!(!inputs.button(i));
        }
    }
}

// ── Rotary access ──────────────────────────────────────────────────────────

mod rotary_access {
    use super::*;

    #[test]
    fn all_8_rotaries_accessible() {
        let rotaries = [10, 20, 30, 40, 50, 60, 70, 80];
        let inputs = DeviceInputs::new().with_rotaries(rotaries);
        for (i, &expected) in rotaries.iter().enumerate() {
            assert_eq!(inputs.rotary(i), expected);
        }
    }

    #[test]
    fn out_of_range_rotary_returns_zero() {
        let inputs = DeviceInputs::new().with_rotaries([100; 8]);
        assert_eq!(inputs.rotary(8), 0);
        assert_eq!(inputs.rotary(100), 0);
        assert_eq!(inputs.rotary(usize::MAX), 0);
    }

    #[test]
    fn rotary_negative_and_extreme_values() {
        let inputs =
            DeviceInputs::new().with_rotaries([i16::MIN, -1, 0, 1, i16::MAX, -32000, 32000, 0]);
        assert_eq!(inputs.rotary(0), i16::MIN);
        assert_eq!(inputs.rotary(1), -1);
        assert_eq!(inputs.rotary(2), 0);
        assert_eq!(inputs.rotary(3), 1);
        assert_eq!(inputs.rotary(4), i16::MAX);
        assert_eq!(inputs.rotary(5), -32000);
        assert_eq!(inputs.rotary(6), 32000);
        assert_eq!(inputs.rotary(7), 0);
    }
}

// ── Clutch pedal separation ────────────────────────────────────────────────

mod clutch_pedals {
    use super::*;

    #[test]
    fn separate_left_right_combined() {
        let inputs = DeviceInputs {
            clutch_left: Some(100),
            clutch_right: Some(200),
            clutch_combined: Some(150),
            ..Default::default()
        };
        assert_eq!(inputs.clutch_left, Some(100));
        assert_eq!(inputs.clutch_right, Some(200));
        assert_eq!(inputs.clutch_combined, Some(150));
    }

    #[test]
    fn clutch_buttons_independent() {
        let inputs = DeviceInputs {
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            ..Default::default()
        };
        assert_eq!(inputs.clutch_left_button, Some(true));
        assert_eq!(inputs.clutch_right_button, Some(false));
    }

    #[test]
    fn all_clutch_fields_none_by_default() {
        let inputs = DeviceInputs::default();
        assert!(inputs.clutch_left.is_none());
        assert!(inputs.clutch_right.is_none());
        assert!(inputs.clutch_combined.is_none());
        assert!(inputs.clutch_left_button.is_none());
        assert!(inputs.clutch_right_button.is_none());
    }
}

// ── Display/Debug formatting ───────────────────────────────────────────────

mod display_debug {
    use super::*;

    #[test]
    fn device_inputs_debug_not_empty() {
        let inputs = DeviceInputs::new()
            .with_steering(1000)
            .with_pedals(10, 20, 30);
        let debug = format!("{inputs:?}");
        assert!(!debug.is_empty());
        assert!(debug.contains("DeviceInputs"));
    }

    #[test]
    fn hat_direction_debug_not_empty() {
        let dirs = [
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
        for dir in &dirs {
            let debug = format!("{dir:?}");
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn telemetry_data_debug_not_empty() {
        let td = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 10.0,
            temperature_c: 50,
            fault_flags: 0,
            hands_on: true,
        };
        let debug = format!("{td:?}");
        assert!(!debug.is_empty());
        assert!(debug.contains("TelemetryData"));
    }

    #[test]
    fn device_inputs_default_debug() {
        let inputs = DeviceInputs::default();
        let debug = format!("{inputs:?}");
        assert!(debug.contains("tick: 0"));
    }
}

// ── TelemetryData ──────────────────────────────────────────────────────────

mod telemetry {
    use super::*;

    #[test]
    fn construction_and_field_access() {
        let td = TelemetryData {
            wheel_angle_deg: -180.0,
            wheel_speed_rad_s: 100.5,
            temperature_c: 80,
            fault_flags: 0xFF,
            hands_on: false,
        };
        assert!((td.wheel_angle_deg - (-180.0)).abs() < f32::EPSILON);
        assert!((td.wheel_speed_rad_s - 100.5).abs() < f32::EPSILON);
        assert_eq!(td.temperature_c, 80);
        assert_eq!(td.fault_flags, 0xFF);
        assert!(!td.hands_on);
    }

    #[test]
    fn clone_produces_equal_values() {
        let td = TelemetryData {
            wheel_angle_deg: 90.0,
            wheel_speed_rad_s: 5.5,
            temperature_c: 42,
            fault_flags: 0x03,
            hands_on: true,
        };
        let cloned = td.clone();
        assert!((cloned.wheel_angle_deg - 90.0).abs() < f32::EPSILON);
        assert!((cloned.wheel_speed_rad_s - 5.5).abs() < f32::EPSILON);
        assert_eq!(cloned.temperature_c, 42);
        assert_eq!(cloned.fault_flags, 0x03);
        assert!(cloned.hands_on);
    }

    #[test]
    fn extreme_temperature_values() {
        let cold = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert_eq!(cold.temperature_c, 0);

        let hot = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 255,
            fault_flags: 0,
            hands_on: false,
        };
        assert_eq!(hot.temperature_c, 255);
    }

    #[test]
    fn all_fault_flags() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0xFF,
            hands_on: false,
        };
        assert_eq!(td.fault_flags, 0xFF);
        // Test individual bits
        for bit in 0..8 {
            assert_ne!(td.fault_flags & (1 << bit), 0);
        }
    }
}

// ── Trait tests: Copy, Clone, Default ──────────────────────────────────────

mod trait_tests {
    use super::*;

    #[test]
    fn device_inputs_is_copy() {
        let a = DeviceInputs::new().with_steering(500).with_hat(2);
        let b = a; // Copy
        assert_eq!(a.steering, b.steering);
        assert_eq!(a.hat, b.hat);
    }

    #[test]
    fn device_inputs_is_clone() {
        let a = DeviceInputs::new().with_pedals(10, 20, 30);
        #[allow(clippy::clone_on_copy)]
        let b = a.clone();
        assert_eq!(a.throttle, b.throttle);
        assert_eq!(a.brake, b.brake);
        assert_eq!(a.clutch_combined, b.clutch_combined);
    }

    #[test]
    fn device_inputs_default_all_none_or_zero() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.tick, 0);
        assert_eq!(inputs.buttons, [0u8; 16]);
        assert_eq!(inputs.hat, 0);
        assert!(inputs.steering.is_none());
        assert!(inputs.throttle.is_none());
        assert!(inputs.brake.is_none());
        assert!(inputs.handbrake.is_none());
        assert_eq!(inputs.rotaries, [0i16; 8]);
    }

    #[test]
    fn hat_direction_is_copy_and_eq() {
        let d = HatDirection::Right;
        let d2 = d;
        assert_eq!(d, d2);
    }

    #[test]
    fn new_and_default_are_equivalent() {
        let a = DeviceInputs::new();
        let b = DeviceInputs::default();
        assert_eq!(a.tick, b.tick);
        assert_eq!(a.buttons, b.buttons);
        assert_eq!(a.hat, b.hat);
        assert_eq!(a.steering, b.steering);
        assert_eq!(a.rotaries, b.rotaries);
    }
}

// ── Property tests ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    #[test]
    fn prop_button_set_get_roundtrip(idx in 0usize..128) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(idx, true);
        prop_assert!(inputs.button(idx));
        inputs.set_button(idx, false);
        prop_assert!(!inputs.button(idx));
    }

    #[test]
    fn prop_button_independence(a in 0usize..128, b in 0usize..128) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(a, true);
        if a != b {
            prop_assert!(!inputs.button(b), "setting button {a} affected button {b}");
        }
    }

    #[test]
    fn prop_hat_direction_valid_or_neutral(hat in any::<u8>()) {
        let dir = DeviceInputs::new().with_hat(hat).hat_direction();
        if hat < 8 {
            prop_assert_ne!(dir, HatDirection::Neutral);
        } else {
            prop_assert_eq!(dir, HatDirection::Neutral);
        }
    }

    #[test]
    fn prop_rotary_in_bounds_correct(idx in 0usize..8, values in proptest::array::uniform8(any::<i16>())) {
        let inputs = DeviceInputs::new().with_rotaries(values);
        prop_assert_eq!(inputs.rotary(idx), values[idx]);
    }

    #[test]
    fn prop_rotary_out_of_bounds_zero(idx in 8usize..10000) {
        let inputs = DeviceInputs::new().with_rotaries([42; 8]);
        prop_assert_eq!(inputs.rotary(idx), 0);
    }

    #[test]
    fn prop_button_out_of_range_always_false(idx in 128usize..10000) {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(idx, true);
        prop_assert!(!inputs.button(idx));
    }

    #[test]
    fn prop_steering_preserved(val in any::<u16>()) {
        let inputs = DeviceInputs::new().with_steering(val);
        prop_assert_eq!(inputs.steering, Some(val));
    }

    #[test]
    fn prop_handbrake_preserved(val in any::<u16>()) {
        let inputs = DeviceInputs::new().with_handbrake(val);
        prop_assert_eq!(inputs.handbrake, Some(val));
    }

    #[test]
    fn prop_pedals_preserved(throttle in any::<u16>(), brake in any::<u16>(), clutch in any::<u16>()) {
        let inputs = DeviceInputs::new().with_pedals(throttle, brake, clutch);
        prop_assert_eq!(inputs.throttle, Some(throttle));
        prop_assert_eq!(inputs.brake, Some(brake));
        prop_assert_eq!(inputs.clutch_combined, Some(clutch));
    }
}

// ── Device identification: tick and field state ───────────────────────────

mod device_identification {
    use super::*;

    #[test]
    fn tick_field_can_be_set() {
        let inputs = DeviceInputs {
            tick: 42,
            ..Default::default()
        };
        assert_eq!(inputs.tick, 42);
    }

    #[test]
    fn tick_wraps_at_u32_max() {
        let inputs = DeviceInputs {
            tick: u32::MAX,
            ..Default::default()
        };
        assert_eq!(inputs.tick, u32::MAX);
        let next_tick = inputs.tick.wrapping_add(1);
        assert_eq!(next_tick, 0);
    }

    #[test]
    fn tick_incrementing() {
        let mut inputs = DeviceInputs::default();
        for expected in 0u32..100 {
            assert_eq!(inputs.tick, expected);
            inputs.tick = inputs.tick.wrapping_add(1);
        }
    }

    #[test]
    fn inputs_with_all_fields_populated() {
        let inputs = DeviceInputs {
            tick: 999,
            buttons: [0xAA; 16],
            hat: 3,
            steering: Some(32768),
            throttle: Some(1000),
            brake: Some(2000),
            clutch_left: Some(300),
            clutch_right: Some(400),
            clutch_combined: Some(350),
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            handbrake: Some(500),
            rotaries: [10, 20, 30, 40, 50, 60, 70, 80],
        };
        assert_eq!(inputs.tick, 999);
        assert_eq!(inputs.hat, 3);
        assert_eq!(inputs.steering, Some(32768));
        assert_eq!(inputs.throttle, Some(1000));
        assert_eq!(inputs.brake, Some(2000));
        assert_eq!(inputs.clutch_left, Some(300));
        assert_eq!(inputs.clutch_right, Some(400));
        assert_eq!(inputs.clutch_combined, Some(350));
        assert_eq!(inputs.clutch_left_button, Some(true));
        assert_eq!(inputs.clutch_right_button, Some(false));
        assert_eq!(inputs.handbrake, Some(500));
        assert_eq!(inputs.rotary(0), 10);
        assert_eq!(inputs.rotary(7), 80);
    }

    #[test]
    fn partial_struct_update_preserves_unset_fields() {
        let base = DeviceInputs::new()
            .with_steering(50000)
            .with_pedals(100, 200, 300);
        let updated = DeviceInputs {
            handbrake: Some(999),
            ..base
        };
        assert_eq!(updated.steering, Some(50000));
        assert_eq!(updated.throttle, Some(100));
        assert_eq!(updated.handbrake, Some(999));
    }
}

// ── Capability flags: fault flags and button bitmask patterns ─────────────

mod capability_flags {
    use super::*;

    #[test]
    fn fault_flags_individual_bits() {
        for bit in 0u8..8 {
            let td = TelemetryData {
                wheel_angle_deg: 0.0,
                wheel_speed_rad_s: 0.0,
                temperature_c: 0,
                fault_flags: 1 << bit,
                hands_on: false,
            };
            assert_ne!(td.fault_flags & (1 << bit), 0, "bit {bit} not set");
            for other in 0u8..8 {
                if other != bit {
                    assert_eq!(
                        td.fault_flags & (1 << other),
                        0,
                        "bit {other} unexpectedly set when only {bit} should be"
                    );
                }
            }
        }
    }

    #[test]
    fn fault_flags_no_faults() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: true,
        };
        assert_eq!(td.fault_flags, 0);
    }

    #[test]
    fn button_bitmask_all_set() {
        let inputs = DeviceInputs::new().with_buttons([0xFF; 16]);
        for i in 0..16 {
            assert!(inputs.button(i), "button {i} should be set");
        }
    }

    #[test]
    fn button_bitmask_all_clear() {
        let inputs = DeviceInputs::new().with_buttons([0x00; 16]);
        for i in 0..16 {
            assert!(!inputs.button(i), "button {i} should be clear");
        }
    }

    #[test]
    fn button_bitmask_alternating() {
        // Set even-indexed buttons
        let mut inputs = DeviceInputs::default();
        for i in (0..16).step_by(2) {
            inputs.set_button(i, true);
        }
        for i in 0..16 {
            if i % 2 == 0 {
                assert!(inputs.button(i), "even button {i} should be set");
            } else {
                assert!(!inputs.button(i), "odd button {i} should be clear");
            }
        }
    }
}

// ── Combined input states ─────────────────────────────────────────────────

mod combined_states {
    use super::*;

    #[test]
    fn simultaneous_buttons_hat_pedals() {
        let mut inputs = DeviceInputs::new()
            .with_steering(32768)
            .with_pedals(u16::MAX, u16::MAX, 0)
            .with_handbrake(0)
            .with_hat(4); // Down

        inputs.set_button(0, true);
        inputs.set_button(15, true);

        assert!(inputs.button(0));
        assert!(inputs.button(15));
        assert_eq!(inputs.hat_direction(), HatDirection::Down);
        assert_eq!(inputs.steering, Some(32768));
        assert_eq!(inputs.throttle, Some(u16::MAX));
        assert_eq!(inputs.brake, Some(u16::MAX));
        assert_eq!(inputs.clutch_combined, Some(0));
        assert_eq!(inputs.handbrake, Some(0));
    }

    #[test]
    fn full_lock_left_with_full_throttle() {
        let inputs = DeviceInputs::new()
            .with_steering(0) // full left
            .with_pedals(u16::MAX, 0, 0);
        assert_eq!(inputs.steering, Some(0));
        assert_eq!(inputs.throttle, Some(u16::MAX));
        assert_eq!(inputs.brake, Some(0));
    }

    #[test]
    fn full_lock_right_with_full_brake() {
        let inputs = DeviceInputs::new()
            .with_steering(u16::MAX)
            .with_pedals(0, u16::MAX, 0);
        assert_eq!(inputs.steering, Some(u16::MAX));
        assert_eq!(inputs.brake, Some(u16::MAX));
    }
}

// ── TelemetryData: extreme values ─────────────────────────────────────────

mod telemetry_extremes {
    use super::*;

    #[test]
    fn extreme_wheel_angle() {
        let td = TelemetryData {
            wheel_angle_deg: -900.0, // 2.5 turns left
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: true,
        };
        assert!((td.wheel_angle_deg - (-900.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn extreme_wheel_speed() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: f32::MAX,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert_eq!(td.wheel_speed_rad_s, f32::MAX);
    }

    #[test]
    fn zero_telemetry() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert!(td.wheel_angle_deg.abs() < f32::EPSILON);
        assert!(td.wheel_speed_rad_s.abs() < f32::EPSILON);
        assert_eq!(td.temperature_c, 0);
        assert_eq!(td.fault_flags, 0);
        assert!(!td.hands_on);
    }

    #[test]
    fn negative_wheel_speed() {
        let td = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: -50.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert!(td.wheel_speed_rad_s < 0.0);
    }

    #[test]
    fn hands_on_toggle() {
        let on = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: true,
        };
        let off = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        };
        assert!(on.hands_on);
        assert!(!off.hands_on);
    }
}

// ── HatDirection: exhaustive equality ─────────────────────────────────────

mod hat_direction_equality {
    use super::*;

    #[test]
    fn all_pairs_are_distinct() {
        let dirs = [
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
        for (i, a) in dirs.iter().enumerate() {
            for (j, b) in dirs.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b, "{a:?} should != {b:?}");
                }
            }
        }
    }

    #[test]
    fn clone_preserves_equality() {
        let dirs = [
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
        for &d in &dirs {
            #[allow(clippy::clone_on_copy)]
            let cloned = d.clone();
            assert_eq!(d, cloned);
        }
    }
}
