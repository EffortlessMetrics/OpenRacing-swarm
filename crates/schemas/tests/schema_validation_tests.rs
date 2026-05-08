//! Comprehensive schema validation tests.
//!
//! Covers:
//! - Serialize/deserialize roundtrips for all schema types
//! - Schema evolution (add field, remove field, rename field)
//! - Cross-format tests (JSON ↔ protobuf)
//! - Default value handling and stability
//! - Proptest-based fuzzing for schema types

use racing_wheel_schemas::config::{
    BumpstopConfig as ConfigBumpstopConfig, HandsOffConfig as ConfigHandsOffConfig,
    ProfileMigrator, ProfileValidator,
};
use racing_wheel_schemas::domain::{
    CurvePoint, Degrees, DeviceId, FrequencyHz, Gain, ProfileId, TorqueNm, validate_curve_monotonic,
};
use racing_wheel_schemas::entities::{
    BaseSettings, BumpstopConfig, CalibrationData, CalibrationType, Device, DeviceCapabilities,
    DeviceState, DeviceType, FilterConfig, HandsOffConfig, HapticsConfig, LedConfig,
    PedalCalibrationData, Profile, ProfileMetadata, ProfileScope,
};
use racing_wheel_schemas::ipc_conversion::ConversionError;
use racing_wheel_schemas::migration::CURRENT_SCHEMA_VERSION;
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryData, TelemetryFlags, TelemetryValue,
};

use prost::Message;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Helper functions
// =========================================================================

fn make_valid_profile_json() -> String {
    r#"{
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 3,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "bumpstop": { "enabled": true, "strength": 0.5 },
                "handsOff": { "enabled": true, "sensitivity": 0.3 },
                "notchFilters": [
                    { "hz": 60.0, "q": 2.0, "gainDb": -10.0 }
                ],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.5, "output": 0.6 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    }"#
    .to_string()
}

fn make_device(id: &str) -> Result<Device, BoxErr> {
    let device_id: DeviceId = id.parse()?;
    let max_torque = TorqueNm::new(25.0)?;
    let caps = DeviceCapabilities::new(true, true, true, true, max_torque, 4096, 1000);
    Ok(Device::new(
        device_id,
        "Test Wheel".to_string(),
        DeviceType::WheelBase,
        caps,
    ))
}

fn make_profile() -> Result<Profile, BoxErr> {
    let id: ProfileId = "test-profile".parse()?;
    let scope = ProfileScope::for_game("iRacing".to_string());
    let base = BaseSettings::default();
    let metadata = ProfileMetadata {
        name: "Test Profile".to_string(),
        description: Some("A test profile".to_string()),
        author: Some("test".to_string()),
        version: "1.0.0".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        modified_at: "2024-01-01T00:00:00Z".to_string(),
        tags: vec!["test".to_string()],
    };
    Ok(Profile {
        id,
        parent: None,
        scope,
        base_settings: base,
        led_config: None,
        haptics_config: None,
        metadata,
    })
}

// =========================================================================
// Section 1: Serialize/deserialize roundtrips for all schema types
// =========================================================================

#[test]
fn device_serde_json_roundtrip() -> Result<(), BoxErr> {
    let device = make_device("test-wheel-1")?;
    let json = serde_json::to_string(&device)?;
    let restored: Device = serde_json::from_str(&json)?;
    assert_eq!(restored.id, device.id);
    assert_eq!(restored.name, device.name);
    assert_eq!(restored.device_type, device.device_type);
    assert_eq!(restored.capabilities, device.capabilities);
    Ok(())
}

#[test]
fn device_capabilities_serde_roundtrip() -> Result<(), BoxErr> {
    let torque = TorqueNm::new(20.0)?;
    let caps = DeviceCapabilities::new(true, false, true, false, torque, 8192, 1000);
    let json = serde_json::to_string(&caps)?;
    let restored: DeviceCapabilities = serde_json::from_str(&json)?;
    assert_eq!(restored, caps);
    Ok(())
}

#[test]
fn profile_serde_json_roundtrip() -> Result<(), BoxErr> {
    let profile = make_profile()?;
    let json = serde_json::to_string(&profile)?;
    let restored: Profile = serde_json::from_str(&json)?;
    assert_eq!(restored.id, profile.id);
    assert_eq!(restored.scope, profile.scope);
    assert_eq!(restored.base_settings, profile.base_settings);
    assert_eq!(restored.metadata.name, profile.metadata.name);
    Ok(())
}

#[test]
fn filter_config_serde_roundtrip() -> Result<(), BoxErr> {
    let config = FilterConfig::default();
    let json = serde_json::to_string(&config)?;
    let restored: FilterConfig = serde_json::from_str(&json)?;
    assert_eq!(restored, config);
    Ok(())
}

#[test]
fn telemetry_data_serde_roundtrip() -> Result<(), BoxErr> {
    let data = TelemetryData {
        wheel_angle_deg: 45.5,
        wheel_speed_rad_s: 2.3,
        temperature_c: 42,
        fault_flags: 0,
        hands_on: true,
        timestamp: 12345,
    };
    let json = serde_json::to_string(&data)?;
    let restored: TelemetryData = serde_json::from_str(&json)?;
    assert_eq!(restored, data);
    Ok(())
}

#[test]
fn normalized_telemetry_serde_roundtrip() -> Result<(), BoxErr> {
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(5000.0)
        .max_rpm(8000.0)
        .gear(3)
        .throttle(0.7)
        .brake(0.1)
        .lateral_g(1.2)
        .build();
    let json = serde_json::to_string(&telemetry)?;
    let restored: NormalizedTelemetry = serde_json::from_str(&json)?;
    assert!((restored.speed_ms - 30.0).abs() < f32::EPSILON);
    assert!((restored.rpm - 5000.0).abs() < f32::EPSILON);
    assert_eq!(restored.gear, 3);
    Ok(())
}

#[test]
fn telemetry_flags_serde_roundtrip() -> Result<(), BoxErr> {
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        abs_active: true,
        ..Default::default()
    };
    let json = serde_json::to_string(&flags)?;
    let restored: TelemetryFlags = serde_json::from_str(&json)?;
    assert!(restored.yellow_flag);
    assert!(restored.blue_flag);
    assert!(restored.abs_active);
    assert!(!restored.red_flag);
    Ok(())
}

#[test]
fn telemetry_value_variants_roundtrip() -> Result<(), BoxErr> {
    let values = vec![
        TelemetryValue::Float(3.15),
        TelemetryValue::Integer(42),
        TelemetryValue::Boolean(true),
        TelemetryValue::String("test-value".to_string()),
    ];
    for val in &values {
        let json = serde_json::to_string(val)?;
        let restored: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(&restored, val);
    }
    Ok(())
}

#[test]
fn calibration_data_serde_roundtrip() -> Result<(), BoxErr> {
    let mut cal = CalibrationData::new(CalibrationType::Full);
    cal.center_position = Some(0.0);
    cal.min_position = Some(-450.0);
    cal.max_position = Some(450.0);
    cal.pedal_ranges = Some(PedalCalibrationData {
        throttle: Some((0.0, 1.0)),
        brake: Some((0.0, 1.0)),
        clutch: None,
    });
    let json = serde_json::to_string(&cal)?;
    let restored: CalibrationData = serde_json::from_str(&json)?;
    assert_eq!(restored.center_position, Some(0.0));
    assert_eq!(restored.calibration_type, CalibrationType::Full);
    assert!(restored.is_fully_calibrated());
    Ok(())
}

#[test]
fn led_config_serde_roundtrip() -> Result<(), BoxErr> {
    let config = LedConfig::default();
    let json = serde_json::to_string(&config)?;
    let restored: LedConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.pattern, config.pattern);
    assert_eq!(restored.rpm_bands, config.rpm_bands);
    Ok(())
}

#[test]
fn haptics_config_serde_roundtrip() -> Result<(), BoxErr> {
    let config = HapticsConfig::default();
    let json = serde_json::to_string(&config)?;
    let restored: HapticsConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.enabled, config.enabled);
    assert_eq!(restored.effects, config.effects);
    Ok(())
}

#[test]
fn profile_scope_serde_roundtrip() -> Result<(), BoxErr> {
    let scopes = vec![
        ProfileScope::global(),
        ProfileScope::for_game("iRacing".to_string()),
        ProfileScope::for_car("ACC".to_string(), "porsche-992".to_string()),
        ProfileScope::for_track(
            "ACC".to_string(),
            "porsche-992".to_string(),
            "spa".to_string(),
        ),
    ];
    for scope in &scopes {
        let json = serde_json::to_string(scope)?;
        let restored: ProfileScope = serde_json::from_str(&json)?;
        assert_eq!(&restored, scope);
    }
    Ok(())
}

#[test]
fn device_state_all_variants_roundtrip() -> Result<(), BoxErr> {
    let states = [
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ];
    for state in &states {
        let json = serde_json::to_string(state)?;
        let restored: DeviceState = serde_json::from_str(&json)?;
        assert_eq!(&restored, state);
    }
    Ok(())
}

#[test]
fn device_type_all_variants_roundtrip() -> Result<(), BoxErr> {
    let types = [
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ];
    for dt in &types {
        let json = serde_json::to_string(dt)?;
        let restored: DeviceType = serde_json::from_str(&json)?;
        assert_eq!(&restored, dt);
    }
    Ok(())
}

#[test]
fn calibration_type_all_variants_roundtrip() -> Result<(), BoxErr> {
    let types = [
        CalibrationType::Center,
        CalibrationType::Range,
        CalibrationType::Pedals,
        CalibrationType::Full,
    ];
    for ct in &types {
        let json = serde_json::to_string(ct)?;
        let restored: CalibrationType = serde_json::from_str(&json)?;
        assert_eq!(&restored, ct);
    }
    Ok(())
}

// =========================================================================
// Section 2: Schema evolution tests
// =========================================================================

#[test]
fn json_ignores_unknown_fields_device() -> Result<(), BoxErr> {
    // Simulate a future schema with extra fields
    let json_with_extra = r#"{
        "id": "test-device",
        "name": "Wheel",
        "device_type": "WheelBase",
        "capabilities": {
            "supports_pid": true,
            "supports_raw_torque_1khz": true,
            "supports_health_stream": false,
            "supports_led_bus": false,
            "max_torque": 25.0,
            "encoder_cpr": 4096,
            "min_report_period_us": 1000
        },
        "state": "Connected",
        "fault_flags": 0,
        "firmware_version": null,
        "serial_number": null,
        "new_future_field": "should be ignored"
    }"#;
    // serde with deny_unknown_fields would fail; our types don't have that
    let result: Result<Device, _> = serde_json::from_str(json_with_extra);
    // This should succeed because serde defaults to ignoring unknown fields
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn json_missing_optional_fields_telemetry() -> Result<(), BoxErr> {
    // NormalizedTelemetry with only required fields
    let minimal_json = r#"{
        "speed_ms": 10.0,
        "steering_angle": 0.0,
        "throttle": 0.5,
        "brake": 0.0,
        "rpm": 3000.0,
        "gear": 2,
        "sequence": 0
    }"#;
    let t: NormalizedTelemetry = serde_json::from_str(minimal_json)?;
    assert!((t.speed_ms - 10.0).abs() < f32::EPSILON);
    // Optional/default fields should be defaults
    assert_eq!(t.lateral_g, 0.0);
    assert!(t.car_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

#[test]
fn json_default_flags_are_stable() -> Result<(), BoxErr> {
    let flags = TelemetryFlags::default();
    let json = serde_json::to_string(&flags)?;
    let restored: TelemetryFlags = serde_json::from_str(&json)?;
    assert!(restored.green_flag, "default green_flag should be true");
    assert!(!restored.yellow_flag);
    assert!(!restored.red_flag);
    assert!(!restored.session_paused);
    Ok(())
}

#[test]
fn schema_version_detection_current() -> Result<(), BoxErr> {
    let json = make_valid_profile_json();
    let value: serde_json::Value = serde_json::from_str(&json)?;
    let schema = value
        .get("schema")
        .and_then(|v| v.as_str())
        .ok_or("missing schema field")?;
    assert_eq!(schema, CURRENT_SCHEMA_VERSION);
    Ok(())
}

#[test]
fn schema_version_v2_not_parseable_by_current() {
    let json = r#"{
        "schema": "wheel.profile/2",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 3,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    }"#;
    let result = ProfileMigrator::migrate_profile(json);
    assert!(result.is_err(), "v2 schema should not be accepted");
}

#[test]
fn profile_evolution_extra_scope_fields_ignored() -> Result<(), BoxErr> {
    let json = r#"{ "game": "ACC", "car": "bmw-m4", "track": "monza", "weather": "rain" }"#;
    let scope: ProfileScope = serde_json::from_str(json)?;
    assert_eq!(scope.game.as_deref(), Some("ACC"));
    assert_eq!(scope.track.as_deref(), Some("monza"));
    Ok(())
}

// =========================================================================
// Section 3: Cross-format tests (JSON ↔ protobuf)
// =========================================================================

#[test]
fn domain_device_to_proto_and_back() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let device = make_device("my-wheelbase")?;
    let proto_device: proto::DeviceInfo = device.clone().into();

    // Verify protobuf encoding is deterministic
    let mut buf1 = Vec::new();
    proto_device.encode(&mut buf1)?;
    let mut buf2 = Vec::new();
    proto_device.encode(&mut buf2)?;
    assert_eq!(buf1, buf2, "protobuf encoding should be deterministic");

    // Decode back to domain
    let decoded_proto = proto::DeviceInfo::decode(&buf1[..])?;
    let restored: Device = decoded_proto.try_into()?;
    assert_eq!(restored.id, device.id);
    assert_eq!(restored.device_type, device.device_type);
    Ok(())
}

#[test]
fn domain_telemetry_data_to_proto_and_back() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let data = TelemetryData {
        wheel_angle_deg: 123.456,
        wheel_speed_rad_s: 5.5,
        temperature_c: 55,
        fault_flags: 3,
        hands_on: true,
        timestamp: 0,
    };

    let proto_data: proto::TelemetryData = data.clone().into();
    let mut buf = Vec::new();
    proto_data.encode(&mut buf)?;
    let decoded_proto = proto::TelemetryData::decode(&buf[..])?;
    let restored: TelemetryData = decoded_proto.try_into()?;

    // Conversion goes through millidegrees, so check with tolerance
    assert!((restored.wheel_angle_deg - data.wheel_angle_deg).abs() < 0.01);
    assert!((restored.wheel_speed_rad_s - data.wheel_speed_rad_s).abs() < 0.01);
    assert_eq!(restored.temperature_c, data.temperature_c);
    assert_eq!(restored.fault_flags, data.fault_flags);
    assert_eq!(restored.hands_on, data.hands_on);
    Ok(())
}

#[test]
fn domain_profile_to_proto_and_back() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let profile = make_profile()?;
    let proto_profile: proto::Profile = profile.clone().into();

    let mut buf = Vec::new();
    proto_profile.encode(&mut buf)?;
    let decoded_proto = proto::Profile::decode(&buf[..])?;
    let restored: Profile = decoded_proto.try_into()?;

    // Profile fields survive the round-trip (some metadata is lost in proto)
    assert_eq!(
        restored.base_settings.ffb_gain.value(),
        profile.base_settings.ffb_gain.value()
    );
    assert!(
        (restored.base_settings.degrees_of_rotation.value()
            - profile.base_settings.degrees_of_rotation.value())
        .abs()
            < 1.0
    );
    Ok(())
}

#[test]
fn proto_capabilities_unit_conversion() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let torque = TorqueNm::new(15.0)?;
    let caps = DeviceCapabilities::new(true, true, false, true, torque, 4096, 1000);
    let proto_caps: proto::DeviceCapabilities = caps.clone().into();

    // Check unit conversion: Nm → centi-Nm
    assert_eq!(proto_caps.max_torque_cnm, 1500);
    assert_eq!(proto_caps.encoder_cpr, 4096);

    // Round-trip
    let restored: DeviceCapabilities = proto_caps.try_into()?;
    assert!((restored.max_torque.value() - 15.0).abs() < 0.01);
    Ok(())
}

#[test]
fn proto_filter_config_roundtrip() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let config = FilterConfig::default();
    let proto_config: proto::FilterConfig = config.clone().into();

    let mut buf = Vec::new();
    proto_config.encode(&mut buf)?;
    let decoded: proto::FilterConfig = proto::FilterConfig::decode(&buf[..])?;
    let restored: FilterConfig = decoded.try_into()?;

    assert_eq!(restored.reconstruction, config.reconstruction);
    assert_eq!(restored.friction.value(), config.friction.value());
    assert_eq!(restored.curve_points.len(), config.curve_points.len());
    Ok(())
}

// =========================================================================
// Section 4: Default value handling
// =========================================================================

#[test]
fn default_filter_config_is_linear() {
    let config = FilterConfig::default();
    assert!(config.is_linear());
    assert_eq!(config.reconstruction, 0);
    assert!((config.friction.value()).abs() < f32::EPSILON);
    assert!((config.damper.value()).abs() < f32::EPSILON);
    assert!((config.inertia.value()).abs() < f32::EPSILON);
}

#[test]
fn default_base_settings_are_valid() -> Result<(), BoxErr> {
    let settings = BaseSettings::default();
    assert!((settings.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
    assert!((settings.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
    assert!((settings.torque_cap.value() - 15.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn default_bumpstop_config_values_stable() {
    let bs = BumpstopConfig::default();
    assert!(bs.enabled);
    assert!((bs.start_angle - 450.0).abs() < f32::EPSILON);
    assert!((bs.max_angle - 540.0).abs() < f32::EPSILON);
}

#[test]
fn default_hands_off_config_values_stable() {
    let ho = HandsOffConfig::default();
    assert!(ho.enabled);
    assert!((ho.threshold - 0.05).abs() < f32::EPSILON);
    assert!((ho.timeout_seconds - 5.0).abs() < f32::EPSILON);
}

#[test]
fn default_led_config_has_expected_bands() {
    let led = LedConfig::default();
    assert_eq!(led.rpm_bands.len(), 5);
    assert_eq!(led.pattern, "progressive");
    assert!(led.colors.contains_key("green"));
    assert!(led.colors.contains_key("red"));
}

#[test]
fn default_haptics_config_has_effects() {
    let haptics = HapticsConfig::default();
    assert!(haptics.enabled);
    assert!(haptics.effects.contains_key("kerb"));
    assert!(haptics.effects.contains_key("slip"));
}

#[test]
fn default_telemetry_flags_green_flag_on() {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag);
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
}

#[test]
fn default_normalized_telemetry_all_zero() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert!(t.car_id.is_none());
    assert!(t.extended.is_empty());
}

#[test]
fn default_device_state_on_new_device() -> Result<(), BoxErr> {
    let device = make_device("new-device")?;
    assert_eq!(device.state, DeviceState::Connected);
    assert_eq!(device.fault_flags, 0);
    assert!(!device.has_faults());
    Ok(())
}

#[test]
fn default_config_bumpstop_and_hands_off_serde_stability() -> Result<(), BoxErr> {
    // Ensure defaults survive JSON round-trip for config types
    let bs_config = ConfigBumpstopConfig::default();
    let json = serde_json::to_string(&bs_config)?;
    let restored: ConfigBumpstopConfig = serde_json::from_str(&json)?;
    assert!(restored.enabled);
    assert!((restored.strength - 0.5).abs() < f32::EPSILON);

    let ho_config = ConfigHandsOffConfig::default();
    let json = serde_json::to_string(&ho_config)?;
    let restored: ConfigHandsOffConfig = serde_json::from_str(&json)?;
    assert!(restored.enabled);
    assert!((restored.sensitivity - 0.3).abs() < f32::EPSILON);
    Ok(())
}

// =========================================================================
// Section 5: Profile validation (JSON schema)
// =========================================================================

#[test]
fn profile_validator_accepts_valid_profile() -> Result<(), BoxErr> {
    let validator = ProfileValidator::new()?;
    let json = make_valid_profile_json();
    let profile = validator.validate_json(&json)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    Ok(())
}

#[test]
fn profile_validator_rejects_wrong_schema_version() -> Result<(), BoxErr> {
    let validator = ProfileValidator::new()?;
    let json = r#"{
        "schema": "wheel.profile/99",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 3,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    }"#;
    let result = validator.validate_json(json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_rejects_non_monotonic_curve() -> Result<(), BoxErr> {
    let validator = ProfileValidator::new()?;
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.8, "output": 0.9 },
                    { "input": 0.5, "output": 0.6 }
                ]
            }
        }
    }"#;
    let result = validator.validate_json(json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_rejects_malformed_json() -> Result<(), BoxErr> {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json("not valid json {{{");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_migrator_accepts_current_version() -> Result<(), BoxErr> {
    let json = make_valid_profile_json();
    let profile = ProfileMigrator::migrate_profile(&json)?;
    assert_eq!(profile.schema, CURRENT_SCHEMA_VERSION);
    Ok(())
}

#[test]
fn profile_migrator_rejects_unknown_version() {
    let json = r#"{ "schema": "wheel.profile/999" }"#;
    let result = ProfileMigrator::migrate_profile(json);
    assert!(result.is_err());
}

// =========================================================================
// Section 6: IPC conversion validation
// =========================================================================

#[test]
fn proto_conversion_rejects_invalid_device_type() {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let wire = proto::DeviceInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        r#type: 99, // invalid
        capabilities: Some(proto::DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: false,
            supports_led_bus: false,
            max_torque_cnm: 1500,
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        }),
        state: 1,
        vendor_id: 0,
        product_id: 0,
    };
    let result: Result<Device, ConversionError> = wire.try_into();
    assert!(result.is_err());
}

#[test]
fn proto_conversion_rejects_missing_capabilities() {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let wire = proto::DeviceInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        r#type: 1,
        capabilities: None, // missing required field
        state: 1,
        vendor_id: 0,
        product_id: 0,
    };
    let result: Result<Device, ConversionError> = wire.try_into();
    assert!(result.is_err());
}

#[test]
fn proto_conversion_rejects_invalid_encoder_cpr() {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let wire = proto::DeviceCapabilities {
        supports_pid: true,
        supports_raw_torque_1khz: true,
        supports_health_stream: false,
        supports_led_bus: false,
        max_torque_cnm: 1500,
        encoder_cpr: 500, // too low (min 1000)
        min_report_period_us: 1000,
    };
    let result: Result<DeviceCapabilities, ConversionError> = wire.try_into();
    assert!(result.is_err());
}

#[test]
fn proto_conversion_rejects_out_of_range_temperature() {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let wire = proto::TelemetryData {
        wheel_angle_mdeg: 0,
        wheel_speed_mrad_s: 0,
        temp_c: 200, // too high (max 150)
        faults: 0,
        hands_on: false,
        sequence: 0,
    };
    let result: Result<TelemetryData, ConversionError> = wire.try_into();
    assert!(result.is_err());
}

#[test]
fn proto_scope_empty_strings_become_none() -> Result<(), BoxErr> {
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let wire = proto::ProfileScope {
        game: String::new(),
        car: String::new(),
        track: String::new(),
    };
    let scope: ProfileScope = wire.try_into()?;
    assert!(scope.game.is_none());
    assert!(scope.car.is_none());
    assert!(scope.track.is_none());
    Ok(())
}

// =========================================================================
// Section 7: Domain validation edge cases
// =========================================================================

#[test]
fn torque_rejects_nan_and_infinity() {
    assert!(TorqueNm::new(f32::NAN).is_err());
    assert!(TorqueNm::new(f32::INFINITY).is_err());
    assert!(TorqueNm::new(f32::NEG_INFINITY).is_err());
}

#[test]
fn degrees_rejects_nan() {
    assert!(Degrees::new_dor(f32::NAN).is_err());
    assert!(Degrees::new_angle(f32::NAN).is_err());
}

#[test]
fn gain_rejects_nan_and_out_of_range() {
    assert!(Gain::new(f32::NAN).is_err());
    assert!(Gain::new(-0.01).is_err());
    assert!(Gain::new(1.01).is_err());
}

#[test]
fn frequency_rejects_zero_and_negative() {
    assert!(FrequencyHz::new(0.0).is_err());
    assert!(FrequencyHz::new(-1.0).is_err());
    assert!(FrequencyHz::new(f32::NAN).is_err());
}

#[test]
fn curve_point_rejects_out_of_range() {
    assert!(CurvePoint::new(-0.1, 0.5).is_err());
    assert!(CurvePoint::new(0.5, 1.1).is_err());
    assert!(CurvePoint::new(f32::NAN, 0.5).is_err());
}

#[test]
fn validate_curve_rejects_empty() {
    let result = validate_curve_monotonic(&[]);
    assert!(result.is_err());
}

#[test]
fn validate_curve_rejects_non_monotonic() -> Result<(), BoxErr> {
    let points = vec![
        CurvePoint::new(0.0, 0.0)?,
        CurvePoint::new(0.7, 0.7)?,
        CurvePoint::new(0.5, 0.5)?,
    ];
    let result = validate_curve_monotonic(&points);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validate_curve_accepts_single_point() -> Result<(), BoxErr> {
    let points = vec![CurvePoint::new(0.5, 0.5)?];
    assert!(validate_curve_monotonic(&points).is_ok());
    Ok(())
}

#[test]
fn device_id_normalizes_and_validates() -> Result<(), BoxErr> {
    let id: DeviceId = "MOZA-R9".parse()?;
    assert_eq!(id.as_str(), "moza-r9");

    // Spaces are rejected
    assert!("has spaces".parse::<DeviceId>().is_err());
    // Empty string is rejected
    assert!("".parse::<DeviceId>().is_err());
    // Underscores are allowed
    assert!("my_device_1".parse::<DeviceId>().is_ok());
    Ok(())
}

#[test]
fn profile_id_normalizes_and_validates() -> Result<(), BoxErr> {
    let id: ProfileId = "IRacing.GT3".parse()?;
    assert_eq!(id.as_str(), "iracing.gt3");

    // Empty string rejected
    assert!("".parse::<ProfileId>().is_err());
    // Dots are allowed
    assert!("v1.2.3".parse::<ProfileId>().is_ok());
    Ok(())
}

// =========================================================================
// Section 8: Proptest-based fuzzing
// =========================================================================

mod proptest_schemas {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn torque_always_roundtrips_through_json(val in 0.0f32..=50.0f32) {
            if let Ok(torque) = TorqueNm::new(val) {
                let Ok(json) = serde_json::to_string(&torque) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<TorqueNm>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert!((restored.value() - val).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn gain_always_roundtrips_through_json(val in 0.0f32..=1.0f32) {
            if let Ok(gain) = Gain::new(val) {
                let Ok(json) = serde_json::to_string(&gain) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<Gain>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert!((restored.value() - val).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn frequency_always_roundtrips_through_json(val in 0.01f32..=100000.0f32) {
            if let Ok(freq) = FrequencyHz::new(val) {
                let Ok(json) = serde_json::to_string(&freq) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<FrequencyHz>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert!((restored.value() - val).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn degrees_dor_roundtrip(val in 180.0f32..=2160.0f32) {
            if let Ok(deg) = Degrees::new_dor(val) {
                let Ok(json) = serde_json::to_string(&deg) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<Degrees>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert!((restored.value() - val).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn curve_point_roundtrip(input in 0.0f32..=1.0f32, output in 0.0f32..=1.0f32) {
            if let Ok(cp) = CurvePoint::new(input, output) {
                let Ok(json) = serde_json::to_string(&cp) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<CurvePoint>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert!((restored.input - input).abs() < f32::EPSILON);
                prop_assert!((restored.output - output).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn torque_cnm_roundtrip(cnm in 0u16..=5000u16) {
            if let Ok(torque) = TorqueNm::from_cnm(cnm) {
                let cnm_back = torque.to_cnm();
                prop_assert_eq!(cnm_back, cnm);
            }
        }

        #[test]
        fn telemetry_value_float_roundtrip(val in -1000.0f32..=1000.0f32) {
            if val.is_finite() {
                let tv = TelemetryValue::Float(val);
                let Ok(json) = serde_json::to_string(&tv) else {
                    prop_assert!(false, "serialize failed");
                    unreachable!()
                };
                let Ok(restored) = serde_json::from_str::<TelemetryValue>(&json) else {
                    prop_assert!(false, "deserialize failed");
                    unreachable!()
                };
                prop_assert_eq!(restored, tv);
            }
        }

        #[test]
        fn telemetry_data_roundtrip(
            angle in -1800.0f32..=1800.0f32,
            speed in -100.0f32..=100.0f32,
            temp in 0u8..=150u8,
            faults in 0u8..=255u8,
        ) {
            let data = TelemetryData {
                wheel_angle_deg: angle,
                wheel_speed_rad_s: speed,
                temperature_c: temp,
                fault_flags: faults,
                hands_on: true,
                timestamp: 0,
            };
            let Ok(json) = serde_json::to_string(&data) else {
                prop_assert!(false, "serialize failed");
                unreachable!()
            };
            let Ok(restored) = serde_json::from_str::<TelemetryData>(&json) else {
                prop_assert!(false, "deserialize failed");
                unreachable!()
            };
            prop_assert_eq!(restored, data);
        }

        #[test]
        fn device_id_rejects_invalid_characters(s in "[!@#$%^&*()+=\\[\\]{}<>|;:',./? ]+") {
            // These characters are never alphanumeric (even in Unicode) so should always be rejected.
            if !s.trim().is_empty() {
                let result: Result<DeviceId, _> = s.parse();
                prop_assert!(result.is_err());
            }
        }
    }
}
