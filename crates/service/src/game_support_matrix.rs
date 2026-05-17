//! Game support matrix shim backed by shared telemetry metadata.
//!
//! This module is intentionally lightweight and delegates schema ownership to
//! `openracing-telemetry-config`.

use std::collections::HashMap;

use openracing_telemetry_config::support::load_default_matrix;
pub use openracing_telemetry_config::support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameVersion, TelemetryFieldMapping,
    TelemetrySupport,
};
use tracing::warn;

/// Create the canonical default matrix from shared telemetry metadata.
pub fn create_default_matrix() -> GameSupportMatrix {
    load_default_matrix().unwrap_or_else(|err| {
        warn!(
            error = %err,
            "Failed to load default telemetry support matrix; falling back to empty matrix"
        );

        GameSupportMatrix {
            games: HashMap::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_create_default_matrix_returns_non_empty() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            !matrix.games.is_empty(),
            "Default matrix should contain at least one game"
        );
        Ok(())
    }

    #[test]
    fn test_matrix_contains_iracing() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            matrix.games.contains_key("iracing"),
            "Matrix should contain iRacing"
        );
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert_eq!(iracing.name, "iRacing");
        assert!(!iracing.versions.is_empty(), "iRacing should have versions");
        Ok(())
    }

    #[test]
    fn test_matrix_contains_acc() -> Result<()> {
        let matrix = create_default_matrix();
        let acc = matrix
            .games
            .get("acc")
            .ok_or_else(|| anyhow::anyhow!("acc missing"))?;
        assert_eq!(acc.name, "Assetto Corsa Competizione");
        assert!(
            acc.telemetry.update_rate_hz > 0,
            "ACC should have a positive update rate"
        );
        Ok(())
    }

    #[test]
    fn test_matrix_game_lookup_nonexistent() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            !matrix.games.contains_key("nonexistent_game_xyz"),
            "Nonexistent game should return None"
        );
        Ok(())
    }

    #[test]
    fn test_all_games_have_required_fields() -> Result<()> {
        let matrix = create_default_matrix();
        for (game_id, game) in &matrix.games {
            assert!(
                !game.name.is_empty(),
                "Game '{}' should have a non-empty name",
                game_id
            );
            assert!(
                !game.versions.is_empty(),
                "Game '{}' should have at least one version",
                game_id
            );
            assert!(
                !game.telemetry.method.is_empty(),
                "Game '{}' should have a telemetry method",
                game_id
            );
        }
        Ok(())
    }

    #[test]
    fn test_auto_detect_config_populated() -> Result<()> {
        let matrix = create_default_matrix();
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert!(
            !iracing.auto_detect.process_names.is_empty(),
            "iRacing should have auto-detect process names"
        );
        Ok(())
    }

    #[test]
    fn test_telemetry_field_mapping_present() -> Result<()> {
        let matrix = create_default_matrix();
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert!(
            iracing.telemetry.fields.ffb_scalar.is_some(),
            "iRacing should map the ffb_scalar field"
        );
        assert!(
            iracing.telemetry.fields.rpm.is_some(),
            "iRacing should map the rpm field"
        );
        Ok(())
    }

    /// Verify all popular sim racing games with documented telemetry APIs are present.
    #[test]
    fn test_popular_games_with_documented_telemetry_apis_covered() -> Result<()> {
        let matrix = create_default_matrix();
        let expected = [
            ("iracing", "iRacing"),
            ("acc", "Assetto Corsa Competizione"),
            ("assetto_corsa", "Assetto Corsa"),
            ("ams2", "Automobilista 2"),
            ("project_cars_2", "Project CARS 2"),
            ("gran_turismo_7", "Gran Turismo 7"),
            ("rfactor2", "rFactor 2"),
            ("dirt_rally_2", "DiRT Rally 2.0"),
            ("eawrc", "EA SPORTS WRC"),
            ("f1_25", "EA F1 25 (Native UDP)"),
            ("beamng_drive", "BeamNG.drive"),
            ("rbr", "Richard Burns Rally"),
        ];
        for (game_id, display_name) in &expected {
            let game = matrix
                .games
                .get(*game_id)
                .ok_or_else(|| anyhow::anyhow!("missing game: {} ({})", game_id, display_name))?;
            assert_eq!(
                &game.name, display_name,
                "game id '{}' should have name '{}'",
                game_id, display_name
            );
        }
        Ok(())
    }

    /// Each game's telemetry output_target should be a valid socket address when present.
    #[test]
    fn test_telemetry_output_targets_are_valid_addresses() -> Result<()> {
        let matrix = create_default_matrix();
        for (game_id, game) in &matrix.games {
            if let Some(ref target) = game.telemetry.output_target {
                assert!(
                    target.parse::<std::net::SocketAddr>().is_ok(),
                    "Game '{}' has invalid output_target: '{}'",
                    game_id,
                    target
                );
            }
        }
        Ok(())
    }

    /// Telemetry methods should use recognized protocol identifiers.
    #[test]
    fn test_telemetry_methods_are_recognized() -> Result<()> {
        let known_methods = [
            "shared_memory",
            "shared_memory_isi_rf1",
            "shared_memory_r3e",
            "udp_broadcast",
            "udp_outgauge",
            "udp_outsim",
            "udp_sms_pcars2",
            "udp_salsa20_encrypted",
            "udp_codemasters_mode1",
            "udp_codemasters_mode2",
            "udp_custom_codemasters",
            "udp_native_f1_25",
            "udp_native_f1_native",
            "udp_schema",
            "udp_forza_data_out",
            "udp_livedata",
            "udp_flatbuffers_kkfb",
            "udp_rennsport",
            "udp_wrc_kylotonn",
            "udp_wreckfest",
            "probe_discovery",
            "simhub_bridge",
            "simhub_json_udp",
            "simhub_udp_json",
            "scs_sdk",
            "scs_shared_memory",
            "codemasters_udp",
            "dakr_udp",
            "fotc_udp",
            "papyrus_udp",
            "rf2_udp",
            "rfactor1_udp",
            "trackmania_json_udp",
            "none",
        ];
        let matrix = create_default_matrix();
        for (game_id, game) in &matrix.games {
            assert!(
                known_methods.contains(&&*game.telemetry.method),
                "Game '{}' uses unrecognized telemetry method: '{}'",
                game_id,
                game.telemetry.method
            );
        }
        Ok(())
    }
}
