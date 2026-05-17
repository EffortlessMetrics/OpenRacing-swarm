//! Deep configuration validation tests for the telemetry-config crate.
//!
//! Covers:
//! 1. Field range validation
//! 2. Invalid value rejection with clear errors
//! 3. Default config validation
//! 4. Serialization/deserialization roundtrip (TOML-like, JSON, YAML)
//! 5. Config merging and override behavior
//! 6. Environment variable overrides
//! 7. Config file format detection
//! 8. Config hot-reload detection
//! 9. Config validation error aggregation
//! 10. Config schema versioning
//! 11. Per-game config overrides
//! 12. Per-device config overrides

use std::collections::HashSet;

use openracing_telemetry_config::{
    AutoDetectConfig, ConfigDiff, DiffOperation, GameSupportMatrix, GameSupportStatus,
    TelemetryConfig, config_writer_factories, load_default_matrix,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. All config fields have valid ranges
// ===========================================================================

mod field_range_validation {
    use super::*;

    #[test]
    fn update_rate_hz_boundary_zero() -> TestResult {
        let json = r#"{
            "enabled": false,
            "update_rate_hz": 0,
            "output_method": "none",
            "output_target": "",
            "fields": []
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert_eq!(cfg.update_rate_hz, 0);
        Ok(())
    }

    #[test]
    fn update_rate_hz_boundary_max_u32() -> TestResult {
        let json = format!(
            r#"{{
                "enabled": true,
                "update_rate_hz": {},
                "output_method": "shared_memory",
                "output_target": "local",
                "fields": ["rpm"]
            }}"#,
            u32::MAX
        );
        let cfg: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(cfg.update_rate_hz, u32::MAX);
        Ok(())
    }

    #[test]
    fn negative_update_rate_rejected_in_json() {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": -1,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn update_rate_overflow_rejected() {
        let json = format!(
            r#"{{
                "enabled": true,
                "update_rate_hz": {},
                "output_method": "udp",
                "output_target": "127.0.0.1:9999",
                "fields": []
            }}"#,
            u64::from(u32::MAX) + 1
        );
        let result = serde_json::from_str::<TelemetryConfig>(&json);
        assert!(result.is_err());
    }

    #[test]
    fn matrix_update_rates_within_realistic_range() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            let rate = game.telemetry.update_rate_hz;
            // All games should have rate <= 1000 (1kHz) for standard, or a known set
            assert!(
                rate <= 1000,
                "game {} has unrealistically high update_rate_hz: {}",
                id,
                rate
            );
        }
        Ok(())
    }

    #[test]
    fn matrix_high_rate_values_are_greater_than_base_rate() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if let Some(high_rate) = game.telemetry.high_rate_update_rate_hz {
                assert!(
                    high_rate > game.telemetry.update_rate_hz,
                    "game {} high_rate {} should exceed base rate {}",
                    id,
                    high_rate,
                    game.telemetry.update_rate_hz
                );
            }
        }
        Ok(())
    }

    #[test]
    fn fields_list_can_hold_many_entries() -> TestResult {
        let many_fields: Vec<String> = (0..100).map(|i| format!("field_{i}")).collect();
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:9999".to_string(),
            fields: many_fields.clone(),
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.fields.len(), 100);
        Ok(())
    }

    #[test]
    fn output_target_with_high_port_number() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:65535".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "127.0.0.1:65535");
        Ok(())
    }
}

// ===========================================================================
// 2. Invalid values are rejected with clear error messages
// ===========================================================================

mod invalid_value_rejection {
    use super::*;

    #[test]
    fn null_enabled_rejected() -> TestResult {
        let json = r#"{
            "enabled": null,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
        let err_msg = format!("{}", result.as_ref().err().ok_or("expected error")?);
        assert!(
            !err_msg.is_empty(),
            "error message should be non-empty for null enabled"
        );
        Ok(())
    }

    #[test]
    fn string_for_numeric_field_rejected_with_message() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": "fast",
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
        let err_msg = format!("{}", result.as_ref().err().ok_or("expected error")?);
        assert!(
            err_msg.contains("invalid type"),
            "expected 'invalid type' in error, got: {}",
            err_msg
        );
        Ok(())
    }

    #[test]
    fn float_for_u32_field_rejected() {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60.5,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn fields_as_string_instead_of_array_rejected() {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": "rpm"
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn game_support_status_invalid_variant_rejected() {
        let json = r#""unknown_status""#;
        let result = serde_json::from_str::<GameSupportStatus>(json);
        assert!(result.is_err());
    }

    #[test]
    fn diff_operation_invalid_variant_rejected() {
        let json = r#""Delete""#;
        let result = serde_json::from_str::<DiffOperation>(json);
        assert!(result.is_err());
    }

    #[test]
    fn yaml_wrong_type_for_update_rate_rejected() {
        let yaml = r#"
enabled: true
update_rate_hz: "sixty"
output_method: udp
output_target: "127.0.0.1:9999"
fields: []
"#;
        let result = serde_yaml::from_str::<TelemetryConfig>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn extra_unknown_field_accepted_by_default() -> TestResult {
        // serde by default ignores unknown fields (no deny_unknown_fields)
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": [],
            "unknown_field": "ignored"
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(cfg.enabled);
        Ok(())
    }
}

// ===========================================================================
// 3. Default config passes all validations
// ===========================================================================

mod default_config_validation {
    use super::*;

    #[test]
    fn default_matrix_loads_successfully() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(!matrix.games.is_empty());
        Ok(())
    }

    #[test]
    fn every_game_in_default_matrix_has_valid_structure() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.name.is_empty(), "{id}: name must not be empty");
            assert!(
                !game.versions.is_empty(),
                "{id}: must have at least one version"
            );
            assert!(
                !game.config_writer.is_empty(),
                "{id}: config_writer must not be empty"
            );
            assert!(
                !game.telemetry.method.is_empty(),
                "{id}: telemetry method must not be empty"
            );
            for ver in &game.versions {
                assert!(
                    !ver.version.is_empty(),
                    "{id}: version string must not be empty"
                );
                assert!(
                    !ver.telemetry_method.is_empty(),
                    "{id}: version telemetry_method must not be empty"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn default_matrix_config_writers_all_exist_in_factory() -> TestResult {
        let matrix = load_default_matrix()?;
        let factory_ids: HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        let mut missing = Vec::new();
        for (game_id, game) in &matrix.games {
            if !factory_ids.contains(&*game.config_writer) {
                missing.push(format!("{game_id} -> {}", game.config_writer));
            }
        }
        assert!(
            missing.is_empty(),
            "games referencing missing config_writer factories: {:?}",
            missing
        );
        Ok(())
    }

    #[test]
    fn default_high_rate_iracing_flag_is_false_when_missing() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }
}

// ===========================================================================
// 4. Config serialization/deserialization roundtrip
// ===========================================================================

mod serde_roundtrip {
    use super::*;

    fn sample_config() -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:20778".to_string(),
            fields: vec![
                "ffb_scalar".to_string(),
                "rpm".to_string(),
                "speed_ms".to_string(),
                "gear".to_string(),
            ],
            enable_high_rate_iracing_360hz: true,
        }
    }

    #[test]
    fn json_roundtrip_preserves_all_fields() -> TestResult {
        let cfg = sample_config();
        let json = serde_json::to_string_pretty(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.enabled, cfg.enabled);
        assert_eq!(decoded.update_rate_hz, cfg.update_rate_hz);
        assert_eq!(decoded.output_method, cfg.output_method);
        assert_eq!(decoded.output_target, cfg.output_target);
        assert_eq!(decoded.fields, cfg.fields);
        assert_eq!(
            decoded.enable_high_rate_iracing_360hz,
            cfg.enable_high_rate_iracing_360hz
        );
        Ok(())
    }

    #[test]
    fn yaml_roundtrip_preserves_all_fields() -> TestResult {
        let cfg = sample_config();
        let yaml = serde_yaml::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_yaml::from_str(&yaml)?;
        assert_eq!(decoded.enabled, cfg.enabled);
        assert_eq!(decoded.update_rate_hz, cfg.update_rate_hz);
        assert_eq!(decoded.output_method, cfg.output_method);
        assert_eq!(decoded.output_target, cfg.output_target);
        assert_eq!(decoded.fields, cfg.fields);
        assert_eq!(
            decoded.enable_high_rate_iracing_360hz,
            cfg.enable_high_rate_iracing_360hz
        );
        Ok(())
    }

    #[test]
    fn json_to_yaml_cross_format_roundtrip() -> TestResult {
        let cfg = sample_config();
        let json = serde_json::to_string(&cfg)?;
        let from_json: TelemetryConfig = serde_json::from_str(&json)?;
        let yaml = serde_yaml::to_string(&from_json)?;
        let from_yaml: TelemetryConfig = serde_yaml::from_str(&yaml)?;
        assert_eq!(from_yaml.enabled, cfg.enabled);
        assert_eq!(from_yaml.update_rate_hz, cfg.update_rate_hz);
        assert_eq!(from_yaml.output_method, cfg.output_method);
        assert_eq!(from_yaml.fields, cfg.fields);
        Ok(())
    }

    #[test]
    fn game_support_matrix_json_yaml_cross_roundtrip() -> TestResult {
        let matrix = load_default_matrix()?;
        let json = serde_json::to_string(&matrix)?;
        let from_json: GameSupportMatrix = serde_json::from_str(&json)?;
        let yaml = serde_yaml::to_string(&from_json)?;
        let from_yaml: GameSupportMatrix = serde_yaml::from_str(&yaml)?;
        assert_eq!(matrix.games.len(), from_yaml.games.len());
        Ok(())
    }

    #[test]
    fn config_diff_all_operations_roundtrip() -> TestResult {
        for op in [
            DiffOperation::Add,
            DiffOperation::Modify,
            DiffOperation::Remove,
        ] {
            let diff = ConfigDiff {
                file_path: "test.ini".to_string(),
                section: Some("Section".to_string()),
                key: "key".to_string(),
                old_value: Some("old".to_string()),
                new_value: "new".to_string(),
                operation: op.clone(),
            };
            let json = serde_json::to_string(&diff)?;
            let decoded: ConfigDiff = serde_json::from_str(&json)?;
            assert_eq!(decoded, diff);
        }
        Ok(())
    }

    #[test]
    fn auto_detect_config_roundtrip_with_multiple_entries() -> TestResult {
        let config = AutoDetectConfig {
            process_names: vec!["proc1.exe".to_string(), "proc2.exe".to_string()],
            install_registry_keys: vec![
                "HKCU\\Software\\Game1".to_string(),
                "HKLM\\Software\\Game1".to_string(),
            ],
            install_paths: vec![
                "C:\\Games\\Game1".to_string(),
                "D:\\SteamLibrary\\Game1".to_string(),
            ],
        };
        let json = serde_json::to_string(&config)?;
        let decoded: AutoDetectConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.process_names.len(), 2);
        assert_eq!(decoded.install_registry_keys.len(), 2);
        assert_eq!(decoded.install_paths.len(), 2);
        Ok(())
    }
}

// ===========================================================================
// 5. Config merging and override behavior
// ===========================================================================

mod config_merging {
    use super::*;

    fn base_config() -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string(), "gear".to_string()],
            enable_high_rate_iracing_360hz: false,
        }
    }

    #[test]
    fn merge_override_single_field_update_rate() -> TestResult {
        let base = base_config();
        let mut merged = base.clone();
        merged.update_rate_hz = 360;
        assert_eq!(merged.update_rate_hz, 360);
        assert_eq!(merged.output_method, base.output_method);
        assert_eq!(merged.fields, base.fields);
        Ok(())
    }

    #[test]
    fn merge_override_replaces_fields_list() -> TestResult {
        let mut merged = base_config();
        merged.fields = vec![
            "rpm".to_string(),
            "gear".to_string(),
            "speed_ms".to_string(),
            "ffb_scalar".to_string(),
        ];
        assert_eq!(merged.fields.len(), 4);
        assert!(merged.fields.contains(&"speed_ms".to_string()));
        Ok(())
    }

    #[test]
    fn merge_enable_disable_toggle() -> TestResult {
        let mut cfg = base_config();
        assert!(cfg.enabled);
        cfg.enabled = false;
        assert!(!cfg.enabled);
        cfg.enabled = true;
        assert!(cfg.enabled);
        Ok(())
    }

    #[test]
    fn merge_output_target_override() -> TestResult {
        let mut cfg = base_config();
        cfg.output_target = "192.168.1.100:5300".to_string();
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "192.168.1.100:5300");
        Ok(())
    }

    #[test]
    fn merge_output_method_change_preserves_other_fields() -> TestResult {
        let base = base_config();
        let mut merged = base.clone();
        merged.output_method = "shared_memory".to_string();
        assert_eq!(merged.output_method, "shared_memory");
        assert_eq!(merged.update_rate_hz, base.update_rate_hz);
        assert_eq!(merged.fields, base.fields);
        assert_eq!(merged.enabled, base.enabled);
        Ok(())
    }

    #[test]
    fn merge_enable_high_rate_override() -> TestResult {
        let mut cfg = base_config();
        assert!(!cfg.enable_high_rate_iracing_360hz);
        cfg.enable_high_rate_iracing_360hz = true;
        cfg.update_rate_hz = 360;
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert!(decoded.enable_high_rate_iracing_360hz);
        assert_eq!(decoded.update_rate_hz, 360);
        Ok(())
    }

    #[test]
    fn json_deserialized_overlay_takes_precedence() -> TestResult {
        let _base = base_config();
        let overlay_json = r#"{
            "enabled": false,
            "update_rate_hz": 120,
            "output_method": "shared_memory",
            "output_target": "local",
            "fields": ["ffb_scalar"],
            "enable_high_rate_iracing_360hz": false
        }"#;
        let overlay: TelemetryConfig = serde_json::from_str(overlay_json)?;
        assert!(!overlay.enabled);
        assert_eq!(overlay.update_rate_hz, 120);
        assert_eq!(overlay.output_method, "shared_memory");
        assert_eq!(overlay.fields, vec!["ffb_scalar"]);
        Ok(())
    }
}

// ===========================================================================
// 6. Environment variable overrides
// ===========================================================================

mod env_var_overrides {
    use super::*;

    #[test]
    fn env_override_update_rate_simulation() -> TestResult {
        // Simulate reading an env var to override update_rate
        let env_value = "240";
        let rate: u32 = env_value.parse()?;
        let mut cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        cfg.update_rate_hz = rate;
        assert_eq!(cfg.update_rate_hz, 240);
        Ok(())
    }

    #[test]
    fn env_override_enabled_flag_from_string() -> TestResult {
        for (input, expected) in [("true", true), ("false", false), ("1", false), ("0", false)] {
            let parsed: Result<bool, _> = input.parse();
            if let Ok(val) = parsed {
                assert_eq!(
                    val, expected,
                    "parsing '{}' should give {}",
                    input, expected
                );
            }
        }
        Ok(())
    }

    #[test]
    fn env_override_output_target_applies() -> TestResult {
        let env_target = "10.0.0.1:5555";
        let mut cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        cfg.output_target = env_target.to_string();
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "10.0.0.1:5555");
        Ok(())
    }

    #[test]
    fn env_override_invalid_rate_string_fails_parse() {
        let bad = "not_a_number";
        let result: Result<u32, _> = bad.parse();
        assert!(result.is_err());
    }

    #[test]
    fn env_override_fields_from_comma_separated() -> TestResult {
        let env_fields = "rpm,speed_ms,gear,ffb_scalar";
        let fields: Vec<String> = env_fields
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        let mut cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        cfg.fields = fields;
        assert_eq!(cfg.fields.len(), 4);
        assert!(cfg.fields.contains(&"ffb_scalar".to_string()));
        Ok(())
    }

    #[test]
    fn env_override_empty_string_clears_target() -> TestResult {
        let mut cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        cfg.output_target = String::new();
        assert!(cfg.output_target.is_empty());
        Ok(())
    }
}

// ===========================================================================
// 7. Config file format detection (TOML, JSON, YAML)
// ===========================================================================

mod format_detection {
    use super::*;

    fn detect_format(filename: &str) -> &str {
        if filename.ends_with(".json") {
            "json"
        } else if filename.ends_with(".yaml") || filename.ends_with(".yml") {
            "yaml"
        } else if filename.ends_with(".toml") {
            "toml"
        } else {
            "unknown"
        }
    }

    #[test]
    fn detect_json_extension() {
        assert_eq!(detect_format("config.json"), "json");
        assert_eq!(detect_format("telemetry.json"), "json");
    }

    #[test]
    fn detect_yaml_extensions() {
        assert_eq!(detect_format("config.yaml"), "yaml");
        assert_eq!(detect_format("config.yml"), "yaml");
    }

    #[test]
    fn detect_toml_extension() {
        assert_eq!(detect_format("config.toml"), "toml");
    }

    #[test]
    fn detect_unknown_extension() {
        assert_eq!(detect_format("config.xml"), "unknown");
        assert_eq!(detect_format("config.ini"), "unknown");
        assert_eq!(detect_format("config"), "unknown");
    }

    #[test]
    fn parse_config_from_json_content() -> TestResult {
        let content = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm"]
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(content)?;
        assert!(cfg.enabled);
        Ok(())
    }

    #[test]
    fn parse_config_from_yaml_content() -> TestResult {
        let content = "enabled: true\nupdate_rate_hz: 60\noutput_method: udp\noutput_target: \"127.0.0.1:20777\"\nfields:\n  - rpm\n";
        let cfg: TelemetryConfig = serde_yaml::from_str(content)?;
        assert!(cfg.enabled);
        Ok(())
    }

    #[test]
    fn format_specific_parsing_for_json_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("telemetry.json");
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string_pretty(&cfg)?;
        std::fs::write(&path, &json)?;

        let filename = path
            .file_name()
            .ok_or("no filename")?
            .to_str()
            .ok_or("non-utf8 filename")?;
        let content = std::fs::read_to_string(&path)?;
        let loaded: TelemetryConfig = match detect_format(filename) {
            "json" => serde_json::from_str(&content)?,
            "yaml" => serde_yaml::from_str(&content)?,
            other => return Err(format!("unsupported format: {other}").into()),
        };
        assert_eq!(loaded.update_rate_hz, 60);
        Ok(())
    }

    #[test]
    fn format_specific_parsing_for_yaml_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("telemetry.yaml");
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let yaml = serde_yaml::to_string(&cfg)?;
        std::fs::write(&path, &yaml)?;

        let filename = path
            .file_name()
            .ok_or("no filename")?
            .to_str()
            .ok_or("non-utf8 filename")?;
        let content = std::fs::read_to_string(&path)?;
        let loaded: TelemetryConfig = match detect_format(filename) {
            "yaml" => serde_yaml::from_str(&content)?,
            "json" => serde_json::from_str(&content)?,
            other => return Err(format!("unsupported format: {other}").into()),
        };
        assert_eq!(loaded.update_rate_hz, 120);
        Ok(())
    }
}

// ===========================================================================
// 8. Config hot-reload detection
// ===========================================================================

mod hot_reload_detection {
    use super::*;

    #[test]
    fn detect_change_via_json_content_comparison() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let snapshot_1 = serde_json::to_string(&cfg)?;

        let mut modified = cfg.clone();
        modified.update_rate_hz = 120;
        let snapshot_2 = serde_json::to_string(&modified)?;

        assert_ne!(
            snapshot_1, snapshot_2,
            "snapshots should differ after mutation"
        );
        Ok(())
    }

    #[test]
    fn no_change_when_config_unchanged() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let snapshot_1 = serde_json::to_string(&cfg)?;
        let snapshot_2 = serde_json::to_string(&cfg)?;
        assert_eq!(snapshot_1, snapshot_2);
        Ok(())
    }

    #[test]
    fn hot_reload_file_write_then_read_detects_change() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.json");

        let cfg_v1 = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        std::fs::write(&path, serde_json::to_string(&cfg_v1)?)?;
        let content_v1 = std::fs::read_to_string(&path)?;

        let mut cfg_v2 = cfg_v1.clone();
        cfg_v2.update_rate_hz = 360;
        cfg_v2.enable_high_rate_iracing_360hz = true;
        cfg_v2.fields.push("speed_ms".to_string());
        std::fs::write(&path, serde_json::to_string(&cfg_v2)?)?;
        let content_v2 = std::fs::read_to_string(&path)?;

        assert_ne!(content_v1, content_v2);
        let reloaded: TelemetryConfig = serde_json::from_str(&content_v2)?;
        assert_eq!(reloaded.update_rate_hz, 360);
        assert!(reloaded.enable_high_rate_iracing_360hz);
        assert_eq!(reloaded.fields.len(), 2);
        Ok(())
    }

    #[test]
    fn hot_reload_field_addition_detected() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let before = serde_json::to_string(&cfg)?;

        let mut updated = cfg;
        updated.fields.push("gear".to_string());
        let after = serde_json::to_string(&updated)?;

        assert_ne!(before, after);
        let decoded: TelemetryConfig = serde_json::from_str(&after)?;
        assert_eq!(decoded.fields.len(), 2);
        Ok(())
    }
}

// ===========================================================================
// 9. Config validation error aggregation
// ===========================================================================

mod error_aggregation {
    use super::*;

    /// Validate a config and collect all errors instead of failing on first.
    fn validate_telemetry_config(cfg: &TelemetryConfig) -> Vec<String> {
        let mut errors = Vec::new();

        if cfg.enabled && cfg.output_method.is_empty() {
            errors.push("enabled config must have a non-empty output_method".to_string());
        }
        if cfg.enabled && cfg.output_target.is_empty() && cfg.output_method == "udp" {
            errors.push("UDP output_method requires a non-empty output_target".to_string());
        }
        if cfg.update_rate_hz > 1000 && cfg.output_method == "udp" {
            errors.push(format!(
                "update_rate_hz {} exceeds UDP practical limit of 1000",
                cfg.update_rate_hz
            ));
        }
        if cfg.enable_high_rate_iracing_360hz && cfg.update_rate_hz < 360 {
            errors
                .push("enable_high_rate_iracing_360hz is set but update_rate_hz < 360".to_string());
        }

        errors
    }

    #[test]
    fn valid_config_has_no_errors() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn disabled_config_has_no_errors_even_with_empty_method() {
        let cfg = TelemetryConfig {
            enabled: false,
            update_rate_hz: 0,
            output_method: String::new(),
            output_target: String::new(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(errors.is_empty());
    }

    #[test]
    fn enabled_with_empty_method_produces_error() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: String::new(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let errors = validate_telemetry_config(&cfg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("output_method"));
    }

    #[test]
    fn udp_with_empty_target_produces_error() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: String::new(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(errors.iter().any(|e| e.contains("output_target")));
    }

    #[test]
    fn high_rate_with_low_rate_produces_error() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "local".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: true,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(errors.iter().any(|e| e.contains("360")));
    }

    #[test]
    fn multiple_errors_aggregated() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: String::new(),
            output_target: String::new(),
            fields: vec![],
            enable_high_rate_iracing_360hz: true,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(
            errors.len() >= 2,
            "expected at least 2 errors, got {}: {:?}",
            errors.len(),
            errors
        );
    }

    #[test]
    fn excessive_udp_rate_produces_warning() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 5000,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let errors = validate_telemetry_config(&cfg);
        assert!(errors.iter().any(|e| e.contains("exceeds")));
    }
}

// ===========================================================================
// 10. Config schema versioning
// ===========================================================================

mod schema_versioning {
    use super::*;

    #[test]
    fn v1_schema_without_high_rate_field_deserializes() -> TestResult {
        // Simulates a v1 config that predates enable_high_rate_iracing_360hz
        let v1_json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm"]
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(v1_json)?;
        assert!(!cfg.enable_high_rate_iracing_360hz);
        assert_eq!(cfg.update_rate_hz, 60);
        Ok(())
    }

    #[test]
    fn v2_schema_with_high_rate_field_deserializes() -> TestResult {
        let v2_json = r#"{
            "enabled": true,
            "update_rate_hz": 360,
            "output_method": "shared_memory",
            "output_target": "127.0.0.1:12345",
            "fields": ["rpm", "ffb_scalar"],
            "enable_high_rate_iracing_360hz": true
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(v2_json)?;
        assert!(cfg.enable_high_rate_iracing_360hz);
        assert_eq!(cfg.update_rate_hz, 360);
        Ok(())
    }

    #[test]
    fn forward_compat_unknown_fields_ignored() -> TestResult {
        // Future schema might add new fields; current parser should not break
        let future_json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm"],
            "enable_high_rate_iracing_360hz": false,
            "schema_version": 3,
            "new_feature_flag": true,
            "latency_budget_us": 500
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(future_json)?;
        assert!(cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 60);
        Ok(())
    }

    #[test]
    fn game_support_matrix_schema_preserves_all_keys_across_formats() -> TestResult {
        let matrix = load_default_matrix()?;
        let game_keys: HashSet<String> = matrix.games.keys().cloned().collect();

        // JSON roundtrip
        let json = serde_json::to_string(&matrix)?;
        let from_json: GameSupportMatrix = serde_json::from_str(&json)?;
        let json_keys: HashSet<String> = from_json.games.keys().cloned().collect();
        assert_eq!(game_keys, json_keys);

        // YAML roundtrip
        let yaml = serde_yaml::to_string(&matrix)?;
        let from_yaml: GameSupportMatrix = serde_yaml::from_str(&yaml)?;
        let yaml_keys: HashSet<String> = from_yaml.games.keys().cloned().collect();
        assert_eq!(game_keys, yaml_keys);

        Ok(())
    }

    #[test]
    fn minimal_v1_yaml_config_deserializes() -> TestResult {
        let yaml = r#"
enabled: true
update_rate_hz: 60
output_method: udp
output_target: "127.0.0.1:20777"
fields:
  - rpm
"#;
        let cfg: TelemetryConfig = serde_yaml::from_str(yaml)?;
        assert!(!cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }
}

// ===========================================================================
// 11. Per-game config overrides
// ===========================================================================

mod per_game_overrides {
    use super::*;

    #[test]
    fn iracing_config_writer_produces_expected_diffs_enabled() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let factories = config_writer_factories();
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| *id == "iracing")
            .ok_or("iracing factory not found")?;
        let writer = factory();
        let diffs = writer.get_expected_diffs(&cfg)?;
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.key == "telemetryDiskFile"));
        assert!(diffs.iter().all(|d| d.new_value == "1"));
        Ok(())
    }

    #[test]
    fn iracing_config_writer_with_360hz_produces_extra_diff() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 360,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: true,
        };
        let factories = config_writer_factories();
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| *id == "iracing")
            .ok_or("iracing factory not found")?;
        let writer = factory();
        let diffs = writer.get_expected_diffs(&cfg)?;
        assert!(
            diffs.len() >= 2,
            "expected at least 2 diffs for 360hz, got {}",
            diffs.len()
        );
        assert!(
            diffs.iter().any(|d| d.key.contains("360")),
            "expected a diff key containing '360'"
        );
        Ok(())
    }

    #[test]
    fn iracing_disabled_produces_diff_with_value_zero() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: false,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let factories = config_writer_factories();
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| *id == "iracing")
            .ok_or("iracing factory not found")?;
        let writer = factory();
        let diffs = writer.get_expected_diffs(&cfg)?;
        assert!(
            diffs
                .iter()
                .any(|d| d.key == "telemetryDiskFile" && d.new_value == "0"),
            "disabled config should produce telemetryDiskFile=0"
        );
        Ok(())
    }

    #[test]
    fn every_factory_writer_get_expected_diffs_succeeds() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let diffs = writer.get_expected_diffs(&cfg)?;
            assert!(
                !diffs.is_empty(),
                "config writer '{}' returned empty diffs",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn per_game_telemetry_method_matches_writer_id() -> TestResult {
        let matrix = load_default_matrix()?;
        let factory_ids: HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for (game_id, game) in &matrix.games {
            assert!(
                factory_ids.contains(&*game.config_writer),
                "game {} writer '{}' not in factory list",
                game_id,
                game.config_writer
            );
        }
        Ok(())
    }

    #[test]
    fn game_specific_update_rates_are_consistent() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.telemetry.supports_360hz_option {
                let high = game
                    .telemetry
                    .high_rate_update_rate_hz
                    .ok_or(format!("{id}: 360hz option but no high_rate value"))?;
                assert!(
                    high > game.telemetry.update_rate_hz,
                    "{}: high rate {} must exceed base {}",
                    id,
                    high,
                    game.telemetry.update_rate_hz
                );
            }
        }
        Ok(())
    }
}

// ===========================================================================
// 12. Per-device config overrides
// ===========================================================================

mod per_device_overrides {
    use super::*;

    /// Simulated per-device config: device-specific rate/target overlays.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    struct DeviceOverride {
        device_id: String,
        update_rate_hz: Option<u32>,
        output_target: Option<String>,
        enable_high_rate: Option<bool>,
    }

    fn apply_device_override(base: &TelemetryConfig, device: &DeviceOverride) -> TelemetryConfig {
        let mut merged = base.clone();
        if let Some(rate) = device.update_rate_hz {
            merged.update_rate_hz = rate;
        }
        if let Some(ref target) = device.output_target {
            merged.output_target = target.clone();
        }
        if let Some(high_rate) = device.enable_high_rate {
            merged.enable_high_rate_iracing_360hz = high_rate;
        }
        merged
    }

    #[test]
    fn device_override_applies_rate() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let device = DeviceOverride {
            device_id: "fanatec_dd1".to_string(),
            update_rate_hz: Some(360),
            output_target: None,
            enable_high_rate: Some(true),
        };
        let result = apply_device_override(&base, &device);
        assert_eq!(result.update_rate_hz, 360);
        assert!(result.enable_high_rate_iracing_360hz);
        assert_eq!(result.output_target, base.output_target);
        Ok(())
    }

    #[test]
    fn device_override_applies_target() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let device = DeviceOverride {
            device_id: "moza_r9".to_string(),
            update_rate_hz: None,
            output_target: Some("192.168.1.50:5300".to_string()),
            enable_high_rate: None,
        };
        let result = apply_device_override(&base, &device);
        assert_eq!(result.output_target, "192.168.1.50:5300");
        assert_eq!(result.update_rate_hz, base.update_rate_hz);
        Ok(())
    }

    #[test]
    fn device_override_with_no_overrides_returns_base() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let device = DeviceOverride {
            device_id: "generic".to_string(),
            update_rate_hz: None,
            output_target: None,
            enable_high_rate: None,
        };
        let result = apply_device_override(&base, &device);
        assert_eq!(result.update_rate_hz, base.update_rate_hz);
        assert_eq!(result.output_target, base.output_target);
        assert_eq!(
            result.enable_high_rate_iracing_360hz,
            base.enable_high_rate_iracing_360hz
        );
        Ok(())
    }

    #[test]
    fn multiple_device_overrides_last_wins() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let overrides = vec![
            DeviceOverride {
                device_id: "device_a".to_string(),
                update_rate_hz: Some(120),
                output_target: None,
                enable_high_rate: None,
            },
            DeviceOverride {
                device_id: "device_b".to_string(),
                update_rate_hz: Some(240),
                output_target: Some("10.0.0.1:8888".to_string()),
                enable_high_rate: None,
            },
        ];
        let mut result = base;
        for dev in &overrides {
            result = apply_device_override(&result, dev);
        }
        assert_eq!(result.update_rate_hz, 240);
        assert_eq!(result.output_target, "10.0.0.1:8888");
        Ok(())
    }

    #[test]
    fn device_override_serializes_correctly() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let device = DeviceOverride {
            device_id: "simucube_2_pro".to_string(),
            update_rate_hz: Some(360),
            output_target: Some("127.0.0.1:33740".to_string()),
            enable_high_rate: Some(true),
        };
        let result = apply_device_override(&base, &device);
        let json = serde_json::to_string(&result)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.update_rate_hz, 360);
        assert_eq!(decoded.output_target, "127.0.0.1:33740");
        assert!(decoded.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn per_device_per_game_stacked_overrides() -> TestResult {
        // Base game config from matrix
        let matrix = load_default_matrix()?;
        let iracing = matrix.games.get("iracing").ok_or("iracing not found")?;

        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: iracing.telemetry.update_rate_hz,
            output_method: iracing.telemetry.method.clone(),
            output_target: iracing
                .telemetry
                .output_target
                .clone()
                .ok_or("no output_target for iracing")?,
            fields: vec!["rpm".to_string(), "ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        // Device override for a high-end wheel
        let device = DeviceOverride {
            device_id: "fanatec_dd2".to_string(),
            update_rate_hz: Some(360),
            output_target: None,
            enable_high_rate: Some(true),
        };
        let result = apply_device_override(&base, &device);
        assert_eq!(result.update_rate_hz, 360);
        assert!(result.enable_high_rate_iracing_360hz);
        assert_eq!(result.output_method, "shared_memory");
        Ok(())
    }
}
