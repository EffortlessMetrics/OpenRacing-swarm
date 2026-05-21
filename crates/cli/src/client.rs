//! IPC client for communicating with wheeld service
//!
//! Uses gRPC via tonic to connect to the wheeld service. When the service is not
//! reachable, `connect_or_mock()` transparently falls back to a built-in mock
//! backend so that the CLI remains functional for development, testing, and
//! offline profile management.

use crate::error::CliError;
use anyhow::Result;
use racing_wheel_hid_moza_protocol::{
    MOZA_VENDOR_ID, MozaDeviceCategory, identify_device, is_wheelbase_product,
};
use racing_wheel_schemas::generated::wheel::v1 as wire;
use racing_wheel_schemas::telemetry::TelemetryData as SchemasTelemetryData;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;

mod connection;

// ---------------------------------------------------------------------------
// Backend enum -- either a live gRPC channel or a self-contained mock
// ---------------------------------------------------------------------------

enum ClientBackend {
    Grpc(Arc<Mutex<wire::wheel_service_client::WheelServiceClient<tonic::transport::Channel>>>),
    Mock,
}

/// Client for communicating with the wheel service.
///
/// Transparently supports both a live gRPC connection and an in-process mock
/// backend. Use [`WheelClient::connect`] when a running wheeld service is
/// required, or [`WheelClient::connect_or_mock`] to gracefully fall back.
pub struct WheelClient {
    backend: ClientBackend,
}

impl WheelClient {
    /// Create a new client that **requires** a running wheeld service.
    ///
    /// If `endpoint` is `None`, connects to the default endpoint.
    /// Returns `CliError::ServiceUnavailable` if the service cannot be reached.
    pub async fn connect(endpoint: Option<&str>) -> Result<Self> {
        let endpoint_str = connection::resolve_endpoint(endpoint)?;
        let channel = connection::connect_channel(endpoint_str).await?;

        let grpc_client = wire::wheel_service_client::WheelServiceClient::new(channel);

        Ok(Self {
            backend: ClientBackend::Grpc(Arc::new(Mutex::new(grpc_client))),
        })
    }

    /// Try to connect via gRPC; fall back to the mock backend when appropriate.
    ///
    /// This is the primary constructor used by CLI commands. When the wheeld
    /// service is unreachable, the client falls back to an in-process mock so
    /// that the CLI remains usable for development, testing, and offline
    /// profile management.
    ///
    /// The mock fallback is used when:
    /// - No endpoint was specified (default local service not running), or
    /// - An explicit endpoint targeting loopback/localhost was given but the
    ///   service is not running there.
    ///
    /// If the endpoint points to a non-local host, connection failures are
    /// reported as errors because the user clearly intended to reach a
    /// specific remote service.
    pub async fn connect_or_mock(endpoint: Option<&str>) -> Result<Self> {
        match Self::connect(endpoint).await {
            Ok(client) => Ok(client),
            Err(e) => {
                // Fall back to mock for local/loopback endpoints or when none given
                let use_mock = match endpoint {
                    None => true,
                    Some(ep) => is_local_endpoint(ep),
                };
                if use_mock {
                    tracing::debug!("wheeld not reachable; using mock backend");
                    Ok(Self {
                        backend: ClientBackend::Mock,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Public API -- each method dispatches to the appropriate backend
    // -----------------------------------------------------------------------

    /// List all connected devices.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let response = client.list_devices(()).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to list devices: {}",
                        status.message()
                    ))
                })?;

                let mut stream = response.into_inner();
                let mut devices = Vec::new();

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(wire_device) => {
                            devices.push(DeviceInfo::from_wire(wire_device));
                        }
                        Err(status) => {
                            tracing::warn!(
                                "Error receiving device from stream: {}",
                                status.message()
                            );
                            break;
                        }
                    }
                }

                Ok(devices)
            }
            ClientBackend::Mock => Ok(mock::list_devices()),
        }
    }

    /// Get device status.
    pub async fn get_device_status(&self, device_id: &str) -> Result<DeviceStatus> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::DeviceId {
                    id: device_id.to_string(),
                };

                let response = client.get_device_status(request).await.map_err(|status| {
                    if status.code() == tonic::Code::NotFound {
                        CliError::DeviceNotFound(device_id.to_string())
                    } else {
                        CliError::ServiceUnavailable(format!(
                            "Failed to get device status: {}",
                            status.message()
                        ))
                    }
                })?;

                let wire_status = response.into_inner();
                Ok(DeviceStatus::from_wire(wire_status, device_id))
            }
            ClientBackend::Mock => mock::get_device_status(device_id),
        }
    }

    /// Apply profile to device.
    pub async fn apply_profile(
        &self,
        device_id: &str,
        _profile: &racing_wheel_schemas::config::ProfileSchema,
    ) -> Result<()> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::ApplyProfileRequest {
                    device: Some(wire::DeviceId {
                        id: device_id.to_string(),
                    }),
                    profile: Some(wire::Profile {
                        schema_version: "wheel.profile/1".to_string(),
                        scope: None,
                        base: None,
                        leds: None,
                        haptics: None,
                        signature: String::new(),
                    }),
                };

                let response = client.apply_profile(request).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to apply profile: {}",
                        status.message()
                    ))
                })?;

                let result = response.into_inner();
                if result.success {
                    Ok(())
                } else {
                    Err(CliError::ValidationError(result.error_message).into())
                }
            }
            ClientBackend::Mock => {
                tracing::info!("Applying profile to device {}", device_id);
                Ok(())
            }
        }
    }

    /// Get active profile for device.
    #[allow(dead_code)]
    pub async fn get_active_profile(
        &self,
        device_id: &str,
    ) -> Result<racing_wheel_schemas::config::ProfileSchema> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::DeviceId {
                    id: device_id.to_string(),
                };

                let response = client.get_active_profile(request).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to get active profile: {}",
                        status.message()
                    ))
                })?;

                let wire_profile = response.into_inner();
                Ok(WireProfileSchema::from_wire(wire_profile))
            }
            ClientBackend::Mock => Ok(mock::get_active_profile()),
        }
    }

    /// Start high torque mode.
    pub async fn start_high_torque(&self, device_id: &str) -> Result<()> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::DeviceId {
                    id: device_id.to_string(),
                };

                let response = client.start_high_torque(request).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to start high torque: {}",
                        status.message()
                    ))
                })?;

                let result = response.into_inner();
                if result.success {
                    Ok(())
                } else {
                    Err(CliError::ValidationError(result.error_message).into())
                }
            }
            ClientBackend::Mock => {
                tracing::info!("Starting high torque mode for device {}", device_id);
                Ok(())
            }
        }
    }

    /// Emergency stop.
    pub async fn emergency_stop(&self, device_id: Option<&str>) -> Result<()> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::DeviceId {
                    id: device_id.unwrap_or("").to_string(),
                };

                let response = client.emergency_stop(request).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to send emergency stop: {}",
                        status.message()
                    ))
                })?;

                let result = response.into_inner();
                if result.success {
                    Ok(())
                } else {
                    Err(CliError::ValidationError(result.error_message).into())
                }
            }
            ClientBackend::Mock => {
                match device_id {
                    Some(id) => tracing::warn!("Emergency stop for device {}", id),
                    None => tracing::warn!("Emergency stop for all devices"),
                }
                Ok(())
            }
        }
    }

    /// Get diagnostics.
    pub async fn get_diagnostics(&self, device_id: &str) -> Result<DiagnosticInfo> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::DeviceId {
                    id: device_id.to_string(),
                };

                let response = client.get_diagnostics(request).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to get diagnostics: {}",
                        status.message()
                    ))
                })?;

                let wire_diag = response.into_inner();
                Ok(DiagnosticInfo::from_wire(wire_diag))
            }
            ClientBackend::Mock => Ok(mock::get_diagnostics(device_id)),
        }
    }

    /// Configure game telemetry.
    pub async fn configure_telemetry(
        &self,
        game_id: &str,
        install_path: Option<&str>,
    ) -> Result<()> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let request = wire::ConfigureTelemetryRequest {
                    game_id: game_id.to_string(),
                    install_path: install_path.unwrap_or("").to_string(),
                    enable_auto_config: true,
                };

                let response = client
                    .configure_telemetry(request)
                    .await
                    .map_err(|status| {
                        CliError::ServiceUnavailable(format!(
                            "Failed to configure telemetry: {}",
                            status.message()
                        ))
                    })?;

                let result = response.into_inner();
                if result.success {
                    Ok(())
                } else {
                    Err(CliError::ValidationError(result.error_message).into())
                }
            }
            ClientBackend::Mock => {
                tracing::info!(
                    "Configuring telemetry for game {} at path {:?}",
                    game_id,
                    install_path
                );
                Ok(())
            }
        }
    }

    /// Get game status.
    pub async fn get_game_status(&self) -> Result<GameStatus> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let response = client.get_game_status(()).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to get game status: {}",
                        status.message()
                    ))
                })?;

                let wire_status = response.into_inner();
                Ok(GameStatus::from_wire(wire_status))
            }
            ClientBackend::Mock => Ok(mock::get_game_status()),
        }
    }

    /// Subscribe to health events.
    pub async fn subscribe_health(&self) -> Result<HealthEventStream> {
        match &self.backend {
            ClientBackend::Grpc(inner) => {
                let mut client = inner.lock().await;
                let response = client.subscribe_health(()).await.map_err(|status| {
                    CliError::ServiceUnavailable(format!(
                        "Failed to subscribe to health events: {}",
                        status.message()
                    ))
                })?;

                Ok(HealthEventStream {
                    kind: HealthStreamKind::Grpc(Box::new(response.into_inner())),
                })
            }
            ClientBackend::Mock => Ok(HealthEventStream {
                kind: HealthStreamKind::Mock,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `endpoint` targets a loopback or localhost address,
/// meaning the user likely intended to reach a local wheeld and a mock
/// fallback is appropriate.
fn is_local_endpoint(ep: &str) -> bool {
    // Strip the scheme prefix to get the authority
    let authority = ep
        .strip_prefix("http://")
        .or_else(|| ep.strip_prefix("https://"))
        .unwrap_or(ep);

    let host = authority.split(':').next().unwrap_or(authority);
    matches!(
        host,
        "localhost" | "127.0.0.1" | "::1" | "[::1]" | "0.0.0.0"
    )
}

// ---------------------------------------------------------------------------
// Mock backend -- returns canned data identical to the pre-IPC-rewrite client
// ---------------------------------------------------------------------------

mod mock {
    use super::*;

    pub(super) fn list_devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo {
                id: "wheel-001".to_string(),
                name: "Fanatec DD Pro".to_string(),
                source: Some("mock".to_string()),
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                product_string: None,
                serial_number_present: None,
                interface_number: None,
                usage_page: None,
                usage: None,
                hid_path_present: None,
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: true,
                    max_torque_nm: 8.0,
                    encoder_cpr: 2048,
                    min_report_period_us: 1000,
                },
            },
            DeviceInfo {
                id: "pedals-001".to_string(),
                name: "Fanatec V3 Pedals".to_string(),
                source: Some("mock".to_string()),
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                product_string: None,
                serial_number_present: None,
                interface_number: None,
                usage_page: None,
                usage: None,
                hid_path_present: None,
                device_type: DeviceType::Pedals,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities {
                    supports_pid: false,
                    supports_raw_torque_1khz: false,
                    supports_health_stream: true,
                    supports_led_bus: false,
                    max_torque_nm: 0.0,
                    encoder_cpr: 1024,
                    min_report_period_us: 5000,
                },
            },
        ]
    }

    pub(super) fn get_device_status(device_id: &str) -> Result<DeviceStatus> {
        let devices = list_devices();
        if !devices.iter().any(|d| d.id == device_id) {
            return Err(CliError::DeviceNotFound(device_id.to_string()).into());
        }

        Ok(DeviceStatus {
            device: DeviceInfo {
                id: device_id.to_string(),
                name: "Mock Device".to_string(),
                source: Some("mock".to_string()),
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                product_string: None,
                serial_number_present: None,
                interface_number: None,
                usage_page: None,
                usage: None,
                hid_path_present: None,
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: true,
                    max_torque_nm: 8.0,
                    encoder_cpr: 2048,
                    min_report_period_us: 1000,
                },
            },
            last_seen: chrono::Utc::now(),
            active_faults: vec![],
            telemetry: TelemetryData {
                wheel_angle_deg: 0.0,
                wheel_speed_rad_s: 0.0,
                temperature_c: 45,
                fault_flags: 0,
                hands_on: true,
            },
            moza: None,
        })
    }

    pub(super) fn get_active_profile() -> racing_wheel_schemas::config::ProfileSchema {
        racing_wheel_schemas::config::ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: Some("iracing".to_string()),
                car: Some("gt3".to_string()),
                track: None,
            },
            base: racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.75,
                dor_deg: 540,
                torque_cap_nm: 8.0,
                filters: racing_wheel_schemas::config::FilterConfig::default(),
            },
            leds: None,
            haptics: None,
            signature: None,
        }
    }

    pub(super) fn get_diagnostics(device_id: &str) -> DiagnosticInfo {
        DiagnosticInfo {
            device_id: device_id.to_string(),
            system_info: std::collections::HashMap::from([
                ("os".to_string(), std::env::consts::OS.to_string()),
                ("arch".to_string(), std::env::consts::ARCH.to_string()),
            ]),
            recent_faults: vec![],
            performance: PerformanceMetrics {
                p99_jitter_us: 0.15,
                missed_tick_rate: 0.0001,
                total_ticks: 1_000_000,
                missed_ticks: 1,
            },
        }
    }

    pub(super) fn get_game_status() -> GameStatus {
        GameStatus {
            active_game: Some("iracing".to_string()),
            telemetry_active: true,
            car_id: Some("gt3".to_string()),
            track_id: Some("spa".to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Local CLI types -- the output module relies on these structures.
// Wire types from the gRPC schema are converted into these types.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hid_path_present: Option<bool>,
    pub device_type: DeviceType,
    pub state: DeviceState,
    pub capabilities: DeviceCapabilities,
}

impl DeviceInfo {
    fn from_wire(w: wire::DeviceInfo) -> Self {
        Self {
            id: w.id,
            name: w.name,
            source: Some("wheeld".to_string()),
            vendor_id: wire_u32_to_hex_u16(w.vendor_id),
            product_id: wire_u32_to_hex_u16(w.product_id),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            hid_path_present: None,
            device_type: DeviceType::from_wire(w.r#type),
            state: DeviceState::from_wire(w.state),
            capabilities: w
                .capabilities
                .map(DeviceCapabilities::from_wire)
                .unwrap_or_default(),
        }
    }
}

fn wire_u32_to_hex_u16(value: u32) -> Option<String> {
    u16::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .map(|value| format!("0x{value:04X}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    Unknown,
    WheelBase,
    SteeringWheel,
    Pedals,
    Shifter,
    Handbrake,
    ButtonBox,
}

impl DeviceType {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => DeviceType::WheelBase,
            2 => DeviceType::SteeringWheel,
            3 => DeviceType::Pedals,
            4 => DeviceType::Shifter,
            5 => DeviceType::Handbrake,
            6 => DeviceType::ButtonBox,
            _ => DeviceType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceState {
    Connected,
    Disconnected,
    Faulted,
    Calibrating,
}

impl DeviceState {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => DeviceState::Connected,
            2 => DeviceState::Disconnected,
            3 => DeviceState::Faulted,
            4 => DeviceState::Calibrating,
            _ => DeviceState::Disconnected, // default for unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub supports_pid: bool,
    pub supports_raw_torque_1khz: bool,
    pub supports_health_stream: bool,
    pub supports_led_bus: bool,
    pub max_torque_nm: f32,
    pub encoder_cpr: u32,
    pub min_report_period_us: u32,
}

impl DeviceCapabilities {
    fn from_wire(w: wire::DeviceCapabilities) -> Self {
        Self {
            supports_pid: w.supports_pid,
            supports_raw_torque_1khz: w.supports_raw_torque_1khz,
            supports_health_stream: w.supports_health_stream,
            supports_led_bus: w.supports_led_bus,
            // Wire uses centi-Nm (max_torque_cnm); convert to Nm
            max_torque_nm: w.max_torque_cnm as f32 / 100.0,
            encoder_cpr: w.encoder_cpr,
            min_report_period_us: w.min_report_period_us,
        }
    }
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self {
            supports_pid: false,
            supports_raw_torque_1khz: false,
            supports_health_stream: false,
            supports_led_bus: false,
            max_torque_nm: 0.0,
            encoder_cpr: 1024,
            min_report_period_us: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub device: DeviceInfo,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub active_faults: Vec<String>,
    pub telemetry: TelemetryData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moza: Option<MozaReadinessStatus>,
}

impl DeviceStatus {
    fn from_wire(w: wire::DeviceStatus, fallback_id: &str) -> Self {
        let device = w
            .device
            .map(DeviceInfo::from_wire)
            .unwrap_or_else(|| DeviceInfo {
                id: fallback_id.to_string(),
                name: "Unknown".to_string(),
                source: None,
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                product_string: None,
                serial_number_present: None,
                interface_number: None,
                usage_page: None,
                usage: None,
                hid_path_present: None,
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities::default(),
            });
        let moza = w
            .moza
            .map(MozaReadinessStatus::from_wire)
            .or_else(|| MozaReadinessStatus::from_device(&device));

        let last_seen = w
            .last_seen
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
            .unwrap_or_else(chrono::Utc::now);

        let telemetry = w
            .telemetry
            .map(TelemetryData::from_wire)
            .unwrap_or_default();

        Self {
            device,
            last_seen,
            active_faults: w.active_faults,
            telemetry,
            moza,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MozaReadinessStatus {
    pub model: String,
    pub product_id: String,
    pub category: String,
    pub output_capable: bool,
    pub ffb_ready: bool,
    pub init_state: String,
    pub descriptor_trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor_crc32: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    pub direct_mode_allowed: bool,
    pub high_torque_allowed: bool,
    pub safe_to_send_torque: bool,
    pub safety_state: String,
    pub safety_reason: String,
}

impl MozaReadinessStatus {
    fn from_wire(w: wire::MozaReadinessStatus) -> Self {
        Self {
            model: w.model,
            product_id: w.product_id,
            category: w.category,
            output_capable: w.output_capable,
            ffb_ready: w.ffb_ready,
            init_state: w.init_state,
            descriptor_trusted: w.descriptor_trusted,
            descriptor_crc32: non_empty_string(w.descriptor_crc32),
            descriptor_source: non_empty_string(w.descriptor_source),
            lane: non_empty_string(w.lane),
            direct_mode_allowed: w.direct_mode_allowed,
            high_torque_allowed: w.high_torque_allowed,
            safe_to_send_torque: w.safe_to_send_torque,
            safety_state: w.safety_state,
            safety_reason: w.safety_reason,
        }
    }

    pub fn from_device(device: &DeviceInfo) -> Option<Self> {
        let vendor_id = parse_hex_u16(device.vendor_id.as_deref()?)?;
        if vendor_id != MOZA_VENDOR_ID {
            return None;
        }

        let product_id = parse_hex_u16(device.product_id.as_deref()?)?;
        let identity = identify_device(product_id);
        let output_capable = is_wheelbase_product(product_id);
        let init_state = if output_capable {
            "uninitialized"
        } else {
            "not_applicable"
        };

        Some(Self {
            model: identity.name.to_string(),
            product_id: format!("0x{product_id:04X}"),
            category: moza_category_label(identity.category).to_string(),
            output_capable,
            ffb_ready: false,
            init_state: init_state.to_string(),
            descriptor_trusted: false,
            descriptor_crc32: None,
            descriptor_source: None,
            lane: None,
            direct_mode_allowed: false,
            high_torque_allowed: false,
            safe_to_send_torque: false,
            safety_state: "pre_validation".to_string(),
            safety_reason: "device status is observe-only; run explicit Moza init, zero, and torque-test gates before any output".to_string(),
        })
    }

    pub fn apply_descriptor_receipt(
        &mut self,
        lane: String,
        descriptor_trusted: bool,
        descriptor_crc32: Option<String>,
        descriptor_source: Option<String>,
    ) {
        self.lane = Some(lane);
        self.descriptor_trusted = descriptor_trusted;
        self.descriptor_crc32 = descriptor_crc32;
        self.descriptor_source = descriptor_source;
        self.direct_mode_allowed = false;
        self.high_torque_allowed = false;
        self.safe_to_send_torque = false;
    }
}

fn non_empty_string(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u16::from_str_radix(hex, 16).ok()
}

fn moza_category_label(category: MozaDeviceCategory) -> &'static str {
    match category {
        MozaDeviceCategory::Wheelbase => "wheelbase",
        MozaDeviceCategory::Pedals => "pedals",
        MozaDeviceCategory::Shifter => "shifter",
        MozaDeviceCategory::Handbrake => "handbrake",
        MozaDeviceCategory::Unknown => "unknown",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

impl TelemetryData {
    fn from_wire(w: wire::TelemetryData) -> Self {
        // Delegate to the canonical wire-to-domain conversion in schemas crate,
        // which handles unit conversion (milli-degrees -> degrees, etc.) and
        // validation.  On conversion error, fall back to defaults.
        match SchemasTelemetryData::try_from(w) {
            Ok(s) => Self {
                wheel_angle_deg: s.wheel_angle_deg,
                wheel_speed_rad_s: s.wheel_speed_rad_s,
                temperature_c: s.temperature_c,
                fault_flags: s.fault_flags,
                hands_on: s.hands_on,
            },
            Err(_) => Self::default(),
        }
    }
}

impl Default for TelemetryData {
    fn default() -> Self {
        Self {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        }
    }
}

/// Helper struct to convert wire Profile to the CLI's ProfileSchema
struct WireProfileSchema;

impl WireProfileSchema {
    fn from_wire(w: wire::Profile) -> racing_wheel_schemas::config::ProfileSchema {
        let scope = w.scope.map(|s| racing_wheel_schemas::config::ProfileScope {
            game: if s.game.is_empty() {
                None
            } else {
                Some(s.game)
            },
            car: if s.car.is_empty() { None } else { Some(s.car) },
            track: if s.track.is_empty() {
                None
            } else {
                Some(s.track)
            },
        });

        let base = w.base.map(|b| racing_wheel_schemas::config::BaseConfig {
            ffb_gain: b.ffb_gain,
            dor_deg: b.dor_deg as u16,
            torque_cap_nm: b.torque_cap_nm,
            filters: b
                .filters
                .map(|f| racing_wheel_schemas::config::FilterConfig {
                    reconstruction: f.reconstruction as u8,
                    friction: f.friction,
                    damper: f.damper,
                    inertia: f.inertia,
                    bumpstop: Default::default(),
                    hands_off: Default::default(),
                    torque_cap: None,
                    notch_filters: f
                        .notch_filters
                        .into_iter()
                        .map(|n| racing_wheel_schemas::config::NotchFilter {
                            hz: n.hz,
                            q: n.q,
                            gain_db: n.gain_db,
                        })
                        .collect(),
                    slew_rate: f.slew_rate,
                    curve_points: f
                        .curve_points
                        .into_iter()
                        .map(|p| racing_wheel_schemas::config::CurvePoint {
                            input: p.input,
                            output: p.output,
                        })
                        .collect(),
                })
                .unwrap_or_default(),
        });

        racing_wheel_schemas::config::ProfileSchema {
            schema: w.schema_version,
            scope: scope.unwrap_or(racing_wheel_schemas::config::ProfileScope {
                game: None,
                car: None,
                track: None,
            }),
            base: base.unwrap_or(racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.75,
                dor_deg: 540,
                torque_cap_nm: 8.0,
                filters: racing_wheel_schemas::config::FilterConfig::default(),
            }),
            leds: None,
            haptics: None,
            signature: if w.signature.is_empty() {
                None
            } else {
                Some(w.signature)
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub device_id: String,
    pub system_info: std::collections::HashMap<String, String>,
    pub recent_faults: Vec<String>,
    pub performance: PerformanceMetrics,
}

impl DiagnosticInfo {
    fn from_wire(w: wire::DiagnosticInfo) -> Self {
        Self {
            device_id: w.device_id,
            system_info: w.system_info.into_iter().collect(),
            recent_faults: w.recent_faults,
            performance: w
                .performance
                .map(PerformanceMetrics::from_wire)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub p99_jitter_us: f32,
    pub missed_tick_rate: f32,
    pub total_ticks: u64,
    pub missed_ticks: u64,
}

impl PerformanceMetrics {
    fn from_wire(w: wire::PerformanceMetrics) -> Self {
        Self {
            p99_jitter_us: w.p99_jitter_us,
            missed_tick_rate: w.missed_tick_rate,
            total_ticks: w.total_ticks,
            missed_ticks: w.missed_ticks,
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            p99_jitter_us: 0.0,
            missed_tick_rate: 0.0,
            total_ticks: 0,
            missed_ticks: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStatus {
    pub active_game: Option<String>,
    pub telemetry_active: bool,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

impl GameStatus {
    fn from_wire(w: wire::GameStatus) -> Self {
        Self {
            active_game: if w.active_game.is_empty() {
                None
            } else {
                Some(w.active_game)
            },
            telemetry_active: w.telemetry_active,
            car_id: if w.car_id.is_empty() {
                None
            } else {
                Some(w.car_id)
            },
            track_id: if w.track_id.is_empty() {
                None
            } else {
                Some(w.track_id)
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub device_id: String,
    pub event_type: HealthEventType,
    pub message: String,
    pub metadata: std::collections::HashMap<String, String>,
}

impl HealthEvent {
    fn from_wire(w: wire::HealthEvent) -> Self {
        let timestamp = w
            .timestamp
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
            .unwrap_or_else(chrono::Utc::now);

        Self {
            timestamp,
            device_id: w.device_id,
            event_type: HealthEventType::from_wire(w.r#type),
            message: w.message,
            metadata: w.metadata.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEventType {
    DeviceConnected,
    DeviceDisconnected,
    FaultDetected,
    FaultCleared,
    PerformanceWarning,
}

impl HealthEventType {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => HealthEventType::DeviceConnected,
            2 => HealthEventType::DeviceDisconnected,
            3 => HealthEventType::FaultDetected,
            4 => HealthEventType::FaultCleared,
            5 => HealthEventType::PerformanceWarning,
            _ => HealthEventType::PerformanceWarning, // default for unknown
        }
    }
}

// ---------------------------------------------------------------------------
// Health event stream -- works for both gRPC and mock backends
// ---------------------------------------------------------------------------

enum HealthStreamKind {
    Grpc(Box<tonic::codec::Streaming<wire::HealthEvent>>),
    Mock,
}

/// Health event stream wrapping either a gRPC server-streaming response or a
/// mock that emits periodic synthetic events.
pub struct HealthEventStream {
    kind: HealthStreamKind,
}

impl HealthEventStream {
    pub async fn next(&mut self) -> Option<HealthEvent> {
        match &mut self.kind {
            HealthStreamKind::Grpc(stream) => match stream.next().await {
                Some(Ok(wire_event)) => Some(HealthEvent::from_wire(wire_event)),
                Some(Err(status)) => {
                    tracing::warn!("Health stream error: {}", status.message());
                    None
                }
                None => None,
            },
            HealthStreamKind::Mock => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Some(HealthEvent {
                    timestamp: chrono::Utc::now(),
                    device_id: "wheel-001".to_string(),
                    event_type: HealthEventType::PerformanceWarning,
                    message: "High jitter detected".to_string(),
                    metadata: std::collections::HashMap::new(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_rejects_invalid_scheme() {
        let result = WheelClient::connect(Some("ftp://localhost")).await;
        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err_msg.contains("Invalid endpoint"));
    }

    #[tokio::test]
    async fn connect_rejects_plain_string() {
        let result = WheelClient::connect(Some("not-a-url")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_fails_when_service_not_running() {
        // Try connecting to a port where no service is running
        let result = WheelClient::connect(Some("http://127.0.0.1:19999")).await;
        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            err_msg.contains("Could not connect") || err_msg.contains("Service unavailable"),
            "Expected connection error, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn connect_or_mock_falls_back_to_mock() -> Result<(), Box<dyn std::error::Error>> {
        // Even with no service running, connect_or_mock should succeed
        let client = WheelClient::connect_or_mock(None).await?;
        let devices = client.list_devices().await?;
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "wheel-001");
        assert_eq!(devices[1].id, "pedals-001");
        Ok(())
    }

    #[tokio::test]
    async fn mock_get_device_status_known_device() -> Result<(), Box<dyn std::error::Error>> {
        let client = WheelClient::connect_or_mock(None).await?;
        let status = client.get_device_status("wheel-001").await?;
        assert_eq!(status.device.id, "wheel-001");
        assert!(status.active_faults.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn mock_get_device_status_unknown_device() -> Result<(), Box<dyn std::error::Error>> {
        let client = WheelClient::connect_or_mock(None).await?;
        let result = client.get_device_status("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn mock_emergency_stop_all() -> Result<(), Box<dyn std::error::Error>> {
        let client = WheelClient::connect_or_mock(None).await?;
        client.emergency_stop(None).await?;
        Ok(())
    }

    #[tokio::test]
    async fn mock_emergency_stop_specific() -> Result<(), Box<dyn std::error::Error>> {
        let client = WheelClient::connect_or_mock(None).await?;
        client.emergency_stop(Some("wheel-001")).await?;
        Ok(())
    }

    #[test]
    fn device_capabilities_default() {
        let caps = DeviceCapabilities::default();
        assert!(!caps.supports_pid);
        assert!(!caps.supports_raw_torque_1khz);
        assert!(!caps.supports_health_stream);
        assert!(!caps.supports_led_bus);
        assert!((caps.max_torque_nm - 0.0).abs() < f32::EPSILON);
        assert_eq!(caps.encoder_cpr, 1024);
        assert_eq!(caps.min_report_period_us, 1000);
    }

    #[test]
    fn device_info_from_wire() {
        let wire_device = wire::DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Test Wheel".to_string(),
            r#type: 1, // WheelBase
            state: 1,  // Connected
            vendor_id: 0x346E,
            product_id: 0x0014,
            capabilities: Some(wire::DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 800, // 8.0 Nm
                encoder_cpr: 2048,
                min_report_period_us: 1000,
            }),
        };

        let device = DeviceInfo::from_wire(wire_device);
        assert_eq!(device.id, "wheel-001");
        assert_eq!(device.name, "Test Wheel");
        assert_eq!(device.vendor_id.as_deref(), Some("0x346E"));
        assert_eq!(device.product_id.as_deref(), Some("0x0014"));
        assert!(matches!(device.device_type, DeviceType::WheelBase));
        assert!(matches!(device.state, DeviceState::Connected));
        assert!(device.capabilities.supports_pid);
        assert!((device.capabilities.max_torque_nm - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn device_info_from_wire_omits_zero_usb_identity() {
        let wire_device = wire::DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Test Wheel".to_string(),
            r#type: 1,
            state: 1,
            vendor_id: 0,
            product_id: 0,
            capabilities: None,
        };

        let device = DeviceInfo::from_wire(wire_device);
        let json = serde_json::to_value(&device).unwrap_or_default();

        assert!(device.vendor_id.is_none());
        assert!(device.product_id.is_none());
        assert!(json.get("vendor_id").is_none());
        assert!(json.get("product_id").is_none());
    }

    #[test]
    fn device_state_from_wire_covers_all_variants() {
        assert!(matches!(DeviceState::from_wire(1), DeviceState::Connected));
        assert!(matches!(
            DeviceState::from_wire(2),
            DeviceState::Disconnected
        ));
        assert!(matches!(DeviceState::from_wire(3), DeviceState::Faulted));
        assert!(matches!(
            DeviceState::from_wire(4),
            DeviceState::Calibrating
        ));
        // Unknown defaults to Disconnected
        assert!(matches!(
            DeviceState::from_wire(99),
            DeviceState::Disconnected
        ));
    }

    #[test]
    fn device_type_from_wire_covers_all_variants() {
        assert!(matches!(DeviceType::from_wire(0), DeviceType::Unknown));
        assert!(matches!(DeviceType::from_wire(1), DeviceType::WheelBase));
        assert!(matches!(
            DeviceType::from_wire(2),
            DeviceType::SteeringWheel
        ));
        assert!(matches!(DeviceType::from_wire(3), DeviceType::Pedals));
        assert!(matches!(DeviceType::from_wire(4), DeviceType::Shifter));
        assert!(matches!(DeviceType::from_wire(5), DeviceType::Handbrake));
        assert!(matches!(DeviceType::from_wire(6), DeviceType::ButtonBox));
        assert!(matches!(DeviceType::from_wire(99), DeviceType::Unknown));
    }

    #[test]
    fn device_status_from_wire_adds_conservative_moza_readiness()
    -> Result<(), Box<dyn std::error::Error>> {
        let status = DeviceStatus::from_wire(
            wire::DeviceStatus {
                device: Some(wire::DeviceInfo {
                    id: "moza-r5".to_string(),
                    name: "Moza R5".to_string(),
                    r#type: 1,
                    state: 1,
                    vendor_id: 0x346E,
                    product_id: 0x0014,
                    capabilities: Some(wire::DeviceCapabilities {
                        supports_pid: false,
                        supports_raw_torque_1khz: true,
                        supports_health_stream: true,
                        supports_led_bus: false,
                        max_torque_cnm: 550,
                        encoder_cpr: 2048,
                        min_report_period_us: 1000,
                    }),
                }),
                last_seen: None,
                active_faults: Vec::new(),
                telemetry: None,
                moza: None,
            },
            "moza-r5",
        );

        let moza = status.moza.as_ref().ok_or("missing Moza readiness")?;
        assert_eq!(moza.model, "Moza R5");
        assert_eq!(moza.product_id, "0x0014");
        assert_eq!(moza.category, "wheelbase");
        assert!(moza.output_capable);
        assert!(!moza.ffb_ready);
        assert_eq!(moza.init_state, "uninitialized");
        assert!(!moza.descriptor_trusted);
        assert!(!moza.direct_mode_allowed);
        assert!(!moza.high_torque_allowed);
        assert!(!moza.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn device_status_from_wire_marks_moza_peripheral_not_output_capable()
    -> Result<(), Box<dyn std::error::Error>> {
        let status = DeviceStatus::from_wire(
            wire::DeviceStatus {
                device: Some(wire::DeviceInfo {
                    id: "moza-hbp".to_string(),
                    name: "Moza HBP".to_string(),
                    r#type: 5,
                    state: 1,
                    vendor_id: 0x346E,
                    product_id: 0x0022,
                    capabilities: Some(wire::DeviceCapabilities {
                        supports_pid: false,
                        supports_raw_torque_1khz: false,
                        supports_health_stream: true,
                        supports_led_bus: false,
                        max_torque_cnm: 0,
                        encoder_cpr: 2048,
                        min_report_period_us: 1000,
                    }),
                }),
                last_seen: None,
                active_faults: Vec::new(),
                telemetry: None,
                moza: None,
            },
            "moza-hbp",
        );

        let moza = status.moza.as_ref().ok_or("missing Moza readiness")?;
        assert_eq!(moza.model, "Moza HBP Handbrake");
        assert_eq!(moza.category, "handbrake");
        assert!(!moza.output_capable);
        assert_eq!(moza.init_state, "not_applicable");
        assert!(!moza.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn device_status_from_wire_prefers_service_moza_readiness()
    -> Result<(), Box<dyn std::error::Error>> {
        let status = DeviceStatus::from_wire(
            wire::DeviceStatus {
                device: Some(wire::DeviceInfo {
                    id: "moza-r5".to_string(),
                    name: "Moza R5".to_string(),
                    r#type: 1,
                    state: 1,
                    vendor_id: 0x346E,
                    product_id: 0x0014,
                    capabilities: None,
                }),
                last_seen: None,
                active_faults: Vec::new(),
                telemetry: None,
                moza: Some(wire::MozaReadinessStatus {
                    model: "Moza R5".to_string(),
                    product_id: "0x0014".to_string(),
                    category: "wheelbase".to_string(),
                    output_capable: true,
                    ffb_ready: false,
                    init_state: "service_uninitialized".to_string(),
                    descriptor_trusted: false,
                    descriptor_crc32: String::new(),
                    descriptor_source: String::new(),
                    lane: "moza-r5".to_string(),
                    direct_mode_allowed: false,
                    high_torque_allowed: false,
                    safe_to_send_torque: false,
                    safety_state: "pre_validation".to_string(),
                    safety_reason: "service provided".to_string(),
                }),
            },
            "moza-r5",
        );

        let moza = status.moza.as_ref().ok_or("missing Moza readiness")?;
        assert_eq!(moza.init_state, "service_uninitialized");
        assert_eq!(moza.lane.as_deref(), Some("moza-r5"));
        assert_eq!(moza.safety_reason, "service provided");
        Ok(())
    }

    #[test]
    fn telemetry_data_from_wire() {
        let wire_telemetry = wire::TelemetryData {
            wheel_angle_mdeg: 45_000,  // 45.0 degrees
            wheel_speed_mrad_s: 1_500, // 1.5 rad/s
            temp_c: 42,
            faults: 0,
            hands_on: true,
            sequence: 100,
        };

        let telemetry = TelemetryData::from_wire(wire_telemetry);
        assert!((telemetry.wheel_angle_deg - 45.0).abs() < 0.01);
        assert!((telemetry.wheel_speed_rad_s - 1.5).abs() < 0.01);
        assert_eq!(telemetry.temperature_c, 42);
        assert_eq!(telemetry.fault_flags, 0);
        assert!(telemetry.hands_on);
    }

    #[test]
    fn diagnostic_info_from_wire() {
        let wire_diag = wire::DiagnosticInfo {
            device_id: "dev-1".to_string(),
            system_info: std::collections::BTreeMap::from([(
                "os".to_string(),
                "linux".to_string(),
            )]),
            recent_faults: vec!["fault-1".to_string()],
            performance: Some(wire::PerformanceMetrics {
                p99_jitter_us: 0.15,
                missed_tick_rate: 0.0001,
                total_ticks: 1_000_000,
                missed_ticks: 1,
            }),
        };

        let diag = DiagnosticInfo::from_wire(wire_diag);
        assert_eq!(diag.device_id, "dev-1");
        assert_eq!(
            diag.system_info.get("os").map(|s| s.as_str()),
            Some("linux")
        );
        assert_eq!(diag.recent_faults.len(), 1);
        assert!((diag.performance.p99_jitter_us - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn game_status_from_wire() {
        let wire_status = wire::GameStatus {
            active_game: "iracing".to_string(),
            telemetry_active: true,
            car_id: "gt3".to_string(),
            track_id: "spa".to_string(),
        };

        let status = GameStatus::from_wire(wire_status);
        assert_eq!(status.active_game.as_deref(), Some("iracing"));
        assert!(status.telemetry_active);
        assert_eq!(status.car_id.as_deref(), Some("gt3"));
        assert_eq!(status.track_id.as_deref(), Some("spa"));
    }

    #[test]
    fn game_status_from_wire_empty_strings_become_none() {
        let wire_status = wire::GameStatus {
            active_game: String::new(),
            telemetry_active: false,
            car_id: String::new(),
            track_id: String::new(),
        };

        let status = GameStatus::from_wire(wire_status);
        assert!(status.active_game.is_none());
        assert!(status.car_id.is_none());
        assert!(status.track_id.is_none());
    }

    #[test]
    fn health_event_type_from_wire_covers_all_variants() {
        assert!(matches!(
            HealthEventType::from_wire(1),
            HealthEventType::DeviceConnected
        ));
        assert!(matches!(
            HealthEventType::from_wire(2),
            HealthEventType::DeviceDisconnected
        ));
        assert!(matches!(
            HealthEventType::from_wire(3),
            HealthEventType::FaultDetected
        ));
        assert!(matches!(
            HealthEventType::from_wire(4),
            HealthEventType::FaultCleared
        ));
        assert!(matches!(
            HealthEventType::from_wire(5),
            HealthEventType::PerformanceWarning
        ));
    }

    #[test]
    fn performance_metrics_default() {
        let perf = PerformanceMetrics::default();
        assert!((perf.p99_jitter_us - 0.0).abs() < f32::EPSILON);
        assert!((perf.missed_tick_rate - 0.0).abs() < f32::EPSILON);
        assert_eq!(perf.total_ticks, 0);
        assert_eq!(perf.missed_ticks, 0);
    }
}
