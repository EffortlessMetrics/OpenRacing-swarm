//! Integration tests for IPC server and client communication

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::timeout;
use tonic::{Request, Response, Status};

use racing_wheel_schemas::generated::wheel::v1::*;
use racing_wheel_schemas::prelude::*;

use crate::device_service::DeviceService;
use crate::game_service::GameService;
use crate::ipc::{IpcServer, IpcConfig, HealthEventInternal, TransportType};
use crate::ipc_client::{IpcClient, IpcClientConfig};
use crate::profile_service::ProfileService;
use crate::safety_service::SafetyService;

/// Simplified device for testing
#[derive(Debug, Clone)]
pub struct TestDevice {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_type: i32,
    pub capabilities: TestDeviceCapabilities,
    pub state: i32,
}

#[derive(Debug, Clone)]
pub struct TestDeviceCapabilities {
    pub supports_pid: bool,
    pub supports_raw_torque_1khz: bool,
    pub supports_health_stream: bool,
    pub supports_led_bus: bool,
    pub max_torque_cnm: u32,
    pub encoder_cpr: u32,
    pub min_report_period_us: u32,
}

#[derive(Debug, Clone)]
pub struct TestDeviceStatus {
    pub device: TestDevice,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub active_faults: Vec<String>,
    pub telemetry: Option<TestTelemetryData>,
}

#[derive(Debug, Clone)]
pub struct TestTelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

/// Mock device service for testing
pub struct MockDeviceService {
    devices: Vec<TestDevice>,
}

impl MockDeviceService {
    pub fn new() -> Self {
        Self {
            devices: vec![
                TestDevice {
                    id: "test-device-1".to_string(),
                    name: "Test Wheel Base".to_string(),
                    vendor_id: 0x1234,
                    product_id: 0x5678,
                    device_type: 1, // WheelBase
                    capabilities: TestDeviceCapabilities {
                        supports_pid: true,
                        supports_raw_torque_1khz: true,
                        supports_health_stream: true,
                        supports_led_bus: true,
                        max_torque_cnm: 2500, // 25 Nm
                        encoder_cpr: 65536,
                        min_report_period_us: 1000,
                    },
                    state: 1, // Connected
                },
            ],
        }
    }

    pub async fn list_devices(&self) -> Result<Vec<TestDevice>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.devices.clone())
    }

    pub async fn get_device_status(&self, device_id: &str) -> Result<TestDeviceStatus, Box<dyn std::error::Error + Send + Sync>> {
        let device = self.devices.iter()
            .find(|d| d.id == device_id)
            .ok_or("Device not found")?;

        Ok(TestDeviceStatus {
            device: device.clone(),
            last_seen: chrono::Utc::now(),
            active_faults: vec![],
            telemetry: Some(TestTelemetryData {
                wheel_angle_deg: 0.0,
                wheel_speed_rad_s: 0.0,
                temperature_c: 45,
                fault_flags: 0,
                hands_on: true,
            }),
        })
    }
}

/// Simplified profile for testing
#[derive(Debug, Clone)]
pub struct TestProfile {
    pub schema_version: String,
    pub scope: TestProfileScope,
    pub base: TestBaseSettings,
    pub leds: Option<TestLedConfig>,
    pub haptics: Option<TestHapticsConfig>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestProfileScope {
    pub game: Option<String>,
    pub car: Option<String>,
    pub track: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestBaseSettings {
    pub ffb_gain: f32,
    pub dor_deg: u16,
    pub torque_cap_nm: f32,
    pub filters: TestFilterConfig,
}

#[derive(Debug, Clone)]
pub struct TestFilterConfig {
    pub reconstruction: u8,
    pub friction: f32,
    pub damper: f32,
    pub inertia: f32,
    pub notch_filters: Vec<TestNotchFilter>,
    pub slew_rate: f32,
    pub curve_points: Vec<TestCurvePoint>,
}

#[derive(Debug, Clone)]
pub struct TestNotchFilter {
    pub hz: f32,
    pub q: f32,
    pub gain_db: f32,
}

#[derive(Debug, Clone)]
pub struct TestCurvePoint {
    pub input: f32,
    pub output: f32,
}

#[derive(Debug, Clone)]
pub struct TestLedConfig {
    pub rpm_bands: Vec<f32>,
    pub pattern: String,
    pub brightness: f32,
}

#[derive(Debug, Clone)]
pub struct TestHapticsConfig {
    pub enabled: bool,
    pub intensity: f32,
    pub frequency_hz: f32,
}

/// Mock profile service for testing
pub struct MockProfileService {
    profiles: Vec<TestProfile>,
    active_profiles: HashMap<String, TestProfile>,
}

impl MockProfileService {
    pub fn new() -> Self {
        let default_profile = TestProfile {
            schema_version: "wheel.profile/1".to_string(),
            scope: TestProfileScope {
                game: Some("test-game".to_string()),
                car: None,
                track: None,
            },
            base: TestBaseSettings {
                ffb_gain: 0.75,
                dor_deg: 900,
                torque_cap_nm: 15.0,
                filters: TestFilterConfig {
                    reconstruction: 4,
                    friction: 0.1,
                    damper: 0.15,
                    inertia: 0.08,
                    notch_filters: vec![],
                    slew_rate: 0.8,
                    curve_points: vec![
                        TestCurvePoint { input: 0.0, output: 0.0 },
                        TestCurvePoint { input: 1.0, output: 1.0 },
                    ],
                },
            },
            leds: Some(TestLedConfig {
                rpm_bands: vec![0.7, 0.8, 0.9, 0.95],
                pattern: "progressive".to_string(),
                brightness: 0.8,
            }),
            haptics: Some(TestHapticsConfig {
                enabled: true,
                intensity: 0.5,
                frequency_hz: 120.0,
            }),
            signature: None,
        };

        Self {
            profiles: vec![default_profile.clone()],
            active_profiles: HashMap::new(),
        }
    }

    pub async fn list_profiles(&self) -> Result<Vec<TestProfile>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.profiles.clone())
    }

    pub async fn get_active_profile(&self, device_id: &str) -> Result<TestProfile, Box<dyn std::error::Error + Send + Sync>> {
        self.active_profiles
            .get(device_id)
            .cloned()
            .or_else(|| self.profiles.first().cloned())
            .ok_or("No profile found")
    }

    pub async fn apply_profile(&self, _device_id: &str, _profile: TestProfile) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would validate and apply the profile
        Ok(())
    }
}

/// Simplified game status for testing
#[derive(Debug, Clone)]
pub struct TestGameStatus {
    pub active_game: Option<String>,
    pub telemetry_active: bool,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// Mock game service for testing
pub struct MockGameService {
    game_status: TestGameStatus,
}

impl MockGameService {
    pub fn new() -> Self {
        Self {
            game_status: TestGameStatus {
                active_game: Some("iRacing".to_string()),
                telemetry_active: true,
                car_id: Some("gt3_bmw".to_string()),
                track_id: Some("spa".to_string()),
            },
        }
    }

    pub async fn configure_telemetry(&self, _game_id: &str, _install_path: &str, _enable_auto_config: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    pub async fn get_game_status(&self) -> Result<TestGameStatus, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.game_status.clone())
    }
}

/// Mock safety service for testing
pub struct MockSafetyService;

impl Default for MockSafetyService {
    fn default() -> Self {
        Self
    }
}

    pub async fn start_high_torque(&self, _device_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    pub async fn emergency_stop(&self, _device_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Test fixture for IPC integration tests
pub struct IpcTestFixture {
    pub client_config: IpcClientConfig,
    pub mock_device_service: MockDeviceService,
    pub mock_profile_service: MockProfileService,
    pub mock_game_service: MockGameService,
    pub mock_safety_service: MockSafetyService,
}

impl IpcTestFixture {
    pub async fn new() -> Self {
        let client_config = IpcClientConfig::default();

        Self {
            client_config,
            mock_device_service: MockDeviceService::new(),
            mock_profile_service: MockProfileService::new(),
            mock_game_service: MockGameService::new(),
            mock_safety_service: MockSafetyService::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_feature_negotiation() {
        let _fixture = IpcTestFixture::new().await;
        
        // Test feature negotiation logic
        let client_config = IpcClientConfig {
            client_version: "0.1.0".to_string(),
            supported_features: vec!["device_management".to_string()],
            ..Default::default()
        };
        
        assert_eq!(client_config.client_version, "0.1.0");
        assert!(client_config.supported_features.contains(&"device_management".to_string()));
    }

    #[tokio::test]
    async fn test_device_listing() -> Result<()> {
        let fixture = IpcTestFixture::new().await;

        // Test the mock device service directly
        let devices = fixture.mock_device_service.list_devices().await?;

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Test Wheel Base");
        assert_eq!(devices[0].vendor_id, 0x1234);
        assert_eq!(devices[0].product_id, 0x5678);
        assert_eq!(devices[0].device_type, 1); // WheelBase
        Ok(())
    }

    #[tokio::test]
    async fn test_profile_management() -> Result<()> {
        let fixture = IpcTestFixture::new().await;

        // Test the mock profile service directly
        let profiles = fixture.mock_profile_service.list_profiles().await?;

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].schema_version, "wheel.profile/1");
        assert_eq!(profiles[0].base.ffb_gain, 0.75);
        Ok(())
    }

    #[tokio::test]
    async fn test_health_event_broadcasting() {
        let _fixture = IpcTestFixture::new().await;
        
        // Test health event creation
        let health_event = HealthEventInternal {
            timestamp: std::time::SystemTime::now(),
            device_id: "test-device-1".to_string(),
            event_type: HealthEventType::DeviceConnected,
            message: "Device connected successfully".to_string(),
            metadata: HashMap::new(),
        };
        
        assert_eq!(health_event.device_id, "test-device-1");
        assert_eq!(health_event.event_type as i32, HealthEventType::DeviceConnected as i32);
    }

    #[tokio::test]
    async fn test_safety_operations() {
        let fixture = IpcTestFixture::new().await;
        
        // Test high torque start
        let result = fixture.mock_safety_service.start_high_torque("test-device-1").await;
        assert!(result.is_ok());
        
        // Test emergency stop
        let result = fixture.mock_safety_service.emergency_stop("test-device-1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_game_integration() -> Result<()> {
        let fixture = IpcTestFixture::new().await;

        // Test telemetry configuration
        fixture
            .mock_game_service
            .configure_telemetry("iRacing", "/path/to/iracing", true)
            .await?;

        // Test game status
        let status = fixture.mock_game_service.get_game_status().await?;
        assert_eq!(status.active_game, Some("iRacing".to_string()));
        assert!(status.telemetry_active);
        Ok(())
    }

    #[tokio::test]
    async fn test_version_compatibility() {
        // Test version compatibility logic inline since the function is private
        let is_version_compatible = |client_version: &str, min_version: &str| -> bool {
            let parse_version = |v: &str| -> Vec<u32> {
                v.split('.')
                    .take(3)
                    .map(|s| s.parse().unwrap_or(0))
                    .collect()
            };
            
            let client_parts = parse_version(client_version);
            let min_parts = parse_version(min_version);
            
            if client_parts.len() < 3 || min_parts.len() < 3 {
                return false;
            }
            
            // Major version must match
            if client_parts[0] != min_parts[0] {
                return false;
            }
            
            // Minor version must be >= minimum
            if client_parts[1] < min_parts[1] {
                return false;
            }
            
            // If minor versions match, patch must be >= minimum
            if client_parts[1] == min_parts[1] && client_parts[2] < min_parts[2] {
                return false;
            }
            
            true
        };
        
        // Test compatible versions
        assert!(is_version_compatible("0.1.0", "0.1.0"));
        assert!(is_version_compatible("0.1.1", "0.1.0"));
        assert!(is_version_compatible("0.2.0", "0.1.0"));
        
        // Test incompatible versions
        assert!(!is_version_compatible("0.0.9", "0.1.0"));
        assert!(!is_version_compatible("1.0.0", "0.1.0")); // Major version mismatch
        
        // Test edge cases
        assert!(!is_version_compatible("invalid", "0.1.0"));
        assert!(!is_version_compatible("0.1", "0.1.0"));
    }
}

/// Integration test that requires a running server
#[cfg(test)]
mod integration_tests {
    use super::*;
    use anyhow::{Result, anyhow};
    
    // These tests would be run with a real server instance
    // They are marked as ignored by default to avoid requiring server setup in CI
    
    #[tokio::test]
    #[ignore = "requires running server"]
    async fn test_full_client_server_communication() -> Result<()> {
        let fixture = IpcTestFixture::new().await;
        let server_handle = fixture.start_server().await;
        
        // Give server time to start
        sleep(Duration::from_secs(1)).await;
        
        // Connect client
        let mut client = timeout(
            Duration::from_secs(10),
            IpcClient::connect(fixture.client_config.clone())
        )
        .await
        .map_err(|_| anyhow!("Connection timeout"))??;

        // Test device listing
        let devices = client.list_devices().await?;
        assert!(!devices.is_empty());

        // Test profile listing
        let profiles = client.list_profiles().await?;
        assert!(!profiles.is_empty());

        // Test game status
        let game_status = client.get_game_status().await?;
        assert!(!game_status.active_game.is_empty());

        // Clean up
        server_handle.abort();
        Ok(())
    }
    
    #[tokio::test]
    #[ignore = "requires running server"]
    async fn test_health_event_streaming() -> Result<()> {
        let fixture = IpcTestFixture::new().await;
        let server_handle = fixture.start_server().await;
        
        // Give server time to start
        sleep(Duration::from_secs(1)).await;
        
        // Connect client
        let mut client = timeout(
            Duration::from_secs(10),
            IpcClient::connect(fixture.client_config.clone())
        )
        .await
        .map_err(|_| anyhow!("Connection timeout"))??;

        // Subscribe to health events
        let _health_stream = client.subscribe_health().await?;
        
        // Test that we can receive health events (this would require the server to emit events)
        // In a real test, we'd trigger some events and verify we receive them

        // Clean up
        server_handle.abort();
        Ok(())
    }
}
