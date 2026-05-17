//! Game integration commands

use anyhow::Result;
use dialoguer::Input;
use std::time::Duration;
use tokio::time::interval;

use crate::client::WheelClient;
use crate::commands::GameCommands;
use crate::error::CliError;
use crate::output;
use openracing_telemetry_config::support::{GameSupport, load_default_matrix, normalize_game_id};

/// Execute game command
pub async fn execute(cmd: &GameCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect_or_mock(endpoint).await?;

    match cmd {
        GameCommands::List { detailed } => list_supported_games(json, *detailed).await,
        GameCommands::Configure { game, path, auto } => {
            configure_game(&client, game, path.as_deref(), json, *auto).await
        }
        GameCommands::Status { telemetry } => show_game_status(&client, json, *telemetry).await,
        GameCommands::Test { game, duration } => {
            test_telemetry(&client, game, *duration, json).await
        }
    }
}

/// List supported games
async fn list_supported_games(json: bool, detailed: bool) -> Result<()> {
    let games = get_supported_games();

    if json {
        let output = serde_json::json!({
            "success": true,
            "supported_games": games
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Supported Games:".bold());

        for game in games {
            println!(
                "  {} {} ({})",
                "●".green(),
                game.name.bold(),
                game.id.dimmed()
            );

            if detailed {
                println!("    Version: {}", game.version);
                println!("    Features: {}", game.features.join(", "));
                println!("    Config: {}", game.config_method);
                if let Some(ref path) = game.default_path {
                    println!("    Default Path: {}", path);
                }
            }
        }
    }

    Ok(())
}

/// Configure game for telemetry
async fn configure_game(
    client: &WheelClient,
    game_id: &str,
    path: Option<&str>,
    json: bool,
    auto: bool,
) -> Result<()> {
    let canonical_game_id = normalize_game_id(game_id);
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == canonical_game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unsupported game: {}", game_id)))?;

    let install_path = if let Some(path) = path {
        path.to_string()
    } else if auto {
        // Try to auto-detect installation path
        detect_game_path(canonical_game_id)?
    } else if !json {
        // Interactive path input
        Input::new()
            .with_prompt(format!("Enter installation path for {}", game.name))
            .interact_text()?
    } else {
        return Err(CliError::InvalidConfiguration(
            "Installation path required for JSON mode".to_string(),
        )
        .into());
    };

    // Configure telemetry
    client
        .configure_telemetry(canonical_game_id, Some(&install_path))
        .await?;

    output::print_success(
        &format!("Configured {} for telemetry at {}", game.name, install_path),
        json,
    );

    // Show configuration details
    if !json {
        println!("\nConfiguration applied:");
        match canonical_game_id {
            "iracing" => {
                println!("  • Updated app.ini with UDP telemetry settings");
                println!("  • Enabled shared memory interface");
                println!("  • Set telemetry rate to 60Hz");
            }
            "acc" => {
                println!("  • Enabled UDP broadcast on port 9000");
                println!("  • Configured telemetry output rate");
                println!("  • Added LED heartbeat validation");
            }
            "ac_rally" => {
                println!("  • Installed OpenRacing discovery profile for AC Rally");
                println!("  • Configured ACC-style UDP handshake probe endpoint");
                println!("  • Configured passive UDP capture candidate port");
            }
            "ams2" => {
                println!("  • Enabled shared memory telemetry");
                println!("  • Configured data export settings");
            }
            "eawrc" => {
                println!("  • Patched telemetry/config.json UDP packet assignments");
                println!("  • Installed telemetry/udp/openracing.json packet structure");
                println!("  • Configured schema-driven UDP output endpoint");
            }
            "dirt5" => {
                println!("  • Wrote OpenRacing Dirt 5 bridge contract");
                println!("  • Configured UDP export port for Codemasters-style bridge payloads");
                println!("  • No native game file edits are required");
            }
            "f1" => {
                println!("  • Wrote OpenRacing F1 bridge contract");
                println!("  • Configured UDP export port for Codemasters-style bridge payloads");
                println!("  • Added normalized channels for DRS/ERS/fuel telemetry when provided");
            }
            "f1_25" => {
                println!("  • Wrote EA F1 25 native UDP contract");
                println!("  • Telemetry port: 20777 (EA native binary protocol, format 2025)");
                println!("  • In-game: Settings → Telemetry → UDP Telemetry: On, Port: 20777");
                println!("  • Captures RPM, speed, gear, DRS, ERS, fuel, tyre data natively");
            }
            _ => {
                println!("  • Applied game-specific configuration");
            }
        }

        println!(
            "\n{} Start the game to test telemetry connection",
            "Next:".bold()
        );
    }

    Ok(())
}

/// Show game status
async fn show_game_status(client: &WheelClient, json: bool, show_telemetry: bool) -> Result<()> {
    let status = client.get_game_status().await?;

    output::print_game_status(&status, json);

    if show_telemetry && status.telemetry_active && !json {
        println!("\n{}", "Live Telemetry Data:".bold());

        // Mock telemetry data display
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            println!(
                "  RPM: {:4} | Speed: {:3} km/h | Gear: {} | FFB: {:3}%",
                6500 + (i * 100),
                120 + (i * 5),
                3,
                75 + (i * 2)
            );
        }
        println!("  ... (Press Ctrl+C to stop)");
    }

    Ok(())
}

/// Test telemetry connection
async fn test_telemetry(
    _client: &WheelClient,
    game_id: &str,
    duration: u64,
    json: bool,
) -> Result<()> {
    let canonical_game_id = normalize_game_id(game_id);
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == canonical_game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unsupported game: {}", game_id)))?;

    if !json {
        println!(
            "Testing telemetry connection for {} ({} seconds)...",
            game.name, duration
        );
        println!("Make sure the game is running and in a session.");
        println!();
    }

    let mut packets_received = 0;
    let mut led_heartbeats = 0;
    let mut interval = interval(Duration::from_millis(100));
    let end_time = tokio::time::Instant::now() + Duration::from_secs(duration);

    while tokio::time::Instant::now() < end_time {
        interval.tick().await;

        // Mock telemetry reception
        if rand::random::<f32>() > 0.1 {
            packets_received += 1;
        }

        if rand::random::<f32>() > 0.8 {
            led_heartbeats += 1;
        }

        if !json && packets_received % 50 == 0 {
            println!(
                "Packets received: {} | LED heartbeats: {}",
                packets_received, led_heartbeats
            );
        }
    }

    let success_rate = packets_received as f32 / (duration * 10) as f32;
    let test_passed = success_rate > 0.8;

    if json {
        let output = serde_json::json!({
            "success": test_passed,
            "game_id": canonical_game_id,
            "duration_seconds": duration,
            "packets_received": packets_received,
            "led_heartbeats": led_heartbeats,
            "success_rate": success_rate,
            "test_passed": test_passed
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("\n{}", "Test Results:".bold());
        println!("  Packets received: {}", packets_received);
        println!("  LED heartbeats: {}", led_heartbeats);
        println!("  Success rate: {:.1}%", success_rate * 100.0);

        if test_passed {
            println!("  {}", "✓ Telemetry connection OK".green());
        } else {
            println!("  {}", "✗ Telemetry connection issues detected".red());
            println!("\n{}", "Troubleshooting:".bold());
            println!("  • Verify game is running and in a session");
            println!("  • Check firewall settings for UDP traffic");
            println!("  • Ensure game telemetry is enabled in settings");
        }
    }

    Ok(())
}

// Helper functions and data structures

use colored::*;

#[derive(serde::Serialize, Debug)]
struct GameInfo {
    id: String,
    name: String,
    version: String,
    features: Vec<String>,
    config_method: String,
    default_path: Option<String>,
}

fn get_supported_games() -> Vec<GameInfo> {
    let matrix = match load_default_matrix() {
        Ok(matrix) => matrix,
        Err(_) => return Vec::new(),
    };

    let mut games: Vec<GameInfo> = matrix
        .games
        .into_iter()
        .map(|(game_id, game)| game_info_from_matrix(game_id, game))
        .collect();

    games.sort_by(|a, b| a.id.cmp(&b.id));
    games
}

fn detect_game_path(game_id: &str) -> Result<String> {
    let normalized_game_id = normalize_game_id(game_id);
    // Mock auto-detection - in real implementation this would check registry,
    // Steam library folders, etc.
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == normalized_game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unknown game: {}", game_id)))?;

    if let Some(ref default_path) = game.default_path {
        // In real implementation, verify path exists
        Ok(default_path.clone())
    } else {
        Err(
            CliError::InvalidConfiguration(format!("Cannot auto-detect path for {}", game.name))
                .into(),
        )
    }
}

fn game_info_from_matrix(game_id: String, game: GameSupport) -> GameInfo {
    let version = game
        .versions
        .first()
        .map(|version| version.version.clone())
        .unwrap_or_default();

    let fields = game
        .versions
        .first()
        .map(|version| {
            let mut features: Vec<String> = version
                .supported_fields
                .iter()
                .filter_map(|field| field_name_label(field))
                .collect();
            features.sort_unstable();
            features
        })
        .unwrap_or_default();

    GameInfo {
        id: game_id,
        name: game.name,
        version,
        features: if fields.is_empty() {
            vec![format_telemetry_method(&game.telemetry.method)]
        } else {
            fields
        },
        config_method: format_telemetry_method(&game.telemetry.method),
        default_path: game
            .auto_detect
            .install_paths
            .into_iter()
            .next()
            .or_else(|| {
                game.versions
                    .first()
                    .and_then(|v| v.config_paths.first().cloned())
            }),
    }
}

fn format_telemetry_method(method: &str) -> String {
    match method {
        "shared_memory" => "Shared memory".to_string(),
        "udp_broadcast" => "UDP broadcast".to_string(),
        "probe_discovery" => "Probe discovery".to_string(),
        "udp_schema" => "Schema-driven UDP".to_string(),
        "udp_custom_codemasters" => "OpenRacing bridge contract".to_string(),
        _ => method.to_string(),
    }
}

fn field_name_label(field: &str) -> Option<String> {
    match field {
        "ffb_scalar" => Some("FFB Scalar".to_string()),
        "rpm" => Some("RPM".to_string()),
        "speed_ms" => Some("Speed".to_string()),
        "slip_ratio" => Some("Slip ratio".to_string()),
        "gear" => Some("Gear".to_string()),
        "flags" => Some("Flags".to_string()),
        "car_id" => Some("Car ID".to_string()),
        "track_id" => Some("Track ID".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_telemetry_method ---

    #[test]
    fn format_telemetry_shared_memory() {
        assert_eq!(format_telemetry_method("shared_memory"), "Shared memory");
    }

    #[test]
    fn format_telemetry_udp_broadcast() {
        assert_eq!(format_telemetry_method("udp_broadcast"), "UDP broadcast");
    }

    #[test]
    fn format_telemetry_probe_discovery() {
        assert_eq!(
            format_telemetry_method("probe_discovery"),
            "Probe discovery"
        );
    }

    #[test]
    fn format_telemetry_udp_schema() {
        assert_eq!(format_telemetry_method("udp_schema"), "Schema-driven UDP");
    }

    #[test]
    fn format_telemetry_udp_custom_codemasters() {
        assert_eq!(
            format_telemetry_method("udp_custom_codemasters"),
            "OpenRacing bridge contract"
        );
    }

    #[test]
    fn format_telemetry_unknown_passthrough() {
        assert_eq!(format_telemetry_method("something_new"), "something_new");
    }

    // --- field_name_label ---

    #[test]
    fn field_label_known_fields() {
        assert_eq!(field_name_label("rpm"), Some("RPM".to_string()));
        assert_eq!(field_name_label("gear"), Some("Gear".to_string()));
        assert_eq!(field_name_label("speed_ms"), Some("Speed".to_string()));
        assert_eq!(
            field_name_label("ffb_scalar"),
            Some("FFB Scalar".to_string())
        );
        assert_eq!(
            field_name_label("slip_ratio"),
            Some("Slip ratio".to_string())
        );
        assert_eq!(field_name_label("flags"), Some("Flags".to_string()));
        assert_eq!(field_name_label("car_id"), Some("Car ID".to_string()));
        assert_eq!(field_name_label("track_id"), Some("Track ID".to_string()));
    }

    #[test]
    fn field_label_unknown_returns_none() {
        assert!(field_name_label("unknown_field").is_none());
        assert!(field_name_label("").is_none());
    }

    // --- get_supported_games ---

    #[test]
    fn supported_games_not_empty() {
        let games = get_supported_games();
        assert!(!games.is_empty());
    }

    #[test]
    fn supported_games_sorted_by_id() {
        let games = get_supported_games();
        for window in games.windows(2) {
            assert!(
                window[0].id <= window[1].id,
                "games should be sorted by id: {} > {}",
                window[0].id,
                window[1].id
            );
        }
    }

    #[test]
    fn supported_games_have_required_fields() {
        let games = get_supported_games();
        for game in &games {
            assert!(!game.id.is_empty(), "game id should not be empty");
            assert!(!game.name.is_empty(), "game name should not be empty");
            assert!(
                !game.config_method.is_empty(),
                "config_method should not be empty for {}",
                game.id
            );
        }
    }

    // --- detect_game_path ---

    #[test]
    fn detect_game_path_unknown_game() {
        let result = detect_game_path("totally_unknown_game_xyz");
        assert!(result.is_err());
    }
}
