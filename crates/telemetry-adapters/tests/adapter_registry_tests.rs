//! Deep tests for the adapter registry (adapter_factories).

use openracing_telemetry_adapters::adapter_factories;
use std::collections::{HashMap, HashSet};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Registry population tests ───────────────────────────────────────────

#[test]
fn registry_has_at_least_61_adapters() {
    let factories = adapter_factories();
    assert!(
        factories.len() >= 61,
        "Expected at least 61 adapters in the registry, got {}",
        factories.len()
    );
}

#[test]
fn registry_all_game_ids_unique() -> TestResult {
    let factories = adapter_factories();
    let mut seen = HashSet::new();
    for (id, _) in factories {
        assert!(
            seen.insert(*id),
            "Duplicate game_id in adapter_factories: {id}"
        );
    }
    Ok(())
}

#[test]
fn registry_all_adapters_constructible() -> TestResult {
    let factories = adapter_factories();
    for (id, factory) in factories {
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *id,
            "Factory for '{id}' produced adapter with mismatched game_id '{}'",
            adapter.game_id()
        );
    }
    Ok(())
}

// ── Discovery by game name tests ────────────────────────────────────────

#[test]
fn registry_find_adapter_by_exact_id() -> TestResult {
    let factories = adapter_factories();
    let map: HashMap<&str, _> = factories.iter().map(|&(id, f)| (id, f)).collect();

    let adapter = (map.get("iracing").ok_or("iracing not found")?)();
    assert_eq!(adapter.game_id(), "iracing");
    Ok(())
}

#[test]
fn registry_core_racing_sims_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    let required = [
        "acc",
        "acc2",
        "iracing",
        "rfactor2",
        "raceroom",
        "ams2",
        "forza_motorsport",
        "gran_turismo_7",
        "project_cars_2",
        "f1",
        "eawrc",
        "beamng_drive",
    ];

    for game in &required {
        assert!(
            ids.contains(game),
            "Core racing sim '{game}' missing from adapter registry"
        );
    }
    Ok(())
}

#[test]
fn registry_rally_games_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    let rally_games = [
        "dirt_rally_2",
        "eawrc",
        "rbr",
        "wrc_generations",
        "dirt3",
        "dirt4",
    ];

    for game in &rally_games {
        assert!(
            ids.contains(game),
            "Rally game '{game}' missing from adapter registry"
        );
    }
    Ok(())
}

#[test]
fn registry_codemasters_family_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    let codemasters = [
        "f1",
        "f1_25",
        "dirt_rally_2",
        "dirt5",
        "grid_legends",
        "grid_2019",
    ];
    for game in &codemasters {
        assert!(
            ids.contains(game),
            "Codemasters game '{game}' missing from adapter registry"
        );
    }
    Ok(())
}

#[test]
fn registry_rfactor1_variants_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    assert!(ids.contains("rfactor1"));
    assert!(ids.contains("gtr2"));
    assert!(ids.contains("race_07"));
    assert!(ids.contains("gsc"));
    Ok(())
}

#[test]
fn registry_forza_variants_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    assert!(ids.contains("forza_motorsport"));
    assert!(ids.contains("forza_horizon_4"));
    assert!(ids.contains("forza_horizon_5"));
    Ok(())
}

// ── Conflict detection tests ────────────────────────────────────────────

#[test]
fn registry_no_duplicate_factory_functions() -> TestResult {
    // Each factory function should produce a distinct game_id
    let factories = adapter_factories();
    let mut game_ids = Vec::new();
    for (_, factory) in factories {
        let adapter = factory();
        game_ids.push(adapter.game_id().to_string());
    }
    let unique: HashSet<&str> = game_ids.iter().map(|s| &**s).collect();
    assert_eq!(
        game_ids.len(),
        unique.len(),
        "Some factory functions produce adapters with the same game_id"
    );
    Ok(())
}

#[test]
fn registry_game_id_matches_key() -> TestResult {
    // The key in the registry must match what the adapter reports
    let factories = adapter_factories();
    for (key, factory) in factories {
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *key,
            "Registry key '{key}' does not match adapter game_id '{}'",
            adapter.game_id()
        );
    }
    Ok(())
}

#[test]
fn registry_all_update_rates_valid() -> TestResult {
    let factories = adapter_factories();
    for (id, factory) in factories {
        let adapter = factory();
        let rate = adapter.expected_update_rate();
        assert!(
            rate.as_millis() > 0 && rate.as_millis() <= 1000,
            "Adapter '{id}' has suspicious update rate: {rate:?}"
        );
    }
    Ok(())
}

#[test]
fn registry_no_empty_game_ids() -> TestResult {
    let factories = adapter_factories();
    for (key, factory) in factories {
        assert!(!key.is_empty(), "Registry contains empty key");
        let adapter = factory();
        assert!(
            !adapter.game_id().is_empty(),
            "Adapter produces empty game_id"
        );
    }
    Ok(())
}

#[test]
fn registry_new_wave_adapters_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    // Adapters added in recent waves
    let recent = [
        "ac_evo",
        "f1_native",
        "f1_25",
        "ride5",
        "motogp",
        "flatout",
        "mudrunner",
        "snowrunner",
        "dakar_desert_rally",
    ];
    for game in &recent {
        assert!(
            ids.contains(game),
            "Recent adapter '{game}' missing from registry"
        );
    }
    Ok(())
}

#[test]
fn registry_truck_sim_adapters_present() -> TestResult {
    let factories = adapter_factories();
    let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

    assert!(ids.contains("ets2"), "ETS2 adapter missing");
    assert!(ids.contains("ats"), "ATS adapter missing");
    Ok(())
}
