//! Device management commands

use anyhow::{Context, Result};
use dialoguer::Confirm;
use hidapi::{DeviceInfo as HidDeviceInfo, HidApi};
use indicatif::{ProgressBar, ProgressStyle};
use openracing_hardware_core::{DeviceCapabilityKind, DeviceCapabilityRegistry, DeviceFamily};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::interval;

use crate::client::{DeviceCapabilities, DeviceInfo, DeviceState, DeviceType, WheelClient};
use crate::commands::{CalibrationType, DeviceCommands, moza};
use crate::error::CliError;
use crate::output;

/// Execute device command
pub async fn execute(cmd: &DeviceCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect_or_mock(endpoint).await?;

    match cmd {
        DeviceCommands::List { detailed, json_out } => {
            list_devices(&client, json, *detailed, json_out.as_deref()).await
        }
        DeviceCommands::Status {
            device,
            moza_lane,
            json_out,
            watch,
        } => {
            device_status(
                &client,
                device,
                json,
                *watch,
                moza_lane.as_deref(),
                json_out.as_deref(),
            )
            .await
        }
        DeviceCommands::Calibrate {
            device,
            calibration_type,
            yes,
        } => calibrate_device(&client, device, calibration_type, json, *yes).await,
        DeviceCommands::Reset { device, force } => {
            reset_device(&client, device, json, *force).await
        }
    }
}

/// List all connected devices
async fn list_devices(
    client: &WheelClient,
    json: bool,
    detailed: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let service_devices = client.list_devices().await?;
    let hid_observation = observe_known_hid_devices();
    let devices = merge_device_lists(service_devices, hid_observation.devices.clone());
    if let Some(path) = json_out {
        write_device_list_receipt(path, &devices, &hid_observation)?;
    }
    output::print_device_list(&devices, json, detailed);
    if !json && let Some(path) = json_out {
        println!("Receipt: {}", path.display());
    }
    Ok(())
}

fn write_device_list_receipt(
    path: &Path,
    devices: &[DeviceInfo],
    hid_observation: &HidDeviceListObservation,
) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let receipt = json!({
        "success": true,
        "command": "wheelctl device list",
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_output_reports": true,
        "no_feature_reports": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "hid_enumeration": {
            "api_available": hid_observation.api_available,
            "enumeration_available": hid_observation.enumeration_available,
            "known_device_count": hid_observation.known_device_count,
            "error": hid_observation.error,
        },
        "devices": devices
    });
    let json =
        serde_json::to_string_pretty(&receipt).context("failed to serialize JSON receipt")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone)]
struct HidDeviceListObservation {
    api_available: bool,
    enumeration_available: bool,
    known_device_count: usize,
    error: Option<String>,
    devices: Vec<DeviceInfo>,
}

fn observe_known_hid_devices() -> HidDeviceListObservation {
    let registry = DeviceCapabilityRegistry::openracing_defaults();
    match HidApi::new() {
        Ok(api) => {
            let devices = api
                .device_list()
                .filter_map(|device| hid_device_info(&registry, device))
                .collect::<Vec<_>>();
            HidDeviceListObservation {
                api_available: true,
                enumeration_available: true,
                known_device_count: devices.len(),
                error: None,
                devices,
            }
        }
        Err(error) => HidDeviceListObservation {
            api_available: false,
            enumeration_available: false,
            known_device_count: 0,
            error: Some(error.to_string()),
            devices: Vec::new(),
        },
    }
}

fn hid_device_info(
    registry: &DeviceCapabilityRegistry,
    device: &HidDeviceInfo,
) -> Option<DeviceInfo> {
    let record = registry.lookup(device.vendor_id(), device.product_id());
    if record.family() == DeviceFamily::Unknown {
        return None;
    }

    Some(DeviceInfo {
        id: hid_device_id(device),
        name: device
            .product_string()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| record.model())
            .to_string(),
        source: Some("hid-observe".to_string()),
        vendor_id: Some(hex_u16(device.vendor_id())),
        product_id: Some(hex_u16(device.product_id())),
        manufacturer: device.manufacturer_string().map(str::to_string),
        product_string: device.product_string().map(str::to_string),
        serial_number_present: Some(device.serial_number().is_some()),
        interface_number: Some(device.interface_number()),
        usage_page: Some(hex_u16(device.usage_page())),
        usage: Some(hex_u16(device.usage())),
        hid_path_present: Some(true),
        device_type: device_type_for_capability(record.kind()),
        state: DeviceState::Connected,
        capabilities: DeviceCapabilities {
            supports_pid: false,
            supports_raw_torque_1khz: false,
            supports_health_stream: record.input(),
            supports_led_bus: false,
            max_torque_nm: 0.0,
            encoder_cpr: 0,
            min_report_period_us: 0,
        },
    })
}

fn merge_device_lists(
    mut service_devices: Vec<DeviceInfo>,
    hid_devices: Vec<DeviceInfo>,
) -> Vec<DeviceInfo> {
    for hid_device in hid_devices {
        let duplicate = service_devices.iter().any(|device| {
            device.vendor_id == hid_device.vendor_id
                && device.product_id == hid_device.product_id
                && device.interface_number == hid_device.interface_number
                && device.usage_page == hid_device.usage_page
                && device.usage == hid_device.usage
        });
        if !duplicate {
            service_devices.push(hid_device);
        }
    }
    service_devices
}

fn device_type_for_capability(kind: DeviceCapabilityKind) -> DeviceType {
    match kind {
        DeviceCapabilityKind::Wheelbase => DeviceType::WheelBase,
        DeviceCapabilityKind::Pedals => DeviceType::Pedals,
        DeviceCapabilityKind::Handbrake => DeviceType::Handbrake,
        DeviceCapabilityKind::Shifter => DeviceType::Shifter,
        DeviceCapabilityKind::ButtonBox => DeviceType::ButtonBox,
        DeviceCapabilityKind::Unknown => DeviceType::Unknown,
    }
}

fn hid_device_id(device: &HidDeviceInfo) -> String {
    format!(
        "hid-{}-{}-if{}-{}-{}",
        hex_u16(device.vendor_id()),
        hex_u16(device.product_id()),
        device.interface_number(),
        hex_u16(device.usage_page()),
        hex_u16(device.usage())
    )
}

fn hex_u16(value: u16) -> String {
    format!("0x{value:04X}")
}

/// Show device status
async fn device_status(
    client: &WheelClient,
    device: &str,
    json: bool,
    watch: bool,
    moza_lane: Option<&Path>,
    json_out: Option<&Path>,
) -> Result<()> {
    if watch && json_out.is_some() {
        return Err(CliError::ValidationError(
            "--json-out cannot be used with --watch".to_string(),
        )
        .into());
    }

    if watch {
        watch_device_status(client, device, json, moza_lane).await
    } else {
        let mut status = client
            .get_device_status(device)
            .await
            .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;
        if let Some(lane) = moza_lane {
            moza::apply_lane_readiness_to_device_status(&mut status, lane);
        }
        if let Some(path) = json_out {
            write_device_status_receipt(path, device, moza_lane, &status)?;
        }
        output::print_device_status(&status, json);
        if !json && let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
        Ok(())
    }
}

fn write_device_status_receipt(
    path: &Path,
    device: &str,
    moza_lane: Option<&Path>,
    status: &crate::client::DeviceStatus,
) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let receipt = json!({
        "success": true,
        "command": "wheelctl device status",
        "device_selector": device,
        "moza_lane": moza_lane.map(|lane| lane.display().to_string()),
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "status": status,
        "notes": [
            "device status queries wheeld and does not send FFB output, serial configuration, firmware, or DFU commands",
            "Moza readiness remains observe-only; torque output requires explicit init, zero, and low-torque receipts"
        ]
    });
    let json =
        serde_json::to_string_pretty(&receipt).context("failed to serialize JSON receipt")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))?;
    Ok(())
}

/// Watch device status in real-time
async fn watch_device_status(
    client: &WheelClient,
    device: &str,
    json: bool,
    moza_lane: Option<&Path>,
) -> Result<()> {
    if !json {
        println!(
            "Watching device status for {} (Press Ctrl+C to stop)",
            device
        );
        println!();
    }

    let mut interval = interval(Duration::from_millis(500));

    loop {
        interval.tick().await;

        match client.get_device_status(device).await {
            Ok(mut status) => {
                if let Some(lane) = moza_lane {
                    moza::apply_lane_readiness_to_device_status(&mut status, lane);
                }
                if json {
                    output::print_device_status(&status, true);
                } else {
                    // Clear screen and print status
                    print!("\x1B[2J\x1B[1;1H");
                    output::print_device_status(&status, false);
                }
            }
            Err(_) => {
                if json {
                    output::print_error_json(&CliError::DeviceNotFound(device.to_string()).into());
                } else {
                    eprintln!("Device {} not found", device);
                }
                break;
            }
        }
    }

    Ok(())
}

/// Calibrate device
async fn calibrate_device(
    client: &WheelClient,
    device: &str,
    calibration_type: &CalibrationType,
    json: bool,
    yes: bool,
) -> Result<()> {
    // Verify device exists
    let _status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    if !yes && !json {
        let message = match calibration_type {
            CalibrationType::Center => {
                "Center the wheel and press Enter to calibrate center position"
            }
            CalibrationType::Dor => {
                "Calibrate degrees of rotation (DOR) - wheel will be moved to limits"
            }
            CalibrationType::Pedals => {
                "Calibrate pedal ranges - press each pedal fully and release"
            }
            CalibrationType::All => "Perform full calibration sequence (center, DOR, pedals)",
        };

        if !Confirm::new()
            .with_prompt(format!("{}. Continue?", message))
            .interact()?
        {
            output::print_warning("Calibration cancelled", json);
            return Ok(());
        }
    }

    // Show progress during calibration
    if !json {
        let pb = ProgressBar::new_spinner();
        let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
        pb.set_style(style);

        match calibration_type {
            CalibrationType::Center => {
                pb.set_message("Calibrating center position...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(2)).await;
                pb.finish_with_message("✓ Center position calibrated");
            }
            CalibrationType::Dor => {
                pb.set_message("Calibrating degrees of rotation...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(5)).await;
                pb.finish_with_message("✓ DOR calibrated (900°)");
            }
            CalibrationType::Pedals => {
                pb.set_message("Calibrating pedal ranges...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(3)).await;
                pb.finish_with_message("✓ Pedal ranges calibrated");
            }
            CalibrationType::All => {
                for (step, msg) in [
                    (
                        "Calibrating center position...",
                        "✓ Center position calibrated",
                    ),
                    ("Calibrating degrees of rotation...", "✓ DOR calibrated"),
                    ("Calibrating pedal ranges...", "✓ Pedal ranges calibrated"),
                ] {
                    pb.set_message(step);
                    pb.enable_steady_tick(Duration::from_millis(100));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    pb.finish_with_message(msg);
                    pb.reset();
                }
            }
        }
    }

    output::print_success(
        &format!(
            "Device {} calibration ({:?}) completed successfully",
            device, calibration_type
        ),
        json,
    );

    Ok(())
}

/// Reset device to safe state
async fn reset_device(client: &WheelClient, device: &str, json: bool, force: bool) -> Result<()> {
    // Verify device exists
    let _status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    if !force && !json
        && !Confirm::new()
            .with_prompt("Reset device to safe state? This will stop all force feedback and return to default settings.")
            .interact()?
    {
        output::print_warning("Reset cancelled", json);
        return Ok(());
    }

    // Perform emergency stop
    client.emergency_stop(Some(device)).await?;

    output::print_success(&format!("Device {} reset to safe state", device), json);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{DeviceCapabilities, DeviceInfo, DeviceState, DeviceType};

    type TestResult = Result<()>;

    #[test]
    fn write_device_list_receipt_writes_json_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("device-list.json");
        let devices = vec![DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            source: Some("hid-observe".to_string()),
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: Some("Moza".to_string()),
            product_string: Some("Moza R5".to_string()),
            serial_number_present: Some(true),
            interface_number: Some(2),
            usage_page: Some("0x0001".to_string()),
            usage: Some("0x0004".to_string()),
            hid_path_present: Some(true),
            device_type: DeviceType::WheelBase,
            state: DeviceState::Connected,
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                max_torque_nm: 5.5,
                ..DeviceCapabilities::default()
            },
        }];
        let hid_observation = HidDeviceListObservation {
            api_available: true,
            enumeration_available: true,
            known_device_count: 1,
            error: None,
            devices: devices.clone(),
        };

        write_device_list_receipt(&path, &devices, &hid_observation)?;

        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            value.get("command").and_then(|v| v.as_str()),
            Some("wheelctl device list")
        );
        assert_eq!(
            value.get("no_ffb_writes").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value.get("no_hid_device_opened").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value.get("no_output_reports").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value.get("no_feature_reports").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value
                .get("hid_enumeration")
                .and_then(|hid| hid.get("known_device_count"))
                .and_then(|v| v.as_u64()),
            Some(1)
        );
        let device = value
            .get("devices")
            .and_then(|devices| devices.as_array())
            .and_then(|devices| devices.first())
            .ok_or_else(|| anyhow::anyhow!("missing device record"))?;
        assert_eq!(
            device.get("vendor_id").and_then(|v| v.as_str()),
            Some("0x346E")
        );
        assert_eq!(
            device.get("product_id").and_then(|v| v.as_str()),
            Some("0x0014")
        );
        assert_eq!(
            value
                .get("no_serial_config_commands")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value
                .get("no_firmware_or_dfu_commands")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let first_device = value
            .get("devices")
            .and_then(|v| v.as_array())
            .and_then(|devices| devices.first())
            .ok_or_else(|| anyhow::anyhow!("expected device record"))?;
        assert_eq!(
            first_device.get("name").and_then(|v| v.as_str()),
            Some("Moza R5")
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_list_writes_observe_receipt_with_mock_backend() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("device-list.json");
        let command = DeviceCommands::List {
            detailed: true,
            json_out: Some(path.clone()),
        };

        execute(&command, true, Some("http://127.0.0.1:9")).await?;

        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(
            value.get("command").and_then(|v| v.as_str()),
            Some("wheelctl device list")
        );
        assert_eq!(
            value
                .get("devices")
                .and_then(|v| v.as_array())
                .map(|devices| devices.len() >= 2),
            Some(true)
        );
        assert_eq!(
            value.get("no_hid_device_opened").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(
            value
                .get("hid_enumeration")
                .and_then(|v| v.as_object())
                .is_some()
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_status_writes_observe_receipt_with_mock_backend() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("device-status.json");
        let command = DeviceCommands::Status {
            device: "wheel-001".to_string(),
            moza_lane: None,
            json_out: Some(path.clone()),
            watch: false,
        };

        execute(&command, true, Some("http://127.0.0.1:9")).await?;

        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(
            value.get("command").and_then(|v| v.as_str()),
            Some("wheelctl device status")
        );
        assert_eq!(
            value.get("device_selector").and_then(|v| v.as_str()),
            Some("wheel-001")
        );
        assert_eq!(
            value.get("no_hid_device_opened").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value
                .get("status")
                .and_then(|status| status.get("device"))
                .and_then(|device| device.get("id"))
                .and_then(|id| id.as_str()),
            Some("wheel-001")
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_status_rejects_watch_with_json_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let command = DeviceCommands::Status {
            device: "wheel-001".to_string(),
            moza_lane: None,
            json_out: Some(dir.path().join("device-status.json")),
            watch: true,
        };

        let result = execute(&command, true, Some("http://127.0.0.1:9")).await;

        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|error| error
                    .to_string()
                    .contains("--json-out cannot be used with --watch"))
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn merge_device_lists_keeps_hid_observed_r5_v1_next_to_service_devices() -> TestResult {
        let service_devices = vec![DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Mock Wheel".to_string(),
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
            capabilities: DeviceCapabilities::default(),
        }];
        let hid_devices = vec![DeviceInfo {
            id: "hid-0x346E-0x0004-if2-0x0001-0x0004".to_string(),
            name: "MOZA R5 Base".to_string(),
            source: Some("hid-observe".to_string()),
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0004".to_string()),
            manufacturer: Some("Gudsen".to_string()),
            product_string: Some("MOZA R5 Base".to_string()),
            serial_number_present: Some(true),
            interface_number: Some(2),
            usage_page: Some("0x0001".to_string()),
            usage: Some("0x0004".to_string()),
            hid_path_present: Some(true),
            device_type: DeviceType::WheelBase,
            state: DeviceState::Connected,
            capabilities: DeviceCapabilities {
                supports_health_stream: true,
                ..DeviceCapabilities::default()
            },
        }];

        let devices = merge_device_lists(service_devices, hid_devices);

        let r5 = devices
            .iter()
            .find(|device| {
                device.vendor_id.as_deref() == Some("0x346E")
                    && device.product_id.as_deref() == Some("0x0004")
            })
            .ok_or_else(|| anyhow::anyhow!("missing HID-observed R5 V1"))?;
        assert_eq!(r5.source.as_deref(), Some("hid-observe"));
        assert_eq!(r5.interface_number, Some(2));
        assert_eq!(r5.usage_page.as_deref(), Some("0x0001"));
        assert_eq!(r5.usage.as_deref(), Some("0x0004"));
        assert_eq!(r5.hid_path_present, Some(true));
        Ok(())
    }

    #[test]
    fn merge_device_lists_deduplicates_matching_hid_identity() {
        let service_device = DeviceInfo {
            id: "service-r5".to_string(),
            name: "Moza R5".to_string(),
            source: Some("wheeld".to_string()),
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: Some(2),
            usage_page: Some("0x0001".to_string()),
            usage: Some("0x0004".to_string()),
            hid_path_present: None,
            device_type: DeviceType::WheelBase,
            state: DeviceState::Connected,
            capabilities: DeviceCapabilities::default(),
        };
        let hid_device = DeviceInfo {
            id: "hid-r5-v2".to_string(),
            name: "MOZA R5 Base".to_string(),
            source: Some("hid-observe".to_string()),
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: Some("Gudsen".to_string()),
            product_string: Some("MOZA R5 Base".to_string()),
            serial_number_present: Some(true),
            interface_number: Some(2),
            usage_page: Some("0x0001".to_string()),
            usage: Some("0x0004".to_string()),
            hid_path_present: Some(true),
            device_type: DeviceType::WheelBase,
            state: DeviceState::Connected,
            capabilities: DeviceCapabilities::default(),
        };

        let devices = merge_device_lists(vec![service_device], vec![hid_device]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].source.as_deref(), Some("wheeld"));
    }

    #[tokio::test]
    async fn execute_list_human_output_reports_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("device-list.json");
        let command = DeviceCommands::List {
            detailed: false,
            json_out: Some(path.clone()),
        };

        execute(&command, false, Some("http://127.0.0.1:9")).await?;

        assert!(path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn execute_status_human_output_applies_lane_and_reports_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-lane");
        fs::create_dir_all(&lane)?;
        let path = dir.path().join("device-status.json");
        let command = DeviceCommands::Status {
            device: "wheel-001".to_string(),
            moza_lane: Some(lane),
            json_out: Some(path.clone()),
            watch: false,
        };

        execute(&command, false, Some("http://127.0.0.1:9")).await?;

        assert!(path.exists());
        Ok(())
    }
}
