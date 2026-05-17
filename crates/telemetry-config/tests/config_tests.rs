//! Integration tests for the telemetry-config crate.
//!
//! Covers config parsing, validation, game registry, serialization round-trips,
//! default values, and invalid-config handling.

use std::collections::HashSet;

use openracing_telemetry_config::{
    AutoDetectConfig, ConfigDiff, ConfigWriter, DiffOperation, GameSupport, GameSupportMatrix,
    GameSupportStatus, GameVersion, TELEMETRY_SUPPORT_MATRIX_YAML, TelemetryConfig,
    TelemetryFieldMapping, TelemetrySupport, config_writer_factories, load_default_matrix,
    matrix_game_id_set, matrix_game_ids, normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// 1. Config parsing and validation
// ---------------------------------------------------------------------------

#[test]
fn load_default_matrix_parses_embedded_yaml() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(
        !matrix.games.is_empty(),
        "parsed matrix should contain at least one game"
    );
    Ok(())
}

#[test]
fn embedded_yaml_contains_games_key() {
    assert!(
        TELEMETRY_SUPPORT_MATRIX_YAML.contains("games:"),
        "raw YAML must have a top-level 'games:' key"
    );
}

#[test]
fn each_game_entry_has_required_fields() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(!game.name.is_empty(), "game '{}' has empty name", id);
        assert!(!game.versions.is_empty(), "game '{}' has no versions", id);
        assert!(
            !game.config_writer.is_empty(),
            "game '{}' has empty config_writer",
            id
        );
        assert!(
            !game.telemetry.method.is_empty(),
            "game '{}' has empty telemetry method",
            id
        );
    }
    Ok(())
}

#[test]
fn each_game_version_has_non_empty_version_and_method() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            assert!(
                !ver.version.is_empty(),
                "game '{}' has a version entry with empty version string",
                id
            );
            assert!(
                !ver.telemetry_method.is_empty(),
                "game '{}' version '{}' has empty telemetry_method",
                id,
                ver.version
            );
        }
    }
    Ok(())
}

#[test]
fn stable_games_with_telemetry_have_positive_update_rate() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.status == GameSupportStatus::Stable && game.telemetry.method != "none" {
            assert!(
                game.telemetry.update_rate_hz > 0,
                "stable game '{}' should have positive update_rate_hz",
                id
            );
        }
    }
    Ok(())
}

#[test]
fn games_with_360hz_option_declare_high_rate_hz() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.supports_360hz_option {
            assert!(
                game.telemetry.high_rate_update_rate_hz.is_some(),
                "game '{}' supports 360 Hz but has no high_rate_update_rate_hz",
                id
            );
        }
    }
    Ok(())
}

#[test]
fn stable_games_with_telemetry_have_at_least_one_field_mapped() -> TestResult {
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
            "stable game '{}' should have at least one telemetry field mapped",
            id
        );
    }
    Ok(())
}

#[test]
fn config_writer_factory_ids_match_matrix_config_writers() -> TestResult {
    let matrix = load_default_matrix()?;
    let factory_ids: HashSet<&str> = config_writer_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for (game_id, game) in &matrix.games {
        assert!(
            factory_ids.contains(&*game.config_writer),
            "game '{}' references config_writer '{}' with no matching factory",
            game_id,
            game.config_writer
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Game registry completeness
// ---------------------------------------------------------------------------

#[test]
fn game_count_meets_minimum_threshold() -> TestResult {
    let ids = matrix_game_ids()?;
    assert!(
        ids.len() >= 15,
        "expected at least 15 games, got {}",
        ids.len()
    );
    Ok(())
}

#[test]
fn well_known_games_are_present() -> TestResult {
    let ids = matrix_game_id_set()?;
    for expected in [
        "iracing",
        "acc",
        "f1_25",
        "eawrc",
        "ams2",
        "rfactor2",
        "dirt5",
        "forza_motorsport",
        "beamng_drive",
        "gran_turismo_7",
        "assetto_corsa",
        "rbr",
    ] {
        assert!(
            ids.contains(expected),
            "missing well-known game: {}",
            expected
        );
    }
    Ok(())
}

#[test]
fn matrix_game_ids_returns_sorted_vec() -> TestResult {
    let ids = matrix_game_ids()?;
    assert!(
        ids.windows(2).all(|w| w[0] <= w[1]),
        "matrix_game_ids() must return sorted ids"
    );
    Ok(())
}

#[test]
fn matrix_game_id_set_has_no_duplicates() -> TestResult {
    let ids_vec = matrix_game_ids()?;
    let ids_set = matrix_game_id_set()?;
    assert_eq!(
        ids_vec.len(),
        ids_set.len(),
        "game id set and vec lengths differ — duplicates present"
    );
    Ok(())
}

#[test]
fn game_support_matrix_has_game_id_works() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(matrix.has_game_id("iracing"));
    assert!(matrix.has_game_id("acc"));
    assert!(!matrix.has_game_id("__nonexistent__"));
    Ok(())
}

#[test]
fn game_ids_method_matches_keys() -> TestResult {
    let matrix = load_default_matrix()?;
    let ids = matrix.game_ids();
    let mut keys: Vec<String> = matrix.games.keys().cloned().collect();
    keys.sort_unstable();
    assert_eq!(ids, keys);
    Ok(())
}

#[test]
fn stable_and_experimental_partition_all_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable: HashSet<String> = matrix.stable_games().into_iter().collect();
    let experimental: HashSet<String> = matrix.experimental_games().into_iter().collect();
    let all: HashSet<String> = matrix.games.keys().cloned().collect();

    let union: HashSet<String> = stable.union(&experimental).cloned().collect();
    assert_eq!(union, all, "stable ∪ experimental must equal all games");

    let overlap: HashSet<String> = stable.intersection(&experimental).cloned().collect();
    assert!(
        overlap.is_empty(),
        "games should not be both stable and experimental: {:?}",
        overlap
    );
    Ok(())
}

#[test]
fn config_writer_factory_ids_are_unique() {
    let factories = config_writer_factories();
    let mut seen = HashSet::new();
    for (id, _) in factories {
        assert!(
            seen.insert(*id),
            "duplicate config writer factory id: {}",
            id
        );
    }
}

#[test]
fn each_config_writer_factory_produces_a_writer() {
    for (id, factory) in config_writer_factories() {
        let _writer = factory();
        assert!(!id.is_empty(), "factory has empty id");
    }
}

// ---------------------------------------------------------------------------
// 3. Default config values
// ---------------------------------------------------------------------------

#[test]
fn game_support_status_default_is_stable() {
    assert_eq!(GameSupportStatus::default(), GameSupportStatus::Stable);
}

#[test]
fn telemetry_support_optional_fields_default_correctly() -> TestResult {
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
    assert!(
        !decoded.supports_360hz_option,
        "supports_360hz_option should default to false"
    );
    assert!(
        decoded.high_rate_update_rate_hz.is_none(),
        "high_rate_update_rate_hz should default to None"
    );
    Ok(())
}

#[test]
fn telemetry_config_high_rate_defaults_to_false() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": []
    }"#;
    let decoded: TelemetryConfig = serde_json::from_str(json)?;
    assert!(
        !decoded.enable_high_rate_iracing_360hz,
        "enable_high_rate_iracing_360hz should default to false when omitted"
    );
    Ok(())
}

#[test]
fn normalize_game_id_aliases() {
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
    assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    assert_eq!(normalize_game_id("F1_2025"), "f1_25");
}

#[test]
fn normalize_game_id_passthrough_for_unknown_ids() {
    assert_eq!(normalize_game_id("iracing"), "iracing");
    assert_eq!(normalize_game_id("acc"), "acc");
    assert_eq!(normalize_game_id("some_random"), "some_random");
    assert_eq!(normalize_game_id(""), "");
}

// ---------------------------------------------------------------------------
// 4. Config serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn game_support_status_json_round_trip() -> TestResult {
    for status in [GameSupportStatus::Stable, GameSupportStatus::Experimental] {
        let json = serde_json::to_string(&status)?;
        let decoded: GameSupportStatus = serde_json::from_str(&json)?;
        assert_eq!(decoded, status);
    }
    Ok(())
}

#[test]
fn game_support_status_serializes_to_lowercase() -> TestResult {
    assert_eq!(
        serde_json::to_string(&GameSupportStatus::Stable)?,
        r#""stable""#
    );
    assert_eq!(
        serde_json::to_string(&GameSupportStatus::Experimental)?,
        r#""experimental""#
    );
    Ok(())
}

#[test]
fn game_support_matrix_yaml_round_trip() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml_str = serde_yaml::to_string(&matrix)?;
    let decoded: GameSupportMatrix = serde_yaml::from_str(&yaml_str)?;
    assert_eq!(matrix.games.len(), decoded.games.len());
    for key in matrix.games.keys() {
        assert!(decoded.games.contains_key(key), "lost game key: {}", key);
    }
    Ok(())
}

#[test]
fn game_support_matrix_json_round_trip() -> TestResult {
    let matrix = load_default_matrix()?;
    let json_str = serde_json::to_string(&matrix)?;
    let decoded: GameSupportMatrix = serde_json::from_str(&json_str)?;
    assert_eq!(matrix.games.len(), decoded.games.len());
    Ok(())
}

#[test]
fn telemetry_config_json_round_trip() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9999".to_string(),
        fields: vec!["rpm".to_string(), "speed_ms".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.enabled, config.enabled);
    assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
    assert_eq!(decoded.output_method, config.output_method);
    assert_eq!(decoded.output_target, config.output_target);
    assert_eq!(decoded.fields, config.fields);
    assert_eq!(
        decoded.enable_high_rate_iracing_360hz,
        config.enable_high_rate_iracing_360hz
    );
    Ok(())
}

#[test]
fn telemetry_config_yaml_round_trip() -> TestResult {
    let config = TelemetryConfig {
        enabled: false,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:20778".to_string(),
        fields: vec!["ffb_scalar".to_string(), "gear".to_string()],
        enable_high_rate_iracing_360hz: true,
    };
    let yaml_str = serde_yaml::to_string(&config)?;
    let decoded: TelemetryConfig = serde_yaml::from_str(&yaml_str)?;
    assert_eq!(decoded.enabled, config.enabled);
    assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
    assert_eq!(decoded.output_method, config.output_method);
    assert_eq!(decoded.fields, config.fields);
    assert_eq!(
        decoded.enable_high_rate_iracing_360hz,
        config.enable_high_rate_iracing_360hz
    );
    Ok(())
}

#[test]
fn telemetry_field_mapping_round_trip_all_some() -> TestResult {
    let mapping = TelemetryFieldMapping {
        ffb_scalar: Some("SteeringWheelPctTorqueSign".to_string()),
        rpm: Some("RPM".to_string()),
        speed_ms: Some("Speed".to_string()),
        slip_ratio: Some("LFSlipRatio".to_string()),
        gear: Some("Gear".to_string()),
        flags: Some("SessionFlags".to_string()),
        car_id: Some("CarPath".to_string()),
        track_id: Some("TrackName".to_string()),
    };
    let json = serde_json::to_string(&mapping)?;
    let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
    assert_eq!(decoded.ffb_scalar, mapping.ffb_scalar);
    assert_eq!(decoded.rpm, mapping.rpm);
    assert_eq!(decoded.speed_ms, mapping.speed_ms);
    assert_eq!(decoded.slip_ratio, mapping.slip_ratio);
    assert_eq!(decoded.gear, mapping.gear);
    assert_eq!(decoded.flags, mapping.flags);
    assert_eq!(decoded.car_id, mapping.car_id);
    assert_eq!(decoded.track_id, mapping.track_id);
    Ok(())
}

#[test]
fn telemetry_field_mapping_round_trip_all_none() -> TestResult {
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
    Ok(())
}

#[test]
fn auto_detect_config_round_trip() -> TestResult {
    let config = AutoDetectConfig {
        process_names: vec!["iRacingSim64DX11.exe".to_string()],
        install_registry_keys: vec!["HKCU\\Software\\iRacing".to_string()],
        install_paths: vec!["Program Files (x86)/iRacing".to_string()],
    };
    let json = serde_json::to_string(&config)?;
    let decoded: AutoDetectConfig = serde_json::from_str(&json)?;
    assert_eq!(decoded.process_names, config.process_names);
    assert_eq!(decoded.install_registry_keys, config.install_registry_keys);
    assert_eq!(decoded.install_paths, config.install_paths);
    Ok(())
}

#[test]
fn game_version_round_trip() -> TestResult {
    let version = GameVersion {
        version: "2024.x".to_string(),
        config_paths: vec!["Documents/iRacing/app.ini".to_string()],
        executable_patterns: vec!["iRacingSim64DX11.exe".to_string()],
        telemetry_method: "shared_memory".to_string(),
        supported_fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
    };
    let json = serde_json::to_string(&version)?;
    let decoded: GameVersion = serde_json::from_str(&json)?;
    assert_eq!(decoded.version, version.version);
    assert_eq!(decoded.config_paths, version.config_paths);
    assert_eq!(decoded.executable_patterns, version.executable_patterns);
    assert_eq!(decoded.telemetry_method, version.telemetry_method);
    assert_eq!(decoded.supported_fields, version.supported_fields);
    Ok(())
}

#[test]
fn telemetry_support_round_trip() -> TestResult {
    let support = TelemetrySupport {
        method: "shared_memory".to_string(),
        update_rate_hz: 60,
        supports_360hz_option: true,
        high_rate_update_rate_hz: Some(360),
        output_target: Some("127.0.0.1:12345".to_string()),
        fields: TelemetryFieldMapping {
            ffb_scalar: Some("SteeringWheelPctTorqueSign".to_string()),
            rpm: None,
            speed_ms: None,
            slip_ratio: None,
            gear: None,
            flags: None,
            car_id: None,
            track_id: None,
        },
    };
    let json = serde_json::to_string(&support)?;
    let decoded: TelemetrySupport = serde_json::from_str(&json)?;
    assert_eq!(decoded.method, support.method);
    assert_eq!(decoded.update_rate_hz, support.update_rate_hz);
    assert_eq!(decoded.supports_360hz_option, support.supports_360hz_option);
    assert_eq!(
        decoded.high_rate_update_rate_hz,
        support.high_rate_update_rate_hz
    );
    assert_eq!(decoded.output_target, support.output_target);
    Ok(())
}

#[test]
fn config_diff_json_round_trip() -> TestResult {
    let diff = ConfigDiff {
        file_path: "Documents/iRacing/app.ini".to_string(),
        section: Some("Telemetry".to_string()),
        key: "telemetryDiskFile".to_string(),
        old_value: Some("0".to_string()),
        new_value: "1".to_string(),
        operation: DiffOperation::Modify,
    };
    let json = serde_json::to_string(&diff)?;
    let decoded: ConfigDiff = serde_json::from_str(&json)?;
    assert_eq!(decoded, diff);
    Ok(())
}

#[test]
fn diff_operation_all_variants_round_trip() -> TestResult {
    for op in [
        DiffOperation::Add,
        DiffOperation::Modify,
        DiffOperation::Remove,
    ] {
        let json = serde_json::to_string(&op)?;
        let decoded: DiffOperation = serde_json::from_str(&json)?;
        assert_eq!(decoded, op);
    }
    Ok(())
}

#[test]
fn telemetry_config_all_fields_preserved_across_json() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 360,
        output_method: "udp_broadcast".to_string(),
        output_target: "192.168.1.100:5300".to_string(),
        fields: vec![
            "ffb_scalar".to_string(),
            "rpm".to_string(),
            "speed_ms".to_string(),
            "slip_ratio".to_string(),
            "gear".to_string(),
            "flags".to_string(),
            "car_id".to_string(),
            "track_id".to_string(),
        ],
        enable_high_rate_iracing_360hz: true,
    };
    let json = serde_json::to_string(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;
    assert!(decoded.enabled);
    assert_eq!(decoded.update_rate_hz, 360);
    assert_eq!(decoded.output_method, "udp_broadcast");
    assert_eq!(decoded.output_target, "192.168.1.100:5300");
    assert_eq!(decoded.fields.len(), 8);
    assert!(decoded.enable_high_rate_iracing_360hz);
    Ok(())
}

#[test]
fn telemetry_config_empty_fields_round_trip() -> TestResult {
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

// ---------------------------------------------------------------------------
// 5. Invalid config handling
// ---------------------------------------------------------------------------

#[test]
fn invalid_yaml_returns_parse_error() {
    let bad_yaml = "games:\n  - this is not valid: [";
    let result = serde_yaml::from_str::<GameSupportMatrix>(bad_yaml);
    assert!(result.is_err(), "malformed YAML should produce an error");
}

#[test]
fn empty_yaml_returns_error() {
    let result = serde_yaml::from_str::<GameSupportMatrix>("");
    assert!(result.is_err(), "empty input should produce an error");
}

#[test]
fn yaml_missing_games_key_returns_error() {
    let yaml = "not_games:\n  foo: bar";
    let result = serde_yaml::from_str::<GameSupportMatrix>(yaml);
    assert!(
        result.is_err(),
        "missing 'games' key should produce an error"
    );
}

#[test]
fn json_missing_required_telemetry_config_field_returns_error() {
    // Missing "output_target"
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "fields": []
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "missing required field 'output_target' should produce an error"
    );
}

#[test]
fn json_wrong_type_for_update_rate_returns_error() {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": "not_a_number",
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": []
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "wrong type for update_rate_hz should produce an error"
    );
}

#[test]
fn json_wrong_type_for_enabled_returns_error() {
    let json = r#"{
        "enabled": "yes",
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": []
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "wrong type for enabled should produce an error"
    );
}

#[test]
fn json_wrong_type_for_fields_returns_error() {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": "not_an_array"
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "wrong type for fields should produce an error"
    );
}

#[test]
fn json_completely_empty_object_returns_error() {
    let result = serde_json::from_str::<TelemetryConfig>("{}");
    assert!(
        result.is_err(),
        "empty JSON object should fail for TelemetryConfig"
    );
}

#[test]
fn invalid_game_support_status_string_returns_error() {
    let json = r#""unknown_status""#;
    let result = serde_json::from_str::<GameSupportStatus>(json);
    assert!(
        result.is_err(),
        "unrecognized status string should produce an error"
    );
}

#[test]
fn invalid_diff_operation_string_returns_error() {
    let json = r#""Rename""#;
    let result = serde_json::from_str::<DiffOperation>(json);
    assert!(
        result.is_err(),
        "unrecognized DiffOperation string should produce an error"
    );
}

#[test]
fn config_diff_missing_key_returns_error() {
    let json = r#"{
        "file_path": "a.ini",
        "section": null,
        "old_value": null,
        "new_value": "v",
        "operation": "Add"
    }"#;
    let result = serde_json::from_str::<ConfigDiff>(json);
    assert!(
        result.is_err(),
        "missing 'key' field should produce an error"
    );
}

// ---------------------------------------------------------------------------
// Bonus: specific game properties
// ---------------------------------------------------------------------------

#[test]
fn iracing_has_shared_memory_and_360hz() -> TestResult {
    let matrix = load_default_matrix()?;
    let iracing = matrix.games.get("iracing").ok_or("iracing not in matrix")?;
    assert_eq!(iracing.telemetry.method, "shared_memory");
    assert!(iracing.telemetry.supports_360hz_option);
    assert_eq!(iracing.telemetry.high_rate_update_rate_hz, Some(360));
    assert!(iracing.telemetry.fields.ffb_scalar.is_some());
    assert!(iracing.telemetry.fields.rpm.is_some());
    Ok(())
}

#[test]
fn iracing_has_auto_detect_process_names() -> TestResult {
    let matrix = load_default_matrix()?;
    let iracing = matrix.games.get("iracing").ok_or("iracing not in matrix")?;
    assert!(
        !iracing.auto_detect.process_names.is_empty(),
        "iRacing should have auto-detect process names"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Profile loading/saving tests (file I/O with various formats)
// ---------------------------------------------------------------------------

#[test]
fn profile_save_and_load_json_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("profile.json");

    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20778".to_string(),
        fields: vec!["rpm".to_string(), "gear".to_string()],
        enable_high_rate_iracing_360hz: false,
    };

    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&path, &json)?;

    let loaded: TelemetryConfig = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(loaded.enabled, config.enabled);
    assert_eq!(loaded.update_rate_hz, config.update_rate_hz);
    assert_eq!(loaded.output_method, config.output_method);
    assert_eq!(loaded.output_target, config.output_target);
    assert_eq!(loaded.fields, config.fields);
    Ok(())
}

#[test]
fn profile_save_and_load_yaml_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("profile.yaml");

    let config = TelemetryConfig {
        enabled: false,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["ffb_scalar".to_string(), "speed_ms".to_string()],
        enable_high_rate_iracing_360hz: true,
    };

    let yaml = serde_yaml::to_string(&config)?;
    std::fs::write(&path, &yaml)?;

    let loaded: TelemetryConfig = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(loaded.enabled, config.enabled);
    assert_eq!(loaded.update_rate_hz, config.update_rate_hz);
    assert_eq!(
        loaded.enable_high_rate_iracing_360hz,
        config.enable_high_rate_iracing_360hz
    );
    assert_eq!(loaded.fields, config.fields);
    Ok(())
}

#[test]
fn profile_save_matrix_to_json_file_and_reload() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("matrix.json");

    let matrix = load_default_matrix()?;
    let json = serde_json::to_string_pretty(&matrix)?;
    std::fs::write(&path, &json)?;

    let loaded: GameSupportMatrix = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(loaded.games.len(), matrix.games.len());
    for key in matrix.games.keys() {
        assert!(
            loaded.games.contains_key(key),
            "lost key after file round-trip: {}",
            key
        );
    }
    Ok(())
}

#[test]
fn profile_save_matrix_to_yaml_file_and_reload() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("matrix.yaml");

    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    std::fs::write(&path, &yaml)?;

    let loaded: GameSupportMatrix = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(loaded.games.len(), matrix.games.len());
    Ok(())
}

#[test]
fn profile_overwrite_preserves_latest_values() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("overwrite.json");

    let config_v1 = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:9999".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    std::fs::write(&path, serde_json::to_string(&config_v1)?)?;

    let config_v2 = TelemetryConfig {
        enabled: false,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "192.168.1.1:5300".to_string(),
        fields: vec!["ffb_scalar".to_string(), "gear".to_string()],
        enable_high_rate_iracing_360hz: true,
    };
    std::fs::write(&path, serde_json::to_string(&config_v2)?)?;

    let loaded: TelemetryConfig = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert!(!loaded.enabled);
    assert_eq!(loaded.update_rate_hz, 360);
    assert_eq!(loaded.output_method, "shared_memory");
    assert!(loaded.enable_high_rate_iracing_360hz);
    assert_eq!(loaded.fields.len(), 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Default configuration validation
// ---------------------------------------------------------------------------

#[test]
fn default_matrix_stable_games_have_auto_detect_config() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.status != GameSupportStatus::Stable {
            continue;
        }
        // Stable games should have at least one auto-detect hint
        let has_any = !game.auto_detect.process_names.is_empty()
            || !game.auto_detect.install_registry_keys.is_empty()
            || !game.auto_detect.install_paths.is_empty();
        assert!(
            has_any,
            "stable game '{}' has no auto-detect metadata at all",
            id
        );
    }
    Ok(())
}

#[test]
fn default_matrix_config_writer_ids_are_lowercase_ascii() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            game.config_writer
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
            "game '{}' config_writer '{}' must be lowercase ASCII with underscores",
            id,
            game.config_writer
        );
    }
    Ok(())
}

#[test]
fn default_matrix_game_ids_are_lowercase_ascii() -> TestResult {
    let ids = matrix_game_ids()?;
    for id in &ids {
        assert!(
            id.chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
            "game id '{}' must be lowercase ASCII with underscores",
            id
        );
    }
    Ok(())
}

#[test]
fn default_matrix_all_telemetry_methods_are_non_empty() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.telemetry.method.is_empty(),
            "game '{}' has empty top-level telemetry method",
            id
        );
        for ver in &game.versions {
            assert!(
                !ver.telemetry_method.is_empty(),
                "game '{}' version '{}' has empty telemetry_method",
                id,
                ver.version
            );
        }
    }
    Ok(())
}

#[test]
fn default_matrix_no_game_has_zero_versions() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(!game.versions.is_empty(), "game '{}' has zero versions", id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 8. Configuration merging/override tests
// ---------------------------------------------------------------------------

#[test]
fn json_merge_override_update_rate() -> TestResult {
    let base_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": ["rpm"],
        "enable_high_rate_iracing_360hz": false
    }"#;

    let mut base: serde_json::Value = serde_json::from_str(base_json)?;
    let override_json: serde_json::Value = serde_json::from_str(r#"{"update_rate_hz": 360}"#)?;

    if let (Some(base_obj), Some(override_obj)) = (base.as_object_mut(), override_json.as_object())
    {
        for (k, v) in override_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }

    let merged: TelemetryConfig = serde_json::from_value(base)?;
    assert_eq!(merged.update_rate_hz, 360);
    assert!(merged.enabled);
    assert_eq!(merged.output_method, "udp");
    Ok(())
}

#[test]
fn json_merge_override_enabled_flag() -> TestResult {
    let base_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": ["rpm"],
        "enable_high_rate_iracing_360hz": false
    }"#;

    let mut base: serde_json::Value = serde_json::from_str(base_json)?;
    let override_json: serde_json::Value = serde_json::from_str(r#"{"enabled": false}"#)?;

    if let (Some(base_obj), Some(override_obj)) = (base.as_object_mut(), override_json.as_object())
    {
        for (k, v) in override_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }

    let merged: TelemetryConfig = serde_json::from_value(base)?;
    assert!(!merged.enabled);
    assert_eq!(merged.update_rate_hz, 60);
    Ok(())
}

#[test]
fn json_merge_override_fields_list() -> TestResult {
    let base_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": ["rpm"],
        "enable_high_rate_iracing_360hz": false
    }"#;

    let mut base: serde_json::Value = serde_json::from_str(base_json)?;
    let override_json: serde_json::Value =
        serde_json::from_str(r#"{"fields": ["ffb_scalar", "gear", "speed_ms"]}"#)?;

    if let (Some(base_obj), Some(override_obj)) = (base.as_object_mut(), override_json.as_object())
    {
        for (k, v) in override_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }

    let merged: TelemetryConfig = serde_json::from_value(base)?;
    assert_eq!(merged.fields, vec!["ffb_scalar", "gear", "speed_ms"]);
    assert!(merged.enabled);
    Ok(())
}

#[test]
fn json_merge_override_output_target() -> TestResult {
    let base_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": [],
        "enable_high_rate_iracing_360hz": false
    }"#;

    let mut base: serde_json::Value = serde_json::from_str(base_json)?;
    let overrides: serde_json::Value = serde_json::from_str(
        r#"{"output_target": "192.168.1.50:20778", "output_method": "shared_memory"}"#,
    )?;

    if let (Some(base_obj), Some(ov)) = (base.as_object_mut(), overrides.as_object()) {
        for (k, v) in ov {
            base_obj.insert(k.clone(), v.clone());
        }
    }

    let merged: TelemetryConfig = serde_json::from_value(base)?;
    assert_eq!(merged.output_target, "192.168.1.50:20778");
    assert_eq!(merged.output_method, "shared_memory");
    Ok(())
}

#[test]
fn json_merge_enable_high_rate_override() -> TestResult {
    let base_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": [],
        "enable_high_rate_iracing_360hz": false
    }"#;

    let mut base: serde_json::Value = serde_json::from_str(base_json)?;
    let overrides: serde_json::Value =
        serde_json::from_str(r#"{"enable_high_rate_iracing_360hz": true, "update_rate_hz": 360}"#)?;

    if let (Some(base_obj), Some(ov)) = (base.as_object_mut(), overrides.as_object()) {
        for (k, v) in ov {
            base_obj.insert(k.clone(), v.clone());
        }
    }

    let merged: TelemetryConfig = serde_json::from_value(base)?;
    assert!(merged.enable_high_rate_iracing_360hz);
    assert_eq!(merged.update_rate_hz, 360);
    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Invalid configuration handling (additional edge cases)
// ---------------------------------------------------------------------------

#[test]
fn malformed_json_missing_closing_brace() {
    let json = r#"{"enabled": true, "update_rate_hz": 60"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "malformed JSON (no closing brace) should fail"
    );
}

#[test]
fn malformed_json_trailing_comma() {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": [],
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(result.is_err(), "JSON with trailing comma should fail");
}

#[test]
fn json_null_for_required_string_field_returns_error() {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": null,
        "output_target": "127.0.0.1:9999",
        "fields": []
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "null for required string field should fail"
    );
}

#[test]
fn json_negative_update_rate_parses_but_is_invalid() {
    // u32 cannot hold negative values, so serde should reject this
    let json = r#"{
        "enabled": true,
        "update_rate_hz": -1,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": []
    }"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(
        result.is_err(),
        "negative update_rate_hz should fail for u32"
    );
}

#[test]
fn json_extra_unknown_fields_are_ignored() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": [],
        "unknown_field": "should_be_ignored",
        "another_extra": 42
    }"#;
    // serde default behavior allows unknown fields (deny_unknown_fields not set)
    let decoded: TelemetryConfig = serde_json::from_str(json)?;
    assert!(decoded.enabled);
    assert_eq!(decoded.update_rate_hz, 60);
    Ok(())
}

#[test]
fn yaml_with_wrong_type_for_games_returns_error() {
    let yaml = "games: \"not a map\"";
    let result = serde_yaml::from_str::<GameSupportMatrix>(yaml);
    assert!(
        result.is_err(),
        "games as string instead of map should fail"
    );
}

#[test]
fn json_array_instead_of_object_for_telemetry_config_returns_error() {
    let json = r#"[{"enabled": true}]"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(result.is_err(), "array instead of object should fail");
}

#[test]
fn json_missing_all_fields_except_enabled_returns_error() {
    let json = r#"{"enabled": true}"#;
    let result = serde_json::from_str::<TelemetryConfig>(json);
    assert!(result.is_err(), "missing most required fields should fail");
}

#[test]
fn yaml_game_support_matrix_with_incomplete_game_entry_returns_error() {
    let yaml = r#"
games:
  test_game:
    name: "Test"
"#;
    let result = serde_yaml::from_str::<GameSupportMatrix>(yaml);
    assert!(
        result.is_err(),
        "incomplete game entry (missing versions, telemetry, etc.) should fail"
    );
}

// ---------------------------------------------------------------------------
// 10. Configuration migration tests (old format → new format)
// ---------------------------------------------------------------------------

#[test]
fn migration_old_format_without_high_rate_field_defaults_correctly() -> TestResult {
    // Simulates a config saved before enable_high_rate_iracing_360hz existed
    let old_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": ["rpm", "gear"]
    }"#;
    let loaded: TelemetryConfig = serde_json::from_str(old_json)?;
    assert!(
        !loaded.enable_high_rate_iracing_360hz,
        "old config without high-rate field should default to false"
    );
    assert_eq!(loaded.update_rate_hz, 60);
    assert_eq!(loaded.fields, vec!["rpm", "gear"]);
    Ok(())
}

#[test]
fn migration_old_format_yaml_without_high_rate_field() -> TestResult {
    let old_yaml = r#"
enabled: true
update_rate_hz: 60
output_method: udp
output_target: "127.0.0.1:20777"
fields:
  - rpm
  - gear
"#;
    let loaded: TelemetryConfig = serde_yaml::from_str(old_yaml)?;
    assert!(!loaded.enable_high_rate_iracing_360hz);
    assert!(loaded.enabled);
    assert_eq!(loaded.fields.len(), 2);
    Ok(())
}

#[test]
fn migration_old_telemetry_support_without_optional_fields() -> TestResult {
    // Simulates old telemetry support config without supports_360hz_option or high_rate_update_rate_hz
    let old_json = r#"{
        "method": "shared_memory",
        "update_rate_hz": 60,
        "output_target": "127.0.0.1:12345",
        "fields": {
            "ffb_scalar": "SteeringWheelPctTorqueSign",
            "rpm": "RPM",
            "speed_ms": null,
            "slip_ratio": null,
            "gear": null,
            "flags": null,
            "car_id": null,
            "track_id": null
        }
    }"#;
    let loaded: TelemetrySupport = serde_json::from_str(old_json)?;
    assert!(!loaded.supports_360hz_option);
    assert!(loaded.high_rate_update_rate_hz.is_none());
    assert_eq!(loaded.method, "shared_memory");
    assert_eq!(loaded.update_rate_hz, 60);
    assert!(loaded.fields.ffb_scalar.is_some());
    Ok(())
}

#[test]
fn migration_old_game_support_status_defaults_to_stable() -> TestResult {
    // Games added before the status field existed should default to Stable
    let yaml = r#"
name: "Legacy Game"
versions:
  - version: "1.0"
    config_paths: []
    executable_patterns: ["legacy.exe"]
    telemetry_method: "udp"
    supported_fields: ["rpm"]
telemetry:
  method: "udp"
  update_rate_hz: 60
  output_target: null
  fields:
    ffb_scalar: null
    rpm: "rpm"
    speed_ms: null
    slip_ratio: null
    gear: null
    flags: null
    car_id: null
    track_id: null
config_writer: "legacy"
auto_detect:
  process_names: ["legacy.exe"]
  install_registry_keys: []
  install_paths: []
"#;
    let loaded: GameSupport = serde_yaml::from_str(yaml)?;
    assert_eq!(
        loaded.status,
        GameSupportStatus::Stable,
        "omitted status should default to Stable"
    );
    assert_eq!(loaded.name, "Legacy Game");
    Ok(())
}

#[test]
fn migration_re_serialize_old_config_includes_new_fields() -> TestResult {
    let old_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": ["rpm"]
    }"#;
    let loaded: TelemetryConfig = serde_json::from_str(old_json)?;
    let re_serialized = serde_json::to_string(&loaded)?;
    let re_loaded: TelemetryConfig = serde_json::from_str(&re_serialized)?;
    // After round-trip, the new field should be present and false
    assert!(!re_loaded.enable_high_rate_iracing_360hz);
    assert_eq!(re_loaded.update_rate_hz, 60);
    // Verify the serialized JSON includes the field
    assert!(
        re_serialized.contains("enable_high_rate_iracing_360hz"),
        "re-serialized config should include new fields"
    );
    Ok(())
}

#[test]
fn migration_save_upgraded_config_to_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("migrated.json");

    // Load from old format
    let old_json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:9999",
        "fields": ["rpm", "gear"]
    }"#;
    let loaded: TelemetryConfig = serde_json::from_str(old_json)?;

    // Save in new format
    std::fs::write(&path, serde_json::to_string_pretty(&loaded)?)?;

    // Reload and verify all fields present
    let reloaded: TelemetryConfig = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert!(reloaded.enabled);
    assert_eq!(reloaded.update_rate_hz, 60);
    assert!(!reloaded.enable_high_rate_iracing_360hz);
    assert_eq!(reloaded.fields, vec!["rpm", "gear"]);
    Ok(())
}

// ---------------------------------------------------------------------------
// 11. Device-specific configuration tests (ConfigWriter implementations)
// ---------------------------------------------------------------------------

fn make_test_config(enabled: bool, rate: u32, method: &str, target: &str) -> TelemetryConfig {
    TelemetryConfig {
        enabled,
        update_rate_hz: rate,
        output_method: method.to_string(),
        output_target: target.to_string(),
        fields: vec!["rpm".to_string(), "gear".to_string()],
        enable_high_rate_iracing_360hz: false,
    }
}

fn get_writer(
    game_id: &str,
) -> Result<Box<dyn ConfigWriter + Send + Sync>, Box<dyn std::error::Error>> {
    let factories = config_writer_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("no factory for game_id '{}'", game_id))?;
    Ok(factory())
}

#[test]
fn iracing_writer_write_and_validate_enabled() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("iracing")?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };

    let diffs = writer.write_config(dir.path(), &config)?;
    assert!(
        !diffs.is_empty(),
        "iRacing write_config should produce diffs"
    );

    let valid = writer.validate_config(dir.path())?;
    assert!(
        valid,
        "iRacing config should validate after writing enabled config"
    );
    Ok(())
}

#[test]
fn iracing_writer_write_disabled() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("iracing")?;
    let config = make_test_config(false, 60, "shared_memory", "127.0.0.1:12345");

    let diffs = writer.write_config(dir.path(), &config)?;
    assert!(!diffs.is_empty());

    // Disabled config should not validate as enabled
    let valid = writer.validate_config(dir.path())?;
    assert!(
        !valid,
        "disabled iRacing config should not validate as enabled"
    );
    Ok(())
}

#[test]
fn iracing_writer_with_360hz() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("iracing")?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: true,
    };

    let diffs = writer.write_config(dir.path(), &config)?;
    // Should have 2 diffs: telemetryDiskFile and irsdkLog360Hz
    assert!(
        diffs.len() >= 2,
        "iRacing 360Hz config should produce at least 2 diffs, got {}",
        diffs.len()
    );

    let has_360hz_key = diffs.iter().any(|d| d.key.contains("360"));
    assert!(has_360hz_key, "diffs should include 360Hz key");
    Ok(())
}

#[test]
fn iracing_writer_get_expected_diffs_enabled() -> TestResult {
    let writer = get_writer("iracing")?;
    let config = make_test_config(true, 60, "shared_memory", "127.0.0.1:12345");
    let diffs = writer.get_expected_diffs(&config)?;
    assert!(!diffs.is_empty());

    let telemetry_diff = diffs
        .iter()
        .find(|d| d.key == "telemetryDiskFile")
        .ok_or("missing telemetryDiskFile diff")?;
    assert_eq!(telemetry_diff.new_value, "1");
    assert_eq!(telemetry_diff.section, Some("Telemetry".to_string()));
    Ok(())
}

#[test]
fn iracing_writer_get_expected_diffs_disabled() -> TestResult {
    let writer = get_writer("iracing")?;
    let config = make_test_config(false, 60, "shared_memory", "127.0.0.1:12345");
    let diffs = writer.get_expected_diffs(&config)?;
    let telemetry_diff = diffs
        .iter()
        .find(|d| d.key == "telemetryDiskFile")
        .ok_or("missing telemetryDiskFile diff")?;
    assert_eq!(telemetry_diff.new_value, "0");
    Ok(())
}

#[test]
fn iracing_writer_get_expected_diffs_with_360hz() -> TestResult {
    let writer = get_writer("iracing")?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: true,
    };
    let diffs = writer.get_expected_diffs(&config)?;
    assert!(
        diffs.len() >= 2,
        "360Hz config should have >=2 expected diffs"
    );
    let has_360hz = diffs.iter().any(|d| d.key.contains("360"));
    assert!(has_360hz, "expected diffs should include 360Hz key");
    Ok(())
}

#[test]
fn acc_writer_write_and_validate() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("acc")?;
    let config = make_test_config(true, 60, "udp", "127.0.0.1:9000");

    let diffs = writer.write_config(dir.path(), &config)?;
    assert!(!diffs.is_empty(), "ACC write_config should produce diffs");

    let valid = writer.validate_config(dir.path())?;
    assert!(valid, "ACC config should validate after writing");
    Ok(())
}

#[test]
fn acc_writer_validate_without_config_returns_false() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("acc")?;
    let valid = writer.validate_config(dir.path())?;
    assert!(!valid, "ACC should not validate without any config written");
    Ok(())
}

#[test]
fn eawrc_writer_write_and_validate() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("eawrc")?;
    let config = make_test_config(true, 60, "udp", "127.0.0.1:20778");

    let diffs = writer.write_config(dir.path(), &config)?;
    assert!(
        diffs.len() >= 2,
        "EA WRC should produce at least 2 diffs (config + structure), got {}",
        diffs.len()
    );

    let valid = writer.validate_config(dir.path())?;
    assert!(valid, "EA WRC config should validate after writing");
    Ok(())
}

#[test]
fn eawrc_writer_validate_without_config_returns_false() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("eawrc")?;
    let valid = writer.validate_config(dir.path())?;
    assert!(!valid, "EA WRC should not validate without config");
    Ok(())
}

#[test]
fn iracing_writer_validate_without_config_returns_false() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("iracing")?;
    let valid = writer.validate_config(dir.path())?;
    assert!(!valid, "iRacing should not validate in empty dir");
    Ok(())
}

#[test]
fn iracing_writer_overwrite_preserves_enabled_state() -> TestResult {
    let dir = tempfile::tempdir()?;
    let writer = get_writer("iracing")?;

    // Write enabled config
    let config_on = make_test_config(true, 60, "shared_memory", "127.0.0.1:12345");
    writer.write_config(dir.path(), &config_on)?;
    assert!(writer.validate_config(dir.path())?);

    // Overwrite with disabled
    let config_off = make_test_config(false, 60, "shared_memory", "127.0.0.1:12345");
    writer.write_config(dir.path(), &config_off)?;
    assert!(!writer.validate_config(dir.path())?);

    // Re-enable
    writer.write_config(dir.path(), &config_on)?;
    assert!(writer.validate_config(dir.path())?);
    Ok(())
}

#[test]
fn all_config_writers_can_produce_expected_diffs() -> TestResult {
    let config = make_test_config(true, 60, "udp", "127.0.0.1:9999");
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        assert!(
            !diffs.is_empty(),
            "config writer '{}' should produce at least one expected diff",
            id
        );
        for diff in &diffs {
            assert!(
                !diff.file_path.is_empty(),
                "config writer '{}' produced a diff with empty file_path",
                id
            );
        }
    }
    Ok(())
}

#[test]
fn all_config_writers_validate_false_on_empty_dir() -> TestResult {
    for (id, factory) in config_writer_factories() {
        let dir = tempfile::tempdir()?;
        let writer = factory();
        let valid = writer.validate_config(dir.path())?;
        assert!(
            !valid,
            "config writer '{}' should not validate on empty directory",
            id
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 12. Game-specific matrix property tests
// ---------------------------------------------------------------------------

#[test]
fn acc_game_has_udp_telemetry_method() -> TestResult {
    let matrix = load_default_matrix()?;
    let acc = matrix.games.get("acc").ok_or("acc not in matrix")?;
    assert!(
        acc.telemetry.method.contains("udp") || acc.telemetry.method.contains("broadcast"),
        "ACC should use UDP-based telemetry, got '{}'",
        acc.telemetry.method
    );
    assert!(acc.telemetry.update_rate_hz > 0);
    Ok(())
}

#[test]
fn eawrc_game_has_udp_telemetry() -> TestResult {
    let matrix = load_default_matrix()?;
    let eawrc = matrix.games.get("eawrc").ok_or("eawrc not in matrix")?;
    assert!(
        eawrc.telemetry.method.contains("udp"),
        "EA WRC should use UDP telemetry, got '{}'",
        eawrc.telemetry.method
    );
    Ok(())
}

#[test]
fn forza_motorsport_game_has_udp_telemetry() -> TestResult {
    let matrix = load_default_matrix()?;
    let forza = matrix
        .games
        .get("forza_motorsport")
        .ok_or("forza_motorsport not in matrix")?;
    assert!(
        forza.telemetry.method.contains("udp"),
        "Forza Motorsport should use UDP telemetry, got '{}'",
        forza.telemetry.method
    );
    Ok(())
}

#[test]
fn beamng_drive_game_has_expected_properties() -> TestResult {
    let matrix = load_default_matrix()?;
    let beamng = matrix
        .games
        .get("beamng_drive")
        .ok_or("beamng_drive not in matrix")?;
    assert!(!beamng.name.is_empty());
    assert!(!beamng.versions.is_empty());
    assert!(!beamng.auto_detect.process_names.is_empty());
    Ok(())
}

#[test]
fn ams2_game_uses_shared_memory() -> TestResult {
    let matrix = load_default_matrix()?;
    let ams2 = matrix.games.get("ams2").ok_or("ams2 not in matrix")?;
    assert!(
        ams2.telemetry.method.contains("shared_memory"),
        "AMS2 should use shared memory telemetry, got '{}'",
        ams2.telemetry.method
    );
    Ok(())
}

#[test]
fn rfactor2_game_uses_shared_memory() -> TestResult {
    let matrix = load_default_matrix()?;
    let rf2 = matrix
        .games
        .get("rfactor2")
        .ok_or("rfactor2 not in matrix")?;
    assert!(
        rf2.telemetry.method.contains("shared_memory"),
        "rFactor 2 should use shared memory telemetry, got '{}'",
        rf2.telemetry.method
    );
    Ok(())
}

#[test]
fn gran_turismo_7_game_has_encrypted_udp() -> TestResult {
    let matrix = load_default_matrix()?;
    let gt7 = matrix
        .games
        .get("gran_turismo_7")
        .ok_or("gran_turismo_7 not in matrix")?;
    assert!(
        gt7.telemetry.method.contains("udp"),
        "GT7 should use UDP-based telemetry, got '{}'",
        gt7.telemetry.method
    );
    Ok(())
}
