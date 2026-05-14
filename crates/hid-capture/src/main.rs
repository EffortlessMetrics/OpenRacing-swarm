#![deny(static_mut_refs)]

use racing_wheel_hid_capture::{
    CaptureFile, CaptureReport, HidReportDescriptorMetadata, parse_hex_u16,
    parse_hid_report_descriptor_metadata,
};

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand};
use hidapi::HidApi;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const MOZA_VENDOR_ID: u16 = 0x346E;
const MOZA_R5_V1_PID: u16 = 0x0004;
const MOZA_R5_V2_PID: u16 = 0x0014;
const MOZA_SRP_PID: u16 = 0x0003;
const MOZA_HBP_PID: u16 = 0x0022;
const MOZA_DIRECT_TORQUE_REPORT_ID: &str = "0x20";

/// Capture raw HID reports from connected racing wheel devices.
#[derive(Parser)]
#[command(
    name = "hid-capture",
    about = "HID device report capture tool for test fixture generation"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all connected HID devices
    List {
        /// Optional vendor ID filter (hex, e.g. 0x346E)
        #[arg(long, value_parser = parse_hex_u16)]
        vendor: Option<u16>,
        /// Write list receipt to a JSON file
        #[arg(long)]
        json_out: Option<PathBuf>,
    },
    /// Capture HID descriptor metadata without sending reports
    Descriptor {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Optional vendor ID filter (hex, e.g. 0x346E)
        #[arg(long, value_parser = parse_hex_u16)]
        vendor: Option<u16>,
        /// Include full descriptor hex when available
        #[arg(long)]
        descriptor_hex: bool,
        /// Operator-supplied HID report descriptor hex for the selected device
        ///
        /// With --vendor and --device, the receipt keeps other vendor records and applies the
        /// descriptor bytes only to the one selected device.
        #[arg(long)]
        report_descriptor_hex: Option<String>,
        /// Write descriptor receipt to a JSON file
        #[arg(long)]
        json_out: Option<PathBuf>,
    },
    /// Capture raw HID reports from a specific device
    Capture {
        /// Vendor ID (hex, e.g. 0x0EB7)
        #[arg(long, value_parser = parse_hex_u16)]
        vid: u16,
        /// Product ID (hex, e.g. 0x0001)
        #[arg(long, value_parser = parse_hex_u16)]
        pid: u16,
        /// Capture duration in seconds (default: 5)
        #[arg(long, default_value = "5")]
        duration: u64,
        /// Save captures to a JSON file instead of printing
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct HidListReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_filter: Option<String>,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    devices: Vec<HidDeviceRecord>,
}

#[derive(Debug, Serialize)]
struct HidDescriptorReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    descriptor_hex_included: bool,
    operator_descriptor_hex_supplied: bool,
    devices: Vec<HidDeviceRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct HidDeviceRecord {
    vendor_id: String,
    product_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_name: Option<String>,
    serial_number_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    interface_number: i32,
    usage_page: String,
    usage: String,
    path: String,
    descriptor_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_descriptor_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_descriptor_crc32: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_descriptor_hex: Option<String>,
    report_metadata_source: String,
    input_report_lengths: Vec<usize>,
    output_report_ids: Vec<String>,
    output_reports: Vec<HidReportRecord>,
    feature_report_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HidReportRecord {
    report_id: String,
    report_len: usize,
}

#[derive(Debug)]
struct ReportDescriptor {
    len: usize,
    crc32: String,
    hex: Option<String>,
    metadata: Option<HidReportDescriptorMetadata>,
}

impl HidDeviceRecord {
    fn apply_report_descriptor(&mut self, descriptor: ReportDescriptor, source: &str) {
        self.descriptor_source = source.to_string();
        self.report_descriptor_len = Some(descriptor.len);
        self.report_descriptor_crc32 = Some(descriptor.crc32);
        self.report_descriptor_hex = descriptor.hex;
        if let Some(metadata) = descriptor.metadata {
            self.report_metadata_source = "report_descriptor_parsed".to_string();
            self.input_report_lengths = metadata.input_report_lengths;
            self.output_reports = metadata
                .output_reports
                .into_iter()
                .map(|report| HidReportRecord {
                    report_id: hex_u8(report.report_id),
                    report_len: report.report_len,
                })
                .collect();
            self.output_report_ids = metadata.output_report_ids.into_iter().map(hex_u8).collect();
            self.feature_report_ids = metadata
                .feature_report_ids
                .into_iter()
                .map(hex_u8)
                .collect();
        }
    }
}

fn list_devices(api: &HidApi, vendor: Option<u16>, json_out: Option<&Path>) -> Result<()> {
    let devices = enumerate_devices(api, vendor, false, false);
    if devices.is_empty() {
        println!("No HID devices found.");
        write_json_receipt(
            json_out,
            &HidListReceipt {
                success: true,
                command: "hid-capture list",
                generated_at_utc: captured_at_utc(),
                vendor_filter: vendor.map(hex_u16),
                no_hid_device_opened: true,
                no_ffb_writes: true,
                no_serial_config_commands: true,
                no_firmware_or_dfu_commands: true,
                devices,
            },
        )?;
        return Ok(());
    }
    println!(
        "{:<8} {:<8} {:<12} {:<20} Product",
        "VID", "PID", "Usage Page", "Manufacturer"
    );
    println!("{}", "-".repeat(80));
    for dev in &devices {
        println!(
            "{:<8} {:<8} {:<12} {:<20} {}",
            dev.vendor_id,
            dev.product_id,
            dev.usage_page,
            dev.manufacturer.as_deref().unwrap_or("(unknown)"),
            dev.product_name.as_deref().unwrap_or("(unknown)"),
        );
    }

    write_json_receipt(
        json_out,
        &HidListReceipt {
            success: true,
            command: "hid-capture list",
            generated_at_utc: captured_at_utc(),
            vendor_filter: vendor.map(hex_u16),
            no_hid_device_opened: true,
            no_ffb_writes: true,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            devices,
        },
    )?;

    Ok(())
}

fn descriptor_devices(
    api: &HidApi,
    vendor: Option<u16>,
    selector: Option<&str>,
    include_descriptor_hex: bool,
    report_descriptor_hex: Option<&str>,
    json_out: Option<&Path>,
) -> Result<()> {
    let mut devices: Vec<_> = enumerate_devices(api, vendor, true, include_descriptor_hex);
    if let Some(hex) = report_descriptor_hex {
        apply_operator_report_descriptor_hex(&mut devices, selector, hex)?;
        if selector.is_some() && vendor.is_none() {
            devices.retain(|device| selector_matches(device, selector));
        }
    } else {
        devices.retain(|device| selector_matches(device, selector));
    }

    if devices.is_empty() && selector.is_some() {
        return Err(anyhow!(
            "no HID device matched selector '{}'",
            selector.unwrap_or_default()
        ));
    }

    println!(
        "Captured descriptor metadata for {} HID device(s); no reports sent.",
        devices.len()
    );

    write_json_receipt(
        json_out,
        &HidDescriptorReceipt {
            success: true,
            command: "hid-capture descriptor",
            generated_at_utc: captured_at_utc(),
            vendor_filter: vendor.map(hex_u16),
            selector: selector.map(str::to_string),
            no_hid_device_opened: true,
            no_ffb_writes: true,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            descriptor_hex_included: include_descriptor_hex,
            operator_descriptor_hex_supplied: report_descriptor_hex.is_some(),
            devices,
        },
    )
}

fn capture_device(
    api: &HidApi,
    vid: u16,
    pid: u16,
    duration_secs: u64,
    output: Option<&str>,
) -> Result<()> {
    let device = api
        .open(vid, pid)
        .with_context(|| format!("Failed to open device VID=0x{vid:04X} PID=0x{pid:04X}"))?;

    // Non-blocking read: returns immediately if no data available
    device
        .set_blocking_mode(false)
        .context("Failed to set non-blocking mode")?;

    let start = Instant::now();
    let epoch_start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;

    let deadline = Duration::from_secs(duration_secs);
    let mut buf = [0u8; 64];
    let mut captures: Vec<CaptureReport> = Vec::new();

    println!("Capturing from VID=0x{vid:04X} PID=0x{pid:04X} for {duration_secs}s...");

    while start.elapsed() < deadline {
        match device.read(&mut buf) {
            Ok(0) => {
                // No data yet; yield briefly
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Ok(n) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                let timestamp_us = epoch_start + elapsed_us;
                let report_id = buf[0];
                let hex = buf[..n]
                    .iter()
                    .map(|b| format!("0x{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");

                if output.is_none() {
                    println!("[+{elapsed_us:>10}µs] id=0x{report_id:02X}  {hex}");
                }

                captures.push(CaptureReport {
                    timestamp_us,
                    report_id,
                    data: hex,
                });
            }
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
        }
    }

    println!("Captured {} report(s).", captures.len());

    if let Some(path) = output {
        let capture_file = CaptureFile {
            vendor_id: format!("0x{vid:04X}"),
            product_id: format!("0x{pid:04X}"),
            captures,
        };
        let json =
            serde_json::to_string_pretty(&capture_file).context("Failed to serialize captures")?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write output file '{path}'"))?;
        println!("Captures saved to '{path}'.");
    }

    Ok(())
}

fn enumerate_devices(
    api: &HidApi,
    vendor: Option<u16>,
    include_descriptor: bool,
    include_descriptor_hex: bool,
) -> Vec<HidDeviceRecord> {
    let mut devices: Vec<_> = api
        .device_list()
        .filter(|device| vendor.is_none_or(|vid| device.vendor_id() == vid))
        .map(|device| {
            let path = device.path().to_string_lossy().to_string();
            let descriptor = if include_descriptor {
                try_read_report_descriptor(&path, include_descriptor_hex)
            } else {
                None
            };
            let input_report_lengths =
                expected_input_report_lengths(device.vendor_id(), device.product_id());
            let output_report_ids =
                expected_output_report_ids(device.vendor_id(), device.product_id());
            let output_reports = expected_output_reports(device.vendor_id(), device.product_id());
            let feature_report_ids =
                expected_feature_report_ids(device.vendor_id(), device.product_id());

            let descriptor_source = descriptor_source_label(descriptor.as_ref());
            let mut record = HidDeviceRecord {
                vendor_id: hex_u16(device.vendor_id()),
                product_id: hex_u16(device.product_id()),
                product_name: device.product_string().map(str::to_string),
                serial_number_present: device.serial_number().is_some(),
                manufacturer: device.manufacturer_string().map(str::to_string),
                interface_number: device.interface_number(),
                usage_page: hex_u16(device.usage_page()),
                usage: hex_u16(device.usage()),
                path,
                descriptor_source: descriptor_source.clone(),
                report_descriptor_len: None,
                report_descriptor_crc32: None,
                report_descriptor_hex: None,
                report_metadata_source: report_metadata_source(
                    device.vendor_id() == MOZA_VENDOR_ID,
                )
                .to_string(),
                input_report_lengths,
                output_report_ids,
                output_reports,
                feature_report_ids,
            };
            if let Some(descriptor) = descriptor {
                record.apply_report_descriptor(descriptor, &descriptor_source);
            }
            record
        })
        .collect();

    devices.sort_by_key(|device| {
        (
            device.vendor_id.clone(),
            device.product_id.clone(),
            device.interface_number,
            device.usage_page.clone(),
            device.usage.clone(),
            device.path.clone(),
        )
    });
    devices
}

fn apply_operator_report_descriptor_hex(
    devices: &mut [HidDeviceRecord],
    selector: Option<&str>,
    report_descriptor_hex: &str,
) -> Result<()> {
    let selected = matching_device_indices(devices, selector);
    if selected.len() != 1 {
        return Err(anyhow!(
            "--report-descriptor-hex requires exactly one selected HID device, found {}",
            selected.len()
        ));
    }
    let descriptor = report_descriptor_from_operator_hex(report_descriptor_hex)?;
    if let Some(device) = devices.get_mut(selected[0]) {
        device.apply_report_descriptor(descriptor, "operator_supplied_hex");
    }
    Ok(())
}

fn matching_device_indices(devices: &[HidDeviceRecord], selector: Option<&str>) -> Vec<usize> {
    devices
        .iter()
        .enumerate()
        .filter_map(|(index, device)| selector_matches(device, selector).then_some(index))
        .collect()
}

#[cfg(target_os = "linux")]
fn try_read_report_descriptor(hid_path: &str, include_hex: bool) -> Option<ReportDescriptor> {
    if !hid_path.starts_with("/dev/hidraw") {
        return None;
    }

    let node = Path::new(hid_path).file_name()?.to_str()?;
    let sysfs = format!("/sys/class/hidraw/{node}/device/report_descriptor");
    let bytes = fs::read(sysfs).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let crc = hasher.finalize();
    let hex = include_hex.then(|| bytes_hex_compact(&bytes));

    Some(ReportDescriptor {
        len: bytes.len(),
        crc32: format!("0x{crc:08X}"),
        hex,
        metadata: parse_hid_report_descriptor_metadata(&bytes),
    })
}

#[cfg(not(target_os = "linux"))]
fn try_read_report_descriptor(hid_path: &str, include_hex: bool) -> Option<ReportDescriptor> {
    let _ = (hid_path, include_hex);
    None
}

fn report_descriptor_from_operator_hex(value: &str) -> Result<ReportDescriptor> {
    let bytes =
        parse_hex_bytes(value).map_err(|e| anyhow!("invalid report descriptor hex: {e}"))?;
    if bytes.is_empty() {
        return Err(anyhow!(
            "report descriptor hex must contain at least one byte"
        ));
    }
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let crc = hasher.finalize();
    Ok(ReportDescriptor {
        len: bytes.len(),
        crc32: format!("0x{crc:08X}"),
        hex: Some(bytes_hex_compact(&bytes)),
        metadata: parse_hid_report_descriptor_metadata(&bytes),
    })
}

fn parse_hex_bytes(value: &str) -> std::result::Result<Vec<u8>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("empty hex byte string".to_string());
    }
    if trimmed.split_whitespace().count() > 1 {
        return trimmed.split_whitespace().map(parse_hex_u8_token).collect();
    }
    let compact = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !compact.len().is_multiple_of(2) {
        return Err("hex byte string must contain an even number of digits".to_string());
    }
    (0..compact.len())
        .step_by(2)
        .map(|start| parse_hex_u8_token(&compact[start..start + 2]))
        .collect()
}

fn parse_hex_u8_token(token: &str) -> std::result::Result<u8, String> {
    let trimmed = token.trim();
    let value = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if value.is_empty() {
        return Err("empty hex byte".to_string());
    }
    u8::from_str_radix(value, 16).map_err(|_| format!("invalid hex byte '{token}'"))
}

fn selector_matches(device: &HidDeviceRecord, selector: Option<&str>) -> bool {
    let Some(selector) = selector else {
        return true;
    };
    let selector = selector.trim();
    if selector.is_empty() {
        return true;
    }

    if let Some((vid, pid)) = parse_vid_pid_selector(selector) {
        return device.vendor_id.eq_ignore_ascii_case(&hex_u16(vid))
            && device.product_id.eq_ignore_ascii_case(&hex_u16(pid));
    }

    if let Some(pid) = parse_hex_selector(selector) {
        return device.product_id.eq_ignore_ascii_case(&hex_u16(pid));
    }

    let selector_lc = selector.to_ascii_lowercase();
    device.path.to_ascii_lowercase().contains(&selector_lc)
        || device
            .product_name
            .as_deref()
            .map(|s| s.to_ascii_lowercase().contains(&selector_lc))
            .unwrap_or(false)
}

fn parse_vid_pid_selector(selector: &str) -> Option<(u16, u16)> {
    let (vid, pid) = selector.split_once(':')?;
    Some((parse_hex_selector(vid)?, parse_hex_selector(pid)?))
}

fn parse_hex_selector(selector: &str) -> Option<u16> {
    let value = selector
        .strip_prefix("0x")
        .or_else(|| selector.strip_prefix("0X"))
        .unwrap_or(selector);
    u16::from_str_radix(value, 16).ok()
}

fn report_metadata_source(is_moza: bool) -> &'static str {
    if is_moza {
        "moza_protocol_expected"
    } else {
        "not_parsed"
    }
}

fn expected_input_report_lengths(vendor_id: u16, product_id: u16) -> Vec<usize> {
    if vendor_id != MOZA_VENDOR_ID {
        return Vec::new();
    }

    match product_id {
        MOZA_R5_V1_PID => vec![42],
        MOZA_R5_V2_PID => vec![7, 31],
        MOZA_SRP_PID => vec![5],
        MOZA_HBP_PID => vec![2, 3, 4, 5],
        _ => Vec::new(),
    }
}

fn expected_output_report_ids(vendor_id: u16, product_id: u16) -> Vec<String> {
    if vendor_id == MOZA_VENDOR_ID && matches!(product_id, MOZA_R5_V1_PID | MOZA_R5_V2_PID) {
        vec![MOZA_DIRECT_TORQUE_REPORT_ID.to_string()]
    } else {
        Vec::new()
    }
}

fn expected_output_reports(vendor_id: u16, product_id: u16) -> Vec<HidReportRecord> {
    if vendor_id == MOZA_VENDOR_ID && matches!(product_id, MOZA_R5_V1_PID | MOZA_R5_V2_PID) {
        vec![HidReportRecord {
            report_id: MOZA_DIRECT_TORQUE_REPORT_ID.to_string(),
            report_len: 8,
        }]
    } else {
        Vec::new()
    }
}

fn expected_feature_report_ids(vendor_id: u16, product_id: u16) -> Vec<String> {
    if vendor_id == MOZA_VENDOR_ID && matches!(product_id, MOZA_R5_V1_PID | MOZA_R5_V2_PID) {
        vec!["0x02".to_string(), "0x03".to_string(), "0x11".to_string()]
    } else {
        Vec::new()
    }
}

fn descriptor_source_label(report_descriptor: Option<&ReportDescriptor>) -> String {
    if report_descriptor.is_some() {
        "linux_sysfs".to_string()
    } else {
        "unavailable".to_string()
    }
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

fn captured_at_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hex_u16(value: u16) -> String {
    format!("0x{value:04X}")
}

fn hex_u8(value: u8) -> String {
    format!("0x{value:02X}")
}

fn bytes_hex_compact(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = HidApi::new().context("Failed to initialize HidApi")?;

    match &cli.command {
        Commands::List { vendor, json_out } => list_devices(&api, *vendor, json_out.as_deref()),
        Commands::Descriptor {
            device,
            vendor,
            descriptor_hex,
            report_descriptor_hex,
            json_out,
        } => descriptor_devices(
            &api,
            *vendor,
            device.as_deref(),
            *descriptor_hex,
            report_descriptor_hex.as_deref(),
            json_out.as_deref(),
        ),
        Commands::Capture {
            vid,
            pid,
            duration,
            output,
        } => capture_device(&api, *vid, *pid, *duration, output.as_deref()),
    }
}

// ── BDD-style scenario tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ═══ Scenario: VID/PID Hex Parsing ══════════════════════════════════════

    /// GIVEN a valid hex string with 0x prefix
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct u16 value
    #[test]
    fn given_hex_with_0x_prefix_when_parsed_then_correct_u16_returned() {
        assert_eq!(parse_hex_u16("0x0EB7"), Ok(0x0EB7));
        assert_eq!(parse_hex_u16("0x0001"), Ok(0x0001));
        assert_eq!(parse_hex_u16("0X046D"), Ok(0x046D));
    }

    /// GIVEN a valid hex string without the 0x prefix
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct u16 value
    #[test]
    fn given_hex_without_prefix_when_parsed_then_correct_u16_returned() {
        assert_eq!(parse_hex_u16("346E"), Ok(0x346E));
        assert_eq!(parse_hex_u16("FFFF"), Ok(0xFFFF));
        assert_eq!(parse_hex_u16("0000"), Ok(0x0000));
    }

    /// GIVEN an invalid hex string
    /// WHEN parse_hex_u16 is called
    /// THEN it returns a descriptive error
    #[test]
    fn given_invalid_hex_string_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("ZZZZ").is_err());
        assert!(parse_hex_u16("xyz").is_err());
    }

    /// GIVEN operator-supplied report descriptor hex
    /// WHEN converted into descriptor metadata
    /// THEN length, CRC, and normalized hex are recorded
    #[test]
    fn given_report_descriptor_hex_when_parsed_then_crc_is_recorded() -> Result<()> {
        let descriptor = report_descriptor_from_operator_hex("05 01 09 04")?;

        assert_eq!(descriptor.len, 4);
        assert!(descriptor.crc32.starts_with("0x"));
        assert_eq!(descriptor.hex.as_deref(), Some("05010904"));
        Ok(())
    }

    #[test]
    fn given_unmatched_vendor_when_listed_then_empty_observe_receipt_is_written() -> Result<()> {
        let api = HidApi::new().context("failed to initialize test HidApi")?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("receipts").join("hid-list.json");

        list_devices(&api, Some(0xFFFF), Some(&path))?;

        let text = std::fs::read_to_string(&path)?;
        let receipt: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(receipt.get("success").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            receipt.get("command").and_then(|v| v.as_str()),
            Some("hid-capture list")
        );
        assert_eq!(
            receipt.get("vendor_filter").and_then(|v| v.as_str()),
            Some("0xFFFF")
        );
        assert_eq!(
            receipt
                .get("no_hid_device_opened")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            receipt.get("no_ffb_writes").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            receipt
                .get("devices")
                .and_then(|v| v.as_array())
                .map(Vec::len),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn given_unmatched_vendor_when_descriptors_requested_then_empty_receipt_is_written()
    -> Result<()> {
        let api = HidApi::new().context("failed to initialize test HidApi")?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("descriptor.json");

        descriptor_devices(&api, Some(0xFFFF), None, true, None, Some(&path))?;

        let text = std::fs::read_to_string(&path)?;
        let receipt: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(receipt.get("success").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            receipt.get("command").and_then(|v| v.as_str()),
            Some("hid-capture descriptor")
        );
        assert_eq!(
            receipt.get("vendor_filter").and_then(|v| v.as_str()),
            Some("0xFFFF")
        );
        assert_eq!(
            receipt
                .get("descriptor_hex_included")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            receipt
                .get("operator_descriptor_hex_supplied")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            receipt
                .get("devices")
                .and_then(|v| v.as_array())
                .map(Vec::len),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn given_selector_without_matching_hid_when_descriptor_requested_then_error_returned()
    -> Result<()> {
        let api = HidApi::new().context("failed to initialize test HidApi")?;
        let result =
            descriptor_devices(&api, Some(0xFFFF), Some("0x346E:0x0014"), false, None, None);

        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|error| error.to_string().contains("no HID device matched selector"))
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn given_parseable_report_descriptor_when_applied_then_metadata_is_descriptor_derived()
    -> Result<()> {
        let descriptor = report_descriptor_from_operator_hex(
            "85 01 75 08 95 06 81 02 85 02 75 08 95 1E 81 02 85 20 75 08 95 07 91 02 85 03 75 08 95 03 B1 02 85 11 75 08 95 03 B1 02",
        )?;
        let mut device = sample_hid_device_record();

        device.apply_report_descriptor(descriptor, "operator_supplied_hex");

        assert_eq!(device.descriptor_source, "operator_supplied_hex");
        assert_eq!(device.report_metadata_source, "report_descriptor_parsed");
        assert_eq!(device.input_report_lengths, vec![7, 31]);
        assert_eq!(
            device.output_report_ids,
            vec![MOZA_DIRECT_TORQUE_REPORT_ID.to_string()]
        );
        assert_eq!(
            device.output_reports,
            vec![HidReportRecord {
                report_id: "0x20".to_string(),
                report_len: 8,
            }]
        );
        assert_eq!(
            device.feature_report_ids,
            vec!["0x03".to_string(), "0x11".to_string()]
        );
        Ok(())
    }

    // ═══ Scenario: CLI Parsing For Safe Observation Commands ═══════════════

    /// GIVEN the list command with a Moza vendor filter and JSON output
    /// WHEN clap parses the CLI
    /// THEN the command captures the requested safe receipt settings
    #[test]
    fn given_list_vendor_json_out_when_cli_parsed_then_fields_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli::try_parse_from([
            "hid-capture",
            "list",
            "--vendor",
            "0x346E",
            "--json-out",
            "hid-list.json",
        ])?;

        match cli.command {
            Commands::List { vendor, json_out } => {
                assert_eq!(vendor, Some(0x346E));
                assert_eq!(json_out, Some(PathBuf::from("hid-list.json")));
            }
            _ => return Err("expected list command".into()),
        }
        Ok(())
    }

    /// GIVEN the descriptor command with selector, vendor filter, descriptor hex, and JSON output
    /// WHEN clap parses the CLI
    /// THEN all descriptor receipt options are preserved
    #[test]
    fn given_descriptor_options_when_cli_parsed_then_fields_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli::try_parse_from([
            "hid-capture",
            "descriptor",
            "--vendor",
            "0x346E",
            "--device",
            "0x346E:0x0014",
            "--descriptor-hex",
            "--report-descriptor-hex",
            "05010904",
            "--json-out",
            "descriptor.json",
        ])?;

        match cli.command {
            Commands::Descriptor {
                device,
                vendor,
                descriptor_hex,
                report_descriptor_hex,
                json_out,
            } => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(vendor, Some(0x346E));
                assert!(descriptor_hex);
                assert_eq!(report_descriptor_hex.as_deref(), Some("05010904"));
                assert_eq!(json_out, Some(PathBuf::from("descriptor.json")));
            }
            _ => return Err("expected descriptor command".into()),
        }
        Ok(())
    }

    #[test]
    fn given_moza_operator_docs_when_hid_capture_commands_listed_then_they_parse()
    -> Result<(), Box<dyn std::error::Error>> {
        let docs = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware/moza-r5-validation.md"),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/hardware/moza-r5-artifact-checklist.md"),
        ];
        let mut checked = 0usize;

        for path in docs {
            let text = std::fs::read_to_string(&path)?;
            for (line_index, line) in text.lines().enumerate() {
                let command = line.trim().replace("YYYY-MM-DD", "2026-05-06");
                if !command.starts_with("hid-capture ") {
                    continue;
                }

                let args = command.split_whitespace().collect::<Vec<_>>();
                Cli::try_parse_from(args).map_err(|error| {
                    format!(
                        "{}:{} documented hid-capture command failed to parse: {command}\n{error}",
                        path.display(),
                        line_index + 1
                    )
                })?;
                checked += 1;
            }
        }

        assert!(
            checked >= 3,
            "expected to parse documented Moza hid-capture commands, checked {checked}"
        );
        Ok(())
    }

    // ═══ Scenario: Capture Report Serialization ═════════════════════════════

    /// GIVEN a CaptureReport with valid fields
    /// WHEN serialized to JSON and deserialized back
    /// THEN all fields are preserved in the roundtrip
    #[test]
    fn given_capture_report_when_roundtripped_via_json_then_fields_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: 1_000_000,
            report_id: 0x01,
            data: "0x01 0x02 0x03".to_string(),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 1_000_000);
        assert_eq!(restored.report_id, 0x01);
        assert_eq!(restored.data, "0x01 0x02 0x03");
        Ok(())
    }

    /// GIVEN a CaptureFile with vendor/product IDs and multiple captures
    /// WHEN serialized to pretty JSON and deserialized
    /// THEN the full structure including all reports is preserved
    #[test]
    fn given_capture_file_with_reports_when_roundtripped_then_structure_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 100,
                    report_id: 0x01,
                    data: "0x01 0x80 0x00".to_string(),
                },
                CaptureReport {
                    timestamp_us: 200,
                    report_id: 0x02,
                    data: "0x02 0x90 0xFF".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert_eq!(restored.product_id, "0x0002");
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(restored.captures[0].timestamp_us, 100);
        assert_eq!(restored.captures[0].report_id, 0x01);
        assert_eq!(restored.captures[1].timestamp_us, 200);
        assert_eq!(restored.captures[1].report_id, 0x02);
        Ok(())
    }

    /// GIVEN an empty captures list
    /// WHEN serialized as a CaptureFile
    /// THEN the file deserializes with zero captures
    #[test]
    fn given_empty_captures_when_serialized_then_zero_captures_in_output()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert!(restored.captures.is_empty());
        Ok(())
    }

    // ═══ Scenario: Hex Parsing Edge Cases ═══════════════════════════════════

    /// GIVEN the maximum u16 hex value
    /// WHEN parse_hex_u16 is called
    /// THEN it returns 0xFFFF
    #[test]
    fn given_max_u16_hex_when_parsed_then_returns_max_value() {
        assert_eq!(parse_hex_u16("0xFFFF"), Ok(0xFFFF));
    }

    /// GIVEN a hex value that overflows u16
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_overflow_hex_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("0x10000").is_err());
        assert!(parse_hex_u16("0xFFFFF").is_err());
    }

    /// GIVEN an empty string
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_empty_string_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("").is_err());
    }

    /// GIVEN a hex string with mixed case
    /// WHEN parse_hex_u16 is called
    /// THEN it parses correctly regardless of case
    #[test]
    fn given_mixed_case_hex_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0xAbCd"), Ok(0xABCD));
        assert_eq!(parse_hex_u16("abcd"), Ok(0xABCD));
    }

    /// GIVEN just the "0x" prefix with no digits
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_bare_0x_prefix_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("0x").is_err());
        assert!(parse_hex_u16("0X").is_err());
    }

    /// GIVEN a single hex digit
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct value
    #[test]
    fn given_single_digit_hex_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0"), Ok(0));
        assert_eq!(parse_hex_u16("F"), Ok(15));
        assert_eq!(parse_hex_u16("0xA"), Ok(10));
    }

    /// GIVEN a hex string with leading zeros
    /// WHEN parse_hex_u16 is called
    /// THEN leading zeros are handled correctly
    #[test]
    fn given_leading_zeros_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0x0001"), Ok(1));
        assert_eq!(parse_hex_u16("0x00FF"), Ok(255));
        assert_eq!(parse_hex_u16("0001"), Ok(1));
    }

    // ═══ Scenario: CaptureReport Field Boundaries ═══════════════════════════

    /// GIVEN a CaptureReport with zero-valued fields
    /// WHEN serialized and deserialized
    /// THEN zero values are preserved
    #[test]
    fn given_zero_valued_report_when_roundtripped_then_zeros_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: 0,
            report_id: 0x00,
            data: String::new(),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 0);
        assert_eq!(restored.report_id, 0x00);
        assert!(restored.data.is_empty());
        Ok(())
    }

    /// GIVEN a CaptureReport with maximum field values
    /// WHEN serialized and deserialized
    /// THEN max values are preserved
    #[test]
    fn given_max_valued_report_when_roundtripped_then_max_values_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: u64::MAX,
            report_id: 0xFF,
            data: "0xFF".repeat(64),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, u64::MAX);
        assert_eq!(restored.report_id, 0xFF);
        assert_eq!(restored.data.len(), report.data.len());
        Ok(())
    }

    // ═══ Scenario: Capture Session Management ═══════════════════════════════

    /// GIVEN multiple CaptureReports added in sequence
    /// WHEN stored in a CaptureFile
    /// THEN the insertion order is preserved after serialization
    #[test]
    fn given_sequential_reports_when_stored_in_file_then_order_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: Vec::new(),
        };
        for i in 0..10u64 {
            file.captures.push(CaptureReport {
                timestamp_us: i * 1000,
                report_id: (i as u8) % 4,
                data: format!("0x{i:02X}"),
            });
        }
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 10);
        for (i, report) in restored.captures.iter().enumerate() {
            assert_eq!(report.timestamp_us, (i as u64) * 1000);
        }
        Ok(())
    }

    /// GIVEN a CaptureFile with monotonically increasing timestamps
    /// WHEN deserialized
    /// THEN timestamps are in strictly ascending order
    #[test]
    fn given_monotonic_timestamps_when_deserialized_then_ascending_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 100,
                    report_id: 1,
                    data: "0x01".into(),
                },
                CaptureReport {
                    timestamp_us: 200,
                    report_id: 1,
                    data: "0x02".into(),
                },
                CaptureReport {
                    timestamp_us: 300,
                    report_id: 1,
                    data: "0x03".into(),
                },
            ],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        for window in restored.captures.windows(2) {
            assert!(
                window[0].timestamp_us < window[1].timestamp_us,
                "timestamps must be monotonically increasing"
            );
        }
        Ok(())
    }

    /// GIVEN a large number of capture reports
    /// WHEN serialized and deserialized
    /// THEN all reports survive the roundtrip
    #[test]
    fn given_many_reports_when_roundtripped_then_all_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let captures: Vec<CaptureReport> = (0..1000)
            .map(|i| CaptureReport {
                timestamp_us: i * 1000,
                report_id: (i % 256) as u8,
                data: format!("0x{:02X} 0x{:02X}", i % 256, (i / 256) % 256),
            })
            .collect();
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC24F".to_string(),
            captures,
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 1000);
        assert_eq!(restored.captures[0].timestamp_us, 0);
        assert_eq!(restored.captures[999].timestamp_us, 999_000);
        Ok(())
    }

    // ═══ Scenario: File Format JSON Structure ═══════════════════════════════

    /// GIVEN a CaptureFile
    /// WHEN serialized to JSON
    /// THEN the JSON contains the expected top-level keys
    #[test]
    fn given_capture_file_when_serialized_then_json_has_expected_keys()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        assert!(value.get("vendor_id").is_some());
        assert!(value.get("product_id").is_some());
        assert!(value.get("captures").is_some());
        assert!(value.get("captures").and_then(|v| v.as_array()).is_some());
        Ok(())
    }

    /// GIVEN a CaptureReport in a file
    /// WHEN serialized to JSON
    /// THEN each report has timestamp_us, report_id, and data fields
    #[test]
    fn given_report_in_file_when_serialized_then_report_has_expected_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 42,
                report_id: 0x07,
                data: "0x07 0xFF".to_string(),
            }],
        };
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let report = &value["captures"][0];
        assert_eq!(report["timestamp_us"], 42);
        assert_eq!(report["report_id"], 7);
        assert_eq!(report["data"], "0x07 0xFF");
        Ok(())
    }

    /// GIVEN a CaptureFile serialized to pretty JSON
    /// WHEN compared to compact JSON
    /// THEN both deserialize to identical structures
    #[test]
    fn given_pretty_and_compact_json_when_deserialized_then_identical()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 500,
                report_id: 0x03,
                data: "0x03 0x10".to_string(),
            }],
        };
        let compact = serde_json::to_string(&file)?;
        let pretty = serde_json::to_string_pretty(&file)?;
        assert_ne!(
            compact, pretty,
            "pretty and compact should differ in formatting"
        );
        let from_compact: CaptureFile = serde_json::from_str(&compact)?;
        let from_pretty: CaptureFile = serde_json::from_str(&pretty)?;
        assert_eq!(from_compact.vendor_id, from_pretty.vendor_id);
        assert_eq!(from_compact.product_id, from_pretty.product_id);
        assert_eq!(from_compact.captures.len(), from_pretty.captures.len());
        assert_eq!(
            from_compact.captures[0].timestamp_us,
            from_pretty.captures[0].timestamp_us
        );
        Ok(())
    }

    // ═══ Scenario: File I/O Roundtrip ═══════════════════════════════════════

    /// GIVEN a CaptureFile written to a temporary file
    /// WHEN read back and deserialized
    /// THEN the full structure is preserved
    #[test]
    fn given_capture_file_written_to_disk_when_read_back_then_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1000,
                    report_id: 0x01,
                    data: "0x01 0x80 0x7F".to_string(),
                },
                CaptureReport {
                    timestamp_us: 2000,
                    report_id: 0x01,
                    data: "0x01 0x81 0x80".to_string(),
                },
            ],
        };
        let dir = std::env::temp_dir().join("hid_capture_test");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("test_capture.json");
        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(&path, &json)?;
        let read_back = std::fs::read_to_string(&path)?;
        let restored: CaptureFile = serde_json::from_str(&read_back)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert_eq!(restored.product_id, "0xC266");
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(restored.captures[0].data, "0x01 0x80 0x7F");
        assert_eq!(restored.captures[1].data, "0x01 0x81 0x80");
        // cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        Ok(())
    }

    // ═══ Scenario: Error Handling ════════════════════════════════════════════

    /// GIVEN malformed JSON missing required fields
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails
    #[test]
    fn given_malformed_json_when_deserialized_as_capture_file_then_error() {
        let bad_json = r#"{"vendor_id": "0x046D"}"#;
        let result = serde_json::from_str::<CaptureFile>(bad_json);
        assert!(result.is_err());
    }

    /// GIVEN JSON with wrong field types
    /// WHEN deserialized as a CaptureReport
    /// THEN deserialization fails
    #[test]
    fn given_wrong_field_types_when_deserialized_as_report_then_error() {
        // timestamp_us should be u64, not string
        let bad_json = r#"{"timestamp_us": "not_a_number", "report_id": 1, "data": "0x01"}"#;
        let result = serde_json::from_str::<CaptureReport>(bad_json);
        assert!(result.is_err());
    }

    /// GIVEN JSON with extra unknown fields
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization succeeds (serde default ignores unknown fields)
    #[test]
    fn given_extra_fields_when_deserialized_then_succeeds() -> Result<(), Box<dyn std::error::Error>>
    {
        let json = r#"{
            "vendor_id": "0x046D",
            "product_id": "0x0002",
            "captures": [],
            "extra_field": "should be ignored"
        }"#;
        let file: CaptureFile = serde_json::from_str(json)?;
        assert_eq!(file.vendor_id, "0x046D");
        assert!(file.captures.is_empty());
        Ok(())
    }

    /// GIVEN completely invalid JSON
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails
    #[test]
    fn given_invalid_json_when_deserialized_then_error() {
        let not_json = "this is not json at all";
        let result = serde_json::from_str::<CaptureFile>(not_json);
        assert!(result.is_err());
    }

    /// GIVEN an empty JSON object
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails due to missing required fields
    #[test]
    fn given_empty_json_object_when_deserialized_then_error() {
        let empty = "{}";
        let result = serde_json::from_str::<CaptureFile>(empty);
        assert!(result.is_err());
    }

    /// GIVEN JSON with report_id exceeding u8 range
    /// WHEN deserialized as a CaptureReport
    /// THEN deserialization fails
    #[test]
    fn given_report_id_overflow_when_deserialized_then_error() {
        let bad_json = r#"{"timestamp_us": 100, "report_id": 256, "data": "0x01"}"#;
        let result = serde_json::from_str::<CaptureReport>(bad_json);
        assert!(result.is_err());
    }

    // ═══ Scenario: Device Filtering Helpers ═════════════════════════════════

    /// GIVEN vendor and product IDs
    /// WHEN formatted as hex strings for a CaptureFile
    /// THEN the format matches the expected "0xNNNN" pattern
    #[test]
    fn given_vid_pid_when_formatted_then_matches_hex_pattern() {
        let vid: u16 = 0x046D;
        let pid: u16 = 0xC266;
        let vid_str = format!("0x{vid:04X}");
        let pid_str = format!("0x{pid:04X}");
        assert_eq!(vid_str, "0x046D");
        assert_eq!(pid_str, "0xC266");
    }

    /// GIVEN a hex-formatted VID/PID string from a CaptureFile
    /// WHEN parsed back to u16 using parse_hex_u16
    /// THEN it returns the original numeric value
    #[test]
    fn given_formatted_vid_pid_when_parsed_back_then_original_value_restored() {
        let original_vid: u16 = 0x0EB7;
        let original_pid: u16 = 0x0001;
        let vid_str = format!("0x{original_vid:04X}");
        let pid_str = format!("0x{original_pid:04X}");
        assert_eq!(parse_hex_u16(&vid_str), Ok(original_vid));
        assert_eq!(parse_hex_u16(&pid_str), Ok(original_pid));
    }

    /// GIVEN zero VID and PID
    /// WHEN formatted and parsed
    /// THEN roundtrip produces zero
    #[test]
    fn given_zero_vid_pid_when_roundtripped_then_zero_returned() {
        let vid_str = format!("0x{:04X}", 0u16);
        let pid_str = format!("0x{:04X}", 0u16);
        assert_eq!(vid_str, "0x0000");
        assert_eq!(pid_str, "0x0000");
        assert_eq!(parse_hex_u16(&vid_str), Ok(0u16));
        assert_eq!(parse_hex_u16(&pid_str), Ok(0u16));
    }

    fn sample_hid_device_record() -> HidDeviceRecord {
        sample_hid_device_record_with_pid(
            MOZA_R5_V2_PID,
            "MOZA R5 Wheel Base",
            r"\\?\hid#vid_346e&pid_0014#abc",
        )
    }

    fn sample_hid_device_record_with_pid(pid: u16, product: &str, path: &str) -> HidDeviceRecord {
        HidDeviceRecord {
            vendor_id: "0x346E".to_string(),
            product_id: hex_u16(pid),
            product_name: Some(product.to_string()),
            serial_number_present: true,
            manufacturer: Some("Gudsen Moza".to_string()),
            interface_number: 0,
            usage_page: "0x0001".to_string(),
            usage: "0x0004".to_string(),
            path: path.to_string(),
            descriptor_source: "unavailable".to_string(),
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            report_descriptor_hex: None,
            report_metadata_source: "not_parsed".to_string(),
            input_report_lengths: Vec::new(),
            output_report_ids: Vec::new(),
            output_reports: Vec::new(),
            feature_report_ids: Vec::new(),
        }
    }

    /// GIVEN a HID descriptor record
    /// WHEN matching by selector forms accepted by descriptor capture
    /// THEN VID/PID, PID, path, and product selectors match correctly
    #[test]
    fn given_device_record_when_selector_matches_then_supported_forms_work() {
        let device = sample_hid_device_record();

        assert!(selector_matches(&device, Some("0x346E:0x0014")));
        assert!(selector_matches(&device, Some("0014")));
        assert!(selector_matches(&device, Some("hid#vid_346e")));
        assert!(selector_matches(&device, Some("moza r5")));
        assert!(!selector_matches(&device, Some("0x346E:0x0004")));
        assert!(!selector_matches(&device, Some("unrelated")));
    }

    /// GIVEN a serialized HID device receipt record
    /// WHEN serial metadata is present
    /// THEN the receipt records presence without exposing a raw serial number
    #[test]
    fn given_device_record_when_serialized_then_raw_serial_is_not_present()
    -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::to_value(sample_hid_device_record())?;

        assert_eq!(
            value.get("serial_number_present"),
            Some(&serde_json::json!(true))
        );
        assert!(value.get("serial_number").is_none());
        Ok(())
    }

    #[test]
    fn given_moza_r5_when_expected_metadata_requested_then_lane_reports_are_returned() {
        assert_eq!(
            expected_input_report_lengths(MOZA_VENDOR_ID, MOZA_R5_V1_PID),
            vec![42]
        );
        assert_eq!(
            expected_input_report_lengths(MOZA_VENDOR_ID, MOZA_R5_V2_PID),
            vec![7, 31]
        );
        assert_eq!(
            expected_output_report_ids(MOZA_VENDOR_ID, MOZA_R5_V2_PID),
            vec![MOZA_DIRECT_TORQUE_REPORT_ID.to_string()]
        );
        let output_reports = expected_output_reports(MOZA_VENDOR_ID, MOZA_R5_V2_PID);
        assert_eq!(
            output_reports,
            vec![HidReportRecord {
                report_id: "0x20".to_string(),
                report_len: 8,
            }]
        );
        assert_eq!(
            expected_feature_report_ids(MOZA_VENDOR_ID, MOZA_R5_V2_PID),
            vec!["0x02".to_string(), "0x03".to_string(), "0x11".to_string()]
        );
        assert_eq!(report_metadata_source(true), "moza_protocol_expected");
    }

    #[test]
    fn given_vendor_wide_moza_devices_when_operator_hex_applied_then_stack_records_are_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut devices = vec![
            sample_hid_device_record(),
            sample_hid_device_record_with_pid(
                MOZA_SRP_PID,
                "MOZA SR-P Pedals",
                r"\\?\hid#vid_346e&pid_0003#pedals",
            ),
            sample_hid_device_record_with_pid(
                MOZA_HBP_PID,
                "MOZA HBP Handbrake",
                r"\\?\hid#vid_346e&pid_0022#handbrake",
            ),
        ];

        apply_operator_report_descriptor_hex(
            &mut devices,
            Some("0x346E:0x0014"),
            "85 01 75 08 95 06 81 02 85 02 75 08 95 1E 81 02 85 20 75 08 95 07 91 02 85 03 75 08 95 03 B1 02 85 11 75 08 95 03 B1 02",
        )?;

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].product_id, "0x0014");
        assert_eq!(devices[0].descriptor_source, "operator_supplied_hex");
        assert_eq!(
            devices[0].report_metadata_source,
            "report_descriptor_parsed"
        );
        assert_eq!(devices[0].input_report_lengths, vec![7, 31]);
        assert_eq!(
            devices[0].output_report_ids,
            vec![MOZA_DIRECT_TORQUE_REPORT_ID.to_string()]
        );
        assert_eq!(
            devices[0].output_reports,
            vec![HidReportRecord {
                report_id: "0x20".to_string(),
                report_len: 8,
            }]
        );
        assert!(devices.iter().any(|device| device.product_id == "0x0003"));
        assert!(devices.iter().any(|device| device.product_id == "0x0022"));
        assert_eq!(devices[1].descriptor_source, "unavailable");
        assert_eq!(devices[2].descriptor_source, "unavailable");
        Ok(())
    }

    #[test]
    fn given_operator_hex_without_unique_selection_when_applied_then_error_returned() {
        let mut devices = vec![
            sample_hid_device_record(),
            sample_hid_device_record_with_pid(
                MOZA_SRP_PID,
                "MOZA SR-P Pedals",
                r"\\?\hid#vid_346e&pid_0003#pedals",
            ),
        ];

        let result = apply_operator_report_descriptor_hex(&mut devices, None, "05010904");

        assert!(result.is_err());
        assert_eq!(devices[0].descriptor_source, "unavailable");
        assert_eq!(devices[1].descriptor_source, "unavailable");
    }

    #[test]
    fn given_non_moza_device_when_expected_metadata_requested_then_empty_metadata_returned() {
        assert!(expected_input_report_lengths(0x046D, MOZA_R5_V2_PID).is_empty());
        assert!(expected_output_report_ids(0x046D, MOZA_R5_V2_PID).is_empty());
        assert!(expected_feature_report_ids(0x046D, MOZA_R5_V2_PID).is_empty());
        assert_eq!(report_metadata_source(false), "not_parsed");
    }

    // ═══ Scenario: Report Playback and Data Integrity ═══════════════════════

    /// GIVEN a sequence of capture reports representing a playback session
    /// WHEN computing inter-report intervals from timestamps
    /// THEN the intervals match the expected deltas
    #[test]
    fn given_capture_sequence_when_computing_intervals_then_deltas_correct() {
        let captures = [
            CaptureReport {
                timestamp_us: 1000,
                report_id: 1,
                data: "0x01".into(),
            },
            CaptureReport {
                timestamp_us: 2000,
                report_id: 1,
                data: "0x02".into(),
            },
            CaptureReport {
                timestamp_us: 3500,
                report_id: 1,
                data: "0x03".into(),
            },
            CaptureReport {
                timestamp_us: 4000,
                report_id: 1,
                data: "0x04".into(),
            },
        ];
        let intervals: Vec<u64> = captures
            .windows(2)
            .map(|w| w[1].timestamp_us - w[0].timestamp_us)
            .collect();
        assert_eq!(intervals, vec![1000, 1500, 500]);
    }

    /// GIVEN capture reports with various report_id values
    /// WHEN filtered by a specific report_id
    /// THEN only matching reports are returned
    #[test]
    fn given_mixed_report_ids_when_filtered_then_only_matching_returned() {
        let captures = [
            CaptureReport {
                timestamp_us: 100,
                report_id: 0x01,
                data: "a".into(),
            },
            CaptureReport {
                timestamp_us: 200,
                report_id: 0x02,
                data: "b".into(),
            },
            CaptureReport {
                timestamp_us: 300,
                report_id: 0x01,
                data: "c".into(),
            },
            CaptureReport {
                timestamp_us: 400,
                report_id: 0x03,
                data: "d".into(),
            },
            CaptureReport {
                timestamp_us: 500,
                report_id: 0x01,
                data: "e".into(),
            },
        ];
        let filtered: Vec<&CaptureReport> =
            captures.iter().filter(|r| r.report_id == 0x01).collect();
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].data, "a");
        assert_eq!(filtered[1].data, "c");
        assert_eq!(filtered[2].data, "e");
    }

    /// GIVEN a CaptureFile with captures
    /// WHEN the total session duration is computed
    /// THEN it equals the difference between first and last timestamps
    #[test]
    fn given_captures_when_computing_session_duration_then_correct() {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1_000_000,
                    report_id: 1,
                    data: "0x01".into(),
                },
                CaptureReport {
                    timestamp_us: 1_500_000,
                    report_id: 1,
                    data: "0x02".into(),
                },
                CaptureReport {
                    timestamp_us: 6_000_000,
                    report_id: 1,
                    data: "0x03".into(),
                },
            ],
        };
        let duration = file
            .captures
            .last()
            .map(|l| l.timestamp_us)
            .zip(file.captures.first().map(|f| f.timestamp_us))
            .map(|(last, first)| last - first);
        assert_eq!(duration, Some(5_000_000));
    }

    /// GIVEN a hex string with whitespace
    /// WHEN parse_hex_u16 is called
    /// THEN it fails (no implicit whitespace trimming)
    #[test]
    fn given_hex_with_whitespace_when_parsed_then_error() {
        assert!(parse_hex_u16(" 0x0001").is_err());
        assert!(parse_hex_u16("0x0001 ").is_err());
    }
}
