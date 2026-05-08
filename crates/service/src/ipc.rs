//! IPC server implementation with platform-specific transport
//!
//! This module provides the gRPC server implementation for wheel service IPC,
//! with platform-specific transport layers (Named Pipes on Windows, UDS on Linux).

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status, Streaming, transport::Server};
use tracing::{debug, error, info, warn};

use racing_wheel_schemas::generated::wheel::v1::{
    wheel_service_server::{WheelService, WheelServiceServer},
    *,
};
use racing_wheel_schemas::prelude::*;

/// Check if client version is compatible with minimum required version
fn is_version_compatible(client_version: &str, min_version: &str) -> bool {
    // Simplified semantic version comparison
    // In a real implementation, you'd use a proper semver library
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
}

// Placeholder service types for IPC implementation
// These will be replaced with the real service implementations when they're ready

/// Placeholder device service
pub struct DeviceService;

impl DeviceService {
    pub async fn list_devices(
        &self,
    ) -> Result<Vec<TestDevice>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![TestDevice {
            id: "test-device-1".to_string(),
            name: "Test Wheel Base".to_string(),
            vendor_id: 0x1234,
            product_id: 0x5678,
            device_type: 1,
            capabilities: TestDeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: true,
                max_torque_cnm: 2500,
                encoder_cpr: 65536,
                min_report_period_us: 1000,
            },
            state: 1,
        }])
    }

    pub async fn get_device_status(
        &self,
        _device_id: &str,
    ) -> Result<TestDeviceStatus, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TestDeviceStatus {
            device: TestDevice {
                id: "test-device-1".to_string(),
                name: "Test Wheel Base".to_string(),
                vendor_id: 0x1234,
                product_id: 0x5678,
                device_type: 1,
                capabilities: TestDeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: true,
                    max_torque_cnm: 2500,
                    encoder_cpr: 65536,
                    min_report_period_us: 1000,
                },
                state: 1,
            },
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

/// Placeholder profile service
pub struct ProfileService;

impl ProfileService {
    pub async fn list_profiles(
        &self,
    ) -> Result<Vec<TestProfile>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![TestProfile {
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
                        TestCurvePoint {
                            input: 0.0,
                            output: 0.0,
                        },
                        TestCurvePoint {
                            input: 1.0,
                            output: 1.0,
                        },
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
        }])
    }

    pub async fn get_active_profile(
        &self,
        _device_id: &str,
    ) -> Result<TestProfile, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TestProfile {
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
                        TestCurvePoint {
                            input: 0.0,
                            output: 0.0,
                        },
                        TestCurvePoint {
                            input: 1.0,
                            output: 1.0,
                        },
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
        })
    }

    pub async fn apply_profile(
        &self,
        _device_id: &str,
        _profile: TestProfile,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Placeholder game service
pub struct GameService;

impl GameService {
    pub async fn configure_telemetry(
        &self,
        _game_id: &str,
        _install_path: &str,
        _enable_auto_config: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    pub async fn get_game_status(
        &self,
    ) -> Result<TestGameStatus, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TestGameStatus {
            active_game: Some("iRacing".to_string()),
            telemetry_active: true,
            car_id: Some("gt3_bmw".to_string()),
            track_id: Some("spa".to_string()),
        })
    }
}

/// Placeholder safety service
pub struct SafetyService;

impl SafetyService {
    pub async fn start_high_torque(
        &self,
        _device_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    pub async fn emergency_stop(
        &self,
        _device_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

// Test data types
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

#[derive(Debug, Clone)]
pub struct TestGameStatus {
    pub active_game: Option<String>,
    pub telemetry_active: bool,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// IPC server configuration
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Transport type (auto-detected based on platform)
    pub transport: TransportType,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Health event broadcast buffer size
    pub health_buffer_size: usize,
    /// Feature negotiation timeout
    pub negotiation_timeout: Duration,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            transport: TransportType::default(),
            max_connections: 100,
            health_buffer_size: 1000,
            negotiation_timeout: Duration::from_secs(5),
        }
    }
}

/// Transport type for IPC
#[derive(Debug, Clone)]
pub enum TransportType {
    /// Named Pipes (Windows)
    NamedPipe { pipe_name: String },
    /// Unix Domain Socket (Linux/macOS)
    UnixSocket { socket_path: PathBuf },
}

impl Default for TransportType {
    fn default() -> Self {
        #[cfg(windows)]
        {
            Self::NamedPipe {
                pipe_name: r"\\.\pipe\wheel".to_string(),
            }
        }
        #[cfg(unix)]
        {
            let uid = unsafe { libc::getuid() };
            let socket_path = PathBuf::from(format!("/run/user/{}/wheel.sock", uid));
            Self::UnixSocket { socket_path }
        }
    }
}

/// Health event for broadcasting
#[derive(Debug, Clone)]
pub struct HealthEventInternal {
    pub timestamp: std::time::SystemTime,
    pub device_id: String,
    pub event_type: HealthEventType,
    pub message: String,
    pub metadata: HashMap<String, String>,
}

/// Feature negotiation request/response
#[derive(Debug, Clone)]
pub struct FeatureNegotiation {
    pub client_version: String,
    pub supported_features: Vec<String>,
}

/// IPC server implementation
pub struct IpcServer {
    config: IpcConfig,
    device_service: Arc<DeviceService>,
    profile_service: Arc<ProfileService>,
    game_service: Arc<GameService>,
    safety_service: Arc<SafetyService>,
    health_broadcaster: broadcast::Sender<HealthEventInternal>,
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

#[derive(Debug, Clone)]
struct ClientInfo {
    id: String,
    connected_at: Instant,
    features: Vec<String>,
    version: String,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(
        config: IpcConfig,
        device_service: Arc<DeviceService>,
        profile_service: Arc<ProfileService>,
        game_service: Arc<GameService>,
        safety_service: Arc<SafetyService>,
    ) -> Self {
        let (health_broadcaster, _) = broadcast::channel(config.health_buffer_size);

        Self {
            config,
            device_service,
            profile_service,
            game_service,
            safety_service,
            health_broadcaster,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the IPC server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Starting IPC server with transport: {:?}",
            self.config.transport
        );

        let service = WheelServiceImpl {
            device_service: self.device_service.clone(),
            profile_service: self.profile_service.clone(),
            game_service: self.game_service.clone(),
            safety_service: self.safety_service.clone(),
            health_broadcaster: self.health_broadcaster.clone(),
            connected_clients: self.connected_clients.clone(),
        };

        match &self.config.transport {
            TransportType::NamedPipe { pipe_name } => {
                self.start_named_pipe_server(service, pipe_name).await
            }
            TransportType::UnixSocket { socket_path } => {
                self.start_unix_socket_server(service, socket_path).await
            }
        }
    }

    /// Broadcast a health event to all connected clients
    pub fn broadcast_health_event(&self, event: HealthEventInternal) {
        if let Err(e) = self.health_broadcaster.send(event) {
            warn!("Failed to broadcast health event: {}", e);
        }
    }

    #[cfg(windows)]
    async fn start_named_pipe_server(
        &self,
        service: WheelServiceImpl,
        pipe_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tonic::transport::server::TcpIncoming;
        use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE};

        info!("Starting Named Pipe server on: {}", pipe_name);

        // For now, use TCP as a fallback until we implement proper Named Pipe support
        // This is a simplified implementation that will be enhanced with proper Named Pipe transport
        let addr = "127.0.0.1:50051".parse()?;

        Server::builder()
            .add_service(WheelServiceServer::new(service))
            .serve(addr)
            .await?;

        Ok(())
    }

    #[cfg(unix)]
    async fn start_unix_socket_server(
        &self,
        service: WheelServiceImpl,
        socket_path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::UnixListener;
        use tonic::transport::server::UdsConnectInfo;

        info!("Starting Unix Domain Socket server on: {:?}", socket_path);

        // Remove existing socket file if it exists
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let uds = UnixListener::bind(socket_path)?;
        let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);

        // Set socket permissions (readable/writable by owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(socket_path)?.permissions();
            perms.set_mode(0o600); // rw-------
            std::fs::set_permissions(socket_path, perms)?;
        }

        Server::builder()
            .add_service(WheelServiceServer::new(service))
            .serve_with_incoming(uds_stream)
            .await?;

        Ok(())
    }

    #[cfg(windows)]
    async fn start_unix_socket_server(
        &self,
        _service: WheelServiceImpl,
        _socket_path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Unix Domain Sockets not supported on Windows")
    }

    #[cfg(unix)]
    async fn start_named_pipe_server(
        &self,
        _service: WheelServiceImpl,
        _pipe_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Named Pipes not supported on Unix systems")
    }
}

/// gRPC service implementation
#[derive(Clone)]
struct WheelServiceImpl {
    device_service: Arc<DeviceService>,
    profile_service: Arc<ProfileService>,
    game_service: Arc<GameService>,
    safety_service: Arc<SafetyService>,
    health_broadcaster: broadcast::Sender<HealthEventInternal>,
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

#[async_trait]
impl WheelService for WheelServiceImpl {
    type ListDevicesStream = Pin<Box<dyn Stream<Item = Result<DeviceInfo, Status>> + Send>>;
    type SubscribeHealthStream = Pin<Box<dyn Stream<Item = Result<HealthEvent, Status>> + Send>>;

    /// List all connected devices (streaming)
    async fn list_devices(
        &self,
        _request: Request<()>,
    ) -> Result<Response<Self::ListDevicesStream>, Status> {
        debug!("ListDevices called");

        let device_service = self.device_service.clone();

        let stream = async_stream::stream! {
            // Get initial device list
            match device_service.list_devices().await {
                Ok(devices) => {
                    for device in devices {
                        let device_info = DeviceInfo {
                            id: device.id.to_string(),
                            name: device.name.clone(),
                            r#type: device.device_type as i32,
                            vendor_id: u32::from(device.vendor_id),
                            product_id: u32::from(device.product_id),
                            capabilities: Some(DeviceCapabilities {
                                supports_pid: device.capabilities.supports_pid,
                                supports_raw_torque_1khz: device.capabilities.supports_raw_torque_1khz,
                                supports_health_stream: device.capabilities.supports_health_stream,
                                supports_led_bus: device.capabilities.supports_led_bus,
                                max_torque_cnm: device.capabilities.max_torque_cnm,
                                encoder_cpr: device.capabilities.encoder_cpr,
                                min_report_period_us: device.capabilities.min_report_period_us,
                            }),
                            state: device.state as i32,
                        };
                        yield Ok(device_info);
                    }
                }
                Err(e) => {
                    yield Err(Status::internal(format!("Failed to list devices: {}", e)));
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Get device status
    async fn get_device_status(
        &self,
        request: Request<DeviceId>,
    ) -> Result<Response<DeviceStatus>, Status> {
        let device_id = &request.into_inner().id;
        debug!("GetDeviceStatus called for device: {}", device_id);

        match self.device_service.get_device_status(device_id).await {
            Ok(status) => {
                let device_status = DeviceStatus {
                    device: Some(DeviceInfo {
                        id: status.device.id.to_string(),
                        name: status.device.name.clone(),
                        r#type: status.device.device_type as i32,
                        vendor_id: u32::from(status.device.vendor_id),
                        product_id: u32::from(status.device.product_id),
                        capabilities: Some(DeviceCapabilities {
                            supports_pid: status.device.capabilities.supports_pid,
                            supports_raw_torque_1khz: status
                                .device
                                .capabilities
                                .supports_raw_torque_1khz,
                            supports_health_stream: status
                                .device
                                .capabilities
                                .supports_health_stream,
                            supports_led_bus: status.device.capabilities.supports_led_bus,
                            max_torque_cnm: status.device.capabilities.max_torque_cnm,
                            encoder_cpr: status.device.capabilities.encoder_cpr,
                            min_report_period_us: status.device.capabilities.min_report_period_us,
                        }),
                        state: status.device.state as i32,
                    }),
                    last_seen: Some(prost_types::Timestamp {
                        seconds: status.last_seen.timestamp(),
                        nanos: status.last_seen.timestamp_subsec_nanos() as i32,
                    }),
                    active_faults: status.active_faults,
                    telemetry: status.telemetry.map(|t| TelemetryData {
                        wheel_angle_mdeg: openracing_hid_common::math::safe_clamp(
                            t.wheel_angle_deg * 1000.0,
                            i32::MIN as f32,
                            i32::MAX as f32,
                        ) as i32,
                        wheel_speed_mrad_s: openracing_hid_common::math::safe_clamp(
                            t.wheel_speed_rad_s * 1000.0,
                            i32::MIN as f32,
                            i32::MAX as f32,
                        ) as i32,
                        temp_c: t.temperature_c as u32,
                        faults: t.fault_flags as u32,
                        hands_on: t.hands_on,
                        sequence: 0, // Field removed, use 0 as default
                    }),
                    moza: None,
                };
                Ok(Response::new(device_status))
            }
            Err(e) => Err(Status::not_found(format!("Device not found: {}", e))),
        }
    }

    /// Get active profile for a device
    async fn get_active_profile(
        &self,
        request: Request<DeviceId>,
    ) -> Result<Response<Profile>, Status> {
        let device_id = &request.into_inner().id;
        debug!("GetActiveProfile called for device: {}", device_id);

        match self.profile_service.get_active_profile(device_id).await {
            Ok(profile) => {
                let proto_profile = Profile {
                    schema_version: profile.schema_version,
                    scope: Some(ProfileScope {
                        game: profile.scope.game.unwrap_or_default(),
                        car: profile.scope.car.unwrap_or_default(),
                        track: profile.scope.track.unwrap_or_default(),
                    }),
                    base: Some(BaseSettings {
                        ffb_gain: profile.base.ffb_gain,
                        dor_deg: profile.base.dor_deg as u32,
                        torque_cap_nm: profile.base.torque_cap_nm,
                        filters: Some(FilterConfig {
                            reconstruction: profile.base.filters.reconstruction as u32,
                            friction: profile.base.filters.friction,
                            damper: profile.base.filters.damper,
                            inertia: profile.base.filters.inertia,
                            notch_filters: profile
                                .base
                                .filters
                                .notch_filters
                                .into_iter()
                                .map(|nf| NotchFilter {
                                    hz: nf.hz,
                                    q: nf.q,
                                    gain_db: nf.gain_db,
                                })
                                .collect(),
                            slew_rate: profile.base.filters.slew_rate,
                            curve_points: profile
                                .base
                                .filters
                                .curve_points
                                .into_iter()
                                .map(|cp| CurvePoint {
                                    input: cp.input,
                                    output: cp.output,
                                })
                                .collect(),
                        }),
                    }),
                    leds: profile.leds.map(|led| LedConfig {
                        rpm_bands: led.rpm_bands,
                        pattern: led.pattern,
                        brightness: led.brightness,
                    }),
                    haptics: profile.haptics.map(|haptics| HapticsConfig {
                        enabled: haptics.enabled,
                        intensity: haptics.intensity,
                        frequency_hz: haptics.frequency_hz,
                    }),
                    signature: profile.signature.unwrap_or_default(),
                };
                Ok(Response::new(proto_profile))
            }
            Err(e) => Err(Status::not_found(format!("Profile not found: {}", e))),
        }
    }

    /// Apply a profile to a device
    async fn apply_profile(
        &self,
        request: Request<ApplyProfileRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        let device_id = req.device.map(|d| d.id).unwrap_or_default();
        debug!("ApplyProfile called for device: {}", device_id);

        if let Some(profile) = req.profile {
            // Convert protobuf profile to test profile
            let test_profile = TestProfile {
                schema_version: profile.schema_version,
                scope: TestProfileScope {
                    game: profile.scope.as_ref().map(|s| s.game.clone()),
                    car: profile.scope.as_ref().map(|s| s.car.clone()),
                    track: profile.scope.as_ref().map(|s| s.track.clone()),
                },
                base: TestBaseSettings {
                    ffb_gain: profile.base.as_ref().map(|b| b.ffb_gain).unwrap_or(0.5),
                    dor_deg: profile
                        .base
                        .as_ref()
                        .map(|b| b.dor_deg as u16)
                        .unwrap_or(900),
                    torque_cap_nm: profile
                        .base
                        .as_ref()
                        .map(|b| b.torque_cap_nm)
                        .unwrap_or(10.0),
                    filters: TestFilterConfig {
                        reconstruction: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| f.reconstruction as u8)
                            .unwrap_or(4),
                        friction: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| f.friction)
                            .unwrap_or(0.1),
                        damper: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| f.damper)
                            .unwrap_or(0.1),
                        inertia: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| f.inertia)
                            .unwrap_or(0.1),
                        notch_filters: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| {
                                f.notch_filters
                                    .iter()
                                    .map(|nf| TestNotchFilter {
                                        hz: nf.hz,
                                        q: nf.q,
                                        gain_db: nf.gain_db,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        slew_rate: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| f.slew_rate)
                            .unwrap_or(0.8),
                        curve_points: profile
                            .base
                            .as_ref()
                            .and_then(|b| b.filters.as_ref())
                            .map(|f| {
                                f.curve_points
                                    .iter()
                                    .map(|cp| TestCurvePoint {
                                        input: cp.input,
                                        output: cp.output,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                },
                leds: profile.leds.map(|led| TestLedConfig {
                    rpm_bands: led.rpm_bands,
                    pattern: led.pattern,
                    brightness: led.brightness,
                }),
                haptics: profile.haptics.map(|haptics| TestHapticsConfig {
                    enabled: haptics.enabled,
                    intensity: haptics.intensity,
                    frequency_hz: haptics.frequency_hz,
                }),
                signature: if profile.signature.is_empty() {
                    None
                } else {
                    Some(profile.signature)
                },
            };

            match self
                .profile_service
                .apply_profile(&device_id, test_profile)
                .await
            {
                Ok(()) => Ok(Response::new(OpResult {
                    success: true,
                    error_message: String::new(),
                    metadata: HashMap::new(),
                })),
                Err(e) => Ok(Response::new(OpResult {
                    success: false,
                    error_message: format!("Failed to apply profile: {}", e),
                    metadata: HashMap::new(),
                })),
            }
        } else {
            Err(Status::invalid_argument("Profile is required"))
        }
    }

    /// List all available profiles
    async fn list_profiles(&self, _request: Request<()>) -> Result<Response<ProfileList>, Status> {
        debug!("ListProfiles called");

        match self.profile_service.list_profiles().await {
            Ok(profiles) => {
                let proto_profiles = profiles
                    .into_iter()
                    .map(|profile| Profile {
                        schema_version: profile.schema_version,
                        scope: Some(ProfileScope {
                            game: profile.scope.game.unwrap_or_default(),
                            car: profile.scope.car.unwrap_or_default(),
                            track: profile.scope.track.unwrap_or_default(),
                        }),
                        base: Some(BaseSettings {
                            ffb_gain: profile.base.ffb_gain,
                            dor_deg: profile.base.dor_deg as u32,
                            torque_cap_nm: profile.base.torque_cap_nm,
                            filters: Some(FilterConfig {
                                reconstruction: profile.base.filters.reconstruction as u32,
                                friction: profile.base.filters.friction,
                                damper: profile.base.filters.damper,
                                inertia: profile.base.filters.inertia,
                                notch_filters: profile
                                    .base
                                    .filters
                                    .notch_filters
                                    .into_iter()
                                    .map(|nf| NotchFilter {
                                        hz: nf.hz,
                                        q: nf.q,
                                        gain_db: nf.gain_db,
                                    })
                                    .collect(),
                                slew_rate: profile.base.filters.slew_rate,
                                curve_points: profile
                                    .base
                                    .filters
                                    .curve_points
                                    .into_iter()
                                    .map(|cp| CurvePoint {
                                        input: cp.input,
                                        output: cp.output,
                                    })
                                    .collect(),
                            }),
                        }),
                        leds: profile.leds.map(|led| LedConfig {
                            rpm_bands: led.rpm_bands,
                            pattern: led.pattern,
                            brightness: led.brightness,
                        }),
                        haptics: profile.haptics.map(|haptics| HapticsConfig {
                            enabled: haptics.enabled,
                            intensity: haptics.intensity,
                            frequency_hz: haptics.frequency_hz,
                        }),
                        signature: profile.signature.unwrap_or_default(),
                    })
                    .collect();

                Ok(Response::new(ProfileList {
                    profiles: proto_profiles,
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to list profiles: {}", e))),
        }
    }

    /// Start high torque mode for a device
    async fn start_high_torque(
        &self,
        request: Request<DeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id = &request.into_inner().id;
        debug!("StartHighTorque called for device: {}", device_id);

        match self.safety_service.start_high_torque(device_id).await {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: HashMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to start high torque: {}", e),
                metadata: HashMap::new(),
            })),
        }
    }

    /// Emergency stop for a device
    async fn emergency_stop(
        &self,
        request: Request<DeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id = &request.into_inner().id;
        debug!("EmergencyStop called for device: {}", device_id);

        match self.safety_service.emergency_stop(device_id).await {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: HashMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to emergency stop: {}", e),
                metadata: HashMap::new(),
            })),
        }
    }

    /// Subscribe to health events (streaming)
    async fn subscribe_health(
        &self,
        _request: Request<()>,
    ) -> Result<Response<Self::SubscribeHealthStream>, Status> {
        debug!("SubscribeHealth called");

        let mut health_receiver = self.health_broadcaster.subscribe();

        let stream = async_stream::stream! {
            while let Ok(event) = health_receiver.recv().await {
                let health_event = HealthEvent {
                    timestamp: Some(prost_types::Timestamp {
                        seconds: event.timestamp.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_secs() as i64,
                        nanos: 0,
                    }),
                    device_id: event.device_id,
                    r#type: event.event_type as i32,
                    message: event.message,
                    metadata: event.metadata,
                };
                yield Ok(health_event);
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Get diagnostics for a device
    async fn get_diagnostics(
        &self,
        request: Request<DeviceId>,
    ) -> Result<Response<DiagnosticInfo>, Status> {
        let device_id = &request.into_inner().id;
        debug!("GetDiagnostics called for device: {}", device_id);

        // This would integrate with the diagnostic service when implemented
        let diagnostic_info = DiagnosticInfo {
            device_id: device_id.clone(),
            system_info: HashMap::new(),
            recent_faults: vec![],
            performance: Some(PerformanceMetrics {
                p99_jitter_us: 0.0,
                missed_tick_rate: 0.0,
                total_ticks: 0,
                missed_ticks: 0,
            }),
        };

        Ok(Response::new(diagnostic_info))
    }

    /// Configure telemetry for a game
    async fn configure_telemetry(
        &self,
        request: Request<ConfigureTelemetryRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        debug!("ConfigureTelemetry called for game: {}", req.game_id);

        match self
            .game_service
            .configure_telemetry(&req.game_id, &req.install_path, req.enable_auto_config)
            .await
        {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: HashMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to configure telemetry: {}", e),
                metadata: HashMap::new(),
            })),
        }
    }

    /// Get current game status
    async fn get_game_status(&self, _request: Request<()>) -> Result<Response<GameStatus>, Status> {
        debug!("GetGameStatus called");

        match self.game_service.get_game_status().await {
            Ok(status) => Ok(Response::new(GameStatus {
                active_game: status.active_game.unwrap_or_default(),
                telemetry_active: status.telemetry_active,
                car_id: status.car_id.unwrap_or_default(),
                track_id: status.track_id.unwrap_or_default(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to get game status: {}",
                e
            ))),
        }
    }

    /// Negotiate features for backward compatibility
    async fn negotiate_features(
        &self,
        request: Request<FeatureNegotiationRequest>,
    ) -> Result<Response<FeatureNegotiationResponse>, Status> {
        let req = request.into_inner();
        debug!(
            "NegotiateFeatures called - client version: {}, namespace: {}",
            req.client_version, req.namespace
        );

        // Current server version and supported features
        const SERVER_VERSION: &str = "0.1.0";
        const MIN_CLIENT_VERSION: &str = "0.1.0";

        let server_features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
            "game_integration".to_string(),
            "streaming_health".to_string(),
            "streaming_devices".to_string(),
        ];

        // Check namespace compatibility
        let compatible = req.namespace == "wheel.v1" || req.namespace.is_empty();

        if !compatible {
            return Ok(Response::new(FeatureNegotiationResponse {
                server_version: SERVER_VERSION.to_string(),
                supported_features: server_features.clone(),
                enabled_features: vec![],
                compatible: false,
                min_client_version: MIN_CLIENT_VERSION.to_string(),
            }));
        }

        // Check version compatibility (simplified semantic versioning)
        let client_compatible = is_version_compatible(&req.client_version, MIN_CLIENT_VERSION);

        // Determine enabled features (intersection of client and server features)
        let enabled_features: Vec<String> = req
            .supported_features
            .iter()
            .filter(|feature| server_features.contains(feature))
            .cloned()
            .collect();

        // Register client
        let client_id = format!("client_{}", uuid::Uuid::new_v4());
        let client_info = ClientInfo {
            id: client_id.clone(),
            connected_at: Instant::now(),
            features: enabled_features.clone(),
            version: req.client_version.clone(),
        };

        self.connected_clients
            .write()
            .await
            .insert(client_id, client_info);

        Ok(Response::new(FeatureNegotiationResponse {
            server_version: SERVER_VERSION.to_string(),
            supported_features: server_features,
            enabled_features,
            compatible: client_compatible,
            min_client_version: MIN_CLIENT_VERSION.to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_devices_preserves_usb_identity() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (tx, _) = broadcast::channel(10);
        let service = WheelServiceImpl {
            device_service: Arc::new(DeviceService),
            profile_service: Arc::new(ProfileService),
            game_service: Arc::new(GameService),
            safety_service: Arc::new(SafetyService),
            health_broadcaster: tx,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
        };

        let response = service.list_devices(Request::new(())).await?;
        let mut stream = response.into_inner();
        let first = stream
            .next()
            .await
            .ok_or("expected one device")??;

        assert_eq!(first.vendor_id, 0x1234);
        assert_eq!(first.product_id, 0x5678);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_device_status_telemetry_conversion(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (tx, _) = broadcast::channel(10);
        let service = WheelServiceImpl {
            device_service: Arc::new(DeviceService),
            profile_service: Arc::new(ProfileService),
            game_service: Arc::new(GameService),
            safety_service: Arc::new(SafetyService),
            health_broadcaster: tx,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
        };

        let request = Request::new(DeviceId {
            id: "test-device-1".to_string(),
        });

        let response = service.get_device_status(request).await?;

        let status = response.into_inner();

        // Ensure the response contains telemetry and conversions didn't panic.
        assert!(status.telemetry.is_some(), "Expected telemetry data in response");
        Ok(())
    }
}
