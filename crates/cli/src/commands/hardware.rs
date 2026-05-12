//! Hardware environment diagnostics.
//!
//! The doctor command is observe-only. It initializes HID enumeration when
//! available, records tool/platform readiness, and never opens devices or sends
//! output, feature, serial, firmware, or DFU commands.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use hidapi::{DeviceInfo, HidApi};
use openracing_hardware_core::{DeviceCapabilityRegistry, DeviceFamily};
use serde::{Deserialize, Serialize};

use crate::commands::HardwareCommands;

pub async fn execute(cmd: &HardwareCommands, json: bool) -> Result<()> {
    match cmd {
        HardwareCommands::Doctor { json_out } => doctor(json, json_out.as_deref()).await,
    }
}

async fn doctor(json: bool, json_out: Option<&Path>) -> Result<()> {
    let receipt = build_doctor_receipt();
    write_json_receipt(json_out, &receipt)?;
    print_doctor_receipt(json, json_out, &receipt)?;
    Ok(())
}

fn build_doctor_receipt() -> HardwareDoctorReceipt {
    let registry = DeviceCapabilityRegistry::openracing_defaults();
    let tools = ToolChecks {
        hid_capture_on_path: executable_on_path("hid-capture"),
        wheelctl_self_check: true,
    };
    let hid = inspect_hid(&registry);
    let vendor_apps = detect_vendor_apps();

    build_doctor_receipt_from_checks(tools, hid, vendor_apps)
}

fn build_doctor_receipt_from_checks(
    tools: ToolChecks,
    hid: HidChecks,
    vendor_apps: VendorAppChecks,
) -> HardwareDoctorReceipt {
    let warnings = doctor_warnings(&tools, &hid);

    HardwareDoctorReceipt {
        success: true,
        command: "wheelctl hardware doctor",
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        os: OsInfo {
            family: env::consts::FAMILY.to_string(),
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
            raw_report_descriptor_capture: RawDescriptorCaptureSupport::current_platform(),
        },
        tools,
        hid,
        vendor_apps,
        warnings,
        notes: vec![
            "hardware doctor is observe-only and does not open HID device handles".to_string(),
            "missing hardware is diagnostic information, not hardware validation evidence"
                .to_string(),
            "virtual or synthetic evidence must not satisfy real hardware receipt gates"
                .to_string(),
        ],
    }
}

fn inspect_hid(registry: &DeviceCapabilityRegistry) -> HidChecks {
    match HidApi::new() {
        Ok(api) => {
            let all_device_count = api.device_list().count();
            let known_devices_visible = api
                .device_list()
                .filter_map(|device| visible_known_device(registry, device))
                .collect::<Vec<_>>();
            let moza_vid_visible = api
                .device_list()
                .any(|device| device.vendor_id() == MOZA_VENDOR_ID);

            HidChecks {
                api_available: true,
                enumeration_available: true,
                all_device_count,
                known_devices_visible,
                moza_vid_visible,
                error: None,
            }
        }
        Err(error) => HidChecks {
            api_available: false,
            enumeration_available: false,
            all_device_count: 0,
            known_devices_visible: Vec::new(),
            moza_vid_visible: false,
            error: Some(error.to_string()),
        },
    }
}

fn visible_known_device(
    registry: &DeviceCapabilityRegistry,
    device: &DeviceInfo,
) -> Option<VisibleKnownDevice> {
    let record = registry.lookup(device.vendor_id(), device.product_id());
    if record.family() == DeviceFamily::Unknown {
        return None;
    }

    Some(VisibleKnownDevice {
        vendor_id: hex_u16(record.vendor_id()),
        product_id: hex_u16(record.product_id()),
        family: format!("{:?}", record.family()),
        model: record.model().to_string(),
        kind: format!("{:?}", record.kind()),
        input: record.input(),
        ffb_output: record.ffb_output(),
        serial_config: record.serial_config(),
        firmware_dfu: record.firmware_dfu(),
        high_torque: record.high_torque(),
        validated_stages: record
            .validated_stages()
            .iter()
            .map(|stage| format!("{stage:?}"))
            .collect(),
        manufacturer: device.manufacturer_string().map(str::to_string),
        product_string: device.product_string().map(str::to_string),
        serial_number_present: device.serial_number().is_some(),
        interface_number: Some(device.interface_number()),
        usage_page: Some(hex_u16(device.usage_page())),
        usage: Some(hex_u16(device.usage())),
        hid_path_present: true,
    })
}

fn doctor_warnings(tools: &ToolChecks, hid: &HidChecks) -> Vec<String> {
    let mut warnings = Vec::new();

    if !tools.hid_capture_on_path {
        warnings.push("hid-capture was not found on PATH".to_string());
    }
    if !hid.api_available {
        warnings.push("HID API initialization failed".to_string());
    }
    if hid.api_available && !hid.moza_vid_visible {
        warnings.push("no Moza VID 0x346E devices are currently visible".to_string());
    }

    warnings
}

fn detect_vendor_apps() -> VendorAppChecks {
    if cfg!(windows) {
        detect_vendor_apps_windows()
    } else {
        VendorAppChecks {
            process_scan_attempted: false,
            pit_house_running: None,
            matched_processes: Vec::new(),
            error: Some("process scan is currently implemented only on Windows".to_string()),
        }
    }
}

fn detect_vendor_apps_windows() -> VendorAppChecks {
    match Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let matched_processes = moza_processes_from_tasklist(&stdout);
            VendorAppChecks {
                process_scan_attempted: true,
                pit_house_running: Some(!matched_processes.is_empty()),
                matched_processes,
                error: None,
            }
        }
        Ok(output) => VendorAppChecks {
            process_scan_attempted: true,
            pit_house_running: None,
            matched_processes: Vec::new(),
            error: Some(format!("tasklist exited with status {}", output.status)),
        },
        Err(error) => VendorAppChecks {
            process_scan_attempted: true,
            pit_house_running: None,
            matched_processes: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn moza_processes_from_tasklist(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(first_csv_field)
        .filter(|process| {
            let lower = process.to_ascii_lowercase();
            lower.contains("moza") || lower.contains("pit house") || lower.contains("pithouse")
        })
        .collect()
}

fn first_csv_field(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let field = trimmed.split(',').next()?.trim().trim_matches('"');
    if field.is_empty() {
        None
    } else {
        Some(field.to_string())
    }
}

fn executable_on_path(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| {
            executable_candidates(name).any(|candidate| {
                let path = dir.join(candidate);
                path.is_file()
            })
        })
    })
}

fn executable_candidates(name: &str) -> impl Iterator<Item = PathBuf> + '_ {
    let base = PathBuf::from(name);
    let extensions = if cfg!(windows) {
        env::var_os("PATHEXT")
            .and_then(|value| value.into_string().ok())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string())
    } else {
        String::new()
    };

    let mut candidates = vec![base.clone()];
    if cfg!(windows) && base.extension().is_none() {
        candidates.extend(
            extensions
                .split(';')
                .map(str::trim)
                .filter(|ext| !ext.is_empty())
                .map(|ext| PathBuf::from(format!("{name}{ext}"))),
        );
    }
    candidates.into_iter()
}

fn write_json_receipt<T: Serialize>(path: Option<&Path>, value: &T) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("failed to serialize JSON receipt")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))
}

fn print_doctor_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareDoctorReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(
        "Hardware doctor completed; no HID devices were opened and no writes were sent.",
    )?;
    write_stdout_line(&format!(
        "OS: {} / {} / {}",
        receipt.os.family, receipt.os.os, receipt.os.arch
    ))?;
    write_stdout_line(&format!(
        "HID API: available={} devices={} known_visible={}",
        receipt.hid.api_available,
        receipt.hid.all_device_count,
        receipt.hid.known_devices_visible.len()
    ))?;
    write_stdout_line(&format!(
        "hid-capture on PATH: {}",
        receipt.tools.hid_capture_on_path
    ))?;
    write_stdout_line(&format!(
        "Moza VID 0x346E visible: {}",
        receipt.hid.moza_vid_visible
    ))?;
    if let Some(running) = receipt.vendor_apps.pit_house_running {
        write_stdout_line(&format!("Pit House likely running: {running}"))?;
    }
    for warning in &receipt.warnings {
        write_stdout_line(&format!("Warning: {warning}"))?;
    }
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn write_stdout_line(line: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").context("failed to write stdout")
}

fn hex_u16(value: u16) -> String {
    format!("0x{value:04X}")
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareDoctorReceipt {
    success: bool,
    command: &'static str,
    generated_at: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    os: OsInfo,
    tools: ToolChecks,
    hid: HidChecks,
    vendor_apps: VendorAppChecks,
    warnings: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OsInfo {
    family: String,
    os: String,
    arch: String,
    raw_report_descriptor_capture: RawDescriptorCaptureSupport,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawDescriptorCaptureSupport {
    supported: bool,
    fallback_supported: bool,
    note: String,
}

impl RawDescriptorCaptureSupport {
    fn current_platform() -> Self {
        if cfg!(windows) {
            Self {
                supported: false,
                fallback_supported: true,
                note: "Windows HID APIs may not expose raw report descriptor bytes; use descriptor hex fallback when needed".to_string(),
            }
        } else {
            Self {
                supported: true,
                fallback_supported: true,
                note: "platform is expected to expose descriptor metadata through HID tooling; descriptor hex fallback remains available".to_string(),
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolChecks {
    hid_capture_on_path: bool,
    wheelctl_self_check: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct HidChecks {
    api_available: bool,
    enumeration_available: bool,
    all_device_count: usize,
    known_devices_visible: Vec<VisibleKnownDevice>,
    moza_vid_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VisibleKnownDevice {
    vendor_id: String,
    product_id: String,
    family: String,
    model: String,
    kind: String,
    input: bool,
    ffb_output: bool,
    serial_config: bool,
    firmware_dfu: bool,
    high_torque: bool,
    validated_stages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_string: Option<String>,
    serial_number_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    interface_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<String>,
    hid_path_present: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct VendorAppChecks {
    process_scan_attempted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pit_house_running: Option<bool>,
    matched_processes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

const MOZA_VENDOR_ID: u16 = 0x346E;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn sample_receipt() -> HardwareDoctorReceipt {
        build_doctor_receipt_from_checks(
            ToolChecks {
                hid_capture_on_path: false,
                wheelctl_self_check: true,
            },
            HidChecks {
                api_available: true,
                enumeration_available: true,
                all_device_count: 0,
                known_devices_visible: Vec::new(),
                moza_vid_visible: false,
                error: None,
            },
            VendorAppChecks {
                process_scan_attempted: false,
                pit_house_running: None,
                matched_processes: Vec::new(),
                error: Some("not scanned in unit test".to_string()),
            },
        )
    }

    #[test]
    fn tasklist_parser_detects_moza_process_names() {
        let output = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\n\"MOZA Pit House.exe\",\"1234\",\"Console\",\"1\",\"10,000 K\"\n\"notepad.exe\",\"5678\",\"Console\",\"1\",\"5,000 K\"";

        let processes = moza_processes_from_tasklist(output);

        assert_eq!(processes, vec!["MOZA Pit House.exe"]);
    }

    #[test]
    fn tasklist_parser_ignores_empty_and_non_moza_rows() {
        let output = "\n\"notepad.exe\",\"5678\"\n\"explorer.exe\",\"12\"";

        let processes = moza_processes_from_tasklist(output);

        assert!(processes.is_empty());
    }

    #[test]
    fn doctor_receipt_is_observe_only() {
        let receipt = sample_receipt();

        assert!(receipt.success);
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_ffb_writes);
        assert!(receipt.no_output_reports);
        assert!(receipt.no_feature_reports);
        assert!(receipt.no_serial_config_commands);
        assert!(receipt.no_firmware_or_dfu_commands);
    }

    #[test]
    fn json_receipt_writer_creates_parent_directories() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested/hardware-doctor.json");
        let receipt = sample_receipt();

        write_json_receipt(Some(&path), &receipt)?;

        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(
            value.get("command").and_then(serde_json::Value::as_str),
            Some("wheelctl hardware doctor")
        );
        assert_eq!(
            value
                .get("no_ffb_writes")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn executable_candidates_add_windows_extensions_only_when_needed() {
        let candidates = executable_candidates("hid-capture")
            .map(|path| path.file_name().unwrap_or(OsStr::new("")).to_owned())
            .collect::<Vec<_>>();

        assert!(
            candidates
                .iter()
                .any(|name| name == OsStr::new("hid-capture"))
        );
        if cfg!(windows) {
            assert!(candidates.iter().any(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("hid-capture.exe")
            }));
        }
    }
}
