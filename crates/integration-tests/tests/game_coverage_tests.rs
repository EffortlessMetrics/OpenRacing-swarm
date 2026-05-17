//! Integration tests verifying that all game adapters and config writers are
//! registered in both game_support_matrix.yaml files.
//!
//! These tests prevent regressions like the RaceRoom omission, where an adapter
//! and config writer existed in Rust code but had no corresponding entry in either
//! YAML configuration file, causing silent runtime failures.

use openracing_telemetry_adapters::adapter_factories;
use openracing_telemetry_config::config_writer_factories;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Every game ID registered in `adapter_factories()` must appear in both
/// game_support_matrix.yaml files (telemetry-config and telemetry-support).
///
/// A missing entry causes `GameService::new()` to panic with an opaque error
/// at runtime even though all Rust code compiled successfully.
#[test]
fn all_adapter_game_ids_present_in_both_yaml_files() -> TestResult {
    let config_ids = openracing_telemetry_config::matrix_game_id_set()?;
    let support_ids = racing_wheel_telemetry_support::matrix_game_id_set()?;

    let mut missing_from_config: Vec<&str> = Vec::new();
    let mut missing_from_support: Vec<&str> = Vec::new();

    for (game_id, _) in adapter_factories() {
        if !config_ids.contains(*game_id) {
            missing_from_config.push(game_id);
        }
        if !support_ids.contains(*game_id) {
            missing_from_support.push(game_id);
        }
    }

    assert!(
        missing_from_config.is_empty(),
        "Adapters missing from crates/telemetry-config/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_config
    );
    assert!(
        missing_from_support.is_empty(),
        "Adapters missing from crates/telemetry-support/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_support
    );

    Ok(())
}

/// Every game ID registered in `config_writer_factories()` (telemetry-config crate)
/// must appear in both game_support_matrix.yaml files.
///
/// Config writers registered in code without a matching YAML entry are silently
/// skipped at runtime (see friction log F-002).
#[test]
fn all_config_writer_ids_present_in_both_yaml_files() -> TestResult {
    let config_ids = openracing_telemetry_config::matrix_game_id_set()?;
    let support_ids = racing_wheel_telemetry_support::matrix_game_id_set()?;

    let mut missing_from_config: Vec<&str> = Vec::new();
    let mut missing_from_support: Vec<&str> = Vec::new();

    for (game_id, _) in config_writer_factories() {
        if !config_ids.contains(*game_id) {
            missing_from_config.push(game_id);
        }
        if !support_ids.contains(*game_id) {
            missing_from_support.push(game_id);
        }
    }

    assert!(
        missing_from_config.is_empty(),
        "Config writers missing from crates/telemetry-config/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_config
    );
    assert!(
        missing_from_support.is_empty(),
        "Config writers missing from crates/telemetry-support/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_support
    );

    Ok(())
}

/// Every game ID registered in the telemetry-config-writers crate's
/// `config_writer_factories()` must appear in both game_support_matrix.yaml files.
///
/// This also surfaces divergence between the two parallel config-writer crates
/// (see friction log F-002): if one crate registers a game the other doesn't,
/// that omission is caught here.
#[test]
fn all_config_writer_ids_from_writers_crate_present_in_both_yaml_files() -> TestResult {
    let config_ids = openracing_telemetry_config::matrix_game_id_set()?;
    let support_ids = racing_wheel_telemetry_support::matrix_game_id_set()?;

    let mut missing_from_config: Vec<&str> = Vec::new();
    let mut missing_from_support: Vec<&str> = Vec::new();

    for (game_id, _) in openracing_telemetry_config::config_writer_factories() {
        if !config_ids.contains(*game_id) {
            missing_from_config.push(game_id);
        }
        if !support_ids.contains(*game_id) {
            missing_from_support.push(game_id);
        }
    }

    assert!(
        missing_from_config.is_empty(),
        "Config writers (telemetry-config-writers crate) missing from \
         crates/telemetry-config/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_config
    );
    assert!(
        missing_from_support.is_empty(),
        "Config writers (telemetry-config-writers crate) missing from \
         crates/telemetry-support/src/game_support_matrix.yaml: {:?}. \
         Add the missing game IDs to that file.",
        missing_from_support
    );

    Ok(())
}
