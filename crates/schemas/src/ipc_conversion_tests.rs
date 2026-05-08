//! Comprehensive tests for IPC conversion layer
//!
//! This module tests unit conversions, range validation, and round-trip
//! conversions between domain and wire types.

#[cfg(test)]
mod tests {
    // Test helper functions to replace unwrap
    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    use super::super::ipc_conversion::*;
    use crate::domain::*;
    use crate::entities::*;
    use crate::generated::wheel::v1 as proto;
    use crate::telemetry::TelemetryData;

    #[test]
    fn test_torque_unit_conversion() {
        // Test centi-Newton-meters to Newton-meters conversion
        let wire_caps = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500, // 25.00 Nm
            encoder_cpr: 10000,
            min_report_period_us: 1000,
        };

        let domain_caps: DeviceCapabilities = must(wire_caps.try_into());
        assert_eq!(domain_caps.max_torque.value(), 25.0);

        // Test round-trip conversion
        let back_to_wire: proto::DeviceCapabilities = domain_caps.into();
        assert_eq!(back_to_wire.max_torque_cnm, 2500);
    }

    #[test]
    fn test_angle_unit_conversion() {
        // Test millidegrees to degrees conversion
        let wire_telemetry = proto::TelemetryData {
            wheel_angle_mdeg: 45000,  // 45.000 degrees
            wheel_speed_mrad_s: 1571, // ~1.571 rad/s
            temp_c: 50,
            faults: 0,
            hands_on: true,
            sequence: 0,
        };

        let domain_telemetry: TelemetryData = must(wire_telemetry.try_into());
        assert_eq!(domain_telemetry.wheel_angle_deg, 45.0);
        assert!((domain_telemetry.wheel_speed_rad_s - 1.571).abs() < 0.001);

        // Test round-trip conversion
        let back_to_wire: proto::TelemetryData = domain_telemetry.into();
        assert_eq!(back_to_wire.wheel_angle_mdeg, 45000);
        assert_eq!(back_to_wire.wheel_speed_mrad_s, 1571);
    }

    #[test]
    fn test_temperature_range_validation() {
        // Valid temperature
        let valid_temp = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 75, // Valid: 75°C
            faults: 0,
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = valid_temp.try_into();
        assert!(result.is_ok());

        // Invalid temperature (too high)
        let invalid_temp = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 200, // Invalid: > 150°C
            faults: 0,
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = invalid_temp.try_into();
        assert!(result.is_err());

        if let Err(ConversionError::RangeValidation {
            field,
            value,
            min,
            max,
        }) = result
        {
            assert_eq!(field, "temperature_c");
            assert_eq!(value, 200.0);
            assert_eq!(min, 0.0);
            assert_eq!(max, 150.0);
        } else {
            panic!("Expected RangeValidation error");
        }
    }

    #[test]
    fn test_fault_flags_validation() {
        // Valid fault flags (8-bit value)
        let valid_faults = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 50,
            faults: 255, // Valid: max 8-bit value
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = valid_faults.try_into();
        assert!(result.is_ok());
        assert_eq!(must(result).fault_flags, 255);

        // Invalid fault flags (> 8-bit)
        let invalid_faults = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 50,
            faults: 300, // Invalid: > 255
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = invalid_faults.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_gain_validation() {
        // Valid gain values
        let valid_base = proto::BaseSettings {
            ffb_gain: 0.75, // Valid: 0.0-1.0
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.15,
                inertia: 0.08,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![
                    proto::CurvePoint {
                        input: 0.0,
                        output: 0.0,
                    },
                    proto::CurvePoint {
                        input: 1.0,
                        output: 1.0,
                    },
                ],
            }),
        };

        let result: Result<BaseSettings, _> = valid_base.try_into();
        assert!(result.is_ok());

        // Invalid gain (> 1.0)
        let invalid_gain = proto::BaseSettings {
            ffb_gain: 1.5, // Invalid: > 1.0
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![],
            }),
        };

        let result: Result<BaseSettings, _> = invalid_gain.try_into();
        assert!(result.is_err());

        // Invalid gain (< 0.0)
        let negative_gain = proto::BaseSettings {
            ffb_gain: -0.1, // Invalid: < 0.0
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![],
            }),
        };

        let result: Result<BaseSettings, _> = negative_gain.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_degrees_of_rotation_validation() {
        // Valid DOR
        let valid_dor = proto::BaseSettings {
            ffb_gain: 0.75,
            dor_deg: 900, // Valid: 180-2160 degrees
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![
                    proto::CurvePoint {
                        input: 0.0,
                        output: 0.0,
                    },
                    proto::CurvePoint {
                        input: 1.0,
                        output: 1.0,
                    },
                ],
            }),
        };

        let result: Result<BaseSettings, _> = valid_dor.try_into();
        assert!(result.is_ok());

        // Invalid DOR (too low)
        let invalid_dor = proto::BaseSettings {
            ffb_gain: 0.75,
            dor_deg: 90, // Invalid: < 180 degrees
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![],
            }),
        };

        let result: Result<BaseSettings, _> = invalid_dor.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_notch_filter_validation() {
        // Valid notch filter
        let valid_filter = proto::NotchFilter {
            hz: 60.0,       // Valid frequency
            q: 2.0,         // Valid Q factor (0.1-100.0)
            gain_db: -20.0, // Valid gain (-60dB to +20dB)
        };

        let result: Result<NotchFilter, _> = valid_filter.try_into();
        assert!(result.is_ok());
        let filter = must(result);
        assert_eq!(filter.frequency.value(), 60.0);
        assert_eq!(filter.q_factor, 2.0);
        assert_eq!(filter.gain_db, -20.0);

        // Invalid Q factor (too low)
        let invalid_q = proto::NotchFilter {
            hz: 60.0,
            q: 0.05, // Invalid: < 0.1
            gain_db: -20.0,
        };

        let result: Result<NotchFilter, _> = invalid_q.try_into();
        assert!(result.is_err());

        // Invalid gain (too low)
        let invalid_gain = proto::NotchFilter {
            hz: 60.0,
            q: 2.0,
            gain_db: -100.0, // Invalid: < -60dB
        };

        let result: Result<NotchFilter, _> = invalid_gain.try_into();
        assert!(result.is_err());

        // Invalid gain (too high)
        let invalid_gain_high = proto::NotchFilter {
            hz: 60.0,
            q: 2.0,
            gain_db: 50.0, // Invalid: > +20dB
        };

        let result: Result<NotchFilter, _> = invalid_gain_high.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_point_validation() {
        // Valid curve points
        let valid_points = vec![
            proto::CurvePoint {
                input: 0.0,
                output: 0.0,
            },
            proto::CurvePoint {
                input: 0.5,
                output: 0.7,
            },
            proto::CurvePoint {
                input: 1.0,
                output: 1.0,
            },
        ];

        for point in valid_points {
            let result: Result<CurvePoint, _> = point.try_into();
            assert!(result.is_ok());
        }

        // Invalid curve point (input out of range)
        let invalid_input = proto::CurvePoint {
            input: 1.5, // Invalid: > 1.0
            output: 0.5,
        };

        let result: Result<CurvePoint, _> = invalid_input.try_into();
        assert!(result.is_err());

        // Invalid curve point (output out of range)
        let invalid_output = proto::CurvePoint {
            input: 0.5,
            output: -0.1, // Invalid: < 0.0
        };

        let result: Result<CurvePoint, _> = invalid_output.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_reconstruction_level_validation() {
        // Valid reconstruction level
        let valid_filter = proto::FilterConfig {
            reconstruction: 8, // Valid: 0-8
            friction: 0.1,
            damper: 0.1,
            inertia: 0.1,
            notch_filters: vec![],
            slew_rate: 0.8,
            curve_points: vec![
                proto::CurvePoint {
                    input: 0.0,
                    output: 0.0,
                },
                proto::CurvePoint {
                    input: 1.0,
                    output: 1.0,
                },
            ],
        };

        let result: Result<FilterConfig, _> = valid_filter.try_into();
        assert!(result.is_ok());

        // Invalid reconstruction level
        let invalid_filter = proto::FilterConfig {
            reconstruction: 10, // Invalid: > 8
            friction: 0.1,
            damper: 0.1,
            inertia: 0.1,
            notch_filters: vec![],
            slew_rate: 0.8,
            curve_points: vec![],
        };

        let result: Result<FilterConfig, _> = invalid_filter.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_encoder_cpr_validation() {
        // Valid encoder CPR
        let valid_caps = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 10000, // Valid: 1000-100000
            min_report_period_us: 1000,
        };

        let result: Result<DeviceCapabilities, _> = valid_caps.try_into();
        assert!(result.is_ok());

        // Invalid encoder CPR (too low)
        let invalid_cpr_low = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 500, // Invalid: < 1000
            min_report_period_us: 1000,
        };

        let result: Result<DeviceCapabilities, _> = invalid_cpr_low.try_into();
        assert!(result.is_err());

        // Invalid encoder CPR (too high)
        let invalid_cpr_high = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 200000, // Invalid: > 100000
            min_report_period_us: 1000,
        };

        let result: Result<DeviceCapabilities, _> = invalid_cpr_high.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_report_period_validation() {
        // Valid report period (1kHz)
        let valid_caps = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 10000,
            min_report_period_us: 1000, // Valid: 1000us = 1kHz
        };

        let result: Result<DeviceCapabilities, _> = valid_caps.try_into();
        assert!(result.is_ok());

        // Invalid report period (too fast)
        let invalid_period_fast = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 10000,
            min_report_period_us: 500, // Invalid: < 1000us
        };

        let result: Result<DeviceCapabilities, _> = invalid_period_fast.try_into();
        assert!(result.is_err());

        // Invalid report period (too slow)
        let invalid_period_slow = proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 10000,
            min_report_period_us: 200000, // Invalid: > 100000us
        };

        let result: Result<DeviceCapabilities, _> = invalid_period_slow.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_rpm_bands_validation() {
        // Valid RPM bands
        let valid_led = proto::LedConfig {
            rpm_bands: vec![0.7, 0.8, 0.9, 0.95], // Valid: all in [0.0, 1.0]
            pattern: "progressive".to_string(),
            brightness: 0.8,
        };

        let result: Result<LedConfig, _> = valid_led.try_into();
        assert!(result.is_ok());

        // Invalid RPM band (> 1.0)
        let invalid_rpm_high = proto::LedConfig {
            rpm_bands: vec![0.7, 0.8, 1.2], // Invalid: 1.2 > 1.0
            pattern: "progressive".to_string(),
            brightness: 0.8,
        };

        let result: Result<LedConfig, _> = invalid_rpm_high.try_into();
        assert!(result.is_err());

        // Invalid RPM band (< 0.0)
        let invalid_rpm_low = proto::LedConfig {
            rpm_bands: vec![-0.1, 0.8, 0.9], // Invalid: -0.1 < 0.0
            pattern: "progressive".to_string(),
            brightness: 0.8,
        };

        let result: Result<LedConfig, _> = invalid_rpm_low.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_complete_profile_round_trip() {
        // Create a complete domain profile
        let _device_id: DeviceId = must("test-device".parse());
        let profile_id: ProfileId = must("test-profile".parse());

        let base_settings = BaseSettings::new(
            must(Gain::new(0.75)),
            must(Degrees::new_dor(900.0)),
            must(TorqueNm::new(15.0)),
            FilterConfig::default(),
        );

        let profile = Profile::new(
            profile_id,
            ProfileScope::global(),
            base_settings,
            "Test Profile".to_string(),
        );

        // Convert to wire format
        let wire_profile: proto::Profile = profile.clone().into();

        // Convert back to domain format
        let back_to_domain: Profile = must(wire_profile.try_into());

        // Verify key fields are preserved
        assert_eq!(back_to_domain.base_settings.ffb_gain.value(), 0.75);
        assert_eq!(
            back_to_domain.base_settings.degrees_of_rotation.value(),
            900.0
        );
        assert_eq!(back_to_domain.base_settings.torque_cap.value(), 15.0);
    }

    #[test]
    fn test_device_type_conversion() {
        let device_types = vec![
            (DeviceType::Other, 0),
            (DeviceType::WheelBase, 1),
            (DeviceType::SteeringWheel, 2),
            (DeviceType::Pedals, 3),
            (DeviceType::Shifter, 4),
            (DeviceType::Handbrake, 5),
            (DeviceType::ButtonBox, 6),
        ];

        for (domain_type, wire_value) in device_types {
            // Test domain to wire conversion
            let device = Device::new(
                must("test-device".parse()),
                "Test Device".to_string(),
                domain_type,
                DeviceCapabilities::new(
                    true,
                    true,
                    true,
                    false,
                    must(TorqueNm::new(25.0)),
                    10000,
                    1000,
                ),
            );

            let wire_device: proto::DeviceInfo = device.into();
            assert_eq!(wire_device.r#type, wire_value);

            // Test wire to domain conversion
            let wire_device_back = proto::DeviceInfo {
                id: "test-device".to_string(),
                name: "Test Device".to_string(),
                r#type: wire_value,
                capabilities: Some(proto::DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: false,
                    max_torque_cnm: 2500,
                    encoder_cpr: 10000,
                    min_report_period_us: 1000,
                }),
                state: 1,
                vendor_id: 0,
                product_id: 0,
            };

            let domain_device: Device = must(wire_device_back.try_into());
            assert_eq!(domain_device.device_type, domain_type);
        }
    }

    #[test]
    fn test_device_state_conversion() {
        let device_states = vec![
            (DeviceState::Disconnected, 0),
            (DeviceState::Connected, 1),
            (DeviceState::Active, 2),
            (DeviceState::Faulted, 3),
            (DeviceState::SafeMode, 4),
        ];

        for (domain_state, wire_value) in device_states {
            // Test conversion through a complete device
            let mut device = Device::new(
                must("test-device".parse()),
                "Test Device".to_string(),
                DeviceType::WheelBase,
                DeviceCapabilities::new(
                    true,
                    true,
                    true,
                    false,
                    must(TorqueNm::new(25.0)),
                    10000,
                    1000,
                ),
            );
            device.set_state(domain_state);

            let wire_device: proto::DeviceInfo = device.into();
            assert_eq!(wire_device.state, wire_value);
        }
    }

    #[test]
    fn test_precision_preservation() {
        // Test that floating point precision is reasonably preserved
        let test_values = vec![
            (123.456, 123456, 123.456),                       // wheel_angle_deg
            (std::f32::consts::E, 2718, std::f32::consts::E), // wheel_speed_rad_s
            (0.123, 123, 0.123),                              // small values
            (999.999, 999999, 999.999),                       // large values
        ];

        for (original, expected_wire, expected_back) in test_values {
            let telemetry = TelemetryData {
                wheel_angle_deg: original,
                wheel_speed_rad_s: original,
                temperature_c: 50,
                fault_flags: 0,
                hands_on: true,
                timestamp: 1000,
            };

            let wire: proto::TelemetryData = telemetry.into();
            assert_eq!(wire.wheel_angle_mdeg, expected_wire);
            assert_eq!(wire.wheel_speed_mrad_s, expected_wire);

            let back: TelemetryData = must(wire.try_into());
            assert!((back.wheel_angle_deg - expected_back).abs() < 0.001);
            assert!((back.wheel_speed_rad_s - expected_back).abs() < 0.001);
        }
    }
}
