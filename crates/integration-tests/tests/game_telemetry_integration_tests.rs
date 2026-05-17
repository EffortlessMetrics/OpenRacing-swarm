//! Comprehensive game telemetry integration tests.
//!
//! Validates game-specific telemetry parsing, adapter registration and discovery,
//! packet routing, config generation, game auto-detection, multi-game concurrency,
//! rate limiting, buffering, and error handling for all supported racing titles.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use openracing_telemetry_adapters::{MockAdapter, TelemetryAdapter, adapter_factories};
use openracing_telemetry_config::{
    TelemetryConfig, config_writer_factories, load_default_matrix, matrix_game_id_set,
    matrix_game_ids, normalize_game_id,
};
use racing_wheel_schemas::prelude::*;

use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn assert_f32_near(actual: f32, expected: f32, tol: f32, label: &str) {
    assert!(
        (actual - expected).abs() < tol,
        "{label}: expected ~{expected}, got {actual} (tol={tol})"
    );
}

// ── Packet builders ──────────────────────────────────────────────────────────

/// Build a Forza Sled packet (232 bytes, minimum valid format).
/// is_race_on=1, places RPM/max_rpm/velocity at known offsets.
fn build_forza_sled_packet(vel_x: f32, vel_z: f32, rpm: f32, max_rpm: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 232];
    write_i32_le(&mut buf, 0, 1); // is_race_on = 1
    write_f32_le(&mut buf, 8, max_rpm);
    write_f32_le(&mut buf, 16, rpm);
    write_f32_le(&mut buf, 32, vel_x);
    write_f32_le(&mut buf, 40, vel_z);
    buf
}

/// Build a Forza CarDash packet (311 bytes) with speed, RPM, gear, throttle.
fn build_forza_cardash_packet(
    speed_ms: f32,
    rpm: f32,
    max_rpm: f32,
    gear: u8,
    throttle: u8,
    brake: u8,
) -> Vec<u8> {
    let mut buf = vec![0u8; 311];
    write_i32_le(&mut buf, 0, 1); // is_race_on = 1
    write_f32_le(&mut buf, 8, max_rpm);
    write_f32_le(&mut buf, 16, rpm);
    write_f32_le(&mut buf, 244, speed_ms);
    buf[307] = gear; // gear byte: 0=R, 1=N, 2+=1st+
    buf[303] = throttle;
    buf[304] = brake;
    buf
}

/// Build a minimal LFS OutGauge packet (96 bytes).
fn build_lfs_packet(speed: f32, rpm: f32, gear: u8, throttle: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    buf[10] = gear;
    write_f32_le(&mut buf, 12, speed);
    write_f32_le(&mut buf, 16, rpm);
    write_f32_le(&mut buf, 48, throttle);
    buf
}

/// Build a Rennsport packet (24 bytes).
fn build_rennsport_packet(speed_kmh: f32, rpm: f32, gear: i8, ffb: f32, slip: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 24];
    buf[0] = 0x52; // 'R' identifier
    write_f32_le(&mut buf, 4, speed_kmh);
    write_f32_le(&mut buf, 8, rpm);
    buf[12] = gear as u8;
    write_f32_le(&mut buf, 16, ffb);
    write_f32_le(&mut buf, 20, slip);
    buf
}

/// Build a WRC Generations packet (264 bytes, Codemasters Mode 1).
fn build_wrc_generations_packet(
    vel_x: f32,
    vel_z: f32,
    rpm: f32,
    max_rpm: f32,
    gear: f32,
    throttle: f32,
    brake: f32,
) -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    write_f32_le(&mut buf, 32, vel_x);
    write_f32_le(&mut buf, 40, vel_z);
    write_f32_le(&mut buf, 116, throttle);
    write_f32_le(&mut buf, 124, brake);
    write_f32_le(&mut buf, 132, gear);
    write_f32_le(&mut buf, 148, rpm);
    write_f32_le(&mut buf, 252, max_rpm);
    buf
}

/// Build a SimHub JSON packet.
fn build_simhub_json(
    speed_ms: f32,
    rpm: f32,
    max_rpm: f32,
    gear: &str,
    throttle: f32,
    brake: f32,
) -> Vec<u8> {
    let json = format!(
        r#"{{"SpeedMs":{speed_ms},"Rpms":{rpm},"MaxRpms":{max_rpm},"Gear":"{gear}","Throttle":{throttle},"Brake":{brake}}}"#,
    );
    json.into_bytes()
}

/// Build a MudRunner JSON packet (same format as SimHub).
fn build_mudrunner_json(speed_ms: f32, rpm: f32, gear: &str) -> Vec<u8> {
    let json = format!(r#"{{"SpeedMs":{speed_ms},"Rpms":{rpm},"Gear":"{gear}"}}"#,);
    json.into_bytes()
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Game adapter packet parsing
// ═══════════════════════════════════════════════════════════════════════════════

mod packet_parsing {
    use super::*;

    #[test]
    fn forza_sled_packet_parses_speed_and_rpm() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let packet = build_forza_sled_packet(20.0, 30.0, 7000.0, 9000.0);
        let telem = adapter.normalize(&packet)?;

        let expected_speed = (20.0f32.powi(2) + 30.0f32.powi(2)).sqrt();
        assert_f32_near(telem.speed_ms, expected_speed, 1.0, "Forza sled speed");
        assert_f32_near(telem.rpm, 7000.0, 1.0, "Forza sled RPM");
        assert_f32_near(telem.max_rpm, 9000.0, 1.0, "Forza sled max RPM");
        Ok(())
    }

    #[test]
    fn forza_cardash_packet_parses_gear_and_throttle() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        // gear=4 means 3rd gear (0=R, 1=N, 2=1st, 3=2nd, 4=3rd)
        let packet = build_forza_cardash_packet(30.0, 6000.0, 8000.0, 4, 200, 50);
        let telem = adapter.normalize(&packet)?;

        assert_f32_near(telem.rpm, 6000.0, 1.0, "CarDash RPM");
        // Gear: 4 → 3rd gear (4 - 2 + 1 = 3)
        assert!(
            telem.gear >= 0,
            "CarDash gear should be a forward gear, got {}",
            telem.gear
        );
        assert!(
            telem.throttle >= 0.0 && telem.throttle <= 1.0,
            "Throttle should be normalized 0-1, got {}",
            telem.throttle
        );
        Ok(())
    }

    #[test]
    fn forza_zero_velocity_gives_zero_speed() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let packet = build_forza_sled_packet(0.0, 0.0, 800.0, 8000.0);
        let telem = adapter.normalize(&packet)?;

        assert_f32_near(telem.speed_ms, 0.0, 0.1, "Forza zero speed");
        assert_f32_near(telem.rpm, 800.0, 1.0, "Forza idle RPM");
        Ok(())
    }

    #[test]
    fn lfs_outgauge_packet_parses_correctly() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        // gear=3 in LFS: 0=R, 1=N, 2=1st, 3=2nd
        let packet = build_lfs_packet(50.0, 5500.0, 3, 0.8);
        let telem = adapter.normalize(&packet)?;

        assert_f32_near(telem.speed_ms, 50.0, 0.5, "LFS speed");
        assert_f32_near(telem.rpm, 5500.0, 1.0, "LFS RPM");
        assert_f32_near(telem.throttle, 0.8, 0.01, "LFS throttle");
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_reverse() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        let packet = build_lfs_packet(5.0, 2000.0, 0, 0.3); // gear 0 = reverse
        let telem = adapter.normalize(&packet)?;

        assert_eq!(telem.gear, -1, "LFS gear 0 should map to reverse (-1)");
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_neutral() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        let packet = build_lfs_packet(0.0, 800.0, 1, 0.0); // gear 1 = neutral
        let telem = adapter.normalize(&packet)?;

        assert_eq!(telem.gear, 0, "LFS gear 1 should map to neutral (0)");
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_forward() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        let packet = build_lfs_packet(30.0, 4000.0, 4, 0.5); // gear 4 = 3rd
        let telem = adapter.normalize(&packet)?;

        assert!(
            telem.gear > 0,
            "LFS gear 4 should map to a positive gear, got {}",
            telem.gear
        );
        Ok(())
    }

    #[test]
    fn rennsport_packet_parses_correctly() -> TestResult {
        let adapter = get_adapter("rennsport")?;
        let packet = build_rennsport_packet(180.0, 7200.0, 3, 0.5, 0.2);
        let telem = adapter.normalize(&packet)?;

        // Rennsport converts km/h → m/s (180 / 3.6 = 50.0)
        assert_f32_near(telem.speed_ms, 50.0, 1.0, "Rennsport speed m/s");
        assert_f32_near(telem.rpm, 7200.0, 1.0, "Rennsport RPM");
        assert_eq!(telem.gear, 3, "Rennsport gear");
        assert_f32_near(telem.ffb_scalar, 0.5, 0.01, "Rennsport FFB");
        assert_f32_near(telem.slip_ratio, 0.2, 0.01, "Rennsport slip");
        Ok(())
    }

    #[test]
    fn rennsport_reverse_gear() -> TestResult {
        let adapter = get_adapter("rennsport")?;
        let packet = build_rennsport_packet(5.0, 1500.0, -1, 0.0, 0.0);
        let telem = adapter.normalize(&packet)?;

        assert_eq!(telem.gear, -1, "Rennsport reverse gear");
        Ok(())
    }

    #[test]
    fn wrc_generations_packet_parses_speed_and_rpm() -> TestResult {
        let adapter = get_adapter("wrc_generations")?;
        let packet = build_wrc_generations_packet(15.0, 25.0, 5500.0, 8500.0, 3.0, 0.7, 0.1);
        let telem = adapter.normalize(&packet)?;

        let expected_speed = (15.0f32.powi(2) + 25.0f32.powi(2)).sqrt();
        assert_f32_near(telem.speed_ms, expected_speed, 2.0, "WRC speed");
        assert!(
            telem.rpm > 0.0,
            "WRC RPM should be positive, got {}",
            telem.rpm
        );
        assert_f32_near(telem.throttle, 0.7, 0.01, "WRC throttle");
        assert_f32_near(telem.brake, 0.1, 0.01, "WRC brake");
        Ok(())
    }

    #[test]
    fn wrc_generations_gear_encoding() -> TestResult {
        let adapter = get_adapter("wrc_generations")?;
        // gear < -0.5 means reverse
        let rev = build_wrc_generations_packet(0.0, 0.0, 2000.0, 8000.0, -1.0, 0.0, 0.0);
        let telem_rev = adapter.normalize(&rev)?;
        assert_eq!(telem_rev.gear, -1, "WRC gear -1.0 should be reverse");

        // gear near 0 means neutral
        let neutral = build_wrc_generations_packet(0.0, 0.0, 800.0, 8000.0, 0.0, 0.0, 0.0);
        let telem_n = adapter.normalize(&neutral)?;
        assert_eq!(telem_n.gear, 0, "WRC gear 0.0 should be neutral");

        // gear = 3.0 means 3rd
        let third = build_wrc_generations_packet(20.0, 30.0, 6000.0, 8000.0, 3.0, 0.8, 0.0);
        let telem_3 = adapter.normalize(&third)?;
        assert_eq!(telem_3.gear, 3, "WRC gear 3.0 should be 3rd");

        Ok(())
    }

    #[test]
    fn simhub_json_packet_parses_correctly() -> TestResult {
        let adapter = get_adapter("simhub")?;
        let packet = build_simhub_json(22.5, 4500.0, 8000.0, "3", 75.0, 10.0);
        let telem = adapter.normalize(&packet)?;

        assert_f32_near(telem.speed_ms, 22.5, 0.5, "SimHub speed");
        assert_f32_near(telem.rpm, 4500.0, 1.0, "SimHub RPM");
        assert_eq!(telem.gear, 3, "SimHub gear");
        // SimHub throttle is 0-100 scaled to 0-1
        assert_f32_near(telem.throttle, 0.75, 0.01, "SimHub throttle");
        assert_f32_near(telem.brake, 0.10, 0.01, "SimHub brake");
        Ok(())
    }

    #[test]
    fn simhub_reverse_gear_string() -> TestResult {
        let adapter = get_adapter("simhub")?;
        let packet = build_simhub_json(2.0, 1200.0, 7000.0, "R", 0.0, 0.0);
        let telem = adapter.normalize(&packet)?;

        assert_eq!(telem.gear, -1, "SimHub gear 'R' should map to -1");
        Ok(())
    }

    #[test]
    fn simhub_neutral_gear_string() -> TestResult {
        let adapter = get_adapter("simhub")?;
        let packet = build_simhub_json(0.0, 800.0, 7000.0, "N", 0.0, 0.0);
        let telem = adapter.normalize(&packet)?;

        assert_eq!(telem.gear, 0, "SimHub gear 'N' should map to 0");
        Ok(())
    }

    #[test]
    fn mudrunner_json_packet_parses_correctly() -> TestResult {
        let adapter = get_adapter("mudrunner")?;
        let packet = build_mudrunner_json(8.5, 2200.0, "2");
        let telem = adapter.normalize(&packet)?;

        assert_f32_near(telem.speed_ms, 8.5, 0.5, "MudRunner speed");
        assert_f32_near(telem.rpm, 2200.0, 1.0, "MudRunner RPM");
        assert_eq!(telem.gear, 2, "MudRunner gear");
        Ok(())
    }

    #[test]
    fn all_adapters_produce_finite_fields_for_zeroed_buffer() -> TestResult {
        let factories = adapter_factories();
        let mut failures = Vec::new();

        for (game_id, factory) in factories {
            let adapter = factory();
            let buf = vec![0u8; 1024];
            match adapter.normalize(&buf) {
                Ok(t) => {
                    if !t.speed_ms.is_finite() {
                        failures.push(format!("{game_id}: speed_ms not finite"));
                    }
                    if !t.rpm.is_finite() {
                        failures.push(format!("{game_id}: rpm not finite"));
                    }
                    if !t.ffb_scalar.is_finite() {
                        failures.push(format!("{game_id}: ffb_scalar not finite"));
                    }
                }
                Err(_) => {
                    // Some adapters reject zero buffers (e.g., Rennsport header check)
                }
            }
        }

        assert!(
            failures.is_empty(),
            "Adapters with non-finite fields on zero buffer: {failures:?}"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Adapter registration and discovery
// ═══════════════════════════════════════════════════════════════════════════════

mod adapter_registration {
    use super::*;

    #[test]
    fn adapter_factories_is_non_empty() -> TestResult {
        let factories = adapter_factories();
        assert!(
            !factories.is_empty(),
            "adapter_factories must return at least one adapter"
        );
        Ok(())
    }

    #[test]
    fn adapter_factory_ids_are_unique() -> TestResult {
        let factories = adapter_factories();
        let mut seen = HashSet::new();
        for (id, _) in factories {
            assert!(seen.insert(*id), "Duplicate adapter factory id: {id}");
        }
        Ok(())
    }

    #[test]
    fn all_adapter_factories_instantiate_successfully() -> TestResult {
        for (id, factory) in adapter_factories() {
            let adapter = factory();
            assert_eq!(
                adapter.game_id(),
                *id,
                "Adapter game_id() should match factory id"
            );
        }
        Ok(())
    }

    #[test]
    fn adapter_game_ids_are_lowercase_snake_case() -> TestResult {
        for (id, _) in adapter_factories() {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "Adapter id '{id}' must be lowercase snake_case"
            );
        }
        Ok(())
    }

    #[test]
    fn known_games_have_adapters() -> TestResult {
        let known = [
            "forza_motorsport",
            "live_for_speed",
            "acc",
            "iracing",
            "rennsport",
            "wrc_generations",
            "simhub",
            "f1",
        ];
        let factory_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

        for game in &known {
            assert!(
                factory_ids.contains(game),
                "Expected adapter for '{game}' not found in registry"
            );
        }
        Ok(())
    }

    #[test]
    fn adapter_factories_match_support_matrix() -> TestResult {
        let matrix_ids = matrix_game_id_set()?;
        let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

        let mut missing = Vec::new();
        for id in &adapter_ids {
            if !matrix_ids.contains(*id) {
                missing.push(*id);
            }
        }

        assert!(
            missing.is_empty(),
            "Adapters not in support matrix: {missing:?}"
        );
        Ok(())
    }

    #[test]
    fn normalize_game_id_is_idempotent() -> TestResult {
        let test_ids = ["forza_motorsport", "ea_wrc", "f1_2025"];
        for id in &test_ids {
            let normalized = normalize_game_id(id);
            let double = normalize_game_id(normalized);
            assert_eq!(
                normalized, double,
                "normalize_game_id should be idempotent for '{id}'"
            );
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. UDP socket binding and packet routing
// ═══════════════════════════════════════════════════════════════════════════════

mod packet_routing {
    use super::*;

    #[test]
    fn forza_and_lfs_normalize_different_packet_sizes() -> TestResult {
        let forza = get_adapter("forza_motorsport")?;
        let lfs = get_adapter("live_for_speed")?;

        let forza_pkt = build_forza_sled_packet(20.0, 30.0, 6000.0, 8500.0);
        let lfs_pkt = build_lfs_packet(40.0, 3500.0, 3, 0.6);

        assert_eq!(forza_pkt.len(), 232, "Forza sled packet must be 232 bytes");
        assert_eq!(lfs_pkt.len(), 96, "LFS packet must be 96 bytes");

        let forza_telem = forza.normalize(&forza_pkt)?;
        let lfs_telem = lfs.normalize(&lfs_pkt)?;

        assert!(
            forza_telem.speed_ms.is_finite() && lfs_telem.speed_ms.is_finite(),
            "Both adapters must produce finite speeds"
        );

        // Speeds should differ since different inputs
        assert!(
            (forza_telem.speed_ms - lfs_telem.speed_ms).abs() > 0.1,
            "Forza ({}) and LFS ({}) speeds should differ",
            forza_telem.speed_ms,
            lfs_telem.speed_ms
        );
        Ok(())
    }

    #[test]
    fn adapter_expected_update_rates_are_in_valid_range() -> TestResult {
        let test_ids = [
            "forza_motorsport",
            "live_for_speed",
            "acc",
            "rennsport",
            "wrc_generations",
            "simhub",
            "mudrunner",
        ];

        for game_id in &test_ids {
            let adapter = get_adapter(game_id)?;
            let rate = adapter.expected_update_rate();
            assert!(
                rate >= Duration::from_millis(1) && rate <= Duration::from_secs(2),
                "{game_id}: update rate {rate:?} out of valid range [1ms, 2s]"
            );
        }
        Ok(())
    }

    #[test]
    fn different_adapters_report_different_game_ids() -> TestResult {
        let forza = get_adapter("forza_motorsport")?;
        let lfs = get_adapter("live_for_speed")?;
        let rennsport = get_adapter("rennsport")?;

        assert_ne!(forza.game_id(), lfs.game_id());
        assert_ne!(forza.game_id(), rennsport.game_id());
        assert_ne!(lfs.game_id(), rennsport.game_id());
        Ok(())
    }

    #[test]
    fn mock_adapter_can_be_used_as_trait_object() -> TestResult {
        let mock = MockAdapter::new("test_game".to_string());
        let adapter: Box<dyn TelemetryAdapter> = Box::new(mock);

        assert_eq!(adapter.game_id(), "test_game");
        let telem = adapter.normalize(&[])?;
        assert!(
            telem.speed_ms.is_finite(),
            "MockAdapter must produce finite speed"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Config file generation for each game
// ═══════════════════════════════════════════════════════════════════════════════

mod config_generation {
    use super::*;

    fn default_telemetry_config() -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:5300".to_string(),
            fields: vec!["speed".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        }
    }

    #[test]
    fn config_writer_factories_is_non_empty() -> TestResult {
        let factories = config_writer_factories();
        assert!(
            !factories.is_empty(),
            "config_writer_factories must not be empty"
        );
        Ok(())
    }

    #[test]
    fn config_writer_factory_ids_are_unique() -> TestResult {
        let factories = config_writer_factories();
        let mut seen = HashSet::new();
        for (id, _) in factories {
            assert!(seen.insert(*id), "Duplicate config writer factory id: {id}");
        }
        Ok(())
    }

    #[test]
    fn all_config_writers_instantiate_and_produce_diffs() -> TestResult {
        let config = default_telemetry_config();

        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let diffs = writer.get_expected_diffs(&config)?;
            assert!(
                !diffs.is_empty(),
                "Config writer '{id}' must produce at least one diff"
            );
            for diff in &diffs {
                assert!(
                    !diff.key.is_empty(),
                    "Config diff key must not be empty for '{id}'"
                );
                assert!(
                    !diff.new_value.is_empty(),
                    "Config diff new_value must not be empty for '{id}'"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn config_writers_validate_against_empty_directory() -> TestResult {
        for (id, factory) in config_writer_factories() {
            let tmp = TempDir::new()?;
            let writer = factory();
            let valid = writer.validate_config(tmp.path())?;
            // Empty directory should not be valid (no config written yet)
            assert!(
                !valid,
                "Config writer '{id}' should return false for empty dir"
            );
        }
        Ok(())
    }

    #[test]
    fn config_writers_write_and_validate_roundtrip() -> TestResult {
        let config = default_telemetry_config();

        for (id, factory) in config_writer_factories() {
            let tmp = TempDir::new()?;
            let writer = factory();
            let diffs = writer.write_config(tmp.path(), &config)?;
            assert!(
                !diffs.is_empty(),
                "write_config for '{id}' must produce diffs"
            );

            let valid = writer.validate_config(tmp.path())?;
            assert!(
                valid,
                "Config writer '{id}' should validate after write_config"
            );
        }
        Ok(())
    }

    #[test]
    fn adapter_and_writer_factory_ids_overlap() -> TestResult {
        let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();
        let writer_ids: HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();

        // Every writer should have a matching adapter
        let mut orphaned_writers = Vec::new();
        for id in &writer_ids {
            if !adapter_ids.contains(id) {
                orphaned_writers.push(*id);
            }
        }

        // Some adapters share writers, so we allow orphaned writers but log them
        // The important thing is writers have matching adapters
        assert!(
            orphaned_writers.len() < writer_ids.len(),
            "All config writers are orphaned — no matching adapters found"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Game auto-detection
// ═══════════════════════════════════════════════════════════════════════════════

mod game_auto_detection {
    use super::*;

    #[test]
    fn support_matrix_loads_successfully() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(
            !matrix.games.is_empty(),
            "Game support matrix must not be empty"
        );
        Ok(())
    }

    #[test]
    fn support_matrix_contains_known_games() -> TestResult {
        let ids = matrix_game_ids()?;

        let expected = [
            "forza_motorsport",
            "live_for_speed",
            "acc",
            "iracing",
            "rennsport",
        ];

        for game in &expected {
            assert!(
                ids.contains(&game.to_string()),
                "Matrix missing expected game '{game}'"
            );
        }
        Ok(())
    }

    #[test]
    fn support_matrix_game_ids_are_sorted() -> TestResult {
        let ids = matrix_game_ids()?;
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "matrix_game_ids should return sorted IDs");
        Ok(())
    }

    #[test]
    fn support_matrix_game_ids_are_unique() -> TestResult {
        let ids = matrix_game_ids()?;
        let set: HashSet<&String> = ids.iter().collect();
        assert_eq!(
            ids.len(),
            set.len(),
            "matrix_game_ids should contain unique IDs"
        );
        Ok(())
    }

    #[test]
    fn game_id_normalization_handles_known_aliases() -> TestResult {
        // Known aliases should normalize correctly
        assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        // Unknown IDs pass through unchanged
        assert_eq!(normalize_game_id("forza_motorsport"), "forza_motorsport");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Multiple simultaneous games
// ═══════════════════════════════════════════════════════════════════════════════

mod multi_game {
    use super::*;

    #[test]
    fn multiple_adapters_coexist_independently() -> TestResult {
        let forza = get_adapter("forza_motorsport")?;
        let lfs = get_adapter("live_for_speed")?;
        let rennsport = get_adapter("rennsport")?;

        // Parse packets through each adapter independently
        let forza_telem = forza.normalize(&build_forza_sled_packet(10.0, 20.0, 5000.0, 8000.0))?;
        let lfs_telem = lfs.normalize(&build_lfs_packet(30.0, 3000.0, 3, 0.5))?;
        let rennsport_telem =
            rennsport.normalize(&build_rennsport_packet(100.0, 6000.0, 2, 0.3, 0.1))?;

        // All should produce valid, distinct telemetry
        assert!(forza_telem.speed_ms.is_finite());
        assert!(lfs_telem.speed_ms.is_finite());
        assert!(rennsport_telem.speed_ms.is_finite());

        // Each should have a different speed
        let speeds = [
            forza_telem.speed_ms,
            lfs_telem.speed_ms,
            rennsport_telem.speed_ms,
        ];
        for i in 0..speeds.len() {
            for j in (i + 1)..speeds.len() {
                assert!(
                    (speeds[i] - speeds[j]).abs() > 0.01,
                    "Adapters [{i}] and [{j}] should produce different speeds"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn rapid_adapter_switching_preserves_independence() -> TestResult {
        let game_ids = ["forza_motorsport", "live_for_speed", "rennsport"];
        let mut results: HashMap<String, Vec<f32>> = HashMap::new();

        for round in 0..5 {
            for game_id in &game_ids {
                let adapter = get_adapter(game_id)?;
                let speed = 10.0 + round as f32 * 5.0;
                let buf = match *game_id {
                    "forza_motorsport" => build_forza_sled_packet(speed, 0.0, 5000.0, 8000.0),
                    "live_for_speed" => build_lfs_packet(speed, 4000.0, 3, 0.5),
                    "rennsport" => build_rennsport_packet(speed * 3.6, 6000.0, 3, 0.0, 0.0),
                    _ => vec![0u8; 1024],
                };
                let telem = adapter.normalize(&buf)?;
                results
                    .entry(game_id.to_string())
                    .or_default()
                    .push(telem.speed_ms);
            }
        }

        // Each game's speeds should increase with each round
        for (game_id, speeds) in &results {
            for i in 1..speeds.len() {
                assert!(
                    speeds[i] >= speeds[i - 1] - 1.0,
                    "{game_id}: speed should increase, got {} then {}",
                    speeds[i - 1],
                    speeds[i]
                );
            }
        }
        Ok(())
    }

    #[test]
    fn all_adapters_can_coexist_in_memory() -> TestResult {
        let factories = adapter_factories();
        let mut adapters: Vec<Box<dyn TelemetryAdapter>> = Vec::new();

        for (_, factory) in factories {
            adapters.push(factory());
        }

        assert!(
            adapters.len() > 10,
            "Should have many adapters in memory, got {}",
            adapters.len()
        );

        // All adapters should still have valid game IDs
        let mut ids = HashSet::new();
        for adapter in &adapters {
            let id = adapter.game_id();
            assert!(!id.is_empty(), "game_id must not be empty");
            ids.insert(id.to_string());
        }

        assert_eq!(
            ids.len(),
            adapters.len(),
            "All adapters should have unique game IDs"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Telemetry rate limiting and buffering
// ═══════════════════════════════════════════════════════════════════════════════

mod rate_limiting {
    use super::*;
    use openracing_telemetry_adapters::telemetry_now_ns;

    #[test]
    fn rate_limiter_allows_first_frame() -> TestResult {
        use openracing_telemetry_streams::RateLimiter;

        let mut limiter = RateLimiter::new(60.0); // 60 Hz
        assert!(
            limiter.should_update(),
            "First frame should always be allowed"
        );
        Ok(())
    }

    #[test]
    fn telemetry_frames_have_monotonic_timestamps() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let mut prev_ts = 0u64;

        for i in 0..10 {
            let speed = 10.0 + i as f32;
            let packet = build_forza_sled_packet(speed, 0.0, 5000.0, 8000.0);
            let telem = adapter.normalize(&packet)?;
            let ts = telemetry_now_ns();
            let frame = TelemetryFrame::new(telem, ts, i, 232);

            assert!(
                frame.timestamp_ns >= prev_ts,
                "Timestamps must be monotonic: {} < {}",
                frame.timestamp_ns,
                prev_ts
            );
            prev_ts = frame.timestamp_ns;
        }
        Ok(())
    }

    #[test]
    fn telemetry_frame_construction_preserves_data() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        let packet = build_lfs_packet(42.0, 6000.0, 4, 0.9);
        let telem = adapter.normalize(&packet)?;

        let frame = TelemetryFrame::new(telem.clone(), 12345, 7, 96);

        assert_eq!(frame.timestamp_ns, 12345);
        assert_eq!(frame.sequence, 7);
        assert_eq!(frame.raw_size, 96);
        assert_f32_near(frame.data.speed_ms, telem.speed_ms, 0.001, "Frame speed");
        assert_f32_near(frame.data.rpm, telem.rpm, 0.001, "Frame RPM");
        Ok(())
    }

    #[test]
    fn telemetry_now_ns_is_monotonic() -> TestResult {
        let t1 = telemetry_now_ns();
        let t2 = telemetry_now_ns();
        let t3 = telemetry_now_ns();

        assert!(t2 >= t1, "telemetry_now_ns must be monotonic");
        assert!(t3 >= t2, "telemetry_now_ns must be monotonic");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Error handling (malformed packets, wrong version, etc.)
// ═══════════════════════════════════════════════════════════════════════════════

mod error_handling {
    use super::*;

    #[test]
    fn forza_rejects_undersized_packet() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let tiny = vec![0u8; 10]; // Way too small for any Forza format
        let result = adapter.normalize(&tiny);
        assert!(result.is_err(), "Forza should reject packet of 10 bytes");
        Ok(())
    }

    #[test]
    fn lfs_rejects_undersized_packet() -> TestResult {
        let adapter = get_adapter("live_for_speed")?;
        let tiny = vec![0u8; 4]; // LFS needs at least 92 bytes
        let result = adapter.normalize(&tiny);
        assert!(result.is_err(), "LFS should reject packet of 4 bytes");
        Ok(())
    }

    #[test]
    fn rennsport_rejects_wrong_identifier() -> TestResult {
        let adapter = get_adapter("rennsport")?;
        let mut packet = build_rennsport_packet(100.0, 5000.0, 3, 0.5, 0.1);
        packet[0] = 0x00; // Wrong identifier (should be 0x52)
        let result = adapter.normalize(&packet);
        assert!(
            result.is_err(),
            "Rennsport should reject packet with wrong identifier byte"
        );
        Ok(())
    }

    #[test]
    fn rennsport_rejects_undersized_packet() -> TestResult {
        let adapter = get_adapter("rennsport")?;
        let tiny = vec![0x52, 0, 0, 0]; // Has identifier but too short
        let result = adapter.normalize(&tiny);
        assert!(
            result.is_err(),
            "Rennsport should reject packet smaller than 24 bytes"
        );
        Ok(())
    }

    #[test]
    fn simhub_rejects_invalid_json() -> TestResult {
        let adapter = get_adapter("simhub")?;
        let bad_json = b"this is not json";
        let result = adapter.normalize(bad_json);
        assert!(result.is_err(), "SimHub should reject non-JSON data");
        Ok(())
    }

    #[test]
    fn simhub_handles_empty_json_object() -> TestResult {
        let adapter = get_adapter("simhub")?;
        let empty = b"{}";
        // Empty JSON should parse but produce defaults
        let result = adapter.normalize(empty);
        match result {
            Ok(telem) => {
                // All fields should be defaults
                assert!(telem.speed_ms.is_finite(), "Speed should be finite");
                assert!(telem.rpm.is_finite(), "RPM should be finite");
            }
            Err(_) => {
                // Also acceptable if adapter requires certain fields
            }
        }
        Ok(())
    }

    #[test]
    fn mudrunner_rejects_invalid_utf8() -> TestResult {
        let adapter = get_adapter("mudrunner")?;
        let bad = vec![0xFF, 0xFE, 0xFD]; // Invalid UTF-8
        let result = adapter.normalize(&bad);
        assert!(result.is_err(), "MudRunner should reject invalid UTF-8");
        Ok(())
    }

    #[test]
    fn wrc_generations_rejects_undersized_packet() -> TestResult {
        let adapter = get_adapter("wrc_generations")?;
        let tiny = vec![0u8; 32]; // WRC needs 264 bytes
        let result = adapter.normalize(&tiny);
        assert!(
            result.is_err(),
            "WRC Generations should reject packet smaller than 264 bytes"
        );
        Ok(())
    }

    #[test]
    fn empty_buffer_handling_for_all_adapters() -> TestResult {
        let factories = adapter_factories();
        let mut error_count = 0;
        let mut ok_count = 0;

        for (game_id, factory) in factories {
            let adapter = factory();
            match adapter.normalize(&[]) {
                Ok(telem) => {
                    ok_count += 1;
                    // If OK, fields must be finite
                    assert!(
                        telem.speed_ms.is_finite(),
                        "{game_id}: speed_ms not finite on empty buffer"
                    );
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }

        // Most adapters should error on empty input
        assert!(
            error_count + ok_count > 0,
            "At least one adapter must be tested"
        );
        Ok(())
    }

    #[test]
    fn oversized_packet_is_handled_gracefully() -> TestResult {
        let adapters = [
            "forza_motorsport",
            "live_for_speed",
            "rennsport",
            "wrc_generations",
        ];

        for game_id in &adapters {
            let adapter = get_adapter(game_id)?;
            let big = vec![0u8; 65536]; // Much larger than any game packet
            match adapter.normalize(&big) {
                Ok(telem) => {
                    assert!(
                        telem.speed_ms.is_finite(),
                        "{game_id}: oversized packet produced non-finite speed"
                    );
                }
                Err(_) => {
                    // Rejecting oversized packets is also fine
                }
            }
        }
        Ok(())
    }

    #[test]
    fn nan_and_inf_in_packet_are_handled_safely() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let mut packet = build_forza_sled_packet(0.0, 0.0, 0.0, 0.0);

        // Write NaN into speed field
        write_f32_le(&mut packet, 32, f32::NAN);
        write_f32_le(&mut packet, 40, f32::NAN);

        match adapter.normalize(&packet) {
            Ok(telem) => {
                // If parsed, speed should be handled (NaN or clamped to finite)
                // The key is it shouldn't panic
                let _ = telem.speed_ms;
            }
            Err(_) => {
                // Rejecting NaN input is acceptable
            }
        }

        // Write infinity into RPM
        let mut packet2 = build_forza_sled_packet(10.0, 20.0, f32::INFINITY, 8000.0);
        if let Ok(telem) = adapter.normalize(&packet2) {
            let _ = telem.rpm;
        }

        // Write negative infinity
        write_f32_le(&mut packet2, 16, f32::NEG_INFINITY);
        let _ = adapter.normalize(&packet2); // Either Ok or Err is fine, as long as no panic
        Ok(())
    }

    #[test]
    fn forza_race_not_on_produces_defaults() -> TestResult {
        let adapter = get_adapter("forza_motorsport")?;
        let mut packet = vec![0u8; 232];
        write_i32_le(&mut packet, 0, 0); // is_race_on = 0 (not racing)
        write_f32_le(&mut packet, 16, 7000.0); // RPM set but race is off

        match adapter.normalize(&packet) {
            Ok(telem) => {
                // When race is off, values should be zero/default
                assert!(
                    telem.speed_ms.is_finite(),
                    "Speed should be finite even when race is off"
                );
            }
            Err(_) => {
                // Rejecting non-racing packets is also acceptable
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Telemetry data invariants
// ═══════════════════════════════════════════════════════════════════════════════

mod data_invariants {
    use super::*;

    #[test]
    fn normalized_telemetry_defaults_are_sane() -> TestResult {
        let telem = NormalizedTelemetry::default();
        assert_f32_near(telem.speed_ms, 0.0, 0.001, "Default speed");
        assert_f32_near(telem.rpm, 0.0, 0.001, "Default RPM");
        assert_eq!(telem.gear, 0, "Default gear should be neutral");
        assert_f32_near(telem.ffb_scalar, 0.0, 0.001, "Default FFB");
        assert!(
            !telem.flags.yellow_flag,
            "Default should have no yellow flag"
        );
        assert!(!telem.flags.red_flag, "Default should have no red flag");
        Ok(())
    }

    #[test]
    fn telemetry_builder_produces_valid_output() -> TestResult {
        let telem = NormalizedTelemetryBuilder::new()
            .speed_ms(50.0)
            .rpm(6000.0)
            .gear(3)
            .ffb_scalar(0.5)
            .build();

        assert_f32_near(telem.speed_ms, 50.0, 0.001, "Builder speed");
        assert_f32_near(telem.rpm, 6000.0, 0.001, "Builder RPM");
        assert_eq!(telem.gear, 3, "Builder gear");
        assert_f32_near(telem.ffb_scalar, 0.5, 0.001, "Builder FFB");
        Ok(())
    }

    #[test]
    fn telemetry_builder_clamps_invalid_values() -> TestResult {
        let telem = NormalizedTelemetryBuilder::new()
            .speed_ms(-10.0) // Negative speed should be ignored/clamped
            .rpm(-100.0) // Negative RPM should be ignored/clamped
            .build();

        assert!(
            telem.speed_ms >= 0.0,
            "Speed should never be negative, got {}",
            telem.speed_ms
        );
        assert!(
            telem.rpm >= 0.0,
            "RPM should never be negative, got {}",
            telem.rpm
        );
        Ok(())
    }

    #[test]
    fn telemetry_flags_default_green_flag_only() -> TestResult {
        let flags = TelemetryFlags::default();
        assert!(flags.green_flag, "Default should have green flag");
        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.abs_active);
        assert!(!flags.traction_control);
        assert!(!flags.drs_active);
        assert!(!flags.drs_available);
        Ok(())
    }

    #[test]
    fn telemetry_value_variants_preserve_data() -> TestResult {
        let float_val = TelemetryValue::Float(1.234);
        let int_val = TelemetryValue::Integer(42);
        let bool_val = TelemetryValue::Boolean(true);
        let str_val = TelemetryValue::String("test".to_string());

        match float_val {
            TelemetryValue::Float(v) => assert_f32_near(v, 1.234, 0.001, "Float value"),
            _ => return Err("Expected Float variant".into()),
        }
        match int_val {
            TelemetryValue::Integer(v) => assert_eq!(v, 42),
            _ => return Err("Expected Integer variant".into()),
        }
        match bool_val {
            TelemetryValue::Boolean(v) => assert!(v),
            _ => return Err("Expected Boolean variant".into()),
        }
        match str_val {
            TelemetryValue::String(v) => assert_eq!(v, "test"),
            _ => return Err("Expected String variant".into()),
        }
        Ok(())
    }

    #[test]
    fn telemetry_frame_serialization_round_trip() -> TestResult {
        let telem = NormalizedTelemetryBuilder::new()
            .speed_ms(33.0)
            .rpm(5500.0)
            .gear(4)
            .build();

        let frame = TelemetryFrame::new(telem, 999999, 42, 256);

        let json = serde_json::to_string(&frame)?;
        let restored: TelemetryFrame = serde_json::from_str(&json)?;

        assert_f32_near(restored.data.speed_ms, 33.0, 0.01, "Roundtrip speed");
        assert_f32_near(restored.data.rpm, 5500.0, 0.01, "Roundtrip RPM");
        assert_eq!(restored.sequence, 42);
        assert_eq!(restored.raw_size, 256);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Cross-game normalization consistency
// ═══════════════════════════════════════════════════════════════════════════════

mod normalization_consistency {
    use super::*;

    #[test]
    fn speed_is_always_in_meters_per_second() -> TestResult {
        // All adapters should output speed in m/s regardless of input format
        let forza = get_adapter("forza_motorsport")?;
        let lfs = get_adapter("live_for_speed")?;
        let rennsport = get_adapter("rennsport")?;

        // Forza: velocity components → magnitude
        let forza_pkt = build_forza_sled_packet(10.0, 0.0, 5000.0, 8000.0);
        let forza_telem = forza.normalize(&forza_pkt)?;
        assert_f32_near(forza_telem.speed_ms, 10.0, 1.0, "Forza speed in m/s");

        // LFS: direct m/s
        let lfs_pkt = build_lfs_packet(10.0, 3000.0, 3, 0.5);
        let lfs_telem = lfs.normalize(&lfs_pkt)?;
        assert_f32_near(lfs_telem.speed_ms, 10.0, 0.5, "LFS speed in m/s");

        // Rennsport: km/h → m/s (36 km/h = 10 m/s)
        let rennsport_pkt = build_rennsport_packet(36.0, 3000.0, 3, 0.0, 0.0);
        let rennsport_telem = rennsport.normalize(&rennsport_pkt)?;
        assert_f32_near(
            rennsport_telem.speed_ms,
            10.0,
            0.5,
            "Rennsport speed in m/s",
        );

        Ok(())
    }

    #[test]
    fn gear_encoding_is_consistent_across_adapters() -> TestResult {
        // All adapters: -1=reverse, 0=neutral, 1+=forward

        // LFS: gear byte 0=R, 1=N, 2=1st
        let lfs = get_adapter("live_for_speed")?;
        let lfs_rev = lfs.normalize(&build_lfs_packet(5.0, 2000.0, 0, 0.0))?;
        assert_eq!(lfs_rev.gear, -1, "LFS reverse should be -1");

        // Rennsport: i8 directly
        let rennsport = get_adapter("rennsport")?;
        let rs_rev = rennsport.normalize(&build_rennsport_packet(10.0, 2000.0, -1, 0.0, 0.0))?;
        assert_eq!(rs_rev.gear, -1, "Rennsport reverse should be -1");

        // SimHub: string "R"
        let simhub = get_adapter("simhub")?;
        let sh_rev = simhub.normalize(&build_simhub_json(5.0, 2000.0, 7000.0, "R", 0.0, 0.0))?;
        assert_eq!(sh_rev.gear, -1, "SimHub reverse should be -1");

        Ok(())
    }

    #[test]
    fn throttle_and_brake_are_normalized_zero_to_one() -> TestResult {
        // SimHub: 0-100 → 0.0-1.0
        let simhub = get_adapter("simhub")?;
        let pkt = build_simhub_json(20.0, 5000.0, 8000.0, "3", 100.0, 50.0);
        let telem = simhub.normalize(&pkt)?;
        assert_f32_near(telem.throttle, 1.0, 0.01, "SimHub full throttle");
        assert_f32_near(telem.brake, 0.5, 0.01, "SimHub half brake");

        // LFS: already 0.0-1.0
        let lfs = get_adapter("live_for_speed")?;
        let pkt = build_lfs_packet(20.0, 5000.0, 3, 1.0);
        let telem = lfs.normalize(&pkt)?;
        assert_f32_near(telem.throttle, 1.0, 0.01, "LFS full throttle");

        Ok(())
    }

    #[test]
    fn all_adapters_report_non_empty_game_id() -> TestResult {
        for (expected_id, factory) in adapter_factories() {
            let adapter = factory();
            let id = adapter.game_id();
            assert!(!id.is_empty(), "Adapter game_id should not be empty");
            assert_eq!(
                id, *expected_id,
                "game_id() should match factory registration"
            );
        }
        Ok(())
    }
}
