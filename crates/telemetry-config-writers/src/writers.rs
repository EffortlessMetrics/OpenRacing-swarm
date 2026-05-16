use anyhow::{Result, anyhow};
use serde_json::{Map, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use tracing::info;

use crate::path::resolve_game_path;
use crate::types::{ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig};

const EAWRC_STRUCTURE_ID: &str = "openracing";
const EAWRC_PACKET_ID: &str = "session_update";
const EAWRC_DEFAULT_PORT: u16 = 20778;
const AC_RALLY_DEFAULT_DISCOVERY_PORT: u16 = 9000;
const AC_RALLY_PROBE_RELATIVE_PATH: &str =
    "Documents/Assetto Corsa Rally/Config/openracing_probe.json";
const IRACING_360HZ_KEY: &str = "irsdkLog360Hz";
const DIRT5_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dirt5_bridge_contract.json";
const DIRT5_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT5_DEFAULT_PORT: u16 = 20777;
const DIRT5_DEFAULT_MODE: u8 = 1;
const DIRT_RALLY_2_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/dirt_rally_2_bridge_contract.json";
const DIRT_RALLY_2_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT_RALLY_2_DEFAULT_PORT: u16 = 20777;
const DIRT_RALLY_2_DEFAULT_MODE: u8 = 1;
const RBR_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/rbr_bridge_contract.json";
const RBR_BRIDGE_PROTOCOL: &str = "rbr_livedata_udp";
const RBR_DEFAULT_PORT: u16 = 6776;
const F1_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/f1_bridge_contract.json";
const F1_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const F1_DEFAULT_PORT: u16 = 20777;
const F1_DEFAULT_MODE: u8 = 3;
const F1_25_CONTRACT_RELATIVE_PATH: &str = "Documents/OpenRacing/f1_25_contract.json";
const F1_25_NATIVE_PROTOCOL: &str = "f1_25_native_udp";
const F1_25_DEFAULT_PORT: u16 = 20777;
const F1_NATIVE_CONTRACT_RELATIVE_PATH: &str = "Documents/OpenRacing/f1_native_contract.json";
const F1_NATIVE_PROTOCOL: &str = "udp_native_f1_native";
const F1_NATIVE_DEFAULT_PORT: u16 = 20777;
const F1_MANAGER_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/f1_manager_bridge_contract.json";
const WRC_GENERATIONS_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/wrc_generations_bridge_contract.json";
const WRC_GENERATIONS_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const WRC_GENERATIONS_DEFAULT_PORT: u16 = 6777;
const WRC_GENERATIONS_DEFAULT_MODE: u8 = 1;

const WRC_KYLOTONN_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/wrc_kylotonn_bridge_contract.json";
const WRC_KYLOTONN_BRIDGE_PROTOCOL: &str = "wrc_kylotonn_udp";
const WRC_KYLOTONN_DEFAULT_PORT: u16 = 64000;
const DIRT4_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dirt4_bridge_contract.json";
const DIRT4_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT4_DEFAULT_PORT: u16 = 20777;
const DIRT4_DEFAULT_MODE: u8 = 1;

const ETS2_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ets2_bridge_contract.json";
const ETS2_BRIDGE_PROTOCOL: &str = "scs_shared_memory";
const ETS2_DEFAULT_PORT: u16 = 0;

const ATS_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ats_bridge_contract.json";
const ATS_BRIDGE_PROTOCOL: &str = "scs_shared_memory";
const ATS_DEFAULT_PORT: u16 = 0;

const WRECKFEST_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/wreckfest_bridge_contract.json";
const WRECKFEST_BRIDGE_PROTOCOL: &str = "udp_wreckfest";
const WRECKFEST_DEFAULT_PORT: u16 = 5606;

const FLATOUT_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/flatout_bridge_contract.json";
const FLATOUT_BRIDGE_PROTOCOL: &str = "fotc_udp";
const FLATOUT_DEFAULT_PORT: u16 = 7776;

const DAKAR_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dakar_bridge_contract.json";
const DAKAR_BRIDGE_PROTOCOL: &str = "dakr_udp";
const DAKAR_DEFAULT_PORT: u16 = 7779;

const RENNSPORT_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/rennsport_bridge_contract.json";
const RENNSPORT_BRIDGE_PROTOCOL: &str = "udp_rennsport";
const RENNSPORT_DEFAULT_PORT: u16 = 9000;

const GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/grid_autosport_bridge_contract.json";
const GRID_AUTOSPORT_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_AUTOSPORT_DEFAULT_PORT: u16 = 20777;
const GRID_AUTOSPORT_DEFAULT_MODE: u8 = 1;

const GRID_2019_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/grid_2019_bridge_contract.json";
const GRID_2019_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_2019_DEFAULT_PORT: u16 = 20777;
const GRID_2019_DEFAULT_MODE: u8 = 1;

const GRID_LEGENDS_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/grid_legends_bridge_contract.json";
const GRID_LEGENDS_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_LEGENDS_DEFAULT_PORT: u16 = 20777;
const GRID_LEGENDS_DEFAULT_MODE: u8 = 1;

const DIRT3_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dirt3_bridge_contract.json";
const DIRT3_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT3_DEFAULT_PORT: u16 = 20777;
const DIRT3_DEFAULT_MODE: u8 = 1;

const RACE_DRIVER_GRID_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/race_driver_grid_bridge_contract.json";
const RACE_DRIVER_GRID_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const RACE_DRIVER_GRID_DEFAULT_PORT: u16 = 20777;
const RACE_DRIVER_GRID_DEFAULT_MODE: u8 = 1;

const AUTOMOBILISTA_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/automobilista_bridge_contract.json";
const AUTOMOBILISTA_BRIDGE_PROTOCOL: &str = "isi_rf1_shared_memory";

const KARTKRAFT_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/kartkraft_bridge_contract.json";
const KARTKRAFT_BRIDGE_PROTOCOL: &str = "udp_flatbuffers_kartkraft";
const KARTKRAFT_DEFAULT_PORT: u16 = 5000;

const RACEROOM_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/raceroom_bridge_contract.json";
const RACEROOM_BRIDGE_PROTOCOL: &str = "r3e_shared_memory";

const NASCAR_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/nascar_bridge_contract.json";
const NASCAR_BRIDGE_PROTOCOL: &str = "papyrus_udp";
const NASCAR_DEFAULT_PORT: u16 = 5606;

const NASCAR_21_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/nascar_21_bridge_contract.json";
const NASCAR_21_BRIDGE_PROTOCOL: &str = "papyrus_udp";
const NASCAR_21_DEFAULT_PORT: u16 = 5606;

const LMU_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/lmu_bridge_contract.json";
const LMU_BRIDGE_PROTOCOL: &str = "rf2_udp";
const LMU_DEFAULT_PORT: u16 = 6789;

const WTCR_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/wtcr_bridge_contract.json";
const WTCR_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const WTCR_DEFAULT_PORT: u16 = 6778;
const WTCR_DEFAULT_MODE: u8 = 1;

const TRACKMANIA_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/trackmania_bridge_contract.json";
const TRACKMANIA_BRIDGE_PROTOCOL: &str = "trackmania_json_udp";
const TRACKMANIA_DEFAULT_PORT: u16 = 5004;

const SIMHUB_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/simhub_bridge_contract.json";
const SIMHUB_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const SIMHUB_DEFAULT_PORT: u16 = 5555;

const MUDRUNNER_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/mudrunner_bridge_contract.json";
const MUDRUNNER_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const MUDRUNNER_DEFAULT_PORT: u16 = 8877;

const SNOWRUNNER_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/snowrunner_bridge_contract.json";
const SNOWRUNNER_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const SNOWRUNNER_DEFAULT_PORT: u16 = 8877;

const MOTOGP_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/motogp_bridge_contract.json";
const MOTOGP_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const MOTOGP_DEFAULT_PORT: u16 = 5556;

const RIDE5_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ride5_bridge_contract.json";
const RIDE5_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const RIDE5_DEFAULT_PORT: u16 = 5558;

const V_RALLY_4_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/v_rally_4_bridge_contract.json";
const V_RALLY_4_BRIDGE_PROTOCOL: &str = "kylotonn_udp";
const V_RALLY_4_DEFAULT_PORT: u16 = 64000;

const GRAVEL_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/gravel_bridge_contract.json";
const GRAVEL_BRIDGE_PROTOCOL: &str = "simhub_udp_json";
const GRAVEL_DEFAULT_PORT: u16 = 5555;

const SEB_LOEB_RALLY_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/seb_loeb_rally_bridge_contract.json";
const SEB_LOEB_RALLY_BRIDGE_PROTOCOL: &str = "stub";

const ACC2_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/acc2_bridge_contract.json";
const ACC2_BRIDGE_PROTOCOL: &str = "stub";

const AC_EVO_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ac_evo_bridge_contract.json";
const AC_EVO_BRIDGE_PROTOCOL: &str = "stub";

const DIRT_SHOWDOWN_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/dirt_showdown_bridge_contract.json";
const DIRT_SHOWDOWN_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT_SHOWDOWN_DEFAULT_PORT: u16 = 20777;
const DIRT_SHOWDOWN_DEFAULT_MODE: u8 = 1;

/// iRacing configuration writer
pub struct IRacingConfigWriter;

impl Default for IRacingConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for IRacingConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing iRacing telemetry configuration");

        let app_ini_path = resolve_game_path(game_path, "Documents/iRacing/app.ini");
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        // Read existing app.ini if it exists.
        let existing_content = if app_ini_path.exists() {
            fs::read_to_string(&app_ini_path)?
        } else {
            String::new()
        };

        let (mut new_content, prior_value, operation) = upsert_ini_value(
            &existing_content,
            "Telemetry",
            "telemetryDiskFile",
            telemetry_enabled,
        );

        let mut diffs = vec![ConfigDiff {
            file_path: app_ini_path.to_string_lossy().to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: prior_value,
            new_value: telemetry_enabled.to_string(),
            operation,
        }];

        if config.enable_high_rate_iracing_360hz {
            let (updated_content, prior_360hz_value, operation_360hz) =
                upsert_ini_value(&new_content, "Telemetry", IRACING_360HZ_KEY, "1");
            new_content = updated_content;
            diffs.push(ConfigDiff {
                file_path: app_ini_path.to_string_lossy().to_string(),
                section: Some("Telemetry".to_string()),
                key: IRACING_360HZ_KEY.to_string(),
                old_value: prior_360hz_value,
                new_value: "1".to_string(),
                operation: operation_360hz,
            });
        }

        if let Some(parent) = app_ini_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&app_ini_path, &new_content)?;

        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let app_ini_path = resolve_game_path(game_path, "Documents/iRacing/app.ini");

        if !app_ini_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(app_ini_path)?;

        // Check if telemetry is enabled.
        let has_telemetry_section = content.contains("[Telemetry]");
        let has_telemetry_enabled = content
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("telemetryDiskFile=1"));

        Ok(has_telemetry_section && has_telemetry_enabled)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        let mut diffs = vec![ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        }];

        if config.enable_high_rate_iracing_360hz {
            diffs.push(ConfigDiff {
                file_path: "Documents/iRacing/app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: IRACING_360HZ_KEY.to_string(),
                old_value: None,
                new_value: "1".to_string(),
                operation: DiffOperation::Add,
            });
        }

        Ok(diffs)
    }
}

/// ACC (Assetto Corsa Competizione) configuration writer
pub struct ACCConfigWriter;

impl Default for ACCConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACCConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ACC telemetry configuration");

        let broadcasting_json_path = resolve_game_path(
            game_path,
            "Documents/Assetto Corsa Competizione/Config/broadcasting.json",
        );

        let existed_before = broadcasting_json_path.exists();
        let existing_content = if broadcasting_json_path.exists() {
            Some(fs::read_to_string(&broadcasting_json_path)?)
        } else {
            None
        };

        let existing_map = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let connection_id = existing_map
            .get("connectionId")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let connection_password = existing_map
            .get("connectionPassword")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let command_password = existing_map
            .get("commandPassword")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut broadcasting_config = existing_map;
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        // Keep compatibility with environments/tools expecting the corrected key.
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("broadcastingPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("connectionId".to_string(), connection_id);
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(connection_password),
        );
        broadcasting_config.insert(
            "commandPassword".to_string(),
            Value::String(command_password),
        );
        broadcasting_config.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(broadcasting_config))?;

        if let Some(parent) = broadcasting_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&broadcasting_json_path, &new_content)?;

        let diffs = vec![ConfigDiff {
            file_path: broadcasting_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }];

        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let broadcasting_json_path = resolve_game_path(
            game_path,
            "Documents/Assetto Corsa Competizione/Config/broadcasting.json",
        );

        if !broadcasting_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(broadcasting_json_path)?;
        let config_value: Value = serde_json::from_str(&content)?;
        let object = match config_value.as_object() {
            Some(obj) => obj,
            None => return Ok(false),
        };

        // Accept both the original ACC key and the corrected compatibility key.
        let has_listener_port = object
            .get("updListenerPort")
            .or_else(|| object.get("udpListenerPort"))
            .and_then(Value::as_u64)
            .is_some();
        let has_broadcasting_port = object
            .get("broadcastingPort")
            .and_then(Value::as_u64)
            .is_some();
        let has_connection_id = object
            .get("connectionId")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        let has_connection_password = object
            .get("connectionPassword")
            .and_then(Value::as_str)
            .is_some();
        let has_command_password = object
            .get("commandPassword")
            .and_then(Value::as_str)
            .is_some();
        let has_update_rate = object.get("updateRateHz").and_then(Value::as_u64).is_some();

        Ok(has_listener_port
            && has_broadcasting_port
            && has_connection_id
            && has_connection_password
            && has_command_password
            && has_update_rate)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let mut broadcasting_config = Map::new();
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("broadcastingPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("connectionId".to_string(), Value::String(String::new()));
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(String::new()),
        );
        broadcasting_config.insert("commandPassword".to_string(), Value::String(String::new()));
        broadcasting_config.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(broadcasting_config))?;

        Ok(vec![ConfigDiff {
            file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: new_content,
            operation: DiffOperation::Add,
        }])
    }
}

/// Assetto Corsa Rally configuration writer.
///
/// AC Rally telemetry transport is currently discovery-based in OpenRacing.
/// This writer creates a sidecar probe profile consumed by OpenRacing tooling.
pub struct ACRallyConfigWriter;

impl Default for ACRallyConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACRallyConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Assetto Corsa Rally telemetry probe configuration");

        let probe_json_path = resolve_game_path(game_path, AC_RALLY_PROBE_RELATIVE_PATH);
        let existed_before = probe_json_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&probe_json_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let listener_port =
            parse_target_port(&config.output_target).unwrap_or(AC_RALLY_DEFAULT_DISCOVERY_PORT);
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("mode".to_string(), Value::String("discovery".to_string()));
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );
        root.insert(
            "outputTarget".to_string(),
            Value::String(config.output_target.clone()),
        );
        root.insert(
            "probeOrder".to_string(),
            Value::Array(vec![
                Value::String("udp_handshake".to_string()),
                Value::String("udp_passive".to_string()),
                Value::String("shared_memory".to_string()),
            ]),
        );
        root.insert(
            "udpCandidates".to_string(),
            Value::Array(vec![Value::from(listener_port)]),
        );
        root.entry("sharedMemoryCandidates".to_string())
            .or_insert(Value::Array(Vec::new()));
        root.insert(
            "note".to_string(),
            Value::String(
                "OpenRacing discovery profile. Populate sharedMemoryCandidates when map names are known."
                    .to_string(),
            ),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = probe_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&probe_json_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: probe_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let probe_json_path = resolve_game_path(game_path, AC_RALLY_PROBE_RELATIVE_PATH);
        if !probe_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(probe_json_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let mode_discovery = value
            .get("mode")
            .and_then(Value::as_str)
            .map(|mode| mode == "discovery")
            .unwrap_or(false);
        let has_probe_order = value
            .get("probeOrder")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        let has_udp_candidates = value
            .get("udpCandidates")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false);

        Ok(mode_discovery && has_probe_order && has_udp_candidates)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port =
            parse_target_port(&config.output_target).unwrap_or(AC_RALLY_DEFAULT_DISCOVERY_PORT);
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "enabled": config.enabled,
            "mode": "discovery",
            "updateRateHz": config.update_rate_hz,
            "outputTarget": config.output_target,
            "probeOrder": ["udp_handshake", "udp_passive", "shared_memory"],
            "udpCandidates": [listener_port],
            "sharedMemoryCandidates": [],
            "note": "OpenRacing discovery profile. Populate sharedMemoryCandidates when map names are known."
        }))?;

        Ok(vec![ConfigDiff {
            file_path: AC_RALLY_PROBE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: content,
            operation: DiffOperation::Add,
        }])
    }
}

/// AMS2 (Automobilista 2) configuration writer.
///
/// AMS2 shared-memory telemetry requires an in-game toggle. This writer
/// stores explicit telemetry intent in the player config while preserving
/// existing content.
pub struct AMS2ConfigWriter;

impl Default for AMS2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for AMS2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing AMS2 telemetry configuration");

        let player_json_path = resolve_game_path(
            game_path,
            "Documents/Automobilista 2/UserData/player/player.json",
        );
        let existed_before = player_json_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&player_json_path)?)
        } else {
            None
        };

        let mut json_map = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        json_map.insert(
            "sharedMemoryEnabled".to_string(),
            Value::from(config.enabled),
        );
        json_map.insert(
            "openRacingTelemetry".to_string(),
            Value::Object(Map::from_iter([
                ("enabled".to_string(), Value::from(config.enabled)),
                (
                    "sharedMemoryMap".to_string(),
                    Value::String("$pcars2$".to_string()),
                ),
                (
                    "updateRateHz".to_string(),
                    Value::from(config.update_rate_hz),
                ),
                (
                    "note".to_string(),
                    Value::String(
                        "Enable Project CARS 2 shared memory in AMS2 options.".to_string(),
                    ),
                ),
            ])),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(json_map))?;

        if let Some(parent) = player_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&player_json_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: player_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let player_json_path = resolve_game_path(
            game_path,
            "Documents/Automobilista 2/UserData/player/player.json",
        );
        if !player_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(player_json_path)?;
        let config: Value = serde_json::from_str(&content)?;

        let top_level_enabled = config
            .get("sharedMemoryEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let openracing_enabled = config
            .get("openRacingTelemetry")
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(top_level_enabled && openracing_enabled)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut root = Map::new();
        root.insert(
            "sharedMemoryEnabled".to_string(),
            Value::from(config.enabled),
        );
        root.insert(
            "openRacingTelemetry".to_string(),
            Value::Object(Map::from_iter([
                ("enabled".to_string(), Value::from(config.enabled)),
                (
                    "sharedMemoryMap".to_string(),
                    Value::String("$pcars2$".to_string()),
                ),
                (
                    "updateRateHz".to_string(),
                    Value::from(config.update_rate_hz),
                ),
                (
                    "note".to_string(),
                    Value::String(
                        "Enable Project CARS 2 shared memory in AMS2 options.".to_string(),
                    ),
                ),
            ])),
        );

        Ok(vec![ConfigDiff {
            file_path: "Documents/Automobilista 2/UserData/player/player.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&Value::Object(root))?,
            operation: DiffOperation::Add,
        }])
    }
}

/// rFactor 2 configuration writer.
///
/// rFactor 2 telemetry requires the shared-memory plugin. This writer
/// generates an explicit plugin telemetry configuration contract.
pub struct RFactor2ConfigWriter;

impl Default for RFactor2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RFactor2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing rFactor 2 telemetry configuration");

        let config_path = game_path.join("UserData/player/OpenRacing.Telemetry.json");
        let existed_before = config_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&config_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("requiresSharedMemoryPlugin".to_string(), Value::from(true));
        root.insert(
            "telemetryMap".to_string(),
            Value::String("$rFactor2SMMP_Telemetry$".to_string()),
        );
        root.insert(
            "scoringMap".to_string(),
            Value::String("$rFactor2SMMP_Scoring$".to_string()),
        );
        root.insert(
            "forceFeedbackMap".to_string(),
            Value::String("$rFactor2SMMP_ForceFeedback$".to_string()),
        );
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: config_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let config_path = game_path.join("UserData/player/OpenRacing.Telemetry.json");
        if !config_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(config_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let plugin_required = value
            .get("requiresSharedMemoryPlugin")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let has_telemetry_map = value.get("telemetryMap").and_then(Value::as_str).is_some();
        let has_force_map = value
            .get("forceFeedbackMap")
            .and_then(Value::as_str)
            .is_some();

        Ok(plugin_required && has_telemetry_map && has_force_map)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut root = Map::new();
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("requiresSharedMemoryPlugin".to_string(), Value::from(true));
        root.insert(
            "telemetryMap".to_string(),
            Value::String("$rFactor2SMMP_Telemetry$".to_string()),
        );
        root.insert(
            "scoringMap".to_string(),
            Value::String("$rFactor2SMMP_Scoring$".to_string()),
        );
        root.insert(
            "forceFeedbackMap".to_string(),
            Value::String("$rFactor2SMMP_ForceFeedback$".to_string()),
        );
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        Ok(vec![ConfigDiff {
            file_path: "UserData/player/OpenRacing.Telemetry.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&Value::Object(root))?,
            operation: DiffOperation::Add,
        }])
    }
}

/// EA SPORTS WRC configuration writer.
///
/// EA WRC telemetry is configured through a generated telemetry folder under
/// `Documents/My Games/WRC/telemetry`.
pub struct EAWRCConfigWriter;

impl Default for EAWRCConfigWriter {
    fn default() -> Self {
        Self
    }
}

/// Dirt 5 configuration writer.
///
/// Dirt 5 has no native in-game telemetry export settings to toggle. This writer
/// creates a sidecar contract file consumed by OpenRacing and external bridge tools.
pub struct Dirt5ConfigWriter;

impl Default for Dirt5ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Dirt5ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Dirt 5 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, DIRT5_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt5",
            "telemetry_protocol": DIRT5_BRIDGE_PROTOCOL,
            "mode": DIRT5_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 5 telemetry is bridge-backed; no native game config is modified.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DIRT5_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|value| value == DIRT5_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|value| value == "dirt5")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt5",
            "telemetry_protocol": DIRT5_BRIDGE_PROTOCOL,
            "mode": DIRT5_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 5 telemetry is bridge-backed; no native game config is modified.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT5_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// DiRT Rally 2.0 configuration writer.
///
/// DiRT Rally 2.0 uses the same Codemasters UDP Mode 1 format as DiRT 5.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct DirtRally2ConfigWriter;

impl Default for DirtRally2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for DirtRally2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing DiRT Rally 2.0 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, DIRT_RALLY_2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_RALLY_2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_rally_2",
            "telemetry_protocol": DIRT_RALLY_2_BRIDGE_PROTOCOL,
            "mode": DIRT_RALLY_2_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Rally 2.0 telemetry uses Codemasters UDP Mode 1. Enable UDP telemetry in the game's hardware settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DIRT_RALLY_2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == DIRT_RALLY_2_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt_rally_2")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_RALLY_2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_rally_2",
            "telemetry_protocol": DIRT_RALLY_2_BRIDGE_PROTOCOL,
            "mode": DIRT_RALLY_2_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Rally 2.0 telemetry uses Codemasters UDP Mode 1. Enable UDP telemetry in the game's hardware settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT_RALLY_2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Richard Burns Rally configuration writer.
///
/// RBR does not have native UDP telemetry output; it requires the RSF Rallysimfans plugin.
/// This writer creates a bridge contract file documenting the expected UDP connection.
pub struct RBRConfigWriter;

impl Default for RBRConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RBRConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing RBR bridge contract configuration");

        let contract_path = resolve_game_path(game_path, RBR_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(RBR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rbr",
            "telemetry_protocol": RBR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RBR requires the RSF Rallysimfans plugin for UDP telemetry. Configure the plugin to send LiveData to the OpenRacing port.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, RBR_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == RBR_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "rbr")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RBR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rbr",
            "telemetry_protocol": RBR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RBR requires the RSF Rallysimfans plugin for UDP telemetry. Configure the plugin to send LiveData to the OpenRacing port.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: RBR_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Gran Turismo 7 configuration writer.
///
/// GT7 is a PlayStation-exclusive title; there is no PC executable or config file to write.
/// This writer creates a bridge contract that documents the Salsa20-encrypted UDP connection.
pub struct GranTurismo7ConfigWriter;

impl Default for GranTurismo7ConfigWriter {
    fn default() -> Self {
        Self
    }
}

const GT7_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/gran_turismo_7_bridge_contract.json";
const GT7_BRIDGE_PROTOCOL: &str = "gt7_salsa20_udp";
const GT7_DEFAULT_PORT: u16 = 33740;

const GTS_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/gran_turismo_sport_bridge_contract.json";
const GTS_BRIDGE_PROTOCOL: &str = "gt7_salsa20_udp";
const GTS_DEFAULT_PORT: u16 = 33340;

impl ConfigWriter for GranTurismo7ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Gran Turismo 7 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, GT7_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(GT7_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_7",
            "telemetry_protocol": GT7_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT7 sends Salsa20-encrypted UDP packets from the PS4/PS5 to this port. Enable telemetry in GT7 Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GT7_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GT7_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "gran_turismo_7")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GT7_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_7",
            "telemetry_protocol": GT7_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT7 sends Salsa20-encrypted UDP packets from the PS4/PS5 to this port. Enable telemetry in GT7 Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: GT7_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Gran Turismo Sport configuration writer.
///
/// GT Sport is a PlayStation-exclusive title using the same Salsa20 UDP format
/// as GT7 but with receive port 33340.
pub struct GranTurismo7SportsConfigWriter;

impl Default for GranTurismo7SportsConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for GranTurismo7SportsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Gran Turismo Sport bridge contract configuration");

        let contract_path = resolve_game_path(game_path, GTS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(GTS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_sport",
            "telemetry_protocol": GTS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT Sport sends Salsa20-encrypted UDP packets from the PS4 to this port. Enable telemetry in GT Sport Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GTS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GTS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "gran_turismo_sport")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GTS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_sport",
            "telemetry_protocol": GTS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT Sport sends Salsa20-encrypted UDP packets from the PS4 to this port. Enable telemetry in GT Sport Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: GTS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// F1 configuration writer.
///
/// F1 telemetry support is currently bridge-backed. This writer creates a
/// sidecar contract consumed by OpenRacing and optional bridge tools.
pub struct F1ConfigWriter;

impl Default for F1ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, F1_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1",
            "telemetry_protocol": F1_BRIDGE_PROTOCOL,
            "mode": F1_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "F1 telemetry is bridge-backed; no native game config is modified.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, F1_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|value| value == F1_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|value| value == "f1")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1",
            "telemetry_protocol": F1_BRIDGE_PROTOCOL,
            "mode": F1_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "F1 telemetry is bridge-backed; no native game config is modified.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: F1_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// F1 25 native UDP configuration writer.
///
/// EA F1 25 telemetry is transmitted via the game's built-in UDP broadcast.
/// No in-game file needs to be modified; this writer creates a sidecar contract
/// that documents the required game settings (port 20777, packet format 2025)
/// for tooling and diagnostics.
pub struct F1_25ConfigWriter;

impl Default for F1_25ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1_25ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 25 native UDP contract configuration");

        let contract_path = resolve_game_path(game_path, F1_25_CONTRACT_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_25_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_25",
            "telemetry_protocol": F1_25_NATIVE_PROTOCOL,
            "packet_format": 2025,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 25 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz",
                "  UDP Format: 2025"
            ],
            "supported_packets": ["session (1)", "car_telemetry (6)", "car_status (7)"],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, F1_25_CONTRACT_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == F1_25_NATIVE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "f1_25")
            .unwrap_or(false);
        let valid_format = value
            .get("packet_format")
            .and_then(Value::as_u64)
            .map(|v| v == 2025)
            .unwrap_or(false);

        Ok(valid_protocol && valid_game && valid_format)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_25_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_25",
            "telemetry_protocol": F1_25_NATIVE_PROTOCOL,
            "packet_format": 2025,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 25 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz",
                "  UDP Format: 2025"
            ],
            "supported_packets": ["session (1)", "car_telemetry (6)", "car_status (7)"],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: F1_25_CONTRACT_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// EA F1 2023/2024 native UDP configuration writer.
///
/// Writes a contract file to Documents/OpenRacing/f1_native_contract.json
/// documenting the expected native UDP configuration.
pub struct F1NativeConfigWriter;

impl Default for F1NativeConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1NativeConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 native UDP contract configuration");

        let contract_path = resolve_game_path(game_path, F1_NATIVE_CONTRACT_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_NATIVE_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_native",
            "telemetry_protocol": F1_NATIVE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 23/24 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, F1_NATIVE_CONTRACT_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == F1_NATIVE_PROTOCOL);
        let valid_game = value
            .get("game_id")
            .and_then(|v| v.as_str())
            .is_some_and(|g| g == "f1_native");
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_NATIVE_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_native",
            "telemetry_protocol": F1_NATIVE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 23/24 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;
        Ok(vec![ConfigDiff {
            file_path: F1_NATIVE_CONTRACT_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// F1 Manager series configuration writer (stub).
///
/// F1 Manager is a management game without driving simulation or UDP telemetry.
/// This writer creates a minimal bridge contract so users see the game as registered.
pub struct F1ManagerConfigWriter;

impl Default for F1ManagerConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1ManagerConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 Manager bridge contract (stub — no telemetry applicable)");
        let contract_path = resolve_game_path(game_path, F1_MANAGER_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "f1_manager",
            "telemetry_protocol": "none",
            "enabled": config.enabled,
            "bridge_notes": "F1 Manager is a strategy/management game. No UDP telemetry or force-feedback applies.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, F1_MANAGER_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "f1_manager")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "f1_manager",
            "telemetry_protocol": "none",
            "enabled": config.enabled,
            "bridge_notes": "F1 Manager is a strategy/management game. No UDP telemetry or force-feedback applies.",
        });
        Ok(vec![ConfigDiff {
            file_path: F1_MANAGER_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Assetto Corsa (original) configuration writer.
///
/// AC uses the OutGauge UDP protocol (port 9996). Since the OutGauge output target
/// is configured inside the game, this writer creates a bridge contract that documents
/// the expected UDP listener configuration.
pub struct AssettoCorsaConfigWriter;

impl Default for AssettoCorsaConfigWriter {
    fn default() -> Self {
        Self
    }
}

const AC_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/assetto_corsa_bridge_contract.json";
const AC_BRIDGE_PROTOCOL: &str = "ac_outgauge_udp";
const AC_DEFAULT_PORT: u16 = 9996;

impl ConfigWriter for AssettoCorsaConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Assetto Corsa bridge contract configuration");

        let contract_path = resolve_game_path(game_path, AC_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(AC_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "assetto_corsa",
            "telemetry_protocol": AC_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Assetto Corsa, enable OutGauge in the Documents/Assetto Corsa/cfg/openracing.ini file:",
                "  [OutGauge]",
                "  Mode=2",
                "  IP=127.0.0.1",
                "  Port=9996",
                "  Delay=0",
                "  ID=1"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, AC_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == AC_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "assetto_corsa")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(AC_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "assetto_corsa",
            "telemetry_protocol": AC_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Assetto Corsa, enable OutGauge in the Documents/Assetto Corsa/cfg/openracing.ini file:",
                "  [OutGauge]",
                "  Mode=2",
                "  IP=127.0.0.1",
                "  Port=9996",
                "  Delay=0",
                "  ID=1"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: AC_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Forza Motorsport / Forza Horizon configuration writer.
///
/// Forza's "Data Out" feature is configured in-game only. This writer creates a bridge
/// contract documenting the expected UDP listener on port 5300.
pub struct ForzaMotorsportConfigWriter;

impl Default for ForzaMotorsportConfigWriter {
    fn default() -> Self {
        Self
    }
}

const FORZA_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/forza_motorsport_bridge_contract.json";
const FORZA_BRIDGE_PROTOCOL: &str = "forza_data_out_udp";
const FORZA_DEFAULT_PORT: u16 = 5300;

const FH4_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/forza_horizon_4_bridge_contract.json";
const FH4_DEFAULT_PORT: u16 = 12350;

const FH5_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/forza_horizon_5_bridge_contract.json";
const FH5_DEFAULT_PORT: u16 = 5300;

impl ConfigWriter for ForzaMotorsportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Forza Motorsport bridge contract configuration");

        let contract_path = resolve_game_path(game_path, FORZA_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(FORZA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_motorsport",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Motorsport / Forza Horizon, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, FORZA_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == FORZA_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "forza_motorsport")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FORZA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_motorsport",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Motorsport / Forza Horizon, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: FORZA_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Forza Horizon 4 configuration writer.
///
/// Forza Horizon 4 uses the same "Data Out" UDP protocol as Forza Motorsport, but
/// listens on port 12350 by default. This writer creates a bridge contract documenting
/// the expected UDP listener.
pub struct ForzaHorizon4ConfigWriter;

impl Default for ForzaHorizon4ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ForzaHorizon4ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Forza Horizon 4 bridge contract configuration");

        let contract_path = game_path.join(FH4_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(FH4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_horizon_4",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Horizon 4, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 12350"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(FH4_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == FORZA_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "forza_horizon_4")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FH4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_horizon_4",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Horizon 4, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 12350"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: FH4_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Forza Horizon 5 configuration writer.
///
/// Forza Horizon 5 uses the same "Data Out" UDP protocol as Forza Motorsport, sharing
/// port 5300. This writer creates a bridge contract documenting the expected UDP listener.
pub struct ForzaHorizon5ConfigWriter;

impl Default for ForzaHorizon5ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ForzaHorizon5ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Forza Horizon 5 bridge contract configuration");

        let contract_path = game_path.join(FH5_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(FH5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_horizon_5",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Horizon 5, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(FH5_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == FORZA_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "forza_horizon_5")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FH5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_horizon_5",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Horizon 5, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: FH5_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// BeamNG.drive configuration writer.
///
/// BeamNG.drive exposes telemetry via the OutGauge protocol (port 4444), enabled through
/// its in-game apps system. This writer creates a bridge contract documenting the listener.
pub struct BeamNGDriveConfigWriter;

impl Default for BeamNGDriveConfigWriter {
    fn default() -> Self {
        Self
    }
}

const BEAMNG_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/beamng_drive_bridge_contract.json";
const BEAMNG_BRIDGE_PROTOCOL: &str = "beamng_outgauge_udp";
const BEAMNG_DEFAULT_PORT: u16 = 4444;

impl ConfigWriter for BeamNGDriveConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing BeamNG.drive bridge contract configuration");

        let contract_path = resolve_game_path(game_path, BEAMNG_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(BEAMNG_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "beamng_drive",
            "telemetry_protocol": BEAMNG_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In BeamNG.drive, enable the OutGauge app from the apps menu.",
                "Set the UDP IP to 127.0.0.1 and port to 4444.",
                "Alternatively, edit settings/electrics.json to enable OutGauge."
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, BEAMNG_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == BEAMNG_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "beamng_drive")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(BEAMNG_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "beamng_drive",
            "telemetry_protocol": BEAMNG_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In BeamNG.drive, enable the OutGauge app from the apps menu.",
                "Set the UDP IP to 127.0.0.1 and port to 4444.",
                "Alternatively, edit settings/electrics.json to enable OutGauge."
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: BEAMNG_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

const PCARS2_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/project_cars_2_bridge_contract.json";
const PCARS3_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/project_cars_3_bridge_contract.json";
const PCARS2_BRIDGE_PROTOCOL: &str = "sms_udp_pcars2";
const PCARS2_DEFAULT_PORT: u16 = 5606;

/// Project CARS 2 configuration writer.
pub struct PCars2ConfigWriter;

impl Default for PCars2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for PCars2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Project CARS 2 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, PCARS2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_2",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 2, enable UDP telemetry in Options > Visual > UDP Frequency.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "Alternatively, shared memory is used automatically on Windows."
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, PCARS2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == PCARS2_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "project_cars_2")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_2",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 2, enable UDP telemetry in Options > Visual > UDP Frequency.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "Alternatively, shared memory is used automatically on Windows."
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: PCARS2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Project CARS 3 configuration writer.
pub struct PCars3ConfigWriter;

impl Default for PCars3ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for PCars3ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Project CARS 3 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, PCARS3_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_3",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 3, enable UDP telemetry in Gameplay > HUD/Telemetry settings.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "The bridge uses the Project CARS 2-compatible SMS UDP packet layout."
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, PCARS3_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == PCARS2_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "project_cars_3")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_3",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 3, enable UDP telemetry in Gameplay > HUD/Telemetry settings.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "The bridge uses the Project CARS 2-compatible SMS UDP packet layout."
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: PCARS3_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

const LFS_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/live_for_speed_bridge_contract.json";
const LFS_BRIDGE_PROTOCOL: &str = "lfs_outgauge_udp";
const LFS_DEFAULT_PORT: u16 = 30000;

/// Live For Speed configuration writer.
pub struct LFSConfigWriter;

impl Default for LFSConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for LFSConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Live For Speed bridge contract configuration");

        let contract_path = resolve_game_path(game_path, LFS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(LFS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "live_for_speed",
            "telemetry_protocol": LFS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In LFS, enable OutGauge in Options > Output or edit cfg.lfs directly.",
                "Set OutGauge IP to 127.0.0.1 and Port to 30000.",
                "Example cfg.lfs entry: OutGauge Mode 1 Addr 127.0.0.1 Port 30000 Id 1 Delay 1"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, LFS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == LFS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "live_for_speed")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(LFS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "live_for_speed",
            "telemetry_protocol": LFS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In LFS, enable OutGauge in Options > Output or edit cfg.lfs directly.",
                "Set OutGauge IP to 127.0.0.1 and Port to 30000.",
                "Example cfg.lfs entry: OutGauge Mode 1 Addr 127.0.0.1 Port 30000 Id 1 Delay 1"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: LFS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// WRC Generations configuration writer.
///
/// WRC Generations / WRC 23 uses the Codemasters/RallyEngine UDP Mode 1 format.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct WrcGenerationsConfigWriter;

impl Default for WrcGenerationsConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for WrcGenerationsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing WRC Generations bridge contract configuration");

        let contract_path = resolve_game_path(game_path, WRC_GENERATIONS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_GENERATIONS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wrc_generations",
            "telemetry_protocol": WRC_GENERATIONS_BRIDGE_PROTOCOL,
            "mode": WRC_GENERATIONS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC Generations / WRC 23 uses the Codemasters/RallyEngine UDP Mode 1 format. Enable UDP telemetry in the game's accessibility settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, WRC_GENERATIONS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == WRC_GENERATIONS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "wrc_generations")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_GENERATIONS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wrc_generations",
            "telemetry_protocol": WRC_GENERATIONS_BRIDGE_PROTOCOL,
            "mode": WRC_GENERATIONS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC Generations / WRC 23 uses the Codemasters/RallyEngine UDP Mode 1 format. Enable UDP telemetry in the game's accessibility settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: WRC_GENERATIONS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Which Kylotonn WRC title this config writer represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrcKylotonnVariant {
    /// WRC 9
    Wrc9,
    /// WRC 10
    Wrc10,
}

impl WrcKylotonnVariant {
    fn game_id(self) -> &'static str {
        match self {
            Self::Wrc9 => "wrc_9",
            Self::Wrc10 => "wrc_10",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Wrc9 => "WRC 9 FIA World Rally Championship",
            Self::Wrc10 => "WRC 10 FIA World Rally Championship",
        }
    }
}

/// WRC 9 / WRC 10 (Kylotonn) configuration writer.
///
/// Both titles broadcast a custom binary UDP stream on port 64000. This writer creates a
/// bridge contract documenting the expected listener configuration.
pub struct WrcKylotonnConfigWriter {
    /// The specific title version (WRC 9 or WRC 10)
    pub variant: WrcKylotonnVariant,
}

impl ConfigWriter for WrcKylotonnConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let game_name = self.variant.display_name();
        info!("Writing {game_name} bridge contract configuration");

        let contract_path = resolve_game_path(game_path, WRC_KYLOTONN_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_KYLOTONN_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": self.variant.game_id(),
            "telemetry_protocol": WRC_KYLOTONN_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC 9 / WRC 10 broadcast a custom binary UDP packet on port 64000. Enable UDP telemetry in the game's settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, WRC_KYLOTONN_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == WRC_KYLOTONN_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == self.variant.game_id())
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_KYLOTONN_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": self.variant.game_id(),
            "telemetry_protocol": WRC_KYLOTONN_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC 9 / WRC 10 broadcast a custom binary UDP packet on port 64000. Enable UDP telemetry in the game's settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: WRC_KYLOTONN_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Dirt 4 configuration writer.
///
/// Dirt 4 uses the Codemasters extradata v0 UDP format on port 20777.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct Dirt4ConfigWriter;

impl Default for Dirt4ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Dirt4ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Dirt 4 bridge contract configuration");

        let contract_path = resolve_game_path(game_path, DIRT4_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt4",
            "telemetry_protocol": DIRT4_BRIDGE_PROTOCOL,
            "mode": DIRT4_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 4 uses the Codemasters extradata v0 UDP format. Enable UDP telemetry in the game's settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DIRT4_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == DIRT4_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt4")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt4",
            "telemetry_protocol": DIRT4_BRIDGE_PROTOCOL,
            "mode": DIRT4_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 4 uses the Codemasters extradata v0 UDP format. Enable UDP telemetry in the game's settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT4_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// ETS2/ATS configuration writer (SCS Telemetry SDK shared memory)
pub struct Ets2ConfigWriter;

impl Default for Ets2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Ets2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ETS2 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, ETS2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ETS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ets2",
            "telemetry_protocol": ETS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ETS2 uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, ETS2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "ets2")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ETS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ets2",
            "telemetry_protocol": ETS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ETS2 uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        Ok(vec![ConfigDiff {
            file_path: ETS2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// ATS configuration writer (SCS Telemetry SDK shared memory)
pub struct AtsConfigWriter;

impl Default for AtsConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for AtsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ATS bridge contract configuration");
        let contract_path = resolve_game_path(game_path, ATS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ATS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ats",
            "telemetry_protocol": ATS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ATS uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, ATS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "ats")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ATS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ats",
            "telemetry_protocol": ATS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ATS uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        Ok(vec![ConfigDiff {
            file_path: ATS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Wreckfest configuration writer (UDP on port 5606)
pub struct WreckfestConfigWriter;

impl Default for WreckfestConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for WreckfestConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Wreckfest bridge contract configuration");
        let contract_path = resolve_game_path(game_path, WRECKFEST_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WRECKFEST_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wreckfest",
            "telemetry_protocol": WRECKFEST_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Wreckfest sends UDP telemetry on port 5606. Validated by WRKF magic header.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, WRECKFEST_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "wreckfest")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WRECKFEST_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wreckfest",
            "telemetry_protocol": WRECKFEST_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Wreckfest sends UDP telemetry on port 5606. Validated by WRKF magic header.",
        });
        Ok(vec![ConfigDiff {
            file_path: WRECKFEST_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// FlatOut UC / FlatOut 4 configuration writer (UDP bridge on port 7776).
pub struct FlatOutConfigWriter;

impl Default for FlatOutConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for FlatOutConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing FlatOut bridge contract configuration");
        let contract_path = resolve_game_path(game_path, FLATOUT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FLATOUT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "flatout",
            "telemetry_protocol": FLATOUT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "FlatOut bridge sends UDP telemetry on port 7776. Validated by FOTC magic header.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, FLATOUT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "flatout")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FLATOUT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "flatout",
            "telemetry_protocol": FLATOUT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "FlatOut bridge sends UDP telemetry on port 7776. Validated by FOTC magic header.",
        });
        Ok(vec![ConfigDiff {
            file_path: FLATOUT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Dakar Desert Rally configuration writer (UDP bridge on port 7779).
pub struct DakarDesertRallyConfigWriter;

impl Default for DakarDesertRallyConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for DakarDesertRallyConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Dakar Desert Rally bridge contract configuration");
        let contract_path = resolve_game_path(game_path, DAKAR_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DAKAR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dakar_desert_rally",
            "telemetry_protocol": DAKAR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dakar Desert Rally bridge sends UDP telemetry on port 7779. Validated by DAKR magic header.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DAKAR_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dakar_desert_rally")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DAKAR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dakar_desert_rally",
            "telemetry_protocol": DAKAR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dakar Desert Rally bridge sends UDP telemetry on port 7779. Validated by DAKR magic header.",
        });
        Ok(vec![ConfigDiff {
            file_path: DAKAR_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Rennsport configuration writer (UDP on port 9000)
pub struct RennsportConfigWriter;

impl Default for RennsportConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RennsportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Rennsport bridge contract configuration");
        let contract_path = resolve_game_path(game_path, RENNSPORT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RENNSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rennsport",
            "telemetry_protocol": RENNSPORT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Rennsport sends UDP telemetry on port 9000. Validated by 0x52 'R' identifier byte.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, RENNSPORT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "rennsport")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RENNSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rennsport",
            "telemetry_protocol": RENNSPORT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Rennsport sends UDP telemetry on port 9000. Validated by 0x52 'R' identifier byte.",
        });
        Ok(vec![ConfigDiff {
            file_path: RENNSPORT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID Autosport configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct GridAutosportConfigWriter;

impl Default for GridAutosportConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for GridAutosportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID Autosport bridge contract configuration");
        let contract_path = resolve_game_path(game_path, GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(GRID_AUTOSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_autosport",
            "telemetry_protocol": GRID_AUTOSPORT_BRIDGE_PROTOCOL,
            "mode": GRID_AUTOSPORT_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Autosport uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GRID_AUTOSPORT_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "grid_autosport")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(GRID_AUTOSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_autosport",
            "telemetry_protocol": GRID_AUTOSPORT_BRIDGE_PROTOCOL,
            "mode": GRID_AUTOSPORT_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Autosport uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID 2019 configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct Grid2019ConfigWriter;

impl Default for Grid2019ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Grid2019ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID 2019 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, GRID_2019_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_2019_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_2019",
            "telemetry_protocol": GRID_2019_BRIDGE_PROTOCOL,
            "mode": GRID_2019_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID (2019) uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GRID_2019_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GRID_2019_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "grid_2019")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_2019_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_2019",
            "telemetry_protocol": GRID_2019_BRIDGE_PROTOCOL,
            "mode": GRID_2019_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID (2019) uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_2019_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID Legends configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct GridLegendsConfigWriter;

impl Default for GridLegendsConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for GridLegendsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID Legends bridge contract configuration");
        let contract_path = resolve_game_path(game_path, GRID_LEGENDS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(GRID_LEGENDS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_legends",
            "telemetry_protocol": GRID_LEGENDS_BRIDGE_PROTOCOL,
            "mode": GRID_LEGENDS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Legends uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GRID_LEGENDS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GRID_LEGENDS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "grid_legends")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(GRID_LEGENDS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_legends",
            "telemetry_protocol": GRID_LEGENDS_BRIDGE_PROTOCOL,
            "mode": GRID_LEGENDS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Legends uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_LEGENDS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// DiRT 3 configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct Dirt3ConfigWriter;

impl Default for Dirt3ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Dirt3ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing DiRT 3 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, DIRT3_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT3_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt3",
            "telemetry_protocol": DIRT3_BRIDGE_PROTOCOL,
            "mode": DIRT3_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT 3 uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DIRT3_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == DIRT3_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt3")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT3_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt3",
            "telemetry_protocol": DIRT3_BRIDGE_PROTOCOL,
            "mode": DIRT3_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT 3 uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: DIRT3_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Race Driver: GRID configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct RaceDriverGridConfigWriter;

impl Default for RaceDriverGridConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RaceDriverGridConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Race Driver: GRID bridge contract configuration");
        let contract_path = resolve_game_path(game_path, RACE_DRIVER_GRID_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(RACE_DRIVER_GRID_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "race_driver_grid",
            "telemetry_protocol": RACE_DRIVER_GRID_BRIDGE_PROTOCOL,
            "mode": RACE_DRIVER_GRID_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Race Driver: GRID uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, RACE_DRIVER_GRID_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == RACE_DRIVER_GRID_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "race_driver_grid")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(RACE_DRIVER_GRID_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "race_driver_grid",
            "telemetry_protocol": RACE_DRIVER_GRID_BRIDGE_PROTOCOL,
            "mode": RACE_DRIVER_GRID_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Race Driver: GRID uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: RACE_DRIVER_GRID_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Automobilista 1 configuration writer.
///
/// Automobilista 1 uses the ISI rFactor 1 shared memory (`$rFactor$`).
/// This writer creates a bridge contract for the OpenRacing telemetry pipeline.
pub struct AutomobilistaConfigWriter;

impl Default for AutomobilistaConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for AutomobilistaConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Automobilista 1 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, AUTOMOBILISTA_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "automobilista",
            "telemetry_protocol": AUTOMOBILISTA_BRIDGE_PROTOCOL,
            "shared_memory_name": "$rFactor$",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Automobilista 1 uses ISI rFactor 1 shared memory. No in-game config file is required.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, AUTOMOBILISTA_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == AUTOMOBILISTA_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "automobilista")
            .unwrap_or(false);
        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "automobilista",
            "telemetry_protocol": AUTOMOBILISTA_BRIDGE_PROTOCOL,
            "shared_memory_name": "$rFactor$",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Automobilista 1 uses ISI rFactor 1 shared memory. No in-game config file is required.",
        });
        Ok(vec![ConfigDiff {
            file_path: AUTOMOBILISTA_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// KartKraft configuration writer (FlatBuffers UDP on port 5000).
pub struct KartKraftConfigWriter;

impl Default for KartKraftConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for KartKraftConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing KartKraft bridge contract configuration");
        let contract_path = resolve_game_path(game_path, KARTKRAFT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(KARTKRAFT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "kartkraft",
            "telemetry_protocol": KARTKRAFT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "KartKraft sends FlatBuffers UDP packets (KKFB identifier) on port 5000.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, KARTKRAFT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "kartkraft")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(KARTKRAFT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "kartkraft",
            "telemetry_protocol": KARTKRAFT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "KartKraft sends FlatBuffers UDP packets (KKFB identifier) on port 5000.",
        });
        Ok(vec![ConfigDiff {
            file_path: KARTKRAFT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// RaceRoom Racing Experience configuration writer (R3E shared memory)
pub struct RaceRoomConfigWriter;

impl Default for RaceRoomConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RaceRoomConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing RaceRoom bridge contract configuration");
        let contract_path = resolve_game_path(game_path, RACEROOM_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "raceroom",
            "telemetry_protocol": RACEROOM_BRIDGE_PROTOCOL,
            "shared_memory_name": "Local\\$R3E",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "R3E shared memory is Windows-only. RaceRoom writes to Local\\$R3E automatically when running. No in-game settings required. Supported SDK version: 2.x",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, RACEROOM_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "raceroom")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "raceroom",
            "telemetry_protocol": RACEROOM_BRIDGE_PROTOCOL,
            "shared_memory_name": "Local\\$R3E",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "R3E shared memory is Windows-only. RaceRoom writes to Local\\$R3E automatically when running. No in-game settings required. Supported SDK version: 2.x",
        });
        Ok(vec![ConfigDiff {
            file_path: RACEROOM_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

impl ConfigWriter for EAWRCConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing EA WRC telemetry configuration");

        let telemetry_root = resolve_game_path(game_path, "Documents/My Games/WRC/telemetry");
        let config_path = telemetry_root.join("config.json");
        let structure_path = telemetry_root
            .join("udp")
            .join(format!("{EAWRC_STRUCTURE_ID}.json"));

        let existed_before = config_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&config_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let udp_value = root
            .entry("udp".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let udp_object = udp_value
            .as_object_mut()
            .ok_or_else(|| anyhow!("EA WRC config field 'udp' is not a JSON object"))?;

        let assignments_value = udp_object
            .entry("packetAssignments".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let assignments = assignments_value.as_array_mut().ok_or_else(|| {
            anyhow!("EA WRC config field 'udp.packetAssignments' is not a JSON array")
        })?;

        let listener_port = parse_target_port(&config.output_target).unwrap_or(EAWRC_DEFAULT_PORT);
        let listener_ip =
            parse_target_host(&config.output_target).unwrap_or_else(|| "127.0.0.1".to_string());

        let assignment = serde_json::json!({
            "packetId": EAWRC_PACKET_ID,
            "structureId": EAWRC_STRUCTURE_ID,
            "ip": listener_ip,
            "port": listener_port,
            "frequencyHz": i64::from(config.update_rate_hz),
            "bEnabled": config.enabled,
            "enabled": config.enabled,
        });

        let mut updated_existing = false;
        for existing in assignments.iter_mut() {
            let same_packet = existing
                .get("packetId")
                .and_then(Value::as_str)
                .map(|value| value == EAWRC_PACKET_ID)
                .unwrap_or(false);
            let same_structure = existing
                .get("structureId")
                .and_then(Value::as_str)
                .map(|value| value == EAWRC_STRUCTURE_ID)
                .unwrap_or(false);

            if same_packet && same_structure {
                *existing = assignment.clone();
                updated_existing = true;
                break;
            }
        }

        if !updated_existing {
            assignments.push(assignment);
        }

        let new_config_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, &new_config_content)?;

        if let Some(parent) = structure_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let structure_content = serde_json::to_string_pretty(&eawrc_structure_definition())?;
        fs::write(&structure_path, &structure_content)?;

        Ok(vec![
            ConfigDiff {
                file_path: config_path.to_string_lossy().to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: existing_content,
                new_value: new_config_content,
                operation: if existed_before {
                    DiffOperation::Modify
                } else {
                    DiffOperation::Add
                },
            },
            ConfigDiff {
                file_path: structure_path.to_string_lossy().to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: structure_content,
                operation: DiffOperation::Add,
            },
        ])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let telemetry_root = resolve_game_path(game_path, "Documents/My Games/WRC/telemetry");
        let config_path = telemetry_root.join("config.json");
        let structure_path = telemetry_root
            .join("udp")
            .join(format!("{EAWRC_STRUCTURE_ID}.json"));

        if !config_path.exists() || !structure_path.exists() {
            return Ok(false);
        }

        let config_value: Value = serde_json::from_str(&fs::read_to_string(config_path)?)?;
        let assignments = config_value
            .get("udp")
            .and_then(Value::as_object)
            .and_then(|udp| udp.get("packetAssignments"))
            .and_then(Value::as_array)
            .or_else(|| {
                config_value
                    .get("packetAssignments")
                    .and_then(Value::as_array)
            });

        let assignment_ok = assignments
            .map(|entries| {
                entries.iter().any(|entry| {
                    let packet_ok = entry
                        .get("packetId")
                        .and_then(Value::as_str)
                        .map(|value| value == EAWRC_PACKET_ID)
                        .unwrap_or(false);
                    let structure_ok = entry
                        .get("structureId")
                        .and_then(Value::as_str)
                        .map(|value| value == EAWRC_STRUCTURE_ID)
                        .unwrap_or(false);
                    let enabled_ok = entry
                        .get("bEnabled")
                        .and_then(Value::as_bool)
                        .or_else(|| entry.get("enabled").and_then(Value::as_bool))
                        .unwrap_or(false);
                    packet_ok && structure_ok && enabled_ok
                })
            })
            .unwrap_or(false);

        Ok(assignment_ok)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(EAWRC_DEFAULT_PORT);
        let listener_ip =
            parse_target_host(&config.output_target).unwrap_or_else(|| "127.0.0.1".to_string());

        let config_content = serde_json::to_string_pretty(&serde_json::json!({
            "udp": {
                "packetAssignments": [
                    {
                        "packetId": EAWRC_PACKET_ID,
                        "structureId": EAWRC_STRUCTURE_ID,
                        "ip": listener_ip,
                        "port": listener_port,
                        "frequencyHz": i64::from(config.update_rate_hz),
                        "bEnabled": config.enabled,
                        "enabled": config.enabled
                    }
                ]
            }
        }))?;
        let structure_content = serde_json::to_string_pretty(&eawrc_structure_definition())?;

        Ok(vec![
            ConfigDiff {
                file_path: "Documents/My Games/WRC/telemetry/config.json".to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: config_content,
                operation: DiffOperation::Add,
            },
            ConfigDiff {
                file_path: format!(
                    "Documents/My Games/WRC/telemetry/udp/{EAWRC_STRUCTURE_ID}.json"
                ),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: structure_content,
                operation: DiffOperation::Add,
            },
        ])
    }
}

fn eawrc_structure_definition() -> Value {
    serde_json::json!({
        "id": EAWRC_STRUCTURE_ID,
        "packets": [
            {
                "id": EAWRC_PACKET_ID,
                "header": {
                    "channels": ["packet_uid"]
                },
                "channels": [
                    "ffb_scalar",
                    "engine_rpm",
                    "vehicle_speed",
                    "gear",
                    "slip_ratio"
                ]
            }
        ]
    })
}

fn upsert_ini_value(
    content: &str,
    section: &str,
    key: &str,
    new_value: &str,
) -> (String, Option<String>, DiffOperation) {
    let section_header = format!("[{section}]");
    let key_prefix = format!("{key}=");

    let mut lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(ToOwned::to_owned).collect()
    };

    let mut section_start = None;
    let mut section_end = lines.len();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if section_start.is_some() {
                section_end = index;
                break;
            }

            if trimmed.eq_ignore_ascii_case(&section_header) {
                section_start = Some(index);
            }
        }
    }

    let mut previous_value = None;
    if let Some(start) = section_start {
        let search_start = start + 1;
        let mut key_line_index = None;

        for (index, line) in lines
            .iter()
            .enumerate()
            .take(section_end)
            .skip(search_start)
        {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(&key_prefix) {
                key_line_index = Some(index);
                previous_value = Some(rest.trim().to_string());
                break;
            }
        }

        if let Some(index) = key_line_index {
            lines[index] = format!("{key}={new_value}");
            let output = normalize_ini_output(lines);
            return (output, previous_value, DiffOperation::Modify);
        }

        lines.insert(section_end, format!("{key}={new_value}"));
        let output = normalize_ini_output(lines);
        return (output, previous_value, DiffOperation::Add);
    }

    if !lines.is_empty()
        && !lines
            .last()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
    {
        lines.push(String::new());
    }

    lines.push(section_header);
    lines.push(format!("{key}={new_value}"));
    let output = normalize_ini_output(lines);
    (output, previous_value, DiffOperation::Add)
}

fn normalize_ini_output(lines: Vec<String>) -> String {
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn parse_json_object(content: &str) -> Option<Map<String, Value>> {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|value| value.as_object().cloned())
}

fn parse_target_port(target: &str) -> Option<u16> {
    if let Ok(addr) = target.parse::<SocketAddr>() {
        return Some(addr.port());
    }

    let (_, port_part) = target.rsplit_once(':')?;
    port_part.parse::<u16>().ok()
}

fn parse_target_host(target: &str) -> Option<String> {
    if let Ok(addr) = target.parse::<SocketAddr>() {
        return Some(addr.ip().to_string());
    }

    let (host_part, _) = target.rsplit_once(':')?;
    if host_part.starts_with('[') && host_part.ends_with(']') {
        return Some(
            host_part
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
        );
    }

    Some(host_part.to_string())
}

/// NASCAR configuration writer (Papyrus UDP telemetry on port 5606)
pub struct NascarConfigWriter;

impl Default for NascarConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for NascarConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing NASCAR bridge contract configuration");
        let contract_path = resolve_game_path(game_path, NASCAR_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(NASCAR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "nascar",
            "telemetry_protocol": NASCAR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "NASCAR Racing (Papyrus series) sends Papyrus UDP packets on port 5606.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, NASCAR_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "nascar")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(NASCAR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "nascar",
            "telemetry_protocol": NASCAR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "NASCAR Racing (Papyrus series) sends Papyrus UDP packets on port 5606.",
        });
        Ok(vec![ConfigDiff {
            file_path: NASCAR_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// NASCAR 21: Ignition configuration writer (Papyrus UDP telemetry on port 5606).
pub struct Nascar21ConfigWriter;

impl Default for Nascar21ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Nascar21ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing NASCAR 21: Ignition bridge contract configuration");
        let contract_path = resolve_game_path(game_path, NASCAR_21_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(NASCAR_21_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "nascar_21",
            "telemetry_protocol": NASCAR_21_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "NASCAR 21: Ignition uses the Papyrus UDP telemetry format on port 5606.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, NASCAR_21_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "nascar_21")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(NASCAR_21_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "nascar_21",
            "telemetry_protocol": NASCAR_21_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "NASCAR 21: Ignition uses the Papyrus UDP telemetry format on port 5606.",
        });
        Ok(vec![ConfigDiff {
            file_path: NASCAR_21_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Le Mans Ultimate configuration writer (rF2 UDP telemetry on port 6789)
pub struct LeMansUltimateConfigWriter;

impl Default for LeMansUltimateConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for LeMansUltimateConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Le Mans Ultimate bridge contract configuration");
        let contract_path = resolve_game_path(game_path, LMU_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(LMU_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "le_mans_ultimate",
            "telemetry_protocol": LMU_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Le Mans Ultimate uses rF2 UDP telemetry protocol on port 6789.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, LMU_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "le_mans_ultimate")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(LMU_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "le_mans_ultimate",
            "telemetry_protocol": LMU_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Le Mans Ultimate uses rF2 UDP telemetry protocol on port 6789.",
        });
        Ok(vec![ConfigDiff {
            file_path: LMU_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// WTCR configuration writer (Codemasters UDP Mode 1 on port 6778)
pub struct WtcrConfigWriter;

impl Default for WtcrConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for WtcrConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing WTCR bridge contract configuration");
        let contract_path = resolve_game_path(game_path, WTCR_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WTCR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wtcr",
            "telemetry_protocol": WTCR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "udp_mode": WTCR_DEFAULT_MODE,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WTCR Race of the World uses Codemasters UDP Mode 1 on port 6778.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, WTCR_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "wtcr")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WTCR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wtcr",
            "telemetry_protocol": WTCR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "udp_mode": WTCR_DEFAULT_MODE,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WTCR Race of the World uses Codemasters UDP Mode 1 on port 6778.",
        });
        Ok(vec![ConfigDiff {
            file_path: WTCR_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Trackmania configuration writer (JSON-over-UDP on port 5004)
pub struct TrackmaniaConfigWriter;

impl Default for TrackmaniaConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for TrackmaniaConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Trackmania bridge contract configuration");
        let contract_path = resolve_game_path(game_path, TRACKMANIA_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(TRACKMANIA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "trackmania",
            "telemetry_protocol": TRACKMANIA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Trackmania sends JSON-over-UDP telemetry on port 5004.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, TRACKMANIA_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "trackmania")
            .unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(TRACKMANIA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "trackmania",
            "telemetry_protocol": TRACKMANIA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Trackmania sends JSON-over-UDP telemetry on port 5004.",
        });
        Ok(vec![ConfigDiff {
            file_path: TRACKMANIA_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// SimHub UDP JSON passthrough configuration writer.
pub struct SimHubConfigWriter;

impl Default for SimHubConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for SimHubConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing SimHub bridge contract configuration");
        let contract_path = resolve_game_path(game_path, SIMHUB_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(SIMHUB_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "simhub",
            "telemetry_protocol": SIMHUB_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "SimHub forwards game telemetry as JSON UDP datagrams on port 5555.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, SIMHUB_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "simhub")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(SIMHUB_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "simhub",
            "telemetry_protocol": SIMHUB_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "SimHub forwards game telemetry as JSON UDP datagrams on port 5555.",
        });
        Ok(vec![ConfigDiff {
            file_path: SIMHUB_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// MudRunner (Spintires: MudRunner) bridge contract writer.
pub struct MudRunnerConfigWriter;

impl Default for MudRunnerConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for MudRunnerConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing MudRunner bridge contract configuration");
        let contract_path = resolve_game_path(game_path, MUDRUNNER_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(MUDRUNNER_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "mudrunner",
            "telemetry_protocol": MUDRUNNER_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "MudRunner routes telemetry through SimHub JSON UDP on port 8877.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, MUDRUNNER_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "mudrunner")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(MUDRUNNER_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "mudrunner",
            "telemetry_protocol": MUDRUNNER_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "MudRunner routes telemetry through SimHub JSON UDP on port 8877.",
        });
        Ok(vec![ConfigDiff {
            file_path: MUDRUNNER_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// SnowRunner bridge contract writer.
pub struct SnowRunnerConfigWriter;

impl Default for SnowRunnerConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for SnowRunnerConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing SnowRunner bridge contract configuration");
        let contract_path = resolve_game_path(game_path, SNOWRUNNER_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(SNOWRUNNER_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "snowrunner",
            "telemetry_protocol": SNOWRUNNER_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "SnowRunner routes telemetry through SimHub JSON UDP on port 8877.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, SNOWRUNNER_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "snowrunner")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(SNOWRUNNER_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "snowrunner",
            "telemetry_protocol": SNOWRUNNER_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "SnowRunner routes telemetry through SimHub JSON UDP on port 8877.",
        });
        Ok(vec![ConfigDiff {
            file_path: SNOWRUNNER_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// MotoGP 23 / MotoGP 24 (Milestone) bridge contract writer.
///
/// Routes telemetry through SimHub JSON UDP on port 5556.
pub struct MotoGPConfigWriter;

impl Default for MotoGPConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for MotoGPConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing MotoGP bridge contract configuration");
        let contract_path = resolve_game_path(game_path, MOTOGP_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(MOTOGP_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "motogp",
            "telemetry_protocol": MOTOGP_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "MotoGP 23/24 telemetry requires SimHub UDP bridge on port 5556.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, MOTOGP_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "motogp")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(MOTOGP_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "motogp",
            "telemetry_protocol": MOTOGP_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "MotoGP 23/24 telemetry requires SimHub UDP bridge on port 5556.",
        });
        Ok(vec![ConfigDiff {
            file_path: MOTOGP_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// RIDE 5 (Milestone) bridge contract writer.
///
/// Routes telemetry through SimHub JSON UDP on port 5558.
pub struct Ride5ConfigWriter;

impl Default for Ride5ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Ride5ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing RIDE 5 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, RIDE5_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RIDE5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ride5",
            "telemetry_protocol": RIDE5_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RIDE 5 telemetry requires SimHub UDP bridge on port 5558.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, RIDE5_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "ride5")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RIDE5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ride5",
            "telemetry_protocol": RIDE5_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RIDE 5 telemetry requires SimHub UDP bridge on port 5558.",
        });
        Ok(vec![ConfigDiff {
            file_path: RIDE5_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

const RF1_PROTOCOL: &str = "rfactor1_udp";
const RF1_DEFAULT_PORT: u16 = 6776;
const RFACTOR1_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/rfactor1_bridge_contract.json";
const GTR2_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/gtr2_bridge_contract.json";
const RACE07_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/race07_bridge_contract.json";
const GSC_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/gsc_bridge_contract.json";

/// rFactor 1 engine UDP bridge configuration writer.
///
/// Shared implementation for rFactor1, GTR2, Race 07, and Game Stock Car, which
/// all use the same `TelemInfoV2` UDP protocol on port 6776.
pub struct RFactor1ConfigWriter {
    /// Game identifier, e.g. `"rfactor1"`, `"gtr2"`, `"race_07"`, `"gsc"`.
    pub game_id: &'static str,
}

impl ConfigWriter for RFactor1ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing {} bridge contract configuration", self.game_id);
        let relative_path = rf1_bridge_path(self.game_id);
        let contract_path = game_path.join(relative_path);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RF1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": self.game_id,
            "telemetry_protocol": RF1_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "rFactor 1 engine UDP telemetry on port 6776 (TelemInfoV2 format).",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(rf1_bridge_path(self.game_id));
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == self.game_id)
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RF1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": self.game_id,
            "telemetry_protocol": RF1_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "rFactor 1 engine UDP telemetry on port 6776 (TelemInfoV2 format).",
        });
        Ok(vec![ConfigDiff {
            file_path: rf1_bridge_path(self.game_id).to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

fn rf1_bridge_path(game_id: &str) -> &'static str {
    match game_id {
        "rfactor1" => RFACTOR1_BRIDGE_RELATIVE_PATH,
        "gtr2" => GTR2_BRIDGE_RELATIVE_PATH,
        "race_07" => RACE07_BRIDGE_RELATIVE_PATH,
        _ => GSC_BRIDGE_RELATIVE_PATH,
    }
}

/// V-Rally 4 configuration writer (Kylotonn UDP, port 64000).
pub struct VRally4ConfigWriter;

impl Default for VRally4ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for VRally4ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing V-Rally 4 bridge contract configuration");
        let contract_path = resolve_game_path(game_path, V_RALLY_4_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(V_RALLY_4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "v_rally_4",
            "telemetry_protocol": V_RALLY_4_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "V-Rally 4 uses the Kylotonn UDP binary format on port 64000.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, V_RALLY_4_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "v_rally_4")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(V_RALLY_4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "v_rally_4",
            "telemetry_protocol": V_RALLY_4_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "V-Rally 4 uses the Kylotonn UDP binary format on port 64000.",
        });
        Ok(vec![ConfigDiff {
            file_path: V_RALLY_4_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Gravel configuration writer (SimHub JSON UDP bridge, port 5555).
pub struct GravelConfigWriter;

impl Default for GravelConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for GravelConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Gravel bridge contract configuration");
        let contract_path = resolve_game_path(game_path, GRAVEL_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRAVEL_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gravel",
            "telemetry_protocol": GRAVEL_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Gravel routes telemetry through SimHub JSON UDP on port 5555.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, GRAVEL_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "gravel")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRAVEL_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gravel",
            "telemetry_protocol": GRAVEL_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Gravel routes telemetry through SimHub JSON UDP on port 5555.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRAVEL_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Sébastien Loeb Rally EVO configuration writer (stub — no native protocol).
pub struct SebLoebRallyConfigWriter;

impl Default for SebLoebRallyConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for SebLoebRallyConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Sébastien Loeb Rally EVO bridge contract configuration");
        let contract_path = resolve_game_path(game_path, SEB_LOEB_RALLY_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "seb_loeb_rally",
            "telemetry_protocol": SEB_LOEB_RALLY_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "Sébastien Loeb Rally EVO has limited telemetry support. Stub adapter.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, SEB_LOEB_RALLY_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "seb_loeb_rally")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "seb_loeb_rally",
            "telemetry_protocol": SEB_LOEB_RALLY_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "Sébastien Loeb Rally EVO has limited telemetry support. Stub adapter.",
        });
        Ok(vec![ConfigDiff {
            file_path: SEB_LOEB_RALLY_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// ACC2 (Assetto Corsa Competizione 2) configuration writer — stub.
pub struct ACC2ConfigWriter;

impl Default for ACC2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACC2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ACC2 bridge contract (stub — no telemetry protocol published)");
        let contract_path = resolve_game_path(game_path, ACC2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "acc2",
            "telemetry_protocol": ACC2_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "ACC2 has not been announced. No telemetry protocol documented. See F-022.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, ACC2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "acc2")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "acc2",
            "telemetry_protocol": ACC2_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "ACC2 has not been announced. No telemetry protocol documented. See F-022.",
        });
        Ok(vec![ConfigDiff {
            file_path: ACC2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// AC EVO (Assetto Corsa EVO) configuration writer — stub.
pub struct ACEvoConfigWriter;

impl Default for ACEvoConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACEvoConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing AC EVO bridge contract (stub — no telemetry protocol published)");
        let contract_path = resolve_game_path(game_path, AC_EVO_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let contract = serde_json::json!({
            "game_id": "ac_evo",
            "telemetry_protocol": AC_EVO_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "AC EVO is in Early Access with no public telemetry API. See F-022.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, AC_EVO_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "ac_evo")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "ac_evo",
            "telemetry_protocol": AC_EVO_BRIDGE_PROTOCOL,
            "enabled": config.enabled,
            "bridge_notes": "AC EVO is in Early Access with no public telemetry API. See F-022.",
        });
        Ok(vec![ConfigDiff {
            file_path: AC_EVO_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// DiRT Showdown configuration writer (Codemasters UDP Mode 1, port 20777).
pub struct DirtShowdownConfigWriter;

impl Default for DirtShowdownConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for DirtShowdownConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing DiRT Showdown bridge contract configuration");
        let contract_path = resolve_game_path(game_path, DIRT_SHOWDOWN_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_SHOWDOWN_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_showdown",
            "telemetry_protocol": DIRT_SHOWDOWN_BRIDGE_PROTOCOL,
            "mode": DIRT_SHOWDOWN_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Showdown uses Codemasters UDP Mode 1 on port 20777.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = resolve_game_path(game_path, DIRT_SHOWDOWN_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt_showdown")
            .unwrap_or(false))
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_SHOWDOWN_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_showdown",
            "telemetry_protocol": DIRT_SHOWDOWN_BRIDGE_PROTOCOL,
            "mode": DIRT_SHOWDOWN_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Showdown uses Codemasters UDP Mode 1 on port 20777.",
        });
        Ok(vec![ConfigDiff {
            file_path: DIRT_SHOWDOWN_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_writer_factories;
    use tempfile::tempdir;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_ams2_writer_round_trip() -> TestResult {
        let writer = AMS2ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_iracing_writer_optional_360hz_setting() -> TestResult {
        let writer = IRacingConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: true,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 2);
        assert!(writer.validate_config(temp_dir.path())?);

        let first = diffs
            .iter()
            .find(|diff| diff.key == "telemetryDiskFile")
            .expect("telemetryDiskFile diff should be present");
        let second = diffs
            .iter()
            .find(|diff| diff.key == "irsdkLog360Hz")
            .expect("irsdkLog360Hz diff should be present when enabled");

        assert_eq!(first.new_value, "1");
        assert_eq!(second.new_value, "1");

        let expected = writer.get_expected_diffs(&config)?;
        assert_eq!(expected.len(), 2);
        assert!(expected.iter().any(|diff| diff.key == "irsdkLog360Hz"));

        Ok(())
    }

    #[test]
    fn test_iracing_writer_without_360hz_is_idempotent() -> TestResult {
        let writer = IRacingConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let first = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(first.len(), 1);

        let app_ini_path = resolve_game_path(temp_dir.path(), "Documents/iRacing/app.ini");
        let first_content = std::fs::read_to_string(&app_ini_path)?;
        assert!(first_content.contains("telemetryDiskFile=1"));
        assert!(
            !first_content
                .lines()
                .any(|line| line.starts_with("irsdkLog360Hz="))
        );

        let second = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(second.len(), 1);
        assert!(
            second
                .iter()
                .all(|diff| diff.key == "telemetryDiskFile" && diff.new_value == "1")
        );

        let second_content = std::fs::read_to_string(&app_ini_path)?;
        assert!(second_content.contains("telemetryDiskFile=1"));
        assert!(
            !second_content
                .lines()
                .any(|line| line.starts_with("irsdkLog360Hz="))
        );

        Ok(())
    }

    #[test]
    fn test_rfactor2_writer_round_trip() -> TestResult {
        let writer = RFactor2ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_eawrc_writer_round_trip() -> TestResult {
        let writer = EAWRCConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp_schema".to_string(),
            output_target: "127.0.0.1:20790".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 2);
        assert!(writer.validate_config(temp_dir.path())?);

        let expected_structure = temp_dir
            .path()
            .join("Documents/My Games/WRC/telemetry/udp/openracing.json");
        assert!(expected_structure.exists());
        Ok(())
    }

    #[test]
    fn test_eawrc_validate_accepts_enabled_alias_key() -> TestResult {
        let writer = EAWRCConfigWriter;
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path().join("Documents/My Games/WRC/telemetry");
        let config_path = config_dir.join("config.json");
        let structure_path = config_dir.join("udp/openracing.json");

        fs::create_dir_all(&config_dir)?;
        if let Some(parent) = structure_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &config_path,
            r#"{
  "udp": {
    "packetAssignments": [
      {
        "packetId": "session_update",
        "structureId": "openracing",
        "ip": "127.0.0.1",
        "port": 20778,
        "frequencyHz": 120,
        "enabled": true
      }
    ]
  }
}"#,
        )?;
        fs::write(
            &structure_path,
            r#"{
  "id": "openracing",
  "packets": []
}"#,
        )?;

        assert!(writer.validate_config(temp_dir.path())?);

        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp_schema".to_string(),
            output_target: "127.0.0.1:20778".to_string(),
            fields: vec!["ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 2);

        Ok(())
    }

    #[test]
    fn test_ac_rally_writer_round_trip() -> TestResult {
        let writer = ACRallyConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "probe_discovery".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let probe_config = temp_dir
            .path()
            .join("Documents/Assetto Corsa Rally/Config/openracing_probe.json");
        assert!(probe_config.exists());
        Ok(())
    }

    #[test]
    fn test_acc_writer_round_trip_compat_schema() -> TestResult {
        let writer = ACCConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["speed_ms".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let value: Value = serde_json::from_str(&diffs[0].new_value)?;
        assert_eq!(value["updListenerPort"], 9000);
        assert_eq!(value["udpListenerPort"], 9000);
        assert_eq!(value["connectionPassword"], "");
        assert_eq!(value["commandPassword"], "");
        assert_eq!(value["broadcastingPort"], 9000);
        assert_eq!(value["connectionId"], "");
        assert_eq!(value["updateRateHz"], 100);
        Ok(())
    }

    #[test]
    fn test_acc_validate_accepts_full_contract() -> TestResult {
        let writer = ACCConfigWriter;
        let temp_dir = tempdir()?;
        let config_path = temp_dir
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &config_path,
            r#"{
  "udpListenerPort": 9000,
  "broadcastingPort": 9000,
  "connectionId": "",
  "connectionPassword": "",
  "commandPassword": "",
  "updateRateHz": 100,
  "updListenerPort": 9000
}"#,
        )?;

        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_dirt5_writer_round_trip() -> TestResult {
        let writer = Dirt5ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp_custom_codemasters".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec![
                "rpm".to_string(),
                "speed_ms".to_string(),
                "gear".to_string(),
                "slip_ratio".to_string(),
            ],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let expected = writer.get_expected_diffs(&config)?;
        assert_eq!(diffs.len(), expected.len());
        assert!(
            std::path::Path::new(&diffs[0].file_path)
                .ends_with(std::path::Path::new(&expected[0].file_path))
        );
        assert_eq!(diffs[0].new_value, expected[0].new_value);
        assert_eq!(diffs[0].operation, expected[0].operation);
        Ok(())
    }

    #[test]
    fn test_f1_writer_round_trip() -> TestResult {
        let writer = F1ConfigWriter;
        let temp_dir = tempdir()?;
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

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let expected = writer.get_expected_diffs(&config)?;
        assert_eq!(diffs.len(), expected.len());
        assert!(
            std::path::Path::new(&diffs[0].file_path)
                .ends_with(std::path::Path::new(&expected[0].file_path))
        );
        assert_eq!(diffs[0].new_value, expected[0].new_value);
        assert_eq!(diffs[0].operation, expected[0].operation);
        Ok(())
    }

    #[test]
    fn config_writer_factories_is_non_empty() {
        assert!(!config_writer_factories().is_empty());
    }
    #[test]
    fn config_writer_factories_contains_known_game_ids() {
        let ids: Vec<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for expected in ["iracing", "acc", "ams2", "rfactor2", "eawrc"] {
            assert!(ids.contains(&expected), "missing: {}", expected);
        }
    }
    #[test]
    fn config_writer_factories_does_not_contain_unknown() {
        let ids: Vec<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert!(!ids.contains(&"__no_such_game__"));
    }
    #[test]
    fn f1_25_writer_produces_valid_contract_content() -> TestResult {
        let writer = F1_25ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "f1_25_native_udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        let value: Value = serde_json::from_str(&diffs[0].new_value)?;
        assert_eq!(value["game_id"], "f1_25");
        assert_eq!(value["packet_format"], 2025);
        Ok(())
    }
    #[test]
    fn config_writer_factory_instantiates_writer_for_acc() -> TestResult {
        let factories = config_writer_factories();
        let acc_factory = factories
            .iter()
            .find(|(id, _)| *id == "acc")
            .map(|(_, f)| f)
            .ok_or("acc factory not found")?;
        let writer = acc_factory();
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["speed_ms".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert!(!diffs.is_empty());
        Ok(())
    }
}
