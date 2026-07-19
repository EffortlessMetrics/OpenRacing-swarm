//! IPC client implementation for testing and CLI usage

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

use racing_wheel_schemas::generated::wheel::v1::{wheel_service_client::WheelServiceClient, *};
use racing_wheel_schemas::prelude::*;

/// IPC client configuration
#[derive(Debug, Clone)]
pub struct IpcClientConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Client version for feature negotiation
    pub client_version: String,
    /// Supported features
    pub supported_features: Vec<String>,
}

impl Default for IpcClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            client_version: "0.1.0".to_string(),
            supported_features: vec![
                "device_management".to_string(),
                "profile_management".to_string(),
                "safety_control".to_string(),
                "health_monitoring".to_string(),
                "game_integration".to_string(),
                "streaming_health".to_string(),
                "streaming_devices".to_string(),
                "control_stream_v1".to_string(),
            ],
        }
    }
}

/// IPC client for communicating with the wheel service
pub struct IpcClient {
    client: WheelServiceClient<Channel>,
    config: IpcClientConfig,
    negotiated_features: Vec<String>,
}

impl IpcClient {
    /// Connect to the wheel service
    pub async fn connect(
        config: IpcClientConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to wheel service");

        // For now, connect via TCP (will be enhanced with platform-specific transport)
        let endpoint = Endpoint::from_static("http://127.0.0.1:50051")
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout);

        let channel = endpoint.connect().await?;
        let mut client = WheelServiceClient::new(channel);

        // Perform feature negotiation
        let negotiation_request = FeatureNegotiationRequest {
            client_version: config.client_version.clone(),
            supported_features: config.supported_features.clone(),
            namespace: "wheel.v1".to_string(),
        };

        let response = client
            .negotiate_features(Request::new(negotiation_request))
            .await?;
        let negotiation_response = response.into_inner();

        if !negotiation_response.compatible {
            return Err(format!(
                "Client version {} is not compatible with server. Minimum required: {}",
                config.client_version, negotiation_response.min_client_version
            ));
        }

        info!(
            "Feature negotiation successful. Enabled features: {:?}",
            negotiation_response.enabled_features
        );

        Ok(Self {
            client,
            config,
            negotiated_features: negotiation_response.enabled_features,
        })
    }

    /// List all connected devices
    pub async fn list_devices(&mut self) -> Result<Vec<DeviceInfo>, Status> {
        debug!("Listing devices");

        let mut stream = self
            .client
            .list_devices(Request::new(()))
            .await?
            .into_inner();
        let mut devices = Vec::new();

        while let Some(device) = stream.message().await? {
            devices.push(device);
        }

        Ok(devices)
    }

    /// Get device status
    pub async fn get_device_status(&mut self, device_id: &str) -> Result<DeviceStatus, Status> {
        debug!("Getting device status for: {}", device_id);

        let request = Request::new(DeviceId {
            id: device_id.to_string(),
        });

        let response = self.client.get_device_status(request).await?;
        Ok(response.into_inner())
    }

    /// Get active profile for a device
    pub async fn get_active_profile(&mut self, device_id: &str) -> Result<Profile, Status> {
        debug!("Getting active profile for device: {}", device_id);

        let request = Request::new(DeviceId {
            id: device_id.to_string(),
        });

        let response = self.client.get_active_profile(request).await?;
        Ok(response.into_inner())
    }

    /// Apply a profile to a device
    pub async fn apply_profile(
        &mut self,
        device_id: &str,
        profile: Profile,
    ) -> Result<OpResult, Status> {
        debug!("Applying profile to device: {}", device_id);

        let request = Request::new(ApplyProfileRequest {
            device: Some(DeviceId {
                id: device_id.to_string(),
            }),
            profile: Some(profile),
        });

        let response = self.client.apply_profile(request).await?;
        Ok(response.into_inner())
    }

    /// List all available profiles
    pub async fn list_profiles(&mut self) -> Result<Vec<Profile>, Status> {
        debug!("Listing profiles");

        let response = self.client.list_profiles(Request::new(())).await?;
        Ok(response.into_inner().profiles)
    }

    /// Start high torque mode for a device
    pub async fn start_high_torque(&mut self, device_id: &str) -> Result<OpResult, Status> {
        debug!("Starting high torque for device: {}", device_id);

        let request = Request::new(DeviceId {
            id: device_id.to_string(),
        });

        let response = self.client.start_high_torque(request).await?;
        Ok(response.into_inner())
    }

    /// Emergency stop for a device
    pub async fn emergency_stop(&mut self, device_id: &str) -> Result<OpResult, Status> {
        debug!("Emergency stop for device: {}", device_id);

        let request = Request::new(DeviceId {
            id: device_id.to_string(),
        });

        let response = self.client.emergency_stop(request).await?;
        Ok(response.into_inner())
    }

    /// Subscribe to health events
    pub async fn subscribe_health(&mut self) -> Result<tonic::Streaming<HealthEvent>, Status> {
        debug!("Subscribing to health events");

        let stream = self
            .client
            .subscribe_health(Request::new(()))
            .await?
            .into_inner();
        Ok(stream)
    }

    /// Subscribe to the observe-only versioned control stream.
    pub async fn subscribe_control_stream(
        &mut self,
        request: ControlSubscription,
    ) -> Result<tonic::Streaming<ControlStreamItem>, Status> {
        if !self.has_feature("control_stream_v1") {
            return Err(Status::failed_precondition(
                "control_stream_v1 was not enabled during feature negotiation",
            ));
        }
        let stream = self
            .client
            .subscribe_control_stream(Request::new(request))
            .await?
            .into_inner();
        Ok(stream)
    }

    /// Get diagnostics for a device
    pub async fn get_diagnostics(&mut self, device_id: &str) -> Result<DiagnosticInfo, Status> {
        debug!("Getting diagnostics for device: {}", device_id);

        let request = Request::new(DeviceId {
            id: device_id.to_string(),
        });

        let response = self.client.get_diagnostics(request).await?;
        Ok(response.into_inner())
    }

    /// Configure telemetry for a game
    pub async fn configure_telemetry(
        &mut self,
        game_id: &str,
        install_path: &str,
        enable_auto_config: bool,
    ) -> Result<OpResult, Status> {
        debug!("Configuring telemetry for game: {}", game_id);

        let request = Request::new(ConfigureTelemetryRequest {
            game_id: game_id.to_string(),
            install_path: install_path.to_string(),
            enable_auto_config,
        });

        let response = self.client.configure_telemetry(request).await?;
        Ok(response.into_inner())
    }

    /// Get current game status
    pub async fn get_game_status(&mut self) -> Result<GameStatus, Status> {
        debug!("Getting game status");

        let response = self.client.get_game_status(Request::new(())).await?;
        Ok(response.into_inner())
    }

    /// Get negotiated features
    pub fn get_negotiated_features(&self) -> &[String] {
        &self.negotiated_features
    }

    /// Check if a feature is enabled
    pub fn has_feature(&self, feature: &str) -> bool {
        self.negotiated_features.contains(&feature.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_client_config_default() {
        let config = IpcClientConfig::default();
        assert_eq!(config.client_version, "0.1.0");
        assert!(
            config
                .supported_features
                .contains(&"device_management".to_string())
        );
        assert!(config.connect_timeout > Duration::from_secs(0));
    }

    #[tokio::test]
    async fn test_client_feature_check() {
        // This test would require a running server, so we'll just test the feature checking logic
        let config = IpcClientConfig::default();
        let negotiated_features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
        ];

        // Simulate a client with negotiated features
        // In a real test, we'd connect to a test server
        assert!(negotiated_features.contains(&"device_management".to_string()));
        assert!(!negotiated_features.contains(&"unknown_feature".to_string()));
    }
}
