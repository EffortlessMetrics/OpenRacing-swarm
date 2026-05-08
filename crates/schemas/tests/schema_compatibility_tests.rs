//! Schema compatibility tests for IPC message types.
//!
//! Validates backward/forward compatibility of protobuf wire types,
//! JSON schema validation, default values for missing fields, and
//! enum evolution safety.

use prost::Message;
use racing_wheel_schemas::config::{
    BumpstopConfig as ConfigBumpstopConfig, FilterConfig as ConfigFilterConfig,
    HandsOffConfig as ConfigHandsOffConfig, ProfileValidator,
};
use racing_wheel_schemas::generated::wheel::v1 as proto;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// Helper: minimal valid profile JSON
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
// 1. Backward compatibility: old messages parsed by new code
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_device_info_minimal_fields_decode() -> TestResult {
    // Simulate an old client sending only id and name (tag 1, 2)
    let old_msg = proto::DeviceInfo {
        id: "dev-001".into(),
        name: "Old Wheel".into(),
        r#type: 0,
        capabilities: None,
        state: 0,
        vendor_id: 0,
        product_id: 0,
    };
    let bytes = old_msg.encode_to_vec();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.id, "dev-001");
    assert_eq!(decoded.name, "Old Wheel");
    // Default enum value for missing fields
    assert_eq!(decoded.r#type, 0);
    assert_eq!(decoded.state, 0);
    assert!(decoded.capabilities.is_none());
    Ok(())
}

#[test]
fn proto_telemetry_data_old_format_without_sequence() -> TestResult {
    // Old wire format only had angle/speed/temp/faults/hands_on
    let old_msg = proto::TelemetryData {
        wheel_angle_mdeg: 45000,
        wheel_speed_mrad_s: 1500,
        temp_c: 55,
        faults: 0,
        hands_on: true,
        sequence: 0, // default
    };
    let bytes = old_msg.encode_to_vec();
    let decoded = proto::TelemetryData::decode(bytes.as_slice())?;
    assert_eq!(decoded.wheel_angle_mdeg, 45000);
    assert!(decoded.hands_on);
    assert_eq!(decoded.sequence, 0);
    Ok(())
}

#[test]
fn proto_profile_with_empty_optional_fields() -> TestResult {
    let msg = proto::Profile {
        schema_version: "wheel.profile/1".into(),
        scope: Some(proto::ProfileScope {
            game: "ACC".into(),
            car: String::new(),
            track: String::new(),
        }),
        base: None,
        leds: None,
        haptics: None,
        signature: String::new(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::Profile::decode(bytes.as_slice())?;
    assert_eq!(decoded.schema_version, "wheel.profile/1");
    assert!(decoded.base.is_none());
    assert!(decoded.leds.is_none());
    assert!(decoded.haptics.is_none());
    assert!(decoded.signature.is_empty());
    Ok(())
}

#[test]
fn proto_feature_negotiation_old_client_no_namespace() -> TestResult {
    let old_request = proto::FeatureNegotiationRequest {
        client_version: "0.9.0".into(),
        supported_features: vec!["device_management".into()],
        namespace: String::new(), // not set by old clients
    };
    let bytes = old_request.encode_to_vec();
    let decoded = proto::FeatureNegotiationRequest::decode(bytes.as_slice())?;
    assert_eq!(decoded.client_version, "0.9.0");
    assert!(decoded.namespace.is_empty());
    Ok(())
}

#[test]
fn proto_op_result_minimal_success() -> TestResult {
    let msg = proto::OpResult {
        success: true,
        error_message: String::new(),
        metadata: Default::default(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::OpResult::decode(bytes.as_slice())?;
    assert!(decoded.success);
    assert!(decoded.error_message.is_empty());
    assert!(decoded.metadata.is_empty());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 2. Forward compatibility: unknown fields are preserved
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_unknown_fields_in_bytes_are_tolerated() -> TestResult {
    // Encode a DeviceId then append extra bytes (simulating a future field)
    let msg = proto::DeviceId {
        id: "dev-future".into(),
    };
    let mut bytes = msg.encode_to_vec();
    // Append field tag=99, wire type varint, value 42
    // tag = (99 << 3) | 0 = 792 => varint encoding
    bytes.extend_from_slice(&[0xF8, 0x06, 0x2A]);
    let decoded = proto::DeviceId::decode(bytes.as_slice())?;
    assert_eq!(decoded.id, "dev-future");
    Ok(())
}

#[test]
fn proto_unknown_nested_message_tolerated() -> TestResult {
    let scope = proto::ProfileScope {
        game: "rF2".into(),
        car: "GT3".into(),
        track: "Spa".into(),
    };
    let mut bytes = scope.encode_to_vec();
    // Append unknown field tag=50, wire type length-delimited
    bytes.extend_from_slice(&[0xF2, 0x03, 0x03, b'f', b'o', b'o']);
    let decoded = proto::ProfileScope::decode(bytes.as_slice())?;
    assert_eq!(decoded.game, "rF2");
    assert_eq!(decoded.car, "GT3");
    assert_eq!(decoded.track, "Spa");
    Ok(())
}

#[test]
fn proto_extra_repeated_field_tolerated() -> TestResult {
    let msg = proto::FeatureNegotiationResponse {
        server_version: "1.0.0".into(),
        supported_features: vec!["device_management".into()],
        enabled_features: vec!["device_management".into()],
        compatible: true,
        min_client_version: "1.0.0".into(),
    };
    let mut bytes = msg.encode_to_vec();
    // Append an unknown field tag=100, wire type varint
    bytes.extend_from_slice(&[0xA0, 0x06, 0x01]);
    let decoded = proto::FeatureNegotiationResponse::decode(bytes.as_slice())?;
    assert!(decoded.compatible);
    assert_eq!(decoded.server_version, "1.0.0");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 3. Default values for missing optional fields
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_device_capabilities_defaults() {
    let caps = proto::DeviceCapabilities::default();
    assert!(!caps.supports_pid);
    assert!(!caps.supports_raw_torque_1khz);
    assert!(!caps.supports_health_stream);
    assert!(!caps.supports_led_bus);
    assert_eq!(caps.max_torque_cnm, 0);
    assert_eq!(caps.encoder_cpr, 0);
    assert_eq!(caps.min_report_period_us, 0);
}

#[test]
fn proto_health_event_defaults() {
    let event = proto::HealthEvent::default();
    assert!(event.device_id.is_empty());
    assert_eq!(event.r#type, 0);
    assert!(event.message.is_empty());
    assert!(event.metadata.is_empty());
    assert!(event.timestamp.is_none());
}

#[test]
fn proto_performance_metrics_defaults() {
    let metrics = proto::PerformanceMetrics::default();
    assert!((metrics.p99_jitter_us - 0.0_f32).abs() < f32::EPSILON);
    assert!((metrics.missed_tick_rate - 0.0_f32).abs() < f32::EPSILON);
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
}

#[test]
fn proto_game_status_defaults() {
    let status = proto::GameStatus::default();
    assert!(status.active_game.is_empty());
    assert!(!status.telemetry_active);
    assert!(status.car_id.is_empty());
    assert!(status.track_id.is_empty());
}

#[test]
fn config_filter_defaults_have_stable_values() {
    let f = ConfigFilterConfig::default();
    assert_eq!(f.reconstruction, 0);
    assert!((f.friction - 0.0).abs() < f32::EPSILON);
    assert!((f.damper - 0.0).abs() < f32::EPSILON);
    assert!((f.inertia - 0.0).abs() < f32::EPSILON);
    assert!(f.bumpstop.enabled);
    assert!((f.bumpstop.strength - 0.5).abs() < f32::EPSILON);
    assert!(f.hands_off.enabled);
    assert!((f.hands_off.sensitivity - 0.3).abs() < f32::EPSILON);
    assert_eq!(f.curve_points.len(), 2);
}

#[test]
fn config_bumpstop_default_matches_serde_default() -> TestResult {
    let json = r#"{}"#;
    let bs: ConfigBumpstopConfig = serde_json::from_str(json)?;
    assert!(bs.enabled);
    assert!((bs.strength - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_hands_off_default_matches_serde_default() -> TestResult {
    let json = r#"{}"#;
    let ho: ConfigHandsOffConfig = serde_json::from_str(json)?;
    assert!(ho.enabled);
    assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 4. Enum evolution: new variants don't break old parsers
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_device_type_unknown_variant_decodes_as_i32() -> TestResult {
    // Simulate a future DeviceType value (e.g., 99 = DEVICE_TYPE_DISPLAY)
    let msg = proto::DeviceInfo {
        id: "display-001".into(),
        name: "Future Display".into(),
        r#type: 99, // Unknown enum variant
        capabilities: None,
        state: 0,
        vendor_id: 0,
        product_id: 0,
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.r#type, 99);
    // proto3 stores enums as i32, unknown values are preserved
    Ok(())
}

#[test]
fn proto_device_state_unknown_variant_preserved() -> TestResult {
    let msg = proto::DeviceInfo {
        id: "dev-x".into(),
        name: "X".into(),
        r#type: 1,
        capabilities: None,
        state: 50, // hypothetical future state
        vendor_id: 0,
        product_id: 0,
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DeviceInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.state, 50);
    Ok(())
}

#[test]
fn proto_health_event_type_unknown_variant_preserved() -> TestResult {
    let msg = proto::HealthEvent {
        timestamp: None,
        device_id: "dev-1".into(),
        r#type: 99, // future health event type
        message: "future event".into(),
        metadata: Default::default(),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::HealthEvent::decode(bytes.as_slice())?;
    assert_eq!(decoded.r#type, 99);
    assert_eq!(decoded.message, "future event");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 5. JSON schema validation against sample payloads
// ──────────────────────────────────────────────────────────────────────

#[test]
fn json_schema_valid_minimal_profile() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = minimal_profile_json();
    let profile = validator.validate_json(&json)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    Ok(())
}

#[test]
fn json_schema_valid_full_profile() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "ACC", "car": "Ferrari 488 GT3", "track": "Monza" },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 720,
            "torqueCapNm": 20.0,
            "filters": {
                "reconstruction": 3,
                "friction": 0.15,
                "damper": 0.25,
                "inertia": 0.1,
                "bumpstop": { "enabled": true, "strength": 0.6 },
                "handsOff": { "enabled": false, "sensitivity": 0.5 },
                "torqueCap": 18.0,
                "notchFilters": [
                    { "hz": 60.0, "q": 2.0, "gainDb": -10.0 }
                ],
                "slewRate": 0.7,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.5, "output": 0.6 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        },
        "leds": {
            "rpmBands": [0.6, 0.7, 0.8, 0.9],
            "pattern": "progressive",
            "brightness": 0.8
        },
        "haptics": {
            "enabled": true,
            "intensity": 0.6,
            "frequencyHz": 150.0
        },
        "signature": "abc123"
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(&json)?;
    assert_eq!(profile.scope.game.as_deref(), Some("ACC"));
    Ok(())
}

#[test]
fn json_schema_rejects_missing_required_field() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "ACC" }
        // missing "base"
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn json_schema_rejects_out_of_range_ffb_gain() -> TestResult {
    let mut json: serde_json::Value = serde_json::from_str(&minimal_profile_json())?;
    json["base"]["ffbGain"] = serde_json::json!(1.5);
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json.to_string());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn json_schema_rejects_wrong_schema_version() -> TestResult {
    let mut json: serde_json::Value = serde_json::from_str(&minimal_profile_json())?;
    json["schema"] = serde_json::json!("wheel.profile/99");
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json.to_string());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn json_schema_rejects_non_monotonic_curve() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "ACC" },
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
                    { "input": 0.8, "output": 0.8 },
                    { "input": 0.5, "output": 0.6 }
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
// 6. Protobuf roundtrip fidelity
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_filter_config_roundtrip() -> TestResult {
    let original = proto::FilterConfig {
        reconstruction: 4,
        friction: 0.15,
        damper: 0.25,
        inertia: 0.1,
        notch_filters: vec![proto::NotchFilter {
            hz: 60.0,
            q: 2.0,
            gain_db: -12.0,
        }],
        slew_rate: 0.7,
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
    let bytes = original.encode_to_vec();
    let decoded = proto::FilterConfig::decode(bytes.as_slice())?;
    assert_eq!(decoded.reconstruction, 4);
    assert!((decoded.friction - 0.15).abs() < f32::EPSILON);
    assert_eq!(decoded.notch_filters.len(), 1);
    assert_eq!(decoded.curve_points.len(), 2);
    Ok(())
}

#[test]
fn proto_diagnostic_info_roundtrip_with_maps() -> TestResult {
    let mut system_info = std::collections::BTreeMap::new();
    system_info.insert("os".to_string(), "windows".to_string());
    system_info.insert("arch".to_string(), "x86_64".to_string());

    let msg = proto::DiagnosticInfo {
        device_id: "dev-diag".into(),
        system_info,
        recent_faults: vec!["fault-1".into(), "fault-2".into()],
        performance: Some(proto::PerformanceMetrics {
            p99_jitter_us: 150.0,
            missed_tick_rate: 0.0001,
            total_ticks: 1_000_000,
            missed_ticks: 100,
        }),
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::DiagnosticInfo::decode(bytes.as_slice())?;
    assert_eq!(decoded.device_id, "dev-diag");
    assert_eq!(decoded.system_info.len(), 2);
    assert_eq!(decoded.recent_faults.len(), 2);
    let perf = decoded.performance.as_ref();
    assert!(perf.is_some());
    Ok(())
}
