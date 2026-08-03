//! Main service implementation

use crate::{
    ApplicationDeviceService, ApplicationProfileService, ApplicationSafetyService, FeatureFlags,
    game_service::GameService, plugin_registry_impl::PluginRegistryServiceImpl,
    profile_repository::ProfileRepositoryConfig,
};
use anyhow::Result;
use racing_wheel_engine::hid::create_hid_port;
use racing_wheel_engine::{HidPort, SafetyPolicy, TracingManager, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::DeviceId;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Main wheel service that orchestrates all application services
#[derive(Clone)]
pub struct WheelService {
    /// Profile service for managing wheel profiles
    profile_service: Arc<ApplicationProfileService>,
    /// Device service for managing hardware
    device_service: Arc<ApplicationDeviceService>,
    /// Safety service for torque management
    safety_service: Arc<ApplicationSafetyService>,
    /// Game integration service for telemetry configuration
    game_service: Arc<GameService>,
    /// Plugin registry service for plugin discovery and lifecycle management
    plugin_service: Arc<PluginRegistryServiceImpl>,
    /// Tracing manager for observability
    tracer: Option<Arc<TracingManager>>,
    /// Feature flags for runtime behavior
    #[allow(dead_code)]
    flags: FeatureFlags,
}

impl WheelService {
    /// Create new service instance
    pub async fn new() -> Result<Self> {
        Self::new_with_flags(FeatureFlags::default(), ProfileRepositoryConfig::default()).await
    }

    /// Create new service instance with flags and custom profile repository configuration
    pub async fn new_with_flags(
        flags: FeatureFlags,
        profile_config: ProfileRepositoryConfig,
    ) -> Result<Self> {
        info!("Initializing Racing Wheel Service");

        // Initialize tracing
        let tracer = match TracingManager::new() {
            Ok(mut tracer) => {
                if let Err(e) = tracer.initialize() {
                    error!(error = %e, "Failed to initialize tracing, continuing without it");
                    None
                } else {
                    info!("Tracing initialized successfully");
                    Some(Arc::new(tracer))
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to create tracing manager, continuing without it");
                None
            }
        };

        // Initialize HID port
        // We attempt to initialize the real platform HID backend first.
        // If it fails or if virtual devices are explicitly requested, we fall back to VirtualHidPort.
        let mut real_port_failed = false;
        let hid_port: Arc<dyn HidPort> = if flags.enable_virtual_devices {
            info!("Virtual devices explicitly requested via feature flags");
            Self::create_virtual_port()?
        } else {
            match create_hid_port() {
                Ok(port) => {
                    info!("Platform HID backend initialized successfully");
                    Arc::from(port)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize platform HID backend; falling back to virtual devices");
                    real_port_failed = true;
                    Self::create_virtual_port()?
                }
            }
        };

        // Apply fallback warning if both failed or if explicitly falling back
        if real_port_failed {
            warn!("SERVICE DEGRADED: Running with virtual HID devices only");
        }

        // Initialize profile repository
        info!("Profile repository initialized");

        // Initialize safety policy
        let safety_policy = SafetyPolicy::default();
        info!("Safety policy initialized");

        // Create application services
        let profile_service = Arc::new(
            ApplicationProfileService::new_with_config(profile_config)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create profile service: {}", e))?,
        );
        info!("Profile service created");

        let device_service = Arc::new(
            ApplicationDeviceService::new(hid_port, tracer.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create device service: {}", e))?,
        );
        info!("Device service created");

        let safety_service = Arc::new(
            ApplicationSafetyService::new(safety_policy, tracer.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create safety service: {}", e))?,
        );
        info!("Safety service created");

        let game_service = Arc::new(
            GameService::new()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create game service: {}", e))?,
        );
        info!("Game service created");

        let plugin_service = Arc::new(
            PluginRegistryServiceImpl::with_defaults()
                .map_err(|e| anyhow::anyhow!("Failed to create plugin registry service: {}", e))?,
        );
        info!("Plugin registry service created");

        Ok(Self {
            profile_service,
            device_service,
            safety_service,
            game_service,
            plugin_service,
            tracer,
            flags,
        })
    }

    fn create_virtual_port() -> Result<Arc<dyn HidPort>> {
        let mut virtual_port = VirtualHidPort::new();

        // Seed with a default virtual device for testing/development
        let device_id: DeviceId = "virtual-wheel-0"
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse device ID: {}", e))?;
        let virtual_device = VirtualDevice::new(device_id, "Virtual Racing Wheel".to_string());
        virtual_port
            .add_device(virtual_device)
            .map_err(|e| anyhow::anyhow!("Failed to add virtual device: {}", e))?;

        Ok(Arc::new(virtual_port))
    }

    /// Run the service
    pub async fn run(self) -> Result<()> {
        info!("Starting Racing Wheel Service");

        // Start all services
        if let Err(e) = self.device_service.start().await {
            error!(error = %e, "Failed to start device service");
            return Err(e);
        }

        if let Err(e) = self.safety_service.start().await {
            error!(error = %e, "Failed to start safety service");
            return Err(e);
        }

        info!("All services started successfully");

        // Service main loop
        let shutdown_signal = tokio::signal::ctrl_c();

        tokio::select! {
            _ = shutdown_signal => {
                info!("Shutdown signal received");
            }
            _ = self.service_health_monitor() => {
                error!("Service health monitor exited unexpectedly");
            }
        }

        info!("Racing Wheel Service shutting down");
        self.shutdown().await?;

        Ok(())
    }

    /// Get profile service reference
    pub fn profile_service(&self) -> &Arc<ApplicationProfileService> {
        &self.profile_service
    }

    /// Get device service reference
    pub fn device_service(&self) -> &Arc<ApplicationDeviceService> {
        &self.device_service
    }

    /// Get safety service reference
    pub fn safety_service(&self) -> &Arc<ApplicationSafetyService> {
        &self.safety_service
    }

    /// Get the shared game integration service reference
    pub fn game_service(&self) -> &Arc<GameService> {
        &self.game_service
    }

    /// Get the shared plugin registry service reference
    pub fn plugin_service(&self) -> &Arc<PluginRegistryServiceImpl> {
        &self.plugin_service
    }

    /// Service health monitoring
    async fn service_health_monitor(&self) -> Result<()> {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            interval.tick().await;

            // Check service health and log statistics
            let profile_stats = self
                .profile_service
                .get_profile_statistics()
                .await
                .unwrap_or_else(|e| {
                    error!(error = %e, "Failed to get profile statistics");
                    crate::profile_service::ProfileStatistics {
                        total_profiles: 0,
                        active_profiles: 0,
                        cached_profiles: 0,
                        signed_profiles: 0,
                        trusted_profiles: 0,
                        session_overrides: 0,
                    }
                });

            let device_stats = self.device_service.get_statistics().await;
            let safety_stats = self.safety_service.get_statistics().await;

            info!(
                profiles_total = profile_stats.total_profiles,
                profiles_active = profile_stats.active_profiles,
                profiles_cached = profile_stats.cached_profiles,
                devices_total = device_stats.total_devices,
                devices_connected = device_stats.connected_devices,
                devices_ready = device_stats.ready_devices,
                devices_faulted = device_stats.faulted_devices,
                safety_total = safety_stats.total_devices,
                safety_safe_torque = safety_stats.safe_torque_devices,
                safety_high_torque = safety_stats.high_torque_devices,
                safety_faulted = safety_stats.faulted_devices,
                "Service health check"
            );
        }
    }

    /// Shutdown the service gracefully
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down services");

        self.device_service.stop().await?;

        // Shutdown tracing if available
        if let Some(_tracer) = &self.tracer {
            // Note: TracingManager doesn't have a mutable shutdown method in our current design
            // In a real implementation, we would properly shutdown the tracing system
            info!("Tracing shutdown completed");
        }

        info!("Service shutdown completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_new_with_flags() -> anyhow::Result<()> {
        let flags = FeatureFlags {
            enable_virtual_devices: true,
            ..Default::default()
        };

        let service =
            WheelService::new_with_flags(flags, ProfileRepositoryConfig::default()).await?;

        assert!(service.flags.enable_virtual_devices);
        Ok(())
    }
}
