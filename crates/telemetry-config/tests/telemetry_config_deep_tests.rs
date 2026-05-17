//! Deep tests for telemetry configuration parsing, per-game config overrides,
//! rate limiting config, output target config, and config validation/defaults.

use std::collections::HashSet;

use openracing_telemetry_config::{
    AutoDetectConfig, ConfigDiff, DiffOperation, GameSupportMatrix, GameSupportStatus, GameVersion,
    TelemetryConfig, TelemetryFieldMapping, TelemetrySupport, config_writer_factories,
    load_default_matrix, matrix_game_id_set, matrix_game_ids, normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. Telemetry configuration parsing
// ===========================================================================

mod telemetry_config_parsing {
    use super::*;

    #[test]
    fn parse_minimal_json_with_defaults() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": ["rpm"]
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 60);
        assert_eq!(cfg.fields.len(), 1);
        assert!(!cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn parse_full_json_with_all_fields_populated() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 360,
            "output_method": "shared_memory",
            "output_target": "127.0.0.1:12345",
            "fields": ["ffb_scalar","rpm","speed_ms","slip_ratio","gear","flags","car_id","track_id"],
            "enable_high_rate_iracing_360hz": true
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert_eq!(cfg.update_rate_hz, 360);
        assert_eq!(cfg.output_method, "shared_memory");
        assert_eq!(cfg.fields.len(), 8);
        assert!(cfg.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn parse_yaml_preserves_field_order() -> TestResult {
        let yaml = "
enabled: false
update_rate_hz: 120
output_method: udp_broadcast
output_target: \"192.168.1.255:9999\"
fields:
  - rpm
  - gear
  - speed_ms
enable_high_rate_iracing_360hz: false
";
        let cfg: TelemetryConfig = serde_yaml::from_str(yaml)?;
        assert!(!cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 120);
        assert_eq!(cfg.output_method, "udp_broadcast");
        assert_eq!(cfg.output_target, "192.168.1.255:9999");
        assert_eq!(cfg.fields, vec!["rpm", "gear", "speed_ms"]);
        Ok(())
    }

    #[test]
    fn json_to_yaml_cross_format_round_trip() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 200,
            output_method: "udp".to_string(),
            output_target: "10.0.0.1:5050".to_string(),
            fields: vec!["rpm".to_string(), "gear".to_string()],
            enable_high_rate_iracing_360hz: true,
        };
        let json = serde_json::to_string(&cfg)?;
        let from_json: TelemetryConfig = serde_json::from_str(&json)?;
        let yaml = serde_yaml::to_string(&from_json)?;
        let from_yaml: TelemetryConfig = serde_yaml::from_str(&yaml)?;
        assert_eq!(from_yaml.enabled, cfg.enabled);
        assert_eq!(from_yaml.update_rate_hz, cfg.update_rate_hz);
        assert_eq!(from_yaml.output_method, cfg.output_method);
        assert_eq!(from_yaml.output_target, cfg.output_target);
        assert_eq!(from_yaml.fields, cfg.fields);
        assert_eq!(
            from_yaml.enable_high_rate_iracing_360hz,
            cfg.enable_high_rate_iracing_360hz
        );
        Ok(())
    }

    #[test]
    fn reject_missing_required_fields() {
        let json = r#"{"enabled": true, "update_rate_hz": 60}"#;
        assert!(serde_json::from_str::<TelemetryConfig>(json).is_err());
    }

    #[test]
    fn reject_wrong_type_for_enabled() {
        let json = r#"{
            "enabled": "not_a_bool",
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": []
        }"#;
        assert!(serde_json::from_str::<TelemetryConfig>(json).is_err());
    }

    #[test]
    fn reject_wrong_type_for_update_rate() {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": "sixty",
            "output_method": "udp",
            "output_target": "127.0.0.1:20777",
            "fields": []
        }"#;
        assert!(serde_json::from_str::<TelemetryConfig>(json).is_err());
    }

    #[test]
    fn reject_invalid_json_syntax() {
        assert!(serde_json::from_str::<TelemetryConfig>("{broken").is_err());
    }

    #[test]
    fn reject_invalid_yaml_syntax() {
        assert!(serde_yaml::from_str::<TelemetryConfig>("enabled: [[[").is_err());
    }

    #[test]
    fn reject_empty_json() {
        assert!(serde_json::from_str::<TelemetryConfig>("").is_err());
    }

    #[test]
    fn reject_empty_yaml() {
        assert!(serde_yaml::from_str::<TelemetryConfig>("").is_err());
    }

    #[test]
    fn reject_malformed_game_support_matrix() {
        let bad = "games:\n  iracing: 42";
        assert!(serde_yaml::from_str::<GameSupportMatrix>(bad).is_err());
    }

    #[test]
    fn config_diff_all_operations_round_trip() -> TestResult {
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
                operation: op,
            };
            let json = serde_json::to_string(&diff)?;
            let decoded: ConfigDiff = serde_json::from_str(&json)?;
            assert_eq!(decoded, diff);
        }
        Ok(())
    }

    #[test]
    fn disabled_config_with_empty_fields() -> TestResult {
        let json = r#"{
            "enabled": false,
            "update_rate_hz": 0,
            "output_method": "",
            "output_target": "",
            "fields": []
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!cfg.enabled);
        assert_eq!(cfg.update_rate_hz, 0);
        assert!(cfg.output_method.is_empty());
        assert!(cfg.fields.is_empty());
        Ok(())
    }

    #[test]
    fn game_support_matrix_embedded_yaml_parses() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(!matrix.games.is_empty());
        for (id, game) in &matrix.games {
            assert!(!id.is_empty());
            assert!(!game.name.is_empty());
            assert!(!game.versions.is_empty());
        }
        Ok(())
    }

    #[test]
    fn telemetry_config_clone_preserves_all_fields() {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 240,
            output_method: "udp".to_string(),
            output_target: "10.0.0.1:8080".to_string(),
            fields: vec![
                "rpm".to_string(),
                "gear".to_string(),
                "speed_ms".to_string(),
            ],
            enable_high_rate_iracing_360hz: true,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.enabled, cfg.enabled);
        assert_eq!(cloned.update_rate_hz, cfg.update_rate_hz);
        assert_eq!(cloned.output_method, cfg.output_method);
        assert_eq!(cloned.output_target, cfg.output_target);
        assert_eq!(cloned.fields, cfg.fields);
        assert_eq!(
            cloned.enable_high_rate_iracing_360hz,
            cfg.enable_high_rate_iracing_360hz
        );
    }

    #[test]
    fn many_fields_round_trip() -> TestResult {
        let fields: Vec<String> = (0..100).map(|i| format!("field_{i}")).collect();
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: fields.clone(),
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.fields.len(), 100);
        assert_eq!(decoded.fields, fields);
        Ok(())
    }
}

// ===========================================================================
// 2. Per-game config overrides
// ===========================================================================

mod per_game_overrides {
    use super::*;

    #[test]
    fn iracing_uses_shared_memory() -> TestResult {
        let matrix = load_default_matrix()?;
        let game = matrix.games.get("iracing").ok_or("iracing not found")?;
        assert_eq!(game.telemetry.method, "shared_memory");
        assert_eq!(game.status, GameSupportStatus::Stable);
        Ok(())
    }

    #[test]
    fn iracing_supports_360hz_with_high_rate_set() -> TestResult {
        let matrix = load_default_matrix()?;
        let game = matrix.games.get("iracing").ok_or("iracing not found")?;
        assert!(game.telemetry.supports_360hz_option);
        assert_eq!(game.telemetry.high_rate_update_rate_hz, Some(360));
        Ok(())
    }

    #[test]
    fn iracing_has_ffb_and_rpm_field_mappings() -> TestResult {
        let matrix = load_default_matrix()?;
        let game = matrix.games.get("iracing").ok_or("iracing not found")?;
        assert!(game.telemetry.fields.ffb_scalar.is_some());
        assert!(game.telemetry.fields.rpm.is_some());
        Ok(())
    }

    #[test]
    fn acc_is_stable() -> TestResult {
        let matrix = load_default_matrix()?;
        let game = matrix.games.get("acc").ok_or("acc not found")?;
        assert_eq!(game.status, GameSupportStatus::Stable);
        assert!(!game.telemetry.method.is_empty());
        Ok(())
    }

    #[test]
    fn forza_uses_udp_based_method() -> TestResult {
        let matrix = load_default_matrix()?;
        let game = matrix
            .games
            .get("forza_motorsport")
            .ok_or("forza_motorsport not found")?;
        assert!(
            game.telemetry.method.contains("udp"),
            "expected udp method, got '{}'",
            game.telemetry.method
        );
        assert!(game.telemetry.update_rate_hz > 0);
        Ok(())
    }

    #[test]
    fn each_game_version_has_non_empty_telemetry_method() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            for ver in &game.versions {
                assert!(
                    !ver.telemetry_method.is_empty(),
                    "game {} version {} has empty telemetry_method",
                    id,
                    ver.version
                );
            }
        }
        Ok(())
    }

    #[test]
    fn game_specific_ffb_field_names_differ() -> TestResult {
        let matrix = load_default_matrix()?;
        let iracing = matrix.games.get("iracing").ok_or("iracing not found")?;
        let acc = matrix.games.get("acc").ok_or("acc not found")?;

        if let (Some(ir_ffb), Some(acc_ffb)) = (
            &iracing.telemetry.fields.ffb_scalar,
            &acc.telemetry.fields.ffb_scalar,
        ) {
            assert_ne!(
                ir_ffb, acc_ffb,
                "iracing and acc should map ffb_scalar to different game-specific names"
            );
        }
        Ok(())
    }

    #[test]
    fn stable_games_have_auto_detect_process_names() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status == GameSupportStatus::Stable {
                assert!(
                    !game.auto_detect.process_names.is_empty(),
                    "stable game {} should have auto-detect process names",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn every_game_config_writer_has_a_factory() -> TestResult {
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
    fn normalize_game_id_aliases() {
        assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
        assert_eq!(normalize_game_id("iracing"), "iracing");
        assert_eq!(normalize_game_id("acc"), "acc");
        assert_eq!(normalize_game_id(""), "");
    }

    #[test]
    fn each_game_serializes_to_json_individually() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            let json = serde_json::to_string(game)?;
            assert!(!json.is_empty(), "game {} produced empty JSON", id);
            let _decoded: openracing_telemetry_config::GameSupport = serde_json::from_str(&json)?;
        }
        Ok(())
    }

    #[test]
    fn per_game_process_names_are_non_empty_strings() -> TestResult {
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
}

// ===========================================================================
// 3. Rate limiting config
// ===========================================================================

mod rate_limiting_config {
    use super::*;

    #[test]
    fn common_update_rates_are_representable() -> TestResult {
        for rate in [30, 60, 120, 240, 360, 1000] {
            let json = format!(
                r#"{{"enabled":true,"update_rate_hz":{},"output_method":"udp","output_target":"127.0.0.1:20777","fields":["rpm"]}}"#,
                rate
            );
            let cfg: TelemetryConfig = serde_json::from_str(&json)?;
            assert_eq!(cfg.update_rate_hz, rate);
        }
        Ok(())
    }

    #[test]
    fn zero_rate_allowed_for_disabled_config() -> TestResult {
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
    fn max_u32_rate_is_representable() -> TestResult {
        let json = format!(
            r#"{{"enabled":true,"update_rate_hz":{},"output_method":"udp","output_target":"127.0.0.1:20777","fields":[]}}"#,
            u32::MAX
        );
        let cfg: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(cfg.update_rate_hz, u32::MAX);
        Ok(())
    }

    #[test]
    fn high_rate_360hz_defaults_false_when_omitted() -> TestResult {
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

    #[test]
    fn high_rate_360hz_explicitly_enabled() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 360,
            "output_method": "shared_memory",
            "output_target": "127.0.0.1:12345",
            "fields": ["ffb_scalar"],
            "enable_high_rate_iracing_360hz": true
        }"#;
        let cfg: TelemetryConfig = serde_json::from_str(json)?;
        assert!(cfg.enable_high_rate_iracing_360hz);
        assert_eq!(cfg.update_rate_hz, 360);
        Ok(())
    }

    #[test]
    fn games_with_360hz_have_high_rate_exceeding_base() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.telemetry.supports_360hz_option {
                let high_rate = game
                    .telemetry
                    .high_rate_update_rate_hz
                    .ok_or(format!("game {id} missing high_rate_update_rate_hz"))?;
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
    fn stable_games_with_telemetry_have_positive_rate() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status == GameSupportStatus::Stable && game.telemetry.method != "none" {
                assert!(
                    game.telemetry.update_rate_hz > 0,
                    "stable game {} should have positive update_rate_hz",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn rate_field_survives_json_yaml_conversion() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 500,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let from_json: TelemetryConfig = serde_json::from_str(&json)?;
        let yaml = serde_yaml::to_string(&from_json)?;
        let from_yaml: TelemetryConfig = serde_yaml::from_str(&yaml)?;
        assert_eq!(from_yaml.update_rate_hz, 500);
        Ok(())
    }

    #[test]
    fn no_game_has_360hz_without_the_flag() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.telemetry.high_rate_update_rate_hz.is_some() {
                assert!(
                    game.telemetry.supports_360hz_option,
                    "game {} has high_rate_update_rate_hz but supports_360hz_option is false",
                    id
                );
            }
        }
        Ok(())
    }
}

// ===========================================================================
// 4. Output target config
// ===========================================================================

mod output_target_config {
    use super::*;

    #[test]
    fn ipv4_loopback_target_round_trip() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "127.0.0.1:20777");
        Ok(())
    }

    #[test]
    fn ipv6_loopback_target_round_trip() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "[::1]:9999".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "[::1]:9999");
        Ok(())
    }

    #[test]
    fn broadcast_address_target() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp_broadcast".to_string(),
            output_target: "192.168.1.255:5050".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "192.168.1.255:5050");
        assert_eq!(decoded.output_method, "udp_broadcast");
        Ok(())
    }

    #[test]
    fn empty_target_for_non_network_method() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: String::new(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert!(decoded.output_target.is_empty());
        Ok(())
    }

    #[test]
    fn high_port_number_target() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:65535".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        let parts: Vec<&str> = decoded.output_target.rsplitn(2, ':').collect();
        let port: u16 = parts[0].parse()?;
        assert_eq!(port, 65535);
        Ok(())
    }

    #[test]
    fn low_port_number_target() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:1".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert!(decoded.output_target.ends_with(":1"));
        Ok(())
    }

    #[test]
    fn udp_games_in_matrix_have_port_in_target() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.telemetry.method.contains("udp")
                && let Some(target) = &game.telemetry.output_target
            {
                assert!(
                    target.contains(':'),
                    "game {} UDP target '{}' should have host:port format",
                    id,
                    target
                );
            }
        }
        Ok(())
    }

    #[test]
    fn at_least_one_game_uses_shared_memory() -> TestResult {
        let matrix = load_default_matrix()?;
        let count = matrix
            .games
            .values()
            .filter(|g| g.telemetry.method == "shared_memory")
            .count();
        assert!(count > 0, "at least one game should use shared_memory");
        Ok(())
    }

    #[test]
    fn output_target_with_lan_address() -> TestResult {
        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "10.0.0.50:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "10.0.0.50:20777");
        Ok(())
    }

    #[test]
    fn output_method_is_preserved_through_yaml() -> TestResult {
        for method in ["udp", "udp_broadcast", "shared_memory", "none"] {
            let cfg = TelemetryConfig {
                enabled: true,
                update_rate_hz: 60,
                output_method: method.to_string(),
                output_target: "127.0.0.1:20777".to_string(),
                fields: vec![],
                enable_high_rate_iracing_360hz: false,
            };
            let yaml = serde_yaml::to_string(&cfg)?;
            let decoded: TelemetryConfig = serde_yaml::from_str(&yaml)?;
            assert_eq!(decoded.output_method, method);
        }
        Ok(())
    }
}

// ===========================================================================
// 5. Config validation and defaults
// ===========================================================================

mod config_validation_defaults {
    use super::*;

    #[test]
    fn game_support_status_default_is_stable() {
        assert_eq!(GameSupportStatus::default(), GameSupportStatus::Stable);
    }

    #[test]
    fn game_support_status_serde_round_trip() -> TestResult {
        for status in [GameSupportStatus::Stable, GameSupportStatus::Experimental] {
            let json = serde_json::to_string(&status)?;
            let decoded: GameSupportStatus = serde_json::from_str(&json)?;
            assert_eq!(decoded, status);
        }
        Ok(())
    }

    #[test]
    fn matrix_game_ids_are_sorted() -> TestResult {
        let ids = matrix_game_ids()?;
        assert!(
            ids.windows(2).all(|w| w[0] <= w[1]),
            "game ids must be sorted"
        );
        Ok(())
    }

    #[test]
    fn matrix_game_id_set_matches_vec_length() -> TestResult {
        let ids_vec = matrix_game_ids()?;
        let ids_set = matrix_game_id_set()?;
        assert_eq!(ids_vec.len(), ids_set.len());
        for id in &ids_vec {
            assert!(ids_set.contains(id), "set missing id: {}", id);
        }
        Ok(())
    }

    #[test]
    fn matrix_has_minimum_15_games() -> TestResult {
        let ids = matrix_game_ids()?;
        assert!(ids.len() >= 15, "expected >=15 games, got {}", ids.len());
        Ok(())
    }

    #[test]
    fn stable_and_experimental_partition_all_games() -> TestResult {
        let matrix = load_default_matrix()?;
        let stable: HashSet<String> = matrix.stable_games().into_iter().collect();
        let experimental: HashSet<String> = matrix.experimental_games().into_iter().collect();
        let union: HashSet<String> = stable.union(&experimental).cloned().collect();
        let all: HashSet<String> = matrix.games.keys().cloned().collect();
        assert_eq!(union, all, "stable + experimental must cover all games");
        let intersection: HashSet<String> = stable.intersection(&experimental).cloned().collect();
        assert!(
            intersection.is_empty(),
            "no game should be both stable and experimental: {:?}",
            intersection
        );
        Ok(())
    }

    #[test]
    fn each_game_has_name_versions_config_writer_method() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.name.is_empty(), "game {} has empty name", id);
            assert!(!game.versions.is_empty(), "game {} has no versions", id);
            assert!(
                !game.config_writer.is_empty(),
                "game {} has empty config_writer",
                id
            );
            assert!(
                !game.telemetry.method.is_empty(),
                "game {} has empty telemetry method",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn telemetry_field_mapping_all_none_round_trip() -> TestResult {
        let mapping = TelemetryFieldMapping {
            ffb_scalar: None,
            rpm: None,
            speed_ms: None,
            slip_ratio: None,
            gear: None,
            flags: None,
            car_id: None,
            track_id: None,
        };
        let json = serde_json::to_string(&mapping)?;
        let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
        assert!(decoded.ffb_scalar.is_none());
        assert!(decoded.rpm.is_none());
        assert!(decoded.speed_ms.is_none());
        assert!(decoded.slip_ratio.is_none());
        assert!(decoded.gear.is_none());
        assert!(decoded.flags.is_none());
        assert!(decoded.car_id.is_none());
        assert!(decoded.track_id.is_none());
        Ok(())
    }

    #[test]
    fn telemetry_field_mapping_partial_fields_round_trip() -> TestResult {
        let mapping = TelemetryFieldMapping {
            ffb_scalar: Some("SteeringForce".to_string()),
            rpm: Some("EngineRPM".to_string()),
            speed_ms: None,
            slip_ratio: None,
            gear: Some("CurrentGear".to_string()),
            flags: None,
            car_id: None,
            track_id: None,
        };
        let json = serde_json::to_string(&mapping)?;
        let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
        assert_eq!(decoded.ffb_scalar, mapping.ffb_scalar);
        assert_eq!(decoded.rpm, mapping.rpm);
        assert!(decoded.speed_ms.is_none());
        assert_eq!(decoded.gear, mapping.gear);
        Ok(())
    }

    #[test]
    fn auto_detect_config_round_trip() -> TestResult {
        let cfg = AutoDetectConfig {
            process_names: vec!["game.exe".to_string(), "game_dx11.exe".to_string()],
            install_registry_keys: vec!["HKCU\\Software\\Game".to_string()],
            install_paths: vec!["C:\\Games\\MyGame".to_string()],
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: AutoDetectConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.process_names, cfg.process_names);
        assert_eq!(decoded.install_registry_keys, cfg.install_registry_keys);
        assert_eq!(decoded.install_paths, cfg.install_paths);
        Ok(())
    }

    #[test]
    fn game_version_round_trip() -> TestResult {
        let ver = GameVersion {
            version: "2024.x".to_string(),
            config_paths: vec!["Documents/game/config.ini".to_string()],
            executable_patterns: vec!["game*.exe".to_string()],
            telemetry_method: "shared_memory".to_string(),
            supported_fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        };
        let json = serde_json::to_string(&ver)?;
        let decoded: GameVersion = serde_json::from_str(&json)?;
        assert_eq!(decoded.version, ver.version);
        assert_eq!(decoded.config_paths, ver.config_paths);
        assert_eq!(decoded.telemetry_method, ver.telemetry_method);
        assert_eq!(decoded.supported_fields, ver.supported_fields);
        Ok(())
    }

    #[test]
    fn telemetry_support_optional_fields_default() -> TestResult {
        let json = r#"{
            "method": "udp",
            "update_rate_hz": 60,
            "output_target": null,
            "fields": {
                "ffb_scalar": null, "rpm": null, "speed_ms": null,
                "slip_ratio": null, "gear": null, "flags": null,
                "car_id": null, "track_id": null
            }
        }"#;
        let decoded: TelemetrySupport = serde_json::from_str(json)?;
        assert!(!decoded.supports_360hz_option);
        assert!(decoded.high_rate_update_rate_hz.is_none());
        assert!(decoded.output_target.is_none());
        Ok(())
    }

    #[test]
    fn config_writer_factory_ids_are_unique() {
        let mut seen = HashSet::new();
        for (id, _) in config_writer_factories() {
            assert!(seen.insert(id), "duplicate factory id: {}", id);
        }
    }

    #[test]
    fn all_config_writer_factories_instantiate_successfully() {
        for (id, factory) in config_writer_factories() {
            let _writer = factory();
            assert!(!id.is_empty());
        }
    }

    #[test]
    fn has_game_id_correctness() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(matrix.has_game_id("iracing"));
        assert!(matrix.has_game_id("acc"));
        assert!(!matrix.has_game_id("nonexistent_game_xyz"));
        Ok(())
    }

    #[test]
    fn game_ids_method_matches_keys_sorted() -> TestResult {
        let matrix = load_default_matrix()?;
        let ids = matrix.game_ids();
        let mut keys: Vec<String> = matrix.games.keys().cloned().collect();
        keys.sort_unstable();
        assert_eq!(ids, keys);
        Ok(())
    }

    #[test]
    fn stable_games_have_at_least_one_field_mapped() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status != GameSupportStatus::Stable || game.telemetry.method == "none" {
                continue;
            }
            let f = &game.telemetry.fields;
            let has_any = f.ffb_scalar.is_some()
                || f.rpm.is_some()
                || f.speed_ms.is_some()
                || f.slip_ratio.is_some()
                || f.gear.is_some()
                || f.flags.is_some()
                || f.car_id.is_some()
                || f.track_id.is_some();
            assert!(
                has_any,
                "stable game {} should have at least one field mapped",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn config_diff_equality_and_inequality() {
        let diff1 = ConfigDiff {
            file_path: "a.ini".to_string(),
            section: Some("S".to_string()),
            key: "k".to_string(),
            old_value: None,
            new_value: "v".to_string(),
            operation: DiffOperation::Add,
        };
        let diff2 = diff1.clone();
        assert_eq!(diff1, diff2);

        let diff3 = ConfigDiff {
            operation: DiffOperation::Remove,
            ..diff1.clone()
        };
        assert_ne!(diff1, diff3);
    }

    #[test]
    fn game_support_matrix_json_round_trip() -> TestResult {
        let matrix = load_default_matrix()?;
        let json = serde_json::to_string(&matrix)?;
        let decoded: GameSupportMatrix = serde_json::from_str(&json)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        for key in matrix.games.keys() {
            assert!(
                decoded.games.contains_key(key),
                "lost game '{}' in round-trip",
                key
            );
        }
        Ok(())
    }

    #[test]
    fn game_support_matrix_yaml_round_trip() -> TestResult {
        let matrix = load_default_matrix()?;
        let yaml = serde_yaml::to_string(&matrix)?;
        let decoded: GameSupportMatrix = serde_yaml::from_str(&yaml)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        Ok(())
    }

    #[test]
    fn expected_core_games_present() -> TestResult {
        let ids = matrix_game_ids()?;
        for game in [
            "iracing", "acc", "f1_25", "eawrc", "ams2", "rfactor2", "dirt5",
        ] {
            assert!(
                ids.contains(&game.to_string()),
                "missing core game: {}",
                game
            );
        }
        Ok(())
    }

    #[test]
    fn each_game_version_has_non_empty_version_string() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            for ver in &game.versions {
                assert!(
                    !ver.version.is_empty(),
                    "game {} has empty version string",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn config_diff_vec_round_trip() -> TestResult {
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
    fn file_based_config_write_and_reload() -> TestResult {
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

        let content = std::fs::read_to_string(&path)?;
        let loaded: TelemetryConfig = serde_json::from_str(&content)?;
        assert_eq!(loaded.update_rate_hz, 60);
        assert_eq!(loaded.output_target, "127.0.0.1:20777");

        // Simulate hot-reload: modify and re-read
        let mut modified = loaded.clone();
        modified.update_rate_hz = 360;
        modified.enable_high_rate_iracing_360hz = true;
        std::fs::write(&path, serde_json::to_string_pretty(&modified)?)?;

        let reloaded: TelemetryConfig = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        assert_eq!(reloaded.update_rate_hz, 360);
        assert!(reloaded.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn yaml_file_based_config_write_and_reload() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("telemetry.yaml");

        let cfg = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        std::fs::write(&path, serde_yaml::to_string(&cfg)?)?;

        let loaded: TelemetryConfig = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
        assert_eq!(loaded.update_rate_hz, 120);

        let mut modified = loaded.clone();
        modified.update_rate_hz = 240;
        std::fs::write(&path, serde_yaml::to_string(&modified)?)?;

        let reloaded: TelemetryConfig = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
        assert_eq!(reloaded.update_rate_hz, 240);
        Ok(())
    }
}
