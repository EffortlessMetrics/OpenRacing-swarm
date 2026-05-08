//! Deep schema validation tests for racing-wheel-schemas.
//!
//! Covers:
//! 1. All schema types validate correctly
//! 2. Invalid field values rejected
//! 3. Schema evolution compatibility
//! 4. Cross-version migration
//! 5. JSON Schema compliance
//! 6. Proto message round-trips
//! 7. Default values
//! 8. Optional vs required fields

#![deny(clippy::unwrap_used)]

use racing_wheel_schemas::config::{
    BumpstopConfig as ConfigBumpstopConfig, FilterConfig as ConfigFilterConfig,
    HandsOffConfig as ConfigHandsOffConfig, ProfileMigrator, ProfileValidator,
};
use racing_wheel_schemas::domain::{
    CurvePoint, Degrees, DeviceId, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm,
    validate_curve_monotonic,
};
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SchemaVersion,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryData, TelemetryFlags, TelemetryFrame, TelemetrySnapshot,
    TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Build a minimal valid profile JSON string.
fn minimal_valid_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string()
}

/// Build a full profile JSON with all optional fields.
fn full_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "iRacing",
            "car": "MX-5",
            "track": "Laguna Seca"
        },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 900,
            "torqueCapNm": 12.5,
            "filters": {
                "reconstruction": 3,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "bumpstop": { "enabled": true, "strength": 0.6 },
                "handsOff": { "enabled": false, "sensitivity": 0.5 },
                "torqueCap": 8.0,
                "notchFilters": [
                    { "hz": 50.0, "q": 2.0, "gainDb": -12.0 }
                ],
                "slewRate": 0.8,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.5, "output": 0.6 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        },
        "leds": {
            "rpmBands": [0.5, 0.7, 0.9],
            "pattern": "progressive",
            "brightness": 0.8,
            "colors": { "green": [0, 255, 0], "red": [255, 0, 0] }
        },
        "haptics": {
            "enabled": true,
            "intensity": 0.7,
            "frequencyHz": 150.0,
            "effects": { "abs": true, "tc": false }
        },
        "signature": "abc123"
    })
    .to_string()
}

// ═══════════════════════════════════════════════════════════
// 1. All schema types validate correctly
// ═══════════════════════════════════════════════════════════

mod all_types_validate {
    use super::*;

    #[test]
    fn torque_nm_valid_range() -> TestResult {
        for val in [0.0_f32, 1.0, 10.0, 25.0, 49.99, 50.0] {
            let t = TorqueNm::new(val)?;
            assert!((t.value() - val).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn degrees_dor_valid_range() -> TestResult {
        for val in [180.0_f32, 360.0, 540.0, 900.0, 1080.0, 2160.0] {
            let d = Degrees::new_dor(val)?;
            assert!((d.value() - val).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn gain_valid_range() -> TestResult {
        for val in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let g = Gain::new(val)?;
            assert!((g.value() - val).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn frequency_valid_values() -> TestResult {
        for val in [0.001_f32, 1.0, 50.0, 1000.0, 20000.0] {
            let f = FrequencyHz::new(val)?;
            assert!((f.value() - val).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn device_id_valid_patterns() -> TestResult {
        for id in ["a", "device-1", "wheel_base", "abc123", "my-wheel_v2"] {
            let parsed: DeviceId = id.parse()?;
            assert!(!parsed.as_str().is_empty());
        }
        Ok(())
    }

    #[test]
    fn profile_id_valid_patterns() -> TestResult {
        for id in ["a", "iracing.gt3", "profile-v2_test", "global", "a.b.c"] {
            let parsed: ProfileId = id.parse()?;
            assert!(!parsed.as_str().is_empty());
        }
        Ok(())
    }

    #[test]
    fn curve_point_valid_corners() -> TestResult {
        let corners = [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0), (0.5, 0.5)];
        for (i, o) in corners {
            let cp = CurvePoint::new(i, o)?;
            assert!((cp.input - i).abs() < f32::EPSILON);
            assert!((cp.output - o).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn profile_validator_accepts_minimal_profile() -> TestResult {
        let validator = ProfileValidator::new()?;
        let result = validator.validate_json(&minimal_valid_profile_json());
        assert!(
            result.is_ok(),
            "Minimal profile should validate: {result:?}"
        );
        Ok(())
    }

    #[test]
    fn profile_validator_accepts_full_profile() -> TestResult {
        let validator = ProfileValidator::new()?;
        let result = validator.validate_json(&full_profile_json());
        assert!(result.is_ok(), "Full profile should validate: {result:?}");
        Ok(())
    }

    #[test]
    fn telemetry_data_default_is_valid() {
        let td = TelemetryData::default();
        assert!((td.wheel_angle_deg).abs() < f32::EPSILON);
        assert!(!td.hands_on);
    }

    #[test]
    fn normalized_telemetry_builder_produces_valid_data() {
        let t = NormalizedTelemetry::builder()
            .speed_ms(50.0)
            .rpm(6000.0)
            .gear(4)
            .throttle(0.8)
            .brake(0.1)
            .build();
        assert!((t.speed_ms - 50.0).abs() < f32::EPSILON);
        assert_eq!(t.gear, 4);
    }

    #[test]
    fn schema_version_parse_current() -> TestResult {
        let sv = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(sv.is_current());
        assert_eq!(sv.major, 1);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 2. Invalid field values rejected
// ═══════════════════════════════════════════════════════════

mod invalid_values_rejected {
    use super::*;

    #[test]
    fn torque_nm_rejects_negative() {
        assert!(TorqueNm::new(-0.001).is_err());
        assert!(TorqueNm::new(-100.0).is_err());
    }

    #[test]
    fn torque_nm_rejects_over_max() {
        assert!(TorqueNm::new(50.001).is_err());
        assert!(TorqueNm::new(1000.0).is_err());
    }

    #[test]
    fn torque_nm_rejects_non_finite() {
        assert!(TorqueNm::new(f32::NAN).is_err());
        assert!(TorqueNm::new(f32::INFINITY).is_err());
        assert!(TorqueNm::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn degrees_dor_rejects_below_min() {
        assert!(Degrees::new_dor(179.9).is_err());
        assert!(Degrees::new_dor(0.0).is_err());
        assert!(Degrees::new_dor(-1.0).is_err());
    }

    #[test]
    fn degrees_dor_rejects_above_max() {
        assert!(Degrees::new_dor(2160.1).is_err());
        assert!(Degrees::new_dor(5000.0).is_err());
    }

    #[test]
    fn degrees_rejects_nan() {
        assert!(Degrees::new_dor(f32::NAN).is_err());
        assert!(Degrees::new_angle(f32::NAN).is_err());
        assert!(Degrees::new_angle(f32::INFINITY).is_err());
    }

    #[test]
    fn gain_rejects_out_of_range() {
        assert!(Gain::new(-0.001).is_err());
        assert!(Gain::new(1.001).is_err());
        assert!(Gain::new(f32::NAN).is_err());
        assert!(Gain::new(f32::INFINITY).is_err());
    }

    #[test]
    fn frequency_rejects_zero_and_negative() {
        assert!(FrequencyHz::new(0.0).is_err());
        assert!(FrequencyHz::new(-1.0).is_err());
        assert!(FrequencyHz::new(f32::NAN).is_err());
        assert!(FrequencyHz::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn device_id_rejects_empty() {
        assert!("".parse::<DeviceId>().is_err());
        assert!("   ".parse::<DeviceId>().is_err());
    }

    #[test]
    fn device_id_rejects_special_chars() {
        assert!("dev@ice".parse::<DeviceId>().is_err());
        assert!("dev ice".parse::<DeviceId>().is_err());
        assert!("dev/ice".parse::<DeviceId>().is_err());
        assert!("dev#1".parse::<DeviceId>().is_err());
    }

    #[test]
    fn profile_id_rejects_empty_and_special() {
        assert!("".parse::<ProfileId>().is_err());
        assert!("   ".parse::<ProfileId>().is_err());
        assert!("profile with spaces".parse::<ProfileId>().is_err());
        assert!("profile@1".parse::<ProfileId>().is_err());
    }

    #[test]
    fn curve_point_rejects_out_of_range() {
        assert!(CurvePoint::new(-0.1, 0.5).is_err());
        assert!(CurvePoint::new(1.1, 0.5).is_err());
        assert!(CurvePoint::new(0.5, -0.1).is_err());
        assert!(CurvePoint::new(0.5, 1.1).is_err());
    }

    #[test]
    fn curve_point_rejects_non_finite() {
        assert!(CurvePoint::new(f32::NAN, 0.5).is_err());
        assert!(CurvePoint::new(0.5, f32::NAN).is_err());
        assert!(CurvePoint::new(f32::INFINITY, 0.5).is_err());
    }

    #[test]
    fn validate_curve_monotonic_rejects_empty() {
        assert!(validate_curve_monotonic(&[]).is_err());
    }

    #[test]
    fn validate_curve_monotonic_rejects_non_increasing() -> TestResult {
        let points = vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.5)?,
            CurvePoint::new(0.3, 0.8)?,
        ];
        assert!(validate_curve_monotonic(&points).is_err());
        Ok(())
    }

    #[test]
    fn validate_curve_monotonic_rejects_equal_inputs() -> TestResult {
        let points = vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.3)?,
            CurvePoint::new(0.5, 0.8)?,
        ];
        assert!(validate_curve_monotonic(&points).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_missing_schema_field() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "scope": {},
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_invalid_schema_version() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["schema"] = serde_json::Value::String("wheel.profile/99".to_string());
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_ffb_gain_out_of_range() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["ffbGain"] = serde_json::json!(1.5);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_dor_below_minimum() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["dorDeg"] = serde_json::json!(90);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_torque_cap_above_max() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["torqueCapNm"] = serde_json::json!(100.0);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_non_monotonic_curve() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["filters"]["curvePoints"] = serde_json::json!([
            { "input": 0.0, "output": 0.0 },
            { "input": 0.8, "output": 0.5 },
            { "input": 0.5, "output": 1.0 }
        ]);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_unsorted_rpm_bands() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&full_profile_json())?;
        json["leds"]["rpmBands"] = serde_json::json!([0.9, 0.5, 0.7]);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validator_rejects_too_few_curve_points() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["filters"]["curvePoints"] = serde_json::json!([
            { "input": 0.0, "output": 0.0 }
        ]);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 3. Schema evolution compatibility
// ═══════════════════════════════════════════════════════════

mod schema_evolution {
    use super::*;

    #[test]
    fn schema_version_ordering() -> TestResult {
        let v1 = SchemaVersion::parse("wheel.profile/1")?;
        let v2 = SchemaVersion::new(2, 0);
        assert!(v1.is_older_than(&v2));
        assert!(!v2.is_older_than(&v1));
        Ok(())
    }

    #[test]
    fn schema_version_minor_ordering() {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_1 = SchemaVersion::new(1, 1);
        assert!(v1_0.is_older_than(&v1_1));
        assert!(!v1_1.is_older_than(&v1_0));
    }

    #[test]
    fn schema_version_same_not_older() -> TestResult {
        let v1a = SchemaVersion::parse("wheel.profile/1")?;
        let v1b = SchemaVersion::parse("wheel.profile/1")?;
        assert!(!v1a.is_older_than(&v1b));
        assert!(!v1b.is_older_than(&v1a));
        Ok(())
    }

    #[test]
    fn schema_version_display() -> TestResult {
        let v = SchemaVersion::parse("wheel.profile/1")?;
        let display = format!("{v}");
        assert_eq!(display, "wheel.profile/1");
        Ok(())
    }

    #[test]
    fn schema_version_rejects_invalid_format() {
        assert!(SchemaVersion::parse("invalid").is_err());
        assert!(SchemaVersion::parse("foo/bar").is_err());
        assert!(SchemaVersion::parse("wheel.profile/").is_err());
        assert!(SchemaVersion::parse("wheel.profile/abc").is_err());
    }

    #[test]
    fn current_schema_version_is_parseable() -> TestResult {
        let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(v.is_current());
        Ok(())
    }

    #[test]
    fn unknown_fields_in_json_do_not_break_deserialization() -> TestResult {
        // JSON with an unknown top-level field — the validator should reject it
        // because the schema has additionalProperties: false, but the serde
        // deserialization itself should work (or the validator catches it).
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["unknownField"] = serde_json::json!("extra");
        let s = serde_json::to_string(&json)?;
        // With additionalProperties: false, this should fail validation
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn optional_leds_can_be_omitted() -> TestResult {
        let validator = ProfileValidator::new()?;
        // minimal_valid_profile_json has no leds
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.leds.is_none());
        Ok(())
    }

    #[test]
    fn optional_haptics_can_be_omitted() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.haptics.is_none());
        Ok(())
    }

    #[test]
    fn optional_signature_can_be_omitted() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.signature.is_none());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 4. Cross-version migration
// ═══════════════════════════════════════════════════════════

mod cross_version_migration {
    use super::*;

    #[test]
    fn migrate_current_version_is_noop() -> TestResult {
        let profile = ProfileMigrator::migrate_profile(&minimal_valid_profile_json())?;
        assert_eq!(profile.schema, "wheel.profile/1");
        Ok(())
    }

    #[test]
    fn migrate_unknown_version_fails() {
        let json = serde_json::json!({
            "schema": "wheel.profile/99",
            "scope": {},
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        assert!(ProfileMigrator::migrate_profile(&json).is_err());
    }

    #[test]
    fn migrate_missing_schema_field_fails() {
        let json = serde_json::json!({
            "scope": {},
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        assert!(ProfileMigrator::migrate_profile(&json).is_err());
    }

    #[test]
    fn migration_config_without_backups() {
        let config = MigrationConfig::without_backups();
        assert!(!config.create_backups);
        assert_eq!(config.max_backups, 0);
        assert!(config.validate_after_migration);
    }

    #[test]
    fn migration_config_default() {
        let config = MigrationConfig::default();
        assert!(config.create_backups);
        assert!(config.max_backups > 0);
    }

    #[test]
    fn migration_manager_creates_with_no_backup_config() -> TestResult {
        let config = MigrationConfig::without_backups();
        let _manager = MigrationManager::new(config)?;
        Ok(())
    }

    #[test]
    fn schema_version_new_constructs_correctly() {
        let v = SchemaVersion::new(3, 2);
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 2);
        assert_eq!(v.version, "wheel.profile/3.2");
    }
}

// ═══════════════════════════════════════════════════════════
// 5. JSON Schema compliance
// ═══════════════════════════════════════════════════════════

mod json_schema_compliance {
    use super::*;

    #[test]
    fn schema_requires_base_section() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": {}
        })
        .to_string();
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn schema_requires_scope_section() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn schema_rejects_missing_filters() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 900,
                "torqueCapNm": 10.0
            }
        })
        .to_string();
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn schema_rejects_string_for_numeric_field() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["ffbGain"] = serde_json::json!("not_a_number");
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn schema_validates_notch_filter_structure() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        // Missing required "q" field in notch filter
        json["base"]["filters"]["notchFilters"] = serde_json::json!([
            { "hz": 50.0, "gainDb": -6.0 }
        ]);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn schema_accepts_null_optional_fields() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            },
            "leds": null,
            "haptics": null,
            "signature": null
        })
        .to_string();
        let result = validator.validate_json(&json);
        assert!(
            result.is_ok(),
            "Null optional fields should validate: {result:?}"
        );
        Ok(())
    }

    #[test]
    fn schema_rejects_negative_slew_rate() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["filters"]["slewRate"] = serde_json::json!(-0.1);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn schema_rejects_reconstruction_above_max() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut json: serde_json::Value = serde_json::from_str(&minimal_valid_profile_json())?;
        json["base"]["filters"]["reconstruction"] = serde_json::json!(9);
        let s = serde_json::to_string(&json)?;
        assert!(validator.validate_json(&s).is_err());
        Ok(())
    }

    #[test]
    fn profile_validate_profile_struct() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&full_profile_json())?;
        let result = validator.validate_profile(&profile);
        assert!(
            result.is_ok(),
            "Struct re-validation should pass: {result:?}"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 6. Proto message round-trips
// ═══════════════════════════════════════════════════════════

mod proto_roundtrips {
    use super::*;
    use prost::Message;
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    #[test]
    fn device_info_proto_roundtrip() -> TestResult {
        let orig = proto::DeviceInfo {
            id: "moza-r9".to_string(),
            name: "Moza R9".to_string(),
            r#type: proto::DeviceType::WheelBase as i32,
            capabilities: Some(proto::DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 1500,
                encoder_cpr: 65536,
                min_report_period_us: 1000,
            }),
            state: proto::DeviceState::Connected as i32,
            vendor_id: 0x346E,
            product_id: 0x0014,
        };
        let bytes = orig.encode_to_vec();
        let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
        assert_eq!(decoded, orig);
        Ok(())
    }

    #[test]
    fn profile_proto_roundtrip() -> TestResult {
        let orig = proto::Profile {
            schema_version: "wheel.profile/1".to_string(),
            scope: Some(proto::ProfileScope {
                game: "iRacing".to_string(),
                car: "MX-5".to_string(),
                track: "".to_string(),
            }),
            base: Some(proto::BaseSettings {
                ffb_gain: 0.75,
                dor_deg: 900,
                torque_cap_nm: 12.5,
                filters: Some(proto::FilterConfig {
                    reconstruction: 3,
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
                }),
            }),
            leds: None,
            haptics: Some(proto::HapticsConfig {
                enabled: true,
                intensity: 0.7,
                frequency_hz: 150.0,
            }),
            signature: "sig123".to_string(),
        };
        let bytes = orig.encode_to_vec();
        let decoded = proto::Profile::decode(bytes.as_slice())?;
        assert_eq!(decoded, orig);
        Ok(())
    }

    #[test]
    fn telemetry_data_proto_roundtrip() -> TestResult {
        let orig = proto::TelemetryData {
            wheel_angle_mdeg: 45000,
            wheel_speed_mrad_s: 1000,
            temp_c: 42,
            faults: 0,
            hands_on: true,
            sequence: 12345,
        };
        let bytes = orig.encode_to_vec();
        let decoded = proto::TelemetryData::decode(bytes.as_slice())?;
        assert_eq!(decoded, orig);
        Ok(())
    }

    #[test]
    fn health_event_proto_roundtrip() -> TestResult {
        let orig = proto::HealthEvent {
            timestamp: Some(prost_types::Timestamp {
                seconds: 1700000000,
                nanos: 500_000_000,
            }),
            device_id: "wheel-1".to_string(),
            r#type: proto::HealthEventType::FaultDetected as i32,
            message: "Over temperature".to_string(),
            metadata: std::collections::BTreeMap::from([("temp".to_string(), "85".to_string())]),
        };
        let bytes = orig.encode_to_vec();
        let decoded = proto::HealthEvent::decode(bytes.as_slice())?;
        assert_eq!(decoded, orig);
        Ok(())
    }

    #[test]
    fn feature_negotiation_proto_roundtrip() -> TestResult {
        let req = proto::FeatureNegotiationRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec!["device_management".to_string(), "streaming".to_string()],
            namespace: "wheel.v1".to_string(),
        };
        let bytes = req.encode_to_vec();
        let decoded = proto::FeatureNegotiationRequest::decode(bytes.as_slice())?;
        assert_eq!(decoded, req);

        let resp = proto::FeatureNegotiationResponse {
            server_version: "1.0.0".to_string(),
            supported_features: vec!["device_management".to_string()],
            enabled_features: vec!["device_management".to_string()],
            compatible: true,
            min_client_version: "1.0.0".to_string(),
        };
        let bytes = resp.encode_to_vec();
        let decoded = proto::FeatureNegotiationResponse::decode(bytes.as_slice())?;
        assert_eq!(decoded, resp);
        Ok(())
    }

    #[test]
    fn op_result_proto_roundtrip() -> TestResult {
        let orig = proto::OpResult {
            success: false,
            error_message: "device not found".to_string(),
            metadata: std::collections::BTreeMap::new(),
        };
        let bytes = orig.encode_to_vec();
        let decoded = proto::OpResult::decode(bytes.as_slice())?;
        assert_eq!(decoded, orig);
        Ok(())
    }

    #[test]
    fn empty_proto_message_decodes() -> TestResult {
        // An empty byte buffer should decode to default proto values
        let decoded = proto::TelemetryData::decode(&[] as &[u8])?;
        assert_eq!(decoded.wheel_angle_mdeg, 0);
        assert!(!decoded.hands_on);
        Ok(())
    }

    #[test]
    fn profile_list_proto_roundtrip() -> TestResult {
        let list = proto::ProfileList {
            profiles: vec![
                proto::Profile {
                    schema_version: "wheel.profile/1".to_string(),
                    scope: None,
                    base: None,
                    leds: None,
                    haptics: None,
                    signature: String::new(),
                },
                proto::Profile {
                    schema_version: "wheel.profile/1".to_string(),
                    scope: Some(proto::ProfileScope {
                        game: "ACC".to_string(),
                        car: "".to_string(),
                        track: "".to_string(),
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
        assert_eq!(decoded, list);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 7. Default values
// ═══════════════════════════════════════════════════════════

mod default_values {
    use super::*;

    #[test]
    fn config_filter_config_defaults_are_1khz_safe() {
        let fc = ConfigFilterConfig::default();
        assert_eq!(fc.reconstruction, 0);
        assert!((fc.friction).abs() < f32::EPSILON);
        assert!((fc.damper).abs() < f32::EPSILON);
        assert!((fc.inertia).abs() < f32::EPSILON);
        assert!((fc.slew_rate - 1.0).abs() < f32::EPSILON);
        assert!(fc.notch_filters.is_empty());
        assert_eq!(fc.curve_points.len(), 2);
    }

    #[test]
    fn bumpstop_config_defaults() {
        let bs = ConfigBumpstopConfig::default();
        assert!(bs.enabled);
        assert!((bs.strength - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn hands_off_config_defaults() {
        let ho = ConfigHandsOffConfig::default();
        assert!(ho.enabled);
        assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn telemetry_flags_defaults() {
        let flags = TelemetryFlags::default();
        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(flags.green_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.in_pits);
        assert!(!flags.abs_active);
    }

    #[test]
    fn normalized_telemetry_defaults_are_zero() {
        let t = NormalizedTelemetry::default();
        assert!((t.speed_ms).abs() < f32::EPSILON);
        assert!((t.rpm).abs() < f32::EPSILON);
        assert_eq!(t.gear, 0);
        assert!((t.throttle).abs() < f32::EPSILON);
        assert!((t.brake).abs() < f32::EPSILON);
        assert_eq!(t.sequence, 0);
        assert!(t.car_id.is_none());
        assert!(t.track_id.is_none());
        assert!(t.session_id.is_none());
    }

    #[test]
    fn torque_nm_zero_constant() {
        assert!((TorqueNm::ZERO.value()).abs() < f32::EPSILON);
    }

    #[test]
    fn degrees_zero_constant() {
        assert!((Degrees::ZERO.value()).abs() < f32::EPSILON);
    }

    #[test]
    fn gain_zero_and_full_constants() {
        assert!((Gain::ZERO.value()).abs() < f32::EPSILON);
        assert!((Gain::FULL.value() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn telemetry_data_default_all_zero() {
        let td = TelemetryData::default();
        assert!((td.wheel_angle_deg).abs() < f32::EPSILON);
        assert!((td.wheel_speed_rad_s).abs() < f32::EPSILON);
        assert_eq!(td.temperature_c, 0);
        assert_eq!(td.fault_flags, 0);
        assert!(!td.hands_on);
        assert_eq!(td.timestamp, 0);
    }

    #[test]
    fn profile_validator_default_works() {
        let validator = ProfileValidator::default();
        let result = validator.validate_json(&minimal_valid_profile_json());
        assert!(result.is_ok());
    }
}

// ═══════════════════════════════════════════════════════════
// 8. Optional vs required fields
// ═══════════════════════════════════════════════════════════

mod optional_vs_required {
    use super::*;

    #[test]
    fn scope_fields_all_optional() -> TestResult {
        let validator = ProfileValidator::new()?;
        // Empty scope should work
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.scope.game.is_none());
        assert!(profile.scope.car.is_none());
        assert!(profile.scope.track.is_none());
        Ok(())
    }

    #[test]
    fn scope_with_partial_fields() -> TestResult {
        let validator = ProfileValidator::new()?;
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "iRacing" },
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        let profile = validator.validate_json(&json)?;
        assert_eq!(profile.scope.game.as_deref(), Some("iRacing"));
        assert!(profile.scope.car.is_none());
        assert!(profile.scope.track.is_none());
        Ok(())
    }

    #[test]
    fn base_filters_required_fields_enforced() -> TestResult {
        let validator = ProfileValidator::new()?;
        // Missing "friction" from filters
        let json = serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.8, "dorDeg": 900, "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 0.5,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        })
        .to_string();
        assert!(validator.validate_json(&json).is_err());
        Ok(())
    }

    #[test]
    fn bumpstop_defaults_when_omitted() -> TestResult {
        let validator = ProfileValidator::new()?;
        // The minimal profile doesn't specify bumpstop, but serde defaults apply
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.base.filters.bumpstop.enabled);
        assert!((profile.base.filters.bumpstop.strength - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn hands_off_defaults_when_omitted() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.base.filters.hands_off.enabled);
        assert!((profile.base.filters.hands_off.sensitivity - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn torque_cap_filter_is_optional() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&minimal_valid_profile_json())?;
        assert!(profile.base.filters.torque_cap.is_none());
        Ok(())
    }

    #[test]
    fn telemetry_value_enum_variants_serialize() -> TestResult {
        let vals = vec![
            TelemetryValue::Float(1.5),
            TelemetryValue::Integer(42),
            TelemetryValue::Boolean(true),
            TelemetryValue::String("hello".to_string()),
        ];
        for val in &vals {
            let json = serde_json::to_string(val)?;
            let restored: TelemetryValue = serde_json::from_str(&json)?;
            assert_eq!(&restored, val);
        }
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_roundtrip_through_json() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .speed_ms(30.0)
            .rpm(5000.0)
            .gear(3)
            .throttle(0.6)
            .brake(0.0)
            .build();
        let epoch = std::time::Instant::now();
        let snap = TelemetrySnapshot::from_telemetry(&t, epoch);
        let json = serde_json::to_string(&snap)?;
        let restored: TelemetrySnapshot = serde_json::from_str(&json)?;
        assert!((restored.speed_ms - 30.0).abs() < f32::EPSILON);
        assert!((restored.rpm - 5000.0).abs() < f32::EPSILON);
        assert_eq!(restored.gear, 3);
        Ok(())
    }

    #[test]
    fn telemetry_frame_preserves_fields() {
        let t = NormalizedTelemetry::builder().speed_ms(20.0).build();
        let frame = TelemetryFrame::new(t, 1000, 42, 256);
        assert_eq!(frame.sequence, 42);
        assert_eq!(frame.raw_size, 256);
        assert!((frame.data.speed_ms - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalized_telemetry_computed_fields() {
        let t = NormalizedTelemetry::builder()
            .speed_ms(10.0)
            .rpm(6000.0)
            .max_rpm(8000.0)
            .lateral_g(3.0)
            .longitudinal_g(4.0)
            .build();
        assert!((t.speed_kmh() - 36.0).abs() < 0.1);
        assert!((t.speed_mph() - 22.37).abs() < 0.1);
        assert!((t.rpm_fraction() - 0.75).abs() < 0.01);
        assert!((t.total_g() - 5.0).abs() < 0.01);
        assert!(!t.is_stationary());
    }

    #[test]
    fn normalized_telemetry_stationary_check() {
        let stopped = NormalizedTelemetry::builder().speed_ms(0.0).build();
        assert!(stopped.is_stationary());

        let slow = NormalizedTelemetry::builder().speed_ms(0.4).build();
        assert!(slow.is_stationary());

        let moving = NormalizedTelemetry::builder().speed_ms(0.6).build();
        assert!(!moving.is_stationary());
    }

    #[test]
    fn normalized_telemetry_validated_clamps_non_finite() {
        let t = NormalizedTelemetry {
            speed_ms: f32::NAN,
            throttle: 1.5,
            brake: -0.1,
            slip_ratio: f32::INFINITY,
            ..Default::default()
        };
        let v = t.validated();
        assert!((v.speed_ms).abs() < f32::EPSILON);
        assert!((v.throttle - 1.0).abs() < f32::EPSILON);
        assert!((v.brake).abs() < f32::EPSILON);
        assert!((v.slip_ratio).abs() < f32::EPSILON);
    }

    #[test]
    fn domain_error_conversions() {
        let err = DomainError::InvalidTorque(99.0, 50.0);
        let display = format!("{err}");
        assert!(display.contains("99"));

        let err = DomainError::InvalidDeviceId("bad id".to_string());
        let display = format!("{err}");
        assert!(display.contains("bad id"));
    }

    #[test]
    fn device_id_normalizes_case() -> TestResult {
        let id: DeviceId = "MyWheel-V3".parse()?;
        assert_eq!(id.as_str(), "mywheel-v3");
        Ok(())
    }

    #[test]
    fn profile_id_normalizes_case() -> TestResult {
        let id: ProfileId = "IRACING.GT3".parse()?;
        assert_eq!(id.as_str(), "iracing.gt3");
        Ok(())
    }

    #[test]
    fn device_id_into_string() -> TestResult {
        let id: DeviceId = "wheel-1".parse()?;
        let s: String = id.into();
        assert_eq!(s, "wheel-1");
        Ok(())
    }

    #[test]
    fn profile_id_into_string() -> TestResult {
        let id: ProfileId = "default".parse()?;
        let s: String = id.into();
        assert_eq!(s, "default");
        Ok(())
    }
}
