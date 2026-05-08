//! Deep tests for schema evolution, backward compatibility of protobuf and JSON
//! message types, breaking-change detection, enum evolution, and nested message
//! evolution.

use prost::Message;
use racing_wheel_schemas::config::ProfileValidator;
use racing_wheel_schemas::generated::wheel::v1 as proto;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SCHEMA_VERSION_V2, SchemaVersion,
};
use racing_wheel_schemas::telemetry::NormalizedTelemetry;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────
// 1. Schema version bumps preserve backward compat
// ──────────────────────────────────────────────────────────────────────

#[test]
fn schema_version_v1_parses_as_current() -> TestResult {
    let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(v.is_current());
    assert_eq!(v.major, 1);
    Ok(())
}

#[test]
fn schema_version_v2_is_newer_than_v1() -> TestResult {
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
    assert!(v1.is_older_than(&v2));
    assert!(!v2.is_older_than(&v1));
    Ok(())
}

#[test]
fn schema_version_display_round_trip() -> TestResult {
    let v = SchemaVersion::new(3, 7);
    assert_eq!(v.version, "wheel.profile/3.7");
    assert_eq!(v.major, 3);
    assert_eq!(v.minor, 7);
    let display = format!("{}", v);
    assert!(display.contains("wheel.profile/3.7"));
    Ok(())
}

#[test]
fn migration_manager_detects_current_version() -> TestResult {
    let config = MigrationConfig::without_backups();
    let mgr = MigrationManager::new(config)?;
    let json = minimal_profile_json();
    let version = mgr.detect_version(&json)?;
    assert!(version.is_current());
    assert!(!mgr.needs_migration(&json)?);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 2. Required field additions are breaking (detected)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_missing_base_is_rejected() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing" }
        // "base" is missing
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_missing_filters_is_rejected() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0
            // "filters" is missing
        }
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn proto_device_info_missing_required_string_defaults_empty() -> TestResult {
    // Proto3: missing string fields default to empty, not error
    let bytes: Vec<u8> = Vec::new();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.id, "");
    assert_eq!(decoded.name, "");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 3. Optional field additions are non-breaking
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_json_extra_unknown_top_level_fields_rejected_by_schema() -> TestResult {
    let json = serde_json::json!({
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
        },
        "futureSection": { "newField": 42 }
    })
    .to_string();

    // The JSON schema enforces additionalProperties: false — unknown top-level
    // fields are rejected. This is intentional: breaking-change detection relies
    // on strict validation so that new fields must be introduced in the schema
    // definition first.
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn proto_device_info_with_future_field_bytes_decoded() -> TestResult {
    // Encode with all known fields
    let msg = proto::DeviceInfo {
        id: "dev-001".into(),
        name: "Wheel".into(),
        r#type: 1, // WHEEL_BASE
        capabilities: None,
        state: 1, // CONNECTED
        vendor_id: 0,
        product_id: 0,
    };
    let mut bytes = msg.encode_to_vec();

    // Append a hypothetical future field (tag 100, varint value 99)
    // Protobuf wire format: field_number=100, wire_type=0 (varint) → (100 << 3) | 0 = 800
    // 800 in varint encoding = [0xA0, 0x06], value 99 = [0x63]
    bytes.extend_from_slice(&[0xA0, 0x06, 0x63]);

    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.id, "dev-001");
    assert_eq!(decoded.name, "Wheel");
    // Unknown field is silently ignored
    Ok(())
}

#[test]
fn telemetry_optional_fields_absent_gives_defaults() -> TestResult {
    let json = r#"{
        "speed_ms": 30.0,
        "steering_angle": 0.0,
        "throttle": 0.5,
        "brake": 0.0,
        "rpm": 5000.0,
        "gear": 3,
        "flags": {},
        "sequence": 1
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 4. Field removal is breaking (detected)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_json_without_curve_points_is_rejected() -> TestResult {
    let json = serde_json::json!({
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
                "slewRate": 0.8
                // "curvePoints" removed
            }
        }
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_json_without_schema_field_is_rejected() -> TestResult {
    let json = serde_json::json!({
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
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 5. Enum variant additions backward compat
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_device_type_unknown_variant_decoded_as_zero() -> TestResult {
    // Build raw bytes for DeviceInfo with type field = 99 (unknown enum value)
    // In proto3, unknown enum values are kept as their integer value
    let msg = proto::DeviceInfo {
        id: "dev-x".into(),
        name: "Future Device".into(),
        r#type: 99, // Unknown variant
        capabilities: None,
        state: 0,
        vendor_id: 0,
        product_id: 0,
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    // Proto3 preserves the raw integer for unknown enum variants
    assert_eq!(decoded.r#type, 99);
    assert_eq!(decoded.id, "dev-x");
    Ok(())
}

#[test]
fn proto_device_state_unknown_variant_preserved() -> TestResult {
    let msg = proto::DeviceInfo {
        id: "dev-y".into(),
        name: "".into(),
        r#type: 0,
        capabilities: None,
        state: 255, // Unknown DeviceState variant
        vendor_id: 0,
        product_id: 0,
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.state, 255);
    Ok(())
}

#[test]
fn proto_health_event_type_unknown_variant_preserved() -> TestResult {
    let msg = proto::HealthEvent {
        timestamp: None,
        device_id: "dev-z".into(),
        r#type: 42, // Unknown HealthEventType
        message: "future event".into(),
        metadata: Default::default(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::HealthEvent::decode(bytes.as_slice())?;
    assert_eq!(decoded.r#type, 42);
    assert_eq!(decoded.message, "future event");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 6. Nested message evolution
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_device_status_without_telemetry_decodes() -> TestResult {
    let msg = proto::DeviceStatus {
        device: Some(proto::DeviceInfo {
            id: "dev-nested".into(),
            name: "Nested Wheel".into(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: 0,
            product_id: 0,
        }),
        last_seen: None,
        active_faults: vec![],
        telemetry: None,
        moza: None,
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceStatus::decode(bytes.as_slice())?;
    assert!(decoded.telemetry.is_none());
    assert!(decoded.device.is_some());
    let device = decoded.device.as_ref().ok_or("missing device")?;
    assert_eq!(device.id, "dev-nested");
    Ok(())
}

#[test]
fn proto_device_status_with_all_nested_messages() -> TestResult {
    let msg = proto::DeviceStatus {
        device: Some(proto::DeviceInfo {
            id: "dev-full".into(),
            name: "Full Wheel".into(),
            r#type: 1,
            capabilities: Some(proto::DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 2000,
                encoder_cpr: 65536,
                min_report_period_us: 1000,
            }),
            state: 1,
            vendor_id: 0,
            product_id: 0,
        }),
        last_seen: Some(prost_types::Timestamp {
            seconds: 1700000000,
            nanos: 0,
        }),
        active_faults: vec!["OVER_TEMP".into()],
        telemetry: Some(proto::TelemetryData {
            wheel_angle_mdeg: 45000,
            wheel_speed_mrad_s: 100,
            temp_c: 55,
            faults: 0,
            hands_on: true,
            sequence: 42,
        }),
        moza: Some(proto::MozaReadinessStatus {
            model: "Moza R5".into(),
            product_id: "0x0014".into(),
            category: "wheelbase".into(),
            output_capable: true,
            ffb_ready: false,
            init_state: "uninitialized".into(),
            descriptor_trusted: false,
            descriptor_crc32: String::new(),
            descriptor_source: String::new(),
            lane: "moza-r5".into(),
            direct_mode_allowed: false,
            high_torque_allowed: false,
            safe_to_send_torque: false,
            safety_state: "pre_validation".into(),
            safety_reason: "test".into(),
        }),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceStatus::decode(bytes.as_slice())?;

    let device = decoded.device.as_ref().ok_or("missing device")?;
    assert_eq!(device.id, "dev-full");

    let caps = device.capabilities.as_ref().ok_or("missing caps")?;
    assert!(caps.supports_pid);
    assert_eq!(caps.max_torque_cnm, 2000);

    let telemetry = decoded.telemetry.as_ref().ok_or("missing telemetry")?;
    assert_eq!(telemetry.wheel_angle_mdeg, 45000);
    assert!(telemetry.hands_on);

    let moza = decoded.moza.as_ref().ok_or("missing Moza readiness")?;
    assert_eq!(moza.product_id, "0x0014");
    assert!(!moza.safe_to_send_torque);

    assert_eq!(decoded.active_faults.len(), 1);
    Ok(())
}

#[test]
fn proto_profile_nested_filter_evolution() -> TestResult {
    // Encode a profile with filters, decode and verify nested structure
    let msg = proto::Profile {
        schema_version: "wheel.profile/1".into(),
        scope: Some(proto::ProfileScope {
            game: "iRacing".into(),
            car: "".into(),
            track: "".into(),
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
                notch_filters: vec![proto::NotchFilter {
                    hz: 50.0,
                    q: 1.0,
                    gain_db: -6.0,
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
        haptics: None,
        signature: String::new(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::Profile::decode(bytes.as_slice())?;

    let base = decoded.base.as_ref().ok_or("missing base")?;
    let filters = base.filters.as_ref().ok_or("missing filters")?;
    assert_eq!(filters.notch_filters.len(), 1);
    assert!((filters.notch_filters[0].hz - 50.0).abs() < f32::EPSILON);
    assert_eq!(filters.curve_points.len(), 2);
    Ok(())
}

#[test]
fn proto_profile_without_optional_nested_messages() -> TestResult {
    let msg = proto::Profile {
        schema_version: "wheel.profile/1".into(),
        scope: None,
        base: None,
        leds: None,
        haptics: None,
        signature: String::new(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::Profile::decode(bytes.as_slice())?;

    assert!(decoded.scope.is_none());
    assert!(decoded.base.is_none());
    assert!(decoded.leds.is_none());
    assert!(decoded.haptics.is_none());
    Ok(())
}

#[test]
fn proto_feature_negotiation_round_trip() -> TestResult {
    let req = proto::FeatureNegotiationRequest {
        client_version: "1.2.3".into(),
        supported_features: vec!["device_management".into(), "health_monitoring".into()],
        namespace: "wheel.v1".into(),
    };
    let bytes = req.encode_to_vec();
    let decoded = proto::FeatureNegotiationRequest::decode(bytes.as_slice())?;
    assert_eq!(decoded.client_version, "1.2.3");
    assert_eq!(decoded.supported_features.len(), 2);
    assert_eq!(decoded.namespace, "wheel.v1");
    Ok(())
}

#[test]
fn proto_op_result_round_trip() -> TestResult {
    let msg = proto::OpResult {
        success: true,
        error_message: String::new(),
        metadata: [("key".to_string(), "val".to_string())]
            .into_iter()
            .collect(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::OpResult::decode(bytes.as_slice())?;
    assert!(decoded.success);
    assert_eq!(decoded.metadata.get("key").map(|s| &**s), Some("val"));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 7. Legacy migration detection
// ──────────────────────────────────────────────────────────────────────

#[test]
fn migration_detects_legacy_v0_format() -> TestResult {
    let config = MigrationConfig::without_backups();
    let mgr = MigrationManager::new(config)?;

    let legacy_json = serde_json::json!({
        "ffb_gain": 0.7,
        "degrees_of_rotation": 900
    })
    .to_string();

    let version = mgr.detect_version(&legacy_json)?;
    assert_eq!(version.major, 0);
    assert!(mgr.needs_migration(&legacy_json)?);
    Ok(())
}

#[test]
fn migration_legacy_to_v1_produces_valid_structure() -> TestResult {
    let config = MigrationConfig::without_backups();
    let mgr = MigrationManager::new(config)?;

    let legacy_json = serde_json::json!({
        "ffb_gain": 0.7,
        "degrees_of_rotation": 900,
        "torque_cap": 12.0
    })
    .to_string();

    let migrated = mgr.migrate_profile(&legacy_json)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;
    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );
    assert!(value.get("scope").is_some());
    assert!(value.get("base").is_some());
    Ok(())
}

#[test]
fn schema_version_parse_rejects_garbage() {
    let result = SchemaVersion::parse("not-a-schema");
    assert!(result.is_err());

    let result2 = SchemaVersion::parse("");
    assert!(result2.is_err());

    let result3 = SchemaVersion::parse("some.other/1");
    assert!(result3.is_err());
}

#[test]
fn schema_version_ordering_consistent() -> TestResult {
    let v1_0 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    let v2_0 = SchemaVersion::new(2, 0);

    assert!(v1_0.is_older_than(&v1_1));
    assert!(v1_1.is_older_than(&v2_0));
    assert!(v1_0.is_older_than(&v2_0));
    assert!(!v2_0.is_older_than(&v1_0));
    assert!(!v1_0.is_older_than(&v1_0));
    Ok(())
}
