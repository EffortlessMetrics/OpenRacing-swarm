//! Auto Profile Switching Module
//!
//! Implements automatic profile switching based on game detection (GI-02)
//! Provides ≤500ms response time for profile switching

use crate::game_auto_configure::GameAutoConfigurer;
use crate::game_telemetry_bridge::TelemetryAdapterControl;
use crate::process_detection::{ProcessDetectionService, ProcessEvent};
use crate::profile_service::ProfileService;
use anyhow::Result;
use openracing_telemetry_config::support::load_default_matrix;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Auto profile switching service
pub struct AutoProfileSwitchingService {
    /// Profile service for applying profiles
    profile_service: Arc<ProfileService>,
    /// Process detection service
    process_detection: ProcessDetectionService,
    /// Process event receiver
    process_events: mpsc::UnboundedReceiver<ProcessEvent>,
    /// Game to profile mappings
    game_profiles: Arc<RwLock<HashMap<String, String>>>,
    /// Currently active profile
    active_profile: Arc<RwLock<Option<String>>>,
    /// Switch timeout (500ms requirement)
    switch_timeout: Duration,
    /// Last switch time for performance tracking
    last_switch_time: Arc<RwLock<Option<Instant>>>,
    /// Optional telemetry adapter control for starting/stopping adapters on game events
    adapter_control: Option<Arc<dyn TelemetryAdapterControl>>,
    /// Optional auto-configurer for writing game telemetry config on first detection
    auto_configurer: Option<Arc<GameAutoConfigurer>>,
}

/// Profile switching event
#[derive(Debug, Clone)]
pub struct ProfileSwitchEvent {
    pub game_id: String,
    pub profile_id: String,
    pub switch_time_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

impl AutoProfileSwitchingService {
    /// Create new auto profile switching service
    pub fn new(profile_service: Arc<ProfileService>) -> Result<Self> {
        let (process_detection, process_events) = ProcessDetectionService::new();

        Ok(Self {
            profile_service,
            process_detection,
            process_events,
            game_profiles: Arc::new(RwLock::new(HashMap::new())),
            active_profile: Arc::new(RwLock::new(None)),
            switch_timeout: Duration::from_millis(500), // GI-02 requirement
            last_switch_time: Arc::new(RwLock::new(None)),
            adapter_control: None,
            auto_configurer: None,
        })
    }

    /// Attach a [`TelemetryAdapterControl`] so that adapters are started and
    /// stopped automatically alongside profile switches.
    pub fn with_adapter_control(mut self, control: Arc<dyn TelemetryAdapterControl>) -> Self {
        self.adapter_control = Some(control);
        self
    }

    /// Attach a [`GameAutoConfigurer`] so that telemetry is configured
    /// automatically on first game detection.
    pub fn with_game_auto_configurer(mut self, configurer: Arc<GameAutoConfigurer>) -> Self {
        self.auto_configurer = Some(configurer);
        self
    }

    /// Start the auto profile switching service
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting auto profile switching service");

        // Add game process patterns from support matrix
        self.setup_game_patterns().await?;

        // Start process monitoring in background
        let mut process_detection = std::mem::take(&mut self.process_detection);
        tokio::spawn(async move {
            if let Err(e) = process_detection.start_monitoring().await {
                error!(error = %e, "Process detection monitoring failed");
            }
        });

        // Handle process events
        self.handle_process_events().await
    }

    /// Setup game process patterns from support matrix
    async fn setup_game_patterns(&mut self) -> Result<()> {
        let matrix = load_default_matrix().map_err(|err| {
            anyhow::anyhow!(
                "Failed to load telemetry support matrix for auto-detection: {}",
                err
            )
        })?;
        let game_count = matrix.games.len();

        for (game_id, game_support) in matrix.games {
            if !game_support.auto_detect.process_names.is_empty() {
                self.process_detection
                    .add_game_patterns(game_id, game_support.auto_detect.process_names);
            }
        }

        info!(
            games = game_count,
            "Setup game process patterns from shared telemetry support matrix"
        );
        Ok(())
    }

    /// Handle process detection events
    async fn handle_process_events(&mut self) -> Result<()> {
        info!("Starting process event handling");

        while let Some(event) = self.process_events.recv().await {
            self.handle_event(event).await;
        }

        Ok(())
    }

    /// Process a single [`ProcessEvent`].
    ///
    /// Extracted from the event loop so it can be called directly in tests.
    pub async fn handle_event(&self, event: ProcessEvent) {
        match event {
            ProcessEvent::GameStarted {
                game_id,
                process_info,
            } => {
                info!(
                    game_id = %game_id,
                    process = %process_info.name,
                    pid = process_info.pid,
                    "Game started, attempting profile switch and telemetry activation"
                );

                if let Some(configurer) = &self.auto_configurer {
                    configurer.on_game_detected(&game_id).await;
                }

                if let Err(e) = self.switch_to_game_profile(&game_id).await {
                    error!(
                        game_id = %game_id,
                        error = %e,
                        "Failed to switch to game profile"
                    );
                }

                if let Err(e) = self.start_telemetry_for_game(&game_id).await {
                    error!(
                        game_id = %game_id,
                        error = %e,
                        "Failed to start telemetry adapter"
                    );
                }
            }
            ProcessEvent::GameStopped { game_id, .. } => {
                info!(game_id = %game_id, "Game stopped, deactivating telemetry and restoring global profile");

                if let Err(e) = self.stop_telemetry_for_game(&game_id).await {
                    error!(game_id = %game_id, error = %e, "Failed to stop telemetry adapter");
                }

                // Switch back to global profile when game stops
                if let Err(e) = self.switch_to_global_profile().await {
                    error!(error = %e, "Failed to switch to global profile");
                }
            }
            ProcessEvent::ProcessListUpdated { .. } => {
                // Log process list updates at debug level
                debug!("Process list updated");
            }
        }
    }

    /// Start the telemetry adapter for `game_id` if a control is configured.
    async fn start_telemetry_for_game(&self, game_id: &str) -> Result<()> {
        if let Some(control) = &self.adapter_control {
            control.start_for_game(game_id).await?;
        }
        Ok(())
    }

    /// Stop the telemetry adapter for `game_id` if a control is configured.
    async fn stop_telemetry_for_game(&self, game_id: &str) -> Result<()> {
        if let Some(control) = &self.adapter_control {
            control.stop_for_game(game_id).await?;
        }
        Ok(())
    }

    /// Switch to game-specific profile (GI-02)
    async fn switch_to_game_profile(&self, game_id: &str) -> Result<ProfileSwitchEvent> {
        let start_time = Instant::now();

        // Get profile ID for the game
        let game_profiles = self.game_profiles.read().await;
        let profile_id = game_profiles
            .get(game_id)
            .cloned()
            .unwrap_or_else(|| format!("{}_default", game_id));
        drop(game_profiles);

        info!(
            game_id = %game_id,
            profile_id = %profile_id,
            "Switching to game profile"
        );

        // Perform the profile switch with timeout
        let switch_result = timeout(self.switch_timeout, self.apply_profile(&profile_id)).await;

        let switch_time = start_time.elapsed();
        let switch_time_ms = switch_time.as_millis() as u64;

        // Update last switch time
        {
            let mut last_switch = self.last_switch_time.write().await;
            *last_switch = Some(start_time);
        }

        match switch_result {
            Ok(Ok(())) => {
                // Update active profile
                {
                    let mut active = self.active_profile.write().await;
                    *active = Some(profile_id.clone());
                }

                info!(
                    game_id = %game_id,
                    profile_id = %profile_id,
                    switch_time_ms = switch_time_ms,
                    "Successfully switched to game profile"
                );

                Ok(ProfileSwitchEvent {
                    game_id: game_id.to_string(),
                    profile_id,
                    switch_time_ms,
                    success: true,
                    error: None,
                })
            }
            Ok(Err(e)) => {
                let error_msg = e.to_string();
                warn!(
                    game_id = %game_id,
                    profile_id = %profile_id,
                    switch_time_ms = switch_time_ms,
                    error = %error_msg,
                    "Failed to switch to game profile"
                );

                Ok(ProfileSwitchEvent {
                    game_id: game_id.to_string(),
                    profile_id,
                    switch_time_ms,
                    success: false,
                    error: Some(error_msg),
                })
            }
            Err(_) => {
                let error_msg = format!(
                    "Profile switch timeout (>{}ms)",
                    self.switch_timeout.as_millis()
                );
                error!(
                    game_id = %game_id,
                    profile_id = %profile_id,
                    timeout_ms = self.switch_timeout.as_millis(),
                    "Profile switch timed out"
                );

                Ok(ProfileSwitchEvent {
                    game_id: game_id.to_string(),
                    profile_id,
                    switch_time_ms,
                    success: false,
                    error: Some(error_msg),
                })
            }
        }
    }

    /// Switch to global profile
    async fn switch_to_global_profile(&self) -> Result<()> {
        let profile_id = "global".to_string();

        info!("Switching to global profile");

        self.apply_profile(&profile_id).await?;

        // Update active profile
        {
            let mut active = self.active_profile.write().await;
            *active = Some(profile_id.clone());
        }

        info!(profile_id = %profile_id, "Successfully switched to global profile");
        Ok(())
    }

    /// Apply a profile using the profile service
    async fn apply_profile(&self, profile_id: &str) -> Result<()> {
        // Load the profile
        let _profile = self.profile_service.load_profile(profile_id).await?;

        // Note: Profile application requires device-specific information
        // This would need to be called with specific device context
        // For now, just log that the profile would be applied
        info!(profile_id = %profile_id, "Profile loaded for application");

        Ok(())
    }

    /// Set game-specific profile mapping
    pub async fn set_game_profile(&self, game_id: String, profile_id: String) -> Result<()> {
        let mut game_profiles = self.game_profiles.write().await;
        game_profiles.insert(game_id.clone(), profile_id.clone());

        info!(
            game_id = %game_id,
            profile_id = %profile_id,
            "Set game profile mapping"
        );

        Ok(())
    }

    /// Get game-specific profile mapping
    pub async fn get_game_profile(&self, game_id: &str) -> Option<String> {
        let game_profiles = self.game_profiles.read().await;
        game_profiles.get(game_id).cloned()
    }

    /// Get currently active profile
    pub async fn get_active_profile(&self) -> Option<String> {
        let active = self.active_profile.read().await;
        active.clone()
    }

    /// Get currently running games
    pub fn get_running_games(&self) -> Vec<String> {
        self.process_detection.get_running_games()
    }

    /// Get last switch performance metrics
    pub async fn get_switch_metrics(&self) -> Option<Duration> {
        let last_switch = self.last_switch_time.read().await;
        last_switch.map(|time| time.elapsed())
    }

    /// Force profile switch for testing
    pub async fn force_switch_to_profile(&self, profile_id: &str) -> Result<()> {
        info!(profile_id = %profile_id, "Force switching to profile");
        self.apply_profile(profile_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_service::ProfileService;
    use std::sync::Arc;

    async fn create_test_service() -> anyhow::Result<AutoProfileSwitchingService> {
        let profile_service = Arc::new(ProfileService::new().await?);
        AutoProfileSwitchingService::new(profile_service)
    }

    #[tokio::test]
    async fn test_service_creation() -> anyhow::Result<()> {
        let service = create_test_service().await?;
        assert_eq!(service.switch_timeout, Duration::from_millis(500));

        let active_profile = service.get_active_profile().await;
        assert!(active_profile.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_game_profile_mapping() -> anyhow::Result<()> {
        let service = create_test_service().await?;

        service
            .set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
            .await?;

        let profile = service.get_game_profile("iracing").await;
        assert_eq!(profile, Some("iracing_gt3".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_switch_timeout_requirement() -> anyhow::Result<()> {
        let service = create_test_service().await?;

        // Verify the timeout meets the ≤500ms requirement
        assert!(service.switch_timeout <= Duration::from_millis(500));
        Ok(())
    }

    #[tokio::test]
    async fn test_running_games_tracking() -> anyhow::Result<()> {
        let service = create_test_service().await?;

        // Initially no games should be running
        let running_games = service.get_running_games();
        assert!(running_games.is_empty());
        Ok(())
    }

    // ── Telemetry bridge integration tests ──────────────────────────────────

    use crate::game_telemetry_bridge::TelemetryAdapterControl;

    struct MockTelemetryControl {
        starts: Arc<tokio::sync::Mutex<Vec<String>>>,
        stops: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    impl MockTelemetryControl {
        fn new() -> Self {
            Self {
                starts: Arc::new(tokio::sync::Mutex::new(Vec::new())),
                stops: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            }
        }

        async fn started_games(&self) -> Vec<String> {
            self.starts.lock().await.clone()
        }

        async fn stopped_games(&self) -> Vec<String> {
            self.stops.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl TelemetryAdapterControl for MockTelemetryControl {
        async fn start_for_game(&self, game_id: &str) -> anyhow::Result<()> {
            self.starts.lock().await.push(game_id.to_string());
            Ok(())
        }

        async fn stop_for_game(&self, game_id: &str) -> anyhow::Result<()> {
            self.stops.lock().await.push(game_id.to_string());
            Ok(())
        }
    }

    fn make_process_info(name: &str, game_id: &str) -> crate::process_detection::ProcessInfo {
        crate::process_detection::ProcessInfo {
            pid: 1234,
            name: name.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: std::time::Instant::now(),
        }
    }

    #[tokio::test]
    async fn game_started_event_triggers_adapter_start() -> anyhow::Result<()> {
        let profile_service = Arc::new(ProfileService::new().await?);
        let mock = Arc::new(MockTelemetryControl::new());

        let service = AutoProfileSwitchingService::new(profile_service)?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        let event = ProcessEvent::GameStarted {
            game_id: "iracing".to_string(),
            process_info: make_process_info("iRacingSim64DX11.exe", "iracing"),
        };

        service.handle_event(event).await;

        let starts = mock.started_games().await;
        assert_eq!(starts, vec!["iracing"]);
        Ok(())
    }

    #[tokio::test]
    async fn game_stopped_event_triggers_adapter_stop() -> anyhow::Result<()> {
        let profile_service = Arc::new(ProfileService::new().await?);
        let mock = Arc::new(MockTelemetryControl::new());

        let service = AutoProfileSwitchingService::new(profile_service)?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        let event = ProcessEvent::GameStopped {
            game_id: "acc".to_string(),
            process_info: make_process_info("AC2-Win64-Shipping.exe", "acc"),
        };

        service.handle_event(event).await;

        let stops = mock.stopped_games().await;
        assert_eq!(stops, vec!["acc"]);
        Ok(())
    }

    #[tokio::test]
    async fn no_adapter_control_is_harmless() -> anyhow::Result<()> {
        // Without a control attached, events must not panic or return errors.
        let service = create_test_service().await?;

        let start_event = ProcessEvent::GameStarted {
            game_id: "iracing".to_string(),
            process_info: make_process_info("iRacingSim64DX11.exe", "iracing"),
        };
        service.handle_event(start_event).await;

        let stop_event = ProcessEvent::GameStopped {
            game_id: "iracing".to_string(),
            process_info: make_process_info("iRacingSim64DX11.exe", "iracing"),
        };
        service.handle_event(stop_event).await;

        Ok(())
    }
}
