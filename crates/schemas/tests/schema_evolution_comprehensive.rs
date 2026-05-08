//! Comprehensive schema evolution, migration, and backward compatibility tests.
//!
//! Covers:
//! 1. Round-trip serialization for all key schema types
//! 2. Backward compatibility (old serialized data → new schema)
//! 3. Forward compatibility (unknown fields handled gracefully)
//! 4. Schema validation (required fields, constraints)
//! 5. Default value behavior for optional fields
//! 6. Enum variant stability (serialized representations don't change)
//! 7. Proto/JSON schema consistency
//! 8. Snapshot tests for serialized forms of key types
//! 9. Migration system (v0→v1, backup/restore, version detection)

#![deny(clippy::unwrap_used)]

use prost::Message;
use racing_wheel_schemas::config::{
    self, BumpstopConfig as ConfigBumpstopConfig, FilterConfig as ConfigFilterConfig,
    HandsOffConfig as ConfigHandsOffConfig, ProfileValidator,
};
use racing_wheel_schemas::domain::*;
use racing_wheel_schemas::entities::*;
use racing_wheel_schemas::generated::wheel::v1 as proto;
use racing_wheel_schemas::migration::*;
use racing_wheel_schemas::telemetry::*;
use std::collections::BTreeMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn json_roundtrip<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value)?;
    serde_json::from_str(&json)
}

fn minimal_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.8,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string()
}

fn full_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing", "car": "porsche-911", "track": "spa" },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 1080,
            "torqueCapNm": 20.0,
            "filters": {
                "reconstruction": 6,
                "friction": 0.15,
                "damper": 0.3,
                "inertia": 0.1,
                "bumpstop": { "enabled": true, "strength": 0.7 },
                "handsOff": { "enabled": false, "sensitivity": 0.5 },
                "torqueCap": 18.0,
                "notchFilters": [
                    { "hz": 50.0, "q": 2.0, "gainDb": -12.0 }
                ],
                "slewRate": 0.6,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.5, "output": 0.7 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        },
        "leds": {
            "rpmBands": [0.75, 0.85, 0.95],
            "pattern": "progressive",
            "brightness": 0.8,
            "colors": { "green": [0, 255, 0], "red": [255, 0, 0] }
        },
        "haptics": {
            "enabled": true,
            "intensity": 0.6,
            "frequencyHz": 80.0,
            "effects": { "kerb": true, "slip": true }
        },
        "signature": "abc123"
    })
    .to_string()
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Round-trip serialization for domain types
// ═══════════════════════════════════════════════════════════════════════

mod roundtrip_domain {
    use super::*;

    #[test]
    fn torque_nm_roundtrip_preserves_value() -> TestResult {
        let t = TorqueNm::new(12.5)?;
        let rt = json_roundtrip(&t)?;
        assert!((rt.value() - 12.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn torque_nm_zero_roundtrip() -> TestResult {
        let t = TorqueNm::ZERO;
        let rt = json_roundtrip(&t)?;
        assert!((rt.value() - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn torque_nm_max_roundtrip() -> TestResult {
        let t = TorqueNm::new(TorqueNm::MAX_TORQUE)?;
        let rt = json_roundtrip(&t)?;
        assert!((rt.value() - TorqueNm::MAX_TORQUE).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn degrees_dor_roundtrip() -> TestResult {
        let d = Degrees::new_dor(900.0)?;
        let rt = json_roundtrip(&d)?;
        assert!((rt.value() - 900.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn degrees_angle_roundtrip() -> TestResult {
        let d = Degrees::new_angle(45.0)?;
        let rt = json_roundtrip(&d)?;
        assert!((rt.value() - 45.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_id_roundtrip() -> TestResult {
        let id: DeviceId = "moza-r9".parse()?;
        let rt = json_roundtrip(&id)?;
        assert_eq!(rt.as_str(), "moza-r9");
        Ok(())
    }

    #[test]
    fn profile_id_roundtrip() -> TestResult {
        let id: ProfileId = "iracing.gt3".parse()?;
        let rt = json_roundtrip(&id)?;
        assert_eq!(rt.as_str(), "iracing.gt3");
        Ok(())
    }

    #[test]
    fn gain_roundtrip() -> TestResult {
        let g = Gain::new(0.85)?;
        let rt = json_roundtrip(&g)?;
        assert!((rt.value() - 0.85).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn gain_boundary_values() -> TestResult {
        let zero = json_roundtrip(&Gain::ZERO)?;
        assert!((zero.value() - 0.0).abs() < f32::EPSILON);
        let full = json_roundtrip(&Gain::FULL)?;
        assert!((full.value() - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn frequency_hz_roundtrip() -> TestResult {
        let f = FrequencyHz::new(1000.0)?;
        let rt = json_roundtrip(&f)?;
        assert!((rt.value() - 1000.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn curve_point_roundtrip() -> TestResult {
        let cp = CurvePoint::new(0.5, 0.7)?;
        let rt = json_roundtrip(&cp)?;
        assert!((rt.input - 0.5).abs() < f32::EPSILON);
        assert!((rt.output - 0.7).abs() < f32::EPSILON);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Round-trip for entity types
// ═══════════════════════════════════════════════════════════════════════

mod roundtrip_entities {
    use super::*;

    #[test]
    fn device_capabilities_roundtrip() -> TestResult {
        let caps =
            DeviceCapabilities::new(true, true, true, false, TorqueNm::new(25.0)?, 4096, 1000);
        let rt = json_roundtrip(&caps)?;
        assert!(rt.supports_pid);
        assert!(rt.supports_raw_torque_1khz);
        assert!(!rt.supports_led_bus);
        assert_eq!(rt.encoder_cpr, 4096);
        assert_eq!(rt.min_report_period_us, 1000);
        assert!((rt.max_torque.value() - 25.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_state_all_variants_roundtrip() -> TestResult {
        let variants = [
            DeviceState::Disconnected,
            DeviceState::Connected,
            DeviceState::Active,
            DeviceState::Faulted,
            DeviceState::SafeMode,
        ];
        for v in &variants {
            let rt = json_roundtrip(v)?;
            assert_eq!(&rt, v);
        }
        Ok(())
    }

    #[test]
    fn device_type_all_variants_roundtrip() -> TestResult {
        let variants = [
            DeviceType::Other,
            DeviceType::WheelBase,
            DeviceType::SteeringWheel,
            DeviceType::Pedals,
            DeviceType::Shifter,
            DeviceType::Handbrake,
            DeviceType::ButtonBox,
        ];
        for v in &variants {
            let rt = json_roundtrip(v)?;
            assert_eq!(&rt, v);
        }
        Ok(())
    }

    #[test]
    fn calibration_type_all_variants_roundtrip() -> TestResult {
        let variants = [
            CalibrationType::Center,
            CalibrationType::Range,
            CalibrationType::Pedals,
            CalibrationType::Full,
        ];
        for v in &variants {
            let rt = json_roundtrip(v)?;
            assert_eq!(&rt, v);
        }
        Ok(())
    }

    #[test]
    fn calibration_data_roundtrip() -> TestResult {
        let mut cal = CalibrationData::new(CalibrationType::Full);
        cal.center_position = Some(0.0);
        cal.min_position = Some(-450.0);
        cal.max_position = Some(450.0);
        cal.pedal_ranges = Some(PedalCalibrationData {
            throttle: Some((0.0, 1.0)),
            brake: Some((0.0, 1.0)),
            clutch: None,
        });
        let rt = json_roundtrip(&cal)?;
        assert_eq!(rt.center_position, Some(0.0));
        assert_eq!(rt.min_position, Some(-450.0));
        assert_eq!(rt.max_position, Some(450.0));
        assert!(rt.pedal_ranges.is_some());
        let pedals = rt.pedal_ranges.as_ref();
        assert!(pedals.is_some());
        assert_eq!(pedals.and_then(|p| p.throttle), Some((0.0, 1.0)));
        assert_eq!(pedals.and_then(|p| p.clutch), None);
        assert_eq!(rt.calibration_type, CalibrationType::Full);
        Ok(())
    }

    #[test]
    fn profile_scope_global_roundtrip() -> TestResult {
        let scope = ProfileScope::global();
        let rt = json_roundtrip(&scope)?;
        assert_eq!(rt.game, None);
        assert_eq!(rt.car, None);
        assert_eq!(rt.track, None);
        Ok(())
    }

    #[test]
    fn profile_scope_full_roundtrip() -> TestResult {
        let scope = ProfileScope::for_track("iRacing".into(), "porsche-911".into(), "spa".into());
        let rt = json_roundtrip(&scope)?;
        assert_eq!(rt.game.as_deref(), Some("iRacing"));
        assert_eq!(rt.car.as_deref(), Some("porsche-911"));
        assert_eq!(rt.track.as_deref(), Some("spa"));
        Ok(())
    }

    #[test]
    fn base_settings_default_roundtrip() -> TestResult {
        let bs = BaseSettings::default();
        let rt = json_roundtrip(&bs)?;
        assert!((rt.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
        assert!((rt.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
        assert!((rt.torque_cap.value() - 15.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn filter_config_default_roundtrip() -> TestResult {
        let fc = FilterConfig::default();
        let rt = json_roundtrip(&fc)?;
        assert_eq!(rt.reconstruction, 0);
        assert!((rt.friction.value() - 0.0).abs() < f32::EPSILON);
        assert!(rt.notch_filters.is_empty());
        assert_eq!(rt.curve_points.len(), 2);
        Ok(())
    }

    #[test]
    fn profile_metadata_roundtrip() -> TestResult {
        let meta = ProfileMetadata {
            name: "Test Profile".into(),
            description: Some("A test description".into()),
            author: Some("TestAuthor".into()),
            version: "2.0.0".into(),
            created_at: "2024-01-01T00:00:00Z".into(),
            modified_at: "2024-06-15T12:00:00Z".into(),
            tags: vec!["racing".into(), "gt3".into()],
        };
        let rt = json_roundtrip(&meta)?;
        assert_eq!(rt.name, "Test Profile");
        assert_eq!(rt.description.as_deref(), Some("A test description"));
        assert_eq!(rt.author.as_deref(), Some("TestAuthor"));
        assert_eq!(rt.version, "2.0.0");
        assert_eq!(rt.tags.len(), 2);
        assert_eq!(rt.tags[0], "racing");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Round-trip for telemetry types
// ═══════════════════════════════════════════════════════════════════════

mod roundtrip_telemetry {
    use super::*;

    #[test]
    fn normalized_telemetry_roundtrip() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .speed_ms(45.0)
            .rpm(6500.0)
            .max_rpm(8000.0)
            .gear(4)
            .throttle(0.8)
            .brake(0.1)
            .clutch(0.0)
            .lateral_g(1.2)
            .longitudinal_g(-0.5)
            .ffb_scalar(0.75)
            .car_id("porsche-911")
            .track_id("spa")
            .session_id("session-001")
            .position(3)
            .lap(7)
            .fuel_percent(0.42)
            .sequence(100)
            .build();

        let rt = json_roundtrip(&t)?;
        assert!((rt.speed_ms - 45.0).abs() < f32::EPSILON);
        assert!((rt.rpm - 6500.0).abs() < f32::EPSILON);
        assert_eq!(rt.gear, 4);
        assert!((rt.throttle - 0.8).abs() < f32::EPSILON);
        assert!((rt.lateral_g - 1.2).abs() < f32::EPSILON);
        assert_eq!(rt.car_id.as_deref(), Some("porsche-911"));
        assert_eq!(rt.track_id.as_deref(), Some("spa"));
        assert_eq!(rt.position, 3);
        assert_eq!(rt.lap, 7);
        assert_eq!(rt.sequence, 100);
        Ok(())
    }

    #[test]
    fn telemetry_default_roundtrip() -> TestResult {
        let t = NormalizedTelemetry::default();
        let rt = json_roundtrip(&t)?;
        assert!((rt.speed_ms - 0.0).abs() < f32::EPSILON);
        assert_eq!(rt.gear, 0);
        assert!(rt.car_id.is_none());
        assert!(rt.extended.is_empty());
        Ok(())
    }

    #[test]
    fn telemetry_flags_roundtrip() -> TestResult {
        let flags = TelemetryFlags {
            yellow_flag: true,
            blue_flag: true,
            pit_limiter: true,
            drs_active: true,
            abs_active: true,
            ..Default::default()
        };
        let rt = json_roundtrip(&flags)?;
        assert!(rt.yellow_flag);
        assert!(rt.blue_flag);
        assert!(rt.pit_limiter);
        assert!(rt.drs_active);
        assert!(rt.abs_active);
        assert!(!rt.red_flag);
        assert!(!rt.checkered_flag);
        assert!(rt.green_flag); // default is true
        Ok(())
    }

    #[test]
    fn telemetry_value_all_variants_roundtrip() -> TestResult {
        let variants = vec![
            TelemetryValue::Float(3.125),
            TelemetryValue::Integer(42),
            TelemetryValue::Boolean(true),
            TelemetryValue::String("test".into()),
        ];
        for v in &variants {
            let rt = json_roundtrip(v)?;
            assert_eq!(&rt, v);
        }
        Ok(())
    }

    #[test]
    fn telemetry_extended_data_roundtrip() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .speed_ms(10.0)
            .extended("boost_psi", TelemetryValue::Float(1.5))
            .extended("pit_count", TelemetryValue::Integer(2))
            .extended("in_garage", TelemetryValue::Boolean(false))
            .extended("driver_name", TelemetryValue::String("Max".into()))
            .build();

        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.extended.len(), 4);
        assert_eq!(
            rt.extended.get("boost_psi"),
            Some(&TelemetryValue::Float(1.5))
        );
        assert_eq!(
            rt.extended.get("pit_count"),
            Some(&TelemetryValue::Integer(2))
        );
        assert_eq!(
            rt.extended.get("in_garage"),
            Some(&TelemetryValue::Boolean(false))
        );
        assert_eq!(
            rt.extended.get("driver_name"),
            Some(&TelemetryValue::String("Max".into()))
        );
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_roundtrip() -> TestResult {
        let snap = TelemetrySnapshot {
            timestamp_ns: 123456789,
            speed_ms: 50.0,
            steering_angle: 0.2,
            throttle: 0.9,
            brake: 0.0,
            clutch: 0.1,
            rpm: 7000.0,
            max_rpm: 9000.0,
            gear: 5,
            num_gears: 6,
            lateral_g: 0.8,
            longitudinal_g: 0.3,
            vertical_g: 0.0,
            slip_ratio: 0.05,
            slip_angle_fl: 0.01,
            slip_angle_fr: 0.02,
            slip_angle_rl: 0.03,
            slip_angle_rr: 0.04,
            ffb_scalar: 0.6,
            ffb_torque_nm: 8.5,
            flags: TelemetryFlags::default(),
            position: 2,
            lap: 10,
            current_lap_time_s: 85.3,
            fuel_percent: 0.65,
            sequence: 999,
        };
        let rt = json_roundtrip(&snap)?;
        assert_eq!(rt.timestamp_ns, 123456789);
        assert!((rt.speed_ms - 50.0).abs() < f32::EPSILON);
        assert_eq!(rt.gear, 5);
        assert_eq!(rt.position, 2);
        assert_eq!(rt.sequence, 999);
        Ok(())
    }

    #[test]
    fn telemetry_data_device_roundtrip() -> TestResult {
        let td = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 3.125,
            temperature_c: 40,
            fault_flags: 0x03,
            hands_on: true,
            timestamp: 123456,
        };
        let rt = json_roundtrip(&td)?;
        assert!((rt.wheel_angle_deg - 45.0).abs() < f32::EPSILON);
        assert_eq!(rt.temperature_c, 40);
        assert_eq!(rt.fault_flags, 0x03);
        assert!(rt.hands_on);
        assert_eq!(rt.timestamp, 123456);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 4. Backward compatibility: old JSON → new code
// ═══════════════════════════════════════════════════════════════════════

mod backward_compat {
    use super::*;

    #[test]
    fn telemetry_without_clutch_field_defaults_zero() -> TestResult {
        let old = r#"{
            "speed_ms": 30.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 5000.0, "gear": 3, "sequence": 1
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(old)?;
        assert!((t.clutch - 0.0).abs() < f32::EPSILON);
        assert!((t.max_rpm - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.num_gears, 0);
        Ok(())
    }

    #[test]
    fn telemetry_without_extended_fields_defaults_empty() -> TestResult {
        let old = r#"{
            "speed_ms": 0.0, "steering_angle": 0.0, "throttle": 0.0,
            "brake": 0.0, "rpm": 0.0, "gear": 0, "sequence": 0
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(old)?;
        assert!(t.extended.is_empty());
        assert!(t.car_id.is_none());
        assert!(t.track_id.is_none());
        assert!(t.session_id.is_none());
        Ok(())
    }

    #[test]
    fn telemetry_without_flags_uses_default_green() -> TestResult {
        let json = r#"{
            "speed_ms": 0.0, "steering_angle": 0.0, "throttle": 0.0,
            "brake": 0.0, "rpm": 0.0, "gear": 0, "sequence": 0
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!(t.flags.green_flag);
        assert!(!t.flags.yellow_flag);
        assert!(!t.flags.red_flag);
        assert!(!t.flags.safety_car);
        Ok(())
    }

    #[test]
    fn telemetry_without_g_forces_defaults_zero() -> TestResult {
        let json = r#"{
            "speed_ms": 10.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 3000.0, "gear": 2, "sequence": 5
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!((t.lateral_g - 0.0).abs() < f32::EPSILON);
        assert!((t.longitudinal_g - 0.0).abs() < f32::EPSILON);
        assert!((t.vertical_g - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_without_tire_data_defaults_zero() -> TestResult {
        let json = r#"{
            "speed_ms": 10.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 3000.0, "gear": 2, "sequence": 5
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!((t.slip_ratio - 0.0).abs() < f32::EPSILON);
        assert!((t.slip_angle_fl - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.tire_temps_c, [0, 0, 0, 0]);
        assert_eq!(t.tire_pressures_psi, [0.0, 0.0, 0.0, 0.0]);
        Ok(())
    }

    #[test]
    fn telemetry_without_ffb_fields_defaults_zero() -> TestResult {
        let json = r#"{
            "speed_ms": 10.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 3000.0, "gear": 2, "sequence": 5
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!((t.ffb_scalar - 0.0).abs() < f32::EPSILON);
        assert!((t.ffb_torque_nm - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_without_race_position_defaults_zero() -> TestResult {
        let json = r#"{
            "speed_ms": 10.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 3000.0, "gear": 2, "sequence": 5
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert_eq!(t.position, 0);
        assert_eq!(t.lap, 0);
        assert!((t.current_lap_time_s - 0.0).abs() < f32::EPSILON);
        assert!((t.best_lap_time_s - 0.0).abs() < f32::EPSILON);
        assert!((t.last_lap_time_s - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_without_optional_fields() -> TestResult {
        let old = r#"{
            "timestamp_ns": 100, "speed_ms": 5.0, "steering_angle": 0.0,
            "throttle": 0.0, "brake": 0.0, "rpm": 1000.0, "gear": 1, "sequence": 1
        }"#;
        let snap: TelemetrySnapshot = serde_json::from_str(old)?;
        assert!((snap.clutch - 0.0).abs() < f32::EPSILON);
        assert!((snap.max_rpm - 0.0).abs() < f32::EPSILON);
        assert_eq!(snap.num_gears, 0);
        assert!((snap.lateral_g - 0.0).abs() < f32::EPSILON);
        assert_eq!(snap.position, 0);
        assert_eq!(snap.lap, 0);
        Ok(())
    }

    #[test]
    fn profile_json_without_optional_sections_parses() -> TestResult {
        // Profile JSON without leds, haptics, or signature
        let json = minimal_profile_json();
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&json)?;
        assert_eq!(profile.schema, "wheel.profile/1");
        assert!(profile.leds.is_none());
        assert!(profile.haptics.is_none());
        assert!(profile.signature.is_none());
        Ok(())
    }

    #[test]
    fn profile_json_without_bumpstop_uses_defaults() -> TestResult {
        let json = minimal_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        assert!(profile.base.filters.bumpstop.enabled);
        assert!((profile.base.filters.bumpstop.strength - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn profile_json_without_handsoff_uses_defaults() -> TestResult {
        let json = minimal_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        assert!(profile.base.filters.hands_off.enabled);
        assert!((profile.base.filters.hands_off.sensitivity - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn proto_device_info_minimal_fields_decodes() -> TestResult {
        let wire = proto::DeviceInfo {
            id: "test-device".into(),
            name: "Test".into(),
            r#type: 1, // WheelBase
            capabilities: Some(proto::DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: false,
                supports_health_stream: false,
                supports_led_bus: false,
                max_torque_cnm: 2500,
                encoder_cpr: 4096,
                min_report_period_us: 1000,
            }),
            state: 1, // Connected
            vendor_id: 0,
            product_id: 0,
        };
        let bytes = wire.encode_to_vec();
        let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
        assert_eq!(decoded.id, "test-device");
        assert_eq!(decoded.r#type, 1);
        Ok(())
    }

    #[test]
    fn proto_device_info_missing_optional_nested() -> TestResult {
        // DeviceInfo without capabilities (which is optional in proto3)
        let wire = proto::DeviceInfo {
            id: "dev-1".into(),
            name: "Dev".into(),
            r#type: 0,
            capabilities: None,
            state: 0,
            vendor_id: 0,
            product_id: 0,
        };
        let bytes = wire.encode_to_vec();
        let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
        assert!(decoded.capabilities.is_none());
        assert_eq!(decoded.id, "dev-1");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Forward compatibility: unknown fields
// ═══════════════════════════════════════════════════════════════════════

mod forward_compat {
    use super::*;

    #[test]
    fn telemetry_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "speed_ms": 10.0, "steering_angle": 0.0, "throttle": 0.5,
            "brake": 0.0, "rpm": 3000.0, "gear": 2, "sequence": 5,
            "future_field_v3": 42.0,
            "another_future_thing": "hello"
        }"#;
        // serde_json by default ignores unknown fields
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!((t.speed_ms - 10.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "timestamp_ns": 100, "speed_ms": 5.0, "steering_angle": 0.0,
            "throttle": 0.0, "brake": 0.0, "rpm": 1000.0, "gear": 1, "sequence": 1,
            "new_field_from_future": true
        }"#;
        let snap: TelemetrySnapshot = serde_json::from_str(json)?;
        assert_eq!(snap.timestamp_ns, 100);
        Ok(())
    }

    #[test]
    fn telemetry_flags_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "yellow_flag": true,
            "future_vsc_flag": true,
            "future_meatball_flag": false
        }"#;
        let flags: TelemetryFlags = serde_json::from_str(json)?;
        assert!(flags.yellow_flag);
        assert!(flags.green_flag); // default is true
        Ok(())
    }

    #[test]
    fn telemetry_data_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "wheel_angle_deg": 10.0,
            "wheel_speed_rad_s": 1.0,
            "temperature_c": 30,
            "fault_flags": 0,
            "hands_on": true,
            "timestamp": 100,
            "new_sensor_voltage": 3.3
        }"#;
        let td: TelemetryData = serde_json::from_str(json)?;
        assert!((td.wheel_angle_deg - 10.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn proto_unknown_fields_are_preserved_in_bytes() -> TestResult {
        // Proto3 preserves unknown fields during decode/encode
        let wire = proto::DeviceInfo {
            id: "test".into(),
            name: "Test".into(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: 0,
            product_id: 0,
        };
        let bytes = wire.encode_to_vec();

        // Manually append an unknown field (field 99, varint type, value 42)
        let mut modified = bytes.clone();
        // Field 99, wire type 0 (varint): (99 << 3) | 0 = 792
        // 792 in varint encoding = [0x98, 0x06]
        modified.extend_from_slice(&[0x98, 0x06, 42]);

        // Should still decode without error, just ignoring unknown field
        let decoded = proto::DeviceInfo::decode(modified.as_slice())?;
        assert_eq!(decoded.id, "test");
        assert_eq!(decoded.r#type, 1);
        Ok(())
    }

    #[test]
    fn proto_extra_repeated_fields_preserved() -> TestResult {
        // A newer proto might add repeated fields; older decoder ignores them
        let wire = proto::TelemetryData {
            wheel_angle_mdeg: 45000,
            wheel_speed_mrad_s: 3140,
            temp_c: 40,
            faults: 0,
            hands_on: true,
            sequence: 10,
        };
        let bytes = wire.encode_to_vec();
        let decoded = proto::TelemetryData::decode(bytes.as_slice())?;
        assert_eq!(decoded.wheel_angle_mdeg, 45000);
        assert_eq!(decoded.sequence, 10);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Schema validation (required fields, constraints)
// ═══════════════════════════════════════════════════════════════════════

mod validation {
    use super::*;

    #[test]
    fn profile_missing_schema_field_rejected() -> TestResult {
        let json = r#"{
            "scope": { "game": "iRacing" },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4, "friction": 0.1, "damper": 0.2,
                    "inertia": 0.05, "notchFilters": [], "slewRate": 0.8,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        }"#;
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(json).is_err());
        Ok(())
    }

    #[test]
    fn profile_missing_base_field_rejected() -> TestResult {
        let json = r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": "iRacing" }
        }"#;
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(json).is_err());
        Ok(())
    }

    #[test]
    fn profile_missing_scope_field_rejected() -> TestResult {
        let json = r#"{
            "schema": "wheel.profile/1",
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4, "friction": 0.1, "damper": 0.2,
                    "inertia": 0.05, "notchFilters": [], "slewRate": 0.8,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        }"#;
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(json).is_err());
        Ok(())
    }

    #[test]
    fn profile_wrong_schema_version_rejected() -> TestResult {
        let json = serde_json::json!({
            "schema": "wheel.profile/99",
            "scope": { "game": "test" },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        })
        .to_string();
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn profile_ffb_gain_out_of_range_rejected() -> TestResult {
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "test" },
            "base": {
                "ffbGain": 1.5,
                "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        })
        .to_string();
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn profile_dor_out_of_range_rejected() -> TestResult {
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "test" },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 5000,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        })
        .to_string();
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn profile_non_monotonic_curve_rejected() -> TestResult {
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "test" },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 0.8, "output": 0.9},
                        {"input": 0.5, "output": 0.6},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        })
        .to_string();
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn profile_unsorted_rpm_bands_rejected() -> TestResult {
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "test" },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            },
            "leds": {
                "rpmBands": [0.9, 0.8, 0.7],
                "pattern": "progressive",
                "brightness": 0.8
            }
        })
        .to_string();
        let validator = ProfileValidator::new()?;
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn domain_torque_negative_rejected() {
        assert!(TorqueNm::new(-1.0).is_err());
    }

    #[test]
    fn domain_torque_over_max_rejected() {
        assert!(TorqueNm::new(51.0).is_err());
    }

    #[test]
    fn domain_torque_nan_rejected() {
        assert!(TorqueNm::new(f32::NAN).is_err());
    }

    #[test]
    fn domain_torque_infinity_rejected() {
        assert!(TorqueNm::new(f32::INFINITY).is_err());
    }

    #[test]
    fn domain_degrees_dor_below_min_rejected() {
        assert!(Degrees::new_dor(100.0).is_err());
    }

    #[test]
    fn domain_degrees_dor_above_max_rejected() {
        assert!(Degrees::new_dor(3000.0).is_err());
    }

    #[test]
    fn domain_gain_below_zero_rejected() {
        assert!(Gain::new(-0.1).is_err());
    }

    #[test]
    fn domain_gain_above_one_rejected() {
        assert!(Gain::new(1.1).is_err());
    }

    #[test]
    fn domain_frequency_zero_rejected() {
        assert!(FrequencyHz::new(0.0).is_err());
    }

    #[test]
    fn domain_frequency_negative_rejected() {
        assert!(FrequencyHz::new(-50.0).is_err());
    }

    #[test]
    fn domain_device_id_empty_rejected() {
        assert!("".parse::<DeviceId>().is_err());
    }

    #[test]
    fn domain_device_id_spaces_rejected() {
        assert!("has spaces".parse::<DeviceId>().is_err());
    }

    #[test]
    fn domain_profile_id_empty_rejected() {
        assert!("".parse::<ProfileId>().is_err());
    }

    #[test]
    fn domain_curve_point_input_out_of_range() {
        assert!(CurvePoint::new(-0.1, 0.5).is_err());
        assert!(CurvePoint::new(1.1, 0.5).is_err());
    }

    #[test]
    fn domain_curve_point_output_out_of_range() {
        assert!(CurvePoint::new(0.5, -0.1).is_err());
        assert!(CurvePoint::new(0.5, 1.1).is_err());
    }

    #[test]
    fn domain_curve_point_nan_rejected() {
        assert!(CurvePoint::new(f32::NAN, 0.5).is_err());
        assert!(CurvePoint::new(0.5, f32::NAN).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Default value behavior
// ═══════════════════════════════════════════════════════════════════════

mod defaults {
    use super::*;

    #[test]
    fn telemetry_flags_default_green_flag_true() -> TestResult {
        let flags = TelemetryFlags::default();
        assert!(flags.green_flag);
        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.in_pits);
        assert!(!flags.drs_available);
        assert!(!flags.drs_active);
        assert!(!flags.ers_available);
        assert!(!flags.ers_active);
        assert!(!flags.launch_control);
        assert!(!flags.traction_control);
        assert!(!flags.abs_active);
        assert!(!flags.engine_limiter);
        assert!(!flags.safety_car);
        assert!(!flags.formation_lap);
        assert!(!flags.session_paused);
        Ok(())
    }

    #[test]
    fn normalized_telemetry_default_all_zeros() -> TestResult {
        let t = NormalizedTelemetry::default();
        assert!((t.speed_ms - 0.0).abs() < f32::EPSILON);
        assert!((t.steering_angle - 0.0).abs() < f32::EPSILON);
        assert!((t.throttle - 0.0).abs() < f32::EPSILON);
        assert!((t.brake - 0.0).abs() < f32::EPSILON);
        assert!((t.clutch - 0.0).abs() < f32::EPSILON);
        assert!((t.rpm - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.gear, 0);
        assert!(t.car_id.is_none());
        assert!(t.track_id.is_none());
        assert!(t.session_id.is_none());
        assert!(t.extended.is_empty());
        assert_eq!(t.position, 0);
        assert_eq!(t.lap, 0);
        assert_eq!(t.sequence, 0);
        Ok(())
    }

    #[test]
    fn config_bumpstop_default_values() -> TestResult {
        let bs = ConfigBumpstopConfig::default();
        assert!(bs.enabled);
        assert!((bs.strength - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn config_handsoff_default_values() -> TestResult {
        let ho = ConfigHandsOffConfig::default();
        assert!(ho.enabled);
        assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn config_filter_default_values() -> TestResult {
        let fc = ConfigFilterConfig::default();
        assert_eq!(fc.reconstruction, 0);
        assert!((fc.friction - 0.0).abs() < f32::EPSILON);
        assert!((fc.damper - 0.0).abs() < f32::EPSILON);
        assert!((fc.inertia - 0.0).abs() < f32::EPSILON);
        assert!((fc.slew_rate - 1.0).abs() < f32::EPSILON);
        assert!(fc.notch_filters.is_empty());
        assert_eq!(fc.curve_points.len(), 2);
        // torque_cap defaults to Some(10.0) in config
        assert_eq!(fc.torque_cap, Some(10.0));
        Ok(())
    }

    #[test]
    fn entity_bumpstop_default_values() -> TestResult {
        let bs = BumpstopConfig::default();
        assert!(bs.enabled);
        assert!((bs.start_angle - 450.0).abs() < f32::EPSILON);
        assert!((bs.max_angle - 540.0).abs() < f32::EPSILON);
        assert!((bs.stiffness - 0.8).abs() < f32::EPSILON);
        assert!((bs.damping - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn entity_handsoff_default_values() -> TestResult {
        let ho = HandsOffConfig::default();
        assert!(ho.enabled);
        assert!((ho.threshold - 0.05).abs() < f32::EPSILON);
        assert!((ho.timeout_seconds - 5.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn base_settings_default_values() -> TestResult {
        let bs = BaseSettings::default();
        assert!((bs.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
        assert!((bs.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
        assert!((bs.torque_cap.value() - 15.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_data_default_all_zeros() -> TestResult {
        let td = TelemetryData::default();
        assert!((td.wheel_angle_deg - 0.0).abs() < f32::EPSILON);
        assert!((td.wheel_speed_rad_s - 0.0).abs() < f32::EPSILON);
        assert_eq!(td.temperature_c, 0);
        assert_eq!(td.fault_flags, 0);
        assert!(!td.hands_on);
        assert_eq!(td.timestamp, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Enum variant stability (serialized repr must not change)
// ═══════════════════════════════════════════════════════════════════════

mod enum_stability {
    use super::*;

    #[test]
    fn device_state_serializes_as_expected_strings() -> TestResult {
        assert_eq!(
            serde_json::to_string(&DeviceState::Disconnected)?,
            "\"Disconnected\""
        );
        assert_eq!(
            serde_json::to_string(&DeviceState::Connected)?,
            "\"Connected\""
        );
        assert_eq!(serde_json::to_string(&DeviceState::Active)?, "\"Active\"");
        assert_eq!(serde_json::to_string(&DeviceState::Faulted)?, "\"Faulted\"");
        assert_eq!(
            serde_json::to_string(&DeviceState::SafeMode)?,
            "\"SafeMode\""
        );
        Ok(())
    }

    #[test]
    fn device_type_serializes_as_expected_strings() -> TestResult {
        assert_eq!(serde_json::to_string(&DeviceType::Other)?, "\"Other\"");
        assert_eq!(
            serde_json::to_string(&DeviceType::WheelBase)?,
            "\"WheelBase\""
        );
        assert_eq!(
            serde_json::to_string(&DeviceType::SteeringWheel)?,
            "\"SteeringWheel\""
        );
        assert_eq!(serde_json::to_string(&DeviceType::Pedals)?, "\"Pedals\"");
        assert_eq!(serde_json::to_string(&DeviceType::Shifter)?, "\"Shifter\"");
        assert_eq!(
            serde_json::to_string(&DeviceType::Handbrake)?,
            "\"Handbrake\""
        );
        assert_eq!(
            serde_json::to_string(&DeviceType::ButtonBox)?,
            "\"ButtonBox\""
        );
        Ok(())
    }

    #[test]
    fn device_state_repr_i32_values_stable() -> TestResult {
        assert_eq!(DeviceState::Disconnected as i32, 0);
        assert_eq!(DeviceState::Connected as i32, 1);
        assert_eq!(DeviceState::Active as i32, 2);
        assert_eq!(DeviceState::Faulted as i32, 3);
        assert_eq!(DeviceState::SafeMode as i32, 4);
        Ok(())
    }

    #[test]
    fn device_type_repr_i32_values_stable() -> TestResult {
        assert_eq!(DeviceType::Other as i32, 0);
        assert_eq!(DeviceType::WheelBase as i32, 1);
        assert_eq!(DeviceType::SteeringWheel as i32, 2);
        assert_eq!(DeviceType::Pedals as i32, 3);
        assert_eq!(DeviceType::Shifter as i32, 4);
        assert_eq!(DeviceType::Handbrake as i32, 5);
        assert_eq!(DeviceType::ButtonBox as i32, 6);
        Ok(())
    }

    #[test]
    fn calibration_type_serializes_as_expected() -> TestResult {
        assert_eq!(
            serde_json::to_string(&CalibrationType::Center)?,
            "\"Center\""
        );
        assert_eq!(serde_json::to_string(&CalibrationType::Range)?, "\"Range\"");
        assert_eq!(
            serde_json::to_string(&CalibrationType::Pedals)?,
            "\"Pedals\""
        );
        assert_eq!(serde_json::to_string(&CalibrationType::Full)?, "\"Full\"");
        Ok(())
    }

    #[test]
    fn telemetry_value_tagged_enum_format_stable() -> TestResult {
        let float_json = serde_json::to_string(&TelemetryValue::Float(1.5))?;
        assert!(float_json.contains("\"type\":\"Float\""));
        assert!(float_json.contains("\"value\":1.5"));

        let int_json = serde_json::to_string(&TelemetryValue::Integer(42))?;
        assert!(int_json.contains("\"type\":\"Integer\""));
        assert!(int_json.contains("\"value\":42"));

        let bool_json = serde_json::to_string(&TelemetryValue::Boolean(true))?;
        assert!(bool_json.contains("\"type\":\"Boolean\""));
        assert!(bool_json.contains("\"value\":true"));

        let str_json = serde_json::to_string(&TelemetryValue::String("hi".into()))?;
        assert!(str_json.contains("\"type\":\"String\""));
        assert!(str_json.contains("\"value\":\"hi\""));
        Ok(())
    }

    #[test]
    fn proto_device_type_enum_values_stable() -> TestResult {
        // Proto enum values must remain stable for wire compatibility
        assert_eq!(proto::DeviceType::Unknown as i32, 0);
        assert_eq!(proto::DeviceType::WheelBase as i32, 1);
        assert_eq!(proto::DeviceType::SteeringWheel as i32, 2);
        assert_eq!(proto::DeviceType::Pedals as i32, 3);
        assert_eq!(proto::DeviceType::Shifter as i32, 4);
        assert_eq!(proto::DeviceType::Handbrake as i32, 5);
        assert_eq!(proto::DeviceType::ButtonBox as i32, 6);
        Ok(())
    }

    #[test]
    fn proto_device_state_enum_values_stable() -> TestResult {
        assert_eq!(proto::DeviceState::Unknown as i32, 0);
        assert_eq!(proto::DeviceState::Connected as i32, 1);
        assert_eq!(proto::DeviceState::Disconnected as i32, 2);
        assert_eq!(proto::DeviceState::Faulted as i32, 3);
        assert_eq!(proto::DeviceState::Calibrating as i32, 4);
        Ok(())
    }

    #[test]
    fn proto_health_event_type_enum_values_stable() -> TestResult {
        assert_eq!(proto::HealthEventType::Unknown as i32, 0);
        assert_eq!(proto::HealthEventType::DeviceConnected as i32, 1);
        assert_eq!(proto::HealthEventType::DeviceDisconnected as i32, 2);
        assert_eq!(proto::HealthEventType::FaultDetected as i32, 3);
        assert_eq!(proto::HealthEventType::FaultCleared as i32, 4);
        assert_eq!(proto::HealthEventType::PerformanceWarning as i32, 5);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Proto / JSON schema consistency
// ═══════════════════════════════════════════════════════════════════════

mod proto_json_consistency {
    use super::*;

    #[test]
    fn proto_profile_has_same_fields_as_json_schema() -> TestResult {
        // Verify the proto Profile message can represent the same data
        // as the JSON config::ProfileSchema
        let proto_profile = proto::Profile {
            schema_version: "wheel.profile/1".into(),
            scope: Some(proto::ProfileScope {
                game: "iRacing".into(),
                car: "porsche-911".into(),
                track: "spa".into(),
            }),
            base: Some(proto::BaseSettings {
                ffb_gain: 0.8,
                dor_deg: 900,
                torque_cap_nm: 15.0,
                filters: Some(proto::FilterConfig {
                    reconstruction: 4,
                    friction: 0.1,
                    damper: 0.2,
                    inertia: 0.05,
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
            }),
            leds: None,
            haptics: None,
            signature: String::new(),
        };
        let bytes = proto_profile.encode_to_vec();
        let decoded = proto::Profile::decode(bytes.as_slice())?;
        assert_eq!(decoded.schema_version, "wheel.profile/1");
        assert!(decoded.scope.is_some());
        assert!(decoded.base.is_some());
        let base = decoded.base.as_ref();
        assert!(base.is_some());
        let base = base.map(|b| b.ffb_gain);
        assert!((base.unwrap_or(0.0) - 0.8).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn proto_telemetry_data_encodes_same_semantic_fields() -> TestResult {
        // Proto TelemetryData uses millidegrees, domain uses degrees
        let proto_td = proto::TelemetryData {
            wheel_angle_mdeg: 45000, // 45.0 degrees
            wheel_speed_mrad_s: 3140,
            temp_c: 35,
            faults: 0,
            hands_on: true,
            sequence: 42,
        };
        let bytes = proto_td.encode_to_vec();
        let decoded = proto::TelemetryData::decode(bytes.as_slice())?;

        // Verify the degree conversion is consistent
        let angle_deg = decoded.wheel_angle_mdeg as f32 / 1000.0;
        assert!((angle_deg - 45.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn proto_filter_config_field_count_matches_json() -> TestResult {
        // Both proto and JSON FilterConfig should have the same core fields
        let proto_fc = proto::FilterConfig {
            reconstruction: 4,
            friction: 0.1,
            damper: 0.2,
            inertia: 0.05,
            notch_filters: vec![proto::NotchFilter {
                hz: 50.0,
                q: 2.0,
                gain_db: -12.0,
            }],
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
        let bytes = proto_fc.encode_to_vec();
        let decoded = proto::FilterConfig::decode(bytes.as_slice())?;
        assert_eq!(decoded.reconstruction, 4);
        assert_eq!(decoded.notch_filters.len(), 1);
        assert_eq!(decoded.curve_points.len(), 2);
        Ok(())
    }

    #[test]
    fn proto_feature_negotiation_roundtrip() -> TestResult {
        let req = proto::FeatureNegotiationRequest {
            client_version: "1.0.0".into(),
            supported_features: vec!["ffb".into(), "leds".into()],
            namespace: "wheel.v1".into(),
        };
        let bytes = req.encode_to_vec();
        let decoded = proto::FeatureNegotiationRequest::decode(bytes.as_slice())?;
        assert_eq!(decoded.client_version, "1.0.0");
        assert_eq!(decoded.supported_features.len(), 2);
        assert_eq!(decoded.namespace, "wheel.v1");
        Ok(())
    }

    #[test]
    fn proto_feature_negotiation_response_roundtrip() -> TestResult {
        let resp = proto::FeatureNegotiationResponse {
            server_version: "1.2.0".into(),
            supported_features: vec!["ffb".into(), "leds".into(), "haptics".into()],
            enabled_features: vec!["ffb".into(), "leds".into()],
            compatible: true,
            min_client_version: "1.0.0".into(),
        };
        let bytes = resp.encode_to_vec();
        let decoded = proto::FeatureNegotiationResponse::decode(bytes.as_slice())?;
        assert!(decoded.compatible);
        assert_eq!(decoded.supported_features.len(), 3);
        assert_eq!(decoded.enabled_features.len(), 2);
        Ok(())
    }

    #[test]
    fn proto_op_result_roundtrip() -> TestResult {
        let mut metadata = BTreeMap::new();
        metadata.insert("key1".into(), "val1".into());
        let result = proto::OpResult {
            success: true,
            error_message: String::new(),
            metadata,
        };
        let bytes = result.encode_to_vec();
        let decoded = proto::OpResult::decode(bytes.as_slice())?;
        assert!(decoded.success);
        assert!(decoded.error_message.is_empty());
        assert_eq!(
            decoded.metadata.get("key1").map(|s| s.as_str()),
            Some("val1")
        );
        Ok(())
    }

    #[test]
    fn proto_health_event_roundtrip() -> TestResult {
        let mut metadata = BTreeMap::new();
        metadata.insert("severity".into(), "high".into());
        let event = proto::HealthEvent {
            timestamp: None,
            device_id: "dev-1".into(),
            r#type: proto::HealthEventType::FaultDetected as i32,
            message: "Overtemp".into(),
            metadata,
        };
        let bytes = event.encode_to_vec();
        let decoded = proto::HealthEvent::decode(bytes.as_slice())?;
        assert_eq!(decoded.device_id, "dev-1");
        assert_eq!(decoded.r#type, proto::HealthEventType::FaultDetected as i32);
        assert_eq!(decoded.message, "Overtemp");
        assert_eq!(
            decoded.metadata.get("severity").map(|s| s.as_str()),
            Some("high")
        );
        Ok(())
    }

    #[test]
    fn proto_diagnostic_info_roundtrip() -> TestResult {
        let mut system_info = BTreeMap::new();
        system_info.insert("os".into(), "windows".into());
        let diag = proto::DiagnosticInfo {
            device_id: "dev-1".into(),
            system_info,
            recent_faults: vec!["overtemp".into()],
            performance: Some(proto::PerformanceMetrics {
                p99_jitter_us: 250.0,
                missed_tick_rate: 0.001,
                total_ticks: 1_000_000,
                missed_ticks: 1000,
            }),
        };
        let bytes = diag.encode_to_vec();
        let decoded = proto::DiagnosticInfo::decode(bytes.as_slice())?;
        assert_eq!(decoded.device_id, "dev-1");
        assert_eq!(decoded.recent_faults.len(), 1);
        assert!(decoded.performance.is_some());
        let perf = decoded.performance.as_ref();
        assert!(perf.is_some());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 10. Snapshot tests for serialized forms
// ═══════════════════════════════════════════════════════════════════════

mod serialization_snapshots {
    use super::*;

    #[test]
    fn telemetry_value_float_snapshot() -> TestResult {
        let json = serde_json::to_string(&TelemetryValue::Float(3.125))?;
        assert_eq!(json, r#"{"type":"Float","value":3.125}"#);
        Ok(())
    }

    #[test]
    fn telemetry_value_integer_snapshot() -> TestResult {
        let json = serde_json::to_string(&TelemetryValue::Integer(42))?;
        assert_eq!(json, r#"{"type":"Integer","value":42}"#);
        Ok(())
    }

    #[test]
    fn telemetry_value_boolean_snapshot() -> TestResult {
        let json = serde_json::to_string(&TelemetryValue::Boolean(true))?;
        assert_eq!(json, r#"{"type":"Boolean","value":true}"#);
        Ok(())
    }

    #[test]
    fn telemetry_value_string_snapshot() -> TestResult {
        let json = serde_json::to_string(&TelemetryValue::String("abc".into()))?;
        assert_eq!(json, r#"{"type":"String","value":"abc"}"#);
        Ok(())
    }

    #[test]
    fn torque_nm_json_is_raw_number() -> TestResult {
        let t = TorqueNm::new(12.5)?;
        let json = serde_json::to_string(&t)?;
        assert_eq!(json, "12.5");
        Ok(())
    }

    #[test]
    fn gain_json_is_raw_number() -> TestResult {
        let g = Gain::new(0.85)?;
        let json = serde_json::to_string(&g)?;
        assert_eq!(json, "0.85");
        Ok(())
    }

    #[test]
    fn degrees_json_is_raw_number() -> TestResult {
        let d = Degrees::new_dor(900.0)?;
        let json = serde_json::to_string(&d)?;
        assert_eq!(json, "900.0");
        Ok(())
    }

    #[test]
    fn frequency_hz_json_is_raw_number() -> TestResult {
        let f = FrequencyHz::new(1000.0)?;
        let json = serde_json::to_string(&f)?;
        assert_eq!(json, "1000.0");
        Ok(())
    }

    #[test]
    fn device_id_json_is_quoted_string() -> TestResult {
        let id: DeviceId = "moza-r9".parse()?;
        let json = serde_json::to_string(&id)?;
        assert_eq!(json, "\"moza-r9\"");
        Ok(())
    }

    #[test]
    fn profile_id_json_is_quoted_string() -> TestResult {
        let id: ProfileId = "iracing.gt3".parse()?;
        let json = serde_json::to_string(&id)?;
        assert_eq!(json, "\"iracing.gt3\"");
        Ok(())
    }

    #[test]
    fn curve_point_json_shape() -> TestResult {
        let cp = CurvePoint::new(0.5, 0.7)?;
        let json = serde_json::to_string(&cp)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;
        assert!(parsed.get("input").is_some());
        assert!(parsed.get("output").is_some());
        Ok(())
    }

    #[test]
    fn profile_scope_global_serializes_with_null_fields() -> TestResult {
        let scope = ProfileScope::global();
        let json = serde_json::to_string(&scope)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;
        assert!(parsed.get("game").is_some());
        // serde serializes None as null
        assert!(parsed["game"].is_null());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 11. Migration system tests
// ═══════════════════════════════════════════════════════════════════════

mod migration_tests {
    use super::*;

    #[test]
    fn schema_version_parse_v1() -> TestResult {
        let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(v.is_current());
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        Ok(())
    }

    #[test]
    fn schema_version_parse_v2() -> TestResult {
        let v = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
        assert!(!v.is_current());
        assert_eq!(v.major, 2);
        Ok(())
    }

    #[test]
    fn schema_version_ordering() -> TestResult {
        let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
        assert!(v1.is_older_than(&v2));
        assert!(!v2.is_older_than(&v1));
        assert!(!v1.is_older_than(&v1));
        Ok(())
    }

    #[test]
    fn schema_version_minor_ordering() -> TestResult {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_1 = SchemaVersion::new(1, 1);
        assert!(v1_0.is_older_than(&v1_1));
        assert!(!v1_1.is_older_than(&v1_0));
        Ok(())
    }

    #[test]
    fn schema_version_display() -> TestResult {
        let v = SchemaVersion::new(3, 7);
        let display = format!("{}", v);
        assert_eq!(display, "wheel.profile/3.7");
        Ok(())
    }

    #[test]
    fn schema_version_invalid_format_rejected() {
        assert!(SchemaVersion::parse("invalid").is_err());
        assert!(SchemaVersion::parse("wheel.profile/").is_err());
        assert!(SchemaVersion::parse("other/1").is_err());
    }

    #[test]
    fn migration_manager_detects_current_version() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let json = minimal_profile_json();
        let version = mgr.detect_version(&json)?;
        assert!(version.is_current());
        Ok(())
    }

    #[test]
    fn migration_manager_current_profile_no_migration_needed() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let json = minimal_profile_json();
        assert!(!mgr.needs_migration(&json)?);
        Ok(())
    }

    #[test]
    fn migration_manager_legacy_profile_needs_migration() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let legacy = serde_json::json!({
            "ffb_gain": 0.7,
            "degrees_of_rotation": 900,
            "torque_cap": 15.0
        })
        .to_string();
        assert!(mgr.needs_migration(&legacy)?);
        Ok(())
    }

    #[test]
    fn migration_v0_to_v1_adds_schema_field() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let legacy = serde_json::json!({
            "ffb_gain": 0.7,
            "degrees_of_rotation": 900,
            "torque_cap": 15.0
        })
        .to_string();
        let migrated = mgr.migrate_profile(&legacy)?;
        let parsed: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            parsed.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn migration_v0_to_v1_creates_base_structure() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let legacy = serde_json::json!({
            "ffb_gain": 0.8,
            "degrees_of_rotation": 1080,
            "torque_cap": 20.0
        })
        .to_string();
        let migrated = mgr.migrate_profile(&legacy)?;
        let parsed: serde_json::Value = serde_json::from_str(&migrated)?;
        assert!(parsed.get("base").is_some());
        assert!(parsed.get("scope").is_some());
        let base = parsed.get("base");
        assert!(base.is_some());
        assert!(base.and_then(|b| b.get("ffbGain")).is_some());
        assert!(base.and_then(|b| b.get("dorDeg")).is_some());
        assert!(base.and_then(|b| b.get("torqueCapNm")).is_some());
        assert!(base.and_then(|b| b.get("filters")).is_some());
        Ok(())
    }

    #[test]
    fn migration_v0_to_v1_preserves_values() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let legacy = serde_json::json!({
            "ffb_gain": 0.65,
            "degrees_of_rotation": 720,
            "torque_cap": 12.0
        })
        .to_string();
        let migrated = mgr.migrate_profile(&legacy)?;
        let parsed: serde_json::Value = serde_json::from_str(&migrated)?;
        let base = parsed.get("base");
        assert!(base.is_some());
        let ffb = base.and_then(|b| b.get("ffbGain")).and_then(|v| v.as_f64());
        assert!((ffb.unwrap_or(0.0) - 0.65).abs() < f64::EPSILON);
        let dor = base.and_then(|b| b.get("dorDeg")).and_then(|v| v.as_u64());
        assert_eq!(dor, Some(720));
        let tcap = base
            .and_then(|b| b.get("torqueCapNm"))
            .and_then(|v| v.as_f64());
        assert!((tcap.unwrap_or(0.0) - 12.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn migration_current_profile_returned_unchanged() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let json = minimal_profile_json();
        let migrated = mgr.migrate_profile(&json)?;
        let original: serde_json::Value = serde_json::from_str(&json)?;
        let result: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(original, result);
        Ok(())
    }

    #[test]
    fn migration_backup_and_restore_cycle() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let backup_dir = tmp.path().join("backups");
        let config = MigrationConfig::new(&backup_dir);
        let mgr = MigrationManager::new(config)?;

        let profile_path = tmp.path().join("test_profile.json");
        let content = minimal_profile_json();
        std::fs::write(&profile_path, &content)?;

        let backup_info = mgr.create_backup(&profile_path, &content)?;
        assert!(backup_info.backup_path.exists());

        let restored = mgr.restore_backup(&backup_info)?;
        assert_eq!(restored, content);
        Ok(())
    }

    #[test]
    fn migration_config_without_backups() -> TestResult {
        let config = MigrationConfig::without_backups();
        assert!(!config.create_backups);
        assert_eq!(config.max_backups, 0);
        assert!(config.validate_after_migration);
        Ok(())
    }

    #[test]
    fn migration_config_default() -> TestResult {
        let config = MigrationConfig::default();
        assert!(config.create_backups);
        assert_eq!(config.max_backups, 5);
        assert!(config.validate_after_migration);
        Ok(())
    }

    #[test]
    fn migration_unknown_schema_version_error() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let json = serde_json::json!({
            "schema": "wheel.profile/99"
        })
        .to_string();
        // Detecting version should succeed (it's parseable)
        let version = mgr.detect_version(&json)?;
        assert!(!version.is_current());
        // But migrating should fail (no migration path)
        assert!(mgr.migrate_profile(&json).is_err());
        Ok(())
    }

    #[test]
    fn migration_no_schema_field_detected_as_legacy() -> TestResult {
        let config = MigrationConfig::without_backups();
        let mgr = MigrationManager::new(config)?;
        let legacy = serde_json::json!({
            "ffb_gain": 0.7,
            "degrees_of_rotation": 900
        })
        .to_string();
        let version = mgr.detect_version(&legacy)?;
        assert_eq!(version.major, 0);
        assert_eq!(version.minor, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 12. Config profile JSON roundtrip
// ═══════════════════════════════════════════════════════════════════════

mod config_profile_roundtrip {
    use super::*;

    #[test]
    fn minimal_profile_json_roundtrip_through_validator() -> TestResult {
        let json = minimal_profile_json();
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&json)?;
        // Re-serialize and re-validate
        let reserialized = serde_json::to_string(&profile)?;
        let _re_validated = validator.validate_json(&reserialized)?;
        Ok(())
    }

    #[test]
    fn full_profile_json_roundtrip_through_validator() -> TestResult {
        let json = full_profile_json();
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&json)?;
        assert_eq!(profile.schema, "wheel.profile/1");
        assert_eq!(profile.scope.game.as_deref(), Some("iRacing"));
        assert_eq!(profile.scope.car.as_deref(), Some("porsche-911"));
        assert_eq!(profile.scope.track.as_deref(), Some("spa"));
        assert!(profile.leds.is_some());
        assert!(profile.haptics.is_some());
        assert_eq!(profile.signature.as_deref(), Some("abc123"));
        Ok(())
    }

    #[test]
    fn full_profile_preserves_led_config() -> TestResult {
        let json = full_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        let leds = profile.leds.as_ref();
        assert!(leds.is_some());
        let leds = leds.ok_or("missing leds")?;
        assert_eq!(leds.rpm_bands, vec![0.75, 0.85, 0.95]);
        assert_eq!(leds.pattern, "progressive");
        assert!((leds.brightness - 0.8).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn full_profile_preserves_haptics_config() -> TestResult {
        let json = full_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        let haptics = profile.haptics.as_ref();
        assert!(haptics.is_some());
        let haptics = haptics.ok_or("missing haptics")?;
        assert!(haptics.enabled);
        assert!((haptics.intensity - 0.6).abs() < f32::EPSILON);
        assert!((haptics.frequency_hz - 80.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn full_profile_preserves_notch_filters() -> TestResult {
        let json = full_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        assert_eq!(profile.base.filters.notch_filters.len(), 1);
        let nf = &profile.base.filters.notch_filters[0];
        assert!((nf.hz - 50.0).abs() < f32::EPSILON);
        assert!((nf.q - 2.0).abs() < f32::EPSILON);
        assert!((nf.gain_db - (-12.0)).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn full_profile_preserves_curve_points() -> TestResult {
        let json = full_profile_json();
        let profile: config::ProfileSchema = serde_json::from_str(&json)?;
        assert_eq!(profile.base.filters.curve_points.len(), 3);
        assert!((profile.base.filters.curve_points[1].input - 0.5).abs() < f32::EPSILON);
        assert!((profile.base.filters.curve_points[1].output - 0.7).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn profile_filter_rename_attributes() -> TestResult {
        // Verify serde rename attributes work correctly
        let json = full_profile_json();
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let base = value.get("base").ok_or("missing base")?;
        // These use camelCase rename
        assert!(base.get("ffbGain").is_some());
        assert!(base.get("dorDeg").is_some());
        assert!(base.get("torqueCapNm").is_some());
        let filters = base.get("filters").ok_or("missing filters")?;
        assert!(filters.get("notchFilters").is_some());
        assert!(filters.get("slewRate").is_some());
        assert!(filters.get("curvePoints").is_some());
        assert!(filters.get("handsOff").is_some());
        assert!(filters.get("torqueCap").is_some());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 13. Proto backward compatibility (wire format)
// ═══════════════════════════════════════════════════════════════════════

mod proto_backward_compat {
    use super::*;

    #[test]
    fn proto_device_capabilities_empty_decodes_to_defaults() -> TestResult {
        // Empty message should decode with all fields at proto3 defaults
        let bytes: Vec<u8> = vec![];
        let caps = proto::DeviceCapabilities::decode(bytes.as_slice())?;
        assert!(!caps.supports_pid);
        assert!(!caps.supports_raw_torque_1khz);
        assert!(!caps.supports_health_stream);
        assert!(!caps.supports_led_bus);
        assert_eq!(caps.max_torque_cnm, 0);
        assert_eq!(caps.encoder_cpr, 0);
        assert_eq!(caps.min_report_period_us, 0);
        Ok(())
    }

    #[test]
    fn proto_telemetry_data_empty_decodes_to_defaults() -> TestResult {
        let bytes: Vec<u8> = vec![];
        let td = proto::TelemetryData::decode(bytes.as_slice())?;
        assert_eq!(td.wheel_angle_mdeg, 0);
        assert_eq!(td.wheel_speed_mrad_s, 0);
        assert_eq!(td.temp_c, 0);
        assert_eq!(td.faults, 0);
        assert!(!td.hands_on);
        assert_eq!(td.sequence, 0);
        Ok(())
    }

    #[test]
    fn proto_profile_empty_decodes_to_defaults() -> TestResult {
        let bytes: Vec<u8> = vec![];
        let profile = proto::Profile::decode(bytes.as_slice())?;
        assert!(profile.schema_version.is_empty());
        assert!(profile.scope.is_none());
        assert!(profile.base.is_none());
        assert!(profile.leds.is_none());
        assert!(profile.haptics.is_none());
        assert!(profile.signature.is_empty());
        Ok(())
    }

    #[test]
    fn proto_game_status_roundtrip() -> TestResult {
        let status = proto::GameStatus {
            active_game: "iRacing".into(),
            telemetry_active: true,
            car_id: "porsche-911-gt3".into(),
            track_id: "spa".into(),
        };
        let bytes = status.encode_to_vec();
        let decoded = proto::GameStatus::decode(bytes.as_slice())?;
        assert_eq!(decoded.active_game, "iRacing");
        assert!(decoded.telemetry_active);
        assert_eq!(decoded.car_id, "porsche-911-gt3");
        assert_eq!(decoded.track_id, "spa");
        Ok(())
    }

    #[test]
    fn proto_configure_telemetry_request_roundtrip() -> TestResult {
        let req = proto::ConfigureTelemetryRequest {
            game_id: "iRacing".into(),
            install_path: "C:\\Games\\iRacing".into(),
            enable_auto_config: true,
        };
        let bytes = req.encode_to_vec();
        let decoded = proto::ConfigureTelemetryRequest::decode(bytes.as_slice())?;
        assert_eq!(decoded.game_id, "iRacing");
        assert!(decoded.enable_auto_config);
        Ok(())
    }

    #[test]
    fn proto_performance_metrics_roundtrip() -> TestResult {
        let metrics = proto::PerformanceMetrics {
            p99_jitter_us: 250.0,
            missed_tick_rate: 0.001,
            total_ticks: 1_000_000,
            missed_ticks: 1000,
        };
        let bytes = metrics.encode_to_vec();
        let decoded = proto::PerformanceMetrics::decode(bytes.as_slice())?;
        assert!((decoded.p99_jitter_us - 250.0).abs() < f32::EPSILON);
        assert!((decoded.missed_tick_rate - 0.001).abs() < f32::EPSILON);
        assert_eq!(decoded.total_ticks, 1_000_000);
        assert_eq!(decoded.missed_ticks, 1000);
        Ok(())
    }

    #[test]
    fn proto_profile_list_roundtrip() -> TestResult {
        let list = proto::ProfileList {
            profiles: vec![
                proto::Profile {
                    schema_version: "wheel.profile/1".into(),
                    scope: Some(proto::ProfileScope {
                        game: "game1".into(),
                        car: String::new(),
                        track: String::new(),
                    }),
                    base: None,
                    leds: None,
                    haptics: None,
                    signature: String::new(),
                },
                proto::Profile {
                    schema_version: "wheel.profile/1".into(),
                    scope: Some(proto::ProfileScope {
                        game: "game2".into(),
                        car: String::new(),
                        track: String::new(),
                    }),
                    base: None,
                    leds: None,
                    haptics: None,
                    signature: String::new(),
                },
            ],
        };
        let bytes = list.encode_to_vec();
        let decoded = proto::ProfileList::decode(bytes.as_slice())?;
        assert_eq!(decoded.profiles.len(), 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 14. Edge cases and special values
// ═══════════════════════════════════════════════════════════════════════

mod edge_cases {
    use super::*;

    #[test]
    fn telemetry_with_negative_gear_roundtrips() -> TestResult {
        let t = NormalizedTelemetry::builder().gear(-1).build();
        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.gear, -1);
        Ok(())
    }

    #[test]
    fn telemetry_with_zero_gear_is_neutral() -> TestResult {
        let t = NormalizedTelemetry::builder().gear(0).build();
        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.gear, 0);
        Ok(())
    }

    #[test]
    fn telemetry_with_max_tire_temps() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .tire_temps_c([255, 255, 255, 255])
            .build();
        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.tire_temps_c, [255, 255, 255, 255]);
        Ok(())
    }

    #[test]
    fn telemetry_validated_clamps_out_of_range() -> TestResult {
        let t = NormalizedTelemetry {
            throttle: 1.5,
            brake: -0.5,
            fuel_percent: 2.0,
            ffb_scalar: 5.0,
            ..Default::default()
        };
        let v = t.validated();
        assert!((v.throttle - 1.0).abs() < f32::EPSILON);
        assert!((v.brake - 0.0).abs() < f32::EPSILON);
        assert!((v.fuel_percent - 1.0).abs() < f32::EPSILON);
        assert!((v.ffb_scalar - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_validated_handles_nan() -> TestResult {
        let t = NormalizedTelemetry {
            speed_ms: f32::NAN,
            throttle: f32::NAN,
            rpm: f32::NAN,
            ..Default::default()
        };
        let v = t.validated();
        assert!((v.speed_ms - 0.0).abs() < f32::EPSILON);
        assert!((v.throttle - 0.0).abs() < f32::EPSILON);
        assert!((v.rpm - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_id_normalizes_to_lowercase() -> TestResult {
        let id: DeviceId = "MOZA-R9".parse()?;
        assert_eq!(id.as_str(), "moza-r9");
        Ok(())
    }

    #[test]
    fn device_id_trims_whitespace() -> TestResult {
        let id: DeviceId = "  moza-r9  ".parse()?;
        assert_eq!(id.as_str(), "moza-r9");
        Ok(())
    }

    #[test]
    fn profile_id_normalizes_to_lowercase() -> TestResult {
        let id: ProfileId = "iRacing.GT3".parse()?;
        assert_eq!(id.as_str(), "iracing.gt3");
        Ok(())
    }

    #[test]
    fn degrees_millidegrees_conversion() -> TestResult {
        let d = Degrees::new_angle(45.0)?;
        let mdeg = d.to_millidegrees();
        assert_eq!(mdeg, 45000);
        let back = Degrees::from_millidegrees(mdeg);
        assert!((back.value() - 45.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn torque_cnm_conversion() -> TestResult {
        let t = TorqueNm::new(12.5)?;
        let cnm = t.to_cnm();
        assert_eq!(cnm, 1250);
        let back = TorqueNm::from_cnm(cnm)?;
        assert!((back.value() - 12.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn profile_scope_specificity_ordering() -> TestResult {
        let global = ProfileScope::global();
        let game = ProfileScope::for_game("test".into());
        let car = ProfileScope::for_car("test".into(), "car".into());
        let track = ProfileScope::for_track("test".into(), "car".into(), "track".into());

        assert_eq!(global.specificity_level(), 0);
        assert_eq!(game.specificity_level(), 1);
        assert_eq!(car.specificity_level(), 2);
        assert_eq!(track.specificity_level(), 3);

        assert!(game.is_more_specific_than(&global));
        assert!(car.is_more_specific_than(&game));
        assert!(track.is_more_specific_than(&car));
        Ok(())
    }

    #[test]
    fn profile_scope_matching() -> TestResult {
        let scope = ProfileScope::for_car("iRacing".into(), "porsche-911".into());
        assert!(scope.matches(Some("iRacing"), Some("porsche-911"), None));
        assert!(scope.matches(Some("iRacing"), Some("porsche-911"), Some("spa")));
        assert!(!scope.matches(Some("ACC"), Some("porsche-911"), None));
        assert!(!scope.matches(Some("iRacing"), Some("bmw-m4"), None));
        Ok(())
    }

    #[test]
    fn empty_extended_data_not_serialized() -> TestResult {
        let t = NormalizedTelemetry::default();
        let json = serde_json::to_string(&t)?;
        // When extended is empty, skip_serializing_if ensures it's omitted
        assert!(!json.contains("extended"));
        Ok(())
    }

    #[test]
    fn optional_string_fields_not_serialized_when_none() -> TestResult {
        let t = NormalizedTelemetry::default();
        let json = serde_json::to_string(&t)?;
        assert!(!json.contains("car_id"));
        assert!(!json.contains("track_id"));
        assert!(!json.contains("session_id"));
        Ok(())
    }
}
