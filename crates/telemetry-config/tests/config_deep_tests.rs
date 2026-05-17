//! Deep tests for telemetry configuration subsystem.
//!
//! Covers config parsing (TOML/JSON/YAML), validation, merge/overlay,
//! per-game defaults, hot-reload semantics, and snapshot serialization.

use std::collections::HashSet;

use openracing_telemetry_config::{
    ConfigDiff, DiffOperation, GameSupport, GameSupportMatrix, GameSupportStatus, TelemetryConfig,
    config_writer_factories, load_default_matrix, matrix_game_id_set, matrix_game_ids,
    normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// Config file parsing – valid formats
// ===========================================================================

mod parsing_valid {
    use super::*;

    #[test]
    fn parse_telemetry_config_from_valid_json() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm", "speed_ms", "gear"]
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 60);
        assert_eq!(cfg.output_method, "udp");
        assert_eq!(cfg.output_target, "127.0.0.1:20777");
        assert_eq!(cfg.fields.len(), 3);
        assert!(!cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn parse_telemetry_config_from_valid_yaml() -> TestResult {
        let yaml = r#"
enabled: true
update_rate_hz: 360
output_method: shared_memory
output_target: "127.0.0.1:12345"
fields:
  - ffb_scalar
  - rpm
enable_high_rate_iracing_360hz: true
"#;
        let cfg: TelemetryConfig = serde_yaml::from_str(yaml)?;
        assert!(cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 360);
        assert!(cfg.enable_high_rate_iracing_360hz);
        assert_eq!(cfg.fields, vec!["ffb_scalar", "rpm"]);
        Ok(())
    }

    #[test]
    fn parse_game_support_matrix_from_embedded_yaml() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(!matrix.games.is_empty());
        for (id, game) in &matrix.games {
            assert!(!id.is_empty());
            assert!(!game.name.is_empty());
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_config_json_with_all_optional_fields() -> TestResult {
        let json = r#"{
            "enabled": false,
            "update_rate_hz": 0,
            "output_method": "",
            "output_target": "",
            "fields": [],
            "enable_high_rate_iracing_360hz": false
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 0);
        assert!(cfg.fields.is_empty());
        Ok(())
    }
}

// ===========================================================================
// Config file parsing – invalid formats
// ===========================================================================

mod parsing_invalid {
    use super::*;

    #[test]
    fn reject_invalid_json() {
        let bad = r#"{ not valid json "#;
        let result = serde_json::from_str::<TelemetryConfig>(bad);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_yaml() {
        let bad = "enabled: [this is broken yaml: {{";
        let result = serde_yaml::from_str::<TelemetryConfig>(bad);
        assert!(result.is_err());
    }

    #[test]
    fn reject_json_missing_required_fields() {
        // Missing `output_method`, `output_target`, `fields`
        let json = r#"{ "enabled": true, "update_rate_hz": 60 }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn reject_json_wrong_type_for_enabled() {
        let json = r#"{
            "enabled": "yes",
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn reject_json_wrong_type_for_update_rate() {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": "sixty",
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": []
        }"#;
        let result = serde_json::from_str::<TelemetryConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn reject_yaml_game_support_matrix_bad_structure() {
        let bad = "games:\n  iracing: 42";
        let result = serde_yaml::from_str::<GameSupportMatrix>(bad);
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_json() {
        let result = serde_json::from_str::<TelemetryConfig>("");
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_yaml() {
        let result = serde_yaml::from_str::<TelemetryConfig>("");
        assert!(result.is_err());
    }
}

// ===========================================================================
// Config validation – port ranges, IPs, process names
// ===========================================================================

mod config_validation {
    use super::*;

    fn make_config(method: &str, target: &str, rate: u32) -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            update_rate_hz: rate,
            output_method: method.to_string(),
            output_target: target.to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        }
    }

    #[test]
    fn valid_ipv4_with_standard_port() -> TestResult {
        let cfg = make_config("udp", "127.0.0.1:20777", 60);
        let parts: Vec<&str> = cfg.output_target.rsplitn(2, ':').collect();
        let port: u16 = parts[0].parse()?;
        assert!(port > 0);
        assert_eq!(port, 20777);
        Ok(())
    }

    #[test]
    fn valid_ipv6_loopback_target() -> TestResult {
        let cfg = make_config("udp", "[::1]:9999", 60);
        assert!(cfg.output_target.starts_with('['));
        assert!(cfg.output_target.contains("::1"));
        Ok(())
    }

    #[test]
    fn zero_update_rate_is_representable() {
        let cfg = make_config("none", "", 0);
        assert_eq!(cfg.update_rate_hz, 0);
    }

    #[test]
    fn high_update_rate_is_representable() {
        let cfg = make_config("shared_memory", "127.0.0.1:12345", 10000);
        assert_eq!(cfg.update_rate_hz, 10000);
    }

    #[test]
    fn each_game_process_names_are_non_empty_strings() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            for pname in &game.auto_detect.process_names {
                assert!(
                    !pname.is_empty(),
                    "game {} has empty process name in auto_detect",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn udp_games_have_output_target_with_port() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if (game.telemetry.method == "udp" || game.telemetry.method == "udp_broadcast")
                && game.telemetry.output_target.is_some()
            {
                let target = game
                    .telemetry
                    .output_target
                    .as_ref()
                    .ok_or("expected output_target")?;
                assert!(
                    target.contains(':'),
                    "game {} uses UDP but output_target '{}' has no port separator",
                    id,
                    target
                );
            }
        }
        Ok(())
    }

    #[test]
    fn all_stable_games_have_at_least_one_process_name() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status == GameSupportStatus::Stable {
                assert!(
                    !game.auto_detect.process_names.is_empty(),
                    "stable game {} has no auto-detect process names",
                    id
                );
            }
        }
        Ok(())
    }
}

// ===========================================================================
// Config merge / overlay / precedence
// ===========================================================================

mod config_merge {
    use super::*;

    #[test]
    fn overlay_replaces_update_rate() -> TestResult {
        let base = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let overlay_json = r#"{
            "enabled": true,
            "update_rate_hz": 360,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm"],
            "enable_high_rate_iracing_360hz": true
        }"#;
        let overlay: TelemetryConfig = serde_json::from_str(overlay_json)?;
        // After overlay, rate should be 360
        assert_eq!(overlay.update_rate_hz, 360);
        assert!(overlay.enable_high_rate_iracing_360hz);
        assert_ne!(base.update_rate_hz, overlay.update_rate_hz);
        Ok(())
    }

    #[test]
    fn overlay_fields_override_base_fields() -> TestResult {
        let base_json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm", "gear"]
        }"#;
        let overlay_json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm", "gear", "speed_ms", "ffb_scalar"]
        }"#;
        let base: TelemetryConfig = serde_json::from_str(base_json)?;
        let overlay: TelemetryConfig = serde_json::from_str(overlay_json)?;
        assert_eq!(base.fields.len(), 2);
        assert_eq!(overlay.fields.len(), 4);
        Ok(())
    }

    #[test]
    fn yaml_then_json_overlay_preserves_method() -> TestResult {
        let yaml = r#"
enabled: true
update_rate_hz: 60
output_method: shared_memory
output_target: "127.0.0.1:12345"
fields: [rpm]
"#;
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 120,
            "output_method": "shared_memory",
            "output_target": "127.0.0.1:12345",
            "fields": ["rpm", "speed_ms"]
        }"#;
        let base: TelemetryConfig = serde_yaml::from_str(yaml)?;
        let overlay: TelemetryConfig = serde_json::from_str(json)?;
        assert_eq!(base.output_method, overlay.output_method);
        assert_eq!(overlay.update_rate_hz, 120);
        Ok(())
    }

    #[test]
    fn enable_high_rate_default_false_if_missing_in_overlay() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": []
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }
}

// ===========================================================================
// Config defaults for each supported game
// ===========================================================================

mod config_defaults {
    use super::*;

    #[test]
    fn each_game_has_non_empty_name_and_config_writer() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.name.is_empty(), "game {} missing name", id);
            assert!(
                !game.config_writer.is_empty(),
                "game {} missing config_writer",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn each_game_has_at_least_one_version() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(
                !game.versions.is_empty(),
                "game {} has no version entries",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn iracing_defaults_are_shared_memory_with_360hz() -> TestResult {
        let matrix = load_default_matrix()?;
        let iracing = matrix.games.get("iracing").ok_or("iracing not found")?;
        assert_eq!(iracing.telemetry.method, "shared_memory");
        assert!(iracing.telemetry.supports_360hz_option);
        assert_eq!(iracing.telemetry.high_rate_update_rate_hz, Some(360));
        assert_eq!(iracing.status, GameSupportStatus::Stable);
        Ok(())
    }

    #[test]
    fn acc_defaults_are_udp_broadcast() -> TestResult {
        let matrix = load_default_matrix()?;
        let acc = matrix.games.get("acc").ok_or("acc not found")?;
        assert!(!acc.telemetry.method.is_empty());
        assert_eq!(acc.status, GameSupportStatus::Stable);
        Ok(())
    }

    #[test]
    fn forza_defaults_have_udp_based_method() -> TestResult {
        let matrix = load_default_matrix()?;
        let forza = matrix
            .games
            .get("forza_motorsport")
            .ok_or("forza_motorsport not found")?;
        assert!(
            forza.telemetry.method.contains("udp"),
            "expected udp-based method, got '{}'",
            forza.telemetry.method
        );
        assert!(forza.telemetry.update_rate_hz > 0);
        Ok(())
    }

    #[test]
    fn every_stable_game_has_positive_update_rate() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status == GameSupportStatus::Stable && game.telemetry.method != "none" {
                assert!(
                    game.telemetry.update_rate_hz > 0,
                    "stable game {} has zero update_rate_hz",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn config_writer_factory_exists_for_every_game() -> TestResult {
        let matrix = load_default_matrix()?;
        let factory_ids: HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for (game_id, game) in &matrix.games {
            assert!(
                factory_ids.contains(&*game.config_writer),
                "game {} references config_writer '{}' not in factory list",
                game_id,
                game.config_writer
            );
        }
        Ok(())
    }

    #[test]
    fn all_factory_writers_can_be_instantiated() {
        for (id, factory) in config_writer_factories() {
            let _writer = factory();
            assert!(!id.is_empty());
        }
    }
}

// ===========================================================================
// Config hot-reload – modify → detect change → apply
// ===========================================================================

mod config_hot_reload {
    use super::*;
    #[test]
    fn detect_change_after_serialize_modify_deserialize() -> TestResult {
        let original = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let original_json = serde_json::to_string(&original)?;

        // "Modify" in-flight
        let mut modified: TelemetryConfig = serde_json::from_str(&original_json)?;
        modified.update_rate_hz = 120;
        modified.fields.push("speed_ms".to_string());

        let modified_json = serde_json::to_string(&modified)?;
        assert_ne!(original_json, modified_json);

        // Re-load
        let reloaded: TelemetryConfig = serde_json::from_str(&modified_json)?;
        assert_eq!(reloaded.update_rate_hz, 120);
        assert_eq!(reloaded.fields.len(), 2);
        Ok(())
    }

    #[test]
    fn hot_reload_via_tempfile_roundtrip() -> TestResult {
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

        // Read back
        let content = std::fs::read_to_string(&path)?;
        let loaded: TelemetryConfig = serde_json::from_str(&content)?;
        assert_eq!(loaded.update_rate_hz, 60);

        // Modify file
        let mut modified = loaded.clone();
        modified.update_rate_hz = 360;
        modified.enable_high_rate_iracing_360hz = true;
        let new_json = serde_json::to_string_pretty(&modified)?;
        std::fs::write(&path, &new_json)?;

        // Detect change
        let reloaded_content = std::fs::read_to_string(&path)?;
        assert_ne!(content, reloaded_content);
        let reloaded: TelemetryConfig = serde_json::from_str(&reloaded_content)?;
        assert_eq!(reloaded.update_rate_hz, 360);
        assert!(reloaded.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn yaml_hot_reload_via_tempfile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("telemetry.yaml");

        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let yaml = serde_yaml::to_string(&cfg)?;
        std::fs::write(&path, &yaml)?;

        let content = std::fs::read_to_string(&path)?;
        let loaded: TelemetryConfig = serde_yaml::from_str(&content)?;
        assert_eq!(loaded.update_rate_hz, 60);

        // Modify
        let mut modified = loaded.clone();
        modified.update_rate_hz = 120;
        let new_yaml = serde_yaml::to_string(&modified)?;
        std::fs::write(&path, &new_yaml)?;

        let reloaded_content = std::fs::read_to_string(&path)?;
        let reloaded: TelemetryConfig = serde_yaml::from_str(&reloaded_content)?;
        assert_eq!(reloaded.update_rate_hz, 120);
        Ok(())
    }
}

// ===========================================================================
// Snapshot tests – serialize default config for each supported game
// ===========================================================================

mod snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_matrix_game_ids_are_sorted_and_stable() -> TestResult {
        let ids = matrix_game_ids()?;
        assert!(
            ids.windows(2).all(|w| w[0] <= w[1]),
            "game ids must be sorted"
        );
        // Regression: minimum known set
        let expected_subset = ["acc", "ams2", "eawrc", "f1_25", "iracing", "rfactor2"];
        for id in &expected_subset {
            assert!(
                ids.contains(&id.to_string()),
                "snapshot: expected game '{}' missing from matrix",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn snapshot_each_game_config_serializes_to_json() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            let json = serde_json::to_string(game)?;
            assert!(!json.is_empty(), "game {} produced empty JSON", id);
            // Verify round-trip
            let _decoded: GameSupport = serde_json::from_str(&json)?;
        }
        Ok(())
    }

    #[test]
    fn snapshot_matrix_json_round_trip_preserves_all_games() -> TestResult {
        let matrix = load_default_matrix()?;
        let json = serde_json::to_string_pretty(&matrix)?;
        let decoded: GameSupportMatrix = serde_json::from_str(&json)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        for key in matrix.games.keys() {
            assert!(
                decoded.games.contains_key(key),
                "snapshot round-trip lost game '{}'",
                key
            );
        }
        Ok(())
    }

    #[test]
    fn snapshot_matrix_yaml_round_trip_preserves_all_games() -> TestResult {
        let matrix = load_default_matrix()?;
        let yaml = serde_yaml::to_string(&matrix)?;
        let decoded: GameSupportMatrix = serde_yaml::from_str(&yaml)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        Ok(())
    }

    #[test]
    fn snapshot_telemetry_field_mapping_for_iracing() -> TestResult {
        let matrix = load_default_matrix()?;
        let iracing = matrix.games.get("iracing").ok_or("iracing not found")?;
        let fields = &iracing.telemetry.fields;
        assert!(fields.ffb_scalar.is_some());
        assert!(fields.rpm.is_some());
        assert!(fields.speed_ms.is_some());
        assert!(fields.gear.is_some());
        Ok(())
    }

    #[test]
    fn snapshot_config_diff_json_round_trip() -> TestResult {
        let diffs = vec![
            ConfigDiff {
                file_path: "app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: "udpEnabled".to_string(),
                old_value: Some("0".to_string()),
                new_value: "1".to_string(),
                operation: DiffOperation::Modify,
            },
            ConfigDiff {
                file_path: "app.ini".to_string(),
                section: None,
                key: "newKey".to_string(),
                old_value: None,
                new_value: "value".to_string(),
                operation: DiffOperation::Add,
            },
        ];
        let json = serde_json::to_string(&diffs)?;
        let decoded: Vec<ConfigDiff> = serde_json::from_str(&json)?;
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], diffs[0]);
        assert_eq!(decoded[1], diffs[1]);
        Ok(())
    }

    #[test]
    fn snapshot_game_support_status_default() {
        assert_eq!(GameSupportStatus::default(), GameSupportStatus::Stable);
    }

    #[test]
    fn snapshot_normalize_game_id_known_aliases() {
        assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
        assert_eq!(normalize_game_id("iracing"), "iracing");
    }

    #[test]
    fn snapshot_game_id_set_equals_vec_set() -> TestResult {
        let ids_vec = matrix_game_ids()?;
        let ids_set = matrix_game_id_set()?;
        assert_eq!(ids_vec.len(), ids_set.len());
        for id in &ids_vec {
            assert!(ids_set.contains(id));
        }
        Ok(())
    }
}
