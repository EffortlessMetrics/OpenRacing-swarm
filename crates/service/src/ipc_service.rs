//! IPC service implementation with domain/wire type conversion
//!
//! Ref: [ADR-0002: IPC Transport](file:///h:/Code/Rust/OpenRacing/docs/adr/0002-ipc-transport.md)
//!
//! This module provides the gRPC service implementation that uses the conversion
//! layer to separate domain logic from wire protocol concerns.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::debug;

use racing_wheel_hid_moza_protocol::{
    MOZA_VENDOR_ID, MozaDeviceCategory, identify_device, is_wheelbase_product, product_ids,
};
use racing_wheel_schemas::generated::wheel::v1::{
    ApplyProfileRequest, ConfigureTelemetryRequest, DeviceId as WireDeviceId, DeviceStatus,
    DiagnosticInfo, FeatureNegotiationRequest, FeatureNegotiationResponse, GameStatus, HealthEvent,
    MozaReadinessStatus, OpResult, Profile as WireProfile, ProfileList,
    wheel_service_server::WheelService,
};
use racing_wheel_schemas::ipc_conversion::ConversionError;
use serde_json::Value;

// Import domain services (these will be the real implementations)
use crate::ApplicationProfileService;
use crate::device_service::ApplicationDeviceService;
use crate::game_service::GameService;
use crate::safety_service::ApplicationSafetyService;

/// Health event for internal broadcasting
#[derive(Debug, Clone)]
pub struct HealthEventInternal {
    /// Timestamp when the health event occurred
    pub timestamp: SystemTime,
    /// Device identifier associated with this event
    pub device_id: String,
    /// Event type discriminant (maps to `HealthEventType` proto enum)
    pub event_type: i32,
    /// Human-readable description of the event
    pub message: String,
    /// Arbitrary key-value metadata attached to the event
    pub metadata: HashMap<String, String>,
}

/// IPC service implementation that uses domain services with conversion layer
#[derive(Clone)]
pub struct WheelServiceImpl {
    device_service: Arc<ApplicationDeviceService>,
    profile_service: Arc<ApplicationProfileService>,
    safety_service: Arc<ApplicationSafetyService>,
    game_service: Arc<GameService>,
    health_broadcaster: broadcast::Sender<HealthEventInternal>,
    hardware_lane: Option<String>,
    #[allow(dead_code)]
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClientInfo {
    id: String,
    connected_at: std::time::Instant,
    features: Vec<String>,
    version: String,
}

fn moza_readiness_for_device(
    device: &racing_wheel_schemas::generated::wheel::v1::DeviceInfo,
    hardware_lane: Option<&str>,
) -> Option<MozaReadinessStatus> {
    let vendor_id = u16::try_from(device.vendor_id).ok()?;
    if vendor_id != MOZA_VENDOR_ID {
        return None;
    }

    let product_id = u16::try_from(device.product_id).ok()?;
    let identity = identify_device(product_id);
    let output_capable = is_wheelbase_product(product_id);
    let init_state = if output_capable {
        "uninitialized"
    } else {
        "not_applicable"
    };
    let lane = hardware_lane.unwrap_or_default().to_string();
    let descriptor = moza_lane_descriptor_details(hardware_lane, product_id);
    let lane_stage = moza_lane_verification_stage(hardware_lane);
    let safety_state = moza_safety_state(descriptor.trusted, &lane_stage);
    let safety_reason = moza_safety_reason(descriptor.trusted, &lane_stage);

    Some(MozaReadinessStatus {
        model: identity.name.to_string(),
        product_id: format!("0x{product_id:04X}"),
        category: moza_category_label(identity.category).to_string(),
        output_capable,
        ffb_ready: false,
        init_state: init_state.to_string(),
        descriptor_trusted: descriptor.trusted,
        descriptor_crc32: descriptor.crc32.unwrap_or_default(),
        descriptor_source: descriptor.source.unwrap_or_default(),
        lane,
        direct_mode_allowed: false,
        high_torque_allowed: false,
        safe_to_send_torque: false,
        safety_state,
        safety_reason,
    })
}

#[derive(Debug, Default)]
struct MozaLaneDescriptorDetails {
    trusted: bool,
    crc32: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Default)]
struct MozaLaneVerificationStage {
    passive_success: bool,
    zero_success: bool,
    smoke_ready_success: bool,
    init_off_success: bool,
    init_standard_success: bool,
}

impl MozaLaneVerificationStage {
    fn highest_passing_stage(&self) -> &'static str {
        if self.smoke_ready_success {
            "smoke_ready"
        } else if self.zero_success {
            "zero"
        } else if self.passive_success {
            "passive"
        } else {
            "none"
        }
    }

    fn next_required_stage(&self) -> &'static str {
        if self.smoke_ready_success {
            "none"
        } else if self.zero_success {
            "smoke_ready"
        } else if self.passive_success {
            "zero"
        } else {
            "passive"
        }
    }

    fn ready_for_low_torque_receipts(&self) -> bool {
        self.zero_success && self.init_off_success && self.init_standard_success
    }
}

fn moza_lane_descriptor_details(
    hardware_lane: Option<&str>,
    product_id: u16,
) -> MozaLaneDescriptorDetails {
    let Some(lane) = hardware_lane else {
        return MozaLaneDescriptorDetails::default();
    };
    let Some(descriptor_path) = moza_lane_descriptor_path(lane) else {
        return MozaLaneDescriptorDetails::default();
    };
    let Ok(text) = fs::read_to_string(&descriptor_path) else {
        return MozaLaneDescriptorDetails::default();
    };
    let Ok(receipt) = serde_json::from_str::<Value>(&text) else {
        return MozaLaneDescriptorDetails::default();
    };
    let expected_product_id = format!("0x{product_id:04X}");

    receipt
        .get("devices")
        .and_then(Value::as_array)
        .and_then(|devices| {
            devices.iter().find(|device| {
                json_string(device, "product_id") == Some(expected_product_id.as_str())
            })
        })
        .map(|device| MozaLaneDescriptorDetails {
            trusted: is_trusted_r5_descriptor(device),
            crc32: json_string(device, "report_descriptor_crc32").map(str::to_string),
            source: json_string(device, "descriptor_source").map(str::to_string),
        })
        .unwrap_or_default()
}

fn moza_lane_verification_stage(hardware_lane: Option<&str>) -> MozaLaneVerificationStage {
    let Some(lane) = hardware_lane.and_then(moza_lane_directory_path) else {
        return MozaLaneVerificationStage::default();
    };
    let passive = read_lane_verification_receipt(&lane, "passive-verification.json", "passive");
    let zero = read_lane_verification_receipt(&lane, "zero-verification.json", "zero");
    let smoke_ready =
        read_lane_verification_receipt(&lane, "smoke-ready-verification.json", "smoke_ready");
    let init_off_observed = moza_init_receipt_observed(&lane, "init-off.json", "off");
    let init_standard_observed =
        moza_init_receipt_observed(&lane, "init-standard.json", "standard");

    MozaLaneVerificationStage {
        passive_success: passive
            .as_ref()
            .map(|receipt| receipt.success)
            .unwrap_or(false),
        zero_success: zero
            .as_ref()
            .map(|receipt| receipt.success)
            .unwrap_or(false),
        smoke_ready_success: smoke_ready
            .as_ref()
            .map(|receipt| receipt.success)
            .unwrap_or(false),
        init_off_success: init_off_observed
            || smoke_ready
                .as_ref()
                .map(|receipt| receipt.gate_passed("init_off_handshake"))
                .unwrap_or(false),
        init_standard_success: init_standard_observed
            || smoke_ready
                .as_ref()
                .map(|receipt| receipt.gate_passed("init_standard_handshake"))
                .unwrap_or(false),
    }
}

fn moza_init_receipt_observed(lane: &Path, relative_path: &str, expected_mode: &str) -> bool {
    let Ok(text) = fs::read_to_string(lane.join(relative_path)) else {
        return false;
    };
    let Ok(receipt) = serde_json::from_str::<Value>(&text) else {
        return false;
    };

    json_bool(&receipt, "success") == Some(true)
        && json_string(&receipt, "command") == Some("wheelctl moza init")
        && json_bool(&receipt, "dry_run") == Some(false)
        && json_bool(&receipt, "no_hid_device_opened") == Some(false)
        && json_bool(&receipt, "no_output_reports") == Some(true)
        && json_bool(&receipt, "no_direct_torque_reports") == Some(true)
        && json_bool(&receipt, "no_serial_config_commands") == Some(true)
        && json_bool(&receipt, "no_firmware_or_dfu_commands") == Some(true)
        && json_bool(&receipt, "no_high_torque") == Some(true)
        && json_bool(&receipt, "high_torque") == Some(false)
        && json_string(&receipt, "mode") == Some(expected_mode)
        && json_string(&receipt, "init_state") == Some("ready")
        && json_bool(&receipt, "ready") == Some(true)
        && json_u64(&receipt, "feature_write_errors") == Some(0)
        && json_u64(&receipt, "output_report_attempts") == Some(0)
        && receipt_targets_r5_output_device(&receipt)
        && receipt
            .get("feature_reports")
            .map(|reports| init_feature_reports_are_safe_value(reports, expected_mode))
            .unwrap_or(false)
}

fn receipt_targets_r5_output_device(receipt: &Value) -> bool {
    let Some(device) = receipt.get("device") else {
        return false;
    };
    json_string(device, "vendor_id") == Some("0x346E")
        && matches!(
            json_string(device, "product_id").and_then(parse_hex_u16),
            Some(product_ids::R5_V1 | product_ids::R5_V2)
        )
        && json_bool(device, "output_capable") == Some(true)
}

fn init_feature_reports_are_safe_value(reports: &Value, expected_mode: &str) -> bool {
    let Some(records) = reports.as_array() else {
        return false;
    };
    if records.len() != 2 {
        return false;
    }

    let expected_mode_payload = match expected_mode {
        "off" => "11FF0000",
        "standard" => "11000000",
        _ => return false,
    };

    init_feature_report_record_is_safe(&records[0], 0, "start_input_reports", "0x03", "03000000")
        && init_feature_report_record_is_safe(
            &records[1],
            1,
            "ffb_mode",
            "0x11",
            expected_mode_payload,
        )
}

fn init_feature_report_record_is_safe(
    record: &Value,
    sequence: u64,
    kind: &str,
    report_id: &str,
    payload_hex: &str,
) -> bool {
    json_u64(record, "sequence") == Some(sequence)
        && json_string(record, "kind") == Some(kind)
        && json_string(record, "report_id") == Some(report_id)
        && json_string(record, "payload_hex") == Some(payload_hex)
        && json_string(record, "result") == Some("ok")
        && json_u64(record, "bytes_written") == u64::try_from(payload_hex.len() / 2).ok()
}

#[derive(Debug)]
struct MozaStoredVerificationReceipt {
    success: bool,
    gates: Vec<MozaStoredGate>,
}

impl MozaStoredVerificationReceipt {
    fn gate_passed(&self, name: &str) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.name == name && gate.status == "pass")
    }
}

#[derive(Debug)]
struct MozaStoredGate {
    name: String,
    status: String,
}

fn read_lane_verification_receipt(
    lane: &Path,
    relative_path: &str,
    expected_stage: &str,
) -> Option<MozaStoredVerificationReceipt> {
    let text = fs::read_to_string(lane.join(relative_path)).ok()?;
    let receipt: Value = serde_json::from_str(&text).ok()?;
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza verify-bundle");
    let lane_ok = path_value_matches(lane, json_string(&receipt, "lane"));
    let stage_ok = json_string(&receipt, "requested_stage") == Some(expected_stage);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = json_bool(&receipt, "no_serial_config_commands") == Some(true)
        && json_bool(&receipt, "no_firmware_or_dfu_commands") == Some(true);
    let identity_safe = command_ok
        && lane_ok
        && stage_ok
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope;
    if !identity_safe {
        return None;
    }

    let counts_ok = json_u64(&receipt, "missing_artifacts").unwrap_or(u64::MAX) == 0
        && json_u64(&receipt, "invalid_artifacts").unwrap_or(u64::MAX) == 0
        && json_u64(&receipt, "failed_gates").unwrap_or(u64::MAX) == 0;
    Some(MozaStoredVerificationReceipt {
        success: json_bool(&receipt, "success") == Some(true) && counts_ok,
        gates: receipt
            .get("gates")
            .and_then(Value::as_array)
            .map(|gates| {
                gates
                    .iter()
                    .filter_map(|gate| {
                        Some(MozaStoredGate {
                            name: json_string(gate, "name")?.to_string(),
                            status: json_string(gate, "status")?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn path_value_matches(expected: &Path, recorded: Option<&str>) -> bool {
    let Some(recorded) = recorded.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    if recorded == expected.display().to_string() {
        return true;
    }

    let recorded_path = Path::new(recorded);
    let recorded_canonical = fs::canonicalize(recorded_path).or_else(|_| {
        std::env::current_dir().and_then(|cwd| fs::canonicalize(cwd.join(recorded_path)))
    });
    match (fs::canonicalize(expected), recorded_canonical) {
        (Ok(expected), Ok(recorded)) => expected == recorded,
        _ => false,
    }
}

fn moza_safety_state(descriptor_trusted: bool, stage: &MozaLaneVerificationStage) -> String {
    if stage.smoke_ready_success {
        "lane_smoke_ready_receipts_observed"
    } else if stage.ready_for_low_torque_receipts() {
        "lane_low_torque_gate_receipts_observed"
    } else if stage.zero_success {
        "lane_zero_torque_verified"
    } else if stage.passive_success {
        "lane_passive_verified"
    } else if descriptor_trusted {
        "descriptor_observed_pre_validation"
    } else {
        "pre_validation"
    }
    .to_string()
}

fn moza_safety_reason(descriptor_trusted: bool, stage: &MozaLaneVerificationStage) -> String {
    let highest = stage.highest_passing_stage();
    let next = stage.next_required_stage();
    if highest != "none" {
        return format!(
            "stored Moza lane verification receipts report highest_passing_stage={highest}, next_required_stage={next}; service status remains observe-only and torque output stays disabled until live service initialization is implemented"
        );
    }
    if descriptor_trusted {
        "trusted descriptor metadata observed from lane receipts; torque remains disabled until explicit init, zero, and torque gates pass".to_string()
    } else {
        "service status is observe-only; run explicit Moza lane gates before any output".to_string()
    }
}

fn moza_lane_descriptor_path(lane: &str) -> Option<PathBuf> {
    let path = Path::new(lane);
    if let Some(lane_dir) = moza_lane_directory_path(lane) {
        return Some(lane_dir.join("descriptor.json"));
    }
    if path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some("descriptor.json")
    {
        return Some(path.to_path_buf());
    }
    None
}

fn moza_lane_directory_path(lane: &str) -> Option<PathBuf> {
    let path = Path::new(lane);
    path.is_dir().then(|| path.to_path_buf())
}

fn is_trusted_r5_descriptor(device: &Value) -> bool {
    matches!(
        parse_hex_u16(json_string(device, "vendor_id").unwrap_or_default()),
        Some(MOZA_VENDOR_ID)
    ) && matches!(
        parse_hex_u16(json_string(device, "product_id").unwrap_or_default()),
        Some(product_ids::R5_V1 | product_ids::R5_V2)
    ) && matches!(
        json_string(device, "report_metadata_source"),
        Some("report_descriptor_parsed" | "descriptor_parsed")
    ) && json_string(device, "descriptor_source")
        .map(|source| matches!(source, "linux_sysfs" | "operator_supplied_hex"))
        .unwrap_or(false)
        && json_string(device, "report_descriptor_crc32")
            .map(|crc| crc.starts_with("0x") && crc.len() == 10)
            .unwrap_or(false)
        && json_bool(device, "serial_number_present") == Some(true)
        && json_string(device, "manufacturer")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && json_string(device, "product_name")
            .or_else(|| json_string(device, "product_string"))
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && device
            .get("interface_number")
            .and_then(Value::as_i64)
            .is_some()
        && json_string(device, "usage_page")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && json_string_array_contains_all(device, "input_report_lengths", &["7", "31"])
        && json_string_array_contains_all(device, "output_report_ids", &["0x20"])
        && json_report_record_contains(device, "output_reports", "0x20", 8)
        && json_string_array_contains_all(device, "feature_report_ids", &["0x03", "0x11"])
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn json_string_array_contains_all(value: &Value, key: &str, expected: &[&str]) -> bool {
    let Some(values) = value.get(key).and_then(Value::as_array) else {
        return false;
    };
    expected.iter().all(|needle| {
        values.iter().any(|entry| {
            entry.as_str() == Some(*needle)
                || entry
                    .as_u64()
                    .map(|number| number.to_string() == *needle)
                    .unwrap_or(false)
        })
    })
}

fn json_report_record_contains(value: &Value, key: &str, report_id: &str, report_len: u64) -> bool {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|records| {
            records.iter().any(|record| {
                json_string(record, "report_id") == Some(report_id)
                    && json_u64(record, "report_len") == Some(report_len)
            })
        })
        .unwrap_or(false)
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let stripped = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(stripped, 16).ok()
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

impl WheelServiceImpl {
    /// Create a new IPC service implementation.
    ///
    /// # Arguments
    /// * `device_service` — manages connected device enumeration and status.
    /// * `profile_service` — manages FFB profile CRUD and activation.
    /// * `safety_service` — manages safety interlocks and emergency stop.
    /// * `game_service` — manages game detection and telemetry configuration.
    /// * `health_broadcaster` — broadcast channel for pushing health events to
    ///   all subscribed clients.
    pub fn new(
        device_service: Arc<ApplicationDeviceService>,
        profile_service: Arc<ApplicationProfileService>,
        safety_service: Arc<ApplicationSafetyService>,
        game_service: Arc<GameService>,
        health_broadcaster: broadcast::Sender<HealthEventInternal>,
    ) -> Self {
        Self {
            device_service,
            profile_service,
            safety_service,
            game_service,
            health_broadcaster,
            hardware_lane: None,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Attach an optional hardware validation lane label/path for read-only readiness reporting.
    pub fn with_hardware_lane(mut self, hardware_lane: Option<String>) -> Self {
        self.hardware_lane = hardware_lane;
        self
    }
}

#[async_trait]
impl WheelService for WheelServiceImpl {
    type ListDevicesStream = Pin<
        Box<
            dyn Stream<
                    Item = Result<racing_wheel_schemas::generated::wheel::v1::DeviceInfo, Status>,
                > + Send,
        >,
    >;
    type SubscribeHealthStream = Pin<Box<dyn Stream<Item = Result<HealthEvent, Status>> + Send>>;

    /// Feature negotiation for backward compatibility
    async fn negotiate_features(
        &self,
        request: Request<FeatureNegotiationRequest>,
    ) -> Result<Response<FeatureNegotiationResponse>, Status> {
        let req = request.into_inner();
        debug!(
            "Feature negotiation from client version: {}",
            req.client_version
        );

        // For now, accept all clients with basic compatibility check
        let compatible = is_version_compatible(&req.client_version, "1.0.0");

        let response = FeatureNegotiationResponse {
            server_version: "1.0.0".to_string(),
            supported_features: vec![
                "device_management".to_string(),
                "profile_management".to_string(),
                "safety_control".to_string(),
                "health_monitoring".to_string(),
            ],
            enabled_features: vec![
                "device_management".to_string(),
                "profile_management".to_string(),
                "safety_control".to_string(),
                "health_monitoring".to_string(),
            ],
            compatible,
            min_client_version: "1.0.0".to_string(),
        };

        Ok(Response::new(response))
    }

    /// List all connected devices (streaming)
    async fn list_devices(
        &self,
        _request: Request<()>,
    ) -> Result<Response<Self::ListDevicesStream>, Status> {
        debug!("ListDevices called");

        let device_service = self.device_service.clone();

        let stream = async_stream::stream! {
            match device_service.list_devices().await {
                Ok(devices) => {
                    for device in devices {
                        // Convert engine DeviceInfo to wire DeviceInfo
                        let device_info = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
                            id: device.id.to_string(),
                            name: device.name,
                            r#type: 1, // Default to WheelBase type
                            state: if device.is_connected { 1 } else { 0 },
                            vendor_id: u32::from(device.vendor_id),
                            product_id: u32::from(device.product_id),
                            capabilities: Some(device.capabilities.into()),
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
        request: Request<WireDeviceId>,
    ) -> Result<Response<DeviceStatus>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetDeviceStatus called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.device_service.get_device_status(&device_id).await {
            Ok((device, telemetry)) => {
                // Convert domain types to wire types
                let device_info = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
                    id: device.id.to_string(),
                    name: device.name,
                    r#type: 1, // Default to WheelBase type
                    state: if device.is_connected { 1 } else { 0 },
                    vendor_id: u32::from(device.vendor_id),
                    product_id: u32::from(device.product_id),
                    capabilities: Some(device.capabilities.into()),
                };
                let telemetry_data: Option<
                    racing_wheel_schemas::generated::wheel::v1::TelemetryData,
                > = telemetry.map(|t| {
                    racing_wheel_schemas::generated::wheel::v1::TelemetryData {
                        wheel_angle_mdeg: (t.wheel_angle_deg * 1000.0)
                            .clamp(i32::MIN as f32, i32::MAX as f32)
                            as i32,
                        wheel_speed_mrad_s: (t.wheel_speed_rad_s * 1000.0)
                            .clamp(i32::MIN as f32, i32::MAX as f32)
                            as i32,
                        temp_c: t.temperature_c as u32,
                        faults: t.fault_flags as u32,
                        hands_on: t.hands_on,
                        sequence: 0, // Default sequence number
                    }
                });

                let device_status = DeviceStatus {
                    moza: moza_readiness_for_device(&device_info, self.hardware_lane.as_deref()),
                    device: Some(device_info),
                    last_seen: Some(prost_types::Timestamp {
                        seconds: SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    active_faults: vec![], // Will be populated by device service
                    telemetry: telemetry_data,
                };

                Ok(Response::new(device_status))
            }
            Err(e) => Err(Status::not_found(format!("Device not found: {}", e))),
        }
    }

    /// Get active profile for a device
    async fn get_active_profile(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<WireProfile>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetActiveProfile called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.profile_service.get_active_profile(&device_id).await {
            Ok(Some(profile_id)) => {
                // Load the full profile and convert to wire format
                match self.profile_service.load_profile(profile_id.as_ref()).await {
                    Ok(profile) => {
                        let wire_profile: WireProfile = profile.into();
                        Ok(Response::new(wire_profile))
                    }
                    Err(e) => Err(Status::not_found(format!("Profile not found: {}", e))),
                }
            }
            Ok(None) => Err(Status::not_found("No active profile for device")),
            Err(e) => Err(Status::internal(format!(
                "Failed to get active profile: {}",
                e
            ))),
        }
    }

    /// Apply a profile to a device
    async fn apply_profile(
        &self,
        request: Request<ApplyProfileRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        debug!("ApplyProfile called");

        let device_id_wire = req
            .device
            .ok_or_else(|| Status::invalid_argument("Device ID is required"))?;

        let profile_wire = req
            .profile
            .ok_or_else(|| Status::invalid_argument("Profile is required"))?;

        // Convert wire types to domain types
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        let _profile: racing_wheel_schemas::entities::Profile =
            profile_wire.try_into().map_err(|e: ConversionError| {
                Status::invalid_argument(format!("Invalid profile: {}", e))
            })?;

        // Get device capabilities (simplified for now)
        let max_torque = racing_wheel_schemas::domain::TorqueNm::new(10.0)
            .map_err(|e| Status::internal(format!("invalid max torque: {}", e)))?;
        let device_capabilities = racing_wheel_schemas::entities::DeviceCapabilities::new(
            true, true, true, true, max_torque, 1024, 1000,
        );

        match self
            .profile_service
            .apply_profile_to_device(&device_id, None, None, None, &device_capabilities)
            .await
        {
            Ok(_profile) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to apply profile: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// List all available profiles
    async fn list_profiles(&self, _request: Request<()>) -> Result<Response<ProfileList>, Status> {
        debug!("ListProfiles called");

        match self.profile_service.list_profiles().await {
            Ok(profiles) => {
                // Convert domain Profiles to wire Profiles
                let wire_profiles: Vec<WireProfile> =
                    profiles.into_iter().map(Into::into).collect();

                Ok(Response::new(ProfileList {
                    profiles: wire_profiles,
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to list profiles: {}", e))),
        }
    }

    /// Start high torque mode
    async fn start_high_torque(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id_wire = request.into_inner();
        debug!("StartHighTorque called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.safety_service.start_high_torque(&device_id).await {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to start high torque: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Emergency stop
    async fn emergency_stop(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id_wire = request.into_inner();
        debug!("EmergencyStop called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self
            .safety_service
            .emergency_stop(&device_id, "IPC request".to_string())
            .await
        {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to emergency stop: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Subscribe to health events
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
                        seconds: event.timestamp
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    device_id: event.device_id,
                    r#type: event.event_type,
                    message: event.message,
                    metadata: event.metadata.into_iter().collect(),
                };
                yield Ok(health_event);
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Get diagnostics
    async fn get_diagnostics(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<DiagnosticInfo>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetDiagnostics called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        // For now, return basic diagnostic info
        // This will be enhanced when the diagnostic service is implemented
        let diagnostic_info = DiagnosticInfo {
            device_id: device_id.to_string(),
            system_info: BTreeMap::new(),
            recent_faults: vec![],
            performance: Some(
                racing_wheel_schemas::generated::wheel::v1::PerformanceMetrics {
                    p99_jitter_us: 0.0,
                    missed_tick_rate: 0.0,
                    total_ticks: 0,
                    missed_ticks: 0,
                },
            ),
        };

        Ok(Response::new(diagnostic_info))
    }

    /// Configure telemetry
    async fn configure_telemetry(
        &self,
        request: Request<ConfigureTelemetryRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        debug!("ConfigureTelemetry called for game: {}", req.game_id);

        use std::path::Path;
        match self
            .game_service
            .configure_telemetry(&req.game_id, Path::new(&req.install_path))
            .await
        {
            Ok(_config_diffs) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to configure telemetry: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Get game status
    async fn get_game_status(&self, _request: Request<()>) -> Result<Response<GameStatus>, Status> {
        debug!("GetGameStatus called");

        match self.game_service.get_game_status().await {
            Ok(status) => {
                let game_status = GameStatus {
                    active_game: status.active_game.unwrap_or_default(),
                    telemetry_active: status.telemetry_active,
                    car_id: status.car_id.unwrap_or_default(),
                    track_id: status.track_id.unwrap_or_default(),
                };
                Ok(Response::new(game_status))
            }
            Err(e) => Err(Status::internal(format!(
                "Failed to get game status: {}",
                e
            ))),
        }
    }
}

/// Check if client version is compatible with minimum required version
fn is_version_compatible(client_version: &str, min_version: &str) -> bool {
    // Simplified semantic version comparison
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_repository::ProfileRepositoryConfig;
    use racing_wheel_engine::{
        DeviceEvent, DeviceHealthStatus, DeviceInfo as EngineDeviceInfo, HidDevice, HidPort,
        RTResult, SafetyPolicy, TelemetryData, VirtualDevice, VirtualHidPort,
    };
    use racing_wheel_schemas::generated::wheel::v1::wheel_service_server::WheelService;
    use racing_wheel_schemas::prelude::{DeviceCapabilities, DeviceId, TorqueNm};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;

    #[derive(Clone)]
    struct StaticMozaHidPort {
        device_info: EngineDeviceInfo,
    }

    impl StaticMozaHidPort {
        fn r5_v2(id: &str) -> anyhow::Result<Self> {
            let device_id: DeviceId = id.parse()?;
            let capabilities = moza_r5_capabilities()?;
            Ok(Self {
                device_info: EngineDeviceInfo {
                    id: device_id,
                    name: "Moza R5".to_string(),
                    vendor_id: MOZA_VENDOR_ID,
                    product_id: product_ids::R5_V2,
                    serial_number: Some("TEST-R5".to_string()),
                    manufacturer: Some("Moza Racing".to_string()),
                    path: "virtual://moza-r5".to_string(),
                    capabilities,
                    is_connected: true,
                },
            })
        }
    }

    #[async_trait]
    impl HidPort for StaticMozaHidPort {
        async fn list_devices(&self) -> Result<Vec<EngineDeviceInfo>, Box<dyn std::error::Error>> {
            Ok(vec![self.device_info.clone()])
        }

        async fn open_device(
            &self,
            id: &DeviceId,
        ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
            if id != &self.device_info.id {
                let error = std::io::Error::new(std::io::ErrorKind::NotFound, "device not found");
                return Err(Box::new(error));
            }
            Ok(Box::new(StaticMozaHidDevice {
                device_info: self.device_info.clone(),
            }))
        }

        async fn monitor_devices(
            &self,
        ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
            let (_tx, rx) = mpsc::channel(1);
            Ok(rx)
        }

        async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    struct StaticMozaHidDevice {
        device_info: EngineDeviceInfo,
    }

    impl HidDevice for StaticMozaHidDevice {
        fn write_ffb_report(&mut self, _torque_nm: f32, _seq: u16) -> RTResult {
            Ok(())
        }

        fn read_telemetry(&mut self) -> Option<TelemetryData> {
            None
        }

        fn capabilities(&self) -> &DeviceCapabilities {
            &self.device_info.capabilities
        }

        fn device_info(&self) -> &EngineDeviceInfo {
            &self.device_info
        }

        fn is_connected(&self) -> bool {
            self.device_info.is_connected
        }

        fn health_status(&self) -> DeviceHealthStatus {
            DeviceHealthStatus {
                temperature_c: 25,
                fault_flags: 0,
                hands_on: false,
                last_communication: std::time::Instant::now(),
                communication_errors: 0,
            }
        }
    }

    fn moza_r5_capabilities() -> anyhow::Result<DeviceCapabilities> {
        Ok(DeviceCapabilities::new(
            false,
            true,
            true,
            false,
            TorqueNm::new(5.5)?,
            10000,
            1000,
        ))
    }

    fn write_moza_descriptor_receipt(path: &Path) -> anyhow::Result<()> {
        fs::write(
            path.join("descriptor.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "success": true,
                "devices": [{
                    "vendor_id": "0x346E",
                    "product_id": "0x0014",
                    "product_name": "Moza R5",
                    "manufacturer": "Moza Racing",
                    "serial_number_present": true,
                    "interface_number": 0,
                    "usage_page": "0x0001",
                    "descriptor_source": "operator_supplied_hex",
                    "report_metadata_source": "report_descriptor_parsed",
                    "report_descriptor_crc32": "0x12345678",
                    "input_report_lengths": [7, 31],
                    "output_report_ids": ["0x20"],
                    "output_reports": [{"report_id": "0x20", "report_len": 8}],
                    "feature_report_ids": ["0x03", "0x11"]
                }]
            }))?,
        )?;
        Ok(())
    }

    fn moza_verification_receipt(
        lane: &Path,
        stage: &str,
        success: bool,
        failed_gates: u64,
        gates: Value,
    ) -> Value {
        serde_json::json!({
            "success": success,
            "command": "wheelctl moza verify-bundle",
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "lane": lane.display().to_string(),
            "requested_stage": stage,
            "missing_artifacts": 0,
            "invalid_artifacts": 0,
            "failed_gates": failed_gates,
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "artifacts": [],
            "gates": gates
        })
    }

    fn moza_init_feature_reports(mode_payload: &str) -> Value {
        serde_json::json!([
            {
                "sequence": 0,
                "kind": "start_input_reports",
                "report_id": "0x03",
                "payload_hex": "03000000",
                "result": "ok",
                "bytes_written": 4
            },
            {
                "sequence": 1,
                "kind": "ffb_mode",
                "report_id": "0x11",
                "payload_hex": mode_payload,
                "result": "ok",
                "bytes_written": 4
            }
        ])
    }

    fn moza_init_receipt(mode: &str) -> Value {
        let mode_payload = if mode == "off" {
            "11FF0000"
        } else {
            "11000000"
        };
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza init",
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "dry_run": false,
            "mode": mode,
            "init_state": "ready",
            "ready": true,
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "no_hid_device_opened": false,
            "no_output_reports": true,
            "no_direct_torque_reports": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "no_high_torque": true,
            "high_torque": false,
            "feature_report_count": 2,
            "feature_write_errors": 0,
            "output_report_attempts": 0,
            "feature_reports": moza_init_feature_reports(mode_payload)
        })
    }

    fn write_moza_stage_receipts(path: &Path) -> anyhow::Result<()> {
        fs::write(
            path.join("passive-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                path,
                "passive",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        fs::write(
            path.join("zero-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                path,
                "zero",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        fs::write(
            path.join("init-off.json"),
            serde_json::to_string_pretty(&moza_init_receipt("off"))?,
        )?;
        fs::write(
            path.join("init-standard.json"),
            serde_json::to_string_pretty(&moza_init_receipt("standard"))?,
        )?;
        fs::write(
            path.join("smoke-ready-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                path,
                "smoke_ready",
                false,
                1,
                serde_json::json!([
                    {"name": "low_torque_real_hardware", "status": "fail"}
                ]),
            ))?,
        )?;
        Ok(())
    }

    #[tokio::test]
    async fn list_devices_preserves_usb_vid_pid() -> anyhow::Result<()> {
        let mut port = VirtualHidPort::new();
        let device_id: DeviceId = "ipc-identity-wheel".parse()?;
        port.add_device(VirtualDevice::new(
            device_id,
            "IPC identity wheel".to_string(),
        ))
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

        let device_service = Arc::new(ApplicationDeviceService::new(Arc::new(port), None).await?);
        let profile_dir = TempDir::new()?;
        let profile_service = Arc::new(
            ApplicationProfileService::new_with_config(ProfileRepositoryConfig {
                profiles_dir: profile_dir.path().to_path_buf(),
                trusted_keys: Vec::new(),
                auto_migrate: true,
                backup_on_migrate: false,
            })
            .await?,
        );
        let safety_service =
            Arc::new(ApplicationSafetyService::new(SafetyPolicy::default(), None).await?);
        let game_service = Arc::new(GameService::new().await?);
        let (health_tx, _) = broadcast::channel(8);
        let service = WheelServiceImpl::new(
            device_service,
            profile_service,
            safety_service,
            game_service,
            health_tx,
        );

        let response = service.list_devices(Request::new(())).await?;
        let mut stream = response.into_inner();
        let first = stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("expected one streamed device"))??;

        assert_eq!(first.vendor_id, 0x1234);
        assert_eq!(first.product_id, 0x5678);
        Ok(())
    }

    #[tokio::test]
    async fn get_device_status_reports_moza_hardware_lane_through_ipc_service() -> anyhow::Result<()>
    {
        let lane_dir = TempDir::new()?;
        write_moza_descriptor_receipt(lane_dir.path())?;
        write_moza_stage_receipts(lane_dir.path())?;
        let lane = lane_dir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("expected UTF-8 temp path"))?
            .to_string();

        let device_id = "moza-r5";
        let port = StaticMozaHidPort::r5_v2(device_id)?;
        let device_service = Arc::new(ApplicationDeviceService::new(Arc::new(port), None).await?);
        device_service.enumerate_devices().await?;

        let profile_dir = TempDir::new()?;
        let profile_service = Arc::new(
            ApplicationProfileService::new_with_config(ProfileRepositoryConfig {
                profiles_dir: profile_dir.path().to_path_buf(),
                trusted_keys: Vec::new(),
                auto_migrate: true,
                backup_on_migrate: false,
            })
            .await?,
        );
        let safety_service =
            Arc::new(ApplicationSafetyService::new(SafetyPolicy::default(), None).await?);
        let game_service = Arc::new(GameService::new().await?);
        let (health_tx, _) = broadcast::channel(8);
        let service = WheelServiceImpl::new(
            device_service,
            profile_service,
            safety_service,
            game_service,
            health_tx,
        )
        .with_hardware_lane(Some(lane.clone()));

        let response = WheelService::get_device_status(
            &service,
            Request::new(WireDeviceId {
                id: device_id.to_string(),
            }),
        )
        .await?;
        let status = response.into_inner();
        let device = status
            .device
            .ok_or_else(|| anyhow::anyhow!("missing status device"))?;
        let moza = status
            .moza
            .ok_or_else(|| anyhow::anyhow!("missing Moza readiness"))?;

        assert_eq!(device.vendor_id, u32::from(MOZA_VENDOR_ID));
        assert_eq!(device.product_id, u32::from(product_ids::R5_V2));
        assert_eq!(moza.model, "Moza R5");
        assert_eq!(moza.product_id, "0x0014");
        assert_eq!(moza.category, "wheelbase");
        assert_eq!(moza.lane, lane);
        assert!(moza.output_capable);
        assert!(moza.descriptor_trusted);
        assert_eq!(moza.descriptor_crc32, "0x12345678");
        assert_eq!(moza.descriptor_source, "operator_supplied_hex");
        assert_eq!(moza.safety_state, "lane_low_torque_gate_receipts_observed");
        assert!(moza.safety_reason.contains("highest_passing_stage=zero"));
        assert!(
            moza.safety_reason
                .contains("next_required_stage=smoke_ready")
        );
        assert!(!moza.ffb_ready);
        assert!(!moza.direct_mode_allowed);
        assert!(!moza.high_torque_allowed);
        assert!(!moza.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn moza_readiness_for_device_is_conservative_and_lane_scoped() -> anyhow::Result<()> {
        let device = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: u32::from(MOZA_VENDOR_ID),
            product_id: 0x0014,
        };

        let readiness = moza_readiness_for_device(&device, Some("moza-r5"))
            .ok_or_else(|| anyhow::anyhow!("missing readiness"))?;

        assert_eq!(readiness.model, "Moza R5");
        assert_eq!(readiness.product_id, "0x0014");
        assert_eq!(readiness.category, "wheelbase");
        assert_eq!(readiness.lane, "moza-r5");
        assert!(readiness.output_capable);
        assert!(!readiness.ffb_ready);
        assert!(!readiness.descriptor_trusted);
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn moza_readiness_reads_descriptor_metadata_from_lane_path() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let descriptor_path = temp_dir.path().join("descriptor.json");
        fs::write(
            &descriptor_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "success": true,
                "devices": [{
                    "vendor_id": "0x346E",
                    "product_id": "0x0014",
                    "product_name": "Moza R5",
                    "manufacturer": "Moza Racing",
                    "serial_number_present": true,
                    "interface_number": 0,
                    "usage_page": "0x0001",
                    "descriptor_source": "operator_supplied_hex",
                    "report_metadata_source": "report_descriptor_parsed",
                    "report_descriptor_crc32": "0x12345678",
                    "input_report_lengths": [7, 31],
                    "output_report_ids": ["0x20"],
                    "output_reports": [{"report_id": "0x20", "report_len": 8}],
                    "feature_report_ids": ["0x03", "0x11"]
                }]
            }))?,
        )?;
        let lane = temp_dir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("expected UTF-8 temp path"))?;
        let device = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: u32::from(MOZA_VENDOR_ID),
            product_id: 0x0014,
        };

        let readiness = moza_readiness_for_device(&device, Some(lane))
            .ok_or_else(|| anyhow::anyhow!("missing readiness"))?;

        assert_eq!(readiness.lane, lane);
        assert!(readiness.descriptor_trusted);
        assert_eq!(readiness.descriptor_crc32, "0x12345678");
        assert_eq!(readiness.descriptor_source, "operator_supplied_hex");
        assert_eq!(readiness.safety_state, "descriptor_observed_pre_validation");
        assert!(!readiness.ffb_ready);
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn moza_readiness_reports_lane_verification_stage_without_enabling_torque() -> anyhow::Result<()>
    {
        let temp_dir = TempDir::new()?;
        fs::write(
            temp_dir.path().join("passive-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                temp_dir.path(),
                "passive",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        fs::write(
            temp_dir.path().join("zero-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                temp_dir.path(),
                "zero",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        fs::write(
            temp_dir.path().join("init-off.json"),
            serde_json::to_string_pretty(&moza_init_receipt("off"))?,
        )?;
        fs::write(
            temp_dir.path().join("init-standard.json"),
            serde_json::to_string_pretty(&moza_init_receipt("standard"))?,
        )?;
        fs::write(
            temp_dir.path().join("smoke-ready-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                temp_dir.path(),
                "smoke_ready",
                false,
                1,
                serde_json::json!([
                    {"name": "low_torque_real_hardware", "status": "fail"}
                ]),
            ))?,
        )?;
        let lane = temp_dir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("expected UTF-8 temp path"))?;
        let device = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: u32::from(MOZA_VENDOR_ID),
            product_id: 0x0014,
        };

        let readiness = moza_readiness_for_device(&device, Some(lane))
            .ok_or_else(|| anyhow::anyhow!("missing readiness"))?;

        assert_eq!(
            readiness.safety_state,
            "lane_low_torque_gate_receipts_observed"
        );
        assert!(
            readiness
                .safety_reason
                .contains("highest_passing_stage=zero")
        );
        assert!(
            readiness
                .safety_reason
                .contains("next_required_stage=smoke_ready")
        );
        assert!(!readiness.ffb_ready);
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn moza_readiness_requires_init_receipts_before_low_torque_stage() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(
            temp_dir.path().join("passive-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                temp_dir.path(),
                "passive",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        fs::write(
            temp_dir.path().join("zero-verification.json"),
            serde_json::to_string_pretty(&moza_verification_receipt(
                temp_dir.path(),
                "zero",
                true,
                0,
                serde_json::json!([]),
            ))?,
        )?;
        let lane = temp_dir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("expected UTF-8 temp path"))?;
        let device = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: u32::from(MOZA_VENDOR_ID),
            product_id: 0x0014,
        };

        let readiness = moza_readiness_for_device(&device, Some(lane))
            .ok_or_else(|| anyhow::anyhow!("missing readiness"))?;

        assert_eq!(readiness.safety_state, "lane_zero_torque_verified");
        assert!(!readiness.ffb_ready);
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn moza_readiness_ignores_untrusted_lane_verification_stage() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(
            temp_dir.path().join("zero-verification.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "success": true,
                "requested_stage": "zero",
                "missing_artifacts": 0,
                "invalid_artifacts": 0,
                "failed_gates": 0,
                "gates": []
            }))?,
        )?;
        let lane = temp_dir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("expected UTF-8 temp path"))?;
        let device = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            r#type: 1,
            capabilities: None,
            state: 1,
            vendor_id: u32::from(MOZA_VENDOR_ID),
            product_id: 0x0014,
        };

        let readiness = moza_readiness_for_device(&device, Some(lane))
            .ok_or_else(|| anyhow::anyhow!("missing readiness"))?;

        assert_eq!(readiness.safety_state, "pre_validation");
        assert!(!readiness.ffb_ready);
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn test_version_compatibility_exact_match() {
        assert!(is_version_compatible("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_minor() {
        assert!(is_version_compatible("1.1.0", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_patch() {
        assert!(is_version_compatible("1.0.1", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_lower_major() {
        assert!(!is_version_compatible("0.9.0", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_major() {
        assert!(!is_version_compatible("2.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_minor_and_patch() {
        assert!(is_version_compatible("1.5.3", "1.2.1"));
    }

    #[test]
    fn test_version_compatibility_lower_minor() {
        assert!(!is_version_compatible("1.0.0", "1.1.0"));
    }

    #[test]
    fn test_version_compatibility_same_minor_lower_patch() {
        assert!(!is_version_compatible("1.0.0", "1.0.1"));
    }

    #[test]
    fn test_version_compatibility_empty_strings() {
        assert!(!is_version_compatible("", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", ""));
        assert!(!is_version_compatible("", ""));
    }

    #[test]
    fn test_version_compatibility_single_component() {
        assert!(!is_version_compatible("1", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "1"));
    }

    #[test]
    fn test_version_compatibility_two_components() {
        assert!(!is_version_compatible("1.0", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "1.0"));
    }

    #[test]
    fn test_version_compatibility_non_numeric() {
        // Non-numeric components parse as 0, which may cause a false match
        // Just verify it doesn't panic
        let _ = is_version_compatible("abc.def.ghi", "1.0.0");
    }

    #[test]
    fn test_version_compatibility_extra_components_ignored() {
        // Extra components beyond the 3rd are ignored; first three must still match
        assert!(is_version_compatible("1.0.0.beta", "1.0.0"));
    }

    #[tokio::test]
    async fn test_device_id_conversion() {
        // Test that invalid device IDs are properly rejected
        let invalid_device_id = WireDeviceId {
            id: "".to_string(), // Empty string should be invalid
        };

        let result: Result<racing_wheel_schemas::domain::DeviceId, _> =
            invalid_device_id.id.parse();
        assert!(result.is_err());
    }
}
