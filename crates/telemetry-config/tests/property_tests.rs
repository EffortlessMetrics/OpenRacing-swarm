#![allow(clippy::redundant_closure)]
//! Property-based tests for telemetry configuration.
//!
//! Tests config validation round-trips, profile merging via serde, and
//! edge cases including empty config and conflicting settings.

use openracing_telemetry_config::{
    AutoDetectConfig, ConfigDiff, DiffOperation, GameSupportStatus, TelemetryConfig,
    TelemetryFieldMapping, load_default_matrix, matrix_game_ids, normalize_game_id,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Proptest: config validation round-trips
// ---------------------------------------------------------------------------

fn arb_diff_operation() -> impl Strategy<Value = DiffOperation> {
    prop_oneof![
        Just(DiffOperation::Add),
        Just(DiffOperation::Modify),
        Just(DiffOperation::Remove),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// TelemetryConfig JSON round-trip preserves all fields.
    #[test]
    fn prop_telemetry_config_json_roundtrip(
        enabled in any::<bool>(),
        rate in 0u32..=10000u32,
        method in "[a-z_]{1,20}",
        target in "[0-9a-z.:]{1,30}",
        high_rate in any::<bool>(),
        num_fields in 0usize..=5,
    ) {
        let fields: Vec<String> = (0..num_fields).map(|i| format!("field_{i}")).collect();
        let config = TelemetryConfig {
            enabled,
            update_rate_hz: rate,
            output_method: method.clone(),
            output_target: target.clone(),
            fields: fields.clone(),
            enable_high_rate_iracing_360hz: high_rate,
        };
        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("serialize: {e}")))?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("deserialize: {e}")))?;
        prop_assert_eq!(decoded.enabled, enabled);
        prop_assert_eq!(decoded.update_rate_hz, rate);
        prop_assert_eq!(&decoded.output_method, &method);
        prop_assert_eq!(&decoded.output_target, &target);
        prop_assert_eq!(&decoded.fields, &fields);
        prop_assert_eq!(decoded.enable_high_rate_iracing_360hz, high_rate);
    }

    /// TelemetryConfig YAML round-trip preserves all fields.
    #[test]
    fn prop_telemetry_config_yaml_roundtrip(
        enabled in any::<bool>(),
        rate in 0u32..=10000u32,
        high_rate in any::<bool>(),
    ) {
        let config = TelemetryConfig {
            enabled,
            update_rate_hz: rate,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:9999".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: high_rate,
        };
        let yaml = serde_yaml::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("serialize: {e}")))?;
        let decoded: TelemetryConfig = serde_yaml::from_str(&yaml)
            .map_err(|e| TestCaseError::fail(format!("deserialize: {e}")))?;
        prop_assert_eq!(decoded.enabled, enabled);
        prop_assert_eq!(decoded.update_rate_hz, rate);
        prop_assert_eq!(decoded.enable_high_rate_iracing_360hz, high_rate);
    }

    /// ConfigDiff JSON round-trip preserves all fields.
    #[test]
    fn prop_config_diff_json_roundtrip(
        file_path in "[a-z/]{1,30}",
        has_section in any::<bool>(),
        key in "[a-z_]{1,20}",
        has_old in any::<bool>(),
        new_value in "[a-z0-9]{0,20}",
        op in arb_diff_operation(),
    ) {
        let diff = ConfigDiff {
            file_path: file_path.clone(),
            section: if has_section { Some("Section".to_string()) } else { None },
            key: key.clone(),
            old_value: if has_old { Some("old".to_string()) } else { None },
            new_value: new_value.clone(),
            operation: op.clone(),
        };
        let json = serde_json::to_string(&diff)
            .map_err(|e| TestCaseError::fail(format!("serialize: {e}")))?;
        let decoded: ConfigDiff = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("deserialize: {e}")))?;
        prop_assert_eq!(decoded, diff);
    }

    /// GameSupportStatus round-trips through JSON.
    #[test]
    fn prop_game_support_status_roundtrip(
        is_stable in any::<bool>(),
    ) {
        let status = if is_stable {
            GameSupportStatus::Stable
        } else {
            GameSupportStatus::Experimental
        };
        let json = serde_json::to_string(&status)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let decoded: GameSupportStatus = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert_eq!(decoded, status);
    }

    /// TelemetryFieldMapping with varying None/Some patterns round-trips.
    #[test]
    fn prop_field_mapping_roundtrip(
        has_ffb in any::<bool>(),
        has_rpm in any::<bool>(),
        has_speed in any::<bool>(),
        has_gear in any::<bool>(),
    ) {
        let mapping = TelemetryFieldMapping {
            ffb_scalar: if has_ffb { Some("ffb".to_string()) } else { None },
            rpm: if has_rpm { Some("RPM".to_string()) } else { None },
            speed_ms: if has_speed { Some("Speed".to_string()) } else { None },
            slip_ratio: None,
            gear: if has_gear { Some("Gear".to_string()) } else { None },
            flags: None,
            car_id: None,
            track_id: None,
        };
        let json = serde_json::to_string(&mapping)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let decoded: TelemetryFieldMapping = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert_eq!(decoded.ffb_scalar, mapping.ffb_scalar);
        prop_assert_eq!(decoded.rpm, mapping.rpm);
        prop_assert_eq!(decoded.speed_ms, mapping.speed_ms);
        prop_assert_eq!(decoded.gear, mapping.gear);
    }
}

// ---------------------------------------------------------------------------
// Profile merging and inheritance tests
// ---------------------------------------------------------------------------

#[test]
fn profile_merge_override_rate() -> Result<(), Box<dyn std::error::Error>> {
    let base = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9999".to_string(),
        fields: vec!["rpm".to_string(), "speed_ms".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    // Simulate merging by overriding specific fields
    let override_json = r#"{
        "enabled": true,
        "update_rate_hz": 360,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": ["rpm", "speed_ms"],
        "enable_high_rate_iracing_360hz": true
    }"#;
    let merged: TelemetryConfig = serde_json::from_str(override_json)?;
    assert_eq!(merged.update_rate_hz, 360);
    assert!(merged.enable_high_rate_iracing_360hz);
    assert_eq!(merged.output_method, base.output_method);
    Ok(())
}

#[test]
fn profile_merge_add_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut base_fields: Vec<String> = vec!["rpm".to_string()];
    let additional = vec!["speed_ms".to_string(), "gear".to_string()];
    base_fields.extend(additional);
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9999".to_string(),
        fields: base_fields.clone(),
        enable_high_rate_iracing_360hz: false,
    };
    assert_eq!(config.fields.len(), 3);
    assert!(config.fields.contains(&"rpm".to_string()));
    assert!(config.fields.contains(&"speed_ms".to_string()));
    assert!(config.fields.contains(&"gear".to_string()));
    Ok(())
}

#[test]
fn profile_inheritance_matrix_game_has_config_writer() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.config_writer.is_empty(),
            "game {} missing config_writer",
            id
        );
    }
    Ok(())
}

#[test]
fn profile_inheritance_all_games_have_telemetry_method() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.telemetry.method.is_empty(),
            "game {} missing telemetry method",
            id
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases: empty config, conflicting settings
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_config_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let config = TelemetryConfig {
        enabled: false,
        update_rate_hz: 0,
        output_method: String::new(),
        output_target: String::new(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert!(!decoded.enabled);
    assert_eq!(decoded.update_rate_hz, 0);
    assert!(decoded.output_method.is_empty());
    assert!(decoded.fields.is_empty());
    Ok(())
}

#[test]
fn edge_conflicting_360hz_without_iracing() -> Result<(), Box<dyn std::error::Error>> {
    // enable_high_rate_iracing_360hz set for a non-iRacing config
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: true, // conflicting: not iRacing
    };
    // Should still serialize/deserialize without error
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert!(decoded.enable_high_rate_iracing_360hz);
    Ok(())
}

#[test]
fn edge_very_high_update_rate() -> Result<(), Box<dyn std::error::Error>> {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: u32::MAX,
        output_method: "shared_memory".to_string(),
        output_target: "local".to_string(),
        fields: vec!["ffb_scalar".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.update_rate_hz, u32::MAX);
    Ok(())
}

#[test]
fn edge_duplicate_fields() -> Result<(), Box<dyn std::error::Error>> {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9999".to_string(),
        fields: vec!["rpm".to_string(), "rpm".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.fields.len(), 3);
    Ok(())
}

#[test]
fn edge_invalid_json_returns_error() {
    let result = serde_json::from_str::<TelemetryConfig>("not valid json");
    assert!(result.is_err());
}

#[test]
fn edge_missing_required_fields_returns_error() {
    let result = serde_json::from_str::<TelemetryConfig>(r#"{"enabled": true}"#);
    assert!(result.is_err());
}

#[test]
fn edge_normalize_game_id_empty() {
    assert_eq!(normalize_game_id(""), "");
}

#[test]
fn edge_normalize_game_id_unknown_passthrough() {
    assert_eq!(
        normalize_game_id("totally_unknown_game"),
        "totally_unknown_game"
    );
}

#[test]
fn edge_config_diff_empty_new_value() -> Result<(), Box<dyn std::error::Error>> {
    let diff = ConfigDiff {
        file_path: "settings.ini".to_string(),
        section: None,
        key: "someKey".to_string(),
        old_value: Some("oldVal".to_string()),
        new_value: String::new(),
        operation: DiffOperation::Remove,
    };
    let json = serde_json::to_string(&diff)?;
    let decoded: ConfigDiff = serde_json::from_str(&json)?;
    assert!(decoded.new_value.is_empty());
    assert_eq!(decoded.operation, DiffOperation::Remove);
    Ok(())
}

#[test]
fn edge_matrix_game_ids_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let ids1 = matrix_game_ids()?;
    let ids2 = matrix_game_ids()?;
    assert_eq!(ids1, ids2, "matrix_game_ids should be deterministic");
    Ok(())
}

#[test]
fn edge_auto_detect_config_empty_lists() -> Result<(), Box<dyn std::error::Error>> {
    let config = AutoDetectConfig {
        process_names: vec![],
        install_registry_keys: vec![],
        install_paths: vec![],
    };
    let json = serde_json::to_string(&config)?;
    let decoded: AutoDetectConfig = serde_json::from_str(&json)?;
    assert!(decoded.process_names.is_empty());
    assert!(decoded.install_registry_keys.is_empty());
    assert!(decoded.install_paths.is_empty());
    Ok(())
}
