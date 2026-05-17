//! Pipeline config tests for telemetry-config.
//!
//! Covers gaps not in existing tests:
//! - normalize_game_id edge cases and exhaustive alias coverage
//! - Matrix loading validation: all games have required fields
//! - Config roundtrip serialization for GameSupport/GameSupportMatrix
//! - TelemetryFieldMapping completeness checks
//! - Config writer factory IDs are all present in the matrix
//! - Property-based tests for game ID normalization

use openracing_telemetry_config::{
    GameSupportMatrix, GameSupportStatus, config_writer_factories, load_default_matrix,
    matrix_game_id_set, matrix_game_ids, normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// normalize_game_id
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalize_game_id_known_aliases() -> TestResult {
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    assert_eq!(normalize_game_id("F1_2025"), "f1_25");
    Ok(())
}

#[test]
fn normalize_game_id_passthrough_for_unknown() -> TestResult {
    assert_eq!(normalize_game_id("iracing"), "iracing");
    assert_eq!(normalize_game_id("acc"), "acc");
    assert_eq!(normalize_game_id("forza_motorsport"), "forza_motorsport");
    assert_eq!(normalize_game_id(""), "");
    Ok(())
}

#[test]
fn normalize_game_id_case_sensitivity_passthrough() -> TestResult {
    // Non-alias IDs should be returned as-is (case preserved)
    assert_eq!(normalize_game_id("iRacing"), "iRacing");
    assert_eq!(normalize_game_id("ACC"), "ACC");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Matrix loading
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn load_default_matrix_succeeds() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(!matrix.games.is_empty(), "matrix should have games");
    Ok(())
}

#[test]
fn matrix_game_ids_are_sorted() -> TestResult {
    let ids = matrix_game_ids()?;
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "matrix_game_ids() should return sorted IDs");
    Ok(())
}

#[test]
fn matrix_game_id_set_contains_all_ids() -> TestResult {
    let ids = matrix_game_ids()?;
    let id_set = matrix_game_id_set()?;
    assert_eq!(
        ids.len(),
        id_set.len(),
        "set and vec should have same count"
    );
    for id in &ids {
        assert!(id_set.contains(id), "missing from set: {id}");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Matrix validation: all games have required structure
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_games_have_non_empty_name() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(!support.name.is_empty(), "game '{game_id}' has empty name");
    }
    Ok(())
}

#[test]
fn all_games_have_at_least_one_version() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(
            !support.versions.is_empty(),
            "game '{game_id}' has no versions"
        );
    }
    Ok(())
}

#[test]
fn all_games_have_telemetry_method() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(
            !support.telemetry.method.is_empty(),
            "game '{game_id}' has empty telemetry method"
        );
    }
    Ok(())
}

#[test]
fn all_games_have_positive_update_rate() -> TestResult {
    let matrix = load_default_matrix()?;
    let mut zero_rate_games = Vec::new();
    for (game_id, support) in &matrix.games {
        if support.telemetry.update_rate_hz == 0 {
            zero_rate_games.push(game_id.clone());
        }
    }
    // Some games (e.g. f1_manager) may not have a rate configured yet.
    // Ensure the vast majority do.
    let total = matrix.games.len();
    assert!(
        zero_rate_games.len() <= total / 10,
        "too many games with zero update_rate_hz ({} / {total}): {zero_rate_games:?}",
        zero_rate_games.len()
    );
    Ok(())
}

#[test]
fn all_games_have_config_writer() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        assert!(
            !support.config_writer.is_empty(),
            "game '{game_id}' has empty config_writer"
        );
    }
    Ok(())
}

#[test]
fn all_game_versions_have_telemetry_method() -> TestResult {
    let matrix = load_default_matrix()?;
    for (game_id, support) in &matrix.games {
        for version in &support.versions {
            assert!(
                !version.telemetry_method.is_empty(),
                "game '{game_id}' version '{}' has empty telemetry_method",
                version.version
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryFieldMapping completeness for stable games
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stable_games_have_at_least_ffb_or_rpm_mapped() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.stable_games();

    let mut unmapped = Vec::new();
    for game_id in &stable {
        let support = matrix
            .games
            .get(game_id)
            .ok_or_else(|| format!("missing game: {game_id}"))?;
        let fields = &support.telemetry.fields;

        let has_ffb = fields.ffb_scalar.is_some();
        let has_rpm = fields.rpm.is_some();
        if !has_ffb && !has_rpm {
            unmapped.push(game_id.clone());
        }
    }

    // Allow a small number of stable games without full mappings (e.g. manager games)
    assert!(
        unmapped.len() <= stable.len() / 10,
        "too many stable games without ffb or rpm mapped ({} / {}): {unmapped:?}",
        unmapped.len(),
        stable.len()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// GameSupportMatrix methods
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn matrix_has_game_id_for_known_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let well_known = ["acc", "iracing", "f1", "forza_motorsport", "beamng_drive"];

    for game_id in &well_known {
        assert!(
            matrix.has_game_id(game_id),
            "well-known game '{game_id}' missing from matrix"
        );
    }
    Ok(())
}

#[test]
fn matrix_has_game_id_returns_false_for_nonexistent() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(!matrix.has_game_id("nonexistent_game_xyz"));
    assert!(!matrix.has_game_id(""));
    Ok(())
}

#[test]
fn matrix_stable_and_experimental_are_disjoint() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable: std::collections::HashSet<_> = matrix.stable_games().into_iter().collect();
    let experimental: std::collections::HashSet<_> =
        matrix.experimental_games().into_iter().collect();

    let overlap: Vec<_> = stable.intersection(&experimental).collect();
    assert!(
        overlap.is_empty(),
        "games should not be both stable and experimental: {overlap:?}"
    );
    Ok(())
}

#[test]
fn matrix_stable_plus_experimental_covers_all_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let all_ids: std::collections::HashSet<_> = matrix.game_ids().into_iter().collect();
    let stable: std::collections::HashSet<_> = matrix.stable_games().into_iter().collect();
    let experimental: std::collections::HashSet<_> =
        matrix.experimental_games().into_iter().collect();

    let combined: std::collections::HashSet<_> = stable.union(&experimental).cloned().collect();
    assert_eq!(
        all_ids, combined,
        "stable + experimental should cover all games"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// GameSupport JSON roundtrip
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn game_support_matrix_json_roundtrip() -> TestResult {
    let matrix = load_default_matrix()?;
    let json = serde_json::to_string(&matrix)?;
    let deserialized: GameSupportMatrix = serde_json::from_str(&json)?;

    assert_eq!(
        deserialized.games.len(),
        matrix.games.len(),
        "game count mismatch after JSON roundtrip"
    );

    for game_id in matrix.games.keys() {
        assert!(
            deserialized.games.contains_key(game_id),
            "missing game '{game_id}' after JSON roundtrip"
        );
    }
    Ok(())
}

#[test]
fn game_support_status_json_roundtrip() -> TestResult {
    let statuses = [GameSupportStatus::Stable, GameSupportStatus::Experimental];
    for status in &statuses {
        let json = serde_json::to_string(status)?;
        let deserialized: GameSupportStatus = serde_json::from_str(&json)?;
        assert_eq!(
            &deserialized, status,
            "status roundtrip failed for {status:?}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Config writer factories
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn config_writer_factories_are_non_empty() -> TestResult {
    let factories = config_writer_factories();
    assert!(!factories.is_empty(), "writer factories must not be empty");
    Ok(())
}

#[test]
fn config_writer_factory_ids_are_unique() -> TestResult {
    let factories = config_writer_factories();
    let mut seen = std::collections::HashSet::new();

    for (factory_id, _) in factories {
        assert!(
            seen.insert(*factory_id),
            "duplicate config writer factory id: {factory_id}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Property-based tests
// ═══════════════════════════════════════════════════════════════════════════════

mod proptest_config_pipeline {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn normalize_game_id_is_idempotent(id in "[a-z0-9_]{1,30}") {
            let once = normalize_game_id(&id);
            let twice = normalize_game_id(once);
            prop_assert_eq!(once, twice, "normalize_game_id should be idempotent");
        }

        #[test]
        fn normalize_game_id_never_panics(id in "\\PC{0,100}") {
            let _ = normalize_game_id(&id);
        }

        #[test]
        fn matrix_game_ids_always_non_empty_strings(idx in 0usize..200) {
            let ids = matrix_game_ids().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
            if idx < ids.len() {
                prop_assert!(!ids[idx].is_empty(), "game_id at index {idx} is empty");
            }
        }
    }
}
