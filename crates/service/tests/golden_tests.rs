//! Golden file tests for game configuration writers
//!
//! Tests that compare generated configs against known fixtures
//! Requirements: GI-01 (one-click telemetry configuration)

// Test helper functions to replace unwrap
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

use openracing_telemetry_config::support::matrix_game_ids;
use racing_wheel_service::config_writers::{
    ACCConfigWriter, Dirt5ConfigWriter, F1_25ConfigWriter, F1ConfigWriter, IRacingConfigWriter,
};
use racing_wheel_service::game_service::*;
use racing_wheel_service::telemetry::TelemetryService;
use std::collections::HashSet;
use std::path::Path;
use tempfile::TempDir;

/// Test data for golden file tests
struct TestGameConfig {
    #[allow(dead_code)]
    game_id: String,
    config: TelemetryConfig,
    expected_diffs: Vec<ConfigDiff>,
}

impl TestGameConfig {
    fn iracing_test_config() -> Self {
        Self {
            game_id: "iracing".to_string(),
            config: TelemetryConfig {
                enabled: true,
                update_rate_hz: 60,
                output_method: "shared_memory".to_string(),
                output_target: "127.0.0.1:12345".to_string(),
                fields: vec![
                    "ffb_scalar".to_string(),
                    "rpm".to_string(),
                    "speed_ms".to_string(),
                    "slip_ratio".to_string(),
                    "gear".to_string(),
                    "car_id".to_string(),
                    "track_id".to_string(),
                ],
                enable_high_rate_iracing_360hz: false,
            },
            expected_diffs: vec![ConfigDiff {
                file_path: "Documents/iRacing/app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: "telemetryDiskFile".to_string(),
                old_value: None,
                new_value: "1".to_string(),
                operation: DiffOperation::Add,
            }],
        }
    }

    fn acc_test_config() -> Self {
        Self {
            game_id: "acc".to_string(),
            config: TelemetryConfig {
                enabled: true,
                update_rate_hz: 100,
                output_method: "udp_broadcast".to_string(),
                output_target: "127.0.0.1:9000".to_string(),
                fields: vec![
                    "ffb_scalar".to_string(),
                    "rpm".to_string(),
                    "speed_ms".to_string(),
                    "slip_ratio".to_string(),
                    "gear".to_string(),
                    "car_id".to_string(),
                    "track_id".to_string(),
                ],
                enable_high_rate_iracing_360hz: false,
            },
            expected_diffs: vec![ConfigDiff {
                file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json"
                    .to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: must(serde_json::to_string_pretty(&serde_json::json!({
                    "updListenerPort": 9000,
                    "udpListenerPort": 9000,
                    "connectionId": "",
                    "connectionPassword": "",
                    "commandPassword": "",
                    "broadcastingPort": 9000,
                    "updateRateHz": 100
                }))),
                operation: DiffOperation::Add,
            }],
        }
    }
}

fn matrix_config_from_support(game_id: &str, support: &GameSupport) -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: support.telemetry.update_rate_hz,
        output_method: support.telemetry.method.clone(),
        output_target: support
            .telemetry
            .output_target
            .clone()
            .unwrap_or_else(|| match game_id {
                "acc" => "127.0.0.1:9000".to_string(),
                "ac_rally" => "127.0.0.1:9000".to_string(),
                "eawrc" => "127.0.0.1:20778".to_string(),
                "dirt5" => "127.0.0.1:20777".to_string(),
                _ => "127.0.0.1:12345".to_string(),
            }),
        fields: support.versions[0].supported_fields.clone(),
        enable_high_rate_iracing_360hz: false,
    }
}

fn file_path_matches(expected: &str, actual: &str) -> bool {
    if expected.eq_ignore_ascii_case(actual) {
        return true;
    }

    path_suffix_matches(expected, actual) || path_suffix_matches(actual, expected)
}

fn path_suffix_matches(expected: &str, actual: &str) -> bool {
    let expected_components: Vec<String> = Path::new(expected)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    let actual_components: Vec<String> = Path::new(actual)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    if expected_components.len() > actual_components.len() {
        return false;
    }

    let start_index = actual_components.len() - expected_components.len();
    actual_components[start_index..] == expected_components
}

fn diff_matches(expected: &ConfigDiff, actual: &ConfigDiff) -> bool {
    file_path_matches(&expected.file_path, &actual.file_path)
        && expected.section == actual.section
        && expected.key == actual.key
        && expected.operation == actual.operation
        && expected.new_value == actual.new_value
}

#[tokio::test]
async fn test_iracing_config_writer_golden() {
    let writer = IRacingConfigWriter;
    let test_config = TestGameConfig::iracing_test_config();
    let temp_dir = must(TempDir::new());

    // Test expected diffs match actual diffs
    let expected_diffs = must(writer.get_expected_diffs(&test_config.config));
    assert_eq!(expected_diffs, test_config.expected_diffs);

    // Test actual config writing
    let actual_diffs = must(writer.write_config(temp_dir.path(), &test_config.config));

    // Compare actual diffs with expected (ignoring file paths which will be different in temp dir)
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    for (actual, expected) in actual_diffs.iter().zip(expected_diffs.iter()) {
        assert_eq!(actual.section, expected.section);
        assert_eq!(actual.key, expected.key);
        assert_eq!(actual.new_value, expected.new_value);
        assert_eq!(actual.operation, expected.operation);
    }
}

#[tokio::test]
async fn test_acc_config_writer_golden() {
    let writer = ACCConfigWriter;
    let test_config = TestGameConfig::acc_test_config();
    let temp_dir = must(TempDir::new());

    // Test expected diffs match actual diffs
    let expected_diffs = must(writer.get_expected_diffs(&test_config.config));
    assert_eq!(expected_diffs.len(), 1);

    // Test actual config writing
    let actual_diffs = must(writer.write_config(temp_dir.path(), &test_config.config));

    // Compare actual diffs with expected (ignoring file paths which will be different in temp dir)
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    for (actual, expected) in actual_diffs.iter().zip(expected_diffs.iter()) {
        assert_eq!(actual.section, expected.section);
        assert_eq!(actual.key, expected.key);
        assert_eq!(actual.operation, expected.operation);

        // For JSON content, parse and compare structure
        if actual.key == "entire_file" {
            let actual_json: serde_json::Value = must(serde_json::from_str(&actual.new_value));
            let expected_json: serde_json::Value = must(serde_json::from_str(&expected.new_value));
            assert_eq!(actual_json, expected_json);
        } else {
            assert_eq!(actual.new_value, expected.new_value);
        }
    }
}

#[tokio::test]
async fn test_dirt5_config_writer_golden() {
    let writer = Dirt5ConfigWriter;
    let temp_dir = must(TempDir::new());
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp_custom_codemasters".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![
            "rpm".to_string(),
            "speed_ms".to_string(),
            "gear".to_string(),
        ],
        enable_high_rate_iracing_360hz: false,
    };

    let expected_diffs = must(writer.get_expected_diffs(&config));
    assert_eq!(expected_diffs.len(), 1);

    let actual_diffs = must(writer.write_config(temp_dir.path(), &config));
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    assert_eq!(actual_diffs[0].key, expected_diffs[0].key);
    assert_eq!(actual_diffs[0].operation, expected_diffs[0].operation);

    let contract: serde_json::Value = must(serde_json::from_str(&actual_diffs[0].new_value));
    assert_eq!(contract["game_id"], "dirt5");
    assert_eq!(contract["telemetry_protocol"], "codemasters_udp");
    assert_eq!(contract["udp_port"], 20777);
}

#[tokio::test]
async fn test_f1_25_config_writer_golden() {
    let writer = F1_25ConfigWriter;
    let temp_dir = must(TempDir::new());
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp_native_f1_25".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![
            "rpm".to_string(),
            "speed_ms".to_string(),
            "gear".to_string(),
            "flags".to_string(),
        ],
        enable_high_rate_iracing_360hz: false,
    };

    let expected_diffs = must(writer.get_expected_diffs(&config));
    assert_eq!(expected_diffs.len(), 1);
    assert_eq!(expected_diffs[0].key, "entire_file");
    assert_eq!(expected_diffs[0].operation, DiffOperation::Add);

    let actual_diffs = must(writer.write_config(temp_dir.path(), &config));
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    assert_eq!(actual_diffs[0].key, expected_diffs[0].key);
    assert_eq!(actual_diffs[0].operation, expected_diffs[0].operation);

    let contract: serde_json::Value = must(serde_json::from_str(&actual_diffs[0].new_value));
    assert_eq!(contract["game_id"], "f1_25");
    assert_eq!(contract["telemetry_protocol"], "f1_25_native_udp");
    assert_eq!(contract["packet_format"], 2025);
    assert_eq!(contract["udp_port"], 20777);
    assert_eq!(contract["update_rate_hz"], 60);
    assert_eq!(contract["enabled"], true);

    // Validate returns true after writing
    let valid = must(writer.validate_config(temp_dir.path()));
    assert!(
        valid,
        "validate_config should return true after write_config"
    );
}

#[tokio::test]
async fn test_f1_config_writer_golden() {
    let writer = F1ConfigWriter;
    let temp_dir = must(TempDir::new());
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp_custom_codemasters".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![
            "rpm".to_string(),
            "speed_ms".to_string(),
            "gear".to_string(),
            "slip_ratio".to_string(),
            "flags".to_string(),
        ],
        enable_high_rate_iracing_360hz: false,
    };

    let expected_diffs = must(writer.get_expected_diffs(&config));
    assert_eq!(expected_diffs.len(), 1);

    let actual_diffs = must(writer.write_config(temp_dir.path(), &config));
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    assert_eq!(actual_diffs[0].key, expected_diffs[0].key);
    assert_eq!(actual_diffs[0].operation, expected_diffs[0].operation);

    let contract: serde_json::Value = must(serde_json::from_str(&actual_diffs[0].new_value));
    assert_eq!(contract["game_id"], "f1");
    assert_eq!(contract["telemetry_protocol"], "codemasters_udp");
    assert_eq!(contract["udp_port"], 20777);
}

#[tokio::test]
async fn test_game_service_yaml_loading() {
    let service = must(GameService::new().await);

    // Test supported games loaded from YAML
    let expected: HashSet<String> = must(matrix_game_ids()).into_iter().collect();
    let actual: HashSet<String> = service.get_supported_games().await.into_iter().collect();
    assert_eq!(actual, expected);

    // Historical alias is accepted through service normalization.
    let ea_wrc_support = must(service.get_game_support("ea_wrc").await);
    assert_eq!(ea_wrc_support.config_writer, "eawrc");
    assert_eq!(ea_wrc_support.name, "EA SPORTS WRC");
}

#[tokio::test]
async fn test_game_support_matrix_structure() {
    let service = must(GameService::new().await);

    // Test iRacing support structure
    let iracing_support = must(service.get_game_support("iracing").await);
    assert_eq!(iracing_support.name, "iRacing");
    assert_eq!(iracing_support.telemetry.method, "shared_memory");
    assert_eq!(iracing_support.telemetry.update_rate_hz, 60);
    assert_eq!(iracing_support.config_writer, "iracing");
    assert_eq!(
        iracing_support.telemetry.output_target,
        Some("127.0.0.1:12345".to_string())
    );

    // Verify version information
    assert_eq!(iracing_support.versions.len(), 1);
    assert_eq!(iracing_support.versions[0].version, "2024.x");
    assert!(
        iracing_support.versions[0]
            .config_paths
            .contains(&"Documents/iRacing/app.ini".to_string())
    );
    assert!(
        iracing_support.versions[0]
            .executable_patterns
            .contains(&"iRacingSim64DX11.exe".to_string())
    );

    // Verify auto-detection config
    assert!(
        iracing_support
            .auto_detect
            .process_names
            .contains(&"iRacingSim64DX11.exe".to_string())
    );
    assert!(
        iracing_support
            .auto_detect
            .install_registry_keys
            .contains(&"HKEY_CURRENT_USER\\Software\\iRacing.com\\iRacing".to_string())
    );

    // Test ACC support structure
    let acc_support = must(service.get_game_support("acc").await);
    assert_eq!(acc_support.name, "Assetto Corsa Competizione");
    assert_eq!(acc_support.telemetry.method, "udp_broadcast");
    assert_eq!(acc_support.telemetry.update_rate_hz, 100);
    assert_eq!(acc_support.config_writer, "acc");
    assert_eq!(
        acc_support.telemetry.output_target,
        Some("127.0.0.1:9000".to_string())
    );

    let eawrc_support = must(service.get_game_support("eawrc").await);
    assert_eq!(eawrc_support.name, "EA SPORTS WRC");
    assert_eq!(eawrc_support.config_writer, "eawrc");
    assert_eq!(
        eawrc_support.telemetry.output_target,
        Some("127.0.0.1:20778".to_string())
    );

    let dirt5_support = must(service.get_game_support("dirt5").await);
    assert_eq!(dirt5_support.name, "Dirt 5");
    assert_eq!(dirt5_support.config_writer, "dirt5");
    assert_eq!(
        dirt5_support.telemetry.output_target,
        Some("127.0.0.1:20777".to_string())
    );

    // Verify version information
    assert_eq!(acc_support.versions.len(), 1);
    assert_eq!(acc_support.versions[0].version, "1.9.x");
    assert!(
        acc_support.versions[0]
            .config_paths
            .contains(&"Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string())
    );
}

#[tokio::test]
async fn test_game_service_matrix_configured_games_roundtrip() {
    let service = must(GameService::new().await);
    let expected: Vec<String> = must(matrix_game_ids());

    let mut supported_games = service.get_supported_games().await;
    supported_games.sort_unstable();
    assert_eq!(supported_games.len(), expected.len());

    for game_id in supported_games {
        let temp_dir = must(TempDir::new());
        let support = must(service.get_game_support(&game_id).await);
        let config = matrix_config_from_support(&game_id, &support);

        let expected = must(service.get_expected_diffs(&game_id, &config).await);
        let actual = must(service.configure_telemetry(&game_id, temp_dir.path()).await);
        assert_eq!(
            expected.len(),
            actual.len(),
            "expected configured diff count to match matrix for {}",
            game_id
        );

        for expected_diff in &expected {
            let matched = actual
                .iter()
                .any(|actual_diff| diff_matches(expected_diff, actual_diff));
            assert!(
                matched,
                "expected configured diff to match matrix for {}: expected={:?}, actual={:?}",
                game_id, expected_diff, actual
            );
        }
    }
}

#[tokio::test]
async fn test_telemetry_field_mapping_coverage() {
    let service = must(GameService::new().await);
    let mut supported_games = service.get_supported_games().await;
    supported_games.sort_unstable();

    for game_id in supported_games {
        let support = must(service.get_game_support(&game_id).await);
        let mapping = must(service.get_telemetry_mapping(&game_id).await);

        let expected = support.telemetry.fields;

        assert_eq!(mapping.ffb_scalar, expected.ffb_scalar);
        assert_eq!(mapping.rpm, expected.rpm);
        assert_eq!(mapping.speed_ms, expected.speed_ms);
        assert_eq!(mapping.slip_ratio, expected.slip_ratio);
        assert_eq!(mapping.gear, expected.gear);
        assert_eq!(mapping.flags, expected.flags);
        assert_eq!(mapping.car_id, expected.car_id);
        assert_eq!(mapping.track_id, expected.track_id);
    }
}

#[tokio::test]
async fn test_configuration_diff_generation() {
    let service = must(GameService::new().await);

    // Test iRacing expected diffs
    let iracing_config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };

    let iracing_diffs = must(service.get_expected_diffs("iracing", &iracing_config).await);
    assert_eq!(iracing_diffs.len(), 1);
    assert_eq!(iracing_diffs[0].key, "telemetryDiskFile");
    assert_eq!(iracing_diffs[0].new_value, "1");
    assert_eq!(iracing_diffs[0].operation, DiffOperation::Add);

    // Test ACC expected diffs
    let acc_config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 100,
        output_method: "udp_broadcast".to_string(),
        output_target: "127.0.0.1:9000".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };

    let acc_diffs = must(service.get_expected_diffs("acc", &acc_config).await);
    assert_eq!(acc_diffs.len(), 1);
    assert_eq!(acc_diffs[0].key, "entire_file");
    assert_eq!(acc_diffs[0].operation, DiffOperation::Add);

    // Verify ACC JSON structure
    let acc_json: serde_json::Value = must(serde_json::from_str(&acc_diffs[0].new_value));
    assert_eq!(acc_json["updListenerPort"], 9000);
    assert_eq!(acc_json["udpListenerPort"], 9000);
    assert_eq!(acc_json["broadcastingPort"], 9000);
    assert_eq!(acc_json["connectionId"], "");
    assert_eq!(acc_json["connectionPassword"], "");
    assert_eq!(acc_json["commandPassword"], "");
    assert_eq!(acc_json["updateRateHz"], 100);
}

#[tokio::test]
async fn test_active_game_management() {
    let service = must(GameService::new().await);

    // Initially no active game
    assert_eq!(service.get_active_game().await, None);

    // Set active game
    must(service.set_active_game(Some("iracing".to_string())).await);
    assert_eq!(service.get_active_game().await, Some("iracing".to_string()));

    // Switch to different game
    must(service.set_active_game(Some("acc".to_string())).await);
    assert_eq!(service.get_active_game().await, Some("acc".to_string()));

    // Clear active game
    must(service.set_active_game(None).await);
    assert_eq!(service.get_active_game().await, None);
}

#[tokio::test]
async fn test_unsupported_game_handling() {
    let service = must(GameService::new().await);

    // Test unsupported game returns error
    let result = service.get_game_support("unsupported_game").await;
    assert!(result.is_err());
    let err = result.err();
    assert!(err.is_some());
    assert!(
        err.as_ref()
            .is_some_and(|e| e.to_string().contains("Unsupported game"))
    );

    let mapping_result = service.get_telemetry_mapping("unsupported_game").await;
    assert!(mapping_result.is_err());
    let err = mapping_result.err();
    assert!(err.is_some());
    assert!(
        err.as_ref()
            .is_some_and(|e| e.to_string().contains("Unsupported game"))
    );

    let config_result = service
        .get_expected_diffs(
            "unsupported_game",
            &TelemetryConfig {
                enabled: true,
                update_rate_hz: 60,
                output_method: "test".to_string(),
                output_target: "test".to_string(),
                fields: vec![],
                enable_high_rate_iracing_360hz: false,
            },
        )
        .await;
    assert!(config_result.is_err());
    let err = config_result.err();
    assert!(err.is_some());
    assert!(
        err.as_ref()
            .is_some_and(|e| { e.to_string().contains("No config writer for game") })
    );
}

#[tokio::test]
async fn test_matrix_driven_integration_consistency() {
    let service = must(GameService::new().await);
    let telemetry_service = TelemetryService::new();

    let mut matrix_games = service.get_supported_games().await;
    let mut telemetry_games = telemetry_service.supported_games();
    matrix_games.sort_unstable();
    telemetry_games.sort_unstable();

    // The telemetry service may have fewer adapters than the matrix (some matrix entries may be experimental)
    // But all adapters in telemetry service should be in the matrix
    for game_id in &telemetry_games {
        assert!(
            matrix_games.contains(game_id),
            "Adapter {} not found in matrix games {:?}",
            game_id,
            matrix_games
        );
    }

    // Log what we have for debugging
    eprintln!("Matrix games: {:?}", matrix_games);
    eprintln!("Telemetry games: {:?}", telemetry_games);

    for game_id in matrix_games {
        let support = must(service.get_game_support(&game_id).await);
        assert!(
            !support.versions.is_empty(),
            "game {game_id} should define at least one telemetry version"
        );

        for version in support.versions.iter() {
            assert_eq!(
                version.telemetry_method, support.telemetry.method,
                "version telemetry_method should match top-level matrix method for {game_id}"
            );
        }

        let config = matrix_config_from_support(&game_id, &support);
        let expected_diffs = must(service.get_expected_diffs(&game_id, &config).await);
        assert!(
            !expected_diffs.is_empty(),
            "expected diffs should exist for {game_id} so writer registration is present"
        );
    }

    let ea_wrc_support = must(service.get_game_support("ea_wrc").await);
    assert_eq!(ea_wrc_support.config_writer, "eawrc");
}

#[tokio::test]
async fn test_end_to_end_telemetry_configuration() {
    let service = must(GameService::new().await);
    let temp_dir = must(TempDir::new());

    // Test iRacing end-to-end configuration
    let iracing_diffs = must(
        service
            .configure_telemetry("iracing", temp_dir.path())
            .await,
    );
    assert_eq!(iracing_diffs.len(), 1);
    assert_eq!(iracing_diffs[0].key, "telemetryDiskFile");
    assert_eq!(iracing_diffs[0].new_value, "1");

    // Test ACC end-to-end configuration
    let acc_diffs = must(service.configure_telemetry("acc", temp_dir.path()).await);
    assert_eq!(acc_diffs.len(), 1);
    assert_eq!(acc_diffs[0].key, "entire_file");

    // Verify ACC JSON is valid
    let acc_json: serde_json::Value = must(serde_json::from_str(&acc_diffs[0].new_value));
    assert!(acc_json.is_object());
    assert!(acc_json.get("updListenerPort").is_some());
    assert!(acc_json.get("udpListenerPort").is_some());
}
