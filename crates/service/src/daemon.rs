//! Service daemon implementation with platform-specific service management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

use crate::{
    IpcConfig, TransportType, WheelService, game_service::GameService,
    ipc_service::WheelServiceImpl, profile_repository::ProfileRepositoryConfig,
};
use racing_wheel_schemas::generated::wheel::v1::wheel_service_server::WheelServiceServer;
use tonic::transport::Server;

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service name
    pub service_name: String,
    /// Service display name
    pub service_display_name: String,
    /// Service description
    pub service_description: String,
    /// IPC configuration
    pub ipc: IpcConfig,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// Maximum restart attempts
    pub max_restart_attempts: u32,
    /// Restart delay in seconds
    pub restart_delay: u64,
    /// Enable automatic restart on failure
    pub auto_restart: bool,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: "wheeld".to_string(),
            service_display_name: "Racing Wheel Service".to_string(),
            service_description: "Racing wheel hardware management and force feedback service"
                .to_string(),
            ipc: IpcConfig {
                transport: TransportType::default(),
                bind_address: None,
                max_connections: 10,
                connection_timeout: Duration::from_secs(30),
                enable_acl: true,
            },
            health_check_interval: 30,
            max_restart_attempts: 3,
            restart_delay: 5,
            auto_restart: true,
        }
    }
}

impl ServiceConfig {
    /// Load configuration from file or create default
    pub async fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .context("Failed to read config file")?;

            let config: ServiceConfig =
                serde_json::from_str(&content).context("Failed to parse config file")?;

            debug!("Loaded config from {:?}", config_path);
            Ok(config)
        } else {
            let config = Self::default();
            config.save().await?;
            info!("Created default config at {:?}", config_path);
            Ok(config)
        }
    }

    /// Save configuration to file
    pub async fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create config directory")?;
        }

        let content = serde_json::to_string_pretty(self).context("Failed to serialize config")?;

        tokio::fs::write(&config_path, content)
            .await
            .context("Failed to write config file")?;

        debug!("Saved config to {:?}", config_path);
        Ok(())
    }

    /// Get configuration file path
    fn config_path() -> Result<PathBuf> {
        let config_dir = if cfg!(windows) {
            std::env::var("LOCALAPPDATA").context("LOCALAPPDATA environment variable not set")?
        } else {
            format!(
                "{}/.config",
                std::env::var("HOME").context("HOME environment variable not set")?
            )
        };

        Ok(PathBuf::from(config_dir).join("wheel").join("service.json"))
    }
}

/// Service daemon that manages the wheel service lifecycle
pub struct ServiceDaemon {
    config: ServiceConfig,
    flags: crate::FeatureFlags,
    hardware_lane: Option<String>,
    shutdown_tx: broadcast::Sender<()>,
    is_running: Arc<AtomicBool>,
    restart_count: Arc<std::sync::atomic::AtomicU32>,
    profile_config: ProfileRepositoryConfig,
}

impl ServiceDaemon {
    /// Create new service daemon
    pub async fn new(config: ServiceConfig) -> Result<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            config,
            flags: crate::FeatureFlags::default(),
            hardware_lane: None,
            shutdown_tx,
            is_running: Arc::new(AtomicBool::new(false)),
            restart_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            profile_config: ProfileRepositoryConfig::default(),
        })
    }

    /// Create new service daemon with feature flags
    pub async fn new_with_flags(config: ServiceConfig, flags: crate::FeatureFlags) -> Result<Self> {
        let mut daemon = Self::new(config).await?;
        daemon.flags = flags;
        Ok(daemon)
    }

    /// Create new service daemon with feature flags and a hardware validation lane.
    pub async fn new_with_flags_and_hardware_lane(
        config: ServiceConfig,
        flags: crate::FeatureFlags,
        hardware_lane: Option<String>,
    ) -> Result<Self> {
        let mut daemon = Self::new_with_flags(config, flags).await?;
        daemon.hardware_lane = hardware_lane;
        Ok(daemon)
    }

    /// Create new service daemon with explicit profile repository config (tests/dev)
    pub async fn new_with_profile_config(
        config: ServiceConfig,
        profile_config: ProfileRepositoryConfig,
    ) -> Result<Self> {
        let mut daemon = Self::new(config).await?;
        daemon.profile_config = profile_config;
        Ok(daemon)
    }

    /// Create new service daemon with feature flags and explicit profile repository config (tests/dev)
    pub async fn new_with_flags_and_profile_config(
        config: ServiceConfig,
        flags: crate::FeatureFlags,
        profile_config: ProfileRepositoryConfig,
    ) -> Result<Self> {
        let mut daemon = Self::new_with_flags(config, flags).await?;
        daemon.profile_config = profile_config;
        Ok(daemon)
    }

    /// Run the service daemon
    pub async fn run(self) -> Result<()> {
        info!("Starting service daemon");

        // Set up signal handling
        let shutdown_tx = self.shutdown_tx.clone();
        let is_running = self.is_running.clone();

        tokio::spawn(async move {
            Self::setup_signal_handlers(shutdown_tx, is_running).await;
        });

        // Main service loop with restart capability
        let mut restart_count = 0;

        loop {
            self.is_running.store(true, Ordering::SeqCst);

            match self.run_service_instance().await {
                Ok(()) => {
                    info!("Service stopped normally");
                    break;
                }
                Err(e) => {
                    error!("Service error: {}", e);

                    if !self.config.auto_restart {
                        error!("Auto-restart disabled, exiting");
                        return Err(e);
                    }

                    restart_count += 1;
                    self.restart_count.store(restart_count, Ordering::SeqCst);

                    if restart_count >= self.config.max_restart_attempts {
                        error!(
                            "Maximum restart attempts ({}) exceeded, exiting",
                            self.config.max_restart_attempts
                        );
                        return Err(e);
                    }

                    warn!(
                        "Restarting service in {} seconds (attempt {}/{})",
                        self.config.restart_delay, restart_count, self.config.max_restart_attempts
                    );

                    tokio::time::sleep(Duration::from_secs(self.config.restart_delay)).await;
                }
            }
        }

        Ok(())
    }

    /// Run a single service instance
    async fn run_service_instance(&self) -> Result<()> {
        info!("Starting wheel service instance");

        // Create wheel service
        let wheel_service = Arc::new(
            WheelService::new_with_flags(self.flags.clone(), self.profile_config.clone())
                .await
                .context("Failed to create wheel service")?,
        );

        info!("Wheel service created successfully");

        // Create game service for gRPC layer
        let game_service = Arc::new(
            GameService::new()
                .await
                .context("Failed to create game service")?,
        );

        // Create gRPC service implementation backed by real domain services
        let (health_tx, _) = broadcast::channel(1000);
        let grpc_service = WheelServiceImpl::new(
            wheel_service.device_service().clone(),
            wheel_service.profile_service().clone(),
            wheel_service.safety_service().clone(),
            game_service,
            health_tx,
        )
        .with_hardware_lane(self.hardware_lane.clone());

        // Start gRPC IPC server
        let grpc_addr = "127.0.0.1:50051";
        let addr = grpc_addr
            .parse()
            .context("Failed to parse gRPC bind address")?;
        info!("Starting gRPC IPC server on {}", grpc_addr);
        let grpc_handle = tokio::spawn(async move {
            if let Err(e) = Server::builder()
                .add_service(WheelServiceServer::new(grpc_service))
                .serve(addr)
                .await
            {
                error!("gRPC server error: {}", e);
            }
        });

        // Start health monitoring
        let health_handle = {
            let service = wheel_service.clone();
            let interval_secs = self.config.health_check_interval;
            let is_running = self.is_running.clone();

            tokio::spawn(async move {
                Self::health_monitor(service, interval_secs, is_running).await;
            })
        };

        // Wait for shutdown signal
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Clone the service for the run method since it takes ownership
        let service_for_run = (*wheel_service).clone();

        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Received shutdown signal");
            }
            result = service_for_run.run() => {
                if let Err(e) = result {
                    error!("Wheel service error: {}", e);
                    return Err(e);
                }
            }
        }

        // Graceful shutdown
        info!("Shutting down service instance");
        self.is_running.store(false, Ordering::SeqCst);

        // Abort gRPC server and wait for tasks to complete
        grpc_handle.abort();
        let _ = tokio::join!(grpc_handle, health_handle);

        info!("Service instance stopped");
        Ok(())
    }

    /// Set up platform-specific signal handlers
    async fn setup_signal_handlers(
        shutdown_tx: broadcast::Sender<()>,
        is_running: Arc<AtomicBool>,
    ) {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(e) => {
                    error!(error = %e, "Failed to register SIGTERM handler");
                    is_running.store(false, Ordering::SeqCst);
                    let _ = shutdown_tx.send(());
                    return;
                }
            };
            let mut sigint = match signal(SignalKind::interrupt()) {
                Ok(signal) => signal,
                Err(e) => {
                    error!(error = %e, "Failed to register SIGINT handler");
                    is_running.store(false, Ordering::SeqCst);
                    let _ = shutdown_tx.send(());
                    return;
                }
            };
            let mut sighup = match signal(SignalKind::hangup()) {
                Ok(signal) => signal,
                Err(e) => {
                    error!(error = %e, "Failed to register SIGHUP handler");
                    is_running.store(false, Ordering::SeqCst);
                    let _ = shutdown_tx.send(());
                    return;
                }
            };

            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT");
                }
                _ = sighup.recv() => {
                    info!("Received SIGHUP");
                }
            }
        }

        #[cfg(windows)]
        {
            if let Err(e) = tokio::signal::ctrl_c().await {
                error!("Error waiting for Ctrl+C: {}", e);
                return;
            }
            info!("Received Ctrl+C");
        }

        is_running.store(false, Ordering::SeqCst);
        let _ = shutdown_tx.send(());
    }

    /// Health monitoring task
    async fn health_monitor(
        service: Arc<WheelService>,
        interval_secs: u64,
        is_running: Arc<AtomicBool>,
    ) {
        let mut interval = interval(Duration::from_secs(interval_secs));

        while is_running.load(Ordering::SeqCst) {
            interval.tick().await;

            // Perform health checks
            match Self::check_service_health(&service).await {
                Ok(healthy) => {
                    if healthy {
                        debug!("Service health check passed");
                    } else {
                        warn!("Service health check failed");
                    }
                }
                Err(e) => {
                    error!("Health check error: {}", e);
                }
            }
        }

        debug!("Health monitor stopped");
    }

    /// Check service health
    async fn check_service_health(service: &WheelService) -> Result<bool> {
        // Check if services are responsive
        let profile_stats = service.profile_service().get_profile_statistics().await?;
        let device_stats = service.device_service().get_statistics().await;
        let safety_stats = service.safety_service().get_statistics().await;

        // Log health metrics
        debug!(
            profiles_total = profile_stats.total_profiles,
            profiles_active = profile_stats.active_profiles,
            devices_total = device_stats.total_devices,
            devices_connected = device_stats.connected_devices,
            safety_devices = safety_stats.total_devices,
            "Service health metrics"
        );

        // Service is healthy if all components are responsive
        Ok(true)
    }

    /// Install service (platform-specific)
    pub async fn install() -> Result<()> {
        #[cfg(windows)]
        {
            Self::install_windows_service().await
        }

        #[cfg(unix)]
        {
            Self::install_unix_service().await
        }
    }

    /// Uninstall service (platform-specific)
    pub async fn uninstall() -> Result<()> {
        #[cfg(windows)]
        {
            Self::uninstall_windows_service().await
        }

        #[cfg(unix)]
        {
            Self::uninstall_unix_service().await
        }
    }

    /// Get service status (platform-specific)
    pub async fn status() -> Result<String> {
        #[cfg(windows)]
        {
            Self::status_windows_service().await
        }

        #[cfg(unix)]
        {
            Self::status_unix_service().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_new_with_flags() -> anyhow::Result<()> {
        let config = ServiceConfig::default();
        let flags = crate::FeatureFlags {
            enable_virtual_devices: true,
            ..Default::default()
        };

        let daemon = ServiceDaemon::new_with_flags(config, flags.clone()).await?;
        assert!(daemon.flags.enable_virtual_devices);

        // Also test new_with_flags_and_profile_config
        let profile_config = ProfileRepositoryConfig::default();
        let daemon2 = ServiceDaemon::new_with_flags_and_profile_config(
            ServiceConfig::default(),
            flags,
            profile_config,
        )
        .await?;
        assert!(daemon2.flags.enable_virtual_devices);

        Ok(())
    }

    #[tokio::test]
    async fn test_daemon_new_with_flags_and_hardware_lane() -> anyhow::Result<()> {
        let config = ServiceConfig::default();
        let flags = crate::FeatureFlags {
            enable_virtual_devices: true,
            ..Default::default()
        };

        let daemon = ServiceDaemon::new_with_flags_and_hardware_lane(
            config,
            flags,
            Some("ci/hardware/moza-r5/2026-05-06".to_string()),
        )
        .await?;

        assert!(daemon.flags.enable_virtual_devices);
        assert_eq!(
            daemon.hardware_lane.as_deref(),
            Some("ci/hardware/moza-r5/2026-05-06")
        );
        Ok(())
    }
}
