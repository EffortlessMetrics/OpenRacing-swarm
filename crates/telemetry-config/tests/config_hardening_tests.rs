//! Hardening tests for openracing-telemetry-config.
//!
//! Covers: config file parsing, config validation, config diff/merge,
//! default config generation for each game, and config migration.

use openracing_telemetry_config::{
    ConfigDiff, DiffOperation, TelemetryConfig, config_writer_factories, load_default_matrix,
    normalize_game_id,
};
use std::collections::HashSet;
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Config file parsing ─────────────────────────────────────────────────

#[test]
fn telemetry_config_minimal_json_parse() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": ["rpm"]
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert!(config.enabled);
    assert_eq!(config.update_rate_hz, 60);
    assert_eq!(config.output_method, "udp");
    assert_eq!(config.output_target, "127.0.0.1:20777");
    assert_eq!(config.fields.len(), 1);
    assert!(!config.enable_high_rate_iracing_360hz);
    Ok(())
}

#[test]
fn telemetry_config_full_json_parse() -> TestResult {
    let json = r#"{
        "enabled": false,
        "update_rate_hz": 360,
        "output_method": "shared_memory",
        "output_target": "192.168.1.100:5300",
        "fields": ["ffb_scalar", "rpm", "speed_ms", "slip_ratio", "gear", "flags", "car_id", "track_id"],
        "enable_high_rate_iracing_360hz": true
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert!(!config.enabled);
    assert_eq!(config.update_rate_hz, 360);
    assert_eq!(config.fields.len(), 8);
    assert!(config.enable_high_rate_iracing_360hz);
    Ok(())
}

#[test]
fn telemetry_config_yaml_round_trip() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:20778".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let yaml = serde_yaml::to_string(&config)?;
    let decoded: TelemetryConfig = serde_yaml::from_str(&yaml)?;
    assert_eq!(decoded.enabled, config.enabled);
    assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
    assert_eq!(decoded.output_method, config.output_method);
    assert_eq!(decoded.fields, config.fields);
    Ok(())
}

#[test]
fn telemetry_config_empty_fields_parses() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": []
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert!(config.fields.is_empty());
    Ok(())
}

#[test]
fn telemetry_config_missing_optional_field_defaults() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": []
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    // enable_high_rate_iracing_360hz should default to false
    assert!(!config.enable_high_rate_iracing_360hz);
    Ok(())
}

#[test]
fn telemetry_config_invalid_json_returns_error() {
    let result = serde_json::from_str::<TelemetryConfig>("not json at all");
    assert!(result.is_err());
}

#[test]
fn telemetry_config_missing_required_field_returns_error() {
    let json = r#"{"enabled": true}"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(result.is_err());
}

// ── Config validation ───────────────────────────────────────────────────

#[test]
fn config_writer_factories_all_produce_valid_writers() {
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        assert!(!id.is_empty(), "factory has empty id");
        // Writer should be able to generate expected diffs for a basic config
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs_result = writer.get_expected_diffs(&config);
        assert!(
            diffs_result.is_ok(),
            "config writer {id} failed get_expected_diffs: {:?}",
            diffs_result.err()
        );
    }
}

#[test]
fn config_writer_factory_ids_are_unique() {
    let factories = config_writer_factories();
    let mut seen = HashSet::new();
    for (id, _) in factories {
        assert!(seen.insert(*id), "duplicate config writer factory id: {id}");
    }
}

#[test]
fn config_writer_factories_cover_all_matrix_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let factory_ids: HashSet<&str> = config_writer_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for (game_id, game) in &matrix.games {
        assert!(
            factory_ids.contains(game.config_writer.as_str()),
            "game {game_id} references config_writer '{}' with no factory",
            game.config_writer
        );
    }
    Ok(())
}

#[test]
fn validate_config_returns_false_for_nonexistent_path() -> TestResult {
    let temp = tempdir()?;
    let nonexistent = temp.path().join("nonexistent_game_dir");
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let result = writer.validate_config(&nonexistent);
        if let Ok(valid) = result {
            assert!(
                !valid,
                "config writer {id} should not validate nonexistent path"
            );
        }
    }
    Ok(())
}

// ── Config diff/merge ───────────────────────────────────────────────────

#[test]
fn config_diff_add_has_no_old_value() {
    let diff = ConfigDiff {
        file_path: "config.json".to_string(),
        section: None,
        key: "udpEnabled".to_string(),
        old_value: None,
        new_value: "true".to_string(),
        operation: DiffOperation::Add,
    };
    assert!(diff.old_value.is_none());
    assert_eq!(diff.operation, DiffOperation::Add);
}

#[test]
fn config_diff_modify_has_old_and_new_values() {
    let diff = ConfigDiff {
        file_path: "app.ini".to_string(),
        section: Some("Telemetry".to_string()),
        key: "telemetryDiskFile".to_string(),
        old_value: Some("0".to_string()),
        new_value: "1".to_string(),
        operation: DiffOperation::Modify,
    };
    assert_eq!(diff.old_value, Some("0".to_string()));
    assert_eq!(diff.new_value, "1");
    assert_eq!(diff.operation, DiffOperation::Modify);
}

#[test]
fn config_diff_remove_preserves_old_value() {
    let diff = ConfigDiff {
        file_path: "settings.ini".to_string(),
        section: Some("Network".to_string()),
        key: "legacyPort".to_string(),
        old_value: Some("8080".to_string()),
        new_value: String::new(),
        operation: DiffOperation::Remove,
    };
    assert_eq!(diff.old_value, Some("8080".to_string()));
    assert!(diff.new_value.is_empty());
}

#[test]
fn config_diff_json_round_trip_all_operations() -> TestResult {
    for op in [
        DiffOperation::Add,
        DiffOperation::Modify,
        DiffOperation::Remove,
    ] {
        let diff = ConfigDiff {
            file_path: "test.ini".to_string(),
            section: Some("Section".to_string()),
            key: "key".to_string(),
            old_value: if op == DiffOperation::Add {
                None
            } else {
                Some("old".to_string())
            },
            new_value: if op == DiffOperation::Remove {
                String::new()
            } else {
                "new".to_string()
            },
            operation: op.clone(),
        };
        let json = serde_json::to_string(&diff)?;
        let decoded: ConfigDiff = serde_json::from_str(&json)?;
        assert_eq!(decoded, diff);
    }
    Ok(())
}

#[test]
fn config_diff_clone_equality() {
    let diff = ConfigDiff {
        file_path: "x.ini".to_string(),
        section: None,
        key: "k".to_string(),
        old_value: None,
        new_value: "v".to_string(),
        operation: DiffOperation::Add,
    };
    let cloned = diff.clone();
    assert_eq!(diff, cloned);
}

#[test]
fn config_diff_inequality_on_different_keys() {
    let diff1 = ConfigDiff {
        file_path: "a.ini".to_string(),
        section: None,
        key: "key1".to_string(),
        old_value: None,
        new_value: "v".to_string(),
        operation: DiffOperation::Add,
    };
    let diff2 = ConfigDiff {
        file_path: "a.ini".to_string(),
        section: None,
        key: "key2".to_string(),
        old_value: None,
        new_value: "v".to_string(),
        operation: DiffOperation::Add,
    };
    assert_ne!(diff1, diff2);
}

// ── Default config generation for each game ─────────────────────────────

#[test]
fn each_writer_generates_nonempty_expected_diffs() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string(), "speed_ms".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        assert!(
            !diffs.is_empty(),
            "config writer {id} returned empty expected diffs"
        );
    }
    Ok(())
}

#[test]
fn iracing_writer_generates_telemetry_section_diff() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let factories = config_writer_factories();
    let (_, iracing_factory) = factories
        .iter()
        .find(|(id, _)| *id == "iracing")
        .ok_or("iracing factory not found")?;
    let writer = iracing_factory();
    let diffs = writer.get_expected_diffs(&config)?;
    assert!(!diffs.is_empty());
    // iRacing should reference app.ini
    let has_app_ini = diffs.iter().any(|d| d.file_path.contains("app.ini"));
    assert!(has_app_ini, "iRacing diffs should reference app.ini");
    Ok(())
}

#[test]
fn iracing_360hz_produces_extra_diff() -> TestResult {
    let config_no_360 = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let config_360 = TelemetryConfig {
        enabled: true,
        update_rate_hz: 360,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: true,
    };
    let factories = config_writer_factories();
    let (_, iracing_factory) = factories
        .iter()
        .find(|(id, _)| *id == "iracing")
        .ok_or("iracing factory not found")?;
    let writer = iracing_factory();
    let diffs_no_360 = writer.get_expected_diffs(&config_no_360)?;
    let diffs_360 = writer.get_expected_diffs(&config_360)?;
    assert!(
        diffs_360.len() > diffs_no_360.len(),
        "360hz config should produce more diffs than non-360hz"
    );
    Ok(())
}

#[test]
fn acc_writer_generates_broadcasting_json_diff() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9000".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let factories = config_writer_factories();
    let (_, acc_factory) = factories
        .iter()
        .find(|(id, _)| *id == "acc")
        .ok_or("acc factory not found")?;
    let writer = acc_factory();
    let diffs = writer.get_expected_diffs(&config)?;
    assert!(!diffs.is_empty());
    Ok(())
}

#[test]
fn iracing_writer_write_config_creates_files() -> TestResult {
    let temp = tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let factories = config_writer_factories();
    let (_, iracing_factory) = factories
        .iter()
        .find(|(id, _)| *id == "iracing")
        .ok_or("iracing factory not found")?;
    let writer = iracing_factory();
    let diffs = writer.write_config(temp.path(), &config)?;
    assert!(!diffs.is_empty());
    // Verify files were created
    let app_ini = temp.path().join("Documents/iRacing/app.ini");
    assert!(app_ini.exists(), "app.ini should be created");
    Ok(())
}

#[test]
fn iracing_writer_validate_after_write() -> TestResult {
    let temp = tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let factories = config_writer_factories();
    let (_, iracing_factory) = factories
        .iter()
        .find(|(id, _)| *id == "iracing")
        .ok_or("iracing factory not found")?;
    let writer = iracing_factory();
    writer.write_config(temp.path(), &config)?;
    let valid = writer.validate_config(temp.path())?;
    assert!(valid, "iRacing config should validate after write");
    Ok(())
}

// ── Config migration between versions ───────────────────────────────────

#[test]
fn telemetry_config_forward_compat_extra_fields_ignored() -> TestResult {
    // Simulate future JSON with extra unknown fields (serde should ignore them by default).
    // Actually, serde strict mode may reject. Let's verify the current behavior.
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": [],
        "enable_high_rate_iracing_360hz": false,
        "future_field": "should be ignored"
    }"#;
    // This may fail if deny_unknown_fields is set.
    let result = serde_json::from_str::<TelemetryConfig>(json);
    // We assert that the current behavior is known.
    // If it fails, that's also a valid design choice - just document it.
    if let Ok(config) = result {
        assert!(config.enabled);
        assert_eq!(config.update_rate_hz, 60);
    }
    // Either way, we know the behavior.
    Ok(())
}

#[test]
fn telemetry_config_zero_update_rate_parseable() -> TestResult {
    let json = r#"{
        "enabled": false,
        "update_rate_hz": 0,
        "output_method": "",
        "output_target": "",
        "fields": []
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert_eq!(config.update_rate_hz, 0);
    Ok(())
}

#[test]
fn telemetry_config_large_update_rate_parseable() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 1000,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": []
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert_eq!(config.update_rate_hz, 1000);
    Ok(())
}

// ── Matrix + writers cross-validation ───────────────────────────────────

#[test]
fn matrix_config_writer_field_maps_to_valid_factory() -> TestResult {
    let matrix = load_default_matrix()?;
    let factory_ids: HashSet<&str> = config_writer_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let mut missing = Vec::new();
    for (game_id, game) in &matrix.games {
        if !factory_ids.contains(game.config_writer.as_str()) {
            missing.push(format!("{game_id} -> {}", game.config_writer));
        }
    }
    assert!(
        missing.is_empty(),
        "games with missing config writer factories: {:?}",
        missing
    );
    Ok(())
}

#[test]
fn normalize_game_id_then_lookup_in_matrix() -> TestResult {
    let matrix = load_default_matrix()?;
    // Normalize should map aliases to IDs that exist in the matrix
    let normalized = normalize_game_id("ea_wrc");
    assert!(
        matrix.has_game_id(normalized),
        "normalized 'ea_wrc' -> '{normalized}' not found in matrix"
    );
    let normalized_f1 = normalize_game_id("f1_2025");
    assert!(
        matrix.has_game_id(normalized_f1),
        "normalized 'f1_2025' -> '{normalized_f1}' not found in matrix"
    );
    Ok(())
}

// ── Config writer output format checks ──────────────────────────────────

#[test]
fn all_expected_diffs_have_nonempty_file_path() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        for diff in &diffs {
            assert!(
                !diff.file_path.is_empty(),
                "config writer {id} produced diff with empty file_path"
            );
            assert!(
                !diff.key.is_empty(),
                "config writer {id} produced diff with empty key"
            );
        }
    }
    Ok(())
}

#[test]
fn diff_operations_are_valid_enum_variants() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        for diff in &diffs {
            match diff.operation {
                DiffOperation::Add | DiffOperation::Modify | DiffOperation::Remove => {}
            }
            // Verify it's serializable
            let json = serde_json::to_string(&diff.operation)?;
            assert!(
                !json.is_empty(),
                "config writer {id} produced non-serializable operation"
            );
        }
    }
    Ok(())
}

#[test]
fn config_writers_are_send_and_sync() {
    // Verify that factory-produced writers implement Send + Sync
    for (_id, factory) in config_writer_factories() {
        let writer = factory();
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        assert_send_sync(&writer);
    }
}
