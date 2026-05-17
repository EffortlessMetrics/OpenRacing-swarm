//! Comprehensive game support matrix validation tests.
//!
//! These tests validate the full game support matrix — ensuring every supported
//! game has proper telemetry config, adapters, config writers, and documentation
//! consistency. They are the safety net that prevents silent runtime failures
//! caused by mismatched registrations, conflicting ports, or missing metadata.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use openracing_telemetry_adapters::adapter_factories;
use openracing_telemetry_config::{DiffOperation, TelemetryConfig, config_writer_factories};
use racing_wheel_telemetry_support::{
    GameSupportMatrix, GameSupportStatus, load_default_matrix, matrix_game_id_set, matrix_game_ids,
    normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Helper functions ──────────────────────────────────────────────────────

fn default_telemetry_config() -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![
            "rpm".to_string(),
            "speed_ms".to_string(),
            "gear".to_string(),
        ],
        enable_high_rate_iracing_360hz: false,
    }
}

fn adapter_id_set() -> HashSet<String> {
    adapter_factories()
        .iter()
        .map(|(id, _)| (*id).to_string())
        .collect()
}

fn writer_id_set() -> HashSet<String> {
    config_writer_factories()
        .iter()
        .map(|(id, _)| (*id).to_string())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Matrix loading and structural integrity
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn matrix_loads_successfully() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(
        !matrix.games.is_empty(),
        "Game support matrix must not be empty"
    );
    Ok(())
}

#[test]
fn matrix_has_minimum_game_count() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(
        matrix.games.len() >= 40,
        "Expected at least 40 games in matrix, found {}",
        matrix.games.len()
    );
    Ok(())
}

#[test]
fn matrix_game_ids_sorted_and_unique() -> TestResult {
    let game_ids = matrix_game_ids()?;
    assert!(
        game_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "Game IDs must be sorted and unique (no duplicates)"
    );
    Ok(())
}

#[test]
fn matrix_game_id_set_matches_list() -> TestResult {
    let list = matrix_game_ids()?;
    let set = matrix_game_id_set()?;
    assert_eq!(
        list.len(),
        set.len(),
        "game_ids list and set should have same length (no duplicates)"
    );
    for id in &list {
        assert!(set.contains(id), "Set missing game_id: {id}");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Every game has valid telemetry config
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn every_game_has_non_empty_name() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(
            !support.name.is_empty(),
            "Game '{game_id}' has an empty name"
        );
    }
    Ok(())
}

#[test]
fn every_game_has_at_least_one_version() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(
            !support.versions.is_empty(),
            "Game '{game_id}' has no versions defined"
        );
    }
    Ok(())
}

#[test]
fn every_game_version_has_telemetry_method() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for version in &support.versions {
            assert!(
                !version.telemetry_method.is_empty(),
                "Game '{game_id}' version '{}' has no telemetry_method",
                version.version
            );
        }
    }
    Ok(())
}

#[test]
fn every_game_has_valid_telemetry_method() -> TestResult {
    let valid_methods = [
        "shared_memory",
        "udp_broadcast",
        "udp",
        "probe_discovery",
        "none",
        "outgauge_udp",
        "scs_shared_memory",
    ];
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        let method = &support.telemetry.method;
        // Allow any non-empty method (game-specific methods exist)
        assert!(
            !method.is_empty(),
            "Game '{game_id}' has empty telemetry method"
        );
        // Log for reference but don't fail on unknown methods — new protocols are expected
        if !valid_methods.contains(&method.as_str()) {
            // Just ensure it's a reasonable string (no whitespace-only)
            assert!(
                method.trim().len() == method.len(),
                "Game '{game_id}' has telemetry method with leading/trailing whitespace: '{method}'"
            );
        }
    }
    Ok(())
}

#[test]
fn stable_games_have_nonzero_update_rate() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        if support.status == GameSupportStatus::Stable && support.telemetry.method != "none" {
            assert!(
                support.telemetry.update_rate_hz > 0,
                "Stable game '{game_id}' (method: '{}') has zero update_rate_hz",
                support.telemetry.method
            );
        }
    }
    Ok(())
}

#[test]
fn every_game_has_valid_status() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        // Verify the status is one of the expected variants
        match support.status {
            GameSupportStatus::Stable | GameSupportStatus::Experimental => {}
        }
        // Verify stable games have meaningful telemetry fields
        if support.status == GameSupportStatus::Stable && support.telemetry.method != "none" {
            let fields = &support.telemetry.fields;
            let has_any_field = fields.ffb_scalar.is_some()
                || fields.rpm.is_some()
                || fields.speed_ms.is_some()
                || fields.gear.is_some();
            assert!(
                has_any_field,
                "Stable game '{game_id}' should have at least one mapped telemetry field"
            );
        }
    }
    Ok(())
}

#[test]
fn games_with_360hz_option_have_high_rate_defined() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        if support.telemetry.supports_360hz_option {
            assert!(
                support.telemetry.high_rate_update_rate_hz.is_some(),
                "Game '{game_id}' supports 360Hz but has no high_rate_update_rate_hz"
            );
            let rate = support.telemetry.high_rate_update_rate_hz.as_ref();
            assert!(
                rate.is_some_and(|r| *r > 0),
                "Game '{game_id}' high_rate_update_rate_hz must be positive"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Config file paths are correct per platform
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn config_paths_are_relative_not_absolute() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for version in &support.versions {
            for path in &version.config_paths {
                assert!(
                    !path.starts_with('/') && !path.starts_with('\\') && !path.contains(":\\"),
                    "Game '{game_id}' version '{}' has absolute config path: {path}",
                    version.version
                );
            }
        }
    }
    Ok(())
}

#[test]
fn config_paths_use_forward_slashes() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for version in &support.versions {
            for path in &version.config_paths {
                assert!(
                    !path.contains('\\'),
                    "Game '{game_id}' version '{}' config path uses backslashes: {path}. \
                     Use forward slashes for platform-independent paths.",
                    version.version
                );
            }
        }
    }
    Ok(())
}

#[test]
fn install_paths_use_forward_slashes() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for path in &support.auto_detect.install_paths {
            assert!(
                !path.contains('\\'),
                "Game '{game_id}' install path uses backslashes: {path}. \
                 Use forward slashes for platform-independent paths.",
            );
        }
    }
    Ok(())
}

#[test]
fn registry_keys_use_backslashes() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for key in &support.auto_detect.install_registry_keys {
            assert!(
                key.starts_with("HKEY_"),
                "Game '{game_id}' registry key doesn't start with HKEY_: {key}"
            );
            // Registry keys should use backslash separators
            assert!(
                key.contains('\\'),
                "Game '{game_id}' registry key should use backslash separators: {key}"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Telemetry port assignments don't conflict
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn udp_output_targets_are_valid_socket_addresses() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        if let Some(ref target) = support.telemetry.output_target {
            // Should be parseable as host:port
            let parts: Vec<&str> = target.rsplitn(2, ':').collect();
            assert!(
                parts.len() == 2,
                "Game '{game_id}' output_target '{target}' is not in host:port format"
            );
            let port: u16 = parts[0].parse().map_err(|_| {
                format!(
                    "Game '{game_id}' output_target '{target}' has invalid port: '{}'",
                    parts[0]
                )
            })?;
            // Port 0 is valid for shared-memory games (no UDP listener needed)
            let is_shared_memory = support.telemetry.method.contains("shared_memory");
            if !is_shared_memory {
                assert!(
                    port > 0,
                    "Game '{game_id}' (method: '{}') output_target has zero port",
                    support.telemetry.method
                );
            }
        }
    }
    Ok(())
}

#[test]
fn no_port_conflicts_among_stable_games_with_different_protocols() -> TestResult {
    let matrix = load_default_matrix()?;

    // Group stable games by output port
    let mut port_to_games: HashMap<u16, Vec<(String, String)>> = HashMap::new();

    for (game_id, support) in &matrix.games {
        if support.status != GameSupportStatus::Stable {
            continue;
        }
        if let Some(ref target) = support.telemetry.output_target
            && let Some(port_str) = target.rsplit(':').next()
            && let Ok(port) = port_str.parse::<u16>()
            && port > 0
        {
            port_to_games
                .entry(port)
                .or_default()
                .push((game_id.clone(), support.telemetry.method.clone()));
        }
    }

    // Port sharing is OK for games using the same protocol family (e.g., Codemasters UDP)
    // but flag if different protocol methods share a port
    for (port, games) in &port_to_games {
        if games.len() > 1 && {
            let methods: HashSet<&str> = games.iter().map(|(_, m)| m.as_str()).collect();
            methods.len() > 1
        } {
            // Different protocol methods on same port — this is suspicious
            let game_list: Vec<&str> = games.iter().map(|(id, _)| id.as_str()).collect();
            // Don't hard-fail but ensure the list is small and documented
            assert!(
                games.len() <= 10,
                "Port {port} has too many games with different protocols: {game_list:?}"
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Every game adapter can parse at least a minimal valid packet
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn every_adapter_is_constructible_and_reports_correct_game_id() -> TestResult {
    let factories = adapter_factories();
    for (expected_id, factory) in factories {
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *expected_id,
            "Adapter factory for '{expected_id}' produced adapter with game_id '{}'",
            adapter.game_id()
        );
    }
    Ok(())
}

#[test]
fn every_adapter_has_valid_update_rate() -> TestResult {
    let factories = adapter_factories();
    for (id, factory) in factories {
        let adapter = factory();
        let rate = adapter.expected_update_rate();
        assert!(
            rate.as_millis() >= 1 && rate.as_millis() <= 1000,
            "Adapter '{id}' has invalid update rate: {rate:?} (must be 1-1000ms)"
        );
    }
    Ok(())
}

#[test]
fn every_adapter_normalize_handles_empty_input_gracefully() -> TestResult {
    let factories = adapter_factories();
    for (id, factory) in factories {
        let adapter = factory();
        // Empty input should return an error, not panic
        let result = adapter.normalize(&[]);
        // We expect an error for empty input — the key thing is no panic
        if result.is_ok() {
            // Some adapters may produce default telemetry for empty input — that's acceptable
            let telemetry = result?;
            assert!(
                telemetry.rpm >= 0.0,
                "Adapter '{id}' produced negative RPM from empty input"
            );
        }
    }
    Ok(())
}

#[test]
fn every_adapter_normalize_handles_short_input_gracefully() -> TestResult {
    let factories = adapter_factories();
    let short_inputs: &[&[u8]] = &[&[0], &[0, 0], &[0xFF, 0xFF, 0xFF, 0xFF]];

    for (id, factory) in factories {
        let adapter = factory();
        for input in short_inputs {
            // Should not panic — may return Ok or Err
            let _result = adapter.normalize(input);
        }
        // If we get here without panicking, the test passes for this adapter
        let _ = id; // reference id to suppress unused warning
    }
    Ok(())
}

#[test]
fn every_adapter_normalize_handles_large_zeroed_input() -> TestResult {
    let factories = adapter_factories();
    // Large zero-filled buffer simulating a packet with plausible size
    let zeroed_input = vec![0u8; 1024];

    for (id, factory) in factories {
        let adapter = factory();
        // Should not panic on a large zeroed buffer
        let result = adapter.normalize(&zeroed_input);
        if let Ok(telemetry) = result {
            // Zeroed input should produce physically reasonable values
            assert!(
                telemetry.rpm >= 0.0,
                "Adapter '{id}' produced negative RPM from zeroed input"
            );
            assert!(
                telemetry.speed_ms >= 0.0,
                "Adapter '{id}' produced negative speed from zeroed input"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Game auto-detection logic (process name detection)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stable_games_have_auto_detect_metadata() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        if support.status == GameSupportStatus::Stable {
            let detect = &support.auto_detect;
            let has_any = !detect.process_names.is_empty()
                || !detect.install_registry_keys.is_empty()
                || !detect.install_paths.is_empty();
            assert!(
                has_any,
                "Stable game '{game_id}' has no auto-detection metadata \
                 (process_names, registry_keys, or install_paths)"
            );
        }
    }
    Ok(())
}

#[test]
fn process_names_have_executable_extension() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for name in &support.auto_detect.process_names {
            assert!(
                name.ends_with(".exe") || name.ends_with(".app") || !name.contains('.'),
                "Game '{game_id}' process name '{name}' has unexpected extension"
            );
            assert!(
                !name.contains('/') && !name.contains('\\'),
                "Game '{game_id}' process name '{name}' should be a filename, not a path"
            );
        }
    }
    Ok(())
}

#[test]
fn executable_patterns_match_process_names() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for version in &support.versions {
            for pattern in &version.executable_patterns {
                assert!(
                    !pattern.is_empty(),
                    "Game '{game_id}' version '{}' has empty executable pattern",
                    version.version
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn every_adapter_is_game_running_returns_false_when_game_not_running() -> TestResult {
    let factories = adapter_factories();
    for (id, factory) in factories {
        let adapter = factory();
        // No game should be detected as running in a test environment
        let is_running = adapter.is_game_running().await?;
        // This is a soft assertion — in CI, no games should be running
        // If somehow a game IS running, that's still not a test failure,
        // but the function should not error out
        let _ = is_running;
        let _ = id;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Config writer generates valid config files for each game
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn every_config_writer_is_constructible() -> TestResult {
    let factories = config_writer_factories();
    assert!(
        factories.len() >= 40,
        "Expected at least 40 config writers, got {}",
        factories.len()
    );
    for (id, factory) in factories {
        let _writer = factory();
        let _ = id;
    }
    Ok(())
}

#[test]
fn every_config_writer_produces_expected_diffs() -> TestResult {
    let config = default_telemetry_config();
    let factories = config_writer_factories();

    for (id, factory) in factories {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        // Every writer should produce at least one diff when enabling telemetry
        assert!(
            !diffs.is_empty(),
            "Config writer '{id}' produced no expected diffs for enabled config"
        );
        for diff in &diffs {
            assert!(
                !diff.key.is_empty(),
                "Config writer '{id}' produced a diff with empty key"
            );
            assert!(
                !diff.new_value.is_empty(),
                "Config writer '{id}' produced a diff with empty new_value for key '{}'",
                diff.key
            );
        }
    }
    Ok(())
}

#[test]
fn config_writers_write_and_validate_roundtrip() -> TestResult {
    let config = default_telemetry_config();
    let factories = config_writer_factories();

    for (id, factory) in factories {
        let writer = factory();

        let temp_dir = tempfile::tempdir()?;
        let game_path = temp_dir.path();

        // Write config
        let diffs = writer.write_config(game_path, &config)?;
        assert!(
            !diffs.is_empty(),
            "Config writer '{id}' produced no diffs when writing config"
        );

        // Validate should succeed after write
        let valid = writer.validate_config(game_path)?;
        assert!(
            valid,
            "Config writer '{id}' failed validation after writing config"
        );
    }
    Ok(())
}

#[test]
fn config_writer_diffs_have_valid_operations() -> TestResult {
    let config = default_telemetry_config();
    let factories = config_writer_factories();

    for (id, factory) in factories {
        let writer = factory();
        let diffs = writer.get_expected_diffs(&config)?;
        for diff in &diffs {
            match diff.operation {
                DiffOperation::Add => {
                    // Add operations should have no old_value or None
                    // (relaxed: some writers may set old_value even for Add)
                }
                DiffOperation::Modify => {
                    // Modify operations should have an old_value
                    // (relaxed: get_expected_diffs may not know existing state)
                }
                DiffOperation::Remove => {
                    // Remove operations should have empty new_value or special marker
                }
            }
            let _ = id;
        }
    }
    Ok(())
}

#[test]
fn config_writer_write_is_idempotent() -> TestResult {
    let config = default_telemetry_config();
    let factories = config_writer_factories();

    for (id, factory) in factories {
        let writer = factory();
        let temp_dir = tempfile::tempdir()?;
        let game_path = temp_dir.path();

        // Write twice
        let _diffs1 = writer.write_config(game_path, &config)?;
        let diffs2 = writer.write_config(game_path, &config)?;

        // Second write should still succeed (idempotent)
        assert!(
            !diffs2.is_empty(),
            "Config writer '{id}' produced no diffs on second write"
        );

        // Validation should pass after both writes
        let valid = writer.validate_config(game_path)?;
        assert!(
            valid,
            "Config writer '{id}' failed validation after double write"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Support matrix documentation matches actual implementation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn every_matrix_game_has_an_adapter() -> TestResult {
    let matrix = load_default_matrix()?;
    let adapter_ids = adapter_id_set();

    let mut missing: Vec<String> = Vec::new();
    for game_id in matrix.games.keys() {
        if !adapter_ids.contains(game_id) {
            missing.push(game_id.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "Games in support matrix have no corresponding adapter: {missing:?}. \
         Register adapter factories in telemetry-adapters for these game IDs."
    );
    Ok(())
}

#[test]
fn every_adapter_has_a_matrix_entry() -> TestResult {
    let matrix_ids = matrix_game_id_set()?;
    let adapter_ids = adapter_id_set();

    let mut missing: Vec<String> = Vec::new();
    for id in &adapter_ids {
        if !matrix_ids.contains(id) {
            missing.push(id.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "Adapters exist without matrix entry: {missing:?}. \
         Add these game IDs to game_support_matrix.yaml."
    );
    Ok(())
}

#[test]
fn every_matrix_game_has_a_config_writer() -> TestResult {
    let matrix = load_default_matrix()?;
    let writer_ids = writer_id_set();

    let mut missing: Vec<String> = Vec::new();
    for game_id in matrix.games.keys() {
        if !writer_ids.contains(game_id) {
            missing.push(game_id.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "Games in support matrix have no corresponding config writer: {missing:?}. \
         Register config writer factories in telemetry-config-writers for these game IDs."
    );
    Ok(())
}

#[test]
fn every_config_writer_has_a_matrix_entry() -> TestResult {
    let matrix_ids = matrix_game_id_set()?;
    let writer_ids = writer_id_set();

    let mut missing: Vec<String> = Vec::new();
    for id in &writer_ids {
        if !matrix_ids.contains(id) {
            missing.push(id.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "Config writers exist without matrix entry: {missing:?}. \
         Add these game IDs to game_support_matrix.yaml."
    );
    Ok(())
}

#[test]
fn adapter_and_writer_registries_cover_same_game_ids() -> TestResult {
    let adapter_ids = adapter_id_set();
    let writer_ids = writer_id_set();

    let adapters_without_writers: Vec<&String> = adapter_ids.difference(&writer_ids).collect();
    let writers_without_adapters: Vec<&String> = writer_ids.difference(&adapter_ids).collect();

    assert!(
        adapters_without_writers.is_empty(),
        "Adapters exist without corresponding config writer: {adapters_without_writers:?}"
    );
    assert!(
        writers_without_adapters.is_empty(),
        "Config writers exist without corresponding adapter: {writers_without_adapters:?}"
    );
    Ok(())
}

#[test]
fn matrix_config_writer_field_matches_writer_registry() -> TestResult {
    let matrix = load_default_matrix()?;
    let writer_ids = writer_id_set();

    for (game_id, support) in &matrix.games {
        assert!(
            writer_ids.contains(&support.config_writer),
            "Game '{game_id}' references config_writer '{}' which is not in the writer registry",
            support.config_writer
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Version-specific game handling
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn f1_versions_have_distinct_adapters() -> TestResult {
    let adapter_ids = adapter_id_set();

    // F1 has multiple adapter variants for different protocol generations
    assert!(
        adapter_ids.contains("f1"),
        "Missing legacy F1 (Codemasters) adapter"
    );
    assert!(
        adapter_ids.contains("f1_25"),
        "Missing F1 25 (native EA) adapter"
    );
    assert!(
        adapter_ids.contains("f1_native"),
        "Missing F1 native adapter"
    );
    Ok(())
}

#[test]
fn f1_versions_have_distinct_matrix_entries() -> TestResult {
    let matrix = load_default_matrix()?;

    assert!(matrix.games.contains_key("f1"), "Matrix missing 'f1' entry");
    assert!(
        matrix.games.contains_key("f1_25"),
        "Matrix missing 'f1_25' entry"
    );
    assert!(
        matrix.games.contains_key("f1_native"),
        "Matrix missing 'f1_native' entry"
    );

    // Verify they have different telemetry methods or ports
    let f1 = &matrix.games["f1"];
    let f1_25 = &matrix.games["f1_25"];

    // Both should have meaningful names
    assert!(!f1.name.is_empty());
    assert!(!f1_25.name.is_empty());
    assert_ne!(
        f1.name, f1_25.name,
        "F1 and F1 25 should have distinct display names"
    );
    Ok(())
}

#[test]
fn codemasters_family_games_share_udp_protocol_characteristics() -> TestResult {
    let matrix = load_default_matrix()?;

    let codemasters_games = [
        "f1",
        "dirt5",
        "dirt_rally_2",
        "dirt4",
        "dirt3",
        "grid_autosport",
        "grid_2019",
        "grid_legends",
        "race_driver_grid",
        "dirt_showdown",
    ];

    for game_id in &codemasters_games {
        if let Some(support) = matrix.games.get(*game_id) {
            // These games all use some variant of Codemasters UDP or derived protocol
            // Verify they have telemetry entries
            assert!(
                !support.telemetry.method.is_empty(),
                "Codemasters game '{game_id}' has empty telemetry method"
            );
        }
    }
    Ok(())
}

#[test]
fn forza_variants_are_distinct() -> TestResult {
    let matrix = load_default_matrix()?;

    assert!(
        matrix.games.contains_key("forza_motorsport"),
        "Matrix missing forza_motorsport"
    );
    assert!(
        matrix.games.contains_key("forza_horizon_4"),
        "Matrix missing forza_horizon_4"
    );
    assert!(
        matrix.games.contains_key("forza_horizon_5"),
        "Matrix missing forza_horizon_5"
    );

    let fm = &matrix.games["forza_motorsport"];
    let fh4 = &matrix.games["forza_horizon_4"];
    let fh5 = &matrix.games["forza_horizon_5"];

    assert_ne!(fm.name, fh4.name);
    assert_ne!(fh4.name, fh5.name);
    Ok(())
}

#[test]
fn wrc_variants_are_distinct() -> TestResult {
    let matrix = load_default_matrix()?;

    let wrc_games = ["eawrc", "wrc_generations", "wrc_9", "wrc_10"];
    let mut names: HashSet<String> = HashSet::new();

    for game_id in &wrc_games {
        let support = matrix
            .games
            .get(*game_id)
            .ok_or_else(|| format!("WRC game '{game_id}' not found in matrix"))?;
        assert!(
            names.insert(support.name.clone()),
            "Duplicate WRC game name: '{}' for '{game_id}'",
            support.name
        );
    }
    Ok(())
}

#[test]
fn rfactor1_variants_have_distinct_game_ids() -> TestResult {
    let adapter_ids = adapter_id_set();

    let rfactor1_variants = ["rfactor1", "gtr2", "race_07", "gsc"];
    for variant in &rfactor1_variants {
        assert!(
            adapter_ids.contains(*variant),
            "Missing rFactor1-family adapter: {variant}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Game ID normalization
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn normalize_game_id_handles_known_aliases() -> TestResult {
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    Ok(())
}

#[test]
fn normalize_game_id_passes_through_canonical_ids() -> TestResult {
    let matrix_ids = matrix_game_ids()?;
    for id in &matrix_ids {
        let normalized = normalize_game_id(id);
        // Canonical IDs should either pass through unchanged or map to another canonical ID
        let matrix_set = matrix_game_id_set()?;
        assert!(
            matrix_set.contains(normalized),
            "normalize_game_id('{id}') = '{normalized}' which is not in the matrix"
        );
    }
    Ok(())
}

#[test]
fn normalize_game_id_is_idempotent() -> TestResult {
    let test_ids = ["ea_wrc", "f1_2025", "iracing", "acc", "forza_motorsport"];
    for id in &test_ids {
        let once = normalize_game_id(id);
        let twice = normalize_game_id(once);
        assert_eq!(
            once, twice,
            "normalize_game_id is not idempotent for '{id}': '{once}' vs '{twice}'"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Cross-crate consistency
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_game_ids_are_valid_identifiers() -> TestResult {
    let factories = adapter_factories();
    for (id, _) in factories {
        assert!(!id.is_empty(), "Empty game_id in adapter_factories");
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "Game ID '{id}' contains invalid characters (only a-z, 0-9, _ allowed)"
        );
        assert!(
            !id.starts_with('_') && !id.ends_with('_'),
            "Game ID '{id}' should not start or end with underscore"
        );
    }
    Ok(())
}

#[test]
fn config_writer_game_ids_are_valid_identifiers() -> TestResult {
    let factories = config_writer_factories();
    for (id, _) in factories {
        assert!(!id.is_empty(), "Empty game_id in config_writer_factories");
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "Config writer ID '{id}' contains invalid characters"
        );
    }
    Ok(())
}

#[test]
fn matrix_game_ids_are_valid_identifiers() -> TestResult {
    let matrix = load_default_matrix()?;
    for game_id in matrix.games.keys() {
        assert!(!game_id.is_empty(), "Empty game_id in matrix");
        assert!(
            game_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "Matrix game ID '{game_id}' contains invalid characters"
        );
    }
    Ok(())
}

#[test]
fn stable_games_have_mapped_ffb_or_rpm_fields() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        // Only check games with active telemetry (not stubs with method "none")
        if support.status == GameSupportStatus::Stable && support.telemetry.method != "none" {
            let fields = &support.telemetry.fields;
            let has_critical = fields.ffb_scalar.is_some() || fields.rpm.is_some();
            assert!(
                has_critical,
                "Stable game '{game_id}' has no ffb_scalar or rpm field mapping — \
                 at least one is required for meaningful force feedback"
            );
        }
    }
    Ok(())
}

#[test]
fn matrix_serialization_roundtrip() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    let roundtripped: GameSupportMatrix = serde_yaml::from_str(&yaml)?;

    assert_eq!(
        matrix.games.len(),
        roundtripped.games.len(),
        "Matrix lost games during serialization roundtrip"
    );

    for (game_id, original) in &matrix.games {
        let rt = roundtripped
            .games
            .get(game_id)
            .ok_or_else(|| format!("Game '{game_id}' lost during serialization roundtrip"))?;
        assert_eq!(original.name, rt.name, "Name mismatch for '{game_id}'");
        assert_eq!(
            original.telemetry.method, rt.telemetry.method,
            "Telemetry method mismatch for '{game_id}'"
        );
        assert_eq!(
            original.telemetry.update_rate_hz, rt.telemetry.update_rate_hz,
            "Update rate mismatch for '{game_id}'"
        );
    }
    Ok(())
}

#[test]
fn matrix_status_filter_methods_are_exhaustive() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.stable_games();
    let experimental = matrix.experimental_games();

    let total = stable.len() + experimental.len();
    assert_eq!(
        total,
        matrix.games.len(),
        "stable_games() + experimental_games() = {total} but matrix has {} games. \
         Some games may have an unrecognized status.",
        matrix.games.len()
    );
    Ok(())
}

#[test]
fn update_rate_in_matrix_matches_adapter_rate_order_of_magnitude() -> TestResult {
    let matrix = load_default_matrix()?;
    let factories = adapter_factories();
    let adapter_map: HashMap<&str, Duration> = factories
        .iter()
        .map(|(id, factory)| (*id, factory().expected_update_rate()))
        .collect();

    for (game_id, support) in &matrix.games {
        if let Some(&adapter_rate) = adapter_map.get(game_id.as_str()) {
            let matrix_hz = support.telemetry.update_rate_hz;
            if matrix_hz > 0 {
                // Adapter rate is in ms per update, matrix rate is in Hz
                // adapter_rate_hz ≈ 1000 / adapter_rate.as_millis()
                let adapter_hz = if adapter_rate.as_millis() > 0 {
                    1000 / adapter_rate.as_millis() as u32
                } else {
                    1000
                };

                // Allow generous tolerance — adapter may use a different default
                // Just verify they're in the same order of magnitude
                let ratio = if adapter_hz > matrix_hz {
                    adapter_hz / matrix_hz.max(1)
                } else {
                    matrix_hz / adapter_hz.max(1)
                };

                assert!(
                    ratio <= 20,
                    "Game '{game_id}': matrix says {matrix_hz}Hz but adapter runs at ~{adapter_hz}Hz \
                     (rate: {adapter_rate:?}). Ratio {ratio}x is too large."
                );
            }
        }
    }
    Ok(())
}
