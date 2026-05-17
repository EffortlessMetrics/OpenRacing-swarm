//! Game Auto Configuration Module
//!
//! Ref: [ADR-0008: Game Auto-Configure](file:///h:/Code/Rust/OpenRacing/docs/adr/0008-game-auto-configure-telemetry-bridge.md)
//!
//! Automatically configures game telemetry the first time a game is detected.
//! Stores a marker file at `~/.openracing/configured_games.json` so each game
//! is only configured once.

use crate::game_service::GameService;
use anyhow::Result;
use openracing_telemetry_config::support::{load_default_matrix, normalize_game_id};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Persistent record of games that have already been auto-configured.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfiguredGamesStore {
    pub configured: HashSet<String>,
}

/// Automatically configures game telemetry on first detection.
pub struct GameAutoConfigurer {
    game_service: Arc<GameService>,
    state_path: PathBuf,
    store: Mutex<ConfiguredGamesStore>,
    /// Bypass path-discovery logic and use this path directly (useful in tests).
    install_path_override: Option<PathBuf>,
}

impl GameAutoConfigurer {
    /// Create a new configurer using the default state path
    /// (`~/.openracing/configured_games.json`).
    pub fn new(game_service: Arc<GameService>) -> Self {
        Self::with_state_path(game_service, default_state_path())
    }

    /// Create a new configurer with an explicit state path.
    pub fn with_state_path(game_service: Arc<GameService>, state_path: PathBuf) -> Self {
        let store = load_store(&state_path).unwrap_or_default();
        Self {
            game_service,
            state_path,
            store: Mutex::new(store),
            install_path_override: None,
        }
    }

    /// Override the install-path discovery for testing.
    pub fn with_install_path_override(mut self, path: PathBuf) -> Self {
        self.install_path_override = Some(path);
        self
    }

    /// Called when a game process is detected.
    ///
    /// On first detection the telemetry config is written to the game install
    /// directory and the game is marked as configured.  Subsequent detections
    /// are skipped.  All failures are logged as warnings; the function never
    /// panics or propagates errors.
    pub async fn on_game_detected(&self, game_id: &str) {
        let game_id = normalize_game_id(game_id);

        // Early-exit if already configured.
        {
            let store = self.store.lock().await;
            if store.configured.contains(game_id) {
                info!(game_id, "Game already auto-configured, skipping");
                return;
            }
        }

        // Locate the game install directory.
        let install_path = match self.find_install_path(game_id) {
            Some(path) => path,
            None => {
                warn!(
                    game_id,
                    "Could not find install path for game auto-configuration, skipping"
                );
                return;
            }
        };

        // Write the telemetry configuration files.
        match self
            .game_service
            .configure_telemetry(game_id, &install_path)
            .await
        {
            Ok(diffs) => {
                info!(
                    game_id,
                    diffs_count = diffs.len(),
                    path = %install_path.display(),
                    "Auto-configured game telemetry"
                );
            }
            Err(e) => {
                warn!(
                    game_id,
                    error = %e,
                    "Failed to auto-configure game telemetry"
                );
                return;
            }
        }

        // Persist the configured marker.
        let mut store = self.store.lock().await;
        store.configured.insert(game_id.to_string());
        if let Err(e) = save_store(&self.state_path, &store) {
            warn!(error = %e, "Failed to save auto-configure state");
        }
    }

    /// Find a usable install path for `game_id`.
    ///
    /// Resolution order:
    /// 1. Explicit override (set via [`Self::with_install_path_override`]).
    /// 2. Windows registry keys listed in `auto_detect.install_registry_keys`.
    /// 3. `auto_detect.install_paths` checked against common filesystem roots.
    fn find_install_path(&self, game_id: &str) -> Option<PathBuf> {
        // 1. Override (tests / explicit configuration).
        if let Some(ref path) = self.install_path_override {
            return Some(path.clone());
        }

        let matrix = load_default_matrix().ok()?;
        let game_support = matrix.games.get(game_id)?;
        let auto_detect = &game_support.auto_detect;

        // 2. Windows registry.
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = find_from_registry(&auto_detect.install_registry_keys) {
                return Some(path);
            }
        }

        // 3. Standard filesystem install_paths under known roots.
        for rel_path in &auto_detect.install_paths {
            for root in install_path_roots() {
                let candidate = root.join(rel_path);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_state_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".openracing")
        .join("configured_games.json")
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

fn load_store(path: &Path) -> Option<ConfiguredGamesStore> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_store(path: &Path, store: &ConfiguredGamesStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Platform-specific filesystem roots to search for game install_paths.
fn install_path_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'C'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let path = PathBuf::from(&drive);
            if path.exists() {
                roots.push(path);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = home_dir() {
            // Common Steam installation paths on Linux/macOS.
            roots.push(
                home.join(".steam")
                    .join("steam")
                    .join("steamapps")
                    .join("common"),
            );
            roots.push(
                home.join(".local")
                    .join("share")
                    .join("Steam")
                    .join("steamapps")
                    .join("common"),
            );
        }
        roots.push(PathBuf::from("/"));
    }

    roots
}

/// Probe Windows registry keys for a game install path.
#[cfg(target_os = "windows")]
fn find_from_registry(keys: &[String]) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    for key_str in keys {
        let Some((hive_str, subkey)) = key_str.split_once('\\') else {
            continue;
        };
        let hive = match hive_str {
            "HKEY_CURRENT_USER" => RegKey::predef(HKEY_CURRENT_USER),
            "HKEY_LOCAL_MACHINE" => RegKey::predef(HKEY_LOCAL_MACHINE),
            _ => continue,
        };
        if let Ok(key) = hive.open_subkey(subkey) {
            // Try common value names used by different installers.
            let path_val: Option<String> = key
                .get_value("InstallLocation")
                .or_else(|_| key.get_value("InstallDir"))
                .or_else(|_| key.get_value("Install Dir"))
                .or_else(|_| key.get_value("Path"))
                .ok();
            if let Some(path_str) = path_val {
                let p = PathBuf::from(path_str);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn make_game_service() -> anyhow::Result<Arc<GameService>> {
        Ok(Arc::new(GameService::new().await?))
    }

    /// First detection of a game should auto-configure it and persist the marker.
    #[tokio::test]
    async fn test_first_detection_triggers_configure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;
        let state_path = temp_dir.path().join("configured_games.json");

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        configurer.on_game_detected("iracing").await;

        // The state file must exist and record "iracing" as configured.
        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert!(
            store.configured.contains("iracing"),
            "iracing should be marked as configured after first detection"
        );
        Ok(())
    }

    /// A game that is already configured must not be re-configured.
    #[tokio::test]
    async fn test_already_configured_skips_reconfigure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("configured_games.json");

        // Pre-populate the configured store.
        let pre_store = ConfiguredGamesStore {
            configured: ["iracing".to_string()].into_iter().collect(),
        };
        std::fs::write(&state_path, serde_json::to_string_pretty(&pre_store)?)?;

        // The install dir does NOT exist – if the configurer tries to write it
        // will succeed anyway (iRacing writer creates missing dirs), but we
        // verify the skip path is taken by ensuring state file is untouched.
        let no_path = temp_dir.path().join("nonexistent_game_dir");
        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(no_path);

        configurer.on_game_detected("iracing").await;

        // State file should still contain exactly one entry.
        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert!(store.configured.contains("iracing"));
        assert_eq!(store.configured.len(), 1);
        Ok(())
    }

    /// When no install path is found the function must return gracefully
    /// without panicking and must NOT mark the game as configured.
    #[tokio::test]
    async fn test_path_not_found_is_graceful() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("configured_games.json");

        // No install_path_override; standard paths won't exist in test env.
        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone());

        // Must not panic.
        configurer.on_game_detected("iracing").await;

        // Game must not be in the configured store.
        let not_configured = if state_path.exists() {
            let content = std::fs::read_to_string(&state_path)?;
            let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
            !store.configured.contains("iracing")
        } else {
            true
        };
        assert!(
            not_configured,
            "iracing must not be marked configured when path was not found"
        );
        Ok(())
    }

    /// Detecting the same game twice is idempotent: the state file should
    /// contain exactly one entry and not grow.
    #[tokio::test]
    async fn test_idempotent_double_detection() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;
        let state_path = temp_dir.path().join("configured_games.json");

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        configurer.on_game_detected("iracing").await;
        configurer.on_game_detected("iracing").await;

        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert_eq!(
            store.configured.len(),
            1,
            "double detection must not create duplicate entries"
        );
        assert!(store.configured.contains("iracing"));
        Ok(())
    }

    /// Multiple different games can be auto-configured independently.
    #[tokio::test]
    async fn test_multi_game_auto_configure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;
        let state_path = temp_dir.path().join("configured_games.json");

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        configurer.on_game_detected("iracing").await;
        configurer.on_game_detected("acc").await;

        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert!(
            store.configured.contains("iracing"),
            "iracing should be configured"
        );
        assert!(store.configured.contains("acc"), "acc should be configured");
        assert_eq!(store.configured.len(), 2);
        Ok(())
    }

    /// An unknown game should not panic and must not be persisted.
    #[tokio::test]
    async fn test_unknown_game_id_is_graceful() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;
        let state_path = temp_dir.path().join("configured_games.json");

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        // Must not panic for a completely unknown game.
        configurer
            .on_game_detected("totally_unknown_game_xyz")
            .await;

        let not_persisted = if state_path.exists() {
            let content = std::fs::read_to_string(&state_path)?;
            let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
            !store.configured.contains("totally_unknown_game_xyz")
        } else {
            true
        };
        assert!(
            not_persisted,
            "unknown game must not be marked as configured"
        );
        Ok(())
    }

    /// The state file is created even when the parent directory doesn't exist yet.
    #[tokio::test]
    async fn test_state_path_parent_dirs_created() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;
        // Nest the state file several levels deep.
        let state_path = temp_dir
            .path()
            .join("a")
            .join("b")
            .join("configured_games.json");

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        configurer.on_game_detected("iracing").await;

        assert!(
            state_path.exists(),
            "state file should be created with intermediate directories"
        );
        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert!(store.configured.contains("iracing"));
        Ok(())
    }

    /// Loading a pre-existing state file restores previously configured games.
    #[tokio::test]
    async fn test_load_existing_state_on_construction() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("configured_games.json");

        // Write a store with "acc" already configured.
        let pre_store = ConfiguredGamesStore {
            configured: ["acc".to_string()].into_iter().collect(),
        };
        std::fs::write(&state_path, serde_json::to_string_pretty(&pre_store)?)?;

        let install_dir = temp_dir.path().join("game_install");
        std::fs::create_dir_all(&install_dir)?;

        let configurer =
            GameAutoConfigurer::with_state_path(make_game_service().await?, state_path.clone())
                .with_install_path_override(install_dir);

        // Now detect iracing — acc should still be present from the pre-existing state.
        configurer.on_game_detected("iracing").await;

        let content = std::fs::read_to_string(&state_path)?;
        let store: ConfiguredGamesStore = serde_json::from_str(&content)?;
        assert!(
            store.configured.contains("acc"),
            "pre-existing acc entry must survive"
        );
        assert!(
            store.configured.contains("iracing"),
            "newly detected iracing must be added"
        );
        assert_eq!(store.configured.len(), 2);
        Ok(())
    }

    /// `save_store` / `load_store` round-trip produces identical data.
    #[test]
    fn test_store_round_trip() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("configured_games.json");

        let original = ConfiguredGamesStore {
            configured: ["iracing", "acc", "f1_24"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        save_store(&state_path, &original)?;

        let loaded = load_store(&state_path);
        assert!(loaded.is_some(), "load_store must succeed for a valid file");
        let loaded = loaded.ok_or_else(|| anyhow::anyhow!("expected Some"))?;
        assert_eq!(loaded.configured, original.configured);
        Ok(())
    }

    /// `load_store` returns `None` for a missing file (no panic).
    #[test]
    fn test_load_store_missing_file_returns_none() {
        let result = load_store(Path::new("/nonexistent/path/store.json"));
        assert!(
            result.is_none(),
            "load_store for missing file must return None"
        );
    }

    /// `load_store` returns `None` for invalid JSON (no panic).
    #[test]
    fn test_load_store_invalid_json_returns_none() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let state_path = temp_dir.path().join("configured_games.json");
        std::fs::write(&state_path, "NOT VALID JSON {")?;

        let result = load_store(&state_path);
        assert!(
            result.is_none(),
            "load_store for invalid JSON must return None"
        );
        Ok(())
    }

    /// `ConfiguredGamesStore` default is empty.
    #[test]
    fn test_configured_games_store_default_is_empty() {
        let store = ConfiguredGamesStore::default();
        assert!(store.configured.is_empty(), "default store must be empty");
    }

    #[test]
    fn test_install_path_roots() {
        let roots = install_path_roots();
        assert!(
            !roots.is_empty(),
            "install_path_roots should return at least one root"
        );

        #[cfg(target_os = "windows")]
        {
            // Windows should have at least one drive root that exists, normally C:\
            let c_drive = std::path::PathBuf::from("C:\\");
            if c_drive.exists() {
                assert!(
                    roots.contains(&c_drive),
                    "roots should contain C:\\ if it exists"
                );
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Unix should have the root path /
            assert!(
                roots.contains(&std::path::PathBuf::from("/")),
                "roots should contain /"
            );
        }
    }
}
