//! Safe Moza HID bring-up commands.
//!
//! Probe, descriptor, capture, and validation commands never send HID output
//! reports, feature reports, serial configuration, or FFB data. The zero-torque
//! command is the only output path here, and it sends only report `0x20` encoded
//! with raw torque `0` and flags `0`.

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use hidapi::{DeviceInfo, HidApi};
use jsonschema::Validator;
use racing_wheel_hid_capture::{
    HidReportDescriptorMetadata, HidReportDescriptorReport, parse_hid_report_descriptor_metadata,
};
use racing_wheel_hid_moza_protocol::report::looks_like_live_r5_v1_extended_report;
use racing_wheel_hid_moza_protocol::{
    DeviceWriter, FfbMode, MOZA_VENDOR_ID, MozaDeviceCategory, MozaDirectTorqueEncoder,
    MozaInitState, MozaInputState, MozaProtocol, MozaTopologyHint, REPORT_LEN, VendorProtocol,
    identify_device, input_report, is_wheelbase_product, parse_axis, product_ids,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::commands::{
    MozaBundleStage, MozaCommands, MozaInitMode, MozaPitHouseEvidenceKind,
    MozaPitHouseObservationCase, MozaReceiptTemplateKind,
};
use crate::error::CliError;

const DIRECT_TORQUE_REPORT_ID: &str = "0x20";
const SIMULATOR_FFB_WRITER_COMMAND: &str = "wheeld --hardware-lane moza-r5";
const SIMULATOR_TELEMETRY_RECORDER_COMMAND: &str = "wheelctl telemetry record";
const MOZA_VENDOR_HEX: &str = "0x346E";
const HIGH_TORQUE_FEATURE_REPORT_ID: &str = "0x02";
const START_REPORTING_FEATURE_REPORT_ID: &str = "0x03";
const FFB_MODE_FEATURE_REPORT_ID: &str = "0x11";
const MOZA_R5_MANIFEST_SCHEMA_JSON: &str =
    include_str!("../../../../ci/hardware/moza-r5/manifest.schema.json");
const SIMULATOR_FFB_PREREQUISITE_ARTIFACTS: [(&str, &str); 6] = [
    ("zero_torque_real_hardware", "zero-torque-proof.json"),
    ("watchdog_zero_output", "watchdog-proof.json"),
    ("disconnect_final_zero", "disconnect-proof.json"),
    ("init_off_handshake", "init-off.json"),
    ("init_standard_handshake", "init-standard.json"),
    ("low_torque_bounded", "low-torque-proof.json"),
];

fn short_hid_write_error(bytes_written: usize) -> Option<String> {
    if bytes_written == REPORT_LEN {
        None
    } else {
        Some(format!(
            "short_hid_write: expected {REPORT_LEN} bytes, wrote {bytes_written}"
        ))
    }
}

fn no_out_of_scope_device_commands(receipt: &Value) -> bool {
    json_bool(receipt, "no_serial_config_commands") == Some(true)
        && json_bool(receipt, "no_firmware_or_dfu_commands") == Some(true)
}

fn receipt_failure(message: impl Into<String>) -> anyhow::Error {
    CliError::ReceiptFailure(message.into()).into()
}

struct TorqueTestRequest<'a> {
    json: bool,
    selector: Option<&'a str>,
    pid_override: Option<&'a str>,
    zero_proof: Option<&'a Path>,
    descriptor: Option<&'a Path>,
    lane: Option<&'a Path>,
    init_off: Option<&'a Path>,
    init_standard: Option<&'a Path>,
    dry_run: bool,
    confirm_low_torque: bool,
    explicit_operator_override: bool,
    max_percent: f32,
    duration_ms: u64,
    hz: u32,
    json_out: Option<&'a Path>,
}

struct PitHouseProofRequest<'a> {
    json: bool,
    lane: &'a Path,
    closed_artifact: &'a Path,
    open_standard_artifact: &'a Path,
    direct_artifact: &'a Path,
    mode_change_artifact: &'a Path,
    firmware_page_artifact: &'a Path,
    shared_control_risk: &'a str,
    json_out: Option<&'a Path>,
    overwrite: bool,
}

struct PitHouseObservationRequest<'a> {
    json: bool,
    case: MozaPitHouseObservationCase,
    evidence_kind: MozaPitHouseEvidenceKind,
    evidence_artifact: Option<&'a Path>,
    operator: &'a str,
    evidence: &'a str,
    json_out: &'a Path,
    overwrite: bool,
}

struct PitHouseCaseRequest<'a> {
    json: bool,
    lane: &'a Path,
    case: MozaPitHouseObservationCase,
    observation_artifact: &'a Path,
    evidence: &'a str,
    json_out: &'a Path,
    overwrite: bool,
}

struct SimulatorFfbSmokeRequest<'a> {
    json: bool,
    lane: &'a Path,
    game: &'a str,
    telemetry_source: &'a str,
    output_log_artifact: &'a Path,
    descriptor_trusted: bool,
    explicit_operator_override: bool,
    watchdog_timeout_ms: u64,
    stop_cleared_output: bool,
    pause_cleared_output: bool,
    game_exit_cleared_output: bool,
    json_out: Option<&'a Path>,
    overwrite: bool,
}

pub async fn execute(cmd: &MozaCommands, json: bool) -> Result<()> {
    match cmd {
        MozaCommands::InitLane {
            lane,
            wheelbase_pid,
            operator,
            overwrite,
        } => init_lane(json, lane, wheelbase_pid, operator, *overwrite).await,
        MozaCommands::Probe { json_out } => probe(json, json_out.as_deref()).await,
        MozaCommands::Status {
            device,
            lane,
            json_out,
        } => {
            status(
                json,
                device.as_deref(),
                lane.as_deref(),
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::Descriptor {
            device,
            descriptor_hex,
            report_descriptor_hex,
            report_descriptor_hex_file,
            report_descriptor_bin_file,
            json_out,
        } => {
            descriptor(
                json,
                device.as_deref(),
                *descriptor_hex,
                report_descriptor_hex.as_deref(),
                report_descriptor_hex_file.as_deref(),
                report_descriptor_bin_file.as_deref(),
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::CaptureInput {
            device,
            duration_ms,
            read_timeout_ms,
            json_out,
        } => {
            capture_input(
                json,
                device.as_deref(),
                *duration_ms,
                *read_timeout_ms,
                json_out,
            )
            .await
        }
        MozaCommands::ValidateCapture {
            capture,
            pid,
            json_out,
        } => validate_capture(json, capture, pid.as_deref(), json_out.as_deref()).await,
        MozaCommands::AnalyzeCapture { capture, json_out } => {
            analyze_capture(json, capture, json_out.as_deref()).await
        }
        MozaCommands::AnalyzeLane { lane, json_out } => {
            analyze_lane(json, lane, json_out.as_deref()).await
        }
        MozaCommands::SyncRoleStatus {
            lane,
            check,
            json_out,
        } => sync_role_status(json, lane, *check, json_out.as_deref()).await,
        MozaCommands::ValidateCaptures { lane, json_out } => {
            validate_captures(json, lane, json_out.as_deref()).await
        }
        MozaCommands::PromoteFixture {
            capture,
            fixture_id,
            fixture_out,
            pid,
            max_reports,
            overwrite,
            json_out,
        } => {
            promote_fixture(
                json,
                capture,
                fixture_id,
                fixture_out,
                pid.as_deref(),
                *max_reports,
                *overwrite,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::PromoteFixtures {
            lane,
            fixture_dir,
            max_reports,
            overwrite,
            json_out,
        } => {
            promote_fixtures(
                json,
                lane,
                fixture_dir,
                *max_reports,
                *overwrite,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::Zero {
            device,
            pid,
            dry_run,
            repeat,
            hz,
            watchdog_timeout_ms,
            json_out,
        } => {
            zero_torque(
                json,
                device.as_deref(),
                pid.as_deref(),
                *dry_run,
                *repeat,
                *hz,
                *watchdog_timeout_ms,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::WatchdogProof {
            device,
            pid,
            dry_run,
            pre_zero_count,
            hz,
            watchdog_timeout_ms,
            json_out,
        } => {
            watchdog_proof(
                json,
                device.as_deref(),
                pid.as_deref(),
                *dry_run,
                *pre_zero_count,
                *hz,
                *watchdog_timeout_ms,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::DisconnectProof {
            device,
            pid,
            dry_run,
            confirm_disconnect_test,
            max_duration_ms,
            hz,
            json_out,
        } => {
            disconnect_proof(
                json,
                device.as_deref(),
                pid.as_deref(),
                *dry_run,
                *confirm_disconnect_test,
                *max_duration_ms,
                *hz,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::Init {
            device,
            pid,
            mode,
            dry_run,
            json_out,
        } => {
            init(
                json,
                device.as_deref(),
                pid.as_deref(),
                *mode,
                *dry_run,
                json_out.as_deref(),
            )
            .await
        }
        MozaCommands::TorqueTest {
            device,
            pid,
            zero_proof,
            descriptor,
            lane,
            init_off,
            init_standard,
            dry_run,
            confirm_low_torque,
            explicit_operator_override,
            max_percent,
            duration_ms,
            hz,
            json_out,
        } => {
            torque_test(TorqueTestRequest {
                json,
                selector: device.as_deref(),
                pid_override: pid.as_deref(),
                zero_proof: zero_proof.as_deref(),
                descriptor: descriptor.as_deref(),
                lane: lane.as_deref(),
                init_off: init_off.as_deref(),
                init_standard: init_standard.as_deref(),
                dry_run: *dry_run,
                confirm_low_torque: *confirm_low_torque,
                explicit_operator_override: *explicit_operator_override,
                max_percent: *max_percent,
                duration_ms: *duration_ms,
                hz: *hz,
                json_out: json_out.as_deref(),
            })
            .await
        }
        MozaCommands::ReceiptTemplate {
            kind,
            json_out,
            overwrite,
        } => receipt_template(json, *kind, json_out, *overwrite).await,
        MozaCommands::PitHouseObservation {
            case,
            evidence_kind,
            evidence_artifact,
            operator,
            evidence,
            json_out,
            overwrite,
        } => {
            pit_house_observation(PitHouseObservationRequest {
                json,
                case: *case,
                evidence_kind: *evidence_kind,
                evidence_artifact: evidence_artifact.as_deref(),
                operator,
                evidence,
                json_out,
                overwrite: *overwrite,
            })
            .await
        }
        MozaCommands::PitHouseCase {
            lane,
            case,
            observation_artifact,
            evidence,
            json_out,
            overwrite,
        } => {
            pit_house_case(PitHouseCaseRequest {
                json,
                lane,
                case: *case,
                observation_artifact,
                evidence,
                json_out,
                overwrite: *overwrite,
            })
            .await
        }
        MozaCommands::PitHouseProof {
            lane,
            closed_artifact,
            open_standard_artifact,
            direct_artifact,
            mode_change_artifact,
            firmware_page_artifact,
            shared_control_risk,
            json_out,
            overwrite,
        } => {
            pit_house_proof(PitHouseProofRequest {
                json,
                lane,
                closed_artifact,
                open_standard_artifact,
                direct_artifact,
                mode_change_artifact,
                firmware_page_artifact,
                shared_control_risk,
                json_out: json_out.as_deref(),
                overwrite: *overwrite,
            })
            .await
        }
        MozaCommands::SimulatorTelemetryProof {
            lane,
            game,
            telemetry_source,
            recorder_artifact,
            duration_ms,
            json_out,
            overwrite,
        } => {
            simulator_telemetry_proof(
                json,
                lane,
                game,
                telemetry_source,
                recorder_artifact,
                *duration_ms,
                json_out.as_deref(),
                *overwrite,
            )
            .await
        }
        MozaCommands::SimulatorFfbSmoke {
            lane,
            game,
            telemetry_source,
            output_log_artifact,
            descriptor_trusted,
            explicit_operator_override,
            watchdog_timeout_ms,
            stop_cleared_output,
            pause_cleared_output,
            game_exit_cleared_output,
            json_out,
            overwrite,
        } => {
            simulator_ffb_smoke(SimulatorFfbSmokeRequest {
                json,
                lane,
                game,
                telemetry_source,
                output_log_artifact,
                descriptor_trusted: *descriptor_trusted,
                explicit_operator_override: *explicit_operator_override,
                watchdog_timeout_ms: *watchdog_timeout_ms,
                stop_cleared_output: *stop_cleared_output,
                pause_cleared_output: *pause_cleared_output,
                game_exit_cleared_output: *game_exit_cleared_output,
                json_out: json_out.as_deref(),
                overwrite: *overwrite,
            })
            .await
        }
        MozaCommands::PromoteManifest {
            lane,
            stage,
            json_out,
        } => promote_manifest(json, lane, *stage, json_out.as_deref()).await,
        MozaCommands::VerifyBundle {
            lane,
            stage,
            json_out,
        } => verify_bundle(json, lane, *stage, json_out.as_deref()).await,
        MozaCommands::AuditLane {
            lane,
            stage,
            json_out,
        } => audit_lane(json, lane, *stage, json_out.as_deref()).await,
    }
}

async fn init_lane(
    json: bool,
    lane: &Path,
    wheelbase_pid: &str,
    operator: &str,
    overwrite: bool,
) -> Result<()> {
    let pid = parse_hex_selector(wheelbase_pid)
        .ok_or_else(|| anyhow!("--wheelbase-pid must be a hex PID, e.g. 0x0014"))?;
    if !matches!(pid, product_ids::R5_V1 | product_ids::R5_V2) {
        return Err(anyhow!(
            "--wheelbase-pid must be 0x0004 or 0x0014 for the Moza R5 lane"
        ));
    }

    let captures_dir = lane.join("captures");
    fs::create_dir_all(&captures_dir)
        .with_context(|| format!("failed to create '{}'", captures_dir.display()))?;

    let manifest_path = lane.join("manifest.json");
    if manifest_path.exists() && !overwrite {
        return Err(anyhow!(
            "{} already exists; pass --overwrite to replace it",
            manifest_path.display()
        ));
    }

    let manifest = moza_lane_manifest_value(pid, operator, "not_started", false, false);
    write_json_file(&manifest_path, &manifest)?;

    let receipt = InitLaneReceipt {
        success: true,
        command: "wheelctl moza init-lane",
        generated_at_utc: now_utc(),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        lane: lane.display().to_string(),
        manifest: manifest_path.display().to_string(),
        captures_dir: captures_dir.display().to_string(),
        wheelbase_pid: hex_u16(pid),
        operator: operator.to_string(),
        completion_state: "not_started",
        notes: vec![
            "init-lane creates only local filesystem artifacts; it opens no HID device".to_string(),
            "the generated manifest is pre-validation and makes no hardware or simulator claim"
                .to_string(),
        ],
    };

    print_init_lane_receipt(json, &receipt)
}

async fn probe(json: bool, json_out: Option<&Path>) -> Result<()> {
    let api = HidApi::new().context("failed to initialize HID API")?;
    let devices = enumerate_moza_devices(&api, false, false);
    let receipt = ProbeReceipt {
        success: true,
        command: "wheelctl moza probe",
        generated_at_utc: now_utc(),
        vendor_id: hex_u16(MOZA_VENDOR_ID),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        devices,
        notes: vec!["probe enumerates HID metadata only; no device writes are sent".to_string()],
    };

    write_json_receipt(json_out, &receipt)?;
    print_probe_receipt(json, json_out, &receipt)
}

async fn status(
    json: bool,
    selector: Option<&str>,
    lane: Option<&Path>,
    json_out: Option<&Path>,
) -> Result<()> {
    let api = HidApi::new().context("failed to initialize HID API")?;
    let devices: Vec<_> = enumerate_moza_devices(&api, false, false)
        .into_iter()
        .filter(|device| selector_matches(device, selector))
        .collect();

    if selector.is_some() && devices.is_empty() {
        return Err(anyhow!(
            "no Moza HID device matched selector '{}'",
            selector.unwrap_or_default()
        ));
    }

    let receipt = moza_status_receipt(devices, selector, lane);
    write_json_receipt(json_out, &receipt)?;
    print_status_receipt(json, json_out, &receipt)
}

async fn descriptor(
    json: bool,
    selector: Option<&str>,
    include_descriptor_hex: bool,
    report_descriptor_hex: Option<&str>,
    report_descriptor_hex_file: Option<&Path>,
    report_descriptor_bin_file: Option<&Path>,
    json_out: Option<&Path>,
) -> Result<()> {
    let api = HidApi::new().context("failed to initialize HID API")?;
    let mut devices: Vec<_> = enumerate_moza_devices(&api, true, include_descriptor_hex);
    let operator_descriptor_hex = operator_report_descriptor_hex(
        report_descriptor_hex,
        report_descriptor_hex_file,
        report_descriptor_bin_file,
    )?;

    if let Some(hex) = operator_descriptor_hex.as_deref() {
        apply_operator_report_descriptor_to_selected_device(&mut devices, selector, hex)?;
    } else {
        devices.retain(|device| selector_matches(device, selector));
        if devices.is_empty() && selector.is_some() {
            return Err(anyhow!(
                "no Moza HID device matched selector '{}'",
                selector.unwrap_or_default()
            ));
        }
    }

    let receipt = DescriptorReceipt {
        success: true,
        command: "wheelctl moza descriptor",
        generated_at_utc: now_utc(),
        vendor_id: hex_u16(MOZA_VENDOR_ID),
        selector: selector.map(str::to_string),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        descriptor_hex_included: include_descriptor_hex,
        operator_descriptor_hex_supplied: operator_descriptor_hex.is_some(),
        operator_descriptor_hex_source: operator_descriptor_source(
            report_descriptor_hex,
            report_descriptor_hex_file,
            report_descriptor_bin_file,
        ),
        devices,
        notes: vec![
            "descriptor metadata is read from enumeration/sysfs or operator-supplied descriptor bytes only; no HID reports are sent".to_string(),
        ],
    };

    write_json_receipt(json_out, &receipt)?;
    print_descriptor_receipt(json, json_out, &receipt)
}

fn apply_operator_report_descriptor_to_selected_device(
    devices: &mut [MozaDeviceRecord],
    selector: Option<&str>,
    report_descriptor_hex: &str,
) -> Result<()> {
    let selected_indices: Vec<_> = devices
        .iter()
        .enumerate()
        .filter_map(|(index, device)| selector_matches(device, selector).then_some(index))
        .collect();

    if selected_indices.len() != 1 {
        return Err(anyhow!(
            "operator-supplied report descriptor requires exactly one selected Moza HID device, found {}",
            selected_indices.len()
        ));
    }

    let descriptor = report_descriptor_from_operator_hex(report_descriptor_hex)?;
    if let Some(device) = selected_indices
        .first()
        .and_then(|index| devices.get_mut(*index))
    {
        device.apply_report_descriptor(descriptor, "operator_supplied_hex");
        Ok(())
    } else {
        Err(anyhow!(
            "operator-supplied report descriptor selected device disappeared before descriptor metadata could be applied"
        ))
    }
}

fn operator_report_descriptor_hex(
    inline_hex: Option<&str>,
    hex_file: Option<&Path>,
    bin_file: Option<&Path>,
) -> Result<Option<String>> {
    match (inline_hex.is_some(), hex_file.is_some(), bin_file.is_some()) {
        (false, false, false) => Ok(None),
        (true, false, false) => Ok(inline_hex.map(str::to_string)),
        (false, true, false) => hex_file.map(read_report_descriptor_hex_file).transpose(),
        (false, false, true) => bin_file.map(read_report_descriptor_bin_file).transpose(),
        _ => Err(anyhow!(
            "use only one of --report-descriptor-hex, --report-descriptor-hex-file, or --report-descriptor-bin-file"
        )),
    }
}

fn operator_descriptor_source(
    inline_hex: Option<&str>,
    hex_file: Option<&Path>,
    bin_file: Option<&Path>,
) -> Option<&'static str> {
    if inline_hex.is_some() {
        Some("inline")
    } else if hex_file.is_some() {
        Some("file")
    } else if bin_file.is_some() {
        Some("binary_file")
    } else {
        None
    }
}

fn read_report_descriptor_hex_file(path: &Path) -> Result<String> {
    let raw = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    let text = match String::from_utf8(raw) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    };
    let bytes = extract_hex_bytes_from_descriptor_text(&text)?;
    if bytes.is_empty() {
        return Err(anyhow!(
            "no HID report descriptor bytes found in '{}'; export or paste the actual Report Descriptor byte block, for example lines like '0000: 05 01 09 04 ...' or a compact hex descriptor. A USBTreeView device/interface summary, wDescriptorLength value, or ERROR_INVALID_PARAMETER descriptor-read failure is not enough.",
            path.display()
        ));
    }
    Ok(bytes_hex_compact(&bytes))
}

fn read_report_descriptor_bin_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    if bytes.is_empty() {
        return Err(anyhow!(
            "no HID report descriptor bytes found in '{}'; provide the raw binary HID report_descriptor file, for example Linux /sys/class/hidraw/<node>/device/report_descriptor.",
            path.display()
        ));
    }
    Ok(bytes_hex_compact(&bytes))
}

fn extract_hex_bytes_from_descriptor_text(text: &str) -> Result<Vec<u8>> {
    if let Some(bytes) = extract_explicit_report_descriptor_block(text)? {
        return Ok(bytes);
    }
    if looks_like_usbtreeview_summary(text) {
        return Ok(Vec::new());
    }

    let mut bytes = Vec::new();
    for line in text.lines() {
        if let Some(mut line_bytes) = extract_hex_bytes_from_descriptor_line(line)? {
            bytes.append(&mut line_bytes);
        }
    }
    Ok(bytes)
}

fn extract_explicit_report_descriptor_block(text: &str) -> Result<Option<Vec<u8>>> {
    let mut in_report_descriptor = false;
    let mut bytes = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if is_report_descriptor_heading(trimmed) {
            in_report_descriptor = true;
            continue;
        }
        if !in_report_descriptor {
            continue;
        }
        if !bytes.is_empty() && starts_next_usbtreeview_descriptor_block(trimmed) {
            break;
        }
        if let Some(mut line_bytes) =
            extract_hex_bytes_from_descriptor_line_with_context(line, true)?
        {
            bytes.append(&mut line_bytes);
        }
    }

    if in_report_descriptor {
        Ok(Some(bytes))
    } else {
        Ok(None)
    }
}

fn is_report_descriptor_heading(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("report descriptor")
}

fn starts_next_usbtreeview_descriptor_block(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("interface descriptor")
        || lower.contains("endpoint descriptor")
        || lower.contains("hid descriptor")
        || lower.contains("string descriptor")
        || lower.contains("device descriptor")
        || lower.contains("configuration descriptor")
}

fn looks_like_usbtreeview_summary(text: &str) -> bool {
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("data (hexdump)")
            || lower.contains("usb device")
            || lower.contains("interface descriptor")
            || lower.contains("hid descriptor")
            || lower.contains("bdescriptortype")
            || lower.contains("error reading descriptor")
    })
}

fn extract_hex_bytes_from_descriptor_line(line: &str) -> Result<Option<Vec<u8>>> {
    extract_hex_bytes_from_descriptor_line_with_context(line, false)
}

fn extract_hex_bytes_from_descriptor_line_with_context(
    line: &str,
    allow_hexdump_prefix: bool,
) -> Result<Option<Vec<u8>>> {
    let without_comments = line.split("//").next().unwrap_or_default().trim();
    if without_comments.is_empty() {
        return Ok(None);
    }

    let Some(candidate) = descriptor_byte_candidate(without_comments, allow_hexdump_prefix) else {
        return Ok(None);
    };
    let tokens = candidate
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(None);
    }

    if tokens.len() == 1 && is_compact_hex_byte_string(tokens[0]) {
        return parse_hex_bytes(tokens[0])
            .map(Some)
            .map_err(|e| anyhow!("invalid descriptor byte line '{without_comments}': {e}"));
    }

    if !tokens.iter().all(|token| is_hex_byte_token(token)) {
        if allow_hexdump_prefix {
            let prefix_tokens = tokens
                .iter()
                .copied()
                .take_while(|token| is_hex_byte_token(token))
                .collect::<Vec<_>>();
            if !prefix_tokens.is_empty() {
                return prefix_tokens
                    .iter()
                    .map(|token| {
                        parse_hex_u8_token(token).map_err(|e| {
                            anyhow!("invalid descriptor byte line '{without_comments}': {e}")
                        })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(Some);
            }
        }
        return Ok(None);
    }

    tokens
        .iter()
        .map(|token| {
            parse_hex_u8_token(token)
                .map_err(|e| anyhow!("invalid descriptor byte line '{without_comments}': {e}"))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn descriptor_byte_candidate(line: &str, allow_hexdump_prefix: bool) -> Option<&str> {
    if let Some((prefix, suffix)) = line.split_once(':') {
        let prefix = prefix.trim();
        let suffix = suffix.trim();
        if is_hex_offset_token(prefix)
            || prefix.to_ascii_lowercase().contains("report descriptor")
            || (allow_hexdump_prefix && prefix.eq_ignore_ascii_case("data (hexdump)"))
        {
            return Some(suffix);
        }
        return None;
    }

    Some(line)
}

fn is_hex_offset_token(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    !value.is_empty()
        && token
            .trim()
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        && value.len() <= 8
        && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_hex_byte_token(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    value.len() == 2 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_compact_hex_byte_string(token: &str) -> bool {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    value.len() > 2 && value.len().is_multiple_of(2) && value.chars().all(|c| c.is_ascii_hexdigit())
}

async fn capture_input(
    json: bool,
    selector: Option<&str>,
    duration_ms: u64,
    read_timeout_ms: i32,
    json_out: &Path,
) -> Result<()> {
    if read_timeout_ms < 0 {
        return Err(anyhow!("--read-timeout-ms must be non-negative"));
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    device
        .set_blocking_mode(true)
        .context("failed to set HID blocking mode")?;

    if let Some(parent) = json_out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let file = File::create(json_out)
        .with_context(|| format!("failed to create '{}'", json_out.display()))?;
    let mut writer = BufWriter::new(file);
    let started_at = Instant::now();
    let deadline = started_at + Duration::from_millis(duration_ms);
    let mut report_count = 0usize;
    let mut buf = [0u8; 256];

    while Instant::now() < deadline {
        let n = device
            .read_timeout(&mut buf, read_timeout_ms)
            .context("HID read error")?;
        if n == 0 {
            continue;
        }

        let elapsed_us = started_at.elapsed().as_micros() as u64;
        let report = CapturedInputReport {
            ts_ns: unix_now_ns(),
            elapsed_us,
            command: "wheelctl moza capture-input",
            no_ffb_writes: true,
            no_output_reports: true,
            no_feature_reports: true,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            vendor_id: snapshot.vendor_id.clone(),
            product_id: snapshot.product_id.clone(),
            product_name: snapshot.product_name.clone(),
            interface_number: snapshot.interface_number,
            usage_page: snapshot.usage_page.clone(),
            path: snapshot.path.clone(),
            report_id: hex_u8(buf[0]),
            report_len: n,
            data_hex: bytes_hex_compact(&buf[..n]),
            data: bytes_hex_array(&buf[..n]),
        };

        let line = serde_json::to_string(&report).context("failed to serialize capture line")?;
        writeln!(writer, "{line}").context("failed to write capture line")?;
        report_count += 1;
    }

    writer.flush().context("failed to flush capture file")?;

    let summary = CaptureSummary {
        success: true,
        command: "wheelctl moza capture-input",
        generated_at_utc: now_utc(),
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        selector: selector.map(str::to_string),
        duration_ms,
        read_timeout_ms,
        output: json_out.display().to_string(),
        report_count,
        device: snapshot,
        notes: vec![
            "capture-input opens the HID device for input reads only; no output or feature reports are sent"
                .to_string(),
        ],
    };

    print_capture_summary(json, &summary)
}

async fn validate_capture(
    json: bool,
    capture: &Path,
    pid_override: Option<&str>,
    json_out: Option<&Path>,
) -> Result<()> {
    let pid_override = pid_override
        .map(parse_required_hex_u16)
        .transpose()
        .with_context(|| "invalid --pid value")?;

    let receipt = validate_capture_file(capture, pid_override)?;
    write_json_receipt(json_out, &receipt)?;
    print_capture_validation_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza capture validation failed: {} parsed, {} rejected",
            receipt.parsed_reports, receipt.rejected_reports
        )));
    }
    Ok(())
}

async fn analyze_capture(json: bool, capture: &Path, json_out: Option<&Path>) -> Result<()> {
    let receipt = analyze_capture_file(capture)?;
    write_json_receipt(json_out, &receipt)?;
    print_capture_analysis_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza capture analysis failed: {} decoded, {} rejected",
            receipt.decoded_reports, receipt.rejected_reports
        )));
    }
    Ok(())
}

async fn analyze_lane(json: bool, lane: &Path, json_out: Option<&Path>) -> Result<()> {
    let receipt = analyze_lane_captures(lane)?;
    write_json_receipt(json_out, &receipt)?;
    print_lane_capture_analysis_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza lane capture analysis failed: {} of {} capture(s) decoded cleanly",
            receipt.analyzed_capture_count, receipt.required_capture_count
        )));
    }
    Ok(())
}

async fn sync_role_status(
    json: bool,
    lane: &Path,
    check: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = sync_role_status_receipt(lane, check)?;
    write_json_receipt(json_out, &receipt)?;
    print_role_status_sync_receipt(json, json_out, &receipt)?;
    if !json_bool(&receipt, "success").unwrap_or(false) {
        return Err(receipt_failure(
            "Moza manifest semantic_status fields are not in sync with lane capture analysis",
        ));
    }
    Ok(())
}

async fn validate_captures(json: bool, lane: &Path, json_out: Option<&Path>) -> Result<()> {
    let receipt = validate_lane_captures(lane)?;
    write_json_receipt(json_out, &receipt)?;
    print_capture_validation_set_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza lane capture validation failed: {} of {} required captures passed",
            receipt.validated_capture_count, receipt.required_capture_count
        )));
    }
    Ok(())
}

async fn promote_fixture(
    json: bool,
    capture: &Path,
    fixture_id: &str,
    fixture_out: &Path,
    pid_override: Option<&str>,
    max_reports: usize,
    overwrite: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let pid_override = pid_override
        .map(parse_required_hex_u16)
        .transpose()
        .with_context(|| "invalid --pid value")?;
    let fixture = build_capture_fixture(capture, fixture_id, pid_override, max_reports)?;

    if fixture_out.exists() && !overwrite {
        return Err(anyhow!(
            "fixture output '{}' already exists; pass --overwrite to replace it",
            fixture_out.display()
        ));
    }

    write_json_file(fixture_out, &fixture)?;

    let receipt = FixturePromotionReceipt {
        success: true,
        command: "wheelctl moza promote-fixture",
        generated_at_utc: now_utc(),
        capture: capture.display().to_string(),
        fixture_out: fixture_out.display().to_string(),
        fixture_id: fixture_id.to_string(),
        pid_override: pid_override.map(hex_u16),
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        no_hid_device_opened: true,
        overwritten_existing: overwrite,
        report_count: fixture.reports.len(),
        product_ids: fixture.product_ids.clone(),
        parsed_by_category: fixture.parsed_by_category.clone(),
        report_ids: fixture.report_ids.clone(),
        report_lengths: fixture.report_lengths.clone(),
        notes: vec![
            "promote-fixture replays JSONL input bytes through Moza parsers only; no HID device is opened".to_string(),
            "fixture output intentionally stores sanitized report bytes and parse summaries, not HID paths or serial numbers".to_string(),
        ],
    };

    write_json_receipt(json_out, &receipt)?;
    print_fixture_promotion_receipt(json, json_out, &receipt)
}

async fn promote_fixtures(
    json: bool,
    lane: &Path,
    fixture_dir: &Path,
    max_reports: usize,
    overwrite: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let mut fixtures = Vec::new();

    let requirements = passive_capture_requirements_for_lane(lane);
    validate_required_passive_captures_for_fixture_promotion(lane, &requirements)?;
    for requirement in &requirements {
        let capture = lane.join(requirement.relative_path);
        let fixture_out = fixture_dir.join(format!("{}.json", requirement.fixture_id));
        let fixture = build_capture_fixture(&capture, requirement.fixture_id, None, max_reports)
            .with_context(|| format!("failed to promote {}", capture.display()))?;

        if fixture_out.exists() && !overwrite {
            return Err(anyhow!(
                "fixture output '{}' already exists; pass --overwrite to replace it",
                fixture_out.display()
            ));
        }

        write_json_file(&fixture_out, &fixture)?;
        fixtures.push(FixturePromotionEntry {
            capture: capture.display().to_string(),
            fixture_out: fixture_out.display().to_string(),
            fixture_id: requirement.fixture_id.to_string(),
            report_count: fixture.reports.len(),
            product_ids: fixture.product_ids,
            parsed_by_category: fixture.parsed_by_category,
            report_ids: fixture.report_ids,
            report_lengths: fixture.report_lengths,
        });
    }

    let receipt = FixturePromotionSetReceipt {
        success: true,
        command: "wheelctl moza promote-fixtures",
        generated_at_utc: now_utc(),
        lane: lane.display().to_string(),
        fixture_dir: fixture_dir.display().to_string(),
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        no_hid_device_opened: true,
        overwritten_existing: overwrite,
        required_fixture_count: requirements.len(),
        fixture_count: fixtures.len(),
        fixtures,
        notes: vec![
            "promote-fixtures replays every required passive lane capture through Moza parsers only; no HID device is opened".to_string(),
            "fixture outputs intentionally store sanitized report bytes and parse summaries, not HID paths or serial numbers".to_string(),
        ],
    };

    write_json_receipt(json_out, &receipt)?;
    print_fixture_set_promotion_receipt(json, json_out, &receipt)
}

fn validate_required_passive_captures_for_fixture_promotion(
    lane: &Path,
    requirements: &[&PassiveCaptureRequirement],
) -> Result<()> {
    let mut failures = Vec::new();

    for requirement in requirements {
        let capture = lane.join(requirement.relative_path);
        let receipt = validate_capture_file(&capture, None)
            .with_context(|| format!("failed to validate {}", capture.display()))?;
        let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
        let evaluation =
            evaluate_passive_capture_requirement(requirement, &receipt, &expected_product_ids);

        if !receipt.success || !evaluation.success {
            failures.push(format!(
                "{}: success={}, expected_product_ids={:?}, product_ids={:?}, missing_requirements={:?}",
                requirement.relative_path,
                receipt.success,
                product_id_hex_list(&expected_product_ids),
                receipt.product_ids,
                evaluation.missing_requirements
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "refusing to promote passive fixtures until required captures validate: {}",
            failures.join("; ")
        ))
    }
}

async fn zero_torque(
    json: bool,
    selector: Option<&str>,
    pid_override: Option<&str>,
    dry_run: bool,
    repeat: u32,
    hz: u32,
    watchdog_timeout_ms: u64,
    json_out: Option<&Path>,
) -> Result<()> {
    validate_zero_torque_args(repeat, hz, watchdog_timeout_ms)?;

    if dry_run {
        let pid = zero_torque_dry_run_pid(selector, pid_override)?;
        if !is_wheelbase_product(pid) {
            return Err(anyhow!(
                "selected Moza PID is not an output-capable wheelbase: {}",
                hex_u16(pid)
            ));
        }
        let payload = zero_torque_payload_for_pid(pid);
        let mut receipt = ZeroTorqueProofReceipt::new(
            "wheelctl moza zero",
            selector.map(str::to_string),
            repeat,
            hz,
            watchdog_timeout_ms,
            synthetic_moza_device_record(pid),
            payload,
            true,
        );
        receipt.success = receipt.non_zero_payloads == 0 && !receipt.motor_enabled;
        receipt
            .notes
            .push("dry-run mode opened no HID device and sent no reports".to_string());
        receipt.set_receipt_path(json_out);

        write_json_receipt(json_out, &receipt)?;
        print_zero_torque_receipt(json, json_out, &receipt)?;
        if !receipt.success {
            return Err(receipt_failure(
                "Moza zero-torque dry-run payload was not safe zero",
            ));
        }
        return Ok(());
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    if !snapshot.output_capable {
        return Err(anyhow!(
            "selected Moza device is not an output-capable wheelbase: {} {}",
            snapshot.product_name,
            snapshot.product_id
        ));
    }

    let pid = parse_required_hex_u16(&snapshot.product_id)?;
    let payload = zero_torque_payload_for_pid(pid);
    let mut receipt = ZeroTorqueProofReceipt::new(
        "wheelctl moza zero",
        selector.map(str::to_string),
        repeat,
        hz,
        watchdog_timeout_ms,
        snapshot,
        payload,
        false,
    );

    let period = Duration::from_micros(1_000_000 / u64::from(hz));
    let watchdog_timeout = Duration::from_millis(watchdog_timeout_ms);
    let started_at = Instant::now();
    let mut next_write_at = Instant::now();
    let mut last_write_at: Option<Instant> = None;

    for sequence in 0..repeat {
        let now = Instant::now();
        if let Some(last) = last_write_at
            && now.duration_since(last) > watchdog_timeout
        {
            receipt.watchdog_faults += 1;
            receipt.abort_reason = Some("watchdog_timeout".to_string());
            break;
        }

        if now < next_write_at {
            std::thread::sleep(next_write_at - now);
        }

        receipt.write_attempts += 1;
        match device.write(&payload) {
            Ok(n) => {
                receipt.bytes_written_total += n;
                if let Some(error) = short_hid_write_error(n) {
                    receipt.write_errors += 1;
                    receipt.abort_reason = Some(error.clone());
                    receipt.record_command(ZeroTorqueCommandRecord::partial(
                        sequence,
                        "scheduled_zero",
                        started_at,
                        payload,
                        n,
                        error,
                    ));
                    break;
                }
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    sequence,
                    "scheduled_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
            Err(e) => {
                receipt.write_errors += 1;
                let error = e.to_string();
                receipt.abort_reason = Some(format!("hid_write_error: {e}"));
                receipt.record_command(ZeroTorqueCommandRecord::error(
                    sequence,
                    "scheduled_zero",
                    started_at,
                    payload,
                    error,
                ));
                break;
            }
        }

        last_write_at = Some(Instant::now());
        next_write_at = last_write_at.unwrap_or_else(Instant::now) + period;
    }

    receipt.final_zero_attempted = true;
    match device.write(&payload) {
        Ok(n) => {
            receipt.bytes_written_total += n;
            if let Some(error) = short_hid_write_error(n) {
                receipt.write_errors += 1;
                receipt.final_zero_error = Some(error.clone());
                receipt.record_command(ZeroTorqueCommandRecord::partial(
                    repeat,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                    error,
                ));
            } else {
                receipt.final_zero_sent = true;
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    repeat,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
        }
        Err(e) => {
            receipt.write_errors += 1;
            let error = e.to_string();
            receipt.final_zero_error = Some(error.clone());
            receipt.record_command(ZeroTorqueCommandRecord::error(
                repeat,
                "final_zero",
                started_at,
                payload,
                error,
            ));
        }
    }

    receipt.success = receipt.non_zero_payloads == 0
        && receipt.write_errors == 0
        && receipt.watchdog_faults == 0
        && receipt.final_zero_sent;
    receipt.set_receipt_path(json_out);

    write_json_receipt(json_out, &receipt)?;
    print_zero_torque_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza zero-torque proof failed: {} writes ok, {} write errors, final_zero_sent={}",
            receipt.writes_ok, receipt.write_errors, receipt.final_zero_sent
        )));
    }

    Ok(())
}

async fn watchdog_proof(
    json: bool,
    selector: Option<&str>,
    pid_override: Option<&str>,
    dry_run: bool,
    pre_zero_count: u32,
    hz: u32,
    watchdog_timeout_ms: u64,
    json_out: Option<&Path>,
) -> Result<()> {
    validate_watchdog_proof_args(pre_zero_count, hz, watchdog_timeout_ms)?;

    if dry_run {
        let pid = zero_torque_dry_run_pid(selector, pid_override)?;
        if !is_wheelbase_product(pid) {
            return Err(anyhow!(
                "selected Moza PID is not an output-capable wheelbase: {}",
                hex_u16(pid)
            ));
        }
        let payload = zero_torque_payload_for_pid(pid);
        let mut receipt = ZeroTorqueProofReceipt::new(
            "wheelctl moza watchdog-proof",
            selector.map(str::to_string),
            pre_zero_count,
            hz,
            watchdog_timeout_ms,
            synthetic_moza_device_record(pid),
            payload,
            true,
        );
        receipt.fault_injected = Some("watchdog_timeout");
        receipt.watchdog_faults = 1;
        receipt.watchdog_triggered = true;
        receipt.final_zero_attempted = true;
        receipt.success = receipt.no_nonzero_torque && receipt.watchdog_triggered;
        receipt
            .notes
            .push("dry-run mode opened no HID device and sent no reports".to_string());
        receipt.set_receipt_path(json_out);

        write_json_receipt(json_out, &receipt)?;
        print_watchdog_proof_receipt(json, json_out, &receipt)?;
        return Ok(());
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    if !snapshot.output_capable {
        return Err(anyhow!(
            "selected Moza device is not an output-capable wheelbase: {} {}",
            snapshot.product_name,
            snapshot.product_id
        ));
    }

    let pid = parse_required_hex_u16(&snapshot.product_id)?;
    let payload = zero_torque_payload_for_pid(pid);
    let mut receipt = ZeroTorqueProofReceipt::new(
        "wheelctl moza watchdog-proof",
        selector.map(str::to_string),
        pre_zero_count,
        hz,
        watchdog_timeout_ms,
        snapshot,
        payload,
        false,
    );
    receipt.fault_injected = Some("watchdog_timeout");
    receipt.notes.push(
        "watchdog-proof intentionally waits past the watchdog timeout, then sends final zero"
            .to_string(),
    );

    let period = Duration::from_micros(1_000_000 / u64::from(hz));
    let started_at = Instant::now();
    for sequence in 0..pre_zero_count {
        receipt.write_attempts += 1;
        match device.write(&payload) {
            Ok(n) => {
                receipt.bytes_written_total += n;
                if let Some(error) = short_hid_write_error(n) {
                    receipt.write_errors += 1;
                    receipt.abort_reason =
                        Some(format!("hid_write_error_before_watchdog: {error}"));
                    receipt.record_command(ZeroTorqueCommandRecord::partial(
                        sequence,
                        "scheduled_zero",
                        started_at,
                        payload,
                        n,
                        error,
                    ));
                    break;
                }
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    sequence,
                    "scheduled_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
            Err(e) => {
                receipt.write_errors += 1;
                let error = e.to_string();
                receipt.abort_reason = Some(format!("hid_write_error_before_watchdog: {e}"));
                receipt.record_command(ZeroTorqueCommandRecord::error(
                    sequence,
                    "scheduled_zero",
                    started_at,
                    payload,
                    error,
                ));
                break;
            }
        }
        std::thread::sleep(period);
    }

    if receipt.write_errors == 0 {
        std::thread::sleep(Duration::from_millis(
            watchdog_timeout_ms.saturating_add(10),
        ));
        receipt.watchdog_faults = 1;
        receipt.watchdog_triggered = true;
        receipt.abort_reason = Some("watchdog_timeout_injected".to_string());
    }

    receipt.final_zero_attempted = true;
    match device.write(&payload) {
        Ok(n) => {
            receipt.bytes_written_total += n;
            if let Some(error) = short_hid_write_error(n) {
                receipt.write_errors += 1;
                receipt.final_zero_error = Some(error.clone());
                receipt.record_command(ZeroTorqueCommandRecord::partial(
                    pre_zero_count,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                    error,
                ));
            } else {
                receipt.final_zero_sent = true;
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    pre_zero_count,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
        }
        Err(e) => {
            receipt.write_errors += 1;
            let error = e.to_string();
            receipt.final_zero_error = Some(error.clone());
            receipt.record_command(ZeroTorqueCommandRecord::error(
                pre_zero_count,
                "final_zero",
                started_at,
                payload,
                error,
            ));
        }
    }

    receipt.success = receipt.no_nonzero_torque
        && receipt.write_errors == 0
        && receipt.watchdog_faults == 1
        && receipt.watchdog_triggered
        && receipt.final_zero_sent;
    receipt.set_receipt_path(json_out);

    write_json_receipt(json_out, &receipt)?;
    print_watchdog_proof_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza watchdog proof failed: watchdog_triggered={}, write_errors={}, final_zero_sent={}",
            receipt.watchdog_triggered, receipt.write_errors, receipt.final_zero_sent
        )));
    }
    Ok(())
}

async fn disconnect_proof(
    json: bool,
    selector: Option<&str>,
    pid_override: Option<&str>,
    dry_run: bool,
    confirm_disconnect_test: bool,
    max_duration_ms: u64,
    hz: u32,
    json_out: Option<&Path>,
) -> Result<()> {
    validate_disconnect_proof_args(max_duration_ms, hz)?;
    if !dry_run && !confirm_disconnect_test {
        return Err(anyhow!(
            "--confirm-disconnect-test is required before the operator disconnect test"
        ));
    }

    if dry_run {
        let pid = zero_torque_dry_run_pid(selector, pid_override)?;
        if !is_wheelbase_product(pid) {
            return Err(anyhow!(
                "selected Moza PID is not an output-capable wheelbase: {}",
                hex_u16(pid)
            ));
        }
        let payload = zero_torque_payload_for_pid(pid);
        let mut receipt = ZeroTorqueProofReceipt::new(
            "wheelctl moza disconnect-proof",
            selector.map(str::to_string),
            disconnect_max_writes(max_duration_ms, hz),
            hz,
            100,
            synthetic_moza_device_record(pid),
            payload,
            true,
        );
        receipt.max_duration_ms = Some(max_duration_ms);
        receipt.fault_injected = Some("operator_disconnect");
        receipt.disconnect_observed = true;
        receipt.final_zero_attempted = true;
        receipt.success = receipt.no_nonzero_torque && receipt.disconnect_observed;
        receipt
            .notes
            .push("dry-run mode opened no HID device and sent no reports".to_string());
        receipt.set_receipt_path(json_out);

        write_json_receipt(json_out, &receipt)?;
        print_disconnect_proof_receipt(json, json_out, &receipt)?;
        return Ok(());
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    if !snapshot.output_capable {
        return Err(anyhow!(
            "selected Moza device is not an output-capable wheelbase: {} {}",
            snapshot.product_name,
            snapshot.product_id
        ));
    }

    let pid = parse_required_hex_u16(&snapshot.product_id)?;
    let payload = zero_torque_payload_for_pid(pid);
    let max_writes = disconnect_max_writes(max_duration_ms, hz);
    let mut receipt = ZeroTorqueProofReceipt::new(
        "wheelctl moza disconnect-proof",
        selector.map(str::to_string),
        max_writes,
        hz,
        100,
        snapshot,
        payload,
        false,
    );
    receipt.operator_confirmed = confirm_disconnect_test;
    receipt.max_duration_ms = Some(max_duration_ms);
    receipt.fault_injected = Some("operator_disconnect");
    receipt.notes.push(
        "disconnect-proof writes only zero torque while the operator disconnects the wheelbase"
            .to_string(),
    );
    receipt.notes.push(
        "final zero is attempted after disconnect; it may fail if the HID handle is already gone"
            .to_string(),
    );

    let period = Duration::from_micros(1_000_000 / u64::from(hz));
    let started_at = Instant::now();
    for sequence in 0..max_writes {
        receipt.write_attempts += 1;
        match device.write(&payload) {
            Ok(n) => {
                receipt.bytes_written_total += n;
                if let Some(error) = short_hid_write_error(n) {
                    receipt.write_errors += 1;
                    receipt.abort_reason = Some(error.clone());
                    receipt.record_command(ZeroTorqueCommandRecord::partial(
                        sequence,
                        "scheduled_zero",
                        started_at,
                        payload,
                        n,
                        error,
                    ));
                    break;
                }
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    sequence,
                    "scheduled_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
            Err(e) => {
                receipt.write_errors += 1;
                receipt.disconnect_observed = true;
                let error = e.to_string();
                receipt.abort_reason = Some(format!("hid_write_error_disconnect_observed: {e}"));
                receipt.record_command(ZeroTorqueCommandRecord::error(
                    sequence,
                    "disconnect_probe",
                    started_at,
                    payload,
                    error,
                ));
                break;
            }
        }
        std::thread::sleep(period);
    }

    if !receipt.disconnect_observed {
        receipt
            .abort_reason
            .get_or_insert_with(|| "disconnect_not_observed_before_timeout".to_string());
    }

    receipt.final_zero_attempted = true;
    match device.write(&payload) {
        Ok(n) => {
            receipt.bytes_written_total += n;
            if let Some(error) = short_hid_write_error(n) {
                receipt.write_errors += 1;
                receipt.final_zero_error = Some(error.clone());
                receipt.record_command(ZeroTorqueCommandRecord::partial(
                    receipt.write_attempts,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                    error,
                ));
            } else {
                receipt.final_zero_sent = true;
                receipt.writes_ok += 1;
                receipt.record_command(ZeroTorqueCommandRecord::ok(
                    receipt.write_attempts,
                    "final_zero",
                    started_at,
                    payload,
                    n,
                ));
            }
        }
        Err(e) => {
            receipt.write_errors += 1;
            let error = e.to_string();
            receipt.final_zero_error = Some(error.clone());
            receipt.record_command(ZeroTorqueCommandRecord::error(
                receipt.write_attempts,
                "final_zero",
                started_at,
                payload,
                error,
            ));
        }
    }

    receipt.success = receipt.no_nonzero_torque
        && receipt.operator_confirmed
        && receipt.disconnect_observed
        && receipt.final_zero_attempted;
    receipt.set_receipt_path(json_out);

    write_json_receipt(json_out, &receipt)?;
    print_disconnect_proof_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza disconnect proof failed: disconnect_observed={}, final_zero_attempted={}, non_zero_payloads={}",
            receipt.disconnect_observed, receipt.final_zero_attempted, receipt.non_zero_payloads
        )));
    }
    Ok(())
}

async fn init(
    json: bool,
    selector: Option<&str>,
    pid_override: Option<&str>,
    mode: MozaInitMode,
    dry_run: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let ffb_mode = init_mode_to_ffb_mode(mode);

    if dry_run {
        let pid = zero_torque_dry_run_pid(selector, pid_override)?;
        if !is_wheelbase_product(pid) {
            return Err(anyhow!(
                "selected Moza PID is not an output-capable wheelbase: {}",
                hex_u16(pid)
            ));
        }
        let protocol = MozaProtocol::new_with_config(pid, ffb_mode, false);
        let mut receipt = MozaInitReceipt::new(
            selector.map(str::to_string),
            synthetic_moza_device_record(pid),
            mode,
            true,
        );
        {
            let mut writer = RecordingFeatureWriter::dry_run(&mut receipt.feature_reports);
            protocol
                .initialize_device(&mut writer)
                .map_err(|e| anyhow!("Moza dry-run init failed: {e}"))?;
            receipt.output_report_attempts = writer.output_report_attempts;
        }
        receipt.finish_from_protocol(&protocol);
        receipt
            .notes
            .push("dry-run mode opened no HID device and sent no feature reports".to_string());
        receipt.set_receipt_path(json_out);

        write_json_receipt(json_out, &receipt)?;
        print_init_receipt(json, json_out, &receipt)?;
        return Ok(());
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    if !snapshot.output_capable {
        return Err(anyhow!(
            "selected Moza device is not an output-capable wheelbase: {} {}",
            snapshot.product_name,
            snapshot.product_id
        ));
    }

    let pid = parse_required_hex_u16(&snapshot.product_id)?;
    let protocol = MozaProtocol::new_with_config(pid, ffb_mode, false);
    let mut receipt = MozaInitReceipt::new(selector.map(str::to_string), snapshot, mode, false);
    {
        let mut writer = RecordingFeatureWriter::new(device, &mut receipt.feature_reports);
        protocol
            .initialize_device(&mut writer)
            .map_err(|e| anyhow!("Moza init failed: {e}"))?;
        receipt.output_report_attempts = writer.output_report_attempts;
    }
    receipt.finish_from_protocol(&protocol);
    receipt.set_receipt_path(json_out);

    write_json_receipt(json_out, &receipt)?;
    print_init_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza init failed: mode={}, init_state={}, feature_write_errors={}, output_report_attempts={}",
            receipt.mode,
            receipt.init_state,
            receipt.feature_write_errors,
            receipt.output_report_attempts
        )));
    }
    Ok(())
}

async fn torque_test(request: TorqueTestRequest<'_>) -> Result<()> {
    let TorqueTestRequest {
        json,
        selector,
        pid_override,
        zero_proof,
        descriptor,
        lane,
        init_off,
        init_standard,
        dry_run,
        confirm_low_torque,
        explicit_operator_override,
        max_percent,
        duration_ms,
        hz,
        json_out,
    } = request;

    validate_torque_test_args(max_percent, duration_ms, hz)?;
    if !dry_run && !confirm_low_torque {
        return Err(anyhow!(
            "--confirm-low-torque is required before actual low-torque writes"
        ));
    }

    if dry_run {
        let zero_proof_summary = zero_proof
            .map(validate_zero_proof_for_torque_test)
            .transpose()?;
        let init_proofs =
            validate_init_proofs_for_torque_test(lane, init_off, init_standard, true)?;
        let pid = zero_torque_dry_run_pid(selector, pid_override)?;
        if !is_wheelbase_product(pid) {
            return Err(anyhow!(
                "selected Moza PID is not an output-capable wheelbase: {}",
                hex_u16(pid)
            ));
        }
        let device = synthetic_moza_device_record(pid);
        let mut receipt = LowTorqueProofReceipt::new(
            selector.map(str::to_string),
            device,
            zero_proof_summary,
            max_percent,
            duration_ms,
            hz,
            true,
        );
        receipt.apply_init_proofs(init_proofs);
        receipt.apply_direct_mode_gate(DirectModeGateSummary::dry_run(
            descriptor.map(Path::to_path_buf),
            explicit_operator_override,
        ));
        receipt.plan_only();
        receipt.success = receipt.no_nonzero_above_limit
            && receipt.final_zero_sent
            && receipt.high_torque == Some(false);
        receipt
            .notes
            .push("dry-run mode opened no HID device and sent no reports".to_string());
        receipt.set_receipt_path(json_out);

        write_json_receipt(json_out, &receipt)?;
        print_low_torque_receipt(json, json_out, &receipt)?;
        return Ok(());
    }

    let preflight = validate_low_torque_real_hardware_preflight(
        selector,
        pid_override,
        lane,
        zero_proof,
        init_off,
        init_standard,
        descriptor,
        explicit_operator_override,
    )?;
    let api = HidApi::new().context("failed to initialize HID API")?;
    let (device, snapshot) = open_single_moza_device(&api, selector)?;
    if !snapshot.output_capable {
        return Err(anyhow!(
            "selected Moza device is not an output-capable wheelbase: {} {}",
            snapshot.product_name,
            snapshot.product_id
        ));
    }
    if preflight.target_product_id != snapshot.product_id {
        return Err(anyhow!(
            "preflight target PID {} does not match selected device PID {}",
            preflight.target_product_id,
            snapshot.product_id
        ));
    }
    if preflight.zero_proof.product_id.as_deref() != Some(snapshot.product_id.as_str()) {
        return Err(anyhow!(
            "zero proof PID {:?} does not match selected device PID {}",
            preflight.zero_proof.product_id,
            snapshot.product_id
        ));
    }
    if !preflight.init_proofs.match_product_id(&snapshot.product_id) {
        return Err(anyhow!(
            "init proof PID(s) {:?}/{:?} do not match selected device PID {}",
            preflight.init_proofs.off.product_id,
            preflight.init_proofs.standard.product_id,
            snapshot.product_id
        ));
    }

    let mut receipt = LowTorqueProofReceipt::new(
        selector.map(str::to_string),
        snapshot,
        Some(preflight.zero_proof),
        max_percent,
        duration_ms,
        hz,
        false,
    );
    receipt.apply_init_proofs(Some(preflight.init_proofs));
    receipt.apply_direct_mode_gate(preflight.direct_mode_gate);
    let period = Duration::from_micros(1_000_000 / u64::from(hz));
    let started_at = Instant::now();
    let mut sequence = 0u32;

    'ladder: for stage in receipt.ladder.clone() {
        let writes = stage.write_count;
        for _ in 0..writes {
            let payload = stage.payload;
            receipt.write_attempts += 1;
            match device.write(&payload) {
                Ok(n) => {
                    receipt.bytes_written_total += n;
                    if let Some(error) = short_hid_write_error(n) {
                        receipt.write_errors += 1;
                        receipt.abort_reason = Some(error.clone());
                        receipt.record_command(LowTorqueCommandRecord::partial(
                            sequence,
                            "low_torque",
                            started_at,
                            stage.percent,
                            payload,
                            n,
                            error,
                        ));
                        break 'ladder;
                    }
                    receipt.writes_ok += 1;
                    receipt.record_command(LowTorqueCommandRecord::ok(
                        sequence,
                        "low_torque",
                        started_at,
                        stage.percent,
                        payload,
                        n,
                    ));
                }
                Err(e) => {
                    receipt.write_errors += 1;
                    let error = e.to_string();
                    receipt.abort_reason = Some(format!("hid_write_error: {e}"));
                    receipt.record_command(LowTorqueCommandRecord::error(
                        sequence,
                        "low_torque",
                        started_at,
                        stage.percent,
                        payload,
                        error,
                    ));
                    break 'ladder;
                }
            }
            sequence += 1;
            std::thread::sleep(period);
        }
    }

    let final_zero =
        zero_torque_payload_for_pid(parse_required_hex_u16(&receipt.device.product_id)?);
    receipt.final_zero_attempted = true;
    match device.write(&final_zero) {
        Ok(n) => {
            receipt.bytes_written_total += n;
            if let Some(error) = short_hid_write_error(n) {
                receipt.write_errors += 1;
                receipt.final_zero_error = Some(error.clone());
                receipt.record_command(LowTorqueCommandRecord::partial(
                    sequence,
                    "final_zero",
                    started_at,
                    0.0,
                    final_zero,
                    n,
                    error,
                ));
            } else {
                receipt.final_zero_sent = true;
                receipt.writes_ok += 1;
                receipt.record_command(LowTorqueCommandRecord::ok(
                    sequence,
                    "final_zero",
                    started_at,
                    0.0,
                    final_zero,
                    n,
                ));
            }
        }
        Err(e) => {
            receipt.write_errors += 1;
            let error = e.to_string();
            receipt.final_zero_error = Some(error.clone());
            receipt.record_command(LowTorqueCommandRecord::error(
                sequence,
                "final_zero",
                started_at,
                0.0,
                final_zero,
                error,
            ));
        }
    }

    receipt.success = receipt.zero_proof_validated
        && receipt.confirmed
        && receipt.init_proofs_validated
        && receipt.no_high_torque
        && receipt.high_torque == Some(false)
        && receipt.direct_mode_gate_satisfied
        && receipt.no_nonzero_above_limit
        && receipt.write_errors == 0
        && receipt.final_zero_sent;
    receipt.set_receipt_path(json_out);

    write_json_receipt(json_out, &receipt)?;
    print_low_torque_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza low-torque proof failed: {} writes ok, {} write errors, final_zero_sent={}",
            receipt.writes_ok, receipt.write_errors, receipt.final_zero_sent
        )));
    }
    Ok(())
}

async fn verify_bundle(
    json: bool,
    lane: &Path,
    stage: MozaBundleStage,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = verify_bundle_dir(lane, stage);
    write_json_receipt(json_out, &receipt)?;
    print_bundle_verification_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza bundle verification failed: {} missing artifact(s), {} invalid artifact(s), {} failed gate(s)",
            receipt.missing_artifacts, receipt.invalid_artifacts, receipt.failed_gates
        )));
    }
    Ok(())
}

async fn audit_lane(
    json: bool,
    lane: &Path,
    stage: MozaBundleStage,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = audit_lane_dir(lane, stage);
    write_json_receipt(json_out, &receipt)?;
    print_lane_audit_receipt(json, json_out, &receipt)?;
    if !receipt.success {
        return Err(receipt_failure(format!(
            "Moza lane audit failed: {} missing receipt(s), {} invalid receipt(s), live_verification_success={}",
            receipt.missing_receipts, receipt.invalid_receipts, receipt.live_verification_success
        )));
    }
    Ok(())
}

async fn receipt_template(
    json: bool,
    kind: MozaReceiptTemplateKind,
    json_out: &Path,
    overwrite: bool,
) -> Result<()> {
    if json_out.exists() && !overwrite {
        return Err(anyhow!(
            "'{}' already exists; pass --overwrite to replace it",
            json_out.display()
        ));
    }

    let receipt = moza_receipt_template(kind);
    write_json_file(json_out, &receipt)?;
    print_receipt_template(json, kind, json_out, &receipt)
}

async fn pit_house_observation(request: PitHouseObservationRequest<'_>) -> Result<()> {
    let PitHouseObservationRequest {
        json,
        case,
        evidence_kind,
        evidence_artifact,
        operator,
        evidence,
        json_out,
        overwrite,
    } = request;

    if evidence.trim().is_empty() {
        return Err(anyhow!(
            "--evidence must describe the observed Pit House state"
        ));
    }
    if operator.trim().is_empty() {
        return Err(anyhow!("--operator must not be empty"));
    }
    if matches!(evidence_kind, MozaPitHouseEvidenceKind::OperatorNotes) {
        return Err(anyhow!(
            "--evidence-kind operator-notes is not sufficient for verifier-accepted Pit House observations"
        ));
    }
    let evidence_artifact = evidence_artifact
        .ok_or_else(|| anyhow!("--evidence-artifact is required for Pit House observations"))?;
    let evidence_artifact =
        simple_lane_relative_path_string(evidence_artifact, "evidence artifact")?;
    let evidence_root = json_out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let evidence_path = evidence_root.join(&evidence_artifact);
    if !evidence_path.is_file() {
        return Err(anyhow!(
            "Pit House evidence artifact '{}' must exist before writing observation receipt '{}'",
            evidence_path.display(),
            json_out.display()
        ));
    }

    let receipt = serde_json::json!({
        "success": true,
        "command": "wheelctl moza pit-house-observation",
        "generated_at_utc": now_utc(),
        "case": pit_house_observation_case_id(case),
        "observed": true,
        "pit_house_observed_state": pit_house_observation_state(case),
        "evidence_kind": pit_house_evidence_kind_label(evidence_kind),
        "observed_at_utc": now_utc(),
        "operator": operator,
        "evidence": evidence,
        "evidence_artifact": evidence_artifact,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true
    });

    ensure_receipt_writable(json_out, overwrite)?;
    write_json_receipt(Some(json_out), &receipt)?;
    print_proof_receipt(json, json_out, "Pit House observation", &receipt)
}

async fn pit_house_case(request: PitHouseCaseRequest<'_>) -> Result<()> {
    let PitHouseCaseRequest {
        json,
        lane,
        case,
        observation_artifact,
        evidence,
        json_out,
        overwrite,
    } = request;

    if evidence.trim().is_empty() {
        return Err(anyhow!(
            "--evidence must describe the Pit House case result"
        ));
    }

    let observation_artifact = lane_relative_artifact_path(lane, observation_artifact)?;
    let receipt = pit_house_case_artifact_receipt(case, &observation_artifact, evidence);
    let case_id = pit_house_observation_case_id(case);
    if !pit_house_case_observation_is_safe(lane, &receipt, case_id) {
        return Err(anyhow!(
            "Pit House observation artifact '{}' does not match case {}",
            observation_artifact,
            case_id
        ));
    }
    if !pit_house_case_source_is_safe(lane, &receipt, case_id, SupportBundleValidationMode::Fresh) {
        return Err(anyhow!(
            "Pit House source receipt for case {} is missing or failed verification",
            case_id
        ));
    }

    let (output_write_path, output_artifact_path) = lane_relative_output_path(lane, json_out)?;
    ensure_receipt_writable(&output_write_path, overwrite)?;
    write_json_receipt(Some(&output_write_path), &receipt)?;
    if !pit_house_case_artifact_is_safe(
        lane,
        &output_artifact_path,
        case_id,
        json_string(&receipt, "result"),
        SupportBundleValidationMode::Fresh,
    ) {
        return Err(anyhow!(
            "generated Pit House case artifact '{}' failed verifier contract",
            output_write_path.display()
        ));
    }

    print_proof_receipt(json, &output_write_path, "Pit House case", &receipt)
}

async fn pit_house_proof(request: PitHouseProofRequest<'_>) -> Result<()> {
    let PitHouseProofRequest {
        json,
        lane,
        closed_artifact,
        open_standard_artifact,
        direct_artifact,
        mode_change_artifact,
        firmware_page_artifact,
        shared_control_risk,
        json_out,
        overwrite,
    } = request;

    let cases = [
        pit_house_proof_case(lane, "pit_house_closed", closed_artifact)?,
        pit_house_proof_case(lane, "pit_house_open_idle_standard", open_standard_artifact)?,
        pit_house_proof_case(lane, "pit_house_open_direct", direct_artifact)?,
        pit_house_proof_case(
            lane,
            "pit_house_mode_change_during_run",
            mode_change_artifact,
        )?,
        pit_house_proof_case(
            lane,
            "pit_house_firmware_update_page_open",
            firmware_page_artifact,
        )?,
    ];
    let direct_requires_ack = cases
        .iter()
        .find(|case| case.case_id == "pit_house_open_direct")
        .map(|case| {
            json_bool(&case.artifact, "blocked") == Some(true)
                || json_bool(&case.artifact, "operator_ack_required") == Some(true)
        })
        .unwrap_or(false);
    let firmware_page_blocks_high_risk = cases
        .iter()
        .find(|case| case.case_id == "pit_house_firmware_update_page_open")
        .map(|case| json_bool(&case.artifact, "high_risk_refused") == Some(true))
        .unwrap_or(false);
    let detection_scope_ok = matches!(
        shared_control_risk,
        "detected" | "warned" | "documented_limit"
    );
    let success = detection_scope_ok
        && direct_requires_ack
        && firmware_page_blocks_high_risk
        && cases.iter().all(|case| case.safe);
    let entries: Vec<Value> = cases.into_iter().map(|case| case.entry).collect();
    let receipt = serde_json::json!({
        "success": success,
        "template": false,
        "evidence_status": "observed_on_real_hardware",
        "command": "wheelctl moza pit-house-proof",
        "generated_at_utc": now_utc(),
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "high_torque": false,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "direct_requires_ack": direct_requires_ack,
        "firmware_page_blocks_high_risk": firmware_page_blocks_high_risk,
        "shared_control_risk": shared_control_risk,
        "cases": entries
    });

    let output = proof_output_path(lane, json_out, "pit-house-coexistence.json");
    ensure_receipt_writable(&output, overwrite)?;
    write_json_receipt(Some(&output), &receipt)?;
    print_proof_receipt(json, &output, "Pit House coexistence", &receipt)?;
    if !success {
        return Err(receipt_failure(
            "Moza Pit House coexistence proof failed safety checks",
        ));
    }
    Ok(())
}

async fn simulator_telemetry_proof(
    json: bool,
    lane: &Path,
    game: &str,
    telemetry_source: &str,
    recorder_artifact: &Path,
    duration_ms: u64,
    json_out: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let recorder_artifact = lane_relative_artifact_path(lane, recorder_artifact)?;
    let records = read_telemetry_artifact_records(lane, &recorder_artifact)
        .ok_or_else(|| anyhow!("failed to read normalized telemetry artifact"))?;
    let snapshot_count = u64::try_from(records.len()).context("too many telemetry records")?;
    let source_ok = matches!(telemetry_source, "real_game" | "simhub_bridge");
    let telemetry_provenance = simulator_telemetry_provenance_for_records(&records);
    let provenance_matches_run = telemetry_provenance
        .as_ref()
        .map(|provenance| {
            provenance.game == game && provenance.telemetry_source == telemetry_source
        })
        .unwrap_or(false);
    let artifact_valid = simulator_telemetry_artifact_is_valid(
        lane,
        &recorder_artifact,
        snapshot_count,
        duration_ms,
    );
    let success = !game.trim().is_empty()
        && source_ok
        && provenance_matches_run
        && duration_ms > 0
        && snapshot_count > 0
        && artifact_valid;
    let recorder_command = telemetry_provenance
        .as_ref()
        .map(|provenance| provenance.recorder_command.as_str());
    let recorder_session_id = telemetry_provenance
        .as_ref()
        .map(|provenance| provenance.recorder_session_id.as_str());
    let receipt = serde_json::json!({
        "success": success,
        "command": "wheelctl moza simulator-telemetry-proof",
        "generated_at_utc": now_utc(),
        "game": game,
        "telemetry_source": telemetry_source,
        "recorder_command": recorder_command,
        "recorder_session_id": recorder_session_id,
        "hardware_output_enabled": false,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "normalized_snapshot_count": snapshot_count,
        "duration_ms": duration_ms,
        "recorder_artifact": recorder_artifact,
        "faults": []
    });

    let output = proof_output_path(lane, json_out, "simulator-telemetry-proof.json");
    ensure_receipt_writable(&output, overwrite)?;
    write_json_receipt(Some(&output), &receipt)?;
    print_proof_receipt(json, &output, "simulator telemetry", &receipt)?;
    if !success {
        return Err(receipt_failure(
            "Moza simulator telemetry proof failed safety checks",
        ));
    }
    Ok(())
}

async fn simulator_ffb_smoke(request: SimulatorFfbSmokeRequest<'_>) -> Result<()> {
    let SimulatorFfbSmokeRequest {
        json,
        lane,
        game,
        telemetry_source,
        output_log_artifact,
        descriptor_trusted,
        explicit_operator_override,
        watchdog_timeout_ms,
        stop_cleared_output,
        pause_cleared_output,
        game_exit_cleared_output,
        json_out,
        overwrite,
    } = request;

    let telemetry_receipt = read_json_value(lane, "simulator-telemetry-proof.json")?;
    let input_telemetry_artifact = json_string(&telemetry_receipt, "recorder_artifact")
        .or_else(|| json_string(&telemetry_receipt, "normalized_snapshot_artifact"))
        .ok_or_else(|| anyhow!("simulator-telemetry-proof.json is missing recorder_artifact"))?;
    let input_telemetry_snapshot_count = json_u64(&telemetry_receipt, "normalized_snapshot_count")
        .ok_or_else(|| {
            anyhow!("simulator-telemetry-proof.json is missing normalized_snapshot_count")
        })?;
    let input_telemetry_recorder_session_id =
        json_string(&telemetry_receipt, "recorder_session_id").ok_or_else(|| {
            anyhow!("simulator-telemetry-proof.json is missing recorder_session_id")
        })?;
    let telemetry_receipt_ok = verify_simulator_telemetry_gate(lane).status == "pass";
    let telemetry_receipt_matches_run = json_string(&telemetry_receipt, "game") == Some(game)
        && json_string(&telemetry_receipt, "telemetry_source") == Some(telemetry_source);
    let output_pid = lane_manifest_r5_pid(lane).unwrap_or(product_ids::R5_V2);
    let output_log_artifact = lane_relative_artifact_path(lane, output_log_artifact)?;
    let records = read_receipt_artifact_records(
        lane,
        &output_log_artifact,
        &["output_log", "records", "commands", "reports"],
    )
    .ok_or_else(|| anyhow!("failed to read simulator FFB output log artifact"))?;
    let output_report_count = u64::try_from(records.len()).context("too many output records")?;
    let mut nonzero_output_count = 0u64;
    let mut zero_output_count = 0u64;
    let mut max_abs_output_percent = 0.0f64;
    let mut all_direct_records = true;
    for record in &records {
        let Some(output) = direct_torque_artifact_record(record) else {
            all_direct_records = false;
            continue;
        };
        max_abs_output_percent = max_abs_output_percent.max(output.percent.abs());
        if output.torque_raw == 0 {
            zero_output_count += 1;
        } else {
            nonzero_output_count += 1;
        }
    }
    let output_provenance = simulator_ffb_output_provenance_for_records(lane, &records, output_pid);
    let writer_started_at_utc = output_provenance
        .as_ref()
        .map(|provenance| provenance.writer_started_at_utc.as_str());
    let writer_completed_at_utc = output_provenance
        .as_ref()
        .map(|provenance| provenance.writer_completed_at_utc.as_str());
    let prerequisite_artifacts =
        simulator_ffb_prerequisite_artifact_summaries(lane).unwrap_or_default();
    let prerequisite_artifacts_bound = simulator_ffb_prerequisite_artifacts_are_ordered(
        &prerequisite_artifacts,
        writer_started_at_utc,
    );
    let prerequisite_artifacts_value = serde_json::to_value(&prerequisite_artifacts)?;
    let last_record = records.last();
    let final_zero_attempted =
        last_record.and_then(|record| json_string(record, "kind")) == Some("final_zero");
    let final_zero_sent = last_record
        .map(|record| {
            json_string(record, "result") == Some("ok") && zero_payload_record_is_safe(record)
        })
        .unwrap_or(false);
    let final_zero_payload_hex = last_record
        .and_then(|record| json_string(record, "payload_hex"))
        .unwrap_or_default();
    let mode_mismatch_cleared_output = simulator_ffb_clear_events_for_records(&records)
        .mode_mismatch
        .is_some();
    let source_ok = matches!(telemetry_source, "real_game" | "simhub_bridge");
    let descriptor_trust_observed = lane_descriptor_trusted_for_pid(lane, &hex_u16(output_pid));
    let descriptor_trust_valid = !descriptor_trusted || descriptor_trust_observed;
    let direct_mode_allowed = ((descriptor_trusted && descriptor_trust_observed)
        || explicit_operator_override)
        && descriptor_trust_valid;
    let prerequisite_gates = simulator_ffb_prerequisite_gates(lane);
    let hardware_prerequisites_validated =
        prerequisite_gates.iter().all(|gate| gate.status == "pass");
    let prerequisite_gates_value = serde_json::to_value(&prerequisite_gates)?;
    let ffb_scalars_by_sequence = simulator_telemetry_ffb_scalars_by_sequence(
        lane,
        input_telemetry_artifact,
        input_telemetry_snapshot_count,
    )
    .unwrap_or_default();
    let telemetry_link = SimulatorFfbTelemetryLink {
        artifact: input_telemetry_artifact.to_string(),
        snapshot_count: input_telemetry_snapshot_count,
        recorder_session_id: input_telemetry_recorder_session_id.to_string(),
        game: game.to_string(),
        telemetry_source: telemetry_source.to_string(),
        ffb_scalars_by_sequence,
    };
    let output_artifact_safe = all_direct_records
        && simulator_ffb_output_artifact_is_safe(
            lane,
            &output_log_artifact,
            output_report_count,
            nonzero_output_count,
            zero_output_count,
            max_abs_output_percent,
            output_pid,
            &telemetry_link,
        );
    let success = !game.trim().is_empty()
        && source_ok
        && telemetry_receipt_ok
        && telemetry_receipt_matches_run
        && hardware_prerequisites_validated
        && prerequisite_artifacts_bound
        && direct_mode_allowed
        && watchdog_timeout_ms > 0
        && output_report_count > 0
        && nonzero_output_count > 0
        && zero_output_count > 0
        && output_provenance.is_some()
        && max_abs_output_percent > 0.0
        && max_abs_output_percent <= 5.0
        && final_zero_attempted
        && final_zero_sent
        && stop_cleared_output
        && pause_cleared_output
        && game_exit_cleared_output
        && mode_mismatch_cleared_output
        && simulator_telemetry_artifact_is_valid(
            lane,
            input_telemetry_artifact,
            input_telemetry_snapshot_count,
            json_u64(&telemetry_receipt, "duration_ms").unwrap_or(0),
        )
        && output_artifact_safe;
    let writer_command = output_provenance
        .as_ref()
        .map(|provenance| provenance.writer_command.as_str());
    let writer_session_id = output_provenance
        .as_ref()
        .map(|provenance| provenance.writer_session_id.as_str());
    let writer_device_path = output_provenance
        .as_ref()
        .map(|provenance| provenance.device_path.as_str());
    let writer_product_id = output_provenance
        .as_ref()
        .map(|provenance| provenance.product_id.as_str());
    let writer_hardware_lane = output_provenance
        .as_ref()
        .map(|provenance| provenance.hardware_lane.as_str());
    let mut receipt = serde_json::Map::new();
    receipt.insert("success".to_string(), Value::Bool(success));
    receipt.insert(
        "command".to_string(),
        Value::String("wheelctl moza simulator-ffb-smoke".to_string()),
    );
    receipt.insert("generated_at_utc".to_string(), Value::String(now_utc()));
    receipt.insert("game".to_string(), Value::String(game.to_string()));
    receipt.insert(
        "telemetry_source".to_string(),
        Value::String(telemetry_source.to_string()),
    );
    receipt.insert("hardware".to_string(), Value::String("moza-r5".to_string()));
    receipt.insert("ffb_mode".to_string(), Value::String("direct".to_string()));
    receipt.insert(
        "descriptor_trusted".to_string(),
        Value::Bool(descriptor_trusted),
    );
    receipt.insert(
        "descriptor_trust_observed".to_string(),
        Value::Bool(descriptor_trust_observed),
    );
    receipt.insert(
        "explicit_operator_override".to_string(),
        Value::Bool(explicit_operator_override),
    );
    receipt.insert("high_torque".to_string(), Value::Bool(false));
    receipt.insert("no_high_torque".to_string(), Value::Bool(true));
    receipt.insert("no_hid_device_opened".to_string(), Value::Bool(false));
    receipt.insert("no_ffb_writes".to_string(), Value::Bool(false));
    receipt.insert("no_serial_config_commands".to_string(), Value::Bool(true));
    receipt.insert("no_firmware_or_dfu_commands".to_string(), Value::Bool(true));
    receipt.insert(
        "hardware_prerequisites_validated".to_string(),
        Value::Bool(hardware_prerequisites_validated),
    );
    receipt.insert("prerequisite_gates".to_string(), prerequisite_gates_value);
    receipt.insert(
        "prerequisite_artifacts".to_string(),
        prerequisite_artifacts_value,
    );
    receipt.insert(
        "device".to_string(),
        serde_json::json!({
            "vendor_id": "0x346E",
            "product_id": hex_u16(output_pid),
            "product_name": "Moza R5",
            "output_capable": true
        }),
    );
    receipt.insert("hardware_output_enabled".to_string(), Value::Bool(true));
    receipt.insert(
        "max_output_percent".to_string(),
        serde_json::json!(max_abs_output_percent),
    );
    receipt.insert(
        "max_abs_output_percent".to_string(),
        serde_json::json!(max_abs_output_percent),
    );
    receipt.insert("watchdog_active".to_string(), Value::Bool(true));
    receipt.insert(
        "watchdog_timeout_ms".to_string(),
        Value::Number(watchdog_timeout_ms.into()),
    );
    receipt.insert(
        "output_report_count".to_string(),
        Value::Number(output_report_count.into()),
    );
    receipt.insert(
        "nonzero_output_count".to_string(),
        Value::Number(nonzero_output_count.into()),
    );
    receipt.insert(
        "zero_output_count".to_string(),
        Value::Number(zero_output_count.into()),
    );
    receipt.insert(
        "input_telemetry_artifact".to_string(),
        Value::String(input_telemetry_artifact.to_string()),
    );
    receipt.insert(
        "input_telemetry_snapshot_count".to_string(),
        Value::Number(input_telemetry_snapshot_count.into()),
    );
    receipt.insert(
        "input_telemetry_recorder_session_id".to_string(),
        Value::String(input_telemetry_recorder_session_id.to_string()),
    );
    receipt.insert(
        "output_log_artifact".to_string(),
        Value::String(output_log_artifact),
    );
    receipt.insert(
        "output_log_provenance_valid".to_string(),
        Value::Bool(output_provenance.is_some()),
    );
    receipt.insert(
        "writer_command".to_string(),
        writer_command.map(Value::from).unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_session_id".to_string(),
        writer_session_id.map(Value::from).unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_device_path".to_string(),
        writer_device_path.map(Value::from).unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_product_id".to_string(),
        writer_product_id.map(Value::from).unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_hardware_lane".to_string(),
        writer_hardware_lane.map(Value::from).unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_started_at_utc".to_string(),
        writer_started_at_utc
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    receipt.insert(
        "writer_completed_at_utc".to_string(),
        writer_completed_at_utc
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    receipt.insert(
        "final_zero_attempted".to_string(),
        Value::Bool(final_zero_attempted),
    );
    receipt.insert("final_zero_sent".to_string(), Value::Bool(final_zero_sent));
    receipt.insert(
        "final_zero_payload_hex".to_string(),
        Value::String(final_zero_payload_hex.to_string()),
    );
    receipt.insert(
        "stop_cleared_output".to_string(),
        Value::Bool(stop_cleared_output),
    );
    receipt.insert(
        "pause_cleared_output".to_string(),
        Value::Bool(pause_cleared_output),
    );
    receipt.insert(
        "game_exit_cleared_output".to_string(),
        Value::Bool(game_exit_cleared_output),
    );
    receipt.insert(
        "mode_mismatch_cleared_output".to_string(),
        Value::Bool(mode_mismatch_cleared_output),
    );
    receipt.insert("faults".to_string(), Value::Array(Vec::new()));
    let receipt = Value::Object(receipt);

    let output = proof_output_path(lane, json_out, "simulator-ffb-smoke.json");
    ensure_receipt_writable(&output, overwrite)?;
    write_json_receipt(Some(&output), &receipt)?;
    print_proof_receipt(json, &output, "simulator FFB smoke", &receipt)?;
    if !success {
        return Err(receipt_failure(
            "Moza simulator FFB smoke proof failed safety checks",
        ));
    }
    Ok(())
}

struct ProducedPitHouseCase {
    case_id: &'static str,
    entry: Value,
    artifact: Value,
    safe: bool,
}

fn pit_house_proof_case(
    lane: &Path,
    case_id: &'static str,
    artifact_path: &Path,
) -> Result<ProducedPitHouseCase> {
    let artifact_path = lane_relative_artifact_path(lane, artifact_path)?;
    let artifact = read_json_value(lane, &artifact_path)?;
    let result = json_string(&artifact, "result")
        .ok_or_else(|| anyhow!("{artifact_path} is missing result"))?;
    let observed = json_bool(&artifact, "observed").unwrap_or(false);
    let high_torque = json_bool(&artifact, "high_torque").unwrap_or(true);
    let evidence = json_string(&artifact, "evidence")
        .or_else(|| json_string(&artifact, "operator_notes"))
        .unwrap_or_default();
    let safe = pit_house_case_artifact_is_safe(
        lane,
        &artifact_path,
        case_id,
        Some(result),
        SupportBundleValidationMode::Fresh,
    );
    let mut entry = serde_json::json!({
        "case": case_id,
        "observed": observed,
        "result": result,
        "high_torque": high_torque,
        "evidence": evidence,
        "artifact": artifact_path
    });
    if let Some(object) = entry.as_object_mut() {
        for key in [
            "source_receipt",
            "source_gate",
            "source_log",
            "source_record_kind",
            "source_clear_event",
            "pit_house_observation_artifact",
            "blocked",
            "operator_ack_required",
            "mismatch_detected",
            "failed_safe",
            "high_risk_refused",
        ] {
            if let Some(value) = json_bool(&artifact, key) {
                object.insert(key.to_string(), Value::Bool(value));
            } else if let Some(value) = json_string(&artifact, key) {
                object.insert(key.to_string(), Value::String(value.to_string()));
            }
        }
        for key in ["source_record_kinds", "source_requires_final_zero"] {
            if let Some(value) = artifact.get(key) {
                object.insert(key.to_string(), value.clone());
            }
        }
    }
    Ok(ProducedPitHouseCase {
        case_id,
        entry,
        artifact,
        safe,
    })
}

fn lane_manifest_r5_pid(lane: &Path) -> Option<u16> {
    let manifest = read_json_value(lane, "manifest.json").ok()?;
    let hardware = manifest.get("hardware")?;
    let pid = json_string(hardware, "wheelbase_pid").and_then(parse_hex_selector)?;
    matches!(pid, product_ids::R5_V1 | product_ids::R5_V2).then_some(pid)
}

fn proof_output_path(lane: &Path, json_out: Option<&Path>, default_file: &str) -> PathBuf {
    json_out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| lane.join(default_file))
}

fn ensure_receipt_writable(path: &Path, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        return Err(anyhow!(
            "{} already exists; pass --overwrite to replace it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    Ok(())
}

fn lane_relative_artifact_path(lane: &Path, artifact: &Path) -> Result<String> {
    let relative = if artifact.is_absolute() {
        let absolute_lane = std::path::absolute(lane)
            .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
        let absolute_artifact = std::path::absolute(artifact)
            .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
        absolute_artifact
            .strip_prefix(&absolute_lane)
            .with_context(|| {
                format!(
                    "artifact '{}' must be under lane '{}'",
                    artifact.display(),
                    lane.display()
                )
            })?
            .to_path_buf()
    } else if let Some(relative) = lane_prefixed_relative_path(lane, artifact) {
        relative
    } else if let Some(relative) = cwd_relative_lane_prefixed_path(lane, artifact)? {
        relative
    } else {
        artifact.to_path_buf()
    };
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "artifact '{}' must be a simple lane-relative path",
            artifact.display()
        ));
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn simple_lane_relative_path_string(path: &Path, label: &str) -> Result<String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "{label} '{}' must be a simple lane-relative path",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn lane_prefixed_relative_path(lane: &Path, artifact: &Path) -> Option<PathBuf> {
    if lane.is_absolute() || artifact.is_absolute() {
        return None;
    }

    let relative = artifact.strip_prefix(lane).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

fn cwd_relative_lane_prefixed_path(lane: &Path, artifact: &Path) -> Result<Option<PathBuf>> {
    if artifact.is_absolute() {
        return Ok(None);
    }

    let absolute_lane = std::path::absolute(lane)
        .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
    let absolute_artifact = std::path::absolute(artifact)
        .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
    let Some(relative) = absolute_artifact
        .strip_prefix(&absolute_lane)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(relative.to_path_buf()))
}

fn lane_relative_output_path(lane: &Path, output: &Path) -> Result<(PathBuf, String)> {
    let artifact_path = lane_relative_artifact_path(lane, output)?;
    let write_path = if output.is_absolute() {
        output.to_path_buf()
    } else {
        lane.join(Path::new(&artifact_path))
    };
    Ok((write_path, artifact_path))
}

async fn promote_manifest(
    json: bool,
    lane: &Path,
    stage: MozaBundleStage,
    json_out: Option<&Path>,
) -> Result<()> {
    let manifest_path = lane.join("manifest.json");
    let mut manifest = read_json_path(&manifest_path)?;
    let previous_completion_state = json_string(&manifest, "completion_state")
        .unwrap_or("missing")
        .to_string();
    let previous_hardware_validated = json_bool(&manifest, "hardware_validated");
    let previous_simulator_validated = json_bool(&manifest, "simulator_validated");

    let verification_before = verify_bundle_dir(lane, stage);
    if !verification_before.success {
        return Err(anyhow!(
            "cannot promote manifest to {}: live bundle verification failed with {} missing artifact(s), {} invalid artifact(s), and {} failed gate(s)",
            stage_label(stage),
            verification_before.missing_artifacts,
            verification_before.invalid_artifacts,
            verification_before.failed_gates
        ));
    }

    let (completion_state, hardware_validated, simulator_validated) =
        manifest_promotion_values(stage);
    let verification_after = promote_manifest_with_post_verification(
        &manifest_path,
        &mut manifest,
        completion_state,
        hardware_validated,
        simulator_validated,
        || verify_bundle_dir(lane, stage),
    )?;

    let receipt = serde_json::json!({
        "success": true,
        "command": "wheelctl moza promote-manifest",
        "generated_at_utc": now_utc(),
        "lane": lane.display().to_string(),
        "manifest": manifest_path.display().to_string(),
        "stage": stage_label(stage),
        "previous_completion_state": previous_completion_state,
        "previous_hardware_validated": previous_hardware_validated,
        "previous_simulator_validated": previous_simulator_validated,
        "completion_state": completion_state,
        "hardware_validated": hardware_validated,
        "simulator_validated": simulator_validated,
        "high_torque_validated": false,
        "release_ready": false,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "verification_before": bundle_verification_summary_value(&verification_before),
        "verification_after": bundle_verification_summary_value(&verification_after),
        "notes": [
            "promote-manifest runs live bundle verification before changing manifest claims",
            "release_ready and high_torque_validated remain false"
        ]
    });
    write_json_receipt(json_out, &receipt)?;
    print_manifest_promotion_receipt(json, json_out, &receipt)
}

fn promote_manifest_with_post_verification(
    manifest_path: &Path,
    manifest: &mut Value,
    completion_state: &str,
    hardware_validated: bool,
    simulator_validated: bool,
    verify_after: impl FnOnce() -> BundleVerificationReceipt,
) -> Result<BundleVerificationReceipt> {
    let previous_manifest = manifest.clone();
    apply_manifest_promotion(
        manifest,
        completion_state,
        hardware_validated,
        simulator_validated,
    )?;
    write_json_file(manifest_path, manifest)?;

    let verification_after = verify_after();
    if verification_after.success {
        return Ok(verification_after);
    }

    write_json_file(manifest_path, &previous_manifest).with_context(|| {
        format!(
            "post-promotion verification failed and rollback failed for '{}'",
            manifest_path.display()
        )
    })?;
    *manifest = previous_manifest;
    Err(anyhow!(
        "manifest promotion wrote '{}', but post-promotion verification failed with {} missing artifact(s), {} invalid artifact(s), and {} failed gate(s); previous manifest restored",
        manifest_path.display(),
        verification_after.missing_artifacts,
        verification_after.invalid_artifacts,
        verification_after.failed_gates
    ))
}

fn enumerate_moza_devices(
    api: &HidApi,
    include_descriptor: bool,
    include_descriptor_hex: bool,
) -> Vec<MozaDeviceRecord> {
    let mut devices: Vec<_> = api
        .device_list()
        .filter(|device| device.vendor_id() == MOZA_VENDOR_ID)
        .map(|device| {
            MozaDeviceRecord::from_device_info(device, include_descriptor, include_descriptor_hex)
        })
        .collect();

    devices.sort_by_key(|device| {
        (
            device.product_id.clone(),
            device.interface_number.unwrap_or(-1),
            device.usage_page.clone().unwrap_or_default(),
            device.usage.clone().unwrap_or_default(),
            device.path.clone(),
        )
    });
    devices
}

fn open_single_moza_device(
    api: &HidApi,
    selector: Option<&str>,
) -> Result<(hidapi::HidDevice, MozaDeviceRecord)> {
    let mut selected: Option<(hidapi::HidDevice, MozaDeviceRecord)> = None;
    let mut match_count = 0usize;

    for info in api.device_list() {
        if info.vendor_id() != MOZA_VENDOR_ID {
            continue;
        }

        let snapshot = MozaDeviceRecord::from_device_info(info, false, false);
        if !selector_matches(&snapshot, selector) {
            continue;
        }

        match_count += 1;
        if match_count > 1 {
            return Err(anyhow!(
                "multiple Moza HID devices matched; pass --device with a HID path, PID, or VID:PID"
            ));
        }

        let device = info.open_device(api).with_context(|| {
            format!(
                "failed to open Moza HID device {} ({})",
                snapshot.product_id, snapshot.path
            )
        })?;
        selected = Some((device, snapshot));
    }

    selected.ok_or_else(|| match selector {
        Some(value) => anyhow!("no Moza HID device matched selector '{value}'"),
        None => anyhow!("no Moza HID devices found"),
    })
}

fn validate_zero_torque_args(repeat: u32, hz: u32, watchdog_timeout_ms: u64) -> Result<()> {
    if repeat == 0 || repeat > 10_000 {
        return Err(anyhow!("--repeat must be in 1..=10000"));
    }
    if hz == 0 || hz > 1000 {
        return Err(anyhow!("--hz must be in 1..=1000"));
    }
    if watchdog_timeout_ms == 0 {
        return Err(anyhow!("--watchdog-timeout-ms must be greater than zero"));
    }
    Ok(())
}

fn validate_watchdog_proof_args(
    pre_zero_count: u32,
    hz: u32,
    watchdog_timeout_ms: u64,
) -> Result<()> {
    if pre_zero_count == 0 || pre_zero_count > 1_000 {
        return Err(anyhow!("--pre-zero-count must be in 1..=1000"));
    }
    if hz == 0 || hz > 1000 {
        return Err(anyhow!("--hz must be in 1..=1000"));
    }
    if watchdog_timeout_ms == 0 || watchdog_timeout_ms > 5_000 {
        return Err(anyhow!("--watchdog-timeout-ms must be in 1..=5000"));
    }
    Ok(())
}

fn validate_disconnect_proof_args(max_duration_ms: u64, hz: u32) -> Result<()> {
    if max_duration_ms == 0 || max_duration_ms > 60_000 {
        return Err(anyhow!("--max-duration-ms must be in 1..=60000"));
    }
    if hz == 0 || hz > 1000 {
        return Err(anyhow!("--hz must be in 1..=1000"));
    }
    Ok(())
}

fn validate_torque_test_args(max_percent: f32, duration_ms: u64, hz: u32) -> Result<()> {
    if !max_percent.is_finite() || !(0.1..=2.0).contains(&max_percent) {
        return Err(anyhow!("--max-percent must be in 0.1..=2.0"));
    }
    if duration_ms == 0 || duration_ms > 1_000 {
        return Err(anyhow!("--duration-ms must be in 1..=1000"));
    }
    if hz == 0 || hz > 1000 {
        return Err(anyhow!("--hz must be in 1..=1000"));
    }
    Ok(())
}

fn disconnect_max_writes(max_duration_ms: u64, hz: u32) -> u32 {
    max_duration_ms
        .saturating_mul(u64::from(hz))
        .div_ceil(1000)
        .max(1)
        .min(u64::from(u32::MAX)) as u32
}

fn init_mode_to_ffb_mode(mode: MozaInitMode) -> FfbMode {
    match mode {
        MozaInitMode::Off => FfbMode::Off,
        MozaInitMode::Standard => FfbMode::Standard,
    }
}

fn init_mode_label(mode: MozaInitMode) -> &'static str {
    match mode {
        MozaInitMode::Off => "off",
        MozaInitMode::Standard => "standard",
    }
}

fn ffb_mode_wire_hex(mode: MozaInitMode) -> String {
    hex_u8(init_mode_to_ffb_mode(mode) as u8)
}

fn init_state_label(state: MozaInitState) -> &'static str {
    match state {
        MozaInitState::Uninitialized => "uninitialized",
        MozaInitState::Initializing => "initializing",
        MozaInitState::Ready => "ready",
        MozaInitState::Failed => "failed",
        MozaInitState::PermanentFailure => "permanent_failure",
    }
}

fn feature_report_kind(report_id: u8) -> &'static str {
    match report_id {
        0x02 => "high_torque",
        0x03 => "start_input_reports",
        0x11 => "ffb_mode",
        _ => "unknown",
    }
}

fn zero_torque_dry_run_pid(selector: Option<&str>, pid_override: Option<&str>) -> Result<u16> {
    if let Some(pid) = pid_override {
        return parse_required_hex_u16(pid);
    }
    if let Some(selector) = selector {
        if let Some((_vid, pid)) = parse_vid_pid_selector(selector) {
            return Ok(pid);
        }
        if let Some(pid) = parse_hex_selector(selector) {
            return Ok(pid);
        }
    }
    Err(anyhow!(
        "--dry-run requires --pid or --device as a PID/VID:PID selector"
    ))
}

fn zero_torque_payload_for_pid(pid: u16) -> [u8; REPORT_LEN] {
    let protocol = MozaProtocol::new_with_config(pid, FfbMode::Off, false);
    let encoder = MozaDirectTorqueEncoder::new(protocol.model().max_torque_nm());
    let mut payload = [0u8; REPORT_LEN];
    let _ = encoder.encode_zero(&mut payload);
    payload
}

fn low_torque_payload_for_pid_percent(pid: u16, percent: f32) -> ([u8; REPORT_LEN], f32) {
    let protocol = MozaProtocol::new_with_config(pid, FfbMode::Off, false);
    let max_torque_nm = protocol.model().max_torque_nm();
    let encoder = MozaDirectTorqueEncoder::new(max_torque_nm);
    let torque_nm = max_torque_nm * (percent / 100.0);
    let mut payload = [0u8; REPORT_LEN];
    let _ = encoder.encode(torque_nm, 0, &mut payload);
    (payload, torque_nm)
}

fn low_torque_ladder_for_pid(
    pid: u16,
    max_percent: f32,
    duration_ms: u64,
    hz: u32,
) -> Vec<LowTorqueStage> {
    let write_count = duration_ms
        .saturating_mul(u64::from(hz))
        .div_ceil(1000)
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    let mut stages = Vec::new();
    for target_percent in [0.1_f32, 0.5, 1.0, max_percent] {
        let percent = target_percent.min(max_percent);
        if stages
            .last()
            .map(|stage: &LowTorqueStage| (stage.percent - percent).abs() < f32::EPSILON)
            .unwrap_or(false)
        {
            continue;
        }
        let (payload, torque_nm) = low_torque_payload_for_pid_percent(pid, percent);
        stages.push(LowTorqueStage::new(
            percent,
            torque_nm,
            write_count,
            payload,
        ));
    }
    stages
}

struct LowTorqueRealHardwarePreflight {
    zero_proof: ZeroProofSummary,
    init_proofs: InitProofSet,
    direct_mode_gate: DirectModeGateSummary,
    target_product_id: String,
}

#[allow(clippy::too_many_arguments)]
fn validate_low_torque_real_hardware_preflight(
    selector: Option<&str>,
    pid_override: Option<&str>,
    lane: Option<&Path>,
    zero_proof: Option<&Path>,
    init_off: Option<&Path>,
    init_standard: Option<&Path>,
    descriptor: Option<&Path>,
    explicit_operator_override: bool,
) -> Result<LowTorqueRealHardwarePreflight> {
    let lane = lane.ok_or_else(|| {
        anyhow!("--lane is required before actual low-torque writes so prerequisites are resolved from the dated hardware lane")
    })?;
    let zero_proof_path = zero_proof
        .ok_or_else(|| anyhow!("--zero-proof is required before actual low-torque writes"))?;
    require_lane_artifact_path(
        lane,
        zero_proof_path,
        "zero-torque-proof.json",
        "--zero-proof",
    )?;
    if let Some(path) = init_off {
        require_lane_artifact_path(lane, path, "init-off.json", "--init-off")?;
    }
    if let Some(path) = init_standard {
        require_lane_artifact_path(lane, path, "init-standard.json", "--init-standard")?;
    }
    if let Some(path) = descriptor {
        require_lane_artifact_path(lane, path, "descriptor.json", "--descriptor")?;
    }

    let preflight_at_utc = now_utc();
    let zero_receipt = read_json_value(lane, "zero-torque-proof.json")?;
    if !receipt_path_matches(lane, &zero_receipt, "zero-torque-proof.json") {
        return Err(anyhow!(
            "zero proof receipt_path must match lane artifact zero-torque-proof.json"
        ));
    }
    let zero_proof = validate_zero_proof_for_torque_test(zero_proof_path)?;
    if !utc_timestamp_pair_is_ordered(&zero_proof.generated_at_utc, &preflight_at_utc) {
        return Err(anyhow!(
            "zero proof generated_at_utc must be before the low-torque preflight timestamp"
        ));
    }

    let init_proofs = validate_init_proofs_for_torque_test(
        Some(lane),
        init_off,
        init_standard,
        false,
    )?
    .ok_or_else(|| {
        anyhow!("--lane or both --init-off and --init-standard are required before actual low-torque writes")
    })?;
    require_lane_receipt_path(lane, "init-off.json")?;
    require_lane_receipt_path(lane, "init-standard.json")?;
    if !utc_timestamp_pair_is_ordered(&init_proofs.off.generated_at_utc, &preflight_at_utc)
        || !utc_timestamp_pair_is_ordered(&init_proofs.standard.generated_at_utc, &preflight_at_utc)
    {
        return Err(anyhow!(
            "init proof generated_at_utc values must be before the low-torque preflight timestamp"
        ));
    }

    let target_product_id = low_torque_preflight_target_product_id(
        selector,
        pid_override,
        lane,
        &zero_proof,
        &init_proofs,
    )?;
    if zero_proof.product_id.as_deref() != Some(target_product_id.as_str()) {
        return Err(anyhow!(
            "zero proof PID {:?} does not match low-torque preflight target PID {}",
            zero_proof.product_id,
            target_product_id
        ));
    }
    if !init_proofs.match_product_id(&target_product_id) {
        return Err(anyhow!(
            "init proof PID(s) {:?}/{:?} do not match low-torque preflight target PID {}",
            init_proofs.off.product_id,
            init_proofs.standard.product_id,
            target_product_id
        ));
    }

    if !explicit_operator_override {
        let descriptor_path = descriptor.ok_or_else(|| {
            anyhow!("actual low-torque direct report writes require --descriptor with a trusted R5 descriptor receipt or --explicit-operator-override")
        })?;
        let descriptor_receipt = read_json_path(descriptor_path)?;
        let generated_at_utc = json_string(&descriptor_receipt, "generated_at_utc")
            .ok_or_else(|| anyhow!("descriptor receipt is missing generated_at_utc"))?;
        if !utc_timestamp_pair_is_ordered(generated_at_utc, &preflight_at_utc) {
            return Err(anyhow!(
                "descriptor generated_at_utc must be before the low-torque preflight timestamp"
            ));
        }
        let _descriptor_crc32 = receipt_file_crc32(descriptor_path)?;
    }
    let direct_mode_gate = validate_direct_mode_gate_for_torque_test(
        descriptor,
        &target_product_id,
        explicit_operator_override,
    )?;

    Ok(LowTorqueRealHardwarePreflight {
        zero_proof,
        init_proofs,
        direct_mode_gate,
        target_product_id,
    })
}

fn require_lane_artifact_path(
    lane: &Path,
    path: &Path,
    relative_path: &str,
    label: &str,
) -> Result<()> {
    if path_value_matches(&lane.join(relative_path), Some(&path.display().to_string())) {
        Ok(())
    } else {
        Err(anyhow!(
            "{label} '{}' must resolve to same-lane artifact '{}'",
            path.display(),
            lane.join(relative_path).display()
        ))
    }
}

fn require_lane_receipt_path(lane: &Path, relative_path: &str) -> Result<()> {
    let receipt = read_json_value(lane, relative_path)?;
    if receipt_path_matches(lane, &receipt, relative_path) {
        Ok(())
    } else {
        Err(anyhow!(
            "{} receipt_path must match the same-lane artifact",
            relative_path
        ))
    }
}

fn low_torque_preflight_target_product_id(
    selector: Option<&str>,
    pid_override: Option<&str>,
    lane: &Path,
    zero_proof: &ZeroProofSummary,
    init_proofs: &InitProofSet,
) -> Result<String> {
    let pid = pid_override
        .and_then(parse_hex_selector)
        .or_else(|| selector.and_then(|selector| parse_vid_pid_selector(selector).map(|(_, pid)| pid)))
        .or_else(|| selector.and_then(parse_hex_selector))
        .or_else(|| lane_manifest_r5_pid(lane))
        .or_else(|| zero_proof.product_id.as_deref().and_then(parse_hex_selector))
        .ok_or_else(|| {
            anyhow!("low-torque preflight could not determine target R5 PID from --pid, --device, lane manifest, or zero proof")
        })?;
    if !matches!(pid, product_ids::R5_V1 | product_ids::R5_V2) {
        return Err(anyhow!(
            "low-torque preflight target PID {} is not a Moza R5 wheelbase",
            hex_u16(pid)
        ));
    }
    let target = hex_u16(pid);
    if init_proofs.off.product_id.as_deref() != Some(target.as_str())
        || init_proofs.standard.product_id.as_deref() != Some(target.as_str())
    {
        return Err(anyhow!(
            "init proof PID(s) {:?}/{:?} do not match low-torque preflight target PID {}",
            init_proofs.off.product_id,
            init_proofs.standard.product_id,
            target
        ));
    }
    Ok(target)
}

fn validate_zero_proof_for_torque_test(path: &Path) -> Result<ZeroProofSummary> {
    let receipt = read_json_path(path)?;
    let receipt_crc32 = receipt_file_crc32(path)?;
    let generated_at_utc = json_string(&receipt, "generated_at_utc").map(str::to_string);
    let repeat = json_u64(&receipt, "repeat").unwrap_or(0);
    let dry_run = json_bool(&receipt, "dry_run");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let hz = json_u64(&receipt, "hz").unwrap_or(0);
    let write_attempts = json_u64(&receipt, "write_attempts").unwrap_or(0);
    let writes_ok = json_u64(&receipt, "writes_ok").unwrap_or(0);
    let write_errors = json_u64(&receipt, "write_errors").unwrap_or(u64::MAX);
    let watchdog_faults = json_u64(&receipt, "watchdog_faults").unwrap_or(u64::MAX);
    let writes_ok_exact = repeat.checked_add(1) == Some(writes_ok);
    let device = receipt.get("device");
    let product_id = device
        .and_then(|device| json_string(device, "product_id"))
        .map(str::to_string);
    let r5_device = device.map(is_r5_device_value).unwrap_or(false);
    let output_capable =
        device.and_then(|device| json_bool(device, "output_capable")) == Some(true);
    let safe = json_bool(&receipt, "success") == Some(true)
        && dry_run == Some(false)
        && json_bool(&receipt, "no_high_torque") == Some(true)
        && json_bool(&receipt, "no_nonzero_torque") == Some(true)
        && json_bool(&receipt, "no_feature_reports") == Some(true)
        && no_out_of_scope
        && json_bool(&receipt, "no_hid_device_opened") == Some(false)
        && json_string(&receipt, "report_id") == Some(DIRECT_TORQUE_REPORT_ID)
        && json_i64(&receipt, "torque_raw") == Some(0)
        && json_u64(&receipt, "flags") == Some(0)
        && json_bool(&receipt, "motor_enabled") == Some(false)
        && json_bool(&receipt, "final_zero_sent") == Some(true)
        && repeat >= 100
        && hz > 0
        && hz <= 1000
        && write_attempts == repeat
        && writes_ok_exact
        && write_errors == 0
        && watchdog_faults == 0
        && zero_command_log_is_safe(&receipt, repeat)
        && generated_at_utc
            .as_deref()
            .map(|value| utc_timestamp_pair_is_ordered(value, value))
            .unwrap_or(false)
        && product_id.is_some()
        && r5_device
        && output_capable;
    if !safe {
        return Err(anyhow!(
            "zero proof '{}' is not a passing real zero-torque receipt",
            path.display()
        ));
    }

    Ok(ZeroProofSummary {
        path: path.display().to_string(),
        generated_at_utc: generated_at_utc.ok_or_else(|| {
            anyhow!(
                "zero proof '{}' is missing generated_at_utc",
                path.display()
            )
        })?,
        receipt_crc32,
        product_id,
        repeat,
        writes_ok,
        final_zero_sent: true,
    })
}

fn validate_init_proofs_for_torque_test(
    lane: Option<&Path>,
    init_off: Option<&Path>,
    init_standard: Option<&Path>,
    dry_run: bool,
) -> Result<Option<InitProofSet>> {
    let off_path = init_off
        .map(Path::to_path_buf)
        .or_else(|| lane.map(|path| path.join("init-off.json")));
    let standard_path = init_standard
        .map(Path::to_path_buf)
        .or_else(|| lane.map(|path| path.join("init-standard.json")));

    match (off_path, standard_path) {
        (Some(off_path), Some(standard_path)) => Ok(Some(InitProofSet {
            off: validate_init_proof_for_torque_test(&off_path, "off")?,
            standard: validate_init_proof_for_torque_test(&standard_path, "standard")?,
        })),
        (None, None) if dry_run => Ok(None),
        _ => Err(anyhow!(
            "--lane or both --init-off and --init-standard are required before actual low-torque writes"
        )),
    }
}

fn validate_init_proof_for_torque_test(
    path: &Path,
    expected_mode: &str,
) -> Result<InitProofSummary> {
    let receipt = read_json_path(path)?;
    let receipt_crc32 = receipt_file_crc32(path)?;
    let generated_at_utc = json_string(&receipt, "generated_at_utc").map(str::to_string);
    let device = receipt.get("device");
    let product_id = device
        .and_then(|device| json_string(device, "product_id"))
        .map(str::to_string);
    let feature_report_count = receipt
        .get("feature_reports")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let init_state = json_string(&receipt, "init_state").unwrap_or("unknown");
    let ready = json_bool(&receipt, "ready") == Some(true);
    let safe = json_bool(&receipt, "success") == Some(true)
        && json_string(&receipt, "command") == Some("wheelctl moza init")
        && json_bool(&receipt, "dry_run") == Some(false)
        && json_bool(&receipt, "no_hid_device_opened") == Some(false)
        && json_bool(&receipt, "no_output_reports") == Some(true)
        && json_bool(&receipt, "no_direct_torque_reports") == Some(true)
        && no_out_of_scope_device_commands(&receipt)
        && json_bool(&receipt, "no_high_torque") == Some(true)
        && json_bool(&receipt, "high_torque") == Some(false)
        && json_string(&receipt, "mode") == Some(expected_mode)
        && init_state == "ready"
        && ready
        && json_u64(&receipt, "feature_write_errors").unwrap_or(u64::MAX) == 0
        && json_u64(&receipt, "output_report_attempts").unwrap_or(u64::MAX) == 0
        && receipt
            .get("feature_reports")
            .map(|reports| init_feature_reports_are_safe_value(reports, expected_mode, false))
            .unwrap_or(false)
        && generated_at_utc
            .as_deref()
            .map(|value| utc_timestamp_pair_is_ordered(value, value))
            .unwrap_or(false)
        && device.map(is_r5_device_value).unwrap_or(false)
        && device.and_then(|device| json_bool(device, "output_capable")) == Some(true)
        && product_id.is_some();

    if !safe {
        return Err(anyhow!(
            "init proof '{}' is not a passing real {}-mode handshake receipt",
            path.display(),
            expected_mode
        ));
    }

    Ok(InitProofSummary {
        path: path.display().to_string(),
        generated_at_utc: generated_at_utc.ok_or_else(|| {
            anyhow!(
                "init proof '{}' is missing generated_at_utc",
                path.display()
            )
        })?,
        receipt_crc32,
        product_id,
        mode: expected_mode.to_string(),
        init_state: init_state.to_string(),
        ready,
        feature_report_count,
    })
}

fn validate_direct_mode_gate_for_torque_test(
    descriptor: Option<&Path>,
    selected_product_id: &str,
    explicit_operator_override: bool,
) -> Result<DirectModeGateSummary> {
    if explicit_operator_override {
        return Ok(DirectModeGateSummary::operator_override(
            descriptor.map(Path::to_path_buf),
            selected_product_id,
        ));
    }

    let descriptor_path = descriptor.ok_or_else(|| {
        anyhow!(
            "direct torque report writes require --descriptor with trusted R5 metadata or --explicit-operator-override"
        )
    })?;
    let receipt = read_json_path(descriptor_path)?;
    let Some(devices) = receipt.get("devices").and_then(Value::as_array) else {
        return Err(anyhow!(
            "descriptor receipt '{}' is missing devices[]",
            descriptor_path.display()
        ));
    };

    let trusted = devices.iter().any(|device| {
        is_r5_device_value(device)
            && json_string(device, "product_id") == Some(selected_product_id)
            && r5_descriptor_trusted_for_direct_mode(device)
    });
    if !trusted {
        return Err(anyhow!(
            "descriptor receipt '{}' does not contain descriptor-derived trusted R5 report metadata for PID {}",
            descriptor_path.display(),
            selected_product_id
        ));
    }

    Ok(DirectModeGateSummary::trusted_descriptor(
        descriptor_path,
        selected_product_id,
    ))
}

fn synthetic_moza_device_record(pid: u16) -> MozaDeviceRecord {
    let identity = identify_device(pid);
    let output_capable = is_wheelbase_product(pid);
    MozaDeviceRecord {
        vendor_id: hex_u16(MOZA_VENDOR_ID),
        product_id: hex_u16(pid),
        product_name: identity.name.to_string(),
        product_category: category_label(identity.category).to_string(),
        topology_hint: topology_label(identity.topology_hint).to_string(),
        output_capable,
        r5_wheelbase_pid: matches!(pid, product_ids::R5_V1 | product_ids::R5_V2),
        manufacturer: None,
        product_string: None,
        serial_number_present: false,
        interface_number: None,
        usage_page: None,
        usage: None,
        path: "dry-run".to_string(),
        descriptor_source: "dry_run".to_string(),
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        report_descriptor_hex: None,
        report_metadata_source: "protocol_expected".to_string(),
        input_report_lengths: expected_input_report_lengths(pid),
        output_report_ids: expected_output_report_ids(output_capable),
        output_reports: expected_output_reports(output_capable),
        feature_report_ids: expected_feature_report_ids(output_capable),
    }
}

fn moza_status_receipt(
    devices: Vec<MozaDeviceRecord>,
    selector: Option<&str>,
    lane: Option<&Path>,
) -> Value {
    let lane_status = lane.map(support_bundle_status);
    let device_statuses: Vec<_> = devices
        .into_iter()
        .map(|device| {
            let descriptor_trusted = lane
                .map(|path| lane_descriptor_trusted_for_pid(path, &device.product_id))
                .unwrap_or(false);
            let init_state = if device.output_capable {
                "uninitialized"
            } else {
                "not_applicable"
            };
            serde_json::json!({
                "device": device,
                "init_state": init_state,
                "ffb_ready": false,
                "descriptor_trusted": descriptor_trusted,
                "direct_mode_allowed": false,
                "high_torque_allowed": false,
                "safe_to_send_torque": false,
                "safety_reason": "status is observe-only; run explicit init, zero, and torque-test gates before any output"
            })
        })
        .collect();

    serde_json::json!({
        "success": true,
        "command": "wheelctl moza status",
        "generated_at_utc": now_utc(),
        "selector": selector,
        "device_count": device_statuses.len(),
        "devices": device_statuses,
        "lane": lane.map(|path| path.display().to_string()),
        "lane_status": lane_status,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "notes": [
            "status enumerates Moza HID identity and optional lane receipts only; it sends no feature reports or FFB output",
            "ffb_ready remains false until an explicit staged init receipt proves readiness"
        ]
    })
}

#[derive(Clone, Copy)]
enum SupportBundleValidationMode {
    Fresh,
    ShapeOnly,
}

fn verify_bundle_dir(lane: &Path, stage: MozaBundleStage) -> BundleVerificationReceipt {
    verify_bundle_dir_with_support_validation(lane, stage, SupportBundleValidationMode::Fresh)
}

fn verify_bundle_dir_with_support_validation(
    lane: &Path,
    stage: MozaBundleStage,
    support_validation: SupportBundleValidationMode,
) -> BundleVerificationReceipt {
    let artifact_requirements = bundle_artifact_requirements_for_lane(lane);
    let artifact_checks: Vec<_> = artifact_requirements
        .iter()
        .filter(|requirement| stage_rank(requirement.stage) <= stage_rank(stage))
        .map(|requirement| check_bundle_artifact(lane, requirement))
        .collect();

    let mut gates = Vec::new();
    gates.push(if lane.is_dir() {
        BundleGateCheck::pass("lane_directory", format!("found {}", lane.display()))
    } else {
        BundleGateCheck::fail(
            "lane_directory",
            format!("missing lane directory {}", lane.display()),
        )
    });
    gates.push(verify_manifest_gate(lane, stage));
    gates.push(
        verify_manifest_r5_pid_consistency_gate_with_support_validation(
            lane,
            stage,
            support_validation,
        ),
    );
    gates.push(verify_moza_r5_observed_gate(lane));
    gates.push(verify_moza_topology_observed_gate(lane));
    gates.push(verify_descriptor_metadata_gate(lane));
    gates.push(verify_passive_receipts_success_gate(lane));
    gates.push(verify_passive_no_writes_gate(lane));
    gates.push(verify_passive_capture_parse_gate(lane));
    gates.push(verify_parser_validation_gate(lane));
    gates.push(verify_fixture_promotion_gate(lane));

    if stage_rank(stage) >= stage_rank(MozaBundleStage::Zero) {
        gates.push(verify_zero_torque_gate(lane));
        gates.push(verify_watchdog_proof_gate(lane));
        gates.push(verify_disconnect_proof_gate(lane));
    }

    if stage_rank(stage) >= stage_rank(MozaBundleStage::SmokeReady) {
        gates.push(verify_init_receipt_gate(
            lane,
            "init_off_handshake",
            "init-off.json",
            "off",
        ));
        gates.push(verify_init_receipt_gate(
            lane,
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        ));
        gates.push(verify_service_status_gate_with_support_validation(
            lane,
            support_validation,
        ));
        gates.push(verify_low_torque_gate(lane));
        gates.push(verify_pit_house_coexistence_gate_with_support_validation(
            lane,
            support_validation,
        ));
        gates.push(verify_simulator_telemetry_gate(lane));
        gates.push(verify_simulator_ffb_gate(lane));
    }

    let missing_artifacts = artifact_checks
        .iter()
        .filter(|check| check.status == "missing")
        .count();
    let invalid_artifacts = artifact_checks
        .iter()
        .filter(|check| check.status == "invalid")
        .count();
    let failed_gates = gates.iter().filter(|check| check.status == "fail").count();
    let success = missing_artifacts == 0 && invalid_artifacts == 0 && failed_gates == 0;
    let endpoint_observations = bundle_endpoint_observations(lane);
    let operator_actions = operator_actions_for_bundle_stage(lane, stage, &gates);
    let next_commands = next_commands_for_bundle_stage(lane, stage, &artifact_checks, &gates);

    BundleVerificationReceipt {
        success,
        command: "wheelctl moza verify-bundle",
        generated_at_utc: now_utc(),
        lane: lane.display().to_string(),
        requested_stage: stage_label(stage).to_string(),
        missing_artifacts,
        invalid_artifacts,
        failed_gates,
        artifacts: artifact_checks,
        gates,
        endpoint_observations,
        operator_actions,
        next_commands,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        notes: vec![
            "verify-bundle reads existing receipts only; it opens no HID device and sends no reports"
                .to_string(),
            "a passing bundle is evidence for the requested lane stage only, not release readiness"
                .to_string(),
        ],
    }
}

fn bundle_endpoint_observations(lane: &Path) -> Vec<BundleEndpointObservation> {
    let Ok(manifest) = read_json_value(lane, "manifest.json") else {
        return Vec::new();
    };
    let Some(topology) = manifest.get("topology") else {
        return Vec::new();
    };
    let Some(endpoints) = topology.get("endpoints").and_then(Value::as_array) else {
        return Vec::new();
    };
    let controls = topology
        .get("logical_controls")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    endpoints
        .iter()
        .map(|endpoint| {
            let id = json_string(endpoint, "id").unwrap_or("<missing-id>");
            let required_logical_controls = endpoint_logical_controls(&controls, id, true);
            let optional_logical_controls = endpoint_logical_controls(&controls, id, false);
            let artifacts = [
                "device-list.json",
                "moza-probe.json",
                "hid-list.json",
                "descriptor.json",
            ]
            .into_iter()
            .map(|path| endpoint_artifact_observation(lane, path, endpoint))
            .collect::<Vec<_>>();
            let observed_artifact_count = artifacts
                .iter()
                .filter(|artifact| artifact.vid_pid_count > 0)
                .count();
            let metadata_match_artifact_count = artifacts
                .iter()
                .filter(|artifact| artifact.metadata_match_count > 0)
                .count();

            BundleEndpointObservation {
                id: id.to_string(),
                kind: json_string(endpoint, "kind").map(str::to_string),
                vendor_id: json_string(endpoint, "vendor_id").map(str::to_string),
                product_id: json_string(endpoint, "product_id").map(str::to_string),
                interface_number: json_u64(endpoint, "interface_number"),
                usage_page: json_string(endpoint, "usage_page").map(str::to_string),
                usage: json_string(endpoint, "usage").map(str::to_string),
                output_capable: json_bool(endpoint, "output_capable"),
                required_logical_controls,
                optional_logical_controls,
                observed_artifact_count,
                metadata_match_artifact_count,
                artifacts,
            }
        })
        .collect()
}

fn endpoint_logical_controls(
    controls: &serde_json::Map<String, Value>,
    endpoint_id: &str,
    required: bool,
) -> Vec<String> {
    let mut names = controls
        .iter()
        .filter(|(_, control)| json_string(control, "source_endpoint") == Some(endpoint_id))
        .filter(|(_, control)| json_bool(control, "required").unwrap_or(true) == required)
        .map(|(name, _)| name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn endpoint_artifact_observation(
    lane: &Path,
    path: &str,
    endpoint: &Value,
) -> BundleEndpointArtifactObservation {
    match read_json_value(lane, path) {
        Ok(receipt) => {
            let vid_pid_count = count_topology_endpoint_vid_pid_devices(&receipt, endpoint);
            let metadata_match_count = count_topology_endpoint_metadata_devices(&receipt, endpoint);
            BundleEndpointArtifactObservation {
                path: path.to_string(),
                status: "read".to_string(),
                vid_pid_count,
                metadata_match_count,
            }
        }
        Err(e) => BundleEndpointArtifactObservation {
            path: path.to_string(),
            status: format!("unavailable:{e}"),
            vid_pid_count: 0,
            metadata_match_count: 0,
        },
    }
}

fn count_topology_endpoint_vid_pid_devices(receipt: &Value, endpoint: &Value) -> usize {
    let Some(vendor_id) = json_string(endpoint, "vendor_id").and_then(parse_hex_selector) else {
        return 0;
    };
    let Some(product_id) = json_string(endpoint, "product_id").and_then(parse_hex_selector) else {
        return 0;
    };
    count_vendor_product_devices(receipt, vendor_id, product_id)
}

fn count_topology_endpoint_metadata_devices(receipt: &Value, endpoint: &Value) -> usize {
    receipt
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| topology_endpoint_metadata_matches_device(device, endpoint))
                .count()
        })
        .unwrap_or(0)
}

fn topology_endpoint_metadata_matches_device(device: &Value, endpoint: &Value) -> bool {
    let Some(vendor_id) = json_string(endpoint, "vendor_id").and_then(parse_hex_selector) else {
        return false;
    };
    let Some(product_id) = json_string(endpoint, "product_id").and_then(parse_hex_selector) else {
        return false;
    };
    if !is_vendor_product_device_value(device, vendor_id, product_id) {
        return false;
    }
    if let Some(interface_number) = json_u64(endpoint, "interface_number")
        && json_u64(device, "interface_number") != Some(interface_number)
    {
        return false;
    }
    if let Some(usage_page) = json_string(endpoint, "usage_page")
        && json_string(device, "usage_page") != Some(usage_page)
    {
        return false;
    }
    if let Some(usage) = json_string(endpoint, "usage")
        && json_string(device, "usage") != Some(usage)
    {
        return false;
    }
    true
}

fn operator_actions_for_bundle_stage(
    lane: &Path,
    _stage: MozaBundleStage,
    gates: &[BundleGateCheck],
) -> Vec<String> {
    let mut actions = Vec::new();
    if !bundle_gate_check_passed(gates, "descriptor_metadata") {
        actions.push(
            "Export the R5 HID report descriptor byte block into target/moza-r5-report-descriptor.txt or target/moza-r5-report-descriptor.bin, then rerun the descriptor file fallback. A USBTreeView summary that only shows wDescriptorLength or ERROR_INVALID_PARAMETER is not enough; use the actual Report Descriptor hex block, Linux sysfs report_descriptor bytes, or an equivalent descriptor tool. Do not run firmware or DFU flows."
                .to_string(),
        );
    }

    let throttle_missing = gates.iter().any(|gate| {
        gate.name == "passive_captures_parse"
            && gate.status == "fail"
            && gate
                .details
                .contains("captures/r5-throttle-only-sweep.jsonl")
            && gate.details.contains("any_axes_ok=false")
    });
    if throttle_missing {
        let endpoint_context = if lane_has_single_observed_moza_hid_endpoint(lane) {
            " Stored observe-only HID/PnP receipts already show only the R5 HID game-controller endpoint, so do not chase another Moza HID path; the visible Moza serial/COM interface is diagnostic topology only and must not be probed or configured in the passive lane."
        } else {
            " If Pit House is unavailable, use observe-only HID/PnP inspection to confirm endpoints; a visible Moza serial/COM interface is diagnostic topology only and must not be probed or configured in the passive lane."
        };
        actions.push(format!(
            "Throttle capture parsed but no parser-visible hub-control axis moved; check throttle pedal cable, pedal-set-to-R5 routing, and vendor input state before replacing the lane capture.{endpoint_context}"
        ));
    }

    let parser_validation_failed = !bundle_gate_check_passed(gates, "parser_fixture_validation");
    let fixture_promotion_failed = !bundle_gate_check_passed(gates, "fixture_promotion");
    if parser_validation_failed && fixture_promotion_failed {
        actions.push(
            "Do not run fixture promotion until validate-captures passes for every manifest-required logical role."
                .to_string(),
        );
    }

    actions
}

fn audit_lane_dir(lane: &Path, stage: MozaBundleStage) -> LaneAuditReceipt {
    let live_verification = verify_bundle_dir(lane, stage);
    let mut receipt_checks = Vec::new();
    for audited_stage in audit_stages_through(stage) {
        receipt_checks.push(audit_stored_verification_receipt(lane, audited_stage));
        receipt_checks.push(audit_manifest_promotion_receipt(lane, audited_stage));
    }

    let missing_receipts = receipt_checks
        .iter()
        .filter(|check| check.status == "missing")
        .count();
    let invalid_receipts = receipt_checks
        .iter()
        .filter(|check| check.status == "invalid")
        .count();
    let success = live_verification.success && missing_receipts == 0 && invalid_receipts == 0;

    LaneAuditReceipt {
        success,
        command: "wheelctl moza audit-lane",
        generated_at_utc: now_utc(),
        lane: lane.display().to_string(),
        requested_stage: stage_label(stage).to_string(),
        live_verification_success: live_verification.success,
        live_verification: bundle_verification_summary_value(&live_verification),
        missing_receipts,
        invalid_receipts,
        receipt_checks,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        notes: vec![
            "audit-lane reads existing verification and promotion receipts only; it opens no HID device and sends no reports".to_string(),
            "audit-lane is a post-promotion completeness audit, not a release-readiness claim".to_string(),
        ],
    }
}

fn next_commands_for_bundle_stage(
    lane: &Path,
    stage: MozaBundleStage,
    artifact_checks: &[BundleArtifactCheck],
    gates: &[BundleGateCheck],
) -> Vec<String> {
    let has_blockers = artifact_checks.iter().any(|check| check.status != "pass")
        || gates.iter().any(|gate| gate.status == "fail");
    if !has_blockers {
        return Vec::new();
    }

    let lane = next_command_lane(lane);
    let wheelbase_pid = lane_manifest_r5_pid(&lane)
        .map(hex_u16)
        .unwrap_or_else(|| "0x0014".to_string());
    let mut commands = Vec::new();
    push_passive_next_commands(&lane, &wheelbase_pid, artifact_checks, gates, &mut commands);

    let passive_ready = passive_stage_gates_passed(gates);
    let zero_ready = zero_stage_gates_passed(gates);

    if stage_rank(stage) >= stage_rank(MozaBundleStage::Zero) && passive_ready && !zero_ready {
        push_zero_next_commands(&lane, gates, &mut commands);
    }

    if stage_rank(stage) >= stage_rank(MozaBundleStage::SmokeReady) && zero_ready {
        push_smoke_ready_next_commands(&lane, gates, &mut commands);
    }

    commands
}

fn next_command_lane(lane: &Path) -> PathBuf {
    if is_moza_lane_root(lane) {
        lane.join("YYYY-MM-DD")
    } else {
        lane.to_path_buf()
    }
}

fn is_moza_lane_root(lane: &Path) -> bool {
    lane.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("moza-r5"))
        && lane.join("README.md").is_file()
        && lane.join("manifest.schema.json").is_file()
        && !lane.join("manifest.json").is_file()
}

fn push_passive_next_commands(
    lane: &Path,
    wheelbase_pid: &str,
    artifact_checks: &[BundleArtifactCheck],
    gates: &[BundleGateCheck],
    commands: &mut Vec<String>,
) {
    let lane_arg = command_arg(&lane.display().to_string());
    let fixture_dir = Path::new("crates/hid-moza-protocol/fixtures").join(format!(
        "moza-r5-{}",
        lane.file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("YYYY-MM-DD")
    ));
    let fixture_dir_arg = command_arg(&fixture_dir.display().to_string());
    let r5_selector = format!("0x346E:{wheelbase_pid}");

    if bundle_artifact_needs_regeneration(artifact_checks, "manifest.json")
        || !bundle_gate_check_passed(gates, "manifest_no_overclaim")
    {
        commands.push(format!(
            "wheelctl moza init-lane --lane {lane_arg} --wheelbase-pid {wheelbase_pid} --operator Steven"
        ));
    }
    if bundle_artifacts_need_regeneration(
        artifact_checks,
        &["device-list.json", "moza-probe.json", "hid-list.json"],
    ) || !bundle_gate_check_passed(gates, "moza_r5_observed")
        || !bundle_gate_check_passed(gates, "moza_topology_observed")
    {
        commands.push(format!(
            "wheelctl device list --hid-observe-only --json-out {}",
            lane_path_arg(lane, "device-list.json")
        ));
        commands.push(format!(
            "wheelctl moza probe --json-out {}",
            lane_path_arg(lane, "moza-probe.json")
        ));
        commands.push(format!(
            "hid-capture list --vendor 0x346E --json-out {}",
            lane_path_arg(lane, "hid-list.json")
        ));
    }
    if bundle_artifact_needs_regeneration(artifact_checks, "hardware-doctor.json") {
        commands.push(format!(
            "wheelctl hardware doctor --json-out {}",
            lane_path_arg(lane, "hardware-doctor.json")
        ));
    }
    if bundle_artifact_needs_regeneration(artifact_checks, "descriptor.json")
        || !bundle_gate_check_passed(gates, "descriptor_metadata")
    {
        commands.push(format!(
            "wheelctl moza descriptor --json-out {}",
            lane_path_arg(lane, "descriptor.json")
        ));
        commands.push(format!(
            "wheelctl moza descriptor --device {r5_selector} --report-descriptor-hex-file target/moza-r5-report-descriptor.txt --json-out {}",
            lane_path_arg(lane, "descriptor.json")
        ));
        commands.push(format!(
            "wheelctl moza descriptor --device {r5_selector} --report-descriptor-bin-file target/moza-r5-report-descriptor.bin --json-out {}",
            lane_path_arg(lane, "descriptor.json")
        ));
    }

    for requirement in passive_capture_requirements_for_lane(lane) {
        if bundle_artifact_needs_regeneration(artifact_checks, requirement.relative_path) {
            let (device, duration_ms) =
                passive_capture_next_command_hint(requirement.relative_path);
            commands.push(format!(
                "wheelctl moza capture-input --device {device} --duration-ms {duration_ms} --json-out {}",
                lane_path_arg(lane, requirement.relative_path)
            ));
        }
    }

    if bundle_artifact_needs_regeneration(artifact_checks, "parser-fixture-validation.json")
        || !bundle_gate_check_passed(gates, "passive_captures_parse")
        || !bundle_gate_check_passed(gates, "parser_fixture_validation")
    {
        commands.push(format!(
            "wheelctl moza analyze-lane --lane {lane_arg} --json-out target/moza-lane-analysis.json"
        ));
        commands.push(format!(
            "wheelctl moza sync-role-status --lane {lane_arg} --json-out target/moza-role-status-sync.json"
        ));
        commands.push(format!(
            "wheelctl moza sync-role-status --lane {lane_arg} --check --json-out target/moza-role-status-check.json"
        ));
        commands.push(format!(
            "wheelctl moza validate-captures --lane {lane_arg} --json-out {}",
            lane_path_arg(lane, "parser-fixture-validation.json")
        ));
    }
    if bundle_gate_check_passed(gates, "passive_captures_parse")
        && bundle_gate_check_passed(gates, "parser_fixture_validation")
        && !bundle_gate_check_passed(gates, "fixture_promotion")
    {
        commands.push(format!(
            "wheelctl moza promote-fixtures --lane {lane_arg} --fixture-dir {fixture_dir_arg} --json-out {}",
            lane_path_arg(lane, "fixture-promotion.json")
        ));
        commands.push(
            "cargo test -p racing-wheel-hid-moza-protocol promoted_capture_fixtures_replay_through_moza_parser"
                .to_string(),
        );
    }
    commands.push(format!(
        "wheelctl moza verify-bundle --lane {lane_arg} --stage passive --json-out {}",
        lane_path_arg(lane, verification_receipt_path(MozaBundleStage::Passive))
    ));
    if bundle_gate_checks_passed(
        gates,
        &[
            "lane_directory",
            "manifest_no_overclaim",
            "manifest_r5_pid_consistency",
            "moza_r5_observed",
            "moza_topology_observed",
            "descriptor_metadata",
            "passive_receipts_no_ffb_writes",
            "passive_captures_parse",
            "parser_fixture_validation",
            "fixture_promotion",
        ],
    ) {
        commands.push(format!(
            "wheelctl moza promote-manifest --lane {lane_arg} --stage passive --json-out {}",
            lane_path_arg(lane, promotion_receipt_path(MozaBundleStage::Passive))
        ));
        commands.push(format!(
            "wheelctl moza audit-lane --lane {lane_arg} --stage passive --json-out {}",
            lane_path_arg(lane, audit_receipt_path(MozaBundleStage::Passive))
        ));
    }
}

fn bundle_gate_check_passed(gates: &[BundleGateCheck], name: &str) -> bool {
    gates
        .iter()
        .any(|gate| gate.name == name && gate.status == "pass")
}

fn bundle_gate_checks_passed(gates: &[BundleGateCheck], names: &[&str]) -> bool {
    names
        .iter()
        .all(|name| bundle_gate_check_passed(gates, name))
}

fn passive_stage_gates_passed(gates: &[BundleGateCheck]) -> bool {
    bundle_gate_checks_passed(
        gates,
        &[
            "lane_directory",
            "manifest_no_overclaim",
            "manifest_r5_pid_consistency",
            "moza_r5_observed",
            "moza_topology_observed",
            "descriptor_metadata",
            "passive_receipts_successful",
            "passive_receipts_no_ffb_writes",
            "passive_captures_parse",
            "parser_fixture_validation",
            "fixture_promotion",
        ],
    )
}

fn zero_stage_gates_passed(gates: &[BundleGateCheck]) -> bool {
    passive_stage_gates_passed(gates)
        && bundle_gate_checks_passed(
            gates,
            &[
                "zero_torque_real_hardware",
                "watchdog_zero_output",
                "disconnect_final_zero",
            ],
        )
}

fn smoke_ready_stage_gates_passed(gates: &[BundleGateCheck]) -> bool {
    zero_stage_gates_passed(gates)
        && bundle_gate_checks_passed(
            gates,
            &[
                "init_off_handshake",
                "init_standard_handshake",
                "service_status_receipts",
                "low_torque_bounded",
                "pit_house_coexistence",
                "simulator_telemetry",
                "simulator_ffb_bounded",
            ],
        )
}

fn bundle_artifact_needs_regeneration(
    artifact_checks: &[BundleArtifactCheck],
    relative_path: &str,
) -> bool {
    artifact_checks
        .iter()
        .find(|check| check.path == relative_path)
        .is_none_or(|check| check.status != "pass")
}

fn bundle_artifacts_need_regeneration(
    artifact_checks: &[BundleArtifactCheck],
    relative_paths: &[&str],
) -> bool {
    relative_paths
        .iter()
        .any(|path| bundle_artifact_needs_regeneration(artifact_checks, path))
}

fn passive_capture_next_command_hint(relative_path: &str) -> (&'static str, u64) {
    match relative_path {
        "captures/r5-idle.jsonl" | "captures/r5-aggregated-idle-after-controls.jsonl" => {
            ("<r5>", 5000)
        }
        "captures/r5-throttle-only-sweep.jsonl"
        | "captures/r5-brake-only-sweep.jsonl"
        | "captures/r5-clutch-only-sweep.jsonl"
        | "captures/r5-handbrake-only-sweep.jsonl" => ("<r5>", 15000),
        "captures/srp-standalone-sweep.jsonl" => ("<srp>", 10000),
        "captures/hbp-standalone-sweep.jsonl" => ("<hbp>", 10000),
        _ => ("<r5>", 10000),
    }
}

fn push_zero_next_commands(lane: &Path, gates: &[BundleGateCheck], commands: &mut Vec<String>) {
    let lane_arg = command_arg(&lane.display().to_string());
    if !bundle_gate_check_passed(gates, "zero_torque_real_hardware") {
        commands.push(format!(
            "wheelctl moza zero --device <r5> --repeat 100 --hz 1000 --json-out {}",
            lane_path_arg(lane, "zero-torque-proof.json")
        ));
    }
    if !bundle_gate_check_passed(gates, "watchdog_zero_output") {
        commands.push(format!(
            "wheelctl moza watchdog-proof --device <r5> --pre-zero-count 3 --watchdog-timeout-ms 100 --json-out {}",
            lane_path_arg(lane, "watchdog-proof.json")
        ));
    }
    if !bundle_gate_check_passed(gates, "disconnect_final_zero") {
        commands.push(format!(
            "wheelctl moza disconnect-proof --device <r5> --confirm-disconnect-test --max-duration-ms 10000 --json-out {}",
            lane_path_arg(lane, "disconnect-proof.json")
        ));
    }
    commands.push(format!(
        "wheelctl moza verify-bundle --lane {lane_arg} --stage zero --json-out {}",
        lane_path_arg(lane, verification_receipt_path(MozaBundleStage::Zero))
    ));
    if zero_stage_gates_passed(gates) {
        commands.push(format!(
            "wheelctl moza promote-manifest --lane {lane_arg} --stage zero --json-out {}",
            lane_path_arg(lane, promotion_receipt_path(MozaBundleStage::Zero))
        ));
        commands.push(format!(
            "wheelctl moza audit-lane --lane {lane_arg} --stage zero --json-out {}",
            lane_path_arg(lane, audit_receipt_path(MozaBundleStage::Zero))
        ));
    }
}

fn push_smoke_ready_next_commands(
    lane: &Path,
    gates: &[BundleGateCheck],
    commands: &mut Vec<String>,
) {
    let lane_arg = command_arg(&lane.display().to_string());
    commands.push(format!(
        "wheelctl moza init --device <r5> --mode off --json-out {}",
        lane_path_arg(lane, "init-off.json")
    ));
    commands.push(format!(
        "wheelctl moza init --device <r5> --mode standard --json-out {}",
        lane_path_arg(lane, "init-standard.json")
    ));
    commands.push(format!("wheeld --hardware-lane {lane_arg}"));
    commands.push(format!(
        "wheelctl moza status --device <r5> --lane {lane_arg} --json-out {}",
        lane_path_arg(lane, "moza-status.json")
    ));
    commands.push(format!(
        "wheelctl device status <r5> --moza-lane {lane_arg} --json-out {} --json",
        lane_path_arg(lane, "device-status.json")
    ));
    commands.push(format!(
        "wheelctl --json support-bundle --device <r5> --moza-lane {lane_arg} --output {}",
        lane_path_arg(lane, "support-bundle.json")
    ));
    commands.push(format!(
        "wheelctl moza torque-test --device <r5> --lane {lane_arg} --zero-proof {} --descriptor {} --max-percent 2 --duration-ms 250 --confirm-low-torque --json-out {}",
        lane_path_arg(lane, "zero-torque-proof.json"),
        lane_path_arg(lane, "descriptor.json"),
        lane_path_arg(lane, "low-torque-proof.json")
    ));

    for (case, evidence_artifact, artifact, evidence) in [
        (
            "closed",
            "pit-house-evidence-closed.json",
            "pit-house-observation-closed.json",
            "Pit House closed; OpenRacing staged handshake observed",
        ),
        (
            "open-standard",
            "pit-house-evidence-open-standard.json",
            "pit-house-observation-open-standard.json",
            "Pit House open and idle; standard mode observed",
        ),
        (
            "open-direct",
            "pit-house-evidence-open-direct.json",
            "pit-house-observation-open-direct.json",
            "Pit House open during direct request; OpenRacing warned or blocked",
        ),
        (
            "firmware-page",
            "pit-house-evidence-firmware-page.json",
            "pit-house-observation-firmware-page.json",
            "Pit House firmware/update page open; high-risk tests refused",
        ),
    ] {
        push_pit_house_observation_next_command(
            lane,
            commands,
            case,
            evidence_artifact,
            artifact,
            evidence,
        );
    }

    for (case, observation, artifact, evidence) in [
        (
            "closed",
            "pit-house-observation-closed.json",
            "pit-house-closed.json",
            "Pit House closed case observed",
        ),
        (
            "open-standard",
            "pit-house-observation-open-standard.json",
            "pit-house-open-standard.json",
            "Pit House open standard case observed",
        ),
        (
            "open-direct",
            "pit-house-observation-open-direct.json",
            "pit-house-direct-blocked.json",
            "Pit House open direct request warned or blocked",
        ),
        (
            "firmware-page",
            "pit-house-observation-firmware-page.json",
            "pit-house-firmware-page.json",
            "Pit House firmware/update page refusal observed",
        ),
    ] {
        push_pit_house_case_next_command(lane, commands, case, observation, artifact, evidence);
    }

    commands.push(format!(
        "wheelctl telemetry record --game <sim> --telemetry-source real_game --input <normalized-telemetry-source.jsonl> --out {} --duration-ms 30000",
        lane_path_arg(lane, "simulator-telemetry-recording.jsonl")
    ));
    commands.push(format!(
        "wheelctl moza simulator-telemetry-proof --lane {lane_arg} --game <sim> --telemetry-source real_game --recorder-artifact simulator-telemetry-recording.jsonl --duration-ms 30000 --json-out {}",
        lane_path_arg(lane, "simulator-telemetry-proof.json")
    ));
    commands.push(format!("wheeld --hardware-lane {lane_arg}"));
    commands.push(format!(
        "wheelctl moza simulator-ffb-smoke --lane {lane_arg} --game <sim> --telemetry-source real_game --output-log-artifact simulator-ffb-output.jsonl --descriptor-trusted --watchdog-timeout-ms 100 --stop-cleared-output --pause-cleared-output --game-exit-cleared-output --json-out {}",
        lane_path_arg(lane, "simulator-ffb-smoke.json")
    ));
    push_pit_house_observation_next_command(
        lane,
        commands,
        "mode-change",
        "pit-house-evidence-mode-change.json",
        "pit-house-observation-mode-change.json",
        "Pit House mode change observed during bounded run; output cleared",
    );
    push_pit_house_case_next_command(
        lane,
        commands,
        "mode-change",
        "pit-house-observation-mode-change.json",
        "pit-house-mode-change.json",
        "Pit House mode change fail-safe observed",
    );
    commands.push(format!(
        "wheelctl moza pit-house-proof --lane {lane_arg} --closed-artifact pit-house-closed.json --open-standard-artifact pit-house-open-standard.json --direct-artifact pit-house-direct-blocked.json --mode-change-artifact pit-house-mode-change.json --firmware-page-artifact pit-house-firmware-page.json --shared-control-risk warned --json-out {}",
        lane_path_arg(lane, "pit-house-coexistence.json")
    ));
    commands.push(format!(
        "wheelctl moza verify-bundle --lane {lane_arg} --stage smoke-ready --json-out {}",
        lane_path_arg(lane, verification_receipt_path(MozaBundleStage::SmokeReady))
    ));
    if smoke_ready_stage_gates_passed(gates) {
        commands.push(format!(
            "wheelctl moza promote-manifest --lane {lane_arg} --stage smoke-ready --json-out {}",
            lane_path_arg(lane, promotion_receipt_path(MozaBundleStage::SmokeReady))
        ));
        commands.push(format!(
            "wheelctl moza audit-lane --lane {lane_arg} --stage smoke-ready --json-out {}",
            lane_path_arg(lane, audit_receipt_path(MozaBundleStage::SmokeReady))
        ));
    }
}

fn push_pit_house_observation_next_command(
    lane: &Path,
    commands: &mut Vec<String>,
    case: &str,
    evidence_artifact: &str,
    output_artifact: &str,
    evidence: &str,
) {
    commands.push(format!(
        "wheelctl moza pit-house-observation --case {case} --evidence-kind process-window-snapshot --evidence-artifact {evidence_artifact} --operator Steven --evidence {} --json-out {}",
        command_arg(evidence),
        lane_path_arg(lane, output_artifact)
    ));
}

fn push_pit_house_case_next_command(
    lane: &Path,
    commands: &mut Vec<String>,
    case: &str,
    observation_artifact: &str,
    output_artifact: &str,
    evidence: &str,
) {
    let lane_arg = command_arg(&lane.display().to_string());
    commands.push(format!(
        "wheelctl moza pit-house-case --lane {lane_arg} --case {case} --observation-artifact {observation_artifact} --evidence {} --json-out {}",
        command_arg(evidence),
        lane_path_arg(lane, output_artifact)
    ));
}

fn lane_path_arg(lane: &Path, relative_path: &str) -> String {
    command_arg(&lane.join(relative_path).display().to_string())
}

fn command_arg(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '\'' | '"' | '&' | '(' | ')' | '[' | ']' | '{' | '}' | ';' | '<' | '>'
                )
        });
    if needs_quotes {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        value.to_string()
    }
}

fn audit_stages_through(stage: MozaBundleStage) -> Vec<MozaBundleStage> {
    match stage {
        MozaBundleStage::Passive => vec![MozaBundleStage::Passive],
        MozaBundleStage::Zero => vec![MozaBundleStage::Passive, MozaBundleStage::Zero],
        MozaBundleStage::SmokeReady => vec![
            MozaBundleStage::Passive,
            MozaBundleStage::Zero,
            MozaBundleStage::SmokeReady,
        ],
    }
}

fn audit_stored_verification_receipt(lane: &Path, stage: MozaBundleStage) -> LaneAuditCheck {
    let path = verification_receipt_path(stage);
    let stage_label = stage_label(stage);
    let receipt = match read_json_value(lane, path) {
        Ok(value) => value,
        Err(e) => {
            return LaneAuditCheck::missing(stage_label, "verification", path, e.to_string());
        }
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza verify-bundle");
    let lane_ok = path_value_matches(lane, json_string(&receipt, "lane"));
    let stage_ok = json_string(&receipt, "requested_stage") == Some(stage_label);
    let missing_artifacts = json_u64(&receipt, "missing_artifacts").unwrap_or(u64::MAX);
    let invalid_artifacts = json_u64(&receipt, "invalid_artifacts").unwrap_or(u64::MAX);
    let failed_gates = json_u64(&receipt, "failed_gates").unwrap_or(u64::MAX);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let safe = success
        && command_ok
        && lane_ok
        && stage_ok
        && missing_artifacts == 0
        && invalid_artifacts == 0
        && failed_gates == 0
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope;

    if safe {
        LaneAuditCheck::pass(
            stage_label,
            "verification",
            path,
            "stored verify-bundle receipt passed with no missing artifacts, invalid artifacts, failed gates, HID opens, FFB writes, serial config, firmware, or DFU commands".to_string(),
        )
    } else {
        LaneAuditCheck::invalid(
            stage_label,
            "verification",
            path,
            format!(
                "success={success}, command_ok={command_ok}, lane_ok={lane_ok}, stage_ok={stage_ok}, missing_artifacts={missing_artifacts}, invalid_artifacts={invalid_artifacts}, failed_gates={failed_gates}, no_hid_device_opened={no_hid_device_opened}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}"
            ),
        )
    }
}

fn audit_manifest_promotion_receipt(lane: &Path, stage: MozaBundleStage) -> LaneAuditCheck {
    let path = promotion_receipt_path(stage);
    let stage_label = stage_label(stage);
    let receipt = match read_json_value(lane, path) {
        Ok(value) => value,
        Err(e) => {
            return LaneAuditCheck::missing(stage_label, "promotion", path, e.to_string());
        }
    };

    let (completion_state, hardware_validated, simulator_validated) =
        manifest_promotion_values(stage);
    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza promote-manifest");
    let lane_ok = path_value_matches(lane, json_string(&receipt, "lane"));
    let manifest_ok = path_value_matches(
        &lane.join("manifest.json"),
        json_string(&receipt, "manifest"),
    );
    let stage_ok = json_string(&receipt, "stage") == Some(stage_label);
    let completion_ok = json_string(&receipt, "completion_state") == Some(completion_state);
    let hardware_ok = json_bool(&receipt, "hardware_validated") == Some(hardware_validated);
    let simulator_ok = json_bool(&receipt, "simulator_validated") == Some(simulator_validated);
    let no_overclaim = json_bool(&receipt, "high_torque_validated") == Some(false)
        && json_bool(&receipt, "release_ready") == Some(false);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let verification_before_ok =
        promotion_verification_summary_ok(receipt.get("verification_before"), stage_label);
    let verification_after_ok =
        promotion_verification_summary_ok(receipt.get("verification_after"), stage_label);
    let safe = success
        && command_ok
        && lane_ok
        && manifest_ok
        && stage_ok
        && completion_ok
        && hardware_ok
        && simulator_ok
        && no_overclaim
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope
        && verification_before_ok
        && verification_after_ok;

    if safe {
        LaneAuditCheck::pass(
            stage_label,
            "promotion",
            path,
            "stored promote-manifest receipt passed, records before/after live verification, and declares no HID opens, FFB writes, serial config, firmware, or DFU commands".to_string(),
        )
    } else {
        LaneAuditCheck::invalid(
            stage_label,
            "promotion",
            path,
            format!(
                "success={success}, command_ok={command_ok}, lane_ok={lane_ok}, manifest_ok={manifest_ok}, stage_ok={stage_ok}, completion_ok={completion_ok}, hardware_ok={hardware_ok}, simulator_ok={simulator_ok}, no_overclaim={no_overclaim}, no_hid_device_opened={no_hid_device_opened}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}, verification_before_ok={verification_before_ok}, verification_after_ok={verification_after_ok}"
            ),
        )
    }
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

fn receipt_path_matches(lane: &Path, receipt: &Value, relative_path: &str) -> bool {
    path_value_matches(
        &lane.join(relative_path),
        json_string(receipt, "receipt_path"),
    )
}

fn verification_receipt_path(stage: MozaBundleStage) -> &'static str {
    match stage {
        MozaBundleStage::Passive => "passive-verification.json",
        MozaBundleStage::Zero => "zero-verification.json",
        MozaBundleStage::SmokeReady => "smoke-ready-verification.json",
    }
}

fn promotion_verification_summary_ok(summary: Option<&Value>, expected_stage: &str) -> bool {
    let Some(summary) = summary else {
        return false;
    };

    json_bool(summary, "success") == Some(true)
        && json_string(summary, "requested_stage") == Some(expected_stage)
        && json_u64(summary, "missing_artifacts") == Some(0)
        && json_u64(summary, "invalid_artifacts") == Some(0)
        && json_u64(summary, "failed_gates") == Some(0)
        && json_bool(summary, "no_hid_device_opened") == Some(true)
        && json_bool(summary, "no_ffb_writes") == Some(true)
        && json_bool(summary, "no_serial_config_commands") == Some(true)
        && json_bool(summary, "no_firmware_or_dfu_commands") == Some(true)
}

fn promotion_receipt_path(stage: MozaBundleStage) -> &'static str {
    match stage {
        MozaBundleStage::Passive => "manifest-promotion-passive.json",
        MozaBundleStage::Zero => "manifest-promotion-zero.json",
        MozaBundleStage::SmokeReady => "manifest-promotion-smoke-ready.json",
    }
}

fn audit_receipt_path(stage: MozaBundleStage) -> &'static str {
    match stage {
        MozaBundleStage::Passive => "lane-audit-passive.json",
        MozaBundleStage::Zero => "lane-audit-zero.json",
        MozaBundleStage::SmokeReady => "lane-audit-smoke-ready.json",
    }
}

pub(crate) fn support_bundle_status(lane: &Path) -> Value {
    support_bundle_status_with_support_validation(lane, SupportBundleValidationMode::ShapeOnly)
}

fn support_bundle_status_for_receipt_validation(lane: &Path) -> Value {
    support_bundle_status_with_support_validation(lane, SupportBundleValidationMode::ShapeOnly)
}

fn support_bundle_status_with_support_validation(
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> Value {
    let manifest_path = lane.join("manifest.json");
    let manifest = match read_json_path(&manifest_path) {
        Ok(value) => serde_json::json!({
            "present": true,
            "completion_state": json_string(&value, "completion_state"),
            "hardware_validated": json_bool(&value, "hardware_validated"),
            "simulator_validated": json_bool(&value, "simulator_validated"),
            "high_torque_validated": json_bool(&value, "high_torque_validated"),
            "release_ready": json_bool(&value, "release_ready")
        }),
        Err(error) => serde_json::json!({
            "present": manifest_path.exists(),
            "error": error.to_string()
        }),
    };

    let passive = verify_bundle_dir_with_support_validation(
        lane,
        MozaBundleStage::Passive,
        support_validation,
    );
    let zero =
        verify_bundle_dir_with_support_validation(lane, MozaBundleStage::Zero, support_validation);
    let smoke_ready = verify_bundle_dir_with_support_validation(
        lane,
        MozaBundleStage::SmokeReady,
        support_validation,
    );
    let passive_audit_passed = stored_lane_audit_receipt_passed(lane, MozaBundleStage::Passive);
    let zero_audit_passed = stored_lane_audit_receipt_passed(lane, MozaBundleStage::Zero);
    let smoke_ready_audit_passed =
        stored_lane_audit_receipt_passed(lane, MozaBundleStage::SmokeReady);
    let readiness = support_readiness_summary(
        &passive,
        &zero,
        &smoke_ready,
        passive_audit_passed,
        zero_audit_passed,
        smoke_ready_audit_passed,
    );
    let artifact_index: Vec<_> = lane_artifact_index_requirements()
        .map(|requirement| check_bundle_artifact(lane, &requirement))
        .collect();

    serde_json::json!({
        "lane": lane.display().to_string(),
        "lane_directory_present": lane.is_dir(),
        "generated_at_utc": now_utc(),
        "manifest": manifest,
        "artifact_index": artifact_index,
        "readiness": readiness,
        "verifications": {
            "passive": support_verification_summary(&passive),
            "zero": support_verification_summary(&zero),
            "smoke_ready": support_verification_summary(&smoke_ready)
        },
        "notes": [
            "support bundle status reads Moza lane receipts only; it opens no HID device and sends no reports",
            "support bundle status is diagnostic context, not a readiness promotion or release claim"
        ]
    })
}

fn support_readiness_summary(
    passive: &BundleVerificationReceipt,
    zero: &BundleVerificationReceipt,
    smoke_ready: &BundleVerificationReceipt,
    passive_audit_passed: bool,
    zero_audit_passed: bool,
    smoke_ready_audit_passed: bool,
) -> Value {
    let init_handshakes_pass = bundle_gate_passed(smoke_ready, "init_off_handshake")
        && bundle_gate_passed(smoke_ready, "init_standard_handshake");
    let (highest_passing_stage, next_required_stage) = if smoke_ready.success {
        ("smoke_ready", Value::Null)
    } else if zero.success {
        ("zero", Value::String("smoke_ready".to_string()))
    } else if passive.success {
        ("passive", Value::String("zero".to_string()))
    } else {
        ("none", Value::String("passive".to_string()))
    };

    let first_blocking = if !passive.success {
        support_verification_summary(passive)
    } else if !zero.success {
        support_verification_summary(zero)
    } else if !smoke_ready.success {
        support_verification_summary(smoke_ready)
    } else {
        Value::Null
    };

    serde_json::json!({
        "highest_passing_stage": highest_passing_stage,
        "next_required_stage": next_required_stage,
        "ready_for_zero_torque": passive.success && passive_audit_passed,
        "ready_for_low_torque": zero.success && zero_audit_passed && init_handshakes_pass,
        "ready_for_real_hardware_smoke": smoke_ready.success && smoke_ready_audit_passed,
        "passive_lane_audit_passed": passive_audit_passed,
        "zero_lane_audit_passed": zero_audit_passed,
        "smoke_ready_lane_audit_passed": smoke_ready_audit_passed,
        "release_ready": false,
        "first_blocking_stage": first_blocking,
        "claim_scope": "diagnostic_context_only"
    })
}

fn stored_lane_audit_receipt_passed(lane: &Path, stage: MozaBundleStage) -> bool {
    let path = audit_receipt_path(stage);
    let Ok(receipt) = read_json_value(lane, path) else {
        return false;
    };

    let receipt_checks_ok = receipt
        .get("receipt_checks")
        .and_then(Value::as_array)
        .map(|checks| {
            let expected_check_count = audit_stages_through(stage).len() * 2;
            checks.len() == expected_check_count
                && checks
                    .iter()
                    .all(|check| json_string(check, "status") == Some("pass"))
        })
        .unwrap_or(false);

    json_bool(&receipt, "success") == Some(true)
        && json_string(&receipt, "command") == Some("wheelctl moza audit-lane")
        && path_value_matches(lane, json_string(&receipt, "lane"))
        && json_string(&receipt, "requested_stage") == Some(stage_label(stage))
        && json_bool(&receipt, "live_verification_success") == Some(true)
        && json_u64(&receipt, "missing_receipts") == Some(0)
        && json_u64(&receipt, "invalid_receipts") == Some(0)
        && receipt_checks_ok
        && json_bool(&receipt, "no_hid_device_opened") == Some(true)
        && json_bool(&receipt, "no_ffb_writes") == Some(true)
        && no_out_of_scope_device_commands(&receipt)
}

fn bundle_gate_passed(receipt: &BundleVerificationReceipt, name: &str) -> bool {
    receipt
        .gates
        .iter()
        .any(|gate| gate.name == name && gate.status == "pass")
}

fn support_verification_summary(receipt: &BundleVerificationReceipt) -> Value {
    let missing_artifacts: Vec<_> = receipt
        .artifacts
        .iter()
        .filter(|artifact| artifact.status == "missing")
        .map(|artifact| artifact.path.clone())
        .collect();
    let invalid_artifacts: Vec<_> = receipt
        .artifacts
        .iter()
        .filter(|artifact| artifact.status == "invalid")
        .map(|artifact| artifact.path.clone())
        .collect();
    let failed_gates: Vec<_> = receipt
        .gates
        .iter()
        .filter(|gate| gate.status == "fail")
        .map(|gate| gate.name)
        .collect();

    serde_json::json!({
        "success": receipt.success,
        "requested_stage": receipt.requested_stage,
        "missing_artifacts": receipt.missing_artifacts,
        "invalid_artifacts": receipt.invalid_artifacts,
        "failed_gates": receipt.failed_gates,
        "missing_artifact_paths": missing_artifacts,
        "invalid_artifact_paths": invalid_artifacts,
        "failed_gate_names": failed_gates
    })
}

fn build_capture_fixture(
    capture: &Path,
    fixture_id: &str,
    pid_override: Option<u16>,
    max_reports: usize,
) -> Result<MozaCaptureFixture> {
    if fixture_id.trim().is_empty() {
        return Err(anyhow!("--fixture-id must not be empty"));
    }
    if max_reports == 0 || max_reports > 10_000 {
        return Err(anyhow!("--max-reports must be in 1..=10000"));
    }

    let file =
        File::open(capture).with_context(|| format!("failed to open '{}'", capture.display()))?;
    let reader = BufReader::new(file);
    let mut summary = CaptureValidationSummary::default();
    let mut product_ids = BTreeMap::new();
    let mut reports = Vec::new();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line.with_context(|| format!("failed to read line {line_no}"))?;

        if line.trim().is_empty() {
            continue;
        }

        summary.total_reports += 1;

        let parsed_line: CapturedInputReportLine = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSON capture line {line_no}"))?;

        let pid = pid_override
            .or_else(|| {
                parsed_line
                    .product_id
                    .as_deref()
                    .and_then(parse_hex_selector)
            })
            .ok_or_else(|| {
                anyhow!("line {line_no} is missing product_id; pass --pid to override")
            })?;

        let data = parsed_line
            .decode_data()
            .map_err(|e| anyhow!("line {line_no}: {e}"))?;
        if let Some(expected_len) = parsed_line.report_len
            && expected_len != data.len()
        {
            return Err(anyhow!(
                "line {line_no}: report_len mismatch: declared {expected_len}, decoded {}",
                data.len()
            ));
        }

        let report_id = data.first().copied().unwrap_or(0);
        increment_count(&mut summary.report_ids, hex_u8(report_id));
        increment_count(&mut summary.report_lengths, data.len().to_string());
        increment_count(&mut product_ids, hex_u16(pid));

        let protocol = MozaProtocol::new_with_config(pid, FfbMode::Off, false);
        let state = protocol
            .parse_input_state(&data)
            .ok_or_else(|| anyhow!("line {line_no}: Moza parser rejected report"))?;
        summary.record_parsed(pid, &state, &data);

        if reports.len() < max_reports {
            let identity = identify_device(pid);
            reports.push(MozaFixtureReport {
                source_line: line_no,
                product_id: hex_u16(pid),
                product_category: category_label(identity.category).to_string(),
                report_id: hex_u8(report_id),
                report_len: data.len(),
                data_hex: bytes_hex_compact(&data),
                parsed: MozaFixtureParsedState::from_input_state(&state),
            });
        }
    }

    if summary.total_reports == 0 {
        return Err(anyhow!("capture contains no non-empty report lines"));
    }

    Ok(MozaCaptureFixture {
        schema_version: 1,
        fixture_id: fixture_id.to_string(),
        generated_at_utc: now_utc(),
        source_capture: capture.display().to_string(),
        pid_override: pid_override.map(hex_u16),
        no_ffb_writes: true,
        total_reports: summary.total_reports,
        included_reports: reports.len(),
        fixture_truncated: summary.total_reports > reports.len(),
        product_ids,
        parsed_by_category: summary.parsed_by_category,
        report_ids: summary.report_ids,
        report_lengths: summary.report_lengths,
        axis_ranges: summary.axis_ranges,
        reports,
        notes: vec![
            "fixture was generated offline from capture JSONL and parser replay".to_string(),
            "fixture omits HID path, manufacturer, product string, and raw device identity data"
                .to_string(),
        ],
    })
}

fn bundle_artifact_requirements() -> &'static [BundleArtifactRequirement] {
    const REQUIREMENTS: &[BundleArtifactRequirement] = &[
        BundleArtifactRequirement::json("manifest.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("device-list.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("moza-probe.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("hid-list.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("hardware-doctor.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("descriptor.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::jsonl("captures/r5-idle.jsonl", MozaBundleStage::Passive),
        BundleArtifactRequirement::jsonl(
            "captures/r5-steering-sweep.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl(
            "captures/r5-throttle-only-sweep.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl(
            "captures/r5-brake-only-sweep.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl(
            "captures/r5-clutch-only-sweep.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl(
            "captures/r5-handbrake-only-sweep.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl(
            "captures/r5-aggregated-idle-after-controls.jsonl",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::jsonl("captures/ks-controls.jsonl", MozaBundleStage::Passive),
        BundleArtifactRequirement::jsonl("captures/es-controls.jsonl", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("parser-fixture-validation.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("fixture-promotion.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("init-off.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("init-standard.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("moza-status.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("device-status.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("support-bundle.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("zero-torque-proof.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json("watchdog-proof.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json("disconnect-proof.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json("low-torque-proof.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json("pit-house-coexistence.json", MozaBundleStage::SmokeReady),
        BundleArtifactRequirement::json(
            "simulator-telemetry-proof.json",
            MozaBundleStage::SmokeReady,
        ),
        BundleArtifactRequirement::json("simulator-ffb-smoke.json", MozaBundleStage::SmokeReady),
    ];
    REQUIREMENTS
}

fn bundle_artifact_requirements_for_lane(lane: &Path) -> Vec<BundleArtifactRequirement> {
    let passive_paths = passive_capture_requirements_for_lane(lane)
        .into_iter()
        .map(|requirement| requirement.relative_path)
        .collect::<BTreeSet<_>>();
    bundle_artifact_requirements()
        .iter()
        .copied()
        .filter(|requirement| {
            !is_passive_capture_artifact(requirement.relative_path)
                || passive_paths.contains(requirement.relative_path)
        })
        .collect()
}

fn is_passive_capture_artifact(relative_path: &str) -> bool {
    passive_capture_requirements()
        .iter()
        .any(|requirement| requirement.relative_path == relative_path)
}

fn lane_artifact_index_requirements() -> impl Iterator<Item = BundleArtifactRequirement> {
    bundle_artifact_requirements()
        .iter()
        .copied()
        .chain(stored_receipt_artifact_requirements().iter().copied())
}

fn stored_receipt_artifact_requirements() -> &'static [BundleArtifactRequirement] {
    const REQUIREMENTS: &[BundleArtifactRequirement] = &[
        BundleArtifactRequirement::json("passive-verification.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json(
            "manifest-promotion-passive.json",
            MozaBundleStage::Passive,
        ),
        BundleArtifactRequirement::json("lane-audit-passive.json", MozaBundleStage::Passive),
        BundleArtifactRequirement::json("zero-verification.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json("manifest-promotion-zero.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json("lane-audit-zero.json", MozaBundleStage::Zero),
        BundleArtifactRequirement::json(
            "smoke-ready-verification.json",
            MozaBundleStage::SmokeReady,
        ),
        BundleArtifactRequirement::json(
            "manifest-promotion-smoke-ready.json",
            MozaBundleStage::SmokeReady,
        ),
        BundleArtifactRequirement::json("lane-audit-smoke-ready.json", MozaBundleStage::SmokeReady),
    ];
    REQUIREMENTS
}

fn check_bundle_artifact(
    lane: &Path,
    requirement: &BundleArtifactRequirement,
) -> BundleArtifactCheck {
    let path = lane.join(requirement.relative_path);
    if !path.is_file() {
        return BundleArtifactCheck {
            path: requirement.relative_path.to_string(),
            kind: requirement.kind.label().to_string(),
            required_stage: stage_label(requirement.stage).to_string(),
            exists: false,
            valid: false,
            line_count: None,
            status: "missing".to_string(),
            notes: vec![format!("expected {}", path.display())],
        };
    }

    match requirement.kind {
        BundleArtifactKind::Json => match read_json_value(lane, requirement.relative_path) {
            Ok(_) => BundleArtifactCheck {
                path: requirement.relative_path.to_string(),
                kind: requirement.kind.label().to_string(),
                required_stage: stage_label(requirement.stage).to_string(),
                exists: true,
                valid: true,
                line_count: None,
                status: "pass".to_string(),
                notes: Vec::new(),
            },
            Err(e) => BundleArtifactCheck {
                path: requirement.relative_path.to_string(),
                kind: requirement.kind.label().to_string(),
                required_stage: stage_label(requirement.stage).to_string(),
                exists: true,
                valid: false,
                line_count: None,
                status: "invalid".to_string(),
                notes: vec![e.to_string()],
            },
        },
        BundleArtifactKind::JsonLines => match validate_jsonl_file(&path) {
            Ok(line_count) => BundleArtifactCheck {
                path: requirement.relative_path.to_string(),
                kind: requirement.kind.label().to_string(),
                required_stage: stage_label(requirement.stage).to_string(),
                exists: true,
                valid: true,
                line_count: Some(line_count),
                status: "pass".to_string(),
                notes: Vec::new(),
            },
            Err(e) => BundleArtifactCheck {
                path: requirement.relative_path.to_string(),
                kind: requirement.kind.label().to_string(),
                required_stage: stage_label(requirement.stage).to_string(),
                exists: true,
                valid: false,
                line_count: None,
                status: "invalid".to_string(),
                notes: vec![e.to_string()],
            },
        },
    }
}

fn validate_jsonl_file(path: &Path) -> Result<usize> {
    read_jsonl_values(path).map(|records| records.len())
}

fn read_jsonl_values(path: &Path) -> Result<Vec<Value>> {
    let file = File::open(path).with_context(|| format!("failed to open '{}'", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line.with_context(|| format!("failed to read line {line_no}"))?;
        if line.trim().is_empty() {
            continue;
        }
        records.push(
            serde_json::from_str::<Value>(&line)
                .with_context(|| format!("line {line_no} is not valid JSON"))?,
        );
    }

    if records.is_empty() {
        return Err(anyhow!("JSONL artifact has no non-empty JSON lines"));
    }

    Ok(records)
}

fn verify_manifest_gate(lane: &Path, stage: MozaBundleStage) -> BundleGateCheck {
    let manifest = match read_json_value(lane, "manifest.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("manifest_no_overclaim", e.to_string()),
    };

    let completion_state = json_string(&manifest, "completion_state");
    let hardware_validated = json_bool(&manifest, "hardware_validated");
    let simulator_validated = json_bool(&manifest, "simulator_validated");
    let high_torque_validated = json_bool(&manifest, "high_torque_validated");
    let release_ready = json_bool(&manifest, "release_ready");

    let schema_errors = manifest_schema_validation_errors(&manifest);
    let schema_ok = schema_errors.is_empty();
    let contract_ok = manifest_contract_is_moza_r5_lane(&manifest);
    let base_ok = high_torque_validated == Some(false) && release_ready == Some(false);
    let non_claiming_manifest = hardware_validated == Some(false)
        && simulator_validated == Some(false)
        && matches!(
            completion_state,
            Some("not_started" | "passive_capture_ready" | "zero_torque_ready")
        );
    let stage_ok = match stage {
        MozaBundleStage::Passive | MozaBundleStage::Zero => non_claiming_manifest,
        MozaBundleStage::SmokeReady => {
            non_claiming_manifest
                || (completion_state == Some("real_hardware_smoke_ready")
                    && hardware_validated == Some(true)
                    && simulator_validated == Some(true))
        }
    };

    if schema_ok && contract_ok && base_ok && stage_ok {
        BundleGateCheck::pass(
            "manifest_no_overclaim",
            format!(
                "manifest validates against manifest.schema.json and is scoped to {}",
                stage_label(stage)
            ),
        )
    } else {
        BundleGateCheck::fail(
            "manifest_no_overclaim",
            format!(
                "schema_ok={schema_ok}, schema_errors={schema_errors:?}, contract_ok={contract_ok}, completion_state={:?}, hardware_validated={:?}, simulator_validated={:?}, high_torque_validated={:?}, release_ready={:?}",
                completion_state,
                hardware_validated,
                simulator_validated,
                high_torque_validated,
                release_ready
            ),
        )
    }
}

fn verify_manifest_r5_pid_consistency_gate_with_support_validation(
    lane: &Path,
    stage: MozaBundleStage,
    support_validation: SupportBundleValidationMode,
) -> BundleGateCheck {
    let Some(expected_pid) = lane_manifest_r5_pid(lane) else {
        return BundleGateCheck::fail(
            "manifest_r5_pid_consistency",
            "manifest.json is missing a supported hardware.wheelbase_pid".to_string(),
        );
    };
    let expected = hex_u16(expected_pid);
    let mut observed = Vec::new();
    let mut mismatches = Vec::new();
    let mut unavailable = Vec::new();

    for path in [
        "device-list.json",
        "moza-probe.json",
        "hid-list.json",
        "descriptor.json",
    ] {
        match manifest_pid_device_artifact_counts(lane, path) {
            Ok(counts) if pid_counts_only_expected(&counts, &expected) => {
                observed.push(format!("{path}:{}", pid_counts_details(&counts)));
            }
            Ok(counts) => mismatches.push(format!("{path}:{}", pid_counts_details(&counts))),
            Err(error) => unavailable.push(format!("{path}:{error}")),
        }
    }

    for requirement in passive_capture_requirements_for_lane(lane)
        .into_iter()
        .filter(|requirement| {
            matches!(
                requirement.expected_products,
                PassiveCaptureProductRequirement::ManifestR5
            )
        })
    {
        let path = requirement.relative_path;
        match manifest_pid_capture_artifact_counts(lane, path) {
            Ok(counts) if pid_counts_only_expected(&counts, &expected) => {
                observed.push(format!("{path}:{}", pid_counts_details(&counts)));
            }
            Ok(counts) => mismatches.push(format!("{path}:{}", pid_counts_details(&counts))),
            Err(error) => unavailable.push(format!("{path}:{error}")),
        }
    }

    for requirement in passive_capture_requirements_for_lane(lane)
        .into_iter()
        .filter(|requirement| requirement.required_category == "wheelbase")
    {
        match manifest_pid_fixture_requirement_counts(lane, requirement) {
            Ok(counts) if pid_counts_only_expected(&counts, &expected) => {
                observed.push(format!(
                    "fixture:{}:{}",
                    requirement.fixture_id,
                    pid_counts_details(&counts)
                ));
            }
            Ok(counts) => mismatches.push(format!(
                "fixture:{}:{}",
                requirement.fixture_id,
                pid_counts_details(&counts)
            )),
            Err(error) => unavailable.push(format!("fixture:{}:{error}", requirement.fixture_id)),
        }
    }

    if stage_rank(stage) >= stage_rank(MozaBundleStage::Zero) {
        for path in [
            "zero-torque-proof.json",
            "watchdog-proof.json",
            "disconnect-proof.json",
        ] {
            match manifest_pid_receipt_device_pid(lane, path) {
                Ok(pid) if pid == expected => observed.push(format!("{path}:{pid}")),
                Ok(pid) => mismatches.push(format!("{path}:{pid}")),
                Err(error) => unavailable.push(format!("{path}:{error}")),
            }
        }
    }

    if stage_rank(stage) >= stage_rank(MozaBundleStage::SmokeReady) {
        for path in [
            "init-off.json",
            "init-standard.json",
            "low-torque-proof.json",
            "simulator-ffb-smoke.json",
        ] {
            match manifest_pid_receipt_device_pid(lane, path) {
                Ok(pid) if pid == expected => observed.push(format!("{path}:{pid}")),
                Ok(pid) => mismatches.push(format!("{path}:{pid}")),
                Err(error) => unavailable.push(format!("{path}:{error}")),
            }
        }
        match manifest_pid_simulator_writer_pid(lane) {
            Ok(pid) if pid == expected => observed.push(format!("simulator-ffb-writer:{pid}")),
            Ok(pid) => mismatches.push(format!("simulator-ffb-writer:{pid}")),
            Err(error) => unavailable.push(format!("simulator-ffb-writer:{error}")),
        }
        match manifest_pid_service_receipts_with_support_validation(lane, support_validation) {
            Ok(pids) if pids.iter().all(|pid| pid == &expected) => {
                observed.push(format!("service-status:{}", pids.join(",")));
            }
            Ok(pids) => mismatches.push(format!("service-status:{}", pids.join(","))),
            Err(error) => unavailable.push(format!("service-status:{error}")),
        }
    }

    if mismatches.is_empty() {
        let mut details = format!(
            "manifest hardware.wheelbase_pid {expected} matches available lane R5 receipts: {}",
            observed.join(", ")
        );
        if !unavailable.is_empty() {
            details.push_str(&format!(
                "; unavailable PID evidence is covered by stage artifact gates: {unavailable:?}"
            ));
        }
        BundleGateCheck::pass("manifest_r5_pid_consistency", details)
    } else {
        BundleGateCheck::fail(
            "manifest_r5_pid_consistency",
            format!(
                "manifest hardware.wheelbase_pid {expected} does not match available lane R5 receipt PID evidence: {mismatches:?}; unavailable PID evidence: {unavailable:?}"
            ),
        )
    }
}

fn manifest_pid_device_artifact_counts(
    lane: &Path,
    relative_path: &str,
) -> Result<BTreeMap<String, usize>> {
    let artifact = read_json_value(lane, relative_path)?;
    let Some(devices) = artifact.get("devices").and_then(Value::as_array) else {
        return Err(anyhow!("{relative_path} is missing devices array"));
    };
    let mut counts = BTreeMap::new();
    for device in devices.iter().filter(|device| is_r5_device_value(device)) {
        let pid = json_string(device, "product_id")
            .and_then(parse_hex_selector)
            .map(hex_u16)
            .unwrap_or_else(|| "missing".to_string());
        increment_count(&mut counts, pid);
    }
    if counts.is_empty() {
        return Err(anyhow!("{relative_path} has no R5 VID/PID records"));
    }
    Ok(counts)
}

fn manifest_pid_capture_artifact_counts(
    lane: &Path,
    relative_path: &str,
) -> Result<BTreeMap<String, usize>> {
    let records = read_receipt_artifact_records(lane, relative_path, &[])
        .ok_or_else(|| anyhow!("failed to read capture artifact"))?;
    if records.is_empty() {
        return Err(anyhow!("capture contains no records"));
    }

    let mut counts = BTreeMap::new();
    for record in &records {
        let pid = json_string(record, "product_id")
            .and_then(parse_hex_selector)
            .map(hex_u16)
            .unwrap_or_else(|| "missing".to_string());
        increment_count(&mut counts, pid);
    }
    Ok(counts)
}

fn manifest_pid_fixture_requirement_counts(
    lane: &Path,
    requirement: &PassiveCaptureRequirement,
) -> Result<BTreeMap<String, usize>> {
    let receipt = read_json_value(lane, "fixture-promotion.json")?;
    let fixtures = receipt
        .get("fixtures")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("fixture-promotion.json is missing fixtures[]"))?;
    let entry = fixtures
        .iter()
        .find(|entry| fixture_entry_matches_requirement(entry, requirement))
        .ok_or_else(|| anyhow!("fixture promotion entry is missing"))?;

    let mut counts = BTreeMap::new();
    add_product_id_counts_from_map(entry.get("product_ids"), &mut counts);

    let fixture_out =
        json_string(entry, "fixture_out").ok_or_else(|| anyhow!("entry is missing fixture_out"))?;
    let fixture_path = resolve_fixture_out_path(lane, fixture_out).ok_or_else(|| {
        anyhow!("fixture_out must be lane-relative or under crates/hid-moza-protocol/fixtures")
    })?;
    let fixture = read_json_path(&fixture_path)?;
    add_product_id_counts_from_map(fixture.get("product_ids"), &mut counts);
    if let Some(reports) = fixture.get("reports").and_then(Value::as_array) {
        for report in reports {
            let pid = json_string(report, "product_id")
                .and_then(parse_hex_selector)
                .map(hex_u16)
                .unwrap_or_else(|| "missing".to_string());
            increment_count(&mut counts, pid);
        }
    }

    if counts.is_empty() {
        return Err(anyhow!("fixture has no product_id evidence"));
    }
    Ok(counts)
}

fn add_product_id_counts_from_map(value: Option<&Value>, counts: &mut BTreeMap<String, usize>) {
    let Some(map) = value.and_then(Value::as_object) else {
        return;
    };
    for (pid, count) in map {
        let pid = parse_hex_selector(pid)
            .map(hex_u16)
            .unwrap_or_else(|| "missing".to_string());
        let count = count
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        *counts.entry(pid).or_insert(0) += count;
    }
}

fn manifest_pid_receipt_device_pid(lane: &Path, relative_path: &str) -> Result<String> {
    let receipt = read_json_value(lane, relative_path)?;
    receipt_r5_device_pid(&receipt)
        .map(hex_u16)
        .ok_or_else(|| anyhow!("{relative_path} is missing an R5 output-capable device PID"))
}

fn manifest_pid_simulator_writer_pid(lane: &Path) -> Result<String> {
    let receipt = read_json_value(lane, "simulator-ffb-smoke.json")?;
    json_string(&receipt, "writer_product_id")
        .and_then(parse_hex_selector)
        .map(hex_u16)
        .ok_or_else(|| anyhow!("simulator-ffb-smoke.json is missing writer_product_id"))
}

fn manifest_pid_service_receipts_with_support_validation(
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> Result<Vec<String>> {
    let moza_status = read_json_value(lane, "moza-status.json")?;
    let device_status = read_json_value(lane, "device-status.json")?;
    let support_bundle = read_json_value(lane, "support-bundle.json")?;
    let summaries = [
        moza_status_receipt_summary(&moza_status, lane),
        device_status_receipt_summary(&device_status, lane),
        support_bundle_receipt_summary_with_validation(&support_bundle, lane, support_validation),
    ];
    let mut pids = Vec::new();
    for summary in summaries {
        let pid = summary
            .product_id
            .as_deref()
            .and_then(parse_hex_selector)
            .map(hex_u16)
            .ok_or_else(|| anyhow!("service receipt summary missing R5 product_id"))?;
        pids.push(pid);
    }
    pids.extend(support_bundle_top_level_r5_product_ids(&support_bundle));
    Ok(pids)
}

fn pid_counts_only_expected(counts: &BTreeMap<String, usize>, expected: &str) -> bool {
    !counts.is_empty()
        && counts
            .iter()
            .all(|(pid, count)| pid == expected && *count > 0)
}

fn pid_counts_details(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(pid, count)| format!("{pid}={count}"))
        .collect::<Vec<_>>()
        .join("|")
}

fn manifest_schema_validation_errors(manifest: &Value) -> Vec<String> {
    let schema_value: Value = match serde_json::from_str(MOZA_R5_MANIFEST_SCHEMA_JSON) {
        Ok(value) => value,
        Err(e) => return vec![format!("schema JSON parse failed: {e}")],
    };

    let validator = match Validator::new(&schema_value) {
        Ok(validator) => validator,
        Err(e) => return vec![format!("schema compile failed: {e}")],
    };

    validator
        .iter_errors(manifest)
        .take(8)
        .map(|error| {
            let path = error.instance_path().to_string();
            if path.is_empty() {
                format!("root: {error}")
            } else {
                format!("{path}: {error}")
            }
        })
        .collect()
}

fn manifest_contract_is_moza_r5_lane(manifest: &Value) -> bool {
    json_u64(manifest, "schema_version") == Some(1)
        && json_string(manifest, "lane") == Some("moza-r5-windows-usb")
        && json_string(manifest, "operator")
            .map(|operator| !operator.trim().is_empty())
            .unwrap_or(false)
        && manifest_platform_is_windows_hid(manifest.get("platform"))
        && manifest_hardware_is_declared_moza_r5(manifest.get("hardware"))
        && manifest_topology_is_logical_role_model(manifest)
        && manifest_claims_are_staged(manifest.get("claims"))
        && manifest_artifacts_match_lane_contract(manifest.get("artifacts"))
}

fn manifest_platform_is_windows_hid(platform: Option<&Value>) -> bool {
    let Some(platform) = platform else {
        return false;
    };
    let transport = platform.get("transport");
    json_string(platform, "os") == Some("Windows")
        && transport
            .map(|transport| {
                json_bool(transport, "hid") == Some(true)
                    && json_bool(transport, "serial_config") == Some(false)
            })
            .unwrap_or(false)
}

fn manifest_hardware_is_declared_moza_r5(hardware: Option<&Value>) -> bool {
    let Some(hardware) = hardware else {
        return false;
    };
    json_string(hardware, "wheelbase") == Some("Moza R5")
        && matches!(
            json_string(hardware, "wheelbase_pid"),
            Some("0x0004" | "0x0014")
        )
        && optional_json_array_only_contains_strings(hardware, "rims", &["KS", "ES"])
        && optional_json_array_only_contains_strings(hardware, "pedals", &["SR-P"])
        && json_string(hardware, "handbrake")
            .map(|handbrake| handbrake == "HBP")
            .unwrap_or(true)
}

fn optional_json_array_only_contains_strings(value: &Value, key: &str, allowed: &[&str]) -> bool {
    let Some(items) = value.get(key) else {
        return true;
    };
    let Some(items) = items.as_array() else {
        return false;
    };
    items.iter().all(|item| {
        item.as_str()
            .map(|item| allowed.contains(&item))
            .unwrap_or(false)
    })
}

fn manifest_topology_is_logical_role_model(manifest: &Value) -> bool {
    let Some(topology) = manifest.get("topology") else {
        return false;
    };
    let Some(endpoints) = topology.get("endpoints").and_then(Value::as_array) else {
        return false;
    };
    let Some(controls) = topology.get("logical_controls").and_then(Value::as_object) else {
        return false;
    };
    let Some(wheelbase_pid) = manifest
        .get("hardware")
        .and_then(|hardware| json_string(hardware, "wheelbase_pid"))
    else {
        return false;
    };

    let endpoint_ids = endpoints
        .iter()
        .filter_map(|endpoint| json_string(endpoint, "id"))
        .collect::<BTreeSet<_>>();
    let r5_hub_endpoint_ok = endpoints.iter().any(|endpoint| {
        json_string(endpoint, "id") == Some("moza-r5-if2")
            && json_string(endpoint, "kind") == Some("wheelbase_hub")
            && json_string(endpoint, "vendor_id") == Some(MOZA_VENDOR_HEX)
            && json_string(endpoint, "product_id") == Some(wheelbase_pid)
            && json_u64(endpoint, "interface_number") == Some(2)
            && json_string(endpoint, "usage_page") == Some("0x0001")
            && json_string(endpoint, "usage") == Some("0x0004")
            && json_bool(endpoint, "output_capable") == Some(true)
    });

    let controls_ok = !controls.is_empty()
        && controls
            .values()
            .all(|control| topology_control_is_declared_role(control, &endpoint_ids));
    let has_required_control = controls
        .values()
        .any(|control| json_bool(control, "required").unwrap_or(true));

    json_string(topology, "primary_input_path") == Some("wheelbase_hub")
        && r5_hub_endpoint_ok
        && controls_ok
        && has_required_control
}

fn topology_control_is_declared_role(control: &Value, endpoint_ids: &BTreeSet<&str>) -> bool {
    let Some(endpoint) = json_string(control, "source_endpoint") else {
        return false;
    };
    matches!(
        json_string(control, "role"),
        Some(
            "steering"
                | "rim_controls"
                | "throttle"
                | "brake"
                | "clutch"
                | "handbrake"
                | "shifter"
                | "button_box"
        )
    ) && json_bool(control, "required").is_some()
        && endpoint_ids.contains(endpoint)
        && matches!(
            json_string(control, "connection"),
            Some("wheelbase_hub" | "standalone_usb" | "cross_device" | "unknown")
        )
        && topology_control_semantic_status_is_valid(control)
        && json_string(control, "evidence_capture")
            .map(|path| {
                passive_capture_requirements()
                    .iter()
                    .any(|req| req.relative_path == path)
            })
            .unwrap_or(false)
}

fn topology_control_semantic_status_is_valid(control: &Value) -> bool {
    matches!(
        json_string(control, "semantic_status"),
        Some("proven" | "generic_aux" | "missing" | "unavailable" | "deferred")
    )
}

fn manifest_claims_are_staged(claims: Option<&Value>) -> bool {
    let Some(claims) = claims else {
        return false;
    };
    json_string(claims, "ffb") == Some("staged")
        && json_bool(claims, "high_torque") == Some(false)
        && json_string(claims, "pit_house_coexistence") == Some("tested_separately")
}

fn manifest_artifacts_match_lane_contract(artifacts: Option<&Value>) -> bool {
    let Some(artifacts) = artifacts else {
        return false;
    };
    [
        ("manifest", "manifest.json"),
        ("device_list", "device-list.json"),
        ("hid_list", "hid-list.json"),
        ("moza_probe", "moza-probe.json"),
        ("hardware_doctor", "hardware-doctor.json"),
        ("descriptor", "descriptor.json"),
        ("captures_dir", "captures"),
        ("capture_r5_idle", "captures/r5-idle.jsonl"),
        (
            "capture_r5_steering_sweep",
            "captures/r5-steering-sweep.jsonl",
        ),
        (
            "capture_r5_throttle_only_sweep",
            "captures/r5-throttle-only-sweep.jsonl",
        ),
        (
            "capture_r5_brake_only_sweep",
            "captures/r5-brake-only-sweep.jsonl",
        ),
        (
            "capture_r5_clutch_only_sweep",
            "captures/r5-clutch-only-sweep.jsonl",
        ),
        (
            "capture_r5_handbrake_only_sweep",
            "captures/r5-handbrake-only-sweep.jsonl",
        ),
        (
            "capture_r5_aggregated_idle_after_controls",
            "captures/r5-aggregated-idle-after-controls.jsonl",
        ),
        ("capture_ks_controls", "captures/ks-controls.jsonl"),
        ("capture_es_controls", "captures/es-controls.jsonl"),
        (
            "parser_fixture_validation",
            "parser-fixture-validation.json",
        ),
        ("fixture_promotion", "fixture-promotion.json"),
        ("passive_verification", "passive-verification.json"),
        (
            "passive_manifest_promotion",
            "manifest-promotion-passive.json",
        ),
        ("passive_lane_audit", "lane-audit-passive.json"),
        ("init_off", "init-off.json"),
        ("init_standard", "init-standard.json"),
        ("moza_status", "moza-status.json"),
        ("device_status", "device-status.json"),
        ("support_bundle", "support-bundle.json"),
        ("zero_torque_proof", "zero-torque-proof.json"),
        ("watchdog_proof", "watchdog-proof.json"),
        ("disconnect_proof", "disconnect-proof.json"),
        ("zero_verification", "zero-verification.json"),
        ("zero_manifest_promotion", "manifest-promotion-zero.json"),
        ("zero_lane_audit", "lane-audit-zero.json"),
        ("low_torque_proof", "low-torque-proof.json"),
        ("pit_house_coexistence", "pit-house-coexistence.json"),
        (
            "simulator_telemetry_proof",
            "simulator-telemetry-proof.json",
        ),
        ("simulator_ffb_smoke", "simulator-ffb-smoke.json"),
        ("smoke_ready_verification", "smoke-ready-verification.json"),
        (
            "smoke_ready_manifest_promotion",
            "manifest-promotion-smoke-ready.json",
        ),
        ("smoke_ready_lane_audit", "lane-audit-smoke-ready.json"),
    ]
    .iter()
    .all(|(key, expected)| json_string(artifacts, key) == Some(*expected))
}

fn json_string_array_contains_all(value: &Value, key: &str, required: &[&str]) -> bool {
    let Some(values) = value.get(key).and_then(Value::as_array) else {
        return false;
    };
    required.iter().all(|required_value| {
        values
            .iter()
            .any(|value| value.as_str() == Some(*required_value))
    })
}

fn verify_moza_r5_observed_gate(lane: &Path) -> BundleGateCheck {
    let mut observed = Vec::new();
    let mut failures = Vec::new();
    for path in [
        "device-list.json",
        "moza-probe.json",
        "hid-list.json",
        "descriptor.json",
    ] {
        match read_json_value(lane, path) {
            Ok(value) => {
                let count = count_r5_devices(&value);
                if count > 0 {
                    observed.push(format!("{path}:{count}"));
                } else {
                    failures.push(format!("{path}:0"));
                }
            }
            Err(e) => failures.push(format!("{path}:{e}")),
        }
    }

    if failures.is_empty() {
        BundleGateCheck::pass(
            "moza_r5_observed",
            format!("found R5 VID/PID records in {}", observed.join(", ")),
        )
    } else {
        BundleGateCheck::fail(
            "moza_r5_observed",
            format!("missing R5 VID/PID observation in {failures:?}"),
        )
    }
}

fn verify_moza_topology_observed_gate(lane: &Path) -> BundleGateCheck {
    let manifest = match read_json_value(lane, "manifest.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("moza_topology_observed", e.to_string()),
    };
    let Some(topology) = manifest.get("topology") else {
        return BundleGateCheck::fail(
            "moza_topology_observed",
            "manifest.json is missing topology".to_string(),
        );
    };
    let Some(endpoints) = topology.get("endpoints").and_then(Value::as_array) else {
        return BundleGateCheck::fail(
            "moza_topology_observed",
            "manifest topology is missing endpoints[]".to_string(),
        );
    };
    let Some(controls) = topology.get("logical_controls").and_then(Value::as_object) else {
        return BundleGateCheck::fail(
            "moza_topology_observed",
            "manifest topology is missing logical_controls{}".to_string(),
        );
    };

    let mut endpoint_ids = BTreeSet::new();
    let mut endpoint_products = Vec::new();
    let mut failures = Vec::new();

    for endpoint in endpoints {
        let id = json_string(endpoint, "id").unwrap_or("<missing-id>");
        endpoint_ids.insert(id.to_string());
        let vendor_id = json_string(endpoint, "vendor_id").and_then(parse_hex_selector);
        let product_id = json_string(endpoint, "product_id").and_then(parse_hex_selector);
        match (vendor_id, product_id) {
            (Some(vid), Some(pid)) => endpoint_products.push((id.to_string(), vid, pid)),
            _ => failures.push(format!("endpoint:{id}:missing-or-invalid-vid-pid")),
        };
    }

    for (name, control) in controls {
        if !topology_control_semantic_status_is_valid(control) {
            failures.push(format!("control:{name}:invalid-semantic-status"));
        }
        let required = json_bool(control, "required").unwrap_or(true);
        if !required {
            continue;
        }
        let Some(endpoint) = json_string(control, "source_endpoint") else {
            failures.push(format!("control:{name}:missing-source-endpoint"));
            continue;
        };
        if !endpoint_ids.contains(endpoint) {
            failures.push(format!("control:{name}:unknown-endpoint:{endpoint}"));
        }
        if !matches!(
            json_string(control, "connection"),
            Some("wheelbase_hub" | "standalone_usb" | "cross_device" | "unknown")
        ) {
            failures.push(format!("control:{name}:invalid-connection"));
        }
        match json_string(control, "evidence_capture") {
            Some(path)
                if passive_capture_requirements()
                    .iter()
                    .any(|requirement| requirement.relative_path == path) => {}
            Some(path) => failures.push(format!("control:{name}:unknown-evidence-capture:{path}")),
            None => failures.push(format!("control:{name}:missing-evidence-capture")),
        }
    }

    let mut observed = Vec::new();

    for path in [
        "device-list.json",
        "moza-probe.json",
        "hid-list.json",
        "descriptor.json",
    ] {
        match read_json_value(lane, path) {
            Ok(value) => {
                for (endpoint_id, vendor_id, product_id) in &endpoint_products {
                    let count = count_vendor_product_devices(&value, *vendor_id, *product_id);
                    if count > 0 {
                        observed.push(format!(
                            "{path}:{endpoint_id}:{}:{}:{count}",
                            hex_u16(*vendor_id),
                            hex_u16(*product_id)
                        ));
                    } else {
                        failures.push(format!(
                            "{path}:{endpoint_id}:{}:{}:0",
                            hex_u16(*vendor_id),
                            hex_u16(*product_id)
                        ));
                    }
                }
            }
            Err(e) => failures.push(format!("{path}:{e}")),
        }
    }

    if failures.is_empty() {
        BundleGateCheck::pass(
            "moza_topology_observed",
            format!(
                "found declared topology endpoint VID/PID records in {}",
                observed.join(", ")
            ),
        )
    } else {
        BundleGateCheck::fail(
            "moza_topology_observed",
            format!(
                "missing declared topology endpoint observation or logical role evidence in {failures:?}"
            ),
        )
    }
}

fn verify_descriptor_metadata_gate(lane: &Path) -> BundleGateCheck {
    let descriptor = match read_json_value(lane, "descriptor.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("descriptor_metadata", e.to_string()),
    };

    let r5_devices: Vec<_> = descriptor
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| is_r5_device_value(device))
                .collect()
        })
        .unwrap_or_default();

    let valid_count = r5_devices
        .iter()
        .filter(|device| r5_descriptor_metadata_is_complete(device))
        .count();

    if valid_count > 0 {
        BundleGateCheck::pass(
            "descriptor_metadata",
            format!("found complete descriptor metadata on {valid_count} R5 record(s)"),
        )
    } else {
        let diagnostics = r5_devices
            .iter()
            .enumerate()
            .map(|(index, device)| {
                let product_id = json_string(device, "product_id").unwrap_or("<missing>");
                let descriptor_source =
                    json_string(device, "descriptor_source").unwrap_or("<missing>");
                let report_metadata_source =
                    json_string(device, "report_metadata_source").unwrap_or("<missing>");
                let missing = r5_descriptor_metadata_missing_requirements(device);
                format!(
                    "r5[{index}] product_id={product_id} descriptor_source={descriptor_source} report_metadata_source={report_metadata_source} missing={missing:?}"
                )
            })
            .collect::<Vec<_>>();
        BundleGateCheck::fail(
            "descriptor_metadata",
            format!(
                "descriptor.json has {} R5 record(s), but none have trusted descriptor source, CRC, identity metadata, and R5 input/output/feature report metadata; diagnostics={diagnostics:?}",
                r5_devices.len(),
            ),
        )
    }
}

fn r5_descriptor_metadata_is_complete(device: &Value) -> bool {
    r5_descriptor_metadata_missing_requirements(device).is_empty()
}

fn r5_descriptor_metadata_missing_requirements(device: &Value) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !matches!(
        json_string(device, "descriptor_source"),
        Some("linux_sysfs" | "operator_supplied_hex")
    ) {
        missing.push("descriptor_source");
    }
    if !json_string(device, "report_descriptor_crc32")
        .map(|value| value.starts_with("0x") && value.len() == 10)
        .unwrap_or(false)
    {
        missing.push("report_descriptor_crc32");
    }
    let identity_ok = json_bool(device, "serial_number_present") == Some(true)
        && json_string(device, "product_name")
            .or_else(|| json_string(device, "product_string"))
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && json_string(device, "manufacturer")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    if !identity_ok {
        missing.push("identity_metadata");
    }
    let interface_ok = device
        .get("interface_number")
        .and_then(Value::as_i64)
        .is_some()
        && json_string(device, "usage_page")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    if !interface_ok {
        missing.push("interface_usage_metadata");
    }
    let input_ok = json_usize_array(device, "input_report_lengths")
        .map(|lengths| r5_input_report_lengths_supported(device, &lengths))
        .unwrap_or(false);
    if !input_ok {
        missing.push("input_report_lengths");
    }
    let output_ok = json_string_array_contains_all(device, "output_report_ids", &["0x20"])
        && json_report_record_contains(device, "output_reports", "0x20", REPORT_LEN);
    if !output_ok {
        missing.push("output_report_0x20_len_8");
    }
    if !json_string_array_contains_all(device, "feature_report_ids", &["0x03", "0x11"]) {
        missing.push("feature_report_ids_0x03_0x11");
    }
    missing
}

fn r5_descriptor_trusted_for_direct_mode(device: &Value) -> bool {
    r5_descriptor_metadata_is_complete(device)
        && matches!(
            json_string(device, "report_metadata_source"),
            Some("report_descriptor_parsed" | "descriptor_parsed")
        )
        && report_descriptor_hex_proves_r5_metadata(device)
}

fn report_descriptor_hex_proves_r5_metadata(device: &Value) -> bool {
    let Some(hex) = json_string(device, "report_descriptor_hex") else {
        return false;
    };
    let Ok(bytes) = parse_hex_bytes(hex) else {
        return false;
    };
    if bytes.is_empty() {
        return false;
    }
    if json_u64(device, "report_descriptor_len") != Some(bytes.len() as u64) {
        return false;
    }

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let expected_crc = format!("0x{:08X}", hasher.finalize());
    if json_string(device, "report_descriptor_crc32") != Some(expected_crc.as_str()) {
        return false;
    }

    parse_hid_report_descriptor_metadata(&bytes)
        .map(|metadata| {
            let direct_output_report_shape_ok = metadata
                .output_reports
                .iter()
                .any(|report| report.report_id == 0x20 && report.report_len == REPORT_LEN);
            r5_input_report_lengths_supported(device, &metadata.input_report_lengths)
                && direct_output_report_shape_ok
                && json_usize_array_equals(
                    device,
                    "input_report_lengths",
                    &metadata.input_report_lengths,
                )
                && json_string_array_equals_u8_hex(
                    device,
                    "output_report_ids",
                    &metadata.output_report_ids,
                )
                && json_report_records_equal_u8_hex(
                    device,
                    "output_reports",
                    &metadata.output_reports,
                )
                && json_string_array_equals_u8_hex(
                    device,
                    "feature_report_ids",
                    &metadata.feature_report_ids,
                )
        })
        .unwrap_or(false)
}

fn r5_input_report_lengths_supported(device: &Value, lengths: &[usize]) -> bool {
    let product_id = json_string(device, "product_id").and_then(parse_hex_selector);
    r5_supported_input_report_length_sets(product_id).contains(&lengths)
}

fn r5_supported_input_report_length_sets(product_id: Option<u16>) -> &'static [&'static [usize]] {
    match product_id {
        Some(product_ids::R5_V1) => &[&[42], &[7, 31], &[7, 31, 42]],
        Some(product_ids::R5_V2) => &[&[7, 31]],
        _ => &[&[7, 31], &[42], &[7, 31, 42]],
    }
}

fn json_report_record_contains(
    value: &Value,
    key: &str,
    report_id: &str,
    report_len: usize,
) -> bool {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|records| {
            records.iter().any(|record| {
                json_string(record, "report_id") == Some(report_id)
                    && json_u64(record, "report_len") == Some(report_len as u64)
            })
        })
        .unwrap_or(false)
}

fn json_usize_array(value: &Value, key: &str) -> Option<Vec<usize>> {
    let values = value.get(key).and_then(Value::as_array)?;
    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        parsed.push(value.as_u64()? as usize);
    }
    Some(parsed)
}

fn json_usize_array_equals(value: &Value, key: &str, expected: &[usize]) -> bool {
    let Some(values) = value.get(key).and_then(Value::as_array) else {
        return false;
    };
    values.len() == expected.len()
        && values
            .iter()
            .zip(expected.iter())
            .all(|(value, expected)| value.as_u64() == Some(*expected as u64))
}

fn json_string_array_equals_u8_hex(value: &Value, key: &str, expected: &[u8]) -> bool {
    let Some(values) = value.get(key).and_then(Value::as_array) else {
        return false;
    };
    values.len() == expected.len()
        && values
            .iter()
            .zip(expected.iter())
            .all(|(value, expected)| value.as_str() == Some(hex_u8(*expected).as_str()))
}

fn json_report_records_equal_u8_hex(
    value: &Value,
    key: &str,
    expected: &[HidReportDescriptorReport],
) -> bool {
    let Some(values) = value.get(key).and_then(Value::as_array) else {
        return false;
    };
    values.len() == expected.len()
        && values.iter().zip(expected.iter()).all(|(value, expected)| {
            json_string(value, "report_id") == Some(hex_u8(expected.report_id).as_str())
                && json_u64(value, "report_len") == Some(expected.report_len as u64)
        })
}

fn lane_descriptor_trusted_for_pid(lane: &Path, product_id: &str) -> bool {
    read_json_value(lane, "descriptor.json")
        .ok()
        .and_then(|receipt| receipt.get("devices").and_then(Value::as_array).cloned())
        .map(|devices| {
            devices.iter().any(|device| {
                is_r5_device_value(device)
                    && json_string(device, "product_id") == Some(product_id)
                    && r5_descriptor_trusted_for_direct_mode(device)
            })
        })
        .unwrap_or(false)
}

pub(crate) fn apply_lane_readiness_to_device_status(
    status: &mut crate::client::DeviceStatus,
    lane: &Path,
) {
    let Some(readiness) = status.moza.as_mut() else {
        return;
    };
    let Some(product_id) = status.device.product_id.as_deref() else {
        return;
    };

    let (descriptor_trusted, descriptor_crc32, descriptor_source) =
        lane_descriptor_details_for_pid(lane, product_id);
    readiness.apply_descriptor_receipt(
        lane.display().to_string(),
        descriptor_trusted,
        descriptor_crc32,
        descriptor_source,
    );
    apply_lane_verification_stage(readiness, lane);
}

fn lane_descriptor_details_for_pid(
    lane: &Path,
    product_id: &str,
) -> (bool, Option<String>, Option<String>) {
    read_json_value(lane, "descriptor.json")
        .ok()
        .and_then(|receipt| receipt.get("devices").and_then(Value::as_array).cloned())
        .and_then(|devices| {
            devices
                .into_iter()
                .find(|device| json_string(device, "product_id") == Some(product_id))
                .map(|device| {
                    let trusted = is_r5_device_value(&device)
                        && r5_descriptor_trusted_for_direct_mode(&device);
                    let crc = json_string(&device, "report_descriptor_crc32").map(str::to_string);
                    let source = json_string(&device, "descriptor_source").map(str::to_string);
                    (trusted, crc, source)
                })
        })
        .unwrap_or((false, None, None))
}

fn apply_lane_verification_stage(readiness: &mut crate::client::MozaReadinessStatus, lane: &Path) {
    let stage = stored_lane_verification_stage(lane);
    let highest = stage.highest_passing_stage();
    if highest == "none" {
        return;
    }
    let next = stage.next_required_stage();
    readiness.safety_state = stage.safety_state().to_string();
    readiness.safety_reason = format!(
        "stored Moza lane verification receipts report highest_passing_stage={highest}, next_required_stage={next}; CLI status remains observe-only and torque output stays disabled until explicit service initialization is implemented"
    );
    readiness.direct_mode_allowed = false;
    readiness.high_torque_allowed = false;
    readiness.safe_to_send_torque = false;
}

#[derive(Debug, Default)]
struct StoredLaneVerificationStage {
    passive_success: bool,
    zero_success: bool,
    smoke_ready_success: bool,
    init_off_success: bool,
    init_standard_success: bool,
}

impl StoredLaneVerificationStage {
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

    fn safety_state(&self) -> &'static str {
        if self.smoke_ready_success {
            "lane_smoke_ready_receipts_observed"
        } else if self.zero_success && self.init_off_success && self.init_standard_success {
            "lane_low_torque_gate_receipts_observed"
        } else if self.zero_success {
            "lane_zero_torque_verified"
        } else if self.passive_success {
            "lane_passive_verified"
        } else {
            "pre_validation"
        }
    }
}

fn stored_lane_verification_stage(lane: &Path) -> StoredLaneVerificationStage {
    let passive = read_stored_verification_receipt(lane, MozaBundleStage::Passive);
    let zero = read_stored_verification_receipt(lane, MozaBundleStage::Zero);
    let smoke_ready = read_stored_verification_receipt(lane, MozaBundleStage::SmokeReady);
    let init_off_observed =
        verify_init_receipt_gate(lane, "init_off_handshake", "init-off.json", "off").status
            == "pass";
    let init_standard_observed = verify_init_receipt_gate(
        lane,
        "init_standard_handshake",
        "init-standard.json",
        "standard",
    )
    .status
        == "pass";

    StoredLaneVerificationStage {
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

#[derive(Debug)]
struct StoredVerificationReceipt {
    success: bool,
    gates: Vec<StoredVerificationGate>,
}

impl StoredVerificationReceipt {
    fn gate_passed(&self, name: &str) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.name == name && gate.status == "pass")
    }
}

#[derive(Debug)]
struct StoredVerificationGate {
    name: String,
    status: String,
}

fn read_stored_verification_receipt(
    lane: &Path,
    stage: MozaBundleStage,
) -> Option<StoredVerificationReceipt> {
    let relative_path = verification_receipt_path(stage);
    let receipt = read_json_value(lane, relative_path).ok()?;
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza verify-bundle");
    let lane_ok = path_value_matches(lane, json_string(&receipt, "lane"));
    let stage_ok = json_string(&receipt, "requested_stage") == Some(stage_label(stage));
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
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
    Some(StoredVerificationReceipt {
        success: json_bool(&receipt, "success") == Some(true) && counts_ok,
        gates: receipt
            .get("gates")
            .and_then(Value::as_array)
            .map(|gates| {
                gates
                    .iter()
                    .filter_map(|gate| {
                        Some(StoredVerificationGate {
                            name: json_string(gate, "name")?.to_string(),
                            status: json_string(gate, "status")?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn verify_passive_no_writes_gate(lane: &Path) -> BundleGateCheck {
    let mut missing = Vec::new();
    let mut wrong_command = Vec::new();
    let mut unsafe_receipts = Vec::new();
    let mut opened_hid = Vec::new();
    let mut out_of_scope_receipts = Vec::new();
    for requirement in passive_receipt_requirements() {
        match read_json_value(lane, requirement.path) {
            Ok(value) => {
                let command = json_string(&value, "command");
                if !requirement
                    .allowed_commands
                    .iter()
                    .any(|allowed| command == Some(*allowed))
                {
                    wrong_command.push(requirement.path.to_string());
                }
                if requirement.no_hid_device_opened
                    && json_bool(&value, "no_hid_device_opened") != Some(true)
                {
                    opened_hid.push(requirement.path.to_string());
                }
                if json_bool(&value, "no_ffb_writes") != Some(true) {
                    unsafe_receipts.push(requirement.path.to_string());
                }
                if !no_out_of_scope_device_commands(&value) {
                    out_of_scope_receipts.push(requirement.path.to_string());
                }
            }
            Err(_) => missing.push(requirement.path.to_string()),
        }
    }

    if missing.is_empty()
        && wrong_command.is_empty()
        && unsafe_receipts.is_empty()
        && opened_hid.is_empty()
        && out_of_scope_receipts.is_empty()
    {
        BundleGateCheck::pass(
            "passive_receipts_no_ffb_writes",
            "passive receipts come from expected observe-only commands, declare no_ffb_writes=true, keep pure observation receipts no_hid_device_opened=true, and declare no serial/firmware/DFU commands".to_string(),
        )
    } else {
        BundleGateCheck::fail(
            "passive_receipts_no_ffb_writes",
            format!(
                "missing={missing:?}, wrong_command={wrong_command:?}, not_no_ffb_writes={unsafe_receipts:?}, opened_hid={opened_hid:?}, missing_no_serial_firmware_dfu={out_of_scope_receipts:?}"
            ),
        )
    }
}

fn verify_passive_receipts_success_gate(lane: &Path) -> BundleGateCheck {
    let mut missing = Vec::new();
    let mut wrong_command = Vec::new();
    let mut unsuccessful = Vec::new();
    for requirement in passive_receipt_requirements() {
        match read_json_value(lane, requirement.path) {
            Ok(value) => {
                let command = json_string(&value, "command");
                if !requirement
                    .allowed_commands
                    .iter()
                    .any(|allowed| command == Some(*allowed))
                {
                    wrong_command.push(requirement.path.to_string());
                }
                if json_bool(&value, "success") != Some(true) {
                    unsuccessful.push(requirement.path.to_string());
                }
            }
            Err(_) => missing.push(requirement.path.to_string()),
        }
    }

    if missing.is_empty() && wrong_command.is_empty() && unsuccessful.is_empty() {
        BundleGateCheck::pass(
            "passive_receipts_successful",
            "passive receipts come from expected successful observe-only commands".to_string(),
        )
    } else {
        BundleGateCheck::fail(
            "passive_receipts_successful",
            format!(
                "missing={missing:?}, wrong_command={wrong_command:?}, unsuccessful={unsuccessful:?}"
            ),
        )
    }
}

struct PassiveReceiptRequirement {
    path: &'static str,
    allowed_commands: &'static [&'static str],
    no_hid_device_opened: bool,
}

fn passive_receipt_requirements() -> [PassiveReceiptRequirement; 7] {
    [
        PassiveReceiptRequirement {
            path: "device-list.json",
            allowed_commands: &["wheelctl device list"],
            no_hid_device_opened: false,
        },
        PassiveReceiptRequirement {
            path: "moza-probe.json",
            allowed_commands: &["wheelctl moza probe"],
            no_hid_device_opened: true,
        },
        PassiveReceiptRequirement {
            path: "hid-list.json",
            allowed_commands: &["hid-capture list"],
            no_hid_device_opened: true,
        },
        PassiveReceiptRequirement {
            path: "hardware-doctor.json",
            allowed_commands: &["wheelctl hardware doctor"],
            no_hid_device_opened: true,
        },
        PassiveReceiptRequirement {
            path: "descriptor.json",
            allowed_commands: &["wheelctl moza descriptor", "hid-capture descriptor"],
            no_hid_device_opened: true,
        },
        PassiveReceiptRequirement {
            path: "parser-fixture-validation.json",
            allowed_commands: &["wheelctl moza validate-captures"],
            no_hid_device_opened: true,
        },
        PassiveReceiptRequirement {
            path: "fixture-promotion.json",
            allowed_commands: &["wheelctl moza promote-fixtures"],
            no_hid_device_opened: true,
        },
    ]
}

fn verify_passive_capture_parse_gate(lane: &Path) -> BundleGateCheck {
    let mut failures = Vec::new();
    let mut total_reports = 0usize;

    for requirement in passive_capture_requirements_for_lane(lane) {
        let path = lane.join(requirement.relative_path);
        let receipt = match validate_capture_file(&path, None) {
            Ok(receipt) => receipt,
            Err(e) => {
                failures.push(format!("{}: {e}", requirement.relative_path));
                continue;
            }
        };

        total_reports = total_reports.saturating_add(receipt.parsed_reports);
        let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
        let evaluation =
            evaluate_passive_capture_requirement(requirement, &receipt, &expected_product_ids);

        if !receipt.success || !evaluation.success {
            failures.push(format!(
                "{}: success={}, expected_product_ids={:?}, product_ids={:?}, product_ids_ok={}, category={} count={}, axes_ok={}, exact_axes_ok={}, any_axes_ok={}, report_len_ok={}, capture_input_metadata_ok={}, missing_requirements={:?}, total_reports={}, parsed_reports={}, rejected_reports={}, capture_input_format_reports={}",
                requirement.relative_path,
                receipt.success,
                product_id_hex_list(&expected_product_ids),
                receipt.product_ids,
                evaluation.product_ids_ok,
                requirement.required_category,
                evaluation.category_count,
                evaluation.axes_ok,
                evaluation.exact_axes_ok,
                evaluation.any_axes_ok,
                evaluation.report_len_ok,
                evaluation.capture_input_metadata_ok,
                evaluation.missing_requirements,
                receipt.total_reports,
                receipt.parsed_reports,
                receipt.rejected_reports,
                receipt.capture_input_format_reports
            ));
        }
    }

    if failures.is_empty() {
        BundleGateCheck::pass(
            "passive_captures_parse",
            format!("replayed {total_reports} passive capture report(s) through Moza parsers"),
        )
    } else {
        let mut details = failures.join("; ");
        if let Ok(analysis) = analyze_lane_captures(lane)
            && !analysis.safe_diagnostics.is_empty()
        {
            details.push_str("; safe_diagnostics=");
            details.push_str(&format!("{:?}", analysis.safe_diagnostics));
        }
        BundleGateCheck::fail("passive_captures_parse", details)
    }
}

fn passive_capture_requirements() -> &'static [PassiveCaptureRequirement] {
    const NONE: &[&str] = &[];
    const STEERING: &[&str] = &["steering_u16"];
    const PEDALS: &[&str] = &["throttle_u16", "brake_u16"];
    const HANDBRAKE: &[&str] = &["handbrake_u16"];
    const NO_EXACT: &[(&str, u16)] = &[];
    const NO_ANY: &[(&str, &[&str])] = &[];
    const HUB_CONTROL_AXIS_ANY: &[&str] = &[
        "throttle_u16",
        "brake_u16",
        "clutch_u16",
        "handbrake_u16",
        "r5_v1_extended_axis0_u16",
        "r5_v1_extended_axis1_u16",
        "r5_v1_extended_axis2_u16",
        "r5_v1_extended_aux0_u16",
        "r5_v1_extended_aux1_u16",
    ];
    const HUB_CONTROL_GROUPS: &[(&str, &[&str])] = &[("hub_control_axis", HUB_CONTROL_AXIS_ANY)];
    const BUTTONS_ANY: &[&str] = &["buttons_any_u8"];
    const KS_BUTTONS_ANY: &[&str] = &["ks_buttons_any_u8", "buttons_any_u8"];
    const KS_DIRECTION_ANY: &[&str] = &["ks_hat_u8", "hat_u8"];
    const KS_CONTROL_GROUPS: &[(&str, &[&str])] = &[
        ("buttons", KS_BUTTONS_ANY),
        // Live R5 V1 + KS captures move a packed direction/control byte while
        // the legacy rim/funky and rotary bytes stay zero on this layout.
        ("direction", KS_DIRECTION_ANY),
    ];
    const ES_CONTROL_GROUPS: &[(&str, &[&str])] = &[("buttons", BUTTONS_ANY)];
    const REQUIREMENTS: &[PassiveCaptureRequirement] = &[
        PassiveCaptureRequirement {
            relative_path: "captures/r5-idle.jsonl",
            fixture_id: "r5_idle",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: NO_ANY,
            min_report_len: None,
            always_required: true,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-steering-sweep.jsonl",
            fixture_id: "r5_steering_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: STEERING,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: NO_ANY,
            min_report_len: None,
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-throttle-only-sweep.jsonl",
            fixture_id: "r5_throttle_only_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: HUB_CONTROL_GROUPS,
            min_report_len: None,
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-brake-only-sweep.jsonl",
            fixture_id: "r5_brake_only_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: HUB_CONTROL_GROUPS,
            min_report_len: None,
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-clutch-only-sweep.jsonl",
            fixture_id: "r5_clutch_only_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: HUB_CONTROL_GROUPS,
            min_report_len: None,
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-handbrake-only-sweep.jsonl",
            fixture_id: "r5_handbrake_only_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: HUB_CONTROL_GROUPS,
            min_report_len: None,
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/r5-aggregated-idle-after-controls.jsonl",
            fixture_id: "r5_aggregated_idle_after_controls",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: NO_ANY,
            min_report_len: None,
            always_required: true,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/ks-controls.jsonl",
            fixture_id: "ks_controls",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: KS_CONTROL_GROUPS,
            min_report_len: Some(31),
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/es-controls.jsonl",
            fixture_id: "es_controls",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: ES_CONTROL_GROUPS,
            min_report_len: Some(31),
            always_required: false,
            default_required: true,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/srp-wheelbase-aggregated-sweep.jsonl",
            fixture_id: "srp_wheelbase_aggregated_sweep",
            required_category: "wheelbase",
            expected_products: PassiveCaptureProductRequirement::ManifestR5,
            required_axis_variation: NONE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: HUB_CONTROL_GROUPS,
            min_report_len: None,
            always_required: false,
            default_required: false,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/srp-standalone-sweep.jsonl",
            fixture_id: "srp_standalone_sweep",
            required_category: "pedals",
            expected_products: PassiveCaptureProductRequirement::Fixed(&[product_ids::SR_P_PEDALS]),
            required_axis_variation: PEDALS,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: NO_ANY,
            min_report_len: None,
            always_required: false,
            default_required: false,
        },
        PassiveCaptureRequirement {
            relative_path: "captures/hbp-standalone-sweep.jsonl",
            fixture_id: "hbp_standalone_sweep",
            required_category: "handbrake",
            expected_products: PassiveCaptureProductRequirement::Fixed(&[
                product_ids::HBP_HANDBRAKE,
            ]),
            required_axis_variation: HANDBRAKE,
            required_axis_values: NO_EXACT,
            required_any_axis_variation: NO_ANY,
            min_report_len: None,
            always_required: false,
            default_required: false,
        },
    ];
    REQUIREMENTS
}

fn default_passive_capture_requirements() -> Vec<&'static PassiveCaptureRequirement> {
    passive_capture_requirements()
        .iter()
        .filter(|requirement| requirement.default_required)
        .collect()
}

fn passive_capture_requirements_for_lane(lane: &Path) -> Vec<&'static PassiveCaptureRequirement> {
    let required_paths = read_json_value(lane, "manifest.json")
        .ok()
        .and_then(|manifest| manifest_topology_required_capture_paths(&manifest));
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();

    for requirement in passive_capture_requirements() {
        let required = required_paths
            .as_ref()
            .map(|paths| requirement.always_required || paths.contains(requirement.relative_path))
            .unwrap_or(requirement.default_required);
        if required && seen.insert(requirement.relative_path) {
            selected.push(requirement);
        }
    }

    if selected.is_empty() {
        default_passive_capture_requirements()
    } else {
        selected
    }
}

fn manifest_topology_required_capture_paths(manifest: &Value) -> Option<BTreeSet<String>> {
    let controls = manifest
        .get("topology")?
        .get("logical_controls")?
        .as_object()?;
    let mut paths = BTreeSet::new();

    for control in controls.values() {
        let required = json_bool(control, "required").unwrap_or(true);
        if required && let Some(path) = json_string(control, "evidence_capture") {
            paths.insert(path.to_string());
        }
    }

    if paths.is_empty() { None } else { Some(paths) }
}

fn axis_has_variation(ranges: &BTreeMap<String, AxisRange>, axis: &str) -> bool {
    ranges
        .get(axis)
        .and_then(|range| range.min.zip(range.max))
        .map(|(min, max)| min < max)
        .unwrap_or(false)
}

fn axis_contains_value(ranges: &BTreeMap<String, AxisRange>, axis: &str, value: u16) -> bool {
    ranges
        .get(axis)
        .and_then(|range| range.min.zip(range.max))
        .map(|(min, max)| min <= value && value <= max)
        .unwrap_or(false)
}

fn report_lengths_include_minimum(lengths: &BTreeMap<String, usize>, min_len: usize) -> bool {
    lengths
        .keys()
        .filter_map(|value| value.parse::<usize>().ok())
        .any(|len| len >= min_len)
}

fn expected_product_ids_for_requirement(
    requirement: &PassiveCaptureRequirement,
    lane: &Path,
) -> Vec<u16> {
    match requirement.expected_products {
        PassiveCaptureProductRequirement::ManifestR5 => {
            lane_manifest_r5_pid(lane).into_iter().collect()
        }
        PassiveCaptureProductRequirement::Fixed(product_ids) => product_ids.to_vec(),
    }
}

fn product_id_hex_list(product_ids: &[u16]) -> Vec<String> {
    product_ids.iter().copied().map(hex_u16).collect()
}

fn product_id_counts_match_expected(
    counts: &BTreeMap<String, usize>,
    expected_product_ids: &[u16],
    total_reports: usize,
) -> bool {
    let expected: BTreeSet<String> = expected_product_ids.iter().copied().map(hex_u16).collect();
    let counted_reports = counts.values().sum::<usize>();
    !expected.is_empty()
        && total_reports > 0
        && counted_reports == total_reports
        && !counts.is_empty()
        && counts
            .keys()
            .all(|product_id| expected.contains(product_id))
}

fn product_id_array_matches_expected(value: Option<&Value>, expected_product_ids: &[u16]) -> bool {
    let mut actual = value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut expected = product_id_hex_list(expected_product_ids);
    actual.sort_unstable();
    expected.sort_unstable();
    !expected.is_empty() && actual == expected
}

fn evaluate_passive_capture_requirement(
    requirement: &PassiveCaptureRequirement,
    receipt: &CaptureValidationReceipt,
    expected_product_ids: &[u16],
) -> PassiveCaptureEvaluation {
    let product_ids_ok = product_id_counts_match_expected(
        &receipt.product_ids,
        expected_product_ids,
        receipt.total_reports,
    );
    let category_count = receipt
        .parsed_by_category
        .get(requirement.required_category)
        .copied()
        .unwrap_or(0);
    let category_ok = category_count > 0;
    let axes_ok = requirement
        .required_axis_variation
        .iter()
        .all(|axis| axis_has_variation(&receipt.axis_ranges, axis));
    let exact_axes_ok = requirement
        .required_axis_values
        .iter()
        .all(|(axis, value)| axis_contains_value(&receipt.axis_ranges, axis, *value));
    let any_axes_ok = requirement
        .required_any_axis_variation
        .iter()
        .all(|(_, axes)| {
            axes.iter()
                .any(|axis| axis_has_variation(&receipt.axis_ranges, axis))
        });
    let report_len_ok = requirement
        .min_report_len
        .map(|min_len| report_lengths_include_minimum(&receipt.report_lengths, min_len))
        .unwrap_or(true);
    let capture_input_metadata_ok = receipt.all_reports_have_capture_input_metadata;

    let mut missing_requirements = Vec::new();
    if !receipt.success {
        missing_requirements.push(format!(
            "capture validation success with at least one report and no rejected reports (total={}, parsed={}, rejected={})",
            receipt.total_reports, receipt.parsed_reports, receipt.rejected_reports
        ));
    }
    if !product_ids_ok {
        missing_requirements.push(format!(
            "all reports use expected product IDs {:?}; observed {:?}",
            product_id_hex_list(expected_product_ids),
            receipt.product_ids
        ));
    }
    if !category_ok {
        missing_requirements.push(format!(
            "parsed category '{}' count > 0",
            requirement.required_category
        ));
    }
    for axis in requirement.required_axis_variation {
        if !axis_has_variation(&receipt.axis_ranges, axis) {
            missing_requirements.push(format!("axis variation for {axis}"));
        }
    }
    for (axis, value) in requirement.required_axis_values {
        if !axis_contains_value(&receipt.axis_ranges, axis, *value) {
            missing_requirements.push(format!(
                "axis {axis} includes expected value {}",
                hex_axis_value(*value)
            ));
        }
    }
    for (group, axes) in requirement.required_any_axis_variation {
        if !axes
            .iter()
            .any(|axis| axis_has_variation(&receipt.axis_ranges, axis))
        {
            missing_requirements.push(format!(
                "variation in {group} group via one of [{}]",
                axes.join(", ")
            ));
        }
    }
    if let Some(min_len) = requirement.min_report_len
        && !report_len_ok
    {
        missing_requirements.push(format!("at least one report length >= {min_len} bytes"));
    }
    if !capture_input_metadata_ok {
        missing_requirements.push(
            "capture-input metadata and no-output assertions on every report line".to_string(),
        );
    }

    PassiveCaptureEvaluation {
        success: missing_requirements.is_empty(),
        product_ids_ok,
        category_count,
        axes_ok,
        exact_axes_ok,
        any_axes_ok,
        report_len_ok,
        capture_input_metadata_ok,
        missing_requirements,
    }
}

fn string_slice_to_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn requirement_axis_values(values: &[(&str, u16)]) -> Vec<CaptureAxisValueRequirement> {
    values
        .iter()
        .map(|(axis, value)| CaptureAxisValueRequirement {
            axis: (*axis).to_string(),
            value: hex_axis_value(*value),
        })
        .collect()
}

fn requirement_any_axis_variation(groups: &[(&str, &[&str])]) -> Vec<CaptureAnyAxisRequirement> {
    groups
        .iter()
        .map(|(group, axes)| CaptureAnyAxisRequirement {
            group: (*group).to_string(),
            axes: string_slice_to_vec(axes),
        })
        .collect()
}

fn hex_axis_value(value: u16) -> String {
    if let Ok(byte) = u8::try_from(value) {
        hex_u8(byte)
    } else {
        hex_u16(value)
    }
}

fn verify_parser_validation_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "parser-fixture-validation.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("parser_fixture_validation", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza validate-captures");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened") == Some(true);
    let required_capture_count = json_u64(&receipt, "required_capture_count").unwrap_or(0);
    let validated_capture_count = json_u64(&receipt, "validated_capture_count").unwrap_or(0);
    let total_reports = json_u64(&receipt, "total_reports").unwrap_or(0);
    let parsed_reports = json_u64(&receipt, "parsed_reports").unwrap_or(0);
    let rejected_reports = json_u64(&receipt, "rejected_reports").unwrap_or(u64::MAX);
    let captures = receipt
        .get("captures")
        .and_then(Value::as_array)
        .map(Vec::as_slice);
    let safe_diagnostics = receipt
        .get("safe_diagnostics")
        .and_then(Value::as_array)
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let requirements = passive_capture_requirements_for_lane(lane);
    let coverage_ok = captures
        .map(|captures| {
            parser_validation_covers_all_required_captures(captures, lane, &requirements)
        })
        .unwrap_or(false);

    if success
        && command_ok
        && no_out_of_scope
        && no_hid_device_opened
        && required_capture_count == requirements.len() as u64
        && validated_capture_count == required_capture_count
        && total_reports > 0
        && parsed_reports == total_reports
        && rejected_reports == 0
        && coverage_ok
    {
        BundleGateCheck::pass(
            "parser_fixture_validation",
            format!(
                "validated {validated_capture_count} required parser capture(s), {parsed_reports} report(s)"
            ),
        )
    } else {
        let mut details = format!(
            "success={success}, command_ok={command_ok}, no_out_of_scope={no_out_of_scope}, no_hid_device_opened={no_hid_device_opened}, required_capture_count={required_capture_count}, validated_capture_count={validated_capture_count}, total_reports={total_reports}, parsed_reports={parsed_reports}, rejected_reports={rejected_reports}, coverage_ok={coverage_ok}"
        );
        if !safe_diagnostics.is_empty() {
            details.push_str(", safe_diagnostics=");
            details.push_str(&format!("{safe_diagnostics:?}"));
        }
        BundleGateCheck::fail("parser_fixture_validation", details)
    }
}

fn parser_validation_covers_all_required_captures(
    captures: &[Value],
    lane: &Path,
    requirements: &[&PassiveCaptureRequirement],
) -> bool {
    requirements.iter().all(|requirement| {
        captures
            .iter()
            .any(|entry| parser_validation_entry_matches_requirement(entry, requirement, lane))
    })
}

fn parser_validation_entry_matches_requirement(
    entry: &Value,
    requirement: &PassiveCaptureRequirement,
    lane: &Path,
) -> bool {
    let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
    let mut observed_product_ids = BTreeMap::new();
    add_product_id_counts_from_map(entry.get("product_ids"), &mut observed_product_ids);
    let total_reports = json_u64(entry, "total_reports").unwrap_or(0) as usize;

    json_string(entry, "fixture_id") == Some(requirement.fixture_id)
        && json_string(entry, "capture")
            .map(|capture| path_string_ends_with(capture, requirement.relative_path))
            .unwrap_or(false)
        && json_string(entry, "required_category") == Some(requirement.required_category)
        && product_id_array_matches_expected(
            entry.get("required_product_ids"),
            &expected_product_ids,
        )
        && product_id_counts_match_expected(
            &observed_product_ids,
            &expected_product_ids,
            total_reports,
        )
        && json_bool(entry, "success") == Some(true)
        && json_u64(entry, "total_reports").unwrap_or(0) > 0
        && json_u64(entry, "parsed_reports") == json_u64(entry, "total_reports")
        && json_u64(entry, "rejected_reports") == Some(0)
        && json_bool(entry, "all_reports_have_capture_input_metadata") == Some(true)
}

fn verify_fixture_promotion_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "fixture-promotion.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("fixture_promotion", e.to_string()),
    };

    if json_string(&receipt, "command") == Some("wheelctl moza promote-fixtures") {
        return verify_fixture_promotion_set_gate(lane, &receipt);
    }

    BundleGateCheck::fail(
        "fixture_promotion",
        "fixture-promotion.json must come from wheelctl moza promote-fixtures and cover every required passive capture".to_string(),
    )
}

fn verify_fixture_promotion_set_gate(lane: &Path, receipt: &Value) -> BundleGateCheck {
    let success = json_bool(receipt, "success") == Some(true);
    let no_ffb_writes = json_bool(receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(receipt);
    let no_hid_device_opened = json_bool(receipt, "no_hid_device_opened") == Some(true);
    let required_fixture_count = json_u64(receipt, "required_fixture_count").unwrap_or(0);
    let fixture_count = json_u64(receipt, "fixture_count").unwrap_or(0);
    let fixtures = match receipt.get("fixtures").and_then(Value::as_array) {
        Some(fixtures) => fixtures,
        None => {
            return BundleGateCheck::fail(
                "fixture_promotion",
                "fixture-promotion.json is missing fixtures[]".to_string(),
            );
        }
    };

    let mut failures = Vec::new();
    let mut promoted_reports = 0u64;
    let requirements = passive_capture_requirements_for_lane(lane);
    for requirement in &requirements {
        let Some(entry) = fixtures
            .iter()
            .find(|entry| fixture_entry_matches_requirement(entry, requirement))
        else {
            failures.push(format!(
                "{}: missing fixture promotion",
                requirement.relative_path
            ));
            continue;
        };

        let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
        match verify_fixture_entry(lane, entry, &expected_product_ids) {
            Ok(report_count) => promoted_reports = promoted_reports.saturating_add(report_count),
            Err(e) => failures.push(format!("{}: {e}", requirement.relative_path)),
        }
    }

    let passed = success
        && no_ffb_writes
        && no_out_of_scope
        && no_hid_device_opened
        && required_fixture_count == requirements.len() as u64
        && fixture_count >= required_fixture_count
        && failures.is_empty()
        && promoted_reports > 0;

    if passed {
        BundleGateCheck::pass(
            "fixture_promotion",
            format!(
                "promoted {promoted_reports} report(s) across {required_fixture_count} required fixture(s)"
            ),
        )
    } else {
        BundleGateCheck::fail(
            "fixture_promotion",
            format!(
                "success={success}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}, no_hid_device_opened={no_hid_device_opened}, required_fixture_count={required_fixture_count}, fixture_count={fixture_count}, promoted_reports={promoted_reports}, failures={failures:?}"
            ),
        )
    }
}

fn fixture_entry_matches_requirement(
    entry: &Value,
    requirement: &PassiveCaptureRequirement,
) -> bool {
    json_string(entry, "fixture_id") == Some(requirement.fixture_id)
        && json_string(entry, "capture")
            .map(|capture| path_string_ends_with(capture, requirement.relative_path))
            .unwrap_or(false)
}

fn verify_fixture_entry(lane: &Path, entry: &Value, expected_product_ids: &[u16]) -> Result<u64> {
    let report_count = json_u64(entry, "report_count").unwrap_or(0);
    let mut entry_product_ids = BTreeMap::new();
    add_product_id_counts_from_map(entry.get("product_ids"), &mut entry_product_ids);
    let fixture_out = json_string(entry, "fixture_out")
        .ok_or_else(|| anyhow!("fixture entry is missing fixture_out"))?;
    let fixture_path = resolve_fixture_out_path(lane, fixture_out).ok_or_else(|| {
        anyhow!("fixture_out must be lane-relative or under crates/hid-moza-protocol/fixtures")
    })?;
    let fixture = read_json_path(&fixture_path)?;

    let fixture_reports = fixture
        .get("reports")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut fixture_product_ids = BTreeMap::new();
    add_product_id_counts_from_map(fixture.get("product_ids"), &mut fixture_product_ids);
    let report_product_ids = fixture_report_product_id_counts(&fixture);
    let entry_product_ids_ok =
        product_id_counts_use_expected_set(&entry_product_ids, expected_product_ids);
    let fixture_product_ids_ok =
        product_id_counts_use_expected_set(&fixture_product_ids, expected_product_ids);
    let report_product_ids_ok =
        product_id_counts_use_expected_set(&report_product_ids, expected_product_ids);
    let fixture_no_ffb = json_bool(&fixture, "no_ffb_writes") == Some(true);
    let replayed_reports = replay_promoted_fixture_reports(&fixture)?;
    let fixture_has_forbidden_identity = contains_any_key(
        &fixture,
        &[
            "path",
            "manufacturer",
            "product_string",
            "serial_number",
            "serial_number_present",
        ],
    );
    if report_count == 0
        || fixture_reports == 0
        || replayed_reports == 0
        || !fixture_no_ffb
        || fixture_has_forbidden_identity
        || !entry_product_ids_ok
        || !fixture_product_ids_ok
        || !report_product_ids_ok
    {
        return Err(anyhow!(
            "report_count={report_count}, fixture_reports={fixture_reports}, replayed_reports={replayed_reports}, expected_product_ids={:?}, entry_product_ids={entry_product_ids:?}, fixture_product_ids={fixture_product_ids:?}, report_product_ids={report_product_ids:?}, entry_product_ids_ok={entry_product_ids_ok}, fixture_product_ids_ok={fixture_product_ids_ok}, report_product_ids_ok={report_product_ids_ok}, fixture_no_ffb={fixture_no_ffb}, fixture_has_forbidden_identity={fixture_has_forbidden_identity}",
            product_id_hex_list(expected_product_ids)
        ));
    }
    Ok(report_count)
}

fn product_id_counts_use_expected_set(
    counts: &BTreeMap<String, usize>,
    expected_product_ids: &[u16],
) -> bool {
    let expected: BTreeSet<String> = expected_product_ids.iter().copied().map(hex_u16).collect();
    !expected.is_empty()
        && !counts.is_empty()
        && counts.values().any(|count| *count > 0)
        && counts
            .iter()
            .all(|(product_id, count)| *count > 0 && expected.contains(product_id))
}

fn fixture_report_product_id_counts(fixture: &Value) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Some(reports) = fixture.get("reports").and_then(Value::as_array) {
        for report in reports {
            let pid = json_string(report, "product_id")
                .and_then(parse_hex_selector)
                .map(hex_u16)
                .unwrap_or_else(|| "missing".to_string());
            increment_count(&mut counts, pid);
        }
    }
    counts
}

fn replay_promoted_fixture_reports(fixture: &Value) -> Result<u64> {
    if json_u64(fixture, "schema_version") != Some(1)
        || json_bool(fixture, "no_ffb_writes") != Some(true)
    {
        return Err(anyhow!(
            "promoted fixture must have schema_version=1 and no_ffb_writes=true"
        ));
    }
    let reports = fixture
        .get("reports")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("promoted fixture is missing reports[]"))?;
    let included_reports = json_u64(fixture, "included_reports")
        .ok_or_else(|| anyhow!("promoted fixture is missing included_reports"))?;
    if usize::try_from(included_reports).ok() != Some(reports.len()) {
        return Err(anyhow!(
            "promoted fixture included_reports does not match reports[] length"
        ));
    }
    for report in reports {
        replay_promoted_fixture_report(report)?;
    }
    Ok(included_reports)
}

fn replay_promoted_fixture_report(report: &Value) -> Result<()> {
    let product_id = json_string(report, "product_id")
        .and_then(parse_hex_selector)
        .ok_or_else(|| anyhow!("fixture report is missing product_id"))?;
    let data_hex = json_string(report, "data_hex")
        .ok_or_else(|| anyhow!("fixture report is missing data_hex"))?;
    let data = parse_hex_bytes(data_hex)
        .map_err(|error| anyhow!("fixture report data_hex is invalid: {error}"))?;
    let report_len = json_u64(report, "report_len")
        .and_then(|len| usize::try_from(len).ok())
        .ok_or_else(|| anyhow!("fixture report is missing report_len"))?;
    if report_len != data.len() {
        return Err(anyhow!(
            "fixture report_len={report_len} does not match decoded data length {}",
            data.len()
        ));
    }
    let report_id = json_string(report, "report_id")
        .ok_or_else(|| anyhow!("fixture report is missing report_id"))?;
    let first_byte = data
        .first()
        .copied()
        .ok_or_else(|| anyhow!("fixture report data must not be empty"))?;
    if report_id != hex_u8(first_byte) {
        return Err(anyhow!(
            "fixture report_id={report_id} does not match first data byte {}",
            hex_u8(first_byte)
        ));
    }

    let protocol = MozaProtocol::new_with_config(product_id, FfbMode::Off, false);
    let parsed = protocol.parse_input_state(&data).ok_or_else(|| {
        anyhow!(
            "Moza parser rejected promoted fixture report for PID {}",
            hex_u16(product_id)
        )
    })?;
    let expected = report
        .get("parsed")
        .ok_or_else(|| anyhow!("fixture report is missing parsed state"))?;
    fixture_parsed_state_matches(expected, &parsed)
}

fn fixture_parsed_state_matches(expected: &Value, actual: &MozaInputState) -> Result<()> {
    fixture_u16_matches(expected, "steering_u16", actual.steering_u16)?;
    fixture_u16_matches(expected, "throttle_u16", actual.throttle_u16)?;
    fixture_u16_matches(expected, "brake_u16", actual.brake_u16)?;
    fixture_u16_matches(expected, "clutch_u16", actual.clutch_u16)?;
    fixture_u16_matches(expected, "handbrake_u16", actual.handbrake_u16)?;
    fixture_u8_matches(expected, "hat", actual.hat)?;
    fixture_u8_matches(expected, "funky", actual.funky)?;
    fixture_u32_matches(expected, "tick", actual.tick)?;
    fixture_buttons_match(expected, &actual.buttons)?;
    fixture_rotary_matches(expected, &actual.rotary)?;
    Ok(())
}

fn fixture_u16_matches(expected: &Value, key: &str, actual: u16) -> Result<()> {
    let expected_value =
        json_u64(expected, key).ok_or_else(|| anyhow!("fixture parsed state is missing {key}"))?;
    if expected_value != u64::from(actual) {
        return Err(anyhow!(
            "fixture parsed {key} expected {expected_value}, parser produced {actual}"
        ));
    }
    Ok(())
}

fn fixture_u8_matches(expected: &Value, key: &str, actual: u8) -> Result<()> {
    let expected_value =
        json_u64(expected, key).ok_or_else(|| anyhow!("fixture parsed state is missing {key}"))?;
    if expected_value != u64::from(actual) {
        return Err(anyhow!(
            "fixture parsed {key} expected {expected_value}, parser produced {actual}"
        ));
    }
    Ok(())
}

fn fixture_u32_matches(expected: &Value, key: &str, actual: u32) -> Result<()> {
    let expected_value =
        json_u64(expected, key).ok_or_else(|| anyhow!("fixture parsed state is missing {key}"))?;
    if expected_value != u64::from(actual) {
        return Err(anyhow!(
            "fixture parsed {key} expected {expected_value}, parser produced {actual}"
        ));
    }
    Ok(())
}

fn fixture_buttons_match(expected: &Value, actual: &[u8; 16]) -> Result<()> {
    let expected_buttons = expected
        .get("buttons_hex")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("fixture parsed state is missing buttons_hex[]"))?;
    if expected_buttons.len() != actual.len() {
        return Err(anyhow!(
            "fixture parsed buttons_hex length {} does not match parser button length {}",
            expected_buttons.len(),
            actual.len()
        ));
    }
    for (index, expected_button) in expected_buttons.iter().enumerate() {
        let expected_hex = expected_button
            .as_str()
            .ok_or_else(|| anyhow!("fixture parsed buttons_hex[{index}] must be a string"))?;
        let actual_hex = hex_u8(actual[index]);
        if expected_hex != actual_hex {
            return Err(anyhow!(
                "fixture parsed buttons_hex[{index}] expected {expected_hex}, parser produced {actual_hex}"
            ));
        }
    }
    Ok(())
}

fn fixture_rotary_matches(expected: &Value, actual: &[u8; 2]) -> Result<()> {
    let expected_rotary = expected
        .get("rotary")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("fixture parsed state is missing rotary[]"))?;
    if expected_rotary.len() != actual.len() {
        return Err(anyhow!(
            "fixture parsed rotary length {} does not match parser rotary length {}",
            expected_rotary.len(),
            actual.len()
        ));
    }
    for (index, expected_value) in expected_rotary.iter().enumerate() {
        let expected_value = expected_value
            .as_u64()
            .ok_or_else(|| anyhow!("fixture parsed rotary[{index}] must be an integer"))?;
        if expected_value != u64::from(actual[index]) {
            return Err(anyhow!(
                "fixture parsed rotary[{index}] expected {expected_value}, parser produced {}",
                actual[index]
            ));
        }
    }
    Ok(())
}

fn path_string_ends_with(path: &str, suffix: &str) -> bool {
    let normalized_path = path.replace('\\', "/");
    let normalized_suffix = suffix.replace('\\', "/");
    normalized_path.ends_with(&normalized_suffix)
}

fn receipt_targets_r5_output_device(receipt: &Value) -> bool {
    let device = receipt.get("device");
    let r5_device = device.map(is_r5_device_value).unwrap_or(false);
    let output_capable =
        device.and_then(|device| json_bool(device, "output_capable")) == Some(true);
    r5_device && output_capable
}

fn receipt_r5_device_pid(receipt: &Value) -> Option<u16> {
    let device = receipt.get("device")?;
    if !is_r5_device_value(device) {
        return None;
    }
    json_string(device, "product_id").and_then(parse_hex_selector)
}

fn verify_zero_torque_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "zero-torque-proof.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("zero_torque_real_hardware", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza zero");
    let receipt_path_ok = receipt_path_matches(lane, &receipt, "zero-torque-proof.json");
    let generated_at_valid = json_string(&receipt, "generated_at_utc")
        .map(|value| utc_timestamp_pair_is_ordered(value, value))
        .unwrap_or(false);
    let dry_run = json_bool(&receipt, "dry_run");
    let no_high_torque = json_bool(&receipt, "no_high_torque") == Some(true);
    let no_nonzero_torque = json_bool(&receipt, "no_nonzero_torque") == Some(true);
    let report_id = json_string(&receipt, "report_id");
    let torque_raw = json_i64(&receipt, "torque_raw");
    let flags = json_u64(&receipt, "flags");
    let motor_enabled = json_bool(&receipt, "motor_enabled");
    let final_zero_sent = json_bool(&receipt, "final_zero_sent");
    let no_feature_reports = json_bool(&receipt, "no_feature_reports") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let repeat = json_u64(&receipt, "repeat").unwrap_or(0);
    let hz = json_u64(&receipt, "hz").unwrap_or(0);
    let write_attempts = json_u64(&receipt, "write_attempts").unwrap_or(0);
    let writes_ok = json_u64(&receipt, "writes_ok").unwrap_or(0);
    let write_errors = json_u64(&receipt, "write_errors").unwrap_or(u64::MAX);
    let watchdog_faults = json_u64(&receipt, "watchdog_faults").unwrap_or(u64::MAX);
    let command_log_safe = zero_command_log_is_safe(&receipt, repeat);
    let writes_ok_exact = repeat.checked_add(1) == Some(writes_ok);
    let r5_output_device = receipt_targets_r5_output_device(&receipt);

    let safe = success
        && command_ok
        && receipt_path_ok
        && generated_at_valid
        && dry_run == Some(false)
        && no_hid_device_opened == Some(false)
        && no_feature_reports
        && no_out_of_scope
        && no_high_torque
        && no_nonzero_torque
        && report_id == Some(DIRECT_TORQUE_REPORT_ID)
        && torque_raw == Some(0)
        && flags == Some(0)
        && motor_enabled == Some(false)
        && final_zero_sent == Some(true)
        && repeat >= 100
        && hz > 0
        && hz <= 1000
        && write_attempts == repeat
        && writes_ok_exact
        && write_errors == 0
        && watchdog_faults == 0
        && command_log_safe
        && r5_output_device;

    if safe {
        BundleGateCheck::pass(
            "zero_torque_real_hardware",
            format!(
                "real zero-torque proof logged {repeat} scheduled zero write(s) plus final zero"
            ),
        )
    } else {
        BundleGateCheck::fail(
            "zero_torque_real_hardware",
            format!(
                "success={success}, command_ok={command_ok}, receipt_path_ok={receipt_path_ok}, generated_at_valid={generated_at_valid}, dry_run={dry_run:?}, no_hid_device_opened={no_hid_device_opened:?}, no_feature_reports={no_feature_reports}, no_out_of_scope={no_out_of_scope}, no_high_torque={no_high_torque}, no_nonzero_torque={no_nonzero_torque}, report_id={report_id:?}, torque_raw={torque_raw:?}, flags={flags:?}, motor_enabled={motor_enabled:?}, final_zero_sent={final_zero_sent:?}, repeat={repeat}, hz={hz}, write_attempts={write_attempts}, writes_ok={writes_ok}, writes_ok_exact={writes_ok_exact}, write_errors={write_errors}, watchdog_faults={watchdog_faults}, command_log_safe={command_log_safe}, r5_output_device={r5_output_device}"
            ),
        )
    }
}

fn record_sequence_matches_index(record: &Value, index: usize) -> bool {
    match u64::try_from(index) {
        Ok(expected) => json_u64(record, "sequence") == Some(expected),
        Err(_) => false,
    }
}

fn zero_command_log_is_safe(receipt: &Value, repeat: u64) -> bool {
    let Some(records) = receipt.get("command_log").and_then(Value::as_array) else {
        return false;
    };

    let Ok(expected_len) = usize::try_from(repeat.saturating_add(1)) else {
        return false;
    };
    if records.len() != expected_len {
        return false;
    }

    let mut scheduled_ok = 0u64;
    let mut final_zero_ok = false;
    for (index, record) in records.iter().enumerate() {
        let safe_zero = json_string(record, "payload_hex") == Some("2000000000000000")
            && json_string(record, "report_id") == Some(DIRECT_TORQUE_REPORT_ID)
            && json_i64(record, "torque_raw") == Some(0)
            && json_u64(record, "flags") == Some(0)
            && json_bool(record, "motor_enabled") == Some(false)
            && json_string(record, "result") == Some("ok")
            && json_u64(record, "bytes_written") == Some(REPORT_LEN as u64)
            && record_sequence_matches_index(record, index);
        if !safe_zero {
            return false;
        }

        match json_string(record, "kind") {
            Some("scheduled_zero") => {
                if index + 1 == records.len() || final_zero_ok {
                    return false;
                }
                scheduled_ok += 1;
            }
            Some("final_zero") => {
                if index + 1 != records.len() || final_zero_ok {
                    return false;
                }
                final_zero_ok = true;
            }
            _ => return false,
        }
    }

    scheduled_ok == repeat && final_zero_ok
}

fn zero_payload_record_is_safe(record: &Value) -> bool {
    json_string(record, "payload_hex") == Some("2000000000000000")
        && json_string(record, "report_id") == Some(DIRECT_TORQUE_REPORT_ID)
        && json_i64(record, "torque_raw") == Some(0)
        && json_u64(record, "flags") == Some(0)
        && json_bool(record, "motor_enabled") == Some(false)
}

fn verify_watchdog_proof_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "watchdog-proof.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("watchdog_zero_output", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza watchdog-proof");
    let receipt_path_ok = receipt_path_matches(lane, &receipt, "watchdog-proof.json");
    let generated_at_valid = json_string(&receipt, "generated_at_utc")
        .map(|value| utc_timestamp_pair_is_ordered(value, value))
        .unwrap_or(false);
    let dry_run = json_bool(&receipt, "dry_run");
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let no_feature_reports = json_bool(&receipt, "no_feature_reports");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_high_torque = json_bool(&receipt, "no_high_torque");
    let no_nonzero_torque = json_bool(&receipt, "no_nonzero_torque");
    let watchdog_faults = json_u64(&receipt, "watchdog_faults").unwrap_or(0);
    let watchdog_triggered = json_bool(&receipt, "watchdog_triggered");
    let final_zero_attempted = json_bool(&receipt, "final_zero_attempted");
    let final_zero_sent = json_bool(&receipt, "final_zero_sent");
    let write_attempts = json_u64(&receipt, "write_attempts").unwrap_or(0);
    let writes_ok = json_u64(&receipt, "writes_ok").unwrap_or(0);
    let write_errors = json_u64(&receipt, "write_errors").unwrap_or(u64::MAX);
    let repeat = json_u64(&receipt, "repeat").unwrap_or(0);
    let hz = json_u64(&receipt, "hz").unwrap_or(0);
    let watchdog_timeout_ms = json_u64(&receipt, "watchdog_timeout_ms").unwrap_or(0);
    let command_log_safe = watchdog_command_log_is_safe(&receipt, repeat);
    let writes_ok_exact = repeat.checked_add(1) == Some(writes_ok);
    let r5_output_device = receipt_targets_r5_output_device(&receipt);

    let safe = success
        && command_ok
        && receipt_path_ok
        && generated_at_valid
        && dry_run == Some(false)
        && no_hid_device_opened == Some(false)
        && no_feature_reports == Some(true)
        && no_out_of_scope
        && no_high_torque == Some(true)
        && no_nonzero_torque == Some(true)
        && watchdog_faults == 1
        && watchdog_triggered == Some(true)
        && final_zero_attempted == Some(true)
        && final_zero_sent == Some(true)
        && write_attempts == repeat
        && writes_ok_exact
        && write_errors == 0
        && repeat > 0
        && hz > 0
        && hz <= 1000
        && watchdog_timeout_ms > 0
        && command_log_safe
        && r5_output_device;

    if safe {
        BundleGateCheck::pass(
            "watchdog_zero_output",
            format!(
                "watchdog proof injected timeout after {repeat} zero write(s) and sent final zero"
            ),
        )
    } else {
        BundleGateCheck::fail(
            "watchdog_zero_output",
            format!(
                "success={success}, command_ok={command_ok}, receipt_path_ok={receipt_path_ok}, generated_at_valid={generated_at_valid}, dry_run={dry_run:?}, no_hid_device_opened={no_hid_device_opened:?}, no_feature_reports={no_feature_reports:?}, no_out_of_scope={no_out_of_scope}, no_high_torque={no_high_torque:?}, no_nonzero_torque={no_nonzero_torque:?}, watchdog_faults={watchdog_faults}, watchdog_triggered={watchdog_triggered:?}, final_zero_attempted={final_zero_attempted:?}, final_zero_sent={final_zero_sent:?}, write_attempts={write_attempts}, writes_ok={writes_ok}, writes_ok_exact={writes_ok_exact}, write_errors={write_errors}, repeat={repeat}, hz={hz}, watchdog_timeout_ms={watchdog_timeout_ms}, command_log_safe={command_log_safe}, r5_output_device={r5_output_device}"
            ),
        )
    }
}

fn watchdog_command_log_is_safe(receipt: &Value, repeat: u64) -> bool {
    let Some(records) = receipt.get("command_log").and_then(Value::as_array) else {
        return false;
    };

    if records.len() != repeat.saturating_add(1) as usize {
        return false;
    }

    let mut scheduled_ok = 0u64;
    let mut final_zero_ok = false;
    for (index, record) in records.iter().enumerate() {
        if !zero_payload_record_is_safe(record)
            || json_string(record, "result") != Some("ok")
            || json_u64(record, "bytes_written") != Some(REPORT_LEN as u64)
            || !record_sequence_matches_index(record, index)
        {
            return false;
        }
        match json_string(record, "kind") {
            Some("scheduled_zero") => {
                if index + 1 == records.len() || final_zero_ok {
                    return false;
                }
                scheduled_ok += 1;
            }
            Some("final_zero") => {
                if index + 1 != records.len() || final_zero_ok {
                    return false;
                }
                final_zero_ok = true;
            }
            _ => return false,
        }
    }

    scheduled_ok == repeat && final_zero_ok
}

fn verify_disconnect_proof_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "disconnect-proof.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("disconnect_final_zero", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza disconnect-proof");
    let receipt_path_ok = receipt_path_matches(lane, &receipt, "disconnect-proof.json");
    let generated_at_valid = json_string(&receipt, "generated_at_utc")
        .map(|value| utc_timestamp_pair_is_ordered(value, value))
        .unwrap_or(false);
    let dry_run = json_bool(&receipt, "dry_run");
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let operator_confirmed = json_bool(&receipt, "operator_confirmed");
    let no_feature_reports = json_bool(&receipt, "no_feature_reports");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_high_torque = json_bool(&receipt, "no_high_torque");
    let no_nonzero_torque = json_bool(&receipt, "no_nonzero_torque");
    let disconnect_observed = json_bool(&receipt, "disconnect_observed");
    let final_zero_attempted = json_bool(&receipt, "final_zero_attempted");
    let final_zero_sent = json_bool(&receipt, "final_zero_sent");
    let write_errors = json_u64(&receipt, "write_errors").unwrap_or(0);
    let write_attempts = json_u64(&receipt, "write_attempts").unwrap_or(0);
    let writes_ok = json_u64(&receipt, "writes_ok").unwrap_or(0);
    let hz = json_u64(&receipt, "hz").unwrap_or(0);
    let max_duration_ms = json_u64(&receipt, "max_duration_ms").unwrap_or(0);
    let command_log_summary = disconnect_command_log_summary(&receipt);
    let command_log_safe = command_log_summary.is_some();
    let scheduled_zero_writes = command_log_summary
        .as_ref()
        .map(|summary| summary.scheduled_zero_writes)
        .unwrap_or(0);
    let final_zero_log_sent = command_log_summary
        .as_ref()
        .map(|summary| summary.final_zero_sent)
        .unwrap_or(false);
    let final_zero_log_error = command_log_summary
        .as_ref()
        .map(|summary| summary.final_zero_error)
        .unwrap_or(false);
    let expected_write_attempts = scheduled_zero_writes.saturating_add(1);
    let expected_writes_ok = scheduled_zero_writes + u64::from(final_zero_log_sent);
    let expected_write_errors = 1 + u64::from(final_zero_log_error);
    let write_attempts_match = write_attempts == expected_write_attempts;
    let writes_ok_match = writes_ok == expected_writes_ok;
    let write_errors_match = write_errors == expected_write_errors;
    let final_zero_sent_match = final_zero_sent == Some(final_zero_log_sent);
    let r5_output_device = receipt_targets_r5_output_device(&receipt);

    let safe = success
        && command_ok
        && receipt_path_ok
        && generated_at_valid
        && dry_run == Some(false)
        && no_hid_device_opened == Some(false)
        && operator_confirmed == Some(true)
        && no_feature_reports == Some(true)
        && no_out_of_scope
        && no_high_torque == Some(true)
        && no_nonzero_torque == Some(true)
        && disconnect_observed == Some(true)
        && final_zero_attempted == Some(true)
        && final_zero_sent_match
        && write_attempts > 0
        && write_attempts_match
        && writes_ok_match
        && write_errors_match
        && hz > 0
        && hz <= 1000
        && max_duration_ms > 0
        && command_log_safe
        && r5_output_device;

    if safe {
        BundleGateCheck::pass(
            "disconnect_final_zero",
            "disconnect proof observed HID write failure and attempted final zero with zero-only payloads".to_string(),
        )
    } else {
        BundleGateCheck::fail(
            "disconnect_final_zero",
            format!(
                "success={success}, command_ok={command_ok}, receipt_path_ok={receipt_path_ok}, generated_at_valid={generated_at_valid}, dry_run={dry_run:?}, no_hid_device_opened={no_hid_device_opened:?}, operator_confirmed={operator_confirmed:?}, no_feature_reports={no_feature_reports:?}, no_out_of_scope={no_out_of_scope}, no_high_torque={no_high_torque:?}, no_nonzero_torque={no_nonzero_torque:?}, disconnect_observed={disconnect_observed:?}, final_zero_attempted={final_zero_attempted:?}, final_zero_sent={final_zero_sent:?}, final_zero_log_sent={final_zero_log_sent}, final_zero_log_error={final_zero_log_error}, final_zero_sent_match={final_zero_sent_match}, write_errors={write_errors}, expected_write_errors={expected_write_errors}, write_errors_match={write_errors_match}, write_attempts={write_attempts}, expected_write_attempts={expected_write_attempts}, write_attempts_match={write_attempts_match}, writes_ok={writes_ok}, expected_writes_ok={expected_writes_ok}, writes_ok_match={writes_ok_match}, hz={hz}, max_duration_ms={max_duration_ms}, command_log_safe={command_log_safe}, r5_output_device={r5_output_device}"
            ),
        )
    }
}

struct DisconnectCommandLogSummary {
    scheduled_zero_writes: u64,
    final_zero_sent: bool,
    final_zero_error: bool,
}

fn disconnect_command_log_summary(receipt: &Value) -> Option<DisconnectCommandLogSummary> {
    let records = receipt.get("command_log").and_then(Value::as_array)?;
    if records.len() < 2 {
        return None;
    }

    let mut disconnect_error_seen = false;
    let mut final_zero_seen = false;
    let mut final_zero_sent = false;
    let mut final_zero_error = false;
    let mut scheduled_zero_writes = 0u64;
    for (index, record) in records.iter().enumerate() {
        if !zero_payload_record_is_safe(record) {
            return None;
        }
        if !record_sequence_matches_index(record, index) {
            return None;
        }
        match json_string(record, "kind") {
            Some("scheduled_zero") => {
                if json_string(record, "result") != Some("ok")
                    || json_u64(record, "bytes_written") != Some(REPORT_LEN as u64)
                    || disconnect_error_seen
                    || final_zero_seen
                {
                    return None;
                }
                scheduled_zero_writes = scheduled_zero_writes.saturating_add(1);
            }
            Some("disconnect_probe") => {
                if json_string(record, "result") != Some("error")
                    || record.get("error").and_then(Value::as_str).is_none()
                    || disconnect_error_seen
                {
                    return None;
                }
                disconnect_error_seen = true;
            }
            Some("final_zero") => {
                if index + 1 != records.len() || !disconnect_error_seen {
                    return None;
                }
                if final_zero_seen {
                    return None;
                }
                match json_string(record, "result") {
                    Some("ok") => {
                        if json_u64(record, "bytes_written") != Some(REPORT_LEN as u64) {
                            return None;
                        }
                        final_zero_sent = true;
                    }
                    Some("error") => {
                        record.get("error").and_then(Value::as_str)?;
                        final_zero_error = true;
                    }
                    _ => return None,
                }
                final_zero_seen = true;
            }
            _ => return None,
        }
    }

    if disconnect_error_seen && final_zero_seen {
        Some(DisconnectCommandLogSummary {
            scheduled_zero_writes,
            final_zero_sent,
            final_zero_error,
        })
    } else {
        None
    }
}

fn verify_init_receipt_gate(
    lane: &Path,
    name: &'static str,
    relative_path: &str,
    expected_mode: &'static str,
) -> BundleGateCheck {
    let receipt = match read_json_value(lane, relative_path) {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail(name, e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza init");
    let receipt_path_ok = receipt_path_matches(lane, &receipt, relative_path);
    let generated_at_valid = json_string(&receipt, "generated_at_utc")
        .map(|value| utc_timestamp_pair_is_ordered(value, value))
        .unwrap_or(false);
    let dry_run = json_bool(&receipt, "dry_run");
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let no_output_reports = json_bool(&receipt, "no_output_reports");
    let no_direct_torque_reports = json_bool(&receipt, "no_direct_torque_reports");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_high_torque = json_bool(&receipt, "no_high_torque");
    let high_torque = json_bool(&receipt, "high_torque");
    let mode = json_string(&receipt, "mode");
    let init_state = json_string(&receipt, "init_state");
    let ready = json_bool(&receipt, "ready");
    let feature_write_errors = json_u64(&receipt, "feature_write_errors").unwrap_or(u64::MAX);
    let output_report_attempts = json_u64(&receipt, "output_report_attempts").unwrap_or(u64::MAX);
    let feature_reports_safe = receipt
        .get("feature_reports")
        .map(|reports| init_feature_reports_are_safe_value(reports, expected_mode, false))
        .unwrap_or(false);
    let r5_output_device = receipt_targets_r5_output_device(&receipt);

    let safe = success
        && command_ok
        && receipt_path_ok
        && generated_at_valid
        && dry_run == Some(false)
        && no_hid_device_opened == Some(false)
        && no_output_reports == Some(true)
        && no_direct_torque_reports == Some(true)
        && no_out_of_scope
        && no_high_torque == Some(true)
        && high_torque == Some(false)
        && mode == Some(expected_mode)
        && init_state == Some("ready")
        && ready == Some(true)
        && feature_write_errors == 0
        && output_report_attempts == 0
        && feature_reports_safe
        && r5_output_device;

    if safe {
        BundleGateCheck::pass(
            name,
            format!(
                "{relative_path} sent staged {expected_mode} handshake without high torque or output reports"
            ),
        )
    } else {
        BundleGateCheck::fail(
            name,
            format!(
                "success={success}, command_ok={command_ok}, receipt_path_ok={receipt_path_ok}, generated_at_valid={generated_at_valid}, dry_run={dry_run:?}, no_hid_device_opened={no_hid_device_opened:?}, no_output_reports={no_output_reports:?}, no_direct_torque_reports={no_direct_torque_reports:?}, no_out_of_scope={no_out_of_scope}, no_high_torque={no_high_torque:?}, high_torque={high_torque:?}, mode={mode:?}, init_state={init_state:?}, ready={ready:?}, feature_write_errors={feature_write_errors}, output_report_attempts={output_report_attempts}, feature_reports_safe={feature_reports_safe}, r5_output_device={r5_output_device}"
            ),
        )
    }
}

fn init_feature_reports_are_safe_value(
    reports: &Value,
    expected_mode: &str,
    allow_planned: bool,
) -> bool {
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

    init_feature_report_record_is_safe(
        &records[0],
        0,
        "start_input_reports",
        START_REPORTING_FEATURE_REPORT_ID,
        "03000000",
        allow_planned,
    ) && init_feature_report_record_is_safe(
        &records[1],
        1,
        "ffb_mode",
        FFB_MODE_FEATURE_REPORT_ID,
        expected_mode_payload,
        allow_planned,
    )
}

fn init_feature_report_record_is_safe(
    record: &Value,
    sequence: u64,
    kind: &str,
    report_id: &str,
    payload_hex: &str,
    allow_planned: bool,
) -> bool {
    let result = json_string(record, "result");
    let result_ok = match result {
        Some("ok") => {
            json_u64(record, "bytes_written") == u64::try_from(payload_hex.len() / 2).ok()
        }
        Some("planned") if allow_planned => true,
        _ => false,
    };

    json_u64(record, "sequence") == Some(sequence)
        && json_string(record, "kind") == Some(kind)
        && json_string(record, "report_id") == Some(report_id)
        && json_string(record, "payload_hex") == Some(payload_hex)
        && result_ok
}

#[cfg(test)]
fn verify_service_status_gate(lane: &Path) -> BundleGateCheck {
    verify_service_status_gate_with_support_validation(lane, SupportBundleValidationMode::Fresh)
}

fn verify_service_status_gate_with_support_validation(
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> BundleGateCheck {
    let moza_status = match read_json_value(lane, "moza-status.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("service_status_receipts", e.to_string()),
    };
    let device_status = match read_json_value(lane, "device-status.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("service_status_receipts", e.to_string()),
    };
    let support_bundle = match read_json_value(lane, "support-bundle.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("service_status_receipts", e.to_string()),
    };

    let moza = moza_status_receipt_summary(&moza_status, lane);
    let device = device_status_receipt_summary(&device_status, lane);
    let support =
        support_bundle_receipt_summary_with_validation(&support_bundle, lane, support_validation);
    let same_pid = matching_service_receipt_pids(&[
        moza.product_id.as_deref(),
        device.product_id.as_deref(),
        support.product_id.as_deref(),
    ]);
    let safe = moza.ok && device.ok && support.ok && same_pid;

    if safe {
        BundleGateCheck::pass(
            "service_status_receipts",
            format!(
                "moza-status.json, device-status.json, and support-bundle.json all report the same observe-only R5 service lane PID {:?}",
                device.product_id
            ),
        )
    } else {
        BundleGateCheck::fail(
            "service_status_receipts",
            format!(
                "moza_status=({}), device_status=({}), support_bundle=({}), same_pid={same_pid}",
                moza.details, device.details, support.details
            ),
        )
    }
}

#[derive(Debug)]
struct ServiceReceiptSummary {
    ok: bool,
    product_id: Option<String>,
    details: String,
}

fn moza_status_receipt_summary(receipt: &Value, lane: &Path) -> ServiceReceiptSummary {
    let success = json_bool(receipt, "success") == Some(true);
    let command_ok = json_string(receipt, "command") == Some("wheelctl moza status");
    let lane_ok = path_value_matches(lane, json_string(receipt, "lane"));
    let no_hid_device_opened = json_bool(receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(receipt);
    let device_count = json_u64(receipt, "device_count").unwrap_or(0);
    let r5_entry = receipt
        .get("devices")
        .and_then(Value::as_array)
        .and_then(|devices| {
            devices
                .iter()
                .find(|entry| moza_status_device_is_observe_only(entry))
        });
    let product_id = r5_entry.and_then(|entry| {
        entry
            .get("device")
            .and_then(|device| json_string(device, "product_id"))
            .map(str::to_string)
    });
    let r5_observe_only = r5_entry.is_some();
    let ok = success
        && command_ok
        && lane_ok
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope
        && device_count > 0
        && r5_observe_only;

    ServiceReceiptSummary {
        ok,
        product_id,
        details: format!(
            "success={success}, command_ok={command_ok}, lane_ok={lane_ok}, no_hid_device_opened={no_hid_device_opened}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}, device_count={device_count}, r5_observe_only={r5_observe_only}"
        ),
    }
}

fn moza_status_device_is_observe_only(entry: &Value) -> bool {
    let Some(device) = entry.get("device") else {
        return false;
    };

    is_r5_device_value(device)
        && json_bool(device, "output_capable") == Some(true)
        && json_bool(entry, "ffb_ready") == Some(false)
        && json_string(entry, "init_state") == Some("uninitialized")
        && json_bool(entry, "direct_mode_allowed") == Some(false)
        && json_bool(entry, "high_torque_allowed") == Some(false)
        && json_bool(entry, "safe_to_send_torque") == Some(false)
}

fn device_status_receipt_summary(receipt: &Value, lane: &Path) -> ServiceReceiptSummary {
    let success = json_bool(receipt, "success") == Some(true);
    let command_ok = json_string(receipt, "command") == Some("wheelctl device status");
    let no_hid_device_opened = json_bool(receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(receipt);
    let moza_lane_ok = path_value_matches(lane, json_string(receipt, "moza_lane"));
    let status = receipt.get("status");
    let observe_only = status
        .map(|status| device_status_value_is_moza_observe_only(status, lane))
        .unwrap_or(false);
    let product_id = status
        .and_then(|status| status.get("device"))
        .and_then(|device| json_string(device, "product_id"))
        .map(str::to_string);
    let ok = success
        && command_ok
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope
        && moza_lane_ok
        && observe_only;

    ServiceReceiptSummary {
        ok,
        product_id,
        details: format!(
            "success={success}, command_ok={command_ok}, no_hid_device_opened={no_hid_device_opened}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}, moza_lane_ok={moza_lane_ok}, observe_only={observe_only}"
        ),
    }
}

fn support_bundle_receipt_summary_with_validation(
    receipt: &Value,
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> ServiceReceiptSummary {
    let success = json_bool(receipt, "success") == Some(true);
    let command_ok = json_string(receipt, "command") == Some("wheelctl support-bundle");
    let no_hid_device_opened = json_bool(receipt, "no_hid_device_opened") == Some(true);
    let no_ffb_writes = json_bool(receipt, "no_ffb_writes") == Some(true);
    let no_out_of_scope = no_out_of_scope_device_commands(receipt);
    let device_filter_present = json_string(receipt, "device_filter")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let moza_lane_status = receipt.get("moza_lane");
    let moza_lane_lane_ok = moza_lane_status
        .map(|status| path_value_matches(lane, json_string(status, "lane")))
        .unwrap_or(false);
    let moza_lane_readiness_ok = moza_lane_status
        .and_then(|status| status.get("readiness"))
        .map(support_readiness_is_diagnostic)
        .unwrap_or(false);
    let moza_lane_artifact_index_ok = moza_lane_status
        .and_then(|status| status.get("artifact_index"))
        .and_then(Value::as_array)
        .map(|artifacts| support_artifact_index_is_diagnostic(artifacts))
        .unwrap_or(false);
    let moza_lane_notes_ok = moza_lane_status
        .map(support_bundle_moza_lane_notes_are_diagnostic)
        .unwrap_or(false);
    let moza_lane_status_ok = moza_lane_status
        .map(|status| {
            support_bundle_moza_lane_status_is_diagnostic(status, lane, support_validation)
        })
        .unwrap_or(false);
    let device_status = receipt
        .get("device_statuses")
        .and_then(Value::as_array)
        .and_then(|statuses| {
            statuses.iter().find_map(|entry| {
                if json_string(entry, "status") != Some("ok") {
                    return None;
                }
                let status = entry.get("device_status")?;
                device_status_value_is_moza_observe_only(status, lane).then_some(status)
            })
        });
    let product_id = device_status
        .and_then(|status| status.get("device"))
        .and_then(|device| json_string(device, "product_id"))
        .map(str::to_string);
    let observe_only = device_status.is_some();
    let top_level_r5_product_ids = support_bundle_top_level_r5_product_ids(receipt);
    let top_level_r5_present = !top_level_r5_product_ids.is_empty();
    let top_level_pid_matches_status = product_id
        .as_deref()
        .map(|pid| {
            top_level_r5_product_ids
                .iter()
                .all(|top_level| top_level == pid)
        })
        .unwrap_or(false);
    let ok = success
        && command_ok
        && no_hid_device_opened
        && no_ffb_writes
        && no_out_of_scope
        && device_filter_present
        && moza_lane_status_ok
        && observe_only
        && top_level_r5_present
        && top_level_pid_matches_status;

    ServiceReceiptSummary {
        ok,
        product_id,
        details: format!(
            "success={success}, command_ok={command_ok}, no_hid_device_opened={no_hid_device_opened}, no_ffb_writes={no_ffb_writes}, no_out_of_scope={no_out_of_scope}, device_filter_present={device_filter_present}, moza_lane_status_ok={moza_lane_status_ok}, moza_lane_lane_ok={moza_lane_lane_ok}, moza_lane_readiness_ok={moza_lane_readiness_ok}, moza_lane_artifact_index_ok={moza_lane_artifact_index_ok}, moza_lane_notes_ok={moza_lane_notes_ok}, observe_only={observe_only}, top_level_r5_present={top_level_r5_present}, top_level_pid_matches_status={top_level_pid_matches_status}, top_level_r5_product_ids={top_level_r5_product_ids:?}"
        ),
    }
}

fn support_bundle_top_level_r5_product_ids(receipt: &Value) -> Vec<String> {
    receipt
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter_map(|device| {
                    let vendor_id = json_string(device, "vendor_id")?;
                    let product_id = json_string(device, "product_id")?;
                    let pid = parse_hex_selector(product_id)?;
                    (vendor_id == MOZA_VENDOR_HEX
                        && [product_ids::R5_V1, product_ids::R5_V2].contains(&pid))
                    .then(|| hex_u16(pid))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn device_status_value_is_moza_observe_only(status: &Value, lane: &Path) -> bool {
    let Some(device) = status.get("device") else {
        return false;
    };
    let Some(moza) = status.get("moza") else {
        return false;
    };

    let descriptor_crc_present = json_string(moza, "descriptor_crc32")
        .map(|value| value.starts_with("0x") && value.len() == 10)
        .unwrap_or(false);
    let descriptor_source_present = json_string(moza, "descriptor_source")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let lane_ok = path_value_matches(lane, json_string(moza, "lane"));
    let safety_state_present = json_string(moza, "safety_state")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let safety_reason_present = json_string(moza, "safety_reason")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    is_r5_device_value(device)
        && json_bool(moza, "output_capable") == Some(true)
        && json_bool(moza, "ffb_ready") == Some(false)
        && json_string(moza, "init_state") == Some("uninitialized")
        && json_bool(moza, "direct_mode_allowed") == Some(false)
        && json_bool(moza, "high_torque_allowed") == Some(false)
        && json_bool(moza, "safe_to_send_torque") == Some(false)
        && descriptor_crc_present
        && descriptor_source_present
        && lane_ok
        && safety_state_present
        && safety_reason_present
}

fn support_bundle_moza_lane_status_is_diagnostic(
    status: &Value,
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> bool {
    let shape_ok = support_bundle_moza_lane_status_shape_is_diagnostic(status, lane);
    if !shape_ok || matches!(support_validation, SupportBundleValidationMode::ShapeOnly) {
        return shape_ok;
    }

    let expected = support_bundle_status_for_receipt_validation(lane);
    let readiness_current =
        support_readiness_matches_expected(status.get("readiness"), expected.get("readiness"));
    let artifact_index_current = support_artifact_index_matches_expected(
        status.get("artifact_index"),
        expected.get("artifact_index"),
    );

    readiness_current && artifact_index_current
}

fn support_bundle_moza_lane_status_shape_is_diagnostic(status: &Value, lane: &Path) -> bool {
    let lane_ok = path_value_matches(lane, json_string(status, "lane"));
    let readiness_ok = status
        .get("readiness")
        .map(support_readiness_is_diagnostic)
        .unwrap_or(false);
    let artifact_index_ok = status
        .get("artifact_index")
        .and_then(Value::as_array)
        .map(|artifacts| support_artifact_index_is_diagnostic(artifacts))
        .unwrap_or(false);
    let notes_ok = support_bundle_moza_lane_notes_are_diagnostic(status);

    lane_ok && readiness_ok && artifact_index_ok && notes_ok
}

fn support_readiness_matches_expected(actual: Option<&Value>, expected: Option<&Value>) -> bool {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return false;
    };
    let actual_highest = support_readiness_stage_rank(json_string(actual, "highest_passing_stage"));
    let expected_highest =
        support_readiness_stage_rank(json_string(expected, "highest_passing_stage"));
    let stage_does_not_overclaim = matches!(
        (actual_highest, expected_highest),
        (Some(actual), Some(expected)) if actual <= expected
    );
    let next_stage_ok = if actual_highest == expected_highest {
        actual.get("next_required_stage") == expected.get("next_required_stage")
    } else {
        true
    };
    let booleans_do_not_overclaim = [
        "ready_for_zero_torque",
        "ready_for_low_torque",
        "ready_for_real_hardware_smoke",
        "passive_lane_audit_passed",
        "zero_lane_audit_passed",
        "smoke_ready_lane_audit_passed",
    ]
    .iter()
    .all(|field| {
        json_bool(actual, field) != Some(true) || json_bool(expected, field) == Some(true)
    });

    stage_does_not_overclaim
        && next_stage_ok
        && booleans_do_not_overclaim
        && json_bool(actual, "release_ready") == Some(false)
        && json_string(actual, "claim_scope") == Some("diagnostic_context_only")
}

fn support_readiness_stage_rank(stage: Option<&str>) -> Option<u8> {
    match stage {
        Some("none") => Some(0),
        Some("passive") => Some(1),
        Some("zero") => Some(2),
        Some("smoke_ready") => Some(3),
        _ => None,
    }
}

fn support_artifact_index_matches_expected(
    actual: Option<&Value>,
    expected: Option<&Value>,
) -> bool {
    let (Some(actual), Some(expected)) = (
        actual.and_then(Value::as_array),
        expected.and_then(Value::as_array),
    ) else {
        return false;
    };
    actual.len() == expected.len()
        && expected.iter().all(|expected_artifact| {
            let expected_path = json_string(expected_artifact, "path");
            actual
                .iter()
                .find(|artifact| json_string(artifact, "path") == expected_path)
                .map(|artifact| {
                    support_artifact_index_entry_matches_expected(artifact, expected_artifact)
                })
                .unwrap_or(false)
        })
}

fn support_artifact_index_entry_matches_expected(actual: &Value, expected: &Value) -> bool {
    let identity_ok = ["path", "kind", "required_stage"]
        .iter()
        .all(|field| actual.get(*field) == expected.get(*field));
    let exists_does_not_overclaim =
        json_bool(actual, "exists") != Some(true) || json_bool(expected, "exists") == Some(true);
    let valid_does_not_overclaim =
        json_bool(actual, "valid") != Some(true) || json_bool(expected, "valid") == Some(true);
    let status_does_not_overclaim = match json_string(actual, "status") {
        Some("pass") => json_string(expected, "status") == Some("pass"),
        Some("invalid" | "missing") => true,
        _ => false,
    };

    identity_ok
        && exists_does_not_overclaim
        && valid_does_not_overclaim
        && status_does_not_overclaim
}

fn support_bundle_moza_lane_notes_are_diagnostic(status: &Value) -> bool {
    status
        .get("notes")
        .and_then(Value::as_array)
        .map(|notes| {
            notes.iter().any(|note| {
                note.as_str()
                    .map(|note| note.contains("diagnostic context"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn support_readiness_is_diagnostic(readiness: &Value) -> bool {
    let highest = json_string(readiness, "highest_passing_stage");
    let next = readiness.get("next_required_stage");
    let ready_zero = json_bool(readiness, "ready_for_zero_torque");
    let ready_low = json_bool(readiness, "ready_for_low_torque");
    let ready_smoke = json_bool(readiness, "ready_for_real_hardware_smoke");
    let passive_audit = json_bool(readiness, "passive_lane_audit_passed");
    let zero_audit = json_bool(readiness, "zero_lane_audit_passed");
    let smoke_audit = json_bool(readiness, "smoke_ready_lane_audit_passed");
    let first_blocking = readiness.get("first_blocking_stage");
    let highest_ok = matches!(highest, Some("none" | "passive" | "zero" | "smoke_ready"));
    let next_ok = next.map(|value| {
        value.is_null() || matches!(value.as_str(), Some("passive" | "zero" | "smoke_ready"))
    }) == Some(true);
    let first_blocking_ok = first_blocking.is_some();
    let readiness_fields_present =
        ready_zero.is_some() && ready_low.is_some() && ready_smoke.is_some();
    let audit_fields_present =
        passive_audit.is_some() && zero_audit.is_some() && smoke_audit.is_some();
    let readiness_progression_ok = ready_low != Some(true) || ready_zero == Some(true);
    let smoke_progression_ok = ready_smoke != Some(true) || ready_low == Some(true);
    let audit_gate_consistency_ok = (ready_zero != Some(true) || passive_audit == Some(true))
        && (ready_low != Some(true) || zero_audit == Some(true))
        && (ready_smoke != Some(true) || smoke_audit == Some(true));

    highest_ok
        && next_ok
        && first_blocking_ok
        && readiness_fields_present
        && audit_fields_present
        && readiness_progression_ok
        && smoke_progression_ok
        && audit_gate_consistency_ok
        && json_bool(readiness, "release_ready") == Some(false)
        && json_string(readiness, "claim_scope") == Some("diagnostic_context_only")
}

fn support_artifact_index_is_diagnostic(artifacts: &[Value]) -> bool {
    lane_artifact_index_requirements().all(|required| {
        artifacts.iter().any(|artifact| {
            json_string(artifact, "path") == Some(required.relative_path)
                && json_string(artifact, "kind") == Some(required.kind.label())
                && json_string(artifact, "required_stage") == Some(stage_label(required.stage))
                && support_artifact_index_status_is_consistent(artifact)
        })
    })
}

fn support_artifact_index_status_is_consistent(artifact: &Value) -> bool {
    matches!(
        (
            json_string(artifact, "status"),
            json_bool(artifact, "exists"),
            json_bool(artifact, "valid"),
        ),
        (Some("pass"), Some(true), Some(true))
            | (Some("missing"), Some(false), Some(false))
            | (Some("invalid"), Some(true), Some(false))
    )
}

fn matching_service_receipt_pids(pids: &[Option<&str>]) -> bool {
    let mut normalized = pids
        .iter()
        .filter_map(|pid| pid.and_then(canonical_r5_pid))
        .collect::<Vec<_>>();
    if normalized.len() != pids.len() || normalized.is_empty() {
        return false;
    }
    normalized.sort();
    normalized.dedup();
    normalized.len() == 1
}

fn canonical_r5_pid(pid: &str) -> Option<String> {
    let pid = parse_hex_selector(pid)?;
    matches!(pid, product_ids::R5_V1 | product_ids::R5_V2).then(|| hex_u16(pid))
}

fn verify_low_torque_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "low-torque-proof.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("low_torque_bounded", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza torque-test");
    let receipt_path_ok = receipt_path_matches(lane, &receipt, "low-torque-proof.json");
    let dry_run = json_bool(&receipt, "dry_run");
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let confirmed = json_bool(&receipt, "confirmed");
    let zero_proof_validated = json_bool(&receipt, "zero_proof_validated");
    let init_proofs_validated = json_bool(&receipt, "init_proofs_validated");
    let no_feature_reports = json_bool(&receipt, "no_feature_reports");
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let high_torque = json_bool(&receipt, "high_torque");
    let no_high_torque = json_bool(&receipt, "no_high_torque");
    let direct_mode_gate_satisfied = json_bool(&receipt, "direct_mode_gate_satisfied");
    let descriptor_trusted = json_bool(&receipt, "descriptor_trusted");
    let explicit_operator_override = json_bool(&receipt, "explicit_operator_override");
    let no_nonzero_above_limit = json_bool(&receipt, "no_nonzero_above_limit");
    let final_zero_attempted = json_bool(&receipt, "final_zero_attempted");
    let final_zero_sent = json_bool(&receipt, "final_zero_sent");
    let generated_at_utc = json_string(&receipt, "generated_at_utc");
    let generated_at_valid = generated_at_utc
        .map(|value| utc_timestamp_pair_is_ordered(value, value))
        .unwrap_or(false);
    let max_percent = json_f64(&receipt, "max_percent")
        .or_else(|| json_f64(&receipt, "max_output_percent"))
        .or_else(|| json_f64(&receipt, "max_command_percent"));
    let duration_ms = json_u64(&receipt, "duration_ms").unwrap_or(0);
    let hz = json_u64(&receipt, "hz").unwrap_or(0);
    let write_attempts = json_u64(&receipt, "write_attempts").unwrap_or(0);
    let writes_ok = json_u64(&receipt, "writes_ok").unwrap_or(0);
    let write_errors = json_u64(&receipt, "write_errors").unwrap_or(u64::MAX);
    let device = receipt.get("device");
    let r5_device = device.map(is_r5_device_value).unwrap_or(false);
    let output_capable =
        device.and_then(|device| json_bool(device, "output_capable")) == Some(true);
    let device_pid = device.and_then(|device| json_string(device, "product_id"));
    let device_pid_value = device_pid.and_then(parse_hex_selector);
    let zero_proof_pid = receipt
        .get("zero_proof")
        .and_then(|proof| json_string(proof, "product_id"));
    let zero_proof_pid_matches = zero_proof_pid.is_some() && zero_proof_pid == device_pid;
    let zero_proof_lane_match =
        low_torque_zero_proof_matches_lane(lane, &receipt, device_pid, generated_at_utc);
    let descriptor_trust_observed = device_pid
        .map(|pid| lane_descriptor_trusted_for_pid(lane, pid))
        .unwrap_or(false);
    let descriptor_trust_valid = descriptor_trusted != Some(true) || descriptor_trust_observed;
    let direct_mode_allowed = (descriptor_trusted == Some(true) && descriptor_trust_observed)
        || explicit_operator_override == Some(true);
    let init_proofs_match =
        low_torque_init_proofs_match(lane, &receipt, device_pid, generated_at_utc);
    let no_abort_reason = receipt
        .get("abort_reason")
        .map(Value::is_null)
        .unwrap_or(true);
    let no_final_zero_error = receipt
        .get("final_zero_error")
        .map(Value::is_null)
        .unwrap_or(true);

    let bounded = max_percent
        .map(|value| value.is_finite() && value > 0.0 && value <= 2.0)
        .unwrap_or(false);
    let expected_low_torque_writes = max_percent.and_then(|limit| {
        device_pid_value.and_then(|pid| low_torque_ladder_is_safe(&receipt, pid, limit))
    });
    let command_log_safe = max_percent
        .and_then(|limit| {
            device_pid_value.and_then(|pid| {
                expected_low_torque_writes
                    .map(|expected| low_torque_command_log_is_safe(&receipt, pid, limit, expected))
            })
        })
        .unwrap_or(false);
    let expected_writes_match = expected_low_torque_writes
        .map(|expected| write_attempts == expected && writes_ok == expected.saturating_add(1))
        .unwrap_or(false);

    let safe = success
        && command_ok
        && receipt_path_ok
        && dry_run == Some(false)
        && no_hid_device_opened == Some(false)
        && confirmed == Some(true)
        && zero_proof_validated == Some(true)
        && zero_proof_pid_matches
        && zero_proof_lane_match
        && init_proofs_validated == Some(true)
        && init_proofs_match
        && no_feature_reports == Some(true)
        && no_ffb_writes == Some(false)
        && no_out_of_scope
        && high_torque == Some(false)
        && direct_mode_gate_satisfied == Some(true)
        && direct_mode_allowed
        && descriptor_trust_valid
        && no_high_torque == Some(true)
        && no_nonzero_above_limit == Some(true)
        && final_zero_attempted == Some(true)
        && final_zero_sent == Some(true)
        && generated_at_valid
        && bounded
        && duration_ms > 0
        && duration_ms <= 1000
        && hz > 0
        && hz <= 1000
        && write_errors == 0
        && no_abort_reason
        && no_final_zero_error
        && r5_device
        && output_capable
        && expected_writes_match
        && command_log_safe;

    if safe {
        BundleGateCheck::pass(
            "low_torque_bounded",
            format!(
                "real low-torque proof logged {} bounded command(s) plus final zero",
                expected_low_torque_writes.unwrap_or(0)
            ),
        )
    } else {
        BundleGateCheck::fail(
            "low_torque_bounded",
            format!(
                "success={success}, command_ok={command_ok}, receipt_path_ok={receipt_path_ok}, dry_run={dry_run:?}, no_hid_device_opened={no_hid_device_opened:?}, confirmed={confirmed:?}, zero_proof_validated={zero_proof_validated:?}, zero_proof_pid_matches={zero_proof_pid_matches}, zero_proof_lane_match={zero_proof_lane_match}, init_proofs_validated={init_proofs_validated:?}, init_proofs_match={init_proofs_match}, no_feature_reports={no_feature_reports:?}, no_ffb_writes={no_ffb_writes:?}, no_out_of_scope={no_out_of_scope}, high_torque={high_torque:?}, direct_mode_gate_satisfied={direct_mode_gate_satisfied:?}, descriptor_trusted={descriptor_trusted:?}, descriptor_trust_observed={descriptor_trust_observed}, descriptor_trust_valid={descriptor_trust_valid}, explicit_operator_override={explicit_operator_override:?}, direct_mode_allowed={direct_mode_allowed}, no_high_torque={no_high_torque:?}, no_nonzero_above_limit={no_nonzero_above_limit:?}, final_zero_attempted={final_zero_attempted:?}, final_zero_sent={final_zero_sent:?}, generated_at_valid={generated_at_valid}, max_percent={max_percent:?}, duration_ms={duration_ms}, hz={hz}, write_attempts={write_attempts}, writes_ok={writes_ok}, write_errors={write_errors}, r5_device={r5_device}, output_capable={output_capable}, expected_low_torque_writes={expected_low_torque_writes:?}, expected_writes_match={expected_writes_match}, command_log_safe={command_log_safe}, no_abort_reason={no_abort_reason}, no_final_zero_error={no_final_zero_error}"
            ),
        )
    }
}

fn low_torque_zero_proof_matches_lane(
    lane: &Path,
    receipt: &Value,
    device_pid: Option<&str>,
    low_torque_generated_at: Option<&str>,
) -> bool {
    let Some(device_pid) = device_pid else {
        return false;
    };
    let Some(low_torque_generated_at) = low_torque_generated_at else {
        return false;
    };
    let Some(proof) = receipt.get("zero_proof") else {
        return false;
    };
    let proof_path = lane.join("zero-torque-proof.json");
    let Ok(actual_crc32) = receipt_file_crc32(&proof_path) else {
        return false;
    };
    let Ok(actual) = read_json_path(&proof_path) else {
        return false;
    };
    let proof_generated_at = json_string(proof, "generated_at_utc");
    let actual_generated_at = json_string(&actual, "generated_at_utc");

    prerequisite_summary_path_matches(lane, proof, "zero-torque-proof.json")
        && json_string(proof, "product_id") == Some(device_pid)
        && json_u64(proof, "repeat") == json_u64(&actual, "repeat")
        && json_u64(proof, "writes_ok") == json_u64(&actual, "writes_ok")
        && json_bool(proof, "final_zero_sent") == Some(true)
        && proof_generated_at == actual_generated_at
        && proof_generated_at
            .map(|value| utc_timestamp_pair_is_ordered(value, low_torque_generated_at))
            .unwrap_or(false)
        && json_string(proof, "receipt_crc32") == Some(actual_crc32.as_str())
        && verify_zero_torque_gate(lane).status == "pass"
}

fn low_torque_init_proofs_match(
    lane: &Path,
    receipt: &Value,
    device_pid: Option<&str>,
    low_torque_generated_at: Option<&str>,
) -> bool {
    let Some(device_pid) = device_pid else {
        return false;
    };
    let Some(low_torque_generated_at) = low_torque_generated_at else {
        return false;
    };
    let Some(proofs) = receipt.get("init_proofs") else {
        return false;
    };
    init_proof_summary_matches(
        lane,
        proofs.get("off"),
        "off",
        "init-off.json",
        "init_off_handshake",
        device_pid,
        low_torque_generated_at,
    ) && init_proof_summary_matches(
        lane,
        proofs.get("standard"),
        "standard",
        "init-standard.json",
        "init_standard_handshake",
        device_pid,
        low_torque_generated_at,
    )
}

fn init_proof_summary_matches(
    lane: &Path,
    proof: Option<&Value>,
    expected_mode: &'static str,
    expected_relative_path: &'static str,
    expected_gate_name: &'static str,
    device_pid: &str,
    low_torque_generated_at: &str,
) -> bool {
    let Some(proof) = proof else {
        return false;
    };
    let proof_path = lane.join(expected_relative_path);
    let Ok(actual_crc32) = receipt_file_crc32(&proof_path) else {
        return false;
    };
    let Ok(actual) = read_json_path(&proof_path) else {
        return false;
    };
    let proof_generated_at = json_string(proof, "generated_at_utc");
    let actual_generated_at = json_string(&actual, "generated_at_utc");

    prerequisite_summary_path_matches(lane, proof, expected_relative_path)
        && json_string(proof, "product_id") == Some(device_pid)
        && json_string(proof, "mode") == Some(expected_mode)
        && json_string(proof, "init_state") == Some("ready")
        && json_bool(proof, "ready") == Some(true)
        && json_u64(proof, "feature_report_count") == Some(2)
        && proof_generated_at == actual_generated_at
        && proof_generated_at
            .map(|value| utc_timestamp_pair_is_ordered(value, low_torque_generated_at))
            .unwrap_or(false)
        && json_string(proof, "receipt_crc32") == Some(actual_crc32.as_str())
        && verify_init_receipt_gate(
            lane,
            expected_gate_name,
            expected_relative_path,
            expected_mode,
        )
        .status
            == "pass"
}

fn prerequisite_summary_path_matches(
    lane: &Path,
    proof: &Value,
    expected_relative_path: &str,
) -> bool {
    match json_string(proof, "path") {
        Some(path) if path.trim() == expected_relative_path => true,
        Some(path) => path_value_matches(&lane.join(expected_relative_path), Some(path)),
        None => false,
    }
}

fn low_torque_ladder_is_safe(receipt: &Value, pid: u16, max_percent: f64) -> Option<u64> {
    let stages = receipt.get("ladder").and_then(Value::as_array)?;
    if stages.is_empty() {
        return None;
    }

    let mut expected_writes = 0u64;
    for stage in stages {
        let percent = json_f64(stage, "percent")?;
        let write_count = json_u64(stage, "write_count")?;
        let payload_hex = json_string(stage, "payload_hex")?;
        let torque_raw = json_i64(stage, "torque_raw")?;
        let flags = json_u64(stage, "flags")?;
        let motor_enabled = json_bool(stage, "motor_enabled")?;
        let torque_nm = json_f64(stage, "torque_nm")?;
        let expected = low_torque_expected_payload(pid, percent)?;
        let stage_safe = percent.is_finite()
            && percent > 0.0
            && percent <= max_percent
            && write_count > 0
            && payload_hex.len() == REPORT_LEN * 2
            && payload_matches_hex(expected.payload, payload_hex)
            && json_string(stage, "report_id") == Some(DIRECT_TORQUE_REPORT_ID)
            && torque_raw == i64::from(expected.torque_raw)
            && flags == u64::from(expected.flags)
            && motor_enabled == expected.motor_enabled
            && torque_nearly_matches(torque_nm, expected.torque_nm);
        if !stage_safe {
            return None;
        }
        expected_writes = expected_writes.checked_add(write_count)?;
    }

    Some(expected_writes)
}

fn low_torque_command_log_is_safe(
    receipt: &Value,
    pid: u16,
    max_percent: f64,
    expected_writes: u64,
) -> bool {
    let Some(records) = receipt.get("command_log").and_then(Value::as_array) else {
        return false;
    };

    let Ok(expected_len) = usize::try_from(expected_writes.saturating_add(1)) else {
        return false;
    };
    if records.len() != expected_len {
        return false;
    }

    let mut low_torque_ok = 0u64;
    for (index, record) in records.iter().enumerate() {
        if json_string(record, "result") != Some("ok")
            || json_string(record, "report_id") != Some(DIRECT_TORQUE_REPORT_ID)
            || json_u64(record, "bytes_written") != Some(REPORT_LEN as u64)
            || !record_sequence_matches_index(record, index)
        {
            return false;
        }

        match json_string(record, "kind") {
            Some("low_torque") => {
                if index + 1 == records.len() {
                    return false;
                }
                let percent = match json_f64(record, "percent") {
                    Some(value) => value,
                    None => return false,
                };
                let torque_raw = match json_i64(record, "torque_raw") {
                    Some(value) => value,
                    None => return false,
                };
                let flags = match json_u64(record, "flags") {
                    Some(value) => value,
                    None => return false,
                };
                let payload_hex = match json_string(record, "payload_hex") {
                    Some(value) => value,
                    None => return false,
                };
                let Some(expected) = low_torque_expected_payload(pid, percent) else {
                    return false;
                };
                let record_safe = percent.is_finite()
                    && percent > 0.0
                    && percent <= max_percent
                    && payload_hex.len() == REPORT_LEN * 2
                    && payload_matches_hex(expected.payload, payload_hex)
                    && torque_raw == i64::from(expected.torque_raw)
                    && flags == u64::from(expected.flags)
                    && json_bool(record, "motor_enabled") == Some(expected.motor_enabled);
                if !record_safe {
                    return false;
                }
                low_torque_ok += 1;
            }
            Some("final_zero") => {
                if index + 1 != records.len() {
                    return false;
                }
                let final_zero_safe = json_string(record, "payload_hex")
                    == Some("2000000000000000")
                    && json_f64(record, "percent") == Some(0.0)
                    && json_i64(record, "torque_raw") == Some(0)
                    && json_u64(record, "flags") == Some(0)
                    && json_bool(record, "motor_enabled") == Some(false);
                if !final_zero_safe {
                    return false;
                }
            }
            _ => return false,
        }
    }

    low_torque_ok == expected_writes
}

struct ExpectedLowTorquePayload {
    payload: [u8; REPORT_LEN],
    torque_nm: f32,
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
}

fn low_torque_expected_payload(pid: u16, percent: f64) -> Option<ExpectedLowTorquePayload> {
    if !percent.is_finite() || percent <= 0.0 || percent > 2.0 {
        return None;
    }
    let (payload, torque_nm) = low_torque_payload_for_pid_percent(pid, percent as f32);
    let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
    let flags = payload[3];
    Some(ExpectedLowTorquePayload {
        payload,
        torque_nm,
        torque_raw,
        flags,
        motor_enabled: flags & 0x01 != 0,
    })
}

fn payload_matches_hex(payload: [u8; REPORT_LEN], payload_hex: &str) -> bool {
    payload_hex.eq_ignore_ascii_case(&bytes_hex_compact(&payload))
}

fn torque_nearly_matches(actual: f64, expected: f32) -> bool {
    (actual - f64::from(expected)).abs() <= 0.000_1
}

#[cfg(test)]
fn verify_pit_house_coexistence_gate(lane: &Path) -> BundleGateCheck {
    verify_pit_house_coexistence_gate_with_support_validation(
        lane,
        SupportBundleValidationMode::Fresh,
    )
}

fn verify_pit_house_coexistence_gate_with_support_validation(
    lane: &Path,
    support_validation: SupportBundleValidationMode,
) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "pit-house-coexistence.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("pit_house_coexistence", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza pit-house-proof");
    let template = json_bool(&receipt, "template");
    let generated_at_present = json_string(&receipt, "generated_at_utc")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let evidence_status_ok =
        json_string(&receipt, "evidence_status") == Some("observed_on_real_hardware");
    let high_torque = json_bool(&receipt, "high_torque");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let direct_requires_ack = json_bool(&receipt, "direct_requires_ack");
    let firmware_blocks_high_risk = json_bool(&receipt, "firmware_page_blocks_high_risk")
        .or_else(|| json_bool(&receipt, "firmware_update_page_blocks_high_risk"));
    let detection_scope_ok = matches!(
        json_string(&receipt, "shared_control_risk"),
        Some("detected" | "warned" | "documented_limit")
    );
    let cases = receipt
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice);
    let required_cases_ok = cases
        .map(|cases| pit_house_cases_are_safe(lane, cases, support_validation))
        .unwrap_or(false);

    let safe = success
        && command_ok
        && template == Some(false)
        && generated_at_present
        && evidence_status_ok
        && high_torque == Some(false)
        && no_out_of_scope
        && direct_requires_ack == Some(true)
        && firmware_blocks_high_risk == Some(true)
        && detection_scope_ok
        && required_cases_ok;

    if safe {
        BundleGateCheck::pass(
            "pit_house_coexistence",
            "Pit House coexistence matrix covers closed, idle, direct, mode-change, and firmware-page cases".to_string(),
        )
    } else {
        BundleGateCheck::fail(
            "pit_house_coexistence",
            format!(
                "success={success}, command_ok={command_ok}, template={template:?}, generated_at_present={generated_at_present}, evidence_status_ok={evidence_status_ok}, high_torque={high_torque:?}, no_out_of_scope={no_out_of_scope}, direct_requires_ack={direct_requires_ack:?}, firmware_blocks_high_risk={firmware_blocks_high_risk:?}, detection_scope_ok={detection_scope_ok}, required_cases_ok={required_cases_ok}"
            ),
        )
    }
}

fn pit_house_cases_are_safe(
    lane: &Path,
    cases: &[Value],
    support_validation: SupportBundleValidationMode,
) -> bool {
    [
        "pit_house_closed",
        "pit_house_open_idle_standard",
        "pit_house_open_direct",
        "pit_house_mode_change_during_run",
        "pit_house_firmware_update_page_open",
    ]
    .iter()
    .all(|case_id| pit_house_case_is_safe(lane, cases, case_id, support_validation))
}

fn pit_house_case_is_safe(
    lane: &Path,
    cases: &[Value],
    case_id: &str,
    support_validation: SupportBundleValidationMode,
) -> bool {
    let Some(case) = cases
        .iter()
        .find(|case| json_string(case, "case") == Some(case_id))
    else {
        return false;
    };

    let observed = json_bool(case, "observed") == Some(true);
    let evidence_present = json_string(case, "evidence")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let result = json_string(case, "result");
    let artifact_valid = json_string(case, "artifact")
        .or_else(|| json_string(case, "evidence_artifact"))
        .map(|value| {
            pit_house_case_artifact_is_safe(lane, value, case_id, result, support_validation)
        })
        .unwrap_or(false);
    let no_high_torque = json_bool(case, "high_torque") == Some(false);
    let result_ok = match case_id {
        "pit_house_closed" => result == Some("staged_handshake_ok"),
        "pit_house_open_idle_standard" => {
            matches!(result, Some("standard_ok" | "conflict_documented"))
        }
        "pit_house_open_direct" => {
            matches!(result, Some("blocked" | "acknowledgement_required"))
                && (json_bool(case, "blocked") == Some(true)
                    || json_bool(case, "operator_ack_required") == Some(true))
        }
        "pit_house_mode_change_during_run" => {
            matches!(result, Some("mismatch_detected" | "failed_safe"))
                && (json_bool(case, "mismatch_detected") == Some(true)
                    || json_bool(case, "failed_safe") == Some(true))
        }
        "pit_house_firmware_update_page_open" => {
            result == Some("high_risk_refused")
                && json_bool(case, "high_risk_refused") == Some(true)
        }
        _ => false,
    };

    observed && evidence_present && artifact_valid && no_high_torque && result_ok
}

fn pit_house_case_artifact_is_safe(
    lane: &Path,
    path: &str,
    case_id: &str,
    expected_result: Option<&str>,
    support_validation: SupportBundleValidationMode,
) -> bool {
    let Some(path) = resolve_receipt_path(lane, path) else {
        return false;
    };
    if !path.is_file() {
        return false;
    }

    let Ok(artifact) = read_json_path(&path) else {
        return false;
    };

    let evidence_present = json_string(&artifact, "evidence")
        .or_else(|| json_string(&artifact, "operator_notes"))
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let base_safe = json_string(&artifact, "case") == Some(case_id)
        && json_bool(&artifact, "observed") == Some(true)
        && json_string(&artifact, "result") == expected_result
        && json_bool(&artifact, "high_torque") == Some(false)
        && no_out_of_scope_device_commands(&artifact)
        && evidence_present
        && pit_house_case_observation_is_safe(lane, &artifact, case_id)
        && pit_house_case_source_is_safe(lane, &artifact, case_id, support_validation);

    if !base_safe {
        return false;
    }

    match case_id {
        "pit_house_closed" => {
            json_string(&artifact, "pit_house_state") == Some("closed")
                && json_bool(&artifact, "staged_handshake_ready") == Some(true)
                && json_bool(&artifact, "conflict_detected") == Some(false)
        }
        "pit_house_open_idle_standard" => {
            json_string(&artifact, "pit_house_state") == Some("open_idle")
                && json_string(&artifact, "ffb_mode") == Some("standard")
                && json_bool(&artifact, "direct_mode_requested") == Some(false)
        }
        "pit_house_open_direct" => {
            json_string(&artifact, "pit_house_state") == Some("open")
                && json_bool(&artifact, "direct_mode_requested") == Some(true)
                && (json_bool(&artifact, "blocked") == Some(true)
                    || json_bool(&artifact, "operator_ack_required") == Some(true))
        }
        "pit_house_mode_change_during_run" => {
            json_bool(&artifact, "mismatch_detected") == Some(true)
                && json_bool(&artifact, "failed_safe") == Some(true)
                && json_bool(&artifact, "output_cleared") == Some(true)
                && json_bool(&artifact, "final_zero_attempted") == Some(true)
        }
        "pit_house_firmware_update_page_open" => {
            json_bool(&artifact, "firmware_update_page_open") == Some(true)
                && json_bool(&artifact, "high_risk_refused") == Some(true)
        }
        _ => false,
    }
}

fn pit_house_case_observation_is_safe(lane: &Path, artifact: &Value, case_id: &str) -> bool {
    let Some(observation_artifact) = json_string(artifact, "pit_house_observation_artifact") else {
        return false;
    };
    let Some(path) = resolve_receipt_path(lane, observation_artifact) else {
        return false;
    };
    if !path.is_file() {
        return false;
    }

    let Ok(observation) = read_json_path(&path) else {
        return false;
    };
    let Some(expected_state) = pit_house_expected_observation_state(case_id) else {
        return false;
    };
    let evidence_present = json_string(&observation, "evidence")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let observed_at_present = json_string(&observation, "observed_at_utc")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let operator_present = json_string(&observation, "operator")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let evidence_artifact_exists = json_string(&observation, "evidence_artifact")
        .and_then(|value| resolve_receipt_path(lane, value))
        .map(|path| path.is_file())
        .unwrap_or(false);
    let evidence_kind_ok = matches!(
        json_string(&observation, "evidence_kind"),
        Some("operator_screenshot" | "operator_video" | "process_window_snapshot")
    );

    json_bool(&observation, "success") == Some(true)
        && json_string(&observation, "command") == Some("wheelctl moza pit-house-observation")
        && json_string(&observation, "case") == Some(case_id)
        && json_bool(&observation, "observed") == Some(true)
        && json_string(&observation, "pit_house_observed_state") == Some(expected_state)
        && evidence_kind_ok
        && observed_at_present
        && operator_present
        && evidence_artifact_exists
        && json_bool(&observation, "no_hid_device_opened") == Some(true)
        && json_bool(&observation, "no_ffb_writes") == Some(true)
        && no_out_of_scope_device_commands(&observation)
        && evidence_present
}

fn pit_house_case_artifact_receipt(
    case: MozaPitHouseObservationCase,
    observation_artifact: &str,
    evidence: &str,
) -> Value {
    match case {
        MozaPitHouseObservationCase::Closed => serde_json::json!({
            "case": "pit_house_closed",
            "observed": true,
            "result": "staged_handshake_ok",
            "pit_house_state": "closed",
            "staged_handshake_ready": true,
            "conflict_detected": false,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "pit_house_observation_artifact": observation_artifact,
            "source_receipt": "init-off.json",
            "source_gate": "init_off_handshake",
            "source_log": "feature_reports",
            "source_record_kinds": ["start_input_reports", "ffb_mode"],
            "evidence": evidence
        }),
        MozaPitHouseObservationCase::OpenStandard => serde_json::json!({
            "case": "pit_house_open_idle_standard",
            "observed": true,
            "result": "standard_ok",
            "pit_house_state": "open_idle",
            "ffb_mode": "standard",
            "direct_mode_requested": false,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "pit_house_observation_artifact": observation_artifact,
            "source_receipt": "init-standard.json",
            "source_gate": "init_standard_handshake",
            "source_log": "feature_reports",
            "source_record_kinds": ["start_input_reports", "ffb_mode"],
            "evidence": evidence
        }),
        MozaPitHouseObservationCase::OpenDirect => serde_json::json!({
            "case": "pit_house_open_direct",
            "observed": true,
            "result": "blocked",
            "pit_house_state": "open",
            "direct_mode_requested": true,
            "blocked": true,
            "operator_ack_required": true,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "pit_house_observation_artifact": observation_artifact,
            "source_receipt": "low-torque-proof.json",
            "source_gate": "low_torque_bounded",
            "source_log": "command_log",
            "source_record_kind": "low_torque",
            "evidence": evidence
        }),
        MozaPitHouseObservationCase::ModeChange => serde_json::json!({
            "case": "pit_house_mode_change_during_run",
            "observed": true,
            "result": "mismatch_detected",
            "mismatch_detected": true,
            "failed_safe": true,
            "output_cleared": true,
            "final_zero_attempted": true,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "pit_house_observation_artifact": observation_artifact,
            "source_receipt": "simulator-ffb-smoke.json",
            "source_gate": "simulator_ffb_bounded",
            "source_log": "output_log_artifact",
            "source_record_kind": "clear_zero",
            "source_clear_event": "mode_mismatch",
            "source_requires_final_zero": true,
            "evidence": evidence
        }),
        MozaPitHouseObservationCase::FirmwarePage => serde_json::json!({
            "case": "pit_house_firmware_update_page_open",
            "observed": true,
            "result": "high_risk_refused",
            "firmware_update_page_open": true,
            "high_risk_refused": true,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "pit_house_observation_artifact": observation_artifact,
            "source_receipt": "support-bundle.json",
            "source_gate": "service_status_receipts",
            "source_log": "device_statuses",
            "evidence": evidence
        }),
    }
}

fn pit_house_observation_case_id(case: MozaPitHouseObservationCase) -> &'static str {
    match case {
        MozaPitHouseObservationCase::Closed => "pit_house_closed",
        MozaPitHouseObservationCase::OpenStandard => "pit_house_open_idle_standard",
        MozaPitHouseObservationCase::OpenDirect => "pit_house_open_direct",
        MozaPitHouseObservationCase::ModeChange => "pit_house_mode_change_during_run",
        MozaPitHouseObservationCase::FirmwarePage => "pit_house_firmware_update_page_open",
    }
}

fn pit_house_observation_state(case: MozaPitHouseObservationCase) -> &'static str {
    match case {
        MozaPitHouseObservationCase::Closed => "closed",
        MozaPitHouseObservationCase::OpenStandard => "open_idle_standard",
        MozaPitHouseObservationCase::OpenDirect => "open_direct",
        MozaPitHouseObservationCase::ModeChange => "mode_change_during_run",
        MozaPitHouseObservationCase::FirmwarePage => "firmware_update_page_open",
    }
}

fn pit_house_evidence_kind_label(kind: MozaPitHouseEvidenceKind) -> &'static str {
    match kind {
        MozaPitHouseEvidenceKind::OperatorNotes => "operator_notes",
        MozaPitHouseEvidenceKind::OperatorScreenshot => "operator_screenshot",
        MozaPitHouseEvidenceKind::OperatorVideo => "operator_video",
        MozaPitHouseEvidenceKind::ProcessWindowSnapshot => "process_window_snapshot",
    }
}

fn pit_house_expected_observation_state(case_id: &str) -> Option<&'static str> {
    match case_id {
        "pit_house_closed" => Some("closed"),
        "pit_house_open_idle_standard" => Some("open_idle_standard"),
        "pit_house_open_direct" => Some("open_direct"),
        "pit_house_mode_change_during_run" => Some("mode_change_during_run"),
        "pit_house_firmware_update_page_open" => Some("firmware_update_page_open"),
        _ => None,
    }
}

fn pit_house_case_source_is_safe(
    lane: &Path,
    artifact: &Value,
    case_id: &str,
    support_validation: SupportBundleValidationMode,
) -> bool {
    let Some(source_receipt) = json_string(artifact, "source_receipt") else {
        return false;
    };
    let Some(source_gate) = json_string(artifact, "source_gate") else {
        return false;
    };
    let Some(source_log) = json_string(artifact, "source_log") else {
        return false;
    };
    if resolve_receipt_path(lane, source_receipt).map(|path| path.is_file()) != Some(true) {
        return false;
    }

    match case_id {
        "pit_house_closed" => {
            source_receipt == "init-off.json"
                && source_gate == "init_off_handshake"
                && source_log == "feature_reports"
                && verify_init_receipt_gate(lane, "init_off_handshake", source_receipt, "off")
                    .status
                    == "pass"
                && pit_house_source_feature_reports_are_safe(lane, source_receipt, artifact)
        }
        "pit_house_open_idle_standard" => {
            source_receipt == "init-standard.json"
                && source_gate == "init_standard_handshake"
                && source_log == "feature_reports"
                && verify_init_receipt_gate(
                    lane,
                    "init_standard_handshake",
                    source_receipt,
                    "standard",
                )
                .status
                    == "pass"
                && pit_house_source_feature_reports_are_safe(lane, source_receipt, artifact)
        }
        "pit_house_open_direct" => {
            source_receipt == "low-torque-proof.json"
                && source_gate == "low_torque_bounded"
                && source_log == "command_log"
                && json_string(artifact, "source_record_kind") == Some("low_torque")
                && verify_low_torque_gate(lane).status == "pass"
                && pit_house_low_torque_source_is_safe(lane, source_receipt)
        }
        "pit_house_mode_change_during_run" => {
            source_receipt == "simulator-ffb-smoke.json"
                && source_gate == "simulator_ffb_bounded"
                && source_log == "output_log_artifact"
                && json_string(artifact, "source_record_kind") == Some("clear_zero")
                && json_string(artifact, "source_clear_event") == Some("mode_mismatch")
                && json_bool(artifact, "source_requires_final_zero") == Some(true)
                && verify_simulator_ffb_gate(lane).status == "pass"
                && pit_house_mode_change_source_is_safe(lane, source_receipt)
        }
        "pit_house_firmware_update_page_open" => {
            source_receipt == "support-bundle.json"
                && source_gate == "service_status_receipts"
                && source_log == "device_statuses"
                && verify_service_status_gate_with_support_validation(lane, support_validation)
                    .status
                    == "pass"
        }
        _ => false,
    }
}

fn pit_house_source_feature_reports_are_safe(
    lane: &Path,
    source_receipt: &str,
    artifact: &Value,
) -> bool {
    let Ok(receipt) = read_json_value(lane, source_receipt) else {
        return false;
    };
    let Some(expected_kinds) = artifact
        .get("source_record_kinds")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if expected_kinds.is_empty() {
        return false;
    }
    let Some(records) = receipt.get("feature_reports").and_then(Value::as_array) else {
        return false;
    };

    expected_kinds.iter().all(|expected| {
        expected.as_str().is_some_and(|expected| {
            records
                .iter()
                .any(|record| json_string(record, "kind") == Some(expected))
        })
    })
}

fn pit_house_low_torque_source_is_safe(lane: &Path, source_receipt: &str) -> bool {
    let Ok(receipt) = read_json_value(lane, source_receipt) else {
        return false;
    };
    json_bool(&receipt, "confirmed") == Some(true)
        && json_bool(&receipt, "direct_mode_gate_satisfied") == Some(true)
        && receipt
            .get("command_log")
            .and_then(Value::as_array)
            .map(|records| {
                records
                    .iter()
                    .any(|record| json_string(record, "kind") == Some("low_torque"))
            })
            .unwrap_or(false)
}

fn pit_house_mode_change_source_is_safe(lane: &Path, source_receipt: &str) -> bool {
    let Ok(receipt) = read_json_value(lane, source_receipt) else {
        return false;
    };
    let Some(output_log_artifact) = json_string(&receipt, "output_log_artifact") else {
        return false;
    };
    let Some(records) = read_receipt_artifact_records(
        lane,
        output_log_artifact,
        &["output_log", "records", "commands", "reports"],
    ) else {
        return false;
    };

    let mode_mismatch_clear = records.iter().any(|record| {
        json_string(record, "kind") == Some("clear_zero")
            && json_string(record, "clear_event") == Some("mode_mismatch")
            && zero_payload_record_is_safe(record)
    });
    let final_zero = records.last().is_some_and(|record| {
        json_string(record, "kind") == Some("final_zero") && zero_payload_record_is_safe(record)
    });

    mode_mismatch_clear && final_zero
}

fn verify_simulator_telemetry_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "simulator-telemetry-proof.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("simulator_telemetry", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok =
        json_string(&receipt, "command") == Some("wheelctl moza simulator-telemetry-proof");
    let game = json_string(&receipt, "game").unwrap_or_default();
    let source = json_string(&receipt, "telemetry_source");
    let source_ok = matches!(source, Some("real_game" | "simhub_bridge"));
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes");
    let hardware_output_enabled = json_bool(&receipt, "hardware_output_enabled");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let snapshot_count = json_u64(&receipt, "normalized_snapshot_count")
        .or_else(|| json_u64(&receipt, "snapshots_recorded"))
        .unwrap_or(0);
    let duration_ms = json_u64(&receipt, "duration_ms").unwrap_or(0);
    let recorder_artifact = json_string(&receipt, "recorder_artifact")
        .or_else(|| json_string(&receipt, "normalized_snapshot_artifact"));
    let recorder_provenance_valid = recorder_artifact
        .map(|value| simulator_telemetry_artifact_provenance_matches(lane, value, &receipt))
        .unwrap_or(false);
    let recorder_artifact_valid = json_string(&receipt, "recorder_artifact")
        .or_else(|| json_string(&receipt, "normalized_snapshot_artifact"))
        .map(|value| {
            simulator_telemetry_artifact_is_valid(lane, value, snapshot_count, duration_ms)
        })
        .unwrap_or(false);
    let faults_empty = receipt
        .get("faults")
        .and_then(Value::as_array)
        .map(Vec::is_empty)
        .unwrap_or(false);

    let safe = success
        && command_ok
        && !game.trim().is_empty()
        && source_ok
        && no_ffb_writes == Some(true)
        && hardware_output_enabled == Some(false)
        && no_out_of_scope
        && snapshot_count > 0
        && duration_ms > 0
        && recorder_provenance_valid
        && recorder_artifact_valid
        && faults_empty;

    if safe {
        BundleGateCheck::pass(
            "simulator_telemetry",
            format!(
                "game={game}, telemetry_source={source:?}, normalized snapshots={snapshot_count}"
            ),
        )
    } else {
        BundleGateCheck::fail(
            "simulator_telemetry",
            format!(
                "success={success}, command_ok={command_ok}, game_present={}, telemetry_source={source:?}, no_ffb_writes={no_ffb_writes:?}, hardware_output_enabled={hardware_output_enabled:?}, no_out_of_scope={no_out_of_scope}, snapshot_count={snapshot_count}, duration_ms={duration_ms}, recorder_provenance_valid={recorder_provenance_valid}, recorder_artifact_valid={recorder_artifact_valid}, faults_empty={faults_empty}",
                !game.trim().is_empty()
            ),
        )
    }
}

fn verify_simulator_ffb_gate(lane: &Path) -> BundleGateCheck {
    let receipt = match read_json_value(lane, "simulator-ffb-smoke.json") {
        Ok(value) => value,
        Err(e) => return BundleGateCheck::fail("simulator_ffb_bounded", e.to_string()),
    };

    let success = json_bool(&receipt, "success") == Some(true);
    let command_ok = json_string(&receipt, "command") == Some("wheelctl moza simulator-ffb-smoke");
    let game = json_string(&receipt, "game").unwrap_or_default();
    let source = json_string(&receipt, "telemetry_source");
    let source_ok = matches!(source, Some("real_game" | "simhub_bridge"));
    let hardware = json_string(&receipt, "hardware");
    let hardware_ok = hardware == Some("moza-r5");
    let ffb_mode = json_string(&receipt, "ffb_mode");
    let descriptor_trusted = json_bool(&receipt, "descriptor_trusted");
    let explicit_operator_override = json_bool(&receipt, "explicit_operator_override");
    let receipt_pid = receipt_r5_device_pid(&receipt);
    let descriptor_trust_observed = receipt_pid
        .map(|pid| lane_descriptor_trusted_for_pid(lane, &hex_u16(pid)))
        .unwrap_or(false);
    let descriptor_trust_valid = descriptor_trusted != Some(true) || descriptor_trust_observed;
    let direct_mode_allowed = ffb_mode == Some("direct")
        && ((descriptor_trusted == Some(true) && descriptor_trust_observed)
            || explicit_operator_override == Some(true))
        && descriptor_trust_valid;
    let high_torque = json_bool(&receipt, "high_torque");
    let no_high_torque = json_bool(&receipt, "no_high_torque");
    let no_out_of_scope = no_out_of_scope_device_commands(&receipt);
    let no_hid_device_opened = json_bool(&receipt, "no_hid_device_opened");
    let no_ffb_writes = json_bool(&receipt, "no_ffb_writes");
    let hardware_prerequisites_validated = json_bool(&receipt, "hardware_prerequisites_validated");
    let prerequisite_gates_valid = simulator_ffb_prerequisite_gates_match(lane, &receipt);
    let prerequisite_artifacts_valid = simulator_ffb_prerequisite_artifacts_match(
        lane,
        &receipt,
        json_string(&receipt, "writer_started_at_utc"),
    );
    let hardware_output_enabled = json_bool(&receipt, "hardware_output_enabled");
    let watchdog_active = json_bool(&receipt, "watchdog_active");
    let watchdog_timeout_ms = json_u64(&receipt, "watchdog_timeout_ms").unwrap_or(0);
    let final_zero_attempted = json_bool(&receipt, "final_zero_attempted");
    let final_zero_sent = json_bool(&receipt, "final_zero_sent");
    let stop_cleared_output = json_bool(&receipt, "stop_cleared_output");
    let pause_cleared_output = json_bool(&receipt, "pause_cleared_output");
    let game_exit_cleared_output = json_bool(&receipt, "game_exit_cleared_output");
    let mode_mismatch_cleared_output = json_bool(&receipt, "mode_mismatch_cleared_output");
    let output_report_count = json_u64(&receipt, "output_report_count")
        .or_else(|| json_u64(&receipt, "output_command_count"))
        .unwrap_or(0);
    let nonzero_output_count = json_u64(&receipt, "nonzero_output_count")
        .or_else(|| json_u64(&receipt, "nonzero_command_count"))
        .unwrap_or(0);
    let zero_output_count = json_u64(&receipt, "zero_output_count")
        .or_else(|| json_u64(&receipt, "zero_command_count"))
        .unwrap_or(0);
    let final_zero_payload_safe = json_string(&receipt, "final_zero_payload_hex")
        .map(|value| value.eq_ignore_ascii_case("2000000000000000"))
        .unwrap_or(false)
        || (json_string(&receipt, "final_zero_report_id") == Some(DIRECT_TORQUE_REPORT_ID)
            && json_i64(&receipt, "final_zero_torque_raw") == Some(0)
            && json_u64(&receipt, "final_zero_flags") == Some(0));
    let max_percent = json_f64(&receipt, "max_output_percent")
        .or_else(|| json_f64(&receipt, "max_percent"))
        .or_else(|| json_f64(&receipt, "max_command_percent"));
    let max_abs_output_percent = json_f64(&receipt, "max_abs_output_percent")
        .or_else(|| json_f64(&receipt, "max_observed_output_percent"))
        .or(max_percent);
    let simulator_telemetry_gate_valid = verify_simulator_telemetry_gate(lane).status == "pass";
    let linked_telemetry = simulator_ffb_telemetry_link_is_valid(lane, &receipt);
    let linked_telemetry_snapshot_count = linked_telemetry
        .as_ref()
        .map(|telemetry| telemetry.snapshot_count);
    let output_log_provenance_valid = json_string(&receipt, "output_log_artifact")
        .or_else(|| json_string(&receipt, "output_receipt_artifact"))
        .map(|value| {
            receipt_pid
                .map(|pid| {
                    simulator_ffb_output_artifact_provenance_matches(lane, value, pid, &receipt)
                })
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let output_log_artifact_valid = json_string(&receipt, "output_log_artifact")
        .or_else(|| json_string(&receipt, "output_receipt_artifact"))
        .map(|value| {
            let max_artifact_percent = max_abs_output_percent.or(max_percent).unwrap_or(0.0);
            linked_telemetry
                .as_ref()
                .map(|telemetry| {
                    simulator_ffb_output_artifact_is_safe(
                        lane,
                        value,
                        output_report_count,
                        nonzero_output_count,
                        zero_output_count,
                        max_artifact_percent,
                        receipt_pid.unwrap_or(product_ids::R5_V2),
                        telemetry,
                    )
                })
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let faults_empty = receipt
        .get("faults")
        .and_then(Value::as_array)
        .map(Vec::is_empty)
        .unwrap_or(false);
    let r5_output_device = receipt_targets_r5_output_device(&receipt);

    let safe = success
        && command_ok
        && !game.trim().is_empty()
        && source_ok
        && hardware_ok
        && direct_mode_allowed
        && high_torque == Some(false)
        && no_high_torque == Some(true)
        && no_out_of_scope
        && no_hid_device_opened == Some(false)
        && no_ffb_writes == Some(false)
        && hardware_prerequisites_validated == Some(true)
        && prerequisite_gates_valid
        && prerequisite_artifacts_valid
        && hardware_output_enabled == Some(true)
        && watchdog_active == Some(true)
        && watchdog_timeout_ms > 0
        && final_zero_attempted == Some(true)
        && final_zero_sent == Some(true)
        && final_zero_payload_safe
        && stop_cleared_output == Some(true)
        && pause_cleared_output == Some(true)
        && game_exit_cleared_output == Some(true)
        && mode_mismatch_cleared_output == Some(true)
        && output_report_count > 0
        && nonzero_output_count > 0
        && zero_output_count > 0
        && simulator_telemetry_gate_valid
        && linked_telemetry_snapshot_count.is_some()
        && output_log_provenance_valid
        && output_log_artifact_valid
        && faults_empty
        && r5_output_device
        && max_percent
            .map(|value| value.is_finite() && value > 0.0 && value <= 5.0)
            .unwrap_or(false)
        && max_abs_output_percent
            .map(|value| value.is_finite() && (0.0..=5.0).contains(&value))
            .unwrap_or(false);

    if safe {
        BundleGateCheck::pass(
            "simulator_ffb_bounded",
            format!("max_output_percent={max_percent:?}, watchdog active, final zero sent"),
        )
    } else {
        BundleGateCheck::fail(
            "simulator_ffb_bounded",
            format!(
                "success={success}, command_ok={command_ok}, game_present={}, telemetry_source={source:?}, hardware={hardware:?}, ffb_mode={ffb_mode:?}, descriptor_trusted={descriptor_trusted:?}, descriptor_trust_observed={descriptor_trust_observed}, descriptor_trust_valid={descriptor_trust_valid}, explicit_operator_override={explicit_operator_override:?}, direct_mode_allowed={direct_mode_allowed}, high_torque={high_torque:?}, no_high_torque={no_high_torque:?}, no_out_of_scope={no_out_of_scope}, no_hid_device_opened={no_hid_device_opened:?}, no_ffb_writes={no_ffb_writes:?}, hardware_prerequisites_validated={hardware_prerequisites_validated:?}, prerequisite_gates_valid={prerequisite_gates_valid}, prerequisite_artifacts_valid={prerequisite_artifacts_valid}, hardware_output_enabled={hardware_output_enabled:?}, watchdog_active={watchdog_active:?}, watchdog_timeout_ms={watchdog_timeout_ms}, final_zero_attempted={final_zero_attempted:?}, final_zero_sent={final_zero_sent:?}, final_zero_payload_safe={final_zero_payload_safe}, stop_cleared_output={stop_cleared_output:?}, pause_cleared_output={pause_cleared_output:?}, game_exit_cleared_output={game_exit_cleared_output:?}, mode_mismatch_cleared_output={mode_mismatch_cleared_output:?}, output_report_count={output_report_count}, nonzero_output_count={nonzero_output_count}, zero_output_count={zero_output_count}, simulator_telemetry_gate_valid={simulator_telemetry_gate_valid}, linked_telemetry_snapshot_count={linked_telemetry_snapshot_count:?}, output_log_provenance_valid={output_log_provenance_valid}, output_log_artifact_valid={output_log_artifact_valid}, faults_empty={faults_empty}, r5_output_device={r5_output_device}, max_output_percent={max_percent:?}, max_abs_output_percent={max_abs_output_percent:?}",
                !game.trim().is_empty()
            ),
        )
    }
}

fn simulator_ffb_prerequisite_gates(lane: &Path) -> Vec<BundleGateCheck> {
    vec![
        verify_zero_torque_gate(lane),
        verify_watchdog_proof_gate(lane),
        verify_disconnect_proof_gate(lane),
        verify_init_receipt_gate(lane, "init_off_handshake", "init-off.json", "off"),
        verify_init_receipt_gate(
            lane,
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        ),
        verify_low_torque_gate(lane),
    ]
}

fn simulator_ffb_prerequisite_gates_match(lane: &Path, receipt: &Value) -> bool {
    let expected = simulator_ffb_prerequisite_gates(lane);
    if expected.iter().any(|gate| gate.status != "pass") {
        return false;
    }
    let Some(receipt_gates) = receipt.get("prerequisite_gates").and_then(Value::as_array) else {
        return false;
    };
    if receipt_gates.len() != expected.len() {
        return false;
    }

    expected.iter().all(|expected_gate| {
        receipt_gates.iter().any(|gate| {
            json_string(gate, "name") == Some(expected_gate.name)
                && json_string(gate, "status") == Some("pass")
        })
    })
}

fn simulator_ffb_prerequisite_artifact_summaries(lane: &Path) -> Result<Vec<Value>> {
    SIMULATOR_FFB_PREREQUISITE_ARTIFACTS
        .iter()
        .map(|(gate, path)| {
            let receipt = read_json_value(lane, path)?;
            let generated_at_utc = json_string(&receipt, "generated_at_utc")
                .ok_or_else(|| anyhow!("{path} is missing generated_at_utc"))?;
            if !receipt_path_matches(lane, &receipt, path) {
                return Err(anyhow!("{path} receipt_path does not match lane artifact"));
            }
            let receipt_crc32 = receipt_file_crc32(&lane.join(path))?;
            Ok(serde_json::json!({
                "gate": gate,
                "path": path,
                "generated_at_utc": generated_at_utc,
                "receipt_crc32": receipt_crc32
            }))
        })
        .collect()
}

fn simulator_ffb_prerequisite_artifacts_match(
    lane: &Path,
    receipt: &Value,
    writer_started_at_utc: Option<&str>,
) -> bool {
    let Ok(expected) = simulator_ffb_prerequisite_artifact_summaries(lane) else {
        return false;
    };
    if !simulator_ffb_prerequisite_artifacts_are_ordered(&expected, writer_started_at_utc) {
        return false;
    }
    let Some(actual) = receipt
        .get("prerequisite_artifacts")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if actual.len() != expected.len() {
        return false;
    }

    expected.iter().all(|expected_artifact| {
        actual.iter().any(|artifact| {
            json_string(artifact, "gate") == json_string(expected_artifact, "gate")
                && json_string(artifact, "path") == json_string(expected_artifact, "path")
                && json_string(artifact, "generated_at_utc")
                    == json_string(expected_artifact, "generated_at_utc")
                && json_string(artifact, "receipt_crc32")
                    == json_string(expected_artifact, "receipt_crc32")
        })
    })
}

fn simulator_ffb_prerequisite_artifacts_are_ordered(
    artifacts: &[Value],
    writer_started_at_utc: Option<&str>,
) -> bool {
    let Some(writer_started_at_utc) = writer_started_at_utc else {
        return false;
    };
    artifacts.iter().all(|artifact| {
        json_string(artifact, "generated_at_utc")
            .map(|generated_at_utc| {
                utc_timestamp_pair_is_ordered(generated_at_utc, writer_started_at_utc)
            })
            .unwrap_or(false)
    })
}

struct SimulatorFfbTelemetryLink {
    artifact: String,
    snapshot_count: u64,
    recorder_session_id: String,
    game: String,
    telemetry_source: String,
    ffb_scalars_by_sequence: BTreeMap<u64, f64>,
}

fn simulator_ffb_telemetry_link_is_valid(
    lane: &Path,
    receipt: &Value,
) -> Option<SimulatorFfbTelemetryLink> {
    let linked_artifact = json_string(receipt, "input_telemetry_artifact")
        .or_else(|| json_string(receipt, "derived_from_recorder_artifact"))?;
    let linked_count = json_u64(receipt, "input_telemetry_snapshot_count")
        .or_else(|| json_u64(receipt, "derived_from_snapshot_count"))?;
    let linked_recorder_session_id = json_string(receipt, "input_telemetry_recorder_session_id")?;
    if linked_count == 0 {
        return None;
    }

    let telemetry_receipt = read_json_value(lane, "simulator-telemetry-proof.json").ok()?;
    let telemetry_artifact = json_string(&telemetry_receipt, "recorder_artifact")
        .or_else(|| json_string(&telemetry_receipt, "normalized_snapshot_artifact"))?;
    let telemetry_count = json_u64(&telemetry_receipt, "normalized_snapshot_count")?;
    let telemetry_recorder_session_id = json_string(&telemetry_receipt, "recorder_session_id")?;
    let telemetry_game = json_string(&telemetry_receipt, "game")?;
    let telemetry_source = json_string(&telemetry_receipt, "telemetry_source")?;
    if linked_artifact != telemetry_artifact
        || linked_count != telemetry_count
        || linked_recorder_session_id != telemetry_recorder_session_id
        || json_string(receipt, "game") != Some(telemetry_game)
        || json_string(receipt, "telemetry_source") != Some(telemetry_source)
    {
        return None;
    }

    let telemetry_duration_ms = json_u64(&telemetry_receipt, "duration_ms")?;
    if !simulator_telemetry_artifact_is_valid(
        lane,
        linked_artifact,
        linked_count,
        telemetry_duration_ms,
    ) {
        return None;
    }
    let ffb_scalars_by_sequence =
        simulator_telemetry_ffb_scalars_by_sequence(lane, linked_artifact, linked_count)?;

    Some(SimulatorFfbTelemetryLink {
        artifact: linked_artifact.to_string(),
        snapshot_count: linked_count,
        recorder_session_id: linked_recorder_session_id.to_string(),
        game: telemetry_game.to_string(),
        telemetry_source: telemetry_source.to_string(),
        ffb_scalars_by_sequence,
    })
}

fn simulator_telemetry_ffb_scalars_by_sequence(
    lane: &Path,
    path: &str,
    expected_count: u64,
) -> Option<BTreeMap<u64, f64>> {
    let records = read_telemetry_artifact_records(lane, path)?;
    if usize::try_from(expected_count).ok()? != records.len() {
        return None;
    }

    let mut ffb_scalars = BTreeMap::new();
    for record in records {
        let sequence = telemetry_record_u64(&record, "sequence")?;
        let ffb_scalar = telemetry_record_f64(&record, "ffb_scalar")?;
        if sequence >= expected_count
            || !ffb_scalar.is_finite()
            || !(-1.0..=1.0).contains(&ffb_scalar)
            || ffb_scalars.insert(sequence, ffb_scalar).is_some()
        {
            return None;
        }
    }

    (ffb_scalars.len() == usize::try_from(expected_count).ok()?).then_some(ffb_scalars)
}

fn simulator_telemetry_artifact_is_valid(
    lane: &Path,
    path: &str,
    expected_count: u64,
    expected_duration_ms: u64,
) -> bool {
    if expected_count == 0 || expected_duration_ms == 0 {
        return false;
    }

    let Some(records) = read_telemetry_artifact_records(lane, path) else {
        return false;
    };
    let Ok(expected_len) = usize::try_from(expected_count) else {
        return false;
    };
    if records.len() != expected_len {
        return false;
    }

    let mut previous_sequence = None;
    let mut sequence_contiguous = true;
    let mut all_records_have_sequence = true;
    let mut previous_timestamp = None;
    let mut timestamp_monotonic = true;
    let mut all_records_have_timestamp = true;
    let mut timestamp_advanced = false;
    let mut first_timestamp = None;
    let mut last_timestamp = None;
    let mut duration_matches_artifact = true;
    let mut all_records_have_duration = true;
    let mut telemetry_provenance: Option<SimulatorTelemetryProvenance> = None;

    for record in &records {
        if !telemetry_record_has_normalized_fields(record) {
            return false;
        }
        let Some(record_provenance) = simulator_telemetry_record_provenance(record) else {
            return false;
        };
        if let Some(previous) = &telemetry_provenance {
            if previous != &record_provenance {
                return false;
            }
        } else {
            telemetry_provenance = Some(record_provenance);
        }

        match telemetry_record_u64(record, "sequence") {
            Some(sequence) => {
                if let Some(previous) = previous_sequence
                    && sequence != previous + 1
                {
                    sequence_contiguous = false;
                }
                previous_sequence = Some(sequence);
            }
            None => {
                all_records_have_sequence = false;
                sequence_contiguous = false;
            }
        }

        match telemetry_record_u64(record, "timestamp_ns") {
            Some(timestamp) => {
                first_timestamp.get_or_insert(timestamp);
                if let Some(previous) = previous_timestamp {
                    if timestamp < previous {
                        timestamp_monotonic = false;
                    }
                    if timestamp > previous {
                        timestamp_advanced = true;
                    }
                }
                previous_timestamp = Some(timestamp);
                last_timestamp = Some(timestamp);
            }
            None => {
                all_records_have_timestamp = false;
                timestamp_monotonic = false;
            }
        }

        match telemetry_record_u64(record, "recording_duration_ms")
            .or_else(|| telemetry_record_u64(record, "duration_ms"))
        {
            Some(duration_ms) if duration_ms == expected_duration_ms => {}
            Some(_) => {
                duration_matches_artifact = false;
            }
            None => {
                all_records_have_duration = false;
                duration_matches_artifact = false;
            }
        }
    }

    let ordering_ok = (all_records_have_sequence && sequence_contiguous)
        || (all_records_have_timestamp && timestamp_monotonic && timestamp_advanced);
    let timestamp_span_ok =
        timestamp_span_covers_duration(first_timestamp, last_timestamp, expected_duration_ms);
    let duration_ok = (all_records_have_duration && duration_matches_artifact) || timestamp_span_ok;

    ordering_ok && duration_ok
}

fn timestamp_span_covers_duration(
    first_timestamp_ns: Option<u64>,
    last_timestamp_ns: Option<u64>,
    expected_duration_ms: u64,
) -> bool {
    let (Some(first), Some(last)) = (first_timestamp_ns, last_timestamp_ns) else {
        return false;
    };
    let Some(span_ns) = last.checked_sub(first) else {
        return false;
    };
    let required_ns = expected_duration_ms
        .saturating_mul(1_000_000)
        .saturating_mul(9)
        / 10;
    span_ns >= required_ns
}

#[derive(Clone, PartialEq, Eq)]
struct SimulatorTelemetryProvenance {
    recorder_command: String,
    recorder_session_id: String,
    game: String,
    telemetry_source: String,
}

fn simulator_telemetry_provenance_for_records(
    records: &[Value],
) -> Option<SimulatorTelemetryProvenance> {
    let mut summary = None;
    for record in records {
        let next = simulator_telemetry_record_provenance(record)?;
        if let Some(previous) = &summary {
            if previous != &next {
                return None;
            }
        } else {
            summary = Some(next);
        }
    }
    summary
}

fn simulator_telemetry_artifact_provenance_matches(
    lane: &Path,
    path: &str,
    receipt: &Value,
) -> bool {
    let Some(records) = read_telemetry_artifact_records(lane, path) else {
        return false;
    };
    simulator_telemetry_provenance_for_records(&records)
        .map(|provenance| {
            json_string(receipt, "recorder_command") == Some(provenance.recorder_command.as_str())
                && json_string(receipt, "recorder_session_id")
                    == Some(provenance.recorder_session_id.as_str())
                && json_string(receipt, "game") == Some(provenance.game.as_str())
                && json_string(receipt, "telemetry_source")
                    == Some(provenance.telemetry_source.as_str())
        })
        .unwrap_or(false)
}

fn simulator_telemetry_record_provenance(record: &Value) -> Option<SimulatorTelemetryProvenance> {
    let recorder_command = telemetry_record_string(record, "recorder_command")?;
    let recorder_session_id = telemetry_record_string(record, "recorder_session_id")
        .or_else(|| telemetry_record_string(record, "session_id"))?;
    let game = telemetry_record_string(record, "game")?;
    let telemetry_source = telemetry_record_string(record, "telemetry_source")?;
    if recorder_command != SIMULATOR_TELEMETRY_RECORDER_COMMAND
        || recorder_session_id.trim().is_empty()
        || game.trim().is_empty()
        || !matches!(telemetry_source, "real_game" | "simhub_bridge")
        || telemetry_record_bool(record, "hardware_output_enabled") != Some(false)
        || telemetry_record_bool(record, "no_ffb_writes") != Some(true)
        || telemetry_record_bool(record, "no_serial_config_commands") != Some(true)
        || telemetry_record_bool(record, "no_firmware_or_dfu_commands") != Some(true)
    {
        return None;
    }

    Some(SimulatorTelemetryProvenance {
        recorder_command: recorder_command.to_string(),
        recorder_session_id: recorder_session_id.to_string(),
        game: game.to_string(),
        telemetry_source: telemetry_source.to_string(),
    })
}

fn simulator_ffb_output_artifact_is_safe(
    lane: &Path,
    path: &str,
    expected_count: u64,
    expected_nonzero_count: u64,
    expected_zero_count: u64,
    max_percent: f64,
    pid: u16,
    telemetry_link: &SimulatorFfbTelemetryLink,
) -> bool {
    if expected_count == 0
        || expected_nonzero_count == 0
        || expected_zero_count == 0
        || !max_percent.is_finite()
        || !(0.0..=5.0).contains(&max_percent)
        || telemetry_link.snapshot_count == 0
    {
        return false;
    }

    let Some(records) = read_receipt_artifact_records(
        lane,
        path,
        &["output_log", "records", "commands", "reports"],
    ) else {
        return false;
    };
    let Ok(expected_len) = usize::try_from(expected_count) else {
        return false;
    };
    if records.len() != expected_len {
        return false;
    }

    let max_raw_payload = low_torque_payload_for_pid_percent(pid, max_percent as f32).0;
    let max_raw = i32::from(i16::from_le_bytes([max_raw_payload[1], max_raw_payload[2]]))
        .abs()
        .max(1);

    let mut nonzero_count = 0u64;
    let mut zero_count = 0u64;
    let mut clear_events = SimulatorFfbClearEvents::default();
    let mut previous_elapsed_us = None;
    let mut elapsed_advanced = false;

    for (index, record) in records.iter().enumerate() {
        let Some(output) = direct_torque_artifact_record(record) else {
            return false;
        };

        let Some(elapsed_us) = json_u64(record, "elapsed_us") else {
            return false;
        };
        if let Some(previous) = previous_elapsed_us {
            if elapsed_us < previous {
                return false;
            }
            if elapsed_us > previous {
                elapsed_advanced = true;
            }
        }
        previous_elapsed_us = Some(elapsed_us);

        if json_string(record, "result") != Some("ok")
            || json_u64(record, "bytes_written") != Some(REPORT_LEN as u64)
            || !record_sequence_matches_index(record, index)
            || !simulator_ffb_record_has_hid_write_metadata(record)
            || !simulator_ffb_record_links_telemetry(record, telemetry_link)
        {
            return false;
        }

        let is_last = index + 1 == records.len();
        let kind = json_string(record, "kind");
        if is_last {
            if kind != Some("final_zero")
                || !zero_payload_record_is_safe(record)
                || output.percent != 0.0
            {
                return false;
            }
        } else if kind == Some("final_zero") {
            return false;
        }

        if output.torque_raw == 0 {
            if output.flags != 0 || output.motor_enabled || output.percent != 0.0 {
                return false;
            }
            if kind == Some("clear_zero") {
                clear_events.record(record, index);
            }
            zero_count += 1;
        } else {
            if output.flags != 0x01
                || !output.motor_enabled
                || output.percent == 0.0
                || output.percent.abs() > max_percent
                || i32::from(output.torque_raw).abs() > max_raw
                || !simulator_ffb_output_sign_matches_input(record, output.percent)
            {
                return false;
            }
            nonzero_count += 1;
        }
    }

    nonzero_count == expected_nonzero_count
        && zero_count == expected_zero_count
        && elapsed_advanced
        && clear_events.all_ordered_before_final_zero(records.len().saturating_sub(1))
}

fn simulator_ffb_clear_events_for_records(records: &[Value]) -> SimulatorFfbClearEvents {
    let mut clear_events = SimulatorFfbClearEvents::default();
    for (index, record) in records.iter().enumerate() {
        if json_string(record, "kind") == Some("clear_zero") && zero_payload_record_is_safe(record)
        {
            clear_events.record(record, index);
        }
    }
    clear_events
}

struct SimulatorFfbOutputProvenance {
    writer_command: String,
    writer_session_id: String,
    device_path: String,
    product_id: String,
    hardware_lane: String,
    writer_started_at_utc: String,
    writer_completed_at_utc: String,
}

fn simulator_ffb_output_artifact_provenance_matches(
    lane: &Path,
    path: &str,
    pid: u16,
    receipt: &Value,
) -> bool {
    if json_bool(receipt, "output_log_provenance_valid") != Some(true)
        || json_bool(receipt, "no_hid_device_opened") != Some(false)
        || json_bool(receipt, "no_ffb_writes") != Some(false)
    {
        return false;
    }

    let Some(records) = read_receipt_artifact_records(
        lane,
        path,
        &["output_log", "records", "commands", "reports"],
    ) else {
        return false;
    };
    let Some(provenance) = simulator_ffb_output_provenance_for_records(lane, &records, pid) else {
        return false;
    };

    json_string(receipt, "writer_command") == Some(provenance.writer_command.as_str())
        && json_string(receipt, "writer_session_id") == Some(provenance.writer_session_id.as_str())
        && json_string(receipt, "writer_device_path") == Some(provenance.device_path.as_str())
        && json_string(receipt, "writer_product_id") == Some(provenance.product_id.as_str())
        && json_string(receipt, "writer_hardware_lane") == Some(provenance.hardware_lane.as_str())
        && json_string(receipt, "writer_started_at_utc")
            == Some(provenance.writer_started_at_utc.as_str())
        && json_string(receipt, "writer_completed_at_utc")
            == Some(provenance.writer_completed_at_utc.as_str())
}

fn simulator_ffb_output_provenance_for_records(
    lane: &Path,
    records: &[Value],
    pid: u16,
) -> Option<SimulatorFfbOutputProvenance> {
    let mut summary: Option<SimulatorFfbOutputProvenance> = None;
    for record in records {
        let writer_command = json_string(record, "writer_command")
            .or_else(|| json_string(record, "producer_command"))?;
        let writer_session_id = json_string(record, "writer_session_id")
            .or_else(|| json_string(record, "session_id"))?;
        let hardware_lane = json_string(record, "writer_hardware_lane")
            .or_else(|| json_string(record, "moza_lane"))?;
        let device_path = json_string(record, "writer_device_path")
            .or_else(|| json_string(record, "device_path"))
            .or_else(|| {
                record
                    .get("device")
                    .and_then(|device| json_string(device, "path"))
            })?;
        let product_id = json_string(record, "writer_product_id")
            .or_else(|| json_string(record, "product_id"))
            .or_else(|| {
                record
                    .get("device")
                    .and_then(|device| json_string(device, "product_id"))
            })?;
        let writer_started_at_utc = json_string(record, "writer_started_at_utc")?;
        let writer_completed_at_utc = json_string(record, "writer_completed_at_utc")?;
        let vendor_id = json_string(record, "vendor_id").or_else(|| {
            record
                .get("device")
                .and_then(|device| json_string(device, "vendor_id"))
        });
        let output_capable = json_bool(record, "output_capable").or_else(|| {
            record
                .get("device")
                .and_then(|device| json_bool(device, "output_capable"))
        });

        let record_safe = simulator_ffb_writer_command_is_safe(writer_command)
            && !writer_session_id.trim().is_empty()
            && path_value_matches(lane, Some(hardware_lane))
            && !device_path.trim().is_empty()
            && vendor_id == Some(MOZA_VENDOR_HEX)
            && parse_hex_selector(product_id) == Some(pid)
            && utc_timestamp_pair_is_ordered(writer_started_at_utc, writer_completed_at_utc)
            && output_capable == Some(true)
            && json_bool(record, "hardware_output_enabled") == Some(true)
            && json_bool(record, "no_hid_device_opened") == Some(false)
            && json_bool(record, "no_ffb_writes") == Some(false)
            && json_bool(record, "high_torque") == Some(false)
            && json_bool(record, "no_high_torque") == Some(true)
            && no_out_of_scope_device_commands(record);
        if !record_safe {
            return None;
        }

        let next = SimulatorFfbOutputProvenance {
            writer_command: writer_command.to_string(),
            writer_session_id: writer_session_id.to_string(),
            device_path: device_path.to_string(),
            product_id: product_id.to_string(),
            hardware_lane: hardware_lane.to_string(),
            writer_started_at_utc: writer_started_at_utc.to_string(),
            writer_completed_at_utc: writer_completed_at_utc.to_string(),
        };
        if let Some(previous) = &summary {
            if previous.writer_command != next.writer_command
                || previous.writer_session_id != next.writer_session_id
                || previous.device_path != next.device_path
                || previous.product_id != next.product_id
                || previous.hardware_lane != next.hardware_lane
                || previous.writer_started_at_utc != next.writer_started_at_utc
                || previous.writer_completed_at_utc != next.writer_completed_at_utc
            {
                return None;
            }
        } else {
            summary = Some(next);
        }
    }

    summary
}

fn simulator_ffb_writer_command_is_safe(command: &str) -> bool {
    command == SIMULATOR_FFB_WRITER_COMMAND || command.starts_with("wheeld --hardware-lane ")
}

#[derive(Default)]
struct SimulatorFfbClearEvents {
    stop: Option<usize>,
    pause: Option<usize>,
    game_exit: Option<usize>,
    mode_mismatch: Option<usize>,
}

impl SimulatorFfbClearEvents {
    fn record(&mut self, record: &Value, index: usize) {
        match json_string(record, "clear_event") {
            Some("stop" | "stop_requested") => {
                self.stop.get_or_insert(index);
            }
            Some("pause" | "game_paused") => {
                self.pause.get_or_insert(index);
            }
            Some("game_exit" | "game_exited") => {
                self.game_exit.get_or_insert(index);
            }
            Some("mode_mismatch") => {
                self.mode_mismatch.get_or_insert(index);
            }
            _ => {}
        }
    }

    fn all_ordered_before_final_zero(&self, final_zero_index: usize) -> bool {
        let (Some(stop), Some(pause), Some(game_exit), Some(mode_mismatch)) =
            (self.stop, self.pause, self.game_exit, self.mode_mismatch)
        else {
            return false;
        };
        stop < pause
            && pause < game_exit
            && game_exit < mode_mismatch
            && mode_mismatch < final_zero_index
    }
}

fn simulator_ffb_record_has_hid_write_metadata(record: &Value) -> bool {
    json_string(record, "transport") == Some("hid")
        && json_string(record, "hid_write_target") == Some("output_report")
        && json_bool(record, "hid_write_attempted") == Some(true)
}

fn simulator_ffb_record_links_telemetry(
    record: &Value,
    telemetry_link: &SimulatorFfbTelemetryLink,
) -> bool {
    let telemetry_sequence = json_u64(record, "telemetry_sequence");
    let input_ffb_scalar = json_f64(record, "input_ffb_scalar");
    let linked_telemetry_ffb_scalar = telemetry_sequence.and_then(|sequence| {
        telemetry_link
            .ffb_scalars_by_sequence
            .get(&sequence)
            .copied()
    });

    telemetry_sequence
        .zip(linked_telemetry_ffb_scalar)
        .map(|(sequence, telemetry_ffb_scalar)| {
            sequence < telemetry_link.snapshot_count
                && input_ffb_scalar
                    .map(|value| {
                        value.is_finite()
                            && (-1.0..=1.0).contains(&value)
                            && floats_nearly_equal(value, telemetry_ffb_scalar)
                    })
                    .unwrap_or(false)
        })
        .unwrap_or(false)
        && json_string(record, "input_telemetry_artifact") == Some(telemetry_link.artifact.as_str())
        && json_u64(record, "input_telemetry_snapshot_count") == Some(telemetry_link.snapshot_count)
        && json_string(record, "input_telemetry_recorder_session_id")
            == Some(telemetry_link.recorder_session_id.as_str())
        && json_string(record, "input_telemetry_game") == Some(telemetry_link.game.as_str())
        && json_string(record, "input_telemetry_source")
            == Some(telemetry_link.telemetry_source.as_str())
}

fn floats_nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-6
}

fn simulator_ffb_output_sign_matches_input(record: &Value, percent: f64) -> bool {
    let Some(input_ffb_scalar) = json_f64(record, "input_ffb_scalar") else {
        return false;
    };
    input_ffb_scalar != 0.0 && input_ffb_scalar.signum() == percent.signum()
}

fn read_telemetry_artifact_records(lane: &Path, path: &str) -> Option<Vec<Value>> {
    let artifact_path = resolve_receipt_path(lane, path)?;
    if path.trim().is_empty() || !artifact_path.is_file() {
        return None;
    }

    if path_is_jsonl(&artifact_path) {
        return read_jsonl_values(&artifact_path).ok();
    }

    let value = read_json_path(&artifact_path).ok()?;
    let metadata_frame_count = value
        .get("metadata")
        .and_then(|metadata| json_u64(metadata, "frame_count"));
    let records = json_artifact_records_for_keys(
        value,
        &["frames", "records", "snapshots", "normalized_snapshots"],
    )?;
    if metadata_frame_count.is_some_and(|count| usize::try_from(count).ok() != Some(records.len()))
    {
        return None;
    }
    Some(records)
}

fn read_receipt_artifact_records(
    lane: &Path,
    path: &str,
    object_keys: &[&str],
) -> Option<Vec<Value>> {
    let artifact_path = resolve_receipt_path(lane, path)?;
    if path.trim().is_empty() || !artifact_path.is_file() {
        return None;
    }

    if path_is_jsonl(&artifact_path) {
        read_jsonl_values(&artifact_path).ok()
    } else {
        read_json_path(&artifact_path)
            .ok()
            .and_then(|value| json_artifact_records_for_keys(value, object_keys))
    }
}

fn json_artifact_records_for_keys(value: Value, object_keys: &[&str]) -> Option<Vec<Value>> {
    match value {
        Value::Array(values) => Some(values),
        Value::Object(mut map) => {
            for key in object_keys {
                if let Some(Value::Array(records)) = map.remove(*key) {
                    return Some(records);
                }
            }
            None
        }
        _ => None,
    }
}

fn path_is_jsonl(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
        .unwrap_or(false)
}

struct DirectTorqueArtifactRecord {
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
    percent: f64,
}

fn direct_torque_artifact_record(record: &Value) -> Option<DirectTorqueArtifactRecord> {
    let payload_hex = json_string(record, "payload_hex")?;
    let bytes = parse_hex_bytes(payload_hex).ok()?;
    let payload: [u8; REPORT_LEN] = bytes.try_into().ok()?;
    let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
    let flags = payload[3];
    let motor_enabled = flags & 0x01 != 0;
    let percent = json_f64(record, "percent").or_else(|| json_f64(record, "output_percent"))?;

    if payload[0] != 0x20
        || json_string(record, "report_id") != Some(DIRECT_TORQUE_REPORT_ID)
        || json_i64(record, "torque_raw") != Some(i64::from(torque_raw))
        || json_u64(record, "flags") != Some(u64::from(flags))
        || json_bool(record, "motor_enabled") != Some(motor_enabled)
        || !percent.is_finite()
    {
        return None;
    }

    Some(DirectTorqueArtifactRecord {
        torque_raw,
        flags,
        motor_enabled,
        percent,
    })
}

fn telemetry_record_has_normalized_fields(record: &Value) -> bool {
    let Some(data) = telemetry_record_data(record) else {
        return false;
    };

    let speed_ms = json_f64(data, "speed_ms");
    let steering_angle = json_f64(data, "steering_angle");
    let throttle = json_f64(data, "throttle");
    let brake = json_f64(data, "brake");
    let rpm = json_f64(data, "rpm");
    let gear = json_i64(data, "gear");
    let ffb_scalar = json_f64(data, "ffb_scalar");

    speed_ms
        .map(|value| value.is_finite() && (0.0..=200.0).contains(&value))
        .unwrap_or(false)
        && steering_angle
            .map(|value| value.is_finite() && value.abs() <= 100.0)
            .unwrap_or(false)
        && throttle
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && brake
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && rpm
            .map(|value| value.is_finite() && (0.0..=30_000.0).contains(&value))
            .unwrap_or(false)
        && gear
            .map(|value| (-1..=15).contains(&value))
            .unwrap_or(false)
        && ffb_scalar
            .map(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
            .unwrap_or(false)
}

fn telemetry_record_data(record: &Value) -> Option<&Value> {
    for key in [
        "data",
        "normalized",
        "normalized_snapshot",
        "snapshot",
        "telemetry",
    ] {
        if let Some(value) = record.get(key).filter(|value| value.is_object()) {
            return Some(value);
        }
    }
    record.is_object().then_some(record)
}

fn telemetry_record_u64(record: &Value, key: &str) -> Option<u64> {
    json_u64(record, key)
        .or_else(|| telemetry_record_data(record).and_then(|data| json_u64(data, key)))
}

fn telemetry_record_f64(record: &Value, key: &str) -> Option<f64> {
    json_f64(record, key)
        .or_else(|| telemetry_record_data(record).and_then(|data| json_f64(data, key)))
}

fn telemetry_record_string<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    json_string(record, key)
        .or_else(|| telemetry_record_data(record).and_then(|data| json_string(data, key)))
}

fn telemetry_record_bool(record: &Value, key: &str) -> Option<bool> {
    json_bool(record, key)
        .or_else(|| telemetry_record_data(record).and_then(|data| json_bool(data, key)))
}

fn read_json_value(lane: &Path, relative_path: &str) -> Result<Value> {
    let path = lane.join(relative_path);
    read_json_path(&path)
}

fn read_json_path(path: &Path) -> Result<Value> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("invalid JSON in '{}'", path.display()))
}

fn receipt_file_crc32(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    Ok(format!("0x{:08X}", hasher.finalize()))
}

fn resolve_receipt_path(lane: &Path, path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = Path::new(trimmed);
    if candidate
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }

    Some(lane.join(candidate))
}

fn resolve_fixture_out_path(lane: &Path, path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = Path::new(trimmed);
    if candidate
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }

    if repo_relative_moza_fixture_path(candidate) {
        return fixture_repo_root_for_lane(lane).map(|root| root.join(candidate));
    }

    Some(lane.join(candidate))
}

fn repo_relative_moza_fixture_path(path: &Path) -> bool {
    path.starts_with(
        Path::new("crates")
            .join("hid-moza-protocol")
            .join("fixtures"),
    ) && path.extension().and_then(|extension| extension.to_str()) == Some("json")
}

fn fixture_repo_root_for_lane(lane: &Path) -> Option<PathBuf> {
    let absolute_lane = std::path::absolute(lane).ok()?;
    absolute_lane
        .ancestors()
        .find(|ancestor| ancestor.join("crates/hid-moza-protocol/fixtures").is_dir())
        .map(Path::to_path_buf)
}

fn moza_lane_manifest_artifacts_value() -> Value {
    serde_json::json!({
        "manifest": "manifest.json",
        "device_list": "device-list.json",
        "hid_list": "hid-list.json",
        "moza_probe": "moza-probe.json",
        "hardware_doctor": "hardware-doctor.json",
        "descriptor": "descriptor.json",
        "captures_dir": "captures",
        "capture_r5_idle": "captures/r5-idle.jsonl",
        "capture_r5_steering_sweep": "captures/r5-steering-sweep.jsonl",
        "capture_r5_throttle_only_sweep": "captures/r5-throttle-only-sweep.jsonl",
        "capture_r5_brake_only_sweep": "captures/r5-brake-only-sweep.jsonl",
        "capture_r5_clutch_only_sweep": "captures/r5-clutch-only-sweep.jsonl",
        "capture_r5_handbrake_only_sweep": "captures/r5-handbrake-only-sweep.jsonl",
        "capture_r5_aggregated_idle_after_controls": "captures/r5-aggregated-idle-after-controls.jsonl",
        "capture_ks_controls": "captures/ks-controls.jsonl",
        "capture_es_controls": "captures/es-controls.jsonl",
        "parser_fixture_validation": "parser-fixture-validation.json",
        "fixture_promotion": "fixture-promotion.json",
        "passive_verification": "passive-verification.json",
        "passive_manifest_promotion": "manifest-promotion-passive.json",
        "passive_lane_audit": "lane-audit-passive.json",
        "init_off": "init-off.json",
        "init_standard": "init-standard.json",
        "moza_status": "moza-status.json",
        "device_status": "device-status.json",
        "support_bundle": "support-bundle.json",
        "zero_torque_proof": "zero-torque-proof.json",
        "watchdog_proof": "watchdog-proof.json",
        "disconnect_proof": "disconnect-proof.json",
        "zero_verification": "zero-verification.json",
        "zero_manifest_promotion": "manifest-promotion-zero.json",
        "zero_lane_audit": "lane-audit-zero.json",
        "low_torque_proof": "low-torque-proof.json",
        "pit_house_coexistence": "pit-house-coexistence.json",
        "simulator_telemetry_proof": "simulator-telemetry-proof.json",
        "simulator_ffb_smoke": "simulator-ffb-smoke.json",
        "smoke_ready_verification": "smoke-ready-verification.json",
        "smoke_ready_manifest_promotion": "manifest-promotion-smoke-ready.json",
        "smoke_ready_lane_audit": "lane-audit-smoke-ready.json"
    })
}

fn moza_lane_manifest_topology_value(wheelbase_pid: u16) -> Value {
    serde_json::json!({
        "primary_input_path": "wheelbase_hub",
        "endpoints": [
            {
                "id": "moza-r5-if2",
                "kind": "wheelbase_hub",
                "vendor_id": MOZA_VENDOR_HEX,
                "product_id": hex_u16(wheelbase_pid),
                "interface_number": 2,
                "usage_page": "0x0001",
                "usage": "0x0004",
                "output_capable": true
            }
        ],
        "logical_controls": {
            "steering": {
                "role": "steering",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-steering-sweep.jsonl",
                "semantic_status": "deferred"
            },
            "ks_rim_controls": {
                "role": "rim_controls",
                "rim": "KS",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/ks-controls.jsonl",
                "semantic_status": "deferred"
            },
            "es_rim_controls": {
                "role": "rim_controls",
                "rim": "ES",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/es-controls.jsonl",
                "semantic_status": "deferred"
            },
            "throttle": {
                "role": "throttle",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-throttle-only-sweep.jsonl",
                "semantic_status": "deferred"
            },
            "brake": {
                "role": "brake",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-brake-only-sweep.jsonl",
                "semantic_status": "deferred"
            },
            "clutch": {
                "role": "clutch",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-clutch-only-sweep.jsonl",
                "semantic_status": "deferred"
            },
            "handbrake": {
                "role": "handbrake",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-handbrake-only-sweep.jsonl",
                "semantic_status": "deferred"
            }
        },
        "notes": [
            "Primary Moza input evidence is captured through the R5 wheelbase hub aggregated HID endpoint.",
            "Standalone SR-P or HBP USB endpoints are optional direct-plug evidence only when declared in topology."
        ]
    })
}

fn moza_lane_manifest_value(
    wheelbase_pid: u16,
    operator: &str,
    completion_state: &str,
    hardware_validated: bool,
    simulator_validated: bool,
) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "lane": "moza-r5-windows-usb",
        "completion_state": completion_state,
        "generated_at_utc": now_utc(),
        "operator": operator,
        "platform": {
            "os": "Windows",
            "transport": {
                "hid": true,
                "serial_config": false
            }
        },
        "hardware": {
            "wheelbase": "Moza R5",
            "wheelbase_pid": hex_u16(wheelbase_pid),
            "rims": ["KS", "ES"],
            "pedals": ["SR-P"],
            "handbrake": "HBP"
        },
        "topology": moza_lane_manifest_topology_value(wheelbase_pid),
        "claims": {
            "ffb": "staged",
            "high_torque": false,
            "pit_house_coexistence": "tested_separately"
        },
        "hardware_validated": hardware_validated,
        "simulator_validated": simulator_validated,
        "high_torque_validated": false,
        "release_ready": false,
        "artifacts": moza_lane_manifest_artifacts_value(),
        "notes": [
            "No compatibility claim is made until receipts exist and the verifier passes.",
            "No serial configuration, firmware update, or DFU command is in scope."
        ]
    })
}

fn moza_receipt_template(kind: MozaReceiptTemplateKind) -> Value {
    match kind {
        MozaReceiptTemplateKind::PitHouse => serde_json::json!({
            "success": false,
            "template": true,
            "evidence_status": "operator_pending",
            "command": "wheelctl moza receipt-template",
            "generated_at_utc": now_utc(),
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "direct_requires_ack": false,
            "firmware_page_blocks_high_risk": false,
            "shared_control_risk": "operator_pending",
            "cases": [
                {
                    "case": "pit_house_closed",
                    "observed": false,
                    "result": "staged_handshake_ok",
                    "high_torque": false,
                    "artifact": "pit-house-closed.json",
                    "pit_house_observation_artifact": "pit-house-observation-closed.json",
                    "evidence": ""
                },
                {
                    "case": "pit_house_open_idle_standard",
                    "observed": false,
                    "result": "standard_ok",
                    "high_torque": false,
                    "artifact": "pit-house-open-standard.json",
                    "pit_house_observation_artifact": "pit-house-observation-open-standard.json",
                    "evidence": ""
                },
                {
                    "case": "pit_house_open_direct",
                    "observed": false,
                    "result": "blocked",
                    "blocked": false,
                    "operator_ack_required": false,
                    "high_torque": false,
                    "artifact": "pit-house-direct-blocked.json",
                    "pit_house_observation_artifact": "pit-house-observation-open-direct.json",
                    "evidence": ""
                },
                {
                    "case": "pit_house_mode_change_during_run",
                    "observed": false,
                    "result": "mismatch_detected",
                    "mismatch_detected": false,
                    "failed_safe": false,
                    "high_torque": false,
                    "artifact": "pit-house-mode-change.json",
                    "pit_house_observation_artifact": "pit-house-observation-mode-change.json",
                    "evidence": ""
                },
                {
                    "case": "pit_house_firmware_update_page_open",
                    "observed": false,
                    "result": "high_risk_refused",
                    "high_risk_refused": false,
                    "high_torque": false,
                    "artifact": "pit-house-firmware-page.json",
                    "pit_house_observation_artifact": "pit-house-observation-firmware-page.json",
                    "evidence": ""
                }
            ],
            "notes": [
                "Template only: keep success=false until every case is observed on real hardware.",
                "The bundle verifier rejects this file until observed fields and safety booleans are updated from real evidence."
            ]
        }),
        MozaReceiptTemplateKind::SimulatorTelemetry => serde_json::json!({
            "success": false,
            "template": true,
            "evidence_status": "operator_pending",
            "command": "wheelctl moza receipt-template",
            "generated_at_utc": now_utc(),
            "game": "",
            "telemetry_source": "operator_pending",
            "recorder_command": SIMULATOR_TELEMETRY_RECORDER_COMMAND,
            "recorder_session_id": "",
            "hardware_output_enabled": false,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "normalized_snapshot_count": 0,
            "duration_ms": 0,
            "recorder_artifact": "",
            "faults": ["operator_pending"],
            "notes": [
                "Template only: telemetry_source must be real_game or simhub_bridge.",
                "The bundle verifier rejects this file until real telemetry snapshots and a recorder artifact exist."
            ]
        }),
        MozaReceiptTemplateKind::SimulatorFfb => simulator_ffb_receipt_template_value(),
    }
}

fn simulator_ffb_receipt_template_value() -> Value {
    let mut receipt = serde_json::Map::new();
    receipt.insert("success".to_string(), Value::Bool(false));
    receipt.insert("template".to_string(), Value::Bool(true));
    receipt.insert(
        "evidence_status".to_string(),
        Value::String("operator_pending".to_string()),
    );
    receipt.insert(
        "command".to_string(),
        Value::String("wheelctl moza receipt-template".to_string()),
    );
    receipt.insert("generated_at_utc".to_string(), Value::String(now_utc()));
    receipt.insert("game".to_string(), Value::String(String::new()));
    receipt.insert(
        "telemetry_source".to_string(),
        Value::String("operator_pending".to_string()),
    );
    receipt.insert("hardware".to_string(), Value::String("moza-r5".to_string()));
    receipt.insert("ffb_mode".to_string(), Value::String("direct".to_string()));
    receipt.insert("descriptor_trusted".to_string(), Value::Bool(false));
    receipt.insert("descriptor_trust_observed".to_string(), Value::Bool(false));
    receipt.insert("explicit_operator_override".to_string(), Value::Bool(false));
    receipt.insert("high_torque".to_string(), Value::Bool(false));
    receipt.insert("no_high_torque".to_string(), Value::Bool(true));
    receipt.insert("no_hid_device_opened".to_string(), Value::Bool(false));
    receipt.insert("no_ffb_writes".to_string(), Value::Bool(false));
    receipt.insert("no_serial_config_commands".to_string(), Value::Bool(true));
    receipt.insert("no_firmware_or_dfu_commands".to_string(), Value::Bool(true));
    receipt.insert(
        "hardware_prerequisites_validated".to_string(),
        Value::Bool(false),
    );
    receipt.insert(
        "prerequisite_gates".to_string(),
        simulator_ffb_prerequisite_gate_template_value(),
    );
    receipt.insert(
        "prerequisite_artifacts".to_string(),
        simulator_ffb_prerequisite_artifact_template_value(),
    );
    receipt.insert(
        "device".to_string(),
        serde_json::json!({
            "vendor_id": "0x346E",
            "product_id": "0x0014",
            "product_name": "Moza R5",
            "output_capable": true
        }),
    );
    receipt.insert("hardware_output_enabled".to_string(), Value::Bool(false));
    receipt.insert("max_output_percent".to_string(), serde_json::json!(0.0));
    receipt.insert("max_abs_output_percent".to_string(), serde_json::json!(0.0));
    receipt.insert("watchdog_active".to_string(), Value::Bool(false));
    receipt.insert("watchdog_timeout_ms".to_string(), Value::Number(0.into()));
    receipt.insert("output_report_count".to_string(), Value::Number(0.into()));
    receipt.insert("nonzero_output_count".to_string(), Value::Number(0.into()));
    receipt.insert("zero_output_count".to_string(), Value::Number(0.into()));
    receipt.insert(
        "input_telemetry_artifact".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "input_telemetry_snapshot_count".to_string(),
        Value::Number(0.into()),
    );
    receipt.insert(
        "input_telemetry_recorder_session_id".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "output_log_artifact".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "output_log_provenance_valid".to_string(),
        Value::Bool(false),
    );
    receipt.insert(
        "writer_command".to_string(),
        Value::String(SIMULATOR_FFB_WRITER_COMMAND.to_string()),
    );
    receipt.insert(
        "writer_session_id".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "writer_started_at_utc".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "writer_completed_at_utc".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "writer_hardware_lane".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "writer_device_path".to_string(),
        Value::String(String::new()),
    );
    receipt.insert(
        "writer_product_id".to_string(),
        Value::String("0x0014".to_string()),
    );
    receipt.insert("final_zero_attempted".to_string(), Value::Bool(false));
    receipt.insert("final_zero_sent".to_string(), Value::Bool(false));
    receipt.insert(
        "final_zero_payload_hex".to_string(),
        Value::String(String::new()),
    );
    receipt.insert("stop_cleared_output".to_string(), Value::Bool(false));
    receipt.insert("pause_cleared_output".to_string(), Value::Bool(false));
    receipt.insert("game_exit_cleared_output".to_string(), Value::Bool(false));
    receipt.insert(
        "mode_mismatch_cleared_output".to_string(),
        Value::Bool(false),
    );
    receipt.insert(
        "faults".to_string(),
        serde_json::json!(["operator_pending"]),
    );
    receipt.insert(
        "notes".to_string(),
        serde_json::json!([
            "Template only: keep success=false until a bounded real simulator FFB smoke run completes.",
            "Direct mode requires descriptor_trusted=true or explicit_operator_override=true, high_torque=false, watchdog active, and final zero.",
            "Replace prerequisite_gates and prerequisite_artifacts with current same-lane zero/watchdog/disconnect/init/low-torque receipt summaries before running the smoke verifier."
        ]),
    );
    Value::Object(receipt)
}

fn simulator_ffb_prerequisite_gate_template_value() -> Value {
    Value::Array(
        SIMULATOR_FFB_PREREQUISITE_ARTIFACTS
            .iter()
            .map(|(gate, _)| {
                serde_json::json!({
                    "name": gate,
                    "status": "operator_pending",
                    "details": "replace with the passing gate summary from wheelctl moza verify-bundle"
                })
            })
            .collect(),
    )
}

fn simulator_ffb_prerequisite_artifact_template_value() -> Value {
    Value::Array(
        SIMULATOR_FFB_PREREQUISITE_ARTIFACTS
            .iter()
            .map(|(gate, path)| {
                serde_json::json!({
                    "gate": gate,
                    "path": path,
                    "generated_at_utc": "",
                    "receipt_crc32": ""
                })
            })
            .collect(),
    )
}

fn manifest_promotion_values(stage: MozaBundleStage) -> (&'static str, bool, bool) {
    match stage {
        MozaBundleStage::Passive => ("passive_capture_ready", false, false),
        MozaBundleStage::Zero => ("zero_torque_ready", false, false),
        MozaBundleStage::SmokeReady => ("real_hardware_smoke_ready", true, true),
    }
}

fn apply_manifest_promotion(
    manifest: &mut Value,
    completion_state: &str,
    hardware_validated: bool,
    simulator_validated: bool,
) -> Result<()> {
    let Some(map) = manifest.as_object_mut() else {
        return Err(anyhow!("manifest.json must contain a JSON object"));
    };

    map.insert(
        "completion_state".to_string(),
        Value::String(completion_state.to_string()),
    );
    map.insert(
        "hardware_validated".to_string(),
        Value::Bool(hardware_validated),
    );
    map.insert(
        "simulator_validated".to_string(),
        Value::Bool(simulator_validated),
    );
    map.insert("high_torque_validated".to_string(), Value::Bool(false));
    map.insert("release_ready".to_string(), Value::Bool(false));
    Ok(())
}

fn bundle_verification_summary_value(receipt: &BundleVerificationReceipt) -> Value {
    serde_json::json!({
        "success": receipt.success,
        "requested_stage": receipt.requested_stage,
        "missing_artifacts": receipt.missing_artifacts,
        "invalid_artifacts": receipt.invalid_artifacts,
        "failed_gates": receipt.failed_gates,
        "no_hid_device_opened": receipt.no_hid_device_opened,
        "no_ffb_writes": receipt.no_ffb_writes,
        "no_serial_config_commands": receipt.no_serial_config_commands,
        "no_firmware_or_dfu_commands": receipt.no_firmware_or_dfu_commands
    })
}

fn contains_any_key(value: &Value, keys: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            keys.iter().any(|forbidden| key == forbidden) || contains_any_key(value, keys)
        }),
        Value::Array(values) => values.iter().any(|value| contains_any_key(value, keys)),
        _ => false,
    }
}

fn count_r5_devices(value: &Value) -> usize {
    value
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| is_r5_device_value(device))
                .count()
        })
        .unwrap_or(0)
}

fn count_vendor_product_devices(value: &Value, vendor_id: u16, product_id: u16) -> usize {
    value
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| is_vendor_product_device_value(device, vendor_id, product_id))
                .count()
        })
        .unwrap_or(0)
}

fn is_r5_device_value(device: &Value) -> bool {
    is_moza_product_device_value(device, &[product_ids::R5_V1, product_ids::R5_V2])
}

fn is_moza_product_device_value(device: &Value, product_ids: &[u16]) -> bool {
    json_string(device, "vendor_id").and_then(parse_hex_selector) == Some(MOZA_VENDOR_ID)
        && json_string(device, "product_id")
            .and_then(parse_hex_selector)
            .is_some_and(|pid| product_ids.contains(&pid))
}

fn is_vendor_product_device_value(device: &Value, vendor_id: u16, product_id: u16) -> bool {
    json_string(device, "vendor_id").and_then(parse_hex_selector) == Some(vendor_id)
        && json_string(device, "product_id").and_then(parse_hex_selector) == Some(product_id)
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn json_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn stage_rank(stage: MozaBundleStage) -> u8 {
    match stage {
        MozaBundleStage::Passive => 0,
        MozaBundleStage::Zero => 1,
        MozaBundleStage::SmokeReady => 2,
    }
}

fn stage_label(stage: MozaBundleStage) -> &'static str {
    match stage {
        MozaBundleStage::Passive => "passive",
        MozaBundleStage::Zero => "zero",
        MozaBundleStage::SmokeReady => "smoke_ready",
    }
}

fn validate_capture_file(
    capture: &Path,
    pid_override: Option<u16>,
) -> Result<CaptureValidationReceipt> {
    let file =
        File::open(capture).with_context(|| format!("failed to open '{}'", capture.display()))?;
    let reader = BufReader::new(file);
    let mut summary = CaptureValidationSummary::default();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                summary.record_line_error(line_no, format!("failed to read line: {e}"));
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        summary.total_reports += 1;

        let parsed_line: CapturedInputReportLine = match serde_json::from_str(&line) {
            Ok(report) => report,
            Err(e) => {
                summary.record_rejection(line_no, format!("invalid JSON capture line: {e}"));
                continue;
            }
        };

        let pid = match pid_override.or_else(|| {
            parsed_line
                .product_id
                .as_deref()
                .and_then(parse_hex_selector)
        }) {
            Some(pid) => pid,
            None => {
                summary.record_rejection(
                    line_no,
                    "missing or invalid product_id; pass --pid to override".to_string(),
                );
                continue;
            }
        };

        increment_count(&mut summary.product_ids, hex_u16(pid));
        let data = match parsed_line.decode_data() {
            Ok(bytes) => bytes,
            Err(e) => {
                summary.record_rejection(line_no, e);
                continue;
            }
        };

        if let Some(expected_len) = parsed_line.report_len
            && expected_len != data.len()
        {
            summary.record_rejection(
                line_no,
                format!(
                    "report_len mismatch: declared {expected_len}, decoded {}",
                    data.len()
                ),
            );
            continue;
        }

        let report_id = data.first().copied().unwrap_or(0);
        increment_count(&mut summary.report_ids, hex_u8(report_id));
        increment_count(&mut summary.report_lengths, data.len().to_string());
        if parsed_line.has_capture_input_metadata(&data, pid) {
            summary.capture_input_format_reports += 1;
        }

        let protocol = MozaProtocol::new_with_config(pid, FfbMode::Off, false);
        let Some(state) = protocol.parse_input_state(&data) else {
            summary.record_rejection(
                line_no,
                format!("Moza parser rejected report for PID {}", hex_u16(pid)),
            );
            continue;
        };

        summary.record_parsed(pid, &state, &data);
    }

    let success = summary.total_reports > 0
        && summary.rejected_reports == 0
        && summary.line_errors.is_empty();
    let notes = if success {
        vec![
            "capture validation replays JSONL input bytes through Moza parsers only; no HID device is opened".to_string(),
        ]
    } else {
        vec![
            "capture validation did not prove every line is parseable; inspect line_errors before using as a fixture".to_string(),
            "validation is offline and performs no HID reads, output writes, feature reports, or FFB actions".to_string(),
        ]
    };

    Ok(CaptureValidationReceipt {
        success,
        command: "wheelctl moza validate-capture",
        generated_at_utc: now_utc(),
        capture: capture.display().to_string(),
        pid_override: pid_override.map(hex_u16),
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        total_reports: summary.total_reports,
        parsed_reports: summary.parsed_reports,
        rejected_reports: summary.rejected_reports,
        capture_input_format_reports: summary.capture_input_format_reports,
        all_reports_have_capture_input_metadata: summary.total_reports > 0
            && summary.capture_input_format_reports == summary.total_reports,
        product_ids: summary.product_ids,
        parsed_by_category: summary.parsed_by_category,
        report_ids: summary.report_ids,
        report_lengths: summary.report_lengths,
        axis_ranges: summary.axis_ranges,
        line_errors: summary.line_errors,
        line_errors_truncated: summary.line_errors_truncated,
        notes,
    })
}

fn analyze_capture_file(capture: &Path) -> Result<CaptureAnalysisReceipt> {
    let file =
        File::open(capture).with_context(|| format!("failed to open '{}'", capture.display()))?;
    let reader = BufReader::new(file);
    let mut total_reports = 0usize;
    let mut decoded_reports = 0usize;
    let mut rejected_reports = 0usize;
    let mut capture_input_format_reports = 0usize;
    let mut product_ids = BTreeMap::new();
    let mut report_ids = BTreeMap::new();
    let mut report_lengths = BTreeMap::new();
    let mut byte_ranges = Vec::<AxisRange>::new();
    let mut word_ranges_le = Vec::<AxisRange>::new();
    let mut line_errors = Vec::new();
    let mut line_errors_truncated = false;

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                record_capture_analysis_error(
                    &mut line_errors,
                    &mut line_errors_truncated,
                    line_no,
                    format!("failed to read line: {e}"),
                );
                rejected_reports += 1;
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        total_reports += 1;

        let parsed_line: CapturedInputReportLine = match serde_json::from_str(&line) {
            Ok(report) => report,
            Err(e) => {
                record_capture_analysis_error(
                    &mut line_errors,
                    &mut line_errors_truncated,
                    line_no,
                    format!("invalid JSON capture line: {e}"),
                );
                rejected_reports += 1;
                continue;
            }
        };

        let pid = parsed_line
            .product_id
            .as_deref()
            .and_then(parse_hex_selector);
        if let Some(pid) = pid {
            increment_count(&mut product_ids, hex_u16(pid));
        }

        let data = match parsed_line.decode_data() {
            Ok(bytes) => bytes,
            Err(e) => {
                record_capture_analysis_error(
                    &mut line_errors,
                    &mut line_errors_truncated,
                    line_no,
                    e,
                );
                rejected_reports += 1;
                continue;
            }
        };

        if let Some(expected_len) = parsed_line.report_len
            && expected_len != data.len()
        {
            record_capture_analysis_error(
                &mut line_errors,
                &mut line_errors_truncated,
                line_no,
                format!(
                    "report_len mismatch: declared {expected_len}, decoded {}",
                    data.len()
                ),
            );
            rejected_reports += 1;
            continue;
        }

        decoded_reports += 1;
        let report_id = data.first().copied().unwrap_or(0);
        increment_count(&mut report_ids, hex_u8(report_id));
        increment_count(&mut report_lengths, data.len().to_string());
        if pid
            .map(|pid| parsed_line.has_capture_input_metadata(&data, pid))
            .unwrap_or(false)
        {
            capture_input_format_reports += 1;
        }

        for (index, value) in data.iter().copied().enumerate() {
            update_range_at(&mut byte_ranges, index, u16::from(value));
        }

        for (index, pair) in data.windows(2).enumerate() {
            let value = u16::from_le_bytes([pair[0], pair[1]]);
            update_range_at(&mut word_ranges_le, index, value);
        }
    }

    let byte_ranges = materialize_byte_ranges(byte_ranges);
    let word_ranges_le = materialize_word_ranges(word_ranges_le);
    let moving_bytes = byte_ranges
        .iter()
        .filter(|range| range.changed)
        .map(|range| range.index)
        .collect::<Vec<_>>();
    let moving_words_le = word_ranges_le
        .iter()
        .filter(|range| range.changed)
        .map(|range| range.start_index)
        .collect::<Vec<_>>();
    let success = total_reports > 0 && rejected_reports == 0 && line_errors.is_empty();
    let notes = if success {
        vec![
            "capture analysis reads stored JSONL reports only; no HID device is opened".to_string(),
            "byte and word ranges are diagnostic evidence only and do not assign control semantics"
                .to_string(),
        ]
    } else {
        vec![
            "capture analysis did not decode every line; inspect line_errors before using movement evidence".to_string(),
            "analysis is offline and performs no HID reads, output writes, feature reports, or FFB actions".to_string(),
        ]
    };

    Ok(CaptureAnalysisReceipt {
        success,
        command: "wheelctl moza analyze-capture",
        generated_at_utc: now_utc(),
        capture: capture.display().to_string(),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        total_reports,
        decoded_reports,
        rejected_reports,
        capture_input_format_reports,
        all_reports_have_capture_input_metadata: total_reports > 0
            && capture_input_format_reports == total_reports,
        product_ids,
        report_ids,
        report_lengths,
        moving_byte_count: moving_bytes.len(),
        moving_bytes,
        byte_ranges,
        moving_word_le_count: moving_words_le.len(),
        moving_words_le,
        word_ranges_le,
        line_errors,
        line_errors_truncated,
        notes,
    })
}

fn analyze_lane_captures(lane: &Path) -> Result<LaneCaptureAnalysisReceipt> {
    let requirements = passive_capture_requirements_for_lane(lane);
    let idle = analyze_capture_file(&lane.join("captures/r5-idle.jsonl"))
        .context("failed to analyze captures/r5-idle.jsonl")?;
    let idle_moving_bytes = idle.moving_bytes.iter().copied().collect::<BTreeSet<_>>();
    let idle_moving_words_le = idle
        .moving_words_le
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    let mut captures = Vec::new();
    let mut analyzed_capture_count = 0usize;
    let mut total_reports = 0usize;
    let mut decoded_reports = 0usize;
    let mut rejected_reports = 0usize;

    for requirement in &requirements {
        let path = lane.join(requirement.relative_path);
        let analysis = match analyze_capture_file(&path) {
            Ok(analysis) => analysis,
            Err(error) => {
                captures.push(LaneCaptureAnalysisEntry::failed(
                    requirement,
                    format!("failed to analyze {}: {error}", requirement.relative_path),
                ));
                continue;
            }
        };

        analyzed_capture_count += usize::from(analysis.success);
        total_reports = total_reports.saturating_add(analysis.total_reports);
        decoded_reports = decoded_reports.saturating_add(analysis.decoded_reports);
        rejected_reports = rejected_reports.saturating_add(analysis.rejected_reports);

        let unique_moving_bytes = analysis
            .moving_bytes
            .iter()
            .copied()
            .filter(|index| !idle_moving_bytes.contains(index))
            .collect::<Vec<_>>();
        let unique_moving_words_le = analysis
            .moving_words_le
            .iter()
            .copied()
            .filter(|index| !idle_moving_words_le.contains(index))
            .collect::<Vec<_>>();

        let validation = match validate_capture_file(&path, None) {
            Ok(validation) => validation,
            Err(error) => {
                let mut missing_requirements = analysis
                    .line_errors
                    .iter()
                    .map(|line_error| format!("line {}: {}", line_error.line, line_error.error))
                    .collect::<Vec<_>>();
                missing_requirements.push(format!(
                    "failed to validate {}: {error}",
                    requirement.relative_path
                ));
                captures.push(LaneCaptureAnalysisEntry {
                    capture: requirement.relative_path.to_string(),
                    fixture_id: requirement.fixture_id.to_string(),
                    success: false,
                    total_reports: analysis.total_reports,
                    decoded_reports: analysis.decoded_reports,
                    rejected_reports: analysis.rejected_reports,
                    moving_bytes: analysis.moving_bytes,
                    unique_moving_bytes_vs_idle: unique_moving_bytes,
                    moving_words_le: analysis.moving_words_le,
                    unique_moving_words_le_vs_idle: unique_moving_words_le,
                    moving_required_axes: Vec::new(),
                    control_evidence_ok: false,
                    missing_requirements,
                });
                continue;
            }
        };
        let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
        let evaluation =
            evaluate_passive_capture_requirement(requirement, &validation, &expected_product_ids);
        let moving_required_axes = moving_required_axes(requirement, &validation.axis_ranges);
        let control_evidence_ok = required_control_evidence_ok(requirement, &evaluation);

        captures.push(LaneCaptureAnalysisEntry {
            capture: requirement.relative_path.to_string(),
            fixture_id: requirement.fixture_id.to_string(),
            success: analysis.success && validation.success,
            total_reports: analysis.total_reports,
            decoded_reports: analysis.decoded_reports,
            rejected_reports: analysis.rejected_reports,
            moving_bytes: analysis.moving_bytes,
            unique_moving_bytes_vs_idle: unique_moving_bytes,
            moving_words_le: analysis.moving_words_le,
            unique_moving_words_le_vs_idle: unique_moving_words_le,
            moving_required_axes,
            control_evidence_ok,
            missing_requirements: evaluation.missing_requirements,
        });
    }

    let failed_captures = captures
        .iter()
        .filter(|capture| !capture.success)
        .map(|capture| capture.capture.clone())
        .collect::<Vec<_>>();
    let missing_control_evidence = captures
        .iter()
        .filter(|capture| !capture.control_evidence_ok)
        .map(|capture| capture.capture.clone())
        .collect::<Vec<_>>();
    let role_evidence = lane_role_evidence_entries(lane, &captures);
    let success = failed_captures.is_empty() && missing_control_evidence.is_empty();
    let safe_diagnostics = lane_capture_safe_diagnostics(lane, &captures);

    Ok(LaneCaptureAnalysisReceipt {
        success,
        command: "wheelctl moza analyze-lane",
        generated_at_utc: now_utc(),
        lane: lane.display().to_string(),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        required_capture_count: requirements.len(),
        analyzed_capture_count,
        total_reports,
        decoded_reports,
        rejected_reports,
        idle_capture: idle.capture,
        idle_moving_bytes: idle.moving_bytes,
        idle_moving_words_le: idle.moving_words_le,
        failed_captures,
        missing_control_evidence,
        safe_diagnostics,
        role_evidence,
        captures,
        notes: vec![
            "analyze-lane reads stored JSONL reports only; no HID device is opened".to_string(),
            "unique_moving_* fields compare each capture to r5-idle and are diagnostic only"
                .to_string(),
            "control_evidence_ok uses the same parser-visible requirements as passive validation"
                .to_string(),
            "role_evidence derives semantic_status from manifest topology and parser-visible capture evidence; it does not promote lane receipts".to_string(),
        ],
    })
}

fn lane_capture_safe_diagnostics(
    lane: &Path,
    captures: &[LaneCaptureAnalysisEntry],
) -> Vec<String> {
    let mut diagnostics = Vec::new();

    let Some(throttle) = captures
        .iter()
        .find(|capture| capture.capture == "captures/r5-throttle-only-sweep.jsonl")
    else {
        return diagnostics;
    };

    if throttle.success
        && !throttle.control_evidence_ok
        && throttle.moving_required_axes.is_empty()
        && throttle
            .unique_moving_bytes_vs_idle
            .iter()
            .all(|byte| matches!(*byte, 38..=41))
    {
        diagnostics.push(
            "captures/r5-throttle-only-sweep.jsonl parsed cleanly, but only idle/trailer bytes moved versus r5-idle; do not recapture blindly until the throttle physical/vendor path is checked."
                .to_string(),
        );
        if lane_has_single_observed_moza_hid_endpoint(lane) {
            diagnostics.push(
                "observe-only HID/PnP receipts show only the R5 HID game-controller endpoint; throttle is not visible on an alternate Moza HID endpoint, and the visible Moza serial/COM interface is diagnostic topology only and must not be probed or configured in the passive lane."
                    .to_string(),
            );
        }
    }

    diagnostics
}

fn lane_has_single_observed_moza_hid_endpoint(lane: &Path) -> bool {
    hid_list_moza_hid_device_count(lane) == Some(1)
        && hardware_doctor_moza_hid_interface_count(lane).is_none_or(|count| count == 1)
}

fn hid_list_moza_hid_device_count(lane: &Path) -> Option<usize> {
    let receipt = read_json_value(lane, "hid-list.json").ok()?;
    receipt
        .get("devices")
        .and_then(Value::as_array)
        .map(|devices| {
            devices
                .iter()
                .filter(|device| {
                    json_string(device, "vendor_id").and_then(parse_hex_selector)
                        == Some(MOZA_VENDOR_ID)
                })
                .count()
        })
}

fn hardware_doctor_moza_hid_interface_count(lane: &Path) -> Option<usize> {
    let receipt = read_json_value(lane, "hardware-doctor.json").ok()?;
    receipt
        .get("windows_pnp")
        .and_then(|pnp| json_u64(pnp, "hid_interface_count"))
        .and_then(|count| usize::try_from(count).ok())
}

fn lane_role_evidence_entries(
    lane: &Path,
    captures: &[LaneCaptureAnalysisEntry],
) -> Vec<LaneRoleEvidenceEntry> {
    let Ok(manifest) = read_json_value(lane, "manifest.json") else {
        return Vec::new();
    };
    let Some(controls) = manifest
        .get("topology")
        .and_then(|topology| topology.get("logical_controls"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let capture_by_path = captures
        .iter()
        .map(|capture| (capture.capture.as_str(), capture))
        .collect::<BTreeMap<_, _>>();
    let mut entries = Vec::new();

    for (control_key, control) in controls {
        let required = json_bool(control, "required").unwrap_or(true);
        let evidence_capture = json_string(control, "evidence_capture").map(str::to_string);
        let capture = evidence_capture
            .as_deref()
            .and_then(|path| capture_by_path.get(path).copied());
        let moving_required_axes = capture
            .map(|capture| capture.moving_required_axes.clone())
            .unwrap_or_default();
        let missing_requirements = capture
            .map(|capture| capture.missing_requirements.clone())
            .unwrap_or_default();
        let mut notes = Vec::new();
        let semantic_status = role_semantic_status(required, capture, &mut notes);

        entries.push(LaneRoleEvidenceEntry {
            control: control_key.to_string(),
            role: json_string(control, "role")
                .unwrap_or("unknown")
                .to_string(),
            required,
            rim: json_string(control, "rim").map(str::to_string),
            source_endpoint: json_string(control, "source_endpoint").map(str::to_string),
            connection: json_string(control, "connection").map(str::to_string),
            evidence_capture,
            semantic_status,
            parser_visible: matches!(semantic_status, "proven" | "generic_aux"),
            moving_required_axes,
            missing_requirements,
            notes,
        });
    }

    entries
}

fn sync_role_status_receipt(lane: &Path, check: bool) -> Result<Value> {
    let analysis = analyze_lane_captures(lane)?;
    let mut manifest = read_json_value(lane, "manifest.json")?;
    let controls = manifest
        .pointer_mut("/topology/logical_controls")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("manifest.json topology is missing logical_controls object"))?;

    let mut status_updates = Vec::new();
    let mut stale_control_count = 0usize;

    for entry in &analysis.role_evidence {
        let control = controls
            .get_mut(&entry.control)
            .ok_or_else(|| anyhow!("missing logical control '{}'", entry.control))?;
        let old_status = json_string(control, "semantic_status").map(str::to_string);
        let changed = old_status.as_deref() != Some(entry.semantic_status);
        if changed {
            stale_control_count += 1;
            if !check {
                control["semantic_status"] = serde_json::json!(entry.semantic_status);
            }
        }

        status_updates.push(serde_json::json!({
            "control": entry.control,
            "role": entry.role,
            "required": entry.required,
            "source_endpoint": entry.source_endpoint,
            "connection": entry.connection,
            "evidence_capture": entry.evidence_capture,
            "old_semantic_status": old_status,
            "new_semantic_status": entry.semantic_status,
            "changed": changed,
            "parser_visible": entry.parser_visible,
            "moving_required_axes": entry.moving_required_axes,
            "missing_requirements": entry.missing_requirements,
            "notes": entry.notes,
        }));
    }

    let expected_artifacts = moza_lane_manifest_artifacts_value();
    let artifact_map_changed = manifest.get("artifacts") != Some(&expected_artifacts);
    if artifact_map_changed && !check {
        manifest["artifacts"] = expected_artifacts;
    }

    let manifest_written = !check && (stale_control_count > 0 || artifact_map_changed);
    if manifest_written {
        write_json_file(&lane.join("manifest.json"), &manifest)?;
    }

    Ok(serde_json::json!({
        "success": !check || (stale_control_count == 0 && !artifact_map_changed),
        "command": "wheelctl moza sync-role-status",
        "generated_at_utc": now_utc(),
        "lane": lane.display().to_string(),
        "manifest": "manifest.json",
        "check_only": check,
        "manifest_written": manifest_written,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_output_reports": true,
        "no_feature_reports": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "lane_analysis_success": analysis.success,
        "missing_control_evidence": analysis.missing_control_evidence,
        "failed_captures": analysis.failed_captures,
        "role_count": analysis.role_evidence.len(),
        "stale_control_count": stale_control_count,
        "artifact_map_changed": artifact_map_changed,
        "status_updates": status_updates,
        "notes": [
            "sync-role-status reads stored JSONL reports only; no HID device is opened",
            "semantic_status is diagnostic topology evidence and does not promote lane receipts",
            "manifest artifacts are refreshed to the current lane contract without promoting receipts",
            "lane_analysis_success may remain false while missing roles are recorded honestly"
        ]
    }))
}

fn role_semantic_status(
    required: bool,
    capture: Option<&LaneCaptureAnalysisEntry>,
    notes: &mut Vec<String>,
) -> &'static str {
    let Some(capture) = capture else {
        if required {
            notes.push("required role has no selected capture evidence".to_string());
            return "unavailable";
        }
        notes.push("optional role has no selected capture evidence".to_string());
        return "deferred";
    };

    if !capture.success {
        notes.push("capture failed to parse or validate".to_string());
        return "missing";
    }
    if !capture.control_evidence_ok {
        notes.push(
            "capture parsed, but no parser-visible control movement satisfied the role".to_string(),
        );
        return "missing";
    }
    if capture
        .moving_required_axes
        .iter()
        .any(|axis| axis.starts_with("r5_v1_extended_"))
    {
        notes.push(
            "role is backed by generic live R5 V1 extended fields; semantic control naming remains unproven"
                .to_string(),
        );
        return "generic_aux";
    }

    "proven"
}

fn moving_required_axes(
    requirement: &PassiveCaptureRequirement,
    ranges: &BTreeMap<String, AxisRange>,
) -> Vec<String> {
    let mut axes = BTreeSet::new();
    for axis in requirement.required_axis_variation {
        if axis_has_variation(ranges, axis) {
            axes.insert((*axis).to_string());
        }
    }
    for (_, group_axes) in requirement.required_any_axis_variation {
        for axis in *group_axes {
            if axis_has_variation(ranges, axis) {
                axes.insert((*axis).to_string());
            }
        }
    }
    axes.into_iter().collect()
}

fn required_control_evidence_ok(
    requirement: &PassiveCaptureRequirement,
    evaluation: &PassiveCaptureEvaluation,
) -> bool {
    let has_control_requirements = !requirement.required_axis_variation.is_empty()
        || !requirement.required_any_axis_variation.is_empty()
        || !requirement.required_axis_values.is_empty();
    !has_control_requirements
        || (evaluation.axes_ok && evaluation.any_axes_ok && evaluation.exact_axes_ok)
}

fn record_capture_analysis_error(
    line_errors: &mut Vec<CaptureLineError>,
    truncated: &mut bool,
    line: usize,
    error: String,
) {
    const MAX_LINE_ERRORS: usize = 16;
    if line_errors.len() < MAX_LINE_ERRORS {
        line_errors.push(CaptureLineError { line, error });
    } else {
        *truncated = true;
    }
}

fn update_range_at(ranges: &mut Vec<AxisRange>, index: usize, value: u16) {
    if ranges.len() <= index {
        ranges.resize_with(index + 1, AxisRange::default);
    }
    ranges[index].update(value);
}

fn materialize_byte_ranges(ranges: Vec<AxisRange>) -> Vec<ByteRange> {
    ranges
        .into_iter()
        .enumerate()
        .filter_map(|(index, range)| {
            let min = range.min?;
            let max = range.max?;
            Some(ByteRange {
                index,
                min,
                max,
                changed: min != max,
            })
        })
        .collect()
}

fn materialize_word_ranges(ranges: Vec<AxisRange>) -> Vec<WordRange> {
    ranges
        .into_iter()
        .enumerate()
        .filter_map(|(start_index, range)| {
            let min = range.min?;
            let max = range.max?;
            Some(WordRange {
                start_index,
                min,
                max,
                changed: min != max,
            })
        })
        .collect()
}

fn validate_lane_captures(lane: &Path) -> Result<CaptureValidationSetReceipt> {
    let mut captures = Vec::new();
    let mut total_reports = 0usize;
    let mut parsed_reports = 0usize;
    let mut rejected_reports = 0usize;
    let requirements = passive_capture_requirements_for_lane(lane);

    for requirement in &requirements {
        let path = lane.join(requirement.relative_path);
        let receipt = validate_capture_file(&path, None)
            .with_context(|| format!("failed to validate {}", requirement.relative_path))?;

        let expected_product_ids = expected_product_ids_for_requirement(requirement, lane);
        let evaluation =
            evaluate_passive_capture_requirement(requirement, &receipt, &expected_product_ids);
        let success = receipt.success && evaluation.success;

        total_reports = total_reports.saturating_add(receipt.total_reports);
        parsed_reports = parsed_reports.saturating_add(receipt.parsed_reports);
        rejected_reports = rejected_reports.saturating_add(receipt.rejected_reports);
        captures.push(CaptureValidationSetEntry {
            capture: requirement.relative_path.to_string(),
            fixture_id: requirement.fixture_id.to_string(),
            required_category: requirement.required_category.to_string(),
            required_product_ids: product_id_hex_list(&expected_product_ids),
            required_axis_variation: string_slice_to_vec(requirement.required_axis_variation),
            required_axis_values: requirement_axis_values(requirement.required_axis_values),
            required_any_axis_variation: requirement_any_axis_variation(
                requirement.required_any_axis_variation,
            ),
            required_min_report_len: requirement.min_report_len,
            success,
            missing_requirements: evaluation.missing_requirements,
            total_reports: receipt.total_reports,
            parsed_reports: receipt.parsed_reports,
            rejected_reports: receipt.rejected_reports,
            capture_input_format_reports: receipt.capture_input_format_reports,
            all_reports_have_capture_input_metadata: receipt
                .all_reports_have_capture_input_metadata,
            product_ids: receipt.product_ids,
            parsed_by_category: receipt.parsed_by_category,
            report_ids: receipt.report_ids,
            report_lengths: receipt.report_lengths,
            axis_ranges: receipt.axis_ranges,
        });
    }

    let required_capture_count = requirements.len();
    let validated_capture_count = captures.iter().filter(|entry| entry.success).count();
    let success = required_capture_count > 0
        && validated_capture_count == required_capture_count
        && total_reports > 0
        && rejected_reports == 0;
    let safe_diagnostics = analyze_lane_captures(lane)
        .map(|analysis| analysis.safe_diagnostics)
        .unwrap_or_default();

    Ok(CaptureValidationSetReceipt {
        success,
        command: "wheelctl moza validate-captures",
        generated_at_utc: now_utc(),
        lane: lane.display().to_string(),
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        no_hid_device_opened: true,
        required_capture_count,
        validated_capture_count,
        total_reports,
        parsed_reports,
        rejected_reports,
        safe_diagnostics,
        captures,
        notes: vec![
            "validate-captures replays every required passive lane capture through Moza parsers only; no HID device is opened".to_string(),
            "success requires steering, pedal, handbrake, KS, and ES capture coverage, not a single idle capture".to_string(),
        ],
    })
}

fn increment_count(counts: &mut BTreeMap<String, usize>, key: String) {
    let count = counts.entry(key).or_insert(0);
    *count += 1;
}

#[derive(Debug, Deserialize)]
struct CapturedInputReportLine {
    ts_ns: Option<u64>,
    elapsed_us: Option<u64>,
    command: Option<String>,
    no_ffb_writes: Option<bool>,
    no_output_reports: Option<bool>,
    no_feature_reports: Option<bool>,
    no_serial_config_commands: Option<bool>,
    no_firmware_or_dfu_commands: Option<bool>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    product_name: Option<String>,
    interface_number: Option<i32>,
    usage_page: Option<String>,
    path: Option<String>,
    report_id: Option<String>,
    report_len: Option<usize>,
    data_hex: Option<String>,
    data: Option<Vec<String>>,
}

impl CapturedInputReportLine {
    fn decode_data(&self) -> std::result::Result<Vec<u8>, String> {
        if let Some(data) = &self.data {
            if data.is_empty() {
                return Err("capture line contains an empty data array".to_string());
            }

            let bytes: std::result::Result<Vec<_>, _> =
                data.iter().map(|token| parse_hex_u8_token(token)).collect();
            return bytes;
        }

        let Some(hex) = self.data_hex.as_deref() else {
            return Err("capture line has neither data nor data_hex".to_string());
        };

        parse_hex_bytes(hex)
    }

    fn has_capture_input_metadata(&self, data: &[u8], pid: u16) -> bool {
        let Some(report_id) = data.first().map(|id| hex_u8(*id)) else {
            return false;
        };

        self.ts_ns.map(|value| value > 0).unwrap_or(false)
            && self.elapsed_us.is_some()
            && self.command.as_deref() == Some("wheelctl moza capture-input")
            && self.no_ffb_writes == Some(true)
            && self.no_output_reports == Some(true)
            && self.no_feature_reports == Some(true)
            && self.no_serial_config_commands == Some(true)
            && self.no_firmware_or_dfu_commands == Some(true)
            && self.vendor_id.as_deref() == Some("0x346E")
            && self.product_id.as_deref().and_then(parse_hex_selector) == Some(pid)
            && self
                .product_name
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            && self.interface_number.is_some()
            && self
                .usage_page
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            && self
                .path
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            && self
                .report_id
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(&report_id))
                .unwrap_or(false)
    }
}

fn parse_hex_bytes(value: &str) -> std::result::Result<Vec<u8>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("data_hex is empty".to_string());
    }

    if trimmed.contains(char::is_whitespace) {
        return trimmed.split_whitespace().map(parse_hex_u8_token).collect();
    }

    if !trimmed.len().is_multiple_of(2) {
        return Err("compact data_hex must contain an even number of hex digits".to_string());
    }

    (0..trimmed.len())
        .step_by(2)
        .map(|start| parse_hex_u8_token(&trimmed[start..start + 2]))
        .collect()
}

fn parse_hex_u8_token(token: &str) -> std::result::Result<u8, String> {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or_else(|| token.trim());
    u8::from_str_radix(value, 16).map_err(|_| format!("invalid byte token '{token}'"))
}

#[derive(Default)]
struct CaptureValidationSummary {
    total_reports: usize,
    parsed_reports: usize,
    rejected_reports: usize,
    capture_input_format_reports: usize,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    axis_ranges: BTreeMap<String, AxisRange>,
    line_errors: Vec<CaptureLineError>,
    line_errors_truncated: bool,
}

impl CaptureValidationSummary {
    fn record_parsed(&mut self, pid: u16, state: &MozaInputState, report: &[u8]) {
        self.parsed_reports += 1;
        let identity = identify_device(pid);
        increment_count(
            &mut self.parsed_by_category,
            category_label(identity.category).to_string(),
        );
        self.update_axis("steering_u16", state.steering_u16);
        self.update_axis("throttle_u16", state.throttle_u16);
        self.update_axis("brake_u16", state.brake_u16);
        self.update_axis("clutch_u16", state.clutch_u16);
        self.update_axis("handbrake_u16", state.handbrake_u16);
        self.update_axis("buttons0_u8", u16::from(state.buttons[0]));
        self.update_axis(
            "buttons_any_u8",
            u16::from(
                state
                    .buttons
                    .iter()
                    .copied()
                    .fold(0u8, |acc, value| acc | value),
            ),
        );
        self.update_axis("hat_u8", u16::from(state.hat));
        self.update_axis("funky_u8", u16::from(state.funky));
        self.update_axis("rotary0_u8", u16::from(state.rotary[0]));
        self.update_axis("rotary1_u8", u16::from(state.rotary[1]));
        self.update_axis(
            "ks_buttons_any_u8",
            u16::from(
                state
                    .ks_snapshot
                    .buttons
                    .iter()
                    .copied()
                    .fold(0u8, |acc, value| acc | value),
            ),
        );
        self.update_axis("ks_hat_u8", u16::from(state.ks_snapshot.hat));
        if looks_like_live_r5_v1_extended_report(report) {
            self.update_report_axis(
                "r5_v1_extended_ks_axis0_u16",
                report,
                input_report::R5_V1_EXTENDED_KS_AXIS0_START,
            );
            self.update_report_axis(
                "r5_v1_extended_axis0_u16",
                report,
                input_report::R5_V1_EXTENDED_AXIS0_START,
            );
            self.update_report_axis(
                "r5_v1_extended_axis1_u16",
                report,
                input_report::R5_V1_EXTENDED_AXIS1_START,
            );
            self.update_report_axis(
                "r5_v1_extended_axis2_u16",
                report,
                input_report::R5_V1_EXTENDED_AXIS2_START,
            );
            self.update_report_axis(
                "r5_v1_extended_aux0_u16",
                report,
                input_report::R5_V1_EXTENDED_AUX0_START,
            );
            self.update_report_axis(
                "r5_v1_extended_aux1_u16",
                report,
                input_report::R5_V1_EXTENDED_AUX1_START,
            );
        }
    }

    fn update_axis(&mut self, name: &str, value: u16) {
        self.axis_ranges
            .entry(name.to_string())
            .or_default()
            .update(value);
    }

    fn update_report_axis(&mut self, name: &str, report: &[u8], start: usize) {
        if let Some(value) = parse_axis(report, start) {
            self.update_axis(name, value);
        }
    }

    fn record_rejection(&mut self, line: usize, error: String) {
        self.rejected_reports += 1;
        self.record_line_error(line, error);
    }

    fn record_line_error(&mut self, line: usize, error: String) {
        const MAX_LINE_ERRORS: usize = 16;
        if self.line_errors.len() < MAX_LINE_ERRORS {
            self.line_errors.push(CaptureLineError { line, error });
        } else {
            self.line_errors_truncated = true;
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct AxisRange {
    min: Option<u16>,
    max: Option<u16>,
}

impl AxisRange {
    fn update(&mut self, value: u16) {
        self.min = Some(self.min.map_or(value, |min| min.min(value)));
        self.max = Some(self.max.map_or(value, |max| max.max(value)));
    }
}

#[derive(Debug, Serialize)]
struct InitLaneReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    lane: String,
    manifest: String,
    captures_dir: String,
    wheelbase_pid: String,
    operator: String,
    completion_state: &'static str,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProbeReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    vendor_id: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    devices: Vec<MozaDeviceRecord>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DescriptorReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    vendor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    descriptor_hex_included: bool,
    operator_descriptor_hex_supplied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    operator_descriptor_hex_source: Option<&'static str>,
    devices: Vec<MozaDeviceRecord>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaptureSummary {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    duration_ms: u64,
    read_timeout_ms: i32,
    output: String,
    report_count: usize,
    device: MozaDeviceRecord,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaptureValidationReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    capture: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_override: Option<String>,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    total_reports: usize,
    parsed_reports: usize,
    rejected_reports: usize,
    capture_input_format_reports: usize,
    all_reports_have_capture_input_metadata: bool,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    axis_ranges: BTreeMap<String, AxisRange>,
    line_errors: Vec<CaptureLineError>,
    line_errors_truncated: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaptureAnalysisReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    capture: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    total_reports: usize,
    decoded_reports: usize,
    rejected_reports: usize,
    capture_input_format_reports: usize,
    all_reports_have_capture_input_metadata: bool,
    product_ids: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    moving_byte_count: usize,
    moving_bytes: Vec<usize>,
    byte_ranges: Vec<ByteRange>,
    moving_word_le_count: usize,
    moving_words_le: Vec<usize>,
    word_ranges_le: Vec<WordRange>,
    line_errors: Vec<CaptureLineError>,
    line_errors_truncated: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LaneCaptureAnalysisReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    lane: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    required_capture_count: usize,
    analyzed_capture_count: usize,
    total_reports: usize,
    decoded_reports: usize,
    rejected_reports: usize,
    idle_capture: String,
    idle_moving_bytes: Vec<usize>,
    idle_moving_words_le: Vec<usize>,
    failed_captures: Vec<String>,
    missing_control_evidence: Vec<String>,
    safe_diagnostics: Vec<String>,
    role_evidence: Vec<LaneRoleEvidenceEntry>,
    captures: Vec<LaneCaptureAnalysisEntry>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LaneRoleEvidenceEntry {
    control: String,
    role: String,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_capture: Option<String>,
    semantic_status: &'static str,
    parser_visible: bool,
    moving_required_axes: Vec<String>,
    missing_requirements: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LaneCaptureAnalysisEntry {
    capture: String,
    fixture_id: String,
    success: bool,
    total_reports: usize,
    decoded_reports: usize,
    rejected_reports: usize,
    moving_bytes: Vec<usize>,
    unique_moving_bytes_vs_idle: Vec<usize>,
    moving_words_le: Vec<usize>,
    unique_moving_words_le_vs_idle: Vec<usize>,
    moving_required_axes: Vec<String>,
    control_evidence_ok: bool,
    missing_requirements: Vec<String>,
}

impl LaneCaptureAnalysisEntry {
    fn failed(requirement: &PassiveCaptureRequirement, reason: String) -> Self {
        Self {
            capture: requirement.relative_path.to_string(),
            fixture_id: requirement.fixture_id.to_string(),
            success: false,
            total_reports: 0,
            decoded_reports: 0,
            rejected_reports: 0,
            moving_bytes: Vec::new(),
            unique_moving_bytes_vs_idle: Vec::new(),
            moving_words_le: Vec::new(),
            unique_moving_words_le_vs_idle: Vec::new(),
            moving_required_axes: Vec::new(),
            control_evidence_ok: false,
            missing_requirements: vec![reason],
        }
    }
}

#[derive(Debug, Serialize)]
struct ByteRange {
    index: usize,
    min: u16,
    max: u16,
    changed: bool,
}

#[derive(Debug, Serialize)]
struct WordRange {
    start_index: usize,
    min: u16,
    max: u16,
    changed: bool,
}

#[derive(Debug, Serialize)]
struct CaptureValidationSetReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    lane: String,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    no_hid_device_opened: bool,
    required_capture_count: usize,
    validated_capture_count: usize,
    total_reports: usize,
    parsed_reports: usize,
    rejected_reports: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    safe_diagnostics: Vec<String>,
    captures: Vec<CaptureValidationSetEntry>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaptureValidationSetEntry {
    capture: String,
    fixture_id: String,
    required_category: String,
    required_product_ids: Vec<String>,
    required_axis_variation: Vec<String>,
    required_axis_values: Vec<CaptureAxisValueRequirement>,
    required_any_axis_variation: Vec<CaptureAnyAxisRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_min_report_len: Option<usize>,
    success: bool,
    missing_requirements: Vec<String>,
    total_reports: usize,
    parsed_reports: usize,
    rejected_reports: usize,
    capture_input_format_reports: usize,
    all_reports_have_capture_input_metadata: bool,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    axis_ranges: BTreeMap<String, AxisRange>,
}

#[derive(Debug, Serialize)]
struct CaptureAxisValueRequirement {
    axis: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct CaptureAnyAxisRequirement {
    group: String,
    axes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MozaCaptureFixture {
    schema_version: u32,
    fixture_id: String,
    generated_at_utc: String,
    source_capture: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_override: Option<String>,
    no_ffb_writes: bool,
    total_reports: usize,
    included_reports: usize,
    fixture_truncated: bool,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    axis_ranges: BTreeMap<String, AxisRange>,
    reports: Vec<MozaFixtureReport>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MozaFixtureReport {
    source_line: usize,
    product_id: String,
    product_category: String,
    report_id: String,
    report_len: usize,
    data_hex: String,
    parsed: MozaFixtureParsedState,
}

#[derive(Debug, Serialize)]
struct MozaFixtureParsedState {
    steering_u16: u16,
    throttle_u16: u16,
    brake_u16: u16,
    clutch_u16: u16,
    handbrake_u16: u16,
    buttons_hex: Vec<String>,
    hat: u8,
    funky: u8,
    rotary: Vec<u8>,
    tick: u32,
}

impl MozaFixtureParsedState {
    fn from_input_state(state: &MozaInputState) -> Self {
        Self {
            steering_u16: state.steering_u16,
            throttle_u16: state.throttle_u16,
            brake_u16: state.brake_u16,
            clutch_u16: state.clutch_u16,
            handbrake_u16: state.handbrake_u16,
            buttons_hex: state.buttons.iter().map(|value| hex_u8(*value)).collect(),
            hat: state.hat,
            funky: state.funky,
            rotary: state.rotary.to_vec(),
            tick: state.tick,
        }
    }
}

#[derive(Debug, Serialize)]
struct FixturePromotionReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    capture: String,
    fixture_out: String,
    fixture_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_override: Option<String>,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    no_hid_device_opened: bool,
    overwritten_existing: bool,
    report_count: usize,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FixturePromotionSetReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    lane: String,
    fixture_dir: String,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    no_hid_device_opened: bool,
    overwritten_existing: bool,
    required_fixture_count: usize,
    fixture_count: usize,
    fixtures: Vec<FixturePromotionEntry>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FixturePromotionEntry {
    capture: String,
    fixture_out: String,
    fixture_id: String,
    report_count: usize,
    product_ids: BTreeMap<String, usize>,
    parsed_by_category: BTreeMap<String, usize>,
    report_ids: BTreeMap<String, usize>,
    report_lengths: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
struct ZeroProofSummary {
    path: String,
    generated_at_utc: String,
    receipt_crc32: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_id: Option<String>,
    repeat: u64,
    writes_ok: u64,
    final_zero_sent: bool,
}

#[derive(Debug, Clone, Serialize)]
struct InitProofSummary {
    path: String,
    generated_at_utc: String,
    receipt_crc32: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_id: Option<String>,
    mode: String,
    init_state: String,
    ready: bool,
    feature_report_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct InitProofSet {
    off: InitProofSummary,
    standard: InitProofSummary,
}

impl InitProofSet {
    fn match_product_id(&self, selected_product_id: &str) -> bool {
        self.off.product_id.as_deref() == Some(selected_product_id)
            && self.standard.product_id.as_deref() == Some(selected_product_id)
    }
}

#[derive(Debug, Clone)]
struct DirectModeGateSummary {
    satisfied: bool,
    descriptor_trusted: bool,
    explicit_operator_override: bool,
    descriptor_proof: Option<String>,
    matched_product_id: Option<String>,
    reason: String,
}

impl DirectModeGateSummary {
    fn dry_run(descriptor: Option<PathBuf>, explicit_operator_override: bool) -> Self {
        Self {
            satisfied: true,
            descriptor_trusted: false,
            explicit_operator_override,
            descriptor_proof: descriptor.map(|path| path.display().to_string()),
            matched_product_id: None,
            reason: "dry_run_no_hid_writes".to_string(),
        }
    }

    fn operator_override(descriptor: Option<PathBuf>, selected_product_id: &str) -> Self {
        Self {
            satisfied: true,
            descriptor_trusted: false,
            explicit_operator_override: true,
            descriptor_proof: descriptor.map(|path| path.display().to_string()),
            matched_product_id: Some(selected_product_id.to_string()),
            reason: "explicit_operator_override".to_string(),
        }
    }

    fn trusted_descriptor(descriptor: &Path, selected_product_id: &str) -> Self {
        Self {
            satisfied: true,
            descriptor_trusted: true,
            explicit_operator_override: false,
            descriptor_proof: Some(descriptor.display().to_string()),
            matched_product_id: Some(selected_product_id.to_string()),
            reason: "trusted_descriptor_receipt".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct LowTorqueProofReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_path: Option<String>,
    no_feature_reports: bool,
    no_high_torque: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    high_torque: Option<bool>,
    no_nonzero_above_limit: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    dry_run: bool,
    no_hid_device_opened: bool,
    confirmed: bool,
    zero_proof_validated: bool,
    init_proofs_validated: bool,
    direct_mode_gate_satisfied: bool,
    descriptor_trusted: bool,
    explicit_operator_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    descriptor_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    matched_product_id: Option<String>,
    direct_mode_gate_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    zero_proof: Option<ZeroProofSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    init_proofs: Option<InitProofSet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    max_percent: f32,
    duration_ms: u64,
    hz: u32,
    device: MozaDeviceRecord,
    ladder: Vec<LowTorqueStage>,
    write_attempts: u32,
    writes_ok: u32,
    write_errors: u32,
    bytes_written_total: usize,
    final_zero_attempted: bool,
    final_zero_sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_zero_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    abort_reason: Option<String>,
    command_log: Vec<LowTorqueCommandRecord>,
    notes: Vec<String>,
}

impl LowTorqueProofReceipt {
    fn new(
        selector: Option<String>,
        device: MozaDeviceRecord,
        zero_proof: Option<ZeroProofSummary>,
        max_percent: f32,
        duration_ms: u64,
        hz: u32,
        dry_run: bool,
    ) -> Self {
        let pid = parse_hex_selector(&device.product_id).unwrap_or(product_ids::R5_V2);
        Self {
            success: false,
            command: "wheelctl moza torque-test",
            generated_at_utc: now_utc(),
            receipt_path: None,
            no_feature_reports: true,
            no_high_torque: true,
            high_torque: Some(false),
            no_nonzero_above_limit: true,
            no_ffb_writes: dry_run,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            dry_run,
            no_hid_device_opened: dry_run,
            confirmed: !dry_run,
            zero_proof_validated: zero_proof.is_some(),
            init_proofs_validated: false,
            direct_mode_gate_satisfied: false,
            descriptor_trusted: false,
            explicit_operator_override: false,
            descriptor_proof: None,
            matched_product_id: None,
            direct_mode_gate_reason: "not_evaluated".to_string(),
            zero_proof,
            init_proofs: None,
            selector,
            max_percent,
            duration_ms,
            hz,
            device,
            ladder: low_torque_ladder_for_pid(pid, max_percent, duration_ms, hz),
            write_attempts: 0,
            writes_ok: 0,
            write_errors: 0,
            bytes_written_total: 0,
            final_zero_attempted: false,
            final_zero_sent: false,
            final_zero_error: None,
            abort_reason: None,
            command_log: Vec::new(),
            notes: vec![
                "torque-test is gated by a passing real zero-torque proof before any actual HID write".to_string(),
                "this command sends only Moza direct torque report 0x20 and never sends high-torque or feature reports".to_string(),
            ],
        }
    }

    fn apply_direct_mode_gate(&mut self, gate: DirectModeGateSummary) {
        self.direct_mode_gate_satisfied = gate.satisfied;
        self.descriptor_trusted = gate.descriptor_trusted;
        self.explicit_operator_override = gate.explicit_operator_override;
        self.descriptor_proof = gate.descriptor_proof;
        self.matched_product_id = gate.matched_product_id;
        self.direct_mode_gate_reason = gate.reason;
        if self.descriptor_trusted {
            self.notes.push(
                "direct report writes are gated by a trusted R5 descriptor receipt".to_string(),
            );
        } else if self.explicit_operator_override {
            self.notes.push(
                "direct report writes are gated by explicit operator override because descriptor trust was unavailable".to_string(),
            );
        }
    }

    fn apply_init_proofs(&mut self, init_proofs: Option<InitProofSet>) {
        self.init_proofs_validated = init_proofs.is_some();
        self.init_proofs = init_proofs;
        if self.init_proofs_validated {
            self.notes.push(
                "low torque is gated by passing off-mode and standard-mode init receipts"
                    .to_string(),
            );
        }
    }

    fn set_receipt_path(&mut self, path: Option<&Path>) {
        self.receipt_path = path.map(|path| path.display().to_string());
    }

    fn plan_only(&mut self) {
        let started_at = Instant::now();
        let mut sequence = 0u32;
        for stage in self.ladder.clone() {
            self.record_command(LowTorqueCommandRecord::planned(
                sequence,
                "low_torque",
                started_at,
                stage.percent,
                stage.payload,
            ));
            sequence += 1;
        }
        let pid = parse_hex_selector(&self.device.product_id).unwrap_or(product_ids::R5_V2);
        self.final_zero_attempted = true;
        self.final_zero_sent = true;
        self.record_command(LowTorqueCommandRecord::planned(
            sequence,
            "final_zero",
            started_at,
            0.0,
            zero_torque_payload_for_pid(pid),
        ));
    }

    fn record_command(&mut self, record: LowTorqueCommandRecord) {
        let low_torque_ok = if record.kind == "final_zero" {
            record.torque_raw == 0 && record.flags == 0 && !record.motor_enabled
        } else {
            record.percent <= self.max_percent
                && record.report_id == DIRECT_TORQUE_REPORT_ID
                && record.flags & !0x01 == 0
                && record.motor_enabled == (record.torque_raw != 0)
        };
        if !low_torque_ok {
            self.no_nonzero_above_limit = false;
        }
        self.command_log.push(record);
    }
}

#[derive(Debug, Clone, Serialize)]
struct LowTorqueStage {
    percent: f32,
    torque_nm: f32,
    write_count: u32,
    payload_hex: String,
    report_id: String,
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
    #[serde(skip_serializing)]
    payload: [u8; REPORT_LEN],
}

impl LowTorqueStage {
    fn new(percent: f32, torque_nm: f32, write_count: u32, payload: [u8; REPORT_LEN]) -> Self {
        let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
        let flags = payload[3];
        Self {
            percent,
            torque_nm,
            write_count,
            payload_hex: bytes_hex_compact(&payload),
            report_id: hex_u8(payload[0]),
            torque_raw,
            flags,
            motor_enabled: flags & 0x01 != 0,
            payload,
        }
    }
}

#[derive(Debug, Serialize)]
struct LowTorqueCommandRecord {
    sequence: u32,
    kind: &'static str,
    elapsed_us: u64,
    percent: f32,
    payload_hex: String,
    report_id: String,
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_written: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl LowTorqueCommandRecord {
    fn planned(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        percent: f32,
        payload: [u8; REPORT_LEN],
    ) -> Self {
        Self::new(
            sequence, kind, started_at, percent, payload, "planned", None, None,
        )
    }

    fn ok(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        percent: f32,
        payload: [u8; REPORT_LEN],
        bytes_written: usize,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            percent,
            payload,
            "ok",
            Some(bytes_written),
            None,
        )
    }

    fn error(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        percent: f32,
        payload: [u8; REPORT_LEN],
        error: String,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            percent,
            payload,
            "error",
            None,
            Some(error),
        )
    }

    fn partial(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        percent: f32,
        payload: [u8; REPORT_LEN],
        bytes_written: usize,
        error: String,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            percent,
            payload,
            "partial",
            Some(bytes_written),
            Some(error),
        )
    }

    fn new(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        percent: f32,
        payload: [u8; REPORT_LEN],
        result: &'static str,
        bytes_written: Option<usize>,
        error: Option<String>,
    ) -> Self {
        let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
        let flags = payload[3];
        Self {
            sequence,
            kind,
            elapsed_us: started_at.elapsed().as_micros() as u64,
            percent,
            payload_hex: bytes_hex_compact(&payload),
            report_id: hex_u8(payload[0]),
            torque_raw,
            flags,
            motor_enabled: flags & 0x01 != 0,
            result,
            bytes_written,
            error,
        }
    }
}

#[derive(Debug, Serialize)]
struct MozaInitReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_path: Option<String>,
    no_output_reports: bool,
    no_direct_torque_reports: bool,
    no_high_torque: bool,
    high_torque: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    dry_run: bool,
    no_hid_device_opened: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    mode: &'static str,
    mode_wire_value: String,
    init_state: String,
    ready: bool,
    device: MozaDeviceRecord,
    feature_report_count: usize,
    feature_write_errors: u32,
    output_report_attempts: u32,
    feature_reports: Vec<FeatureReportRecord>,
    notes: Vec<String>,
}

impl MozaInitReceipt {
    fn new(
        selector: Option<String>,
        device: MozaDeviceRecord,
        mode: MozaInitMode,
        dry_run: bool,
    ) -> Self {
        Self {
            success: false,
            command: "wheelctl moza init",
            generated_at_utc: now_utc(),
            receipt_path: None,
            no_output_reports: true,
            no_direct_torque_reports: true,
            no_high_torque: true,
            high_torque: false,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            dry_run,
            no_hid_device_opened: dry_run,
            selector,
            mode: init_mode_label(mode),
            mode_wire_value: ffb_mode_wire_hex(mode),
            init_state: "uninitialized".to_string(),
            ready: false,
            device,
            feature_report_count: 0,
            feature_write_errors: 0,
            output_report_attempts: 0,
            feature_reports: Vec::new(),
            notes: vec![
                "init sends only the Moza staged handshake feature reports for off or standard mode".to_string(),
                "init never sends direct torque report 0x20 and never sends high-torque report 0x02".to_string(),
            ],
        }
    }

    fn finish_from_protocol(&mut self, protocol: &MozaProtocol) {
        self.init_state = init_state_label(protocol.init_state()).to_string();
        self.ready = protocol.is_ffb_ready();
        self.feature_report_count = self.feature_reports.len();
        self.feature_write_errors = self
            .feature_reports
            .iter()
            .filter(|record| record.result == "error")
            .count() as u32;
        self.no_high_torque = self
            .feature_reports
            .iter()
            .all(|record| record.report_id != HIGH_TORQUE_FEATURE_REPORT_ID);
        self.no_output_reports = self.output_report_attempts == 0;
        self.no_direct_torque_reports = self.output_report_attempts == 0;
        self.success = self.ready
            && self.feature_write_errors == 0
            && self.output_report_attempts == 0
            && self.no_high_torque
            && init_feature_reports_are_safe_value(
                &serde_json::to_value(&self.feature_reports).unwrap_or(Value::Null),
                self.mode,
                true,
            );
    }

    fn set_receipt_path(&mut self, path: Option<&Path>) {
        self.receipt_path = path.map(|path| path.display().to_string());
    }
}

#[derive(Debug, Serialize)]
struct FeatureReportRecord {
    sequence: u32,
    kind: &'static str,
    payload_hex: String,
    report_id: String,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_written: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl FeatureReportRecord {
    fn planned(sequence: u32, data: &[u8]) -> Self {
        Self::new(sequence, data, "planned", None, None)
    }

    fn ok(sequence: u32, data: &[u8], bytes_written: usize) -> Self {
        Self::new(sequence, data, "ok", Some(bytes_written), None)
    }

    fn error(sequence: u32, data: &[u8], error: String) -> Self {
        Self::new(sequence, data, "error", None, Some(error))
    }

    fn new(
        sequence: u32,
        data: &[u8],
        result: &'static str,
        bytes_written: Option<usize>,
        error: Option<String>,
    ) -> Self {
        let report_id = data.first().copied().unwrap_or(0);
        Self {
            sequence,
            kind: feature_report_kind(report_id),
            payload_hex: bytes_hex_compact(data),
            report_id: hex_u8(report_id),
            result,
            bytes_written,
            error,
        }
    }
}

struct RecordingFeatureWriter<'a> {
    device: Option<hidapi::HidDevice>,
    records: &'a mut Vec<FeatureReportRecord>,
    dry_run: bool,
    output_report_attempts: u32,
}

impl<'a> RecordingFeatureWriter<'a> {
    fn new(device: hidapi::HidDevice, records: &'a mut Vec<FeatureReportRecord>) -> Self {
        Self {
            device: Some(device),
            records,
            dry_run: false,
            output_report_attempts: 0,
        }
    }

    fn dry_run(records: &'a mut Vec<FeatureReportRecord>) -> Self {
        Self {
            device: None,
            records,
            dry_run: true,
            output_report_attempts: 0,
        }
    }
}

impl DeviceWriter for RecordingFeatureWriter<'_> {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        let sequence = self.records.len().min(u32::MAX as usize) as u32;
        if self.dry_run {
            self.records
                .push(FeatureReportRecord::planned(sequence, data));
            return Ok(data.len());
        }

        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "missing HID device for feature report".to_string())?;
        match device.send_feature_report(data) {
            Ok(()) => {
                let bytes_written = data.len();
                self.records
                    .push(FeatureReportRecord::ok(sequence, data, bytes_written));
                Ok(bytes_written)
            }
            Err(e) => {
                let error = e.to_string();
                self.records
                    .push(FeatureReportRecord::error(sequence, data, error.clone()));
                Err(error.into())
            }
        }
    }

    fn write_output_report(&mut self, _data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_report_attempts = self.output_report_attempts.saturating_add(1);
        Err("init command does not allow output reports".into())
    }
}

#[derive(Debug, Serialize)]
struct ZeroTorqueProofReceipt {
    success: bool,
    command: &'static str,
    test_kind: &'static str,
    generated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_path: Option<String>,
    no_feature_reports: bool,
    no_high_torque: bool,
    no_nonzero_torque: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    dry_run: bool,
    no_hid_device_opened: bool,
    operator_confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    repeat: u32,
    hz: u32,
    watchdog_timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fault_injected: Option<&'static str>,
    watchdog_triggered: bool,
    disconnect_observed: bool,
    device: MozaDeviceRecord,
    payload_hex: String,
    report_id: String,
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
    non_zero_payloads: usize,
    write_attempts: u32,
    writes_ok: u32,
    write_errors: u32,
    bytes_written_total: usize,
    watchdog_faults: u32,
    final_zero_attempted: bool,
    final_zero_sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_zero_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    abort_reason: Option<String>,
    command_log: Vec<ZeroTorqueCommandRecord>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ZeroTorqueCommandRecord {
    sequence: u32,
    kind: &'static str,
    elapsed_us: u64,
    payload_hex: String,
    report_id: String,
    torque_raw: i16,
    flags: u8,
    motor_enabled: bool,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_written: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ZeroTorqueCommandRecord {
    fn ok(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        payload: [u8; REPORT_LEN],
        bytes_written: usize,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            payload,
            "ok",
            Some(bytes_written),
            None,
        )
    }

    fn error(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        payload: [u8; REPORT_LEN],
        error: String,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            payload,
            "error",
            None,
            Some(error),
        )
    }

    fn partial(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        payload: [u8; REPORT_LEN],
        bytes_written: usize,
        error: String,
    ) -> Self {
        Self::new(
            sequence,
            kind,
            started_at,
            payload,
            "partial",
            Some(bytes_written),
            Some(error),
        )
    }

    fn new(
        sequence: u32,
        kind: &'static str,
        started_at: Instant,
        payload: [u8; REPORT_LEN],
        result: &'static str,
        bytes_written: Option<usize>,
        error: Option<String>,
    ) -> Self {
        let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
        let flags = payload[3];
        Self {
            sequence,
            kind,
            elapsed_us: started_at.elapsed().as_micros() as u64,
            payload_hex: bytes_hex_compact(&payload),
            report_id: hex_u8(payload[0]),
            torque_raw,
            flags,
            motor_enabled: flags & 0x01 != 0,
            result,
            bytes_written,
            error,
        }
    }
}

#[derive(Debug, Serialize)]
struct BundleVerificationReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    lane: String,
    requested_stage: String,
    missing_artifacts: usize,
    invalid_artifacts: usize,
    failed_gates: usize,
    artifacts: Vec<BundleArtifactCheck>,
    gates: Vec<BundleGateCheck>,
    endpoint_observations: Vec<BundleEndpointObservation>,
    operator_actions: Vec<String>,
    next_commands: Vec<String>,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BundleEndpointObservation {
    id: String,
    kind: Option<String>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    interface_number: Option<u64>,
    usage_page: Option<String>,
    usage: Option<String>,
    output_capable: Option<bool>,
    required_logical_controls: Vec<String>,
    optional_logical_controls: Vec<String>,
    observed_artifact_count: usize,
    metadata_match_artifact_count: usize,
    artifacts: Vec<BundleEndpointArtifactObservation>,
}

#[derive(Debug, Serialize)]
struct BundleEndpointArtifactObservation {
    path: String,
    status: String,
    vid_pid_count: usize,
    metadata_match_count: usize,
}

#[derive(Debug, Serialize)]
struct LaneAuditReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    lane: String,
    requested_stage: String,
    live_verification_success: bool,
    live_verification: Value,
    missing_receipts: usize,
    invalid_receipts: usize,
    receipt_checks: Vec<LaneAuditCheck>,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LaneAuditCheck {
    stage: String,
    kind: String,
    path: String,
    exists: bool,
    status: String,
    details: String,
}

impl LaneAuditCheck {
    fn pass(stage: &str, kind: &str, path: &str, details: String) -> Self {
        Self {
            stage: stage.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
            exists: true,
            status: "pass".to_string(),
            details,
        }
    }

    fn missing(stage: &str, kind: &str, path: &str, details: String) -> Self {
        Self {
            stage: stage.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
            exists: false,
            status: "missing".to_string(),
            details,
        }
    }

    fn invalid(stage: &str, kind: &str, path: &str, details: String) -> Self {
        Self {
            stage: stage.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
            exists: true,
            status: "invalid".to_string(),
            details,
        }
    }
}

#[derive(Debug, Serialize)]
struct BundleArtifactCheck {
    path: String,
    kind: String,
    required_stage: String,
    exists: bool,
    valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_count: Option<usize>,
    status: String,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BundleGateCheck {
    name: &'static str,
    status: &'static str,
    details: String,
}

impl BundleGateCheck {
    fn pass(name: &'static str, details: String) -> Self {
        Self {
            name,
            status: "pass",
            details,
        }
    }

    fn fail(name: &'static str, details: String) -> Self {
        Self {
            name,
            status: "fail",
            details,
        }
    }
}

#[derive(Clone, Copy)]
struct BundleArtifactRequirement {
    relative_path: &'static str,
    kind: BundleArtifactKind,
    stage: MozaBundleStage,
}

impl BundleArtifactRequirement {
    const fn json(relative_path: &'static str, stage: MozaBundleStage) -> Self {
        Self {
            relative_path,
            kind: BundleArtifactKind::Json,
            stage,
        }
    }

    const fn jsonl(relative_path: &'static str, stage: MozaBundleStage) -> Self {
        Self {
            relative_path,
            kind: BundleArtifactKind::JsonLines,
            stage,
        }
    }
}

#[derive(Clone, Copy)]
enum BundleArtifactKind {
    Json,
    JsonLines,
}

impl BundleArtifactKind {
    fn label(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::JsonLines => "jsonl",
        }
    }
}

struct PassiveCaptureRequirement {
    relative_path: &'static str,
    fixture_id: &'static str,
    required_category: &'static str,
    expected_products: PassiveCaptureProductRequirement,
    required_axis_variation: &'static [&'static str],
    required_axis_values: &'static [(&'static str, u16)],
    required_any_axis_variation: &'static [(&'static str, &'static [&'static str])],
    min_report_len: Option<usize>,
    always_required: bool,
    default_required: bool,
}

#[derive(Clone, Copy)]
enum PassiveCaptureProductRequirement {
    ManifestR5,
    Fixed(&'static [u16]),
}

struct PassiveCaptureEvaluation {
    success: bool,
    product_ids_ok: bool,
    category_count: usize,
    axes_ok: bool,
    exact_axes_ok: bool,
    any_axes_ok: bool,
    report_len_ok: bool,
    capture_input_metadata_ok: bool,
    missing_requirements: Vec<String>,
}

impl ZeroTorqueProofReceipt {
    fn new(
        command: &'static str,
        selector: Option<String>,
        repeat: u32,
        hz: u32,
        watchdog_timeout_ms: u64,
        device: MozaDeviceRecord,
        payload: [u8; REPORT_LEN],
        dry_run: bool,
    ) -> Self {
        let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
        let flags = payload[3];
        let motor_enabled = flags & 0x01 != 0;
        let non_zero_payloads = usize::from(torque_raw != 0 || flags != 0);
        let test_kind = match command {
            "wheelctl moza watchdog-proof" => "watchdog_proof",
            "wheelctl moza disconnect-proof" => "disconnect_proof",
            _ => "zero_torque",
        };

        Self {
            success: false,
            command,
            test_kind,
            generated_at_utc: now_utc(),
            receipt_path: None,
            no_feature_reports: true,
            no_high_torque: true,
            no_nonzero_torque: non_zero_payloads == 0,
            no_ffb_writes: dry_run,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            dry_run,
            no_hid_device_opened: dry_run,
            operator_confirmed: !dry_run,
            selector,
            repeat,
            hz,
            watchdog_timeout_ms,
            max_duration_ms: None,
            fault_injected: None,
            watchdog_triggered: false,
            disconnect_observed: false,
            device,
            payload_hex: bytes_hex_compact(&payload),
            report_id: hex_u8(payload[0]),
            torque_raw,
            flags,
            motor_enabled,
            non_zero_payloads,
            write_attempts: 0,
            writes_ok: 0,
            write_errors: 0,
            bytes_written_total: 0,
            watchdog_faults: 0,
            final_zero_attempted: false,
            final_zero_sent: false,
            final_zero_error: None,
            abort_reason: None,
            command_log: Vec::new(),
            notes: vec![
                "zero sends only Moza direct torque report 0x20 encoded with raw torque 0 and flags 0".to_string(),
                "this command does not send Moza feature reports, FFB mode reports, high-torque reports, or non-zero torque".to_string(),
            ],
        }
    }

    fn record_command(&mut self, record: ZeroTorqueCommandRecord) {
        if record.torque_raw != 0 || record.flags != 0 || record.motor_enabled {
            self.non_zero_payloads += 1;
            self.no_nonzero_torque = false;
        }
        self.command_log.push(record);
    }

    fn set_receipt_path(&mut self, path: Option<&Path>) {
        self.receipt_path = path.map(|path| path.display().to_string());
    }
}

#[derive(Debug, Serialize)]
struct CaptureLineError {
    line: usize,
    error: String,
}

#[derive(Debug, Serialize)]
struct CapturedInputReport {
    ts_ns: u64,
    elapsed_us: u64,
    command: &'static str,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    vendor_id: String,
    product_id: String,
    product_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    interface_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_page: Option<String>,
    path: String,
    report_id: String,
    report_len: usize,
    data_hex: String,
    data: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MozaDeviceRecord {
    vendor_id: String,
    product_id: String,
    product_name: String,
    product_category: String,
    topology_hint: String,
    output_capable: bool,
    r5_wheelbase_pid: bool,
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

impl MozaDeviceRecord {
    fn from_device_info(
        info: &DeviceInfo,
        include_descriptor: bool,
        include_descriptor_hex: bool,
    ) -> Self {
        let identity = identify_device(info.product_id());
        let report_descriptor = if include_descriptor {
            try_read_report_descriptor(&info.path().to_string_lossy(), include_descriptor_hex)
        } else {
            None
        };
        let output_capable = is_wheelbase_product(info.product_id());
        let descriptor_source = descriptor_source_label(report_descriptor.as_ref());

        let mut record = Self {
            vendor_id: hex_u16(info.vendor_id()),
            product_id: hex_u16(info.product_id()),
            product_name: identity.name.to_string(),
            product_category: category_label(identity.category).to_string(),
            topology_hint: topology_label(identity.topology_hint).to_string(),
            output_capable,
            r5_wheelbase_pid: matches!(info.product_id(), product_ids::R5_V1 | product_ids::R5_V2),
            manufacturer: info.manufacturer_string().map(str::to_string),
            product_string: info.product_string().map(str::to_string),
            serial_number_present: info.serial_number().is_some(),
            interface_number: Some(info.interface_number()),
            usage_page: Some(hex_u16(info.usage_page())),
            usage: Some(hex_u16(info.usage())),
            path: info.path().to_string_lossy().to_string(),
            descriptor_source: descriptor_source.clone(),
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            report_descriptor_hex: None,
            report_metadata_source: "protocol_expected".to_string(),
            input_report_lengths: expected_input_report_lengths(info.product_id()),
            output_report_ids: expected_output_report_ids(output_capable),
            output_reports: expected_output_reports(output_capable),
            feature_report_ids: expected_feature_report_ids(output_capable),
        };
        if let Some(descriptor) = report_descriptor {
            record.apply_report_descriptor(descriptor, &descriptor_source);
        }
        record
    }

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

#[derive(Debug)]
struct ReportDescriptor {
    len: usize,
    crc32: String,
    hex: Option<String>,
    metadata: Option<HidReportDescriptorMetadata>,
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

fn descriptor_source_label(report_descriptor: Option<&ReportDescriptor>) -> String {
    if report_descriptor.is_some() {
        "linux_sysfs".to_string()
    } else {
        "unavailable".to_string()
    }
}

fn expected_input_report_lengths(pid: u16) -> Vec<usize> {
    if pid == product_ids::R5_V1 {
        vec![42]
    } else if is_wheelbase_product(pid) {
        vec![7, 31]
    } else if pid == product_ids::SR_P_PEDALS {
        vec![5]
    } else if pid == product_ids::HBP_HANDBRAKE {
        vec![2, 3, 4]
    } else {
        Vec::new()
    }
}

fn expected_output_report_ids(output_capable: bool) -> Vec<String> {
    if output_capable {
        vec![DIRECT_TORQUE_REPORT_ID.to_string()]
    } else {
        Vec::new()
    }
}

fn expected_output_reports(output_capable: bool) -> Vec<HidReportRecord> {
    if output_capable {
        vec![HidReportRecord {
            report_id: DIRECT_TORQUE_REPORT_ID.to_string(),
            report_len: REPORT_LEN,
        }]
    } else {
        Vec::new()
    }
}

fn expected_feature_report_ids(output_capable: bool) -> Vec<String> {
    if output_capable {
        vec![
            HIGH_TORQUE_FEATURE_REPORT_ID.to_string(),
            START_REPORTING_FEATURE_REPORT_ID.to_string(),
            FFB_MODE_FEATURE_REPORT_ID.to_string(),
        ]
    } else {
        Vec::new()
    }
}

fn selector_matches(device: &MozaDeviceRecord, selector: Option<&str>) -> bool {
    let Some(selector) = selector else {
        return true;
    };

    let selector = selector.trim();
    if selector.is_empty() {
        return true;
    }

    if let Some(identity) = parse_hid_observe_selector(selector) {
        return hid_observe_selector_matches(device, &identity);
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
            .to_ascii_lowercase()
            .contains(&selector_lc)
        || device
            .product_string
            .as_deref()
            .map(|s| s.to_ascii_lowercase().contains(&selector_lc))
            .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HidObserveSelector {
    vendor_id: u16,
    product_id: u16,
    interface_number: i32,
    usage_page: u16,
    usage: u16,
}

fn parse_hid_observe_selector(selector: &str) -> Option<HidObserveSelector> {
    let rest = selector.strip_prefix("hid-")?;
    let mut parts = rest.split('-');
    let vendor_id = parse_hex_selector(parts.next()?)?;
    let product_id = parse_hex_selector(parts.next()?)?;
    let interface_number = parts.next()?.strip_prefix("if")?.parse::<i32>().ok()?;
    let usage_page = parse_hex_selector(parts.next()?)?;
    let usage = parse_hex_selector(parts.next()?)?;
    parts.next().is_none().then_some(HidObserveSelector {
        vendor_id,
        product_id,
        interface_number,
        usage_page,
        usage,
    })
}

fn hid_observe_selector_matches(device: &MozaDeviceRecord, selector: &HidObserveSelector) -> bool {
    device
        .vendor_id
        .eq_ignore_ascii_case(&hex_u16(selector.vendor_id))
        && device
            .product_id
            .eq_ignore_ascii_case(&hex_u16(selector.product_id))
        && device.interface_number == Some(selector.interface_number)
        && device.usage_page.as_deref().is_some_and(|usage_page| {
            usage_page.eq_ignore_ascii_case(&hex_u16(selector.usage_page))
        })
        && device
            .usage
            .as_deref()
            .is_some_and(|usage| usage.eq_ignore_ascii_case(&hex_u16(selector.usage)))
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

fn parse_required_hex_u16(value: &str) -> Result<u16> {
    parse_hex_selector(value).ok_or_else(|| anyhow!("expected hex u16, got '{value}'"))
}

fn category_label(category: MozaDeviceCategory) -> &'static str {
    match category {
        MozaDeviceCategory::Wheelbase => "wheelbase",
        MozaDeviceCategory::Pedals => "pedals",
        MozaDeviceCategory::Shifter => "shifter",
        MozaDeviceCategory::Handbrake => "handbrake",
        MozaDeviceCategory::Unknown => "unknown",
    }
}

fn topology_label(topology: MozaTopologyHint) -> &'static str {
    match topology {
        MozaTopologyHint::WheelbaseAggregated => "wheelbase_aggregated",
        MozaTopologyHint::StandaloneUsb => "standalone_usb",
        MozaTopologyHint::Unknown => "unknown",
    }
}

fn write_json_receipt<T: Serialize>(path: Option<&Path>, value: &T) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    write_json_file(path, value)
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("failed to serialize JSON receipt")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))?;
    Ok(())
}

fn print_probe_receipt(json: bool, json_out: Option<&Path>, receipt: &ProbeReceipt) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza probe found {} HID device(s); no FFB writes sent.",
            receipt.devices.len()
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_status_receipt(json: bool, json_out: Option<&Path>, receipt: &Value) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        let device_count = json_u64(receipt, "device_count").unwrap_or(0);
        let lane = json_string(receipt, "lane");
        println!("Moza status found {device_count} HID device(s); no FFB writes sent.");
        if let Some(lane) = lane {
            println!("Lane: {lane}");
        }
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_descriptor_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &DescriptorReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza descriptor capture found {} HID device(s); no FFB writes sent.",
            receipt.devices.len()
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_capture_summary(json: bool, summary: &CaptureSummary) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(summary)?);
    } else {
        println!(
            "Captured {} Moza input report(s) to {}; no FFB writes sent.",
            summary.report_count, summary.output
        );
    }
    Ok(())
}

fn print_capture_validation_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &CaptureValidationReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Validated {} Moza capture report(s): {} parsed, {} rejected; no FFB writes sent.",
            receipt.total_reports, receipt.parsed_reports, receipt.rejected_reports
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_capture_analysis_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &CaptureAnalysisReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Analyzed {} Moza capture report(s): {} decoded, {} rejected, {} moving byte(s), {} moving little-endian word(s); no HID device opened.",
            receipt.total_reports,
            receipt.decoded_reports,
            receipt.rejected_reports,
            receipt.moving_byte_count,
            receipt.moving_word_le_count
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_lane_capture_analysis_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &LaneCaptureAnalysisReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Analyzed {} of {} required Moza lane capture(s): {} decoded, {} rejected, {} missing control evidence; no HID device opened.",
            receipt.analyzed_capture_count,
            receipt.required_capture_count,
            receipt.decoded_reports,
            receipt.rejected_reports,
            receipt.missing_control_evidence.len()
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_role_status_sync_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &Value,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        let stale_count = json_u64(receipt, "stale_control_count").unwrap_or(0);
        let check_only = json_bool(receipt, "check_only").unwrap_or(false);
        let manifest_written = json_bool(receipt, "manifest_written").unwrap_or(false);
        let action = if check_only {
            "Checked"
        } else if manifest_written {
            "Synced"
        } else {
            "Confirmed"
        };
        println!("{action} Moza role semantic statuses; stale controls: {stale_count}.");
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_capture_validation_set_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &CaptureValidationSetReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Validated {} of {} required Moza capture(s): {} parsed, {} rejected; no FFB writes sent.",
            receipt.validated_capture_count,
            receipt.required_capture_count,
            receipt.parsed_reports,
            receipt.rejected_reports
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_fixture_promotion_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &FixturePromotionReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Promoted {} Moza capture report(s) into {}; no FFB writes sent.",
            receipt.report_count, receipt.fixture_out
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_fixture_set_promotion_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &FixturePromotionSetReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Promoted {} required Moza fixture(s) into {}; no FFB writes sent.",
            receipt.fixture_count, receipt.fixture_dir
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_zero_torque_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &ZeroTorqueProofReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Sent {} Moza zero-torque report(s), final_zero_sent={}; non_zero_payloads={}.",
            receipt.writes_ok, receipt.final_zero_sent, receipt.non_zero_payloads
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_watchdog_proof_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &ZeroTorqueProofReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza watchdog proof success={}, watchdog_triggered={}, final_zero_sent={}, non_zero_payloads={}.",
            receipt.success,
            receipt.watchdog_triggered,
            receipt.final_zero_sent,
            receipt.non_zero_payloads
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_disconnect_proof_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &ZeroTorqueProofReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza disconnect proof success={}, disconnect_observed={}, final_zero_attempted={}, final_zero_sent={}.",
            receipt.success,
            receipt.disconnect_observed,
            receipt.final_zero_attempted,
            receipt.final_zero_sent
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_init_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &MozaInitReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza init success={}, mode={}, init_state={}, feature_reports={}, high_torque={}.",
            receipt.success,
            receipt.mode,
            receipt.init_state,
            receipt.feature_report_count,
            receipt.high_torque
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_low_torque_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &LowTorqueProofReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Moza low-torque test success={}, dry_run={}, writes_ok={}, final_zero_sent={}, max_percent={}.",
            receipt.success,
            receipt.dry_run,
            receipt.writes_ok,
            receipt.final_zero_sent,
            receipt.max_percent
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_bundle_verification_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &BundleVerificationReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Verified Moza bundle stage '{}': success={}, missing_artifacts={}, invalid_artifacts={}, failed_gates={}.",
            receipt.requested_stage,
            receipt.success,
            receipt.missing_artifacts,
            receipt.invalid_artifacts,
            receipt.failed_gates
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_lane_audit_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &LaneAuditReceipt,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Audited Moza lane stage '{}': success={}, missing_receipts={}, invalid_receipts={}, live_verification_success={}.",
            receipt.requested_stage,
            receipt.success,
            receipt.missing_receipts,
            receipt.invalid_receipts,
            receipt.live_verification_success
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_receipt_template(
    json: bool,
    kind: MozaReceiptTemplateKind,
    json_out: &Path,
    receipt: &Value,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Wrote non-claiming Moza {} receipt template to {}; verifier will reject it until real evidence is filled in.",
            receipt_template_kind_label(kind),
            json_out.display()
        );
    }
    Ok(())
}

fn print_proof_receipt(json: bool, json_out: &Path, label: &str, receipt: &Value) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Wrote Moza {label} proof receipt to {}; success={}.",
            json_out.display(),
            json_bool(receipt, "success").unwrap_or(false)
        );
    }
    Ok(())
}

fn print_manifest_promotion_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &Value,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Promoted Moza manifest to {}; release_ready=false, high_torque_validated=false.",
            json_string(receipt, "completion_state").unwrap_or("unknown")
        );
        if let Some(path) = json_out {
            println!("Receipt: {}", path.display());
        }
    }
    Ok(())
}

fn print_init_lane_receipt(json: bool, receipt: &InitLaneReceipt) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(receipt)?);
    } else {
        println!(
            "Created Moza lane manifest at {}; captures directory: {}.",
            receipt.manifest, receipt.captures_dir
        );
    }
    Ok(())
}

fn receipt_template_kind_label(kind: MozaReceiptTemplateKind) -> &'static str {
    match kind {
        MozaReceiptTemplateKind::PitHouse => "Pit House coexistence",
        MozaReceiptTemplateKind::SimulatorTelemetry => "simulator telemetry",
        MozaReceiptTemplateKind::SimulatorFfb => "simulator FFB smoke",
    }
}

fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn utc_timestamp_pair_is_ordered(start: &str, end: &str) -> bool {
    if !start.ends_with('Z') || !end.ends_with('Z') {
        return false;
    }
    let Ok(start) = chrono::DateTime::parse_from_rfc3339(start) else {
        return false;
    };
    let Ok(end) = chrono::DateTime::parse_from_rfc3339(end) else {
        return false;
    };
    start <= end
}

fn unix_now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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

fn bytes_hex_array(bytes: &[u8]) -> Vec<String> {
    bytes.iter().map(|b| hex_u8(*b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use racing_wheel_hid_moza_protocol::rim_ids;
    use std::path::PathBuf;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
    const TEST_GENERATED_AT: &str = "2026-05-06T00:00:00Z";
    const TEST_LOW_TORQUE_GENERATED_AT: &str = "2026-05-06T00:00:01Z";

    fn write_temp_capture(lines: &[&str]) -> TestResult<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("capture.jsonl");
        let mut file = File::create(&path)?;
        for line in lines {
            writeln!(file, "{line}")?;
        }
        Ok((dir, path))
    }

    fn write_text_file(path: &Path, contents: &str) -> TestResult {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    fn split_generated_command(command: &str) -> TestResult<Vec<String>> {
        let mut args = Vec::new();
        let mut current = String::new();
        let mut chars = command.chars().peekable();
        let mut in_single_quote = false;
        let mut in_double_quote = false;

        while let Some(ch) = chars.next() {
            match ch {
                '\'' if in_single_quote && !in_double_quote && chars.peek() == Some(&'\'') => {
                    current.push('\'');
                    chars.next();
                }
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                ch if ch.is_whitespace() && !in_single_quote && !in_double_quote => {
                    if !current.is_empty() {
                        args.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            }
        }

        if in_single_quote {
            return Err(format!("unclosed single quote in generated command: {command}").into());
        }
        if in_double_quote {
            return Err(format!("unclosed double quote in generated command: {command}").into());
        }
        if !current.is_empty() {
            args.push(current);
        }
        Ok(args)
    }

    fn command_with_test_placeholders(command: &str) -> String {
        command
            .replace("<r5>", "moza-r5")
            .replace("<srp>", "moza-srp")
            .replace("<hbp>", "moza-hbp")
            .replace("<sim>", "simhub-bridge")
            .replace(
                "<normalized-telemetry-source.jsonl>",
                "normalized-telemetry-source.jsonl",
            )
            .replace("<date>", "2026-05-06")
            .replace("YYYY-MM-DD", "2026-05-06")
            .replace("<hex bytes>", "05010904")
    }

    fn write_repeated_jsonl(path: &Path, count: usize) -> TestResult {
        let mut lines = String::new();
        for sequence in 0..count {
            lines.push_str(&format!(r#"{{"sequence":{sequence}}}"#));
            lines.push('\n');
        }
        write_text_file(path, &lines)
    }

    fn simulator_fixture_ffb_scalar(sequence: usize) -> f64 {
        if sequence.is_multiple_of(2) {
            0.4
        } else {
            -0.4
        }
    }

    fn simulator_telemetry_snapshot(sequence: usize) -> Value {
        serde_json::json!({
            "sequence": sequence,
            "timestamp_ns": sequence as u64 * 16_666_667,
            "recorder_command": SIMULATOR_TELEMETRY_RECORDER_COMMAND,
            "recorder_session_id": "sim-telemetry-session-001",
            "recording_duration_ms": 5000,
            "game": "simhub-bridge",
            "telemetry_source": "simhub_bridge",
            "hardware_output_enabled": false,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "speed_ms": 12.5 + (sequence % 7) as f64 * 0.1,
            "steering_angle": ((sequence % 9) as f64 - 4.0) * 0.01,
            "throttle": 0.35,
            "brake": 0.0,
            "rpm": 3200.0 + (sequence % 11) as f64,
            "gear": 3,
            "ffb_scalar": simulator_fixture_ffb_scalar(sequence)
        })
    }

    fn write_simulator_telemetry_jsonl(path: &Path, count: usize) -> TestResult {
        let mut lines = String::new();
        for sequence in 0..count {
            lines.push_str(&serde_json::to_string(&simulator_telemetry_snapshot(
                sequence,
            ))?);
            lines.push('\n');
        }
        write_text_file(path, &lines)
    }

    fn simulator_telemetry_recording_json(count: usize) -> Value {
        let frames: Vec<_> = (0..count)
            .map(|sequence| {
                serde_json::json!({
                    "sequence": sequence,
                    "timestamp_ns": sequence as u64 * 16_666_667,
                    "raw_size": 128,
                    "data": simulator_telemetry_snapshot(sequence)
                })
            })
            .collect();

        serde_json::json!({
            "metadata": {
                "game_id": "simhub-bridge",
                "timestamp": 1_778_000_000u64,
                "duration_seconds": 5.0,
                "frame_count": count,
                "average_fps": 24.0,
                "car_id": "test-car",
                "track_id": "test-track",
                "description": "test recording"
            },
            "frames": frames
        })
    }

    fn simulator_ffb_output_record(
        sequence: usize,
        kind: &'static str,
        percent: f64,
        writer_hardware_lane: &Path,
    ) -> Value {
        let payload = if percent == 0.0 {
            zero_torque_payload_for_pid(product_ids::R5_V2)
        } else {
            low_torque_payload_for_pid_percent(product_ids::R5_V2, percent as f32).0
        };
        let torque_raw = i16::from_le_bytes([payload[1], payload[2]]);
        let flags = payload[3];
        let input_ffb_scalar = simulator_fixture_ffb_scalar(sequence % 120);
        serde_json::json!({
            "sequence": sequence,
            "kind": kind,
            "writer_command": SIMULATOR_FFB_WRITER_COMMAND,
            "writer_session_id": "sim-ffb-session-001",
            "writer_started_at_utc": "2026-05-06T00:00:02Z",
            "writer_completed_at_utc": "2026-05-06T00:00:03Z",
            "writer_device_path": "\\\\?\\hid#vid_346e&pid_0014&mi_00",
            "writer_product_id": "0x0014",
            "writer_hardware_lane": writer_hardware_lane.display().to_string(),
            "vendor_id": "0x346E",
            "product_id": "0x0014",
            "output_capable": true,
            "hardware_output_enabled": true,
            "no_hid_device_opened": false,
            "no_ffb_writes": false,
            "transport": "hid",
            "hid_write_target": "output_report",
            "hid_write_attempted": true,
            "high_torque": false,
            "no_high_torque": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "elapsed_us": sequence as u64 * 1000,
            "telemetry_sequence": sequence % 120,
            "input_ffb_scalar": input_ffb_scalar,
            "input_telemetry_artifact": "simulator-telemetry-recording.jsonl",
            "input_telemetry_snapshot_count": 120,
            "input_telemetry_recorder_session_id": "sim-telemetry-session-001",
            "input_telemetry_game": "simhub-bridge",
            "input_telemetry_source": "simhub_bridge",
            "percent": percent,
            "payload_hex": bytes_hex_compact(&payload),
            "report_id": hex_u8(payload[0]),
            "torque_raw": torque_raw,
            "flags": flags,
            "motor_enabled": flags & 0x01 != 0,
            "result": "ok",
            "bytes_written": REPORT_LEN
        })
    }

    fn write_simulator_ffb_output_jsonl(
        path: &Path,
        total_count: usize,
        nonzero_count: usize,
        zero_count: usize,
    ) -> TestResult {
        write_simulator_ffb_output_jsonl_mutated(
            path,
            total_count,
            nonzero_count,
            zero_count,
            |_, _| {},
        )
    }

    fn write_simulator_ffb_output_jsonl_mutated<F>(
        path: &Path,
        total_count: usize,
        nonzero_count: usize,
        zero_count: usize,
        mut mutate: F,
    ) -> TestResult
    where
        F: FnMut(usize, &mut Value),
    {
        if total_count != nonzero_count + zero_count || zero_count == 0 {
            return Err("invalid simulator FFB output counts".into());
        }

        let writer_hardware_lane = path.parent().unwrap_or_else(|| Path::new("."));
        let mut lines = String::new();
        for sequence in 0..total_count {
            let (kind, percent) = if sequence < nonzero_count {
                let percent = if sequence % 2 == 0 { 2.0 } else { -2.0 };
                ("sim_output", percent)
            } else if sequence + 1 == total_count {
                ("final_zero", 0.0)
            } else {
                ("clear_zero", 0.0)
            };
            let mut record =
                simulator_ffb_output_record(sequence, kind, percent, writer_hardware_lane);
            if kind == "clear_zero" {
                match sequence.saturating_sub(nonzero_count) {
                    0 => record["clear_event"] = serde_json::json!("stop"),
                    1 => record["clear_event"] = serde_json::json!("pause"),
                    2 => record["clear_event"] = serde_json::json!("game_exit"),
                    3 => record["clear_event"] = serde_json::json!("mode_mismatch"),
                    _ => {}
                }
            }
            mutate(sequence, &mut record);
            lines.push_str(&serde_json::to_string(&record)?);
            lines.push('\n');
        }
        write_text_file(path, &lines)
    }

    fn write_test_json_file(path: &Path, value: &Value) -> TestResult {
        let mut value = value.clone();
        if matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(
                "zero-torque-proof.json"
                    | "watchdog-proof.json"
                    | "disconnect-proof.json"
                    | "init-off.json"
                    | "init-standard.json"
                    | "low-torque-proof.json"
            )
        ) && value
            .get("receipt_path")
            .map(Value::is_null)
            .unwrap_or(true)
        {
            value["receipt_path"] = serde_json::json!(path.display().to_string());
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("simulator-ffb-smoke.json")
            && value
                .get("writer_hardware_lane")
                .map(Value::is_null)
                .unwrap_or(true)
            && let Some(parent) = path.parent()
        {
            value["writer_hardware_lane"] = serde_json::json!(parent.display().to_string());
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("simulator-ffb-smoke.json")
            && value.get("prerequisite_artifacts").is_none()
            && let Some(parent) = path.parent()
            && let Ok(artifacts) = simulator_ffb_prerequisite_artifact_summaries(parent)
        {
            value["prerequisite_artifacts"] = serde_json::to_value(artifacts)?;
        }
        let contents = serde_json::to_string_pretty(&value)?;
        write_text_file(path, &contents)
    }

    fn temp_lane_under_cwd() -> TestResult<(tempfile::TempDir, PathBuf)> {
        let cwd = std::env::current_dir()?;
        let root = cwd.join("target").join("moza-path-tests");
        fs::create_dir_all(&root)?;
        let lane_root = tempfile::Builder::new()
            .prefix("pit-house-case-")
            .tempdir_in(root)?;
        let lane = lane_root.path().strip_prefix(&cwd)?.to_path_buf();
        Ok((lane_root, lane))
    }

    fn sample_device() -> MozaDeviceRecord {
        MozaDeviceRecord {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0014".to_string(),
            product_name: "Moza R5".to_string(),
            product_category: "wheelbase".to_string(),
            topology_hint: "wheelbase_aggregated".to_string(),
            output_capable: true,
            r5_wheelbase_pid: true,
            manufacturer: Some("MOZA".to_string()),
            product_string: Some("MOZA R5 Base".to_string()),
            serial_number_present: true,
            interface_number: Some(0),
            usage_page: Some("0x0001".to_string()),
            usage: Some("0x0004".to_string()),
            path: "\\\\?\\hid#vid_346e&pid_0014&mi_00".to_string(),
            descriptor_source: "unavailable".to_string(),
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            report_descriptor_hex: None,
            report_metadata_source: "protocol_expected".to_string(),
            input_report_lengths: vec![7, 31],
            output_report_ids: vec!["0x20".to_string()],
            output_reports: vec![HidReportRecord {
                report_id: "0x20".to_string(),
                report_len: REPORT_LEN,
            }],
            feature_report_ids: vec!["0x02".to_string(), "0x03".to_string(), "0x11".to_string()],
        }
    }

    fn sample_r5_json_device() -> Value {
        serde_json::json!({
            "vendor_id": "0x346E",
            "product_id": "0x0014",
            "product_name": "Moza R5",
            "manufacturer": "MOZA",
            "serial_number_present": true,
            "interface_number": 2,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "descriptor_source": "operator_supplied_hex",
            "report_descriptor_crc32": "0x12345678",
            "report_metadata_source": "protocol_expected",
            "input_report_lengths": [7, 31],
            "output_report_ids": ["0x20"],
            "output_reports": [{"report_id": "0x20", "report_len": 8}],
            "feature_report_ids": ["0x02", "0x03", "0x11"]
        })
    }

    fn sample_srp_json_device() -> Value {
        serde_json::json!({
            "vendor_id": "0x346E",
            "product_id": "0x0003",
            "product_name": "Moza SR-P Pedals",
            "manufacturer": "MOZA",
            "serial_number_present": true,
            "interface_number": 0,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "descriptor_source": "operator_supplied_hex",
            "report_descriptor_crc32": "0x23456789",
            "report_metadata_source": "protocol_expected",
            "input_report_lengths": [5],
            "output_report_ids": [],
            "output_reports": [],
            "feature_report_ids": []
        })
    }

    fn sample_hbp_json_device() -> Value {
        serde_json::json!({
            "vendor_id": "0x346E",
            "product_id": "0x0022",
            "product_name": "Moza HBP Handbrake",
            "manufacturer": "MOZA",
            "serial_number_present": true,
            "interface_number": 0,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "descriptor_source": "operator_supplied_hex",
            "report_descriptor_crc32": "0x3456789A",
            "report_metadata_source": "protocol_expected",
            "input_report_lengths": [2, 3, 4],
            "output_report_ids": [],
            "output_reports": [],
            "feature_report_ids": []
        })
    }

    fn sample_trusted_r5_json_device() -> Value {
        let mut device = sample_r5_json_device();
        device["report_descriptor_len"] = serde_json::json!(40);
        device["report_descriptor_crc32"] = serde_json::json!("0xD8079D85");
        device["report_descriptor_hex"] = serde_json::json!(
            "850175089506810285027508951E81028520750895079102850375089503B102851175089503B102"
        );
        device["report_metadata_source"] = serde_json::json!("report_descriptor_parsed");
        device["input_report_lengths"] = serde_json::json!([7, 31]);
        device["output_report_ids"] = serde_json::json!(["0x20"]);
        device["output_reports"] = serde_json::json!([{"report_id": "0x20", "report_len": 8}]);
        device["feature_report_ids"] = serde_json::json!(["0x03", "0x11"]);
        device
    }

    fn sample_trusted_r5_v1_json_device() -> Value {
        let mut device = sample_trusted_r5_json_device();
        device["product_id"] = serde_json::json!("0x0004");
        device["product_name"] = serde_json::json!("Moza R5 V1");
        device["report_descriptor_len"] = serde_json::json!(32);
        device["report_descriptor_crc32"] = serde_json::json!("0x1C8EF640");
        device["report_descriptor_hex"] =
            serde_json::json!("85017508952981028520750895079102850375089503B102851175089503B102");
        device["input_report_lengths"] = serde_json::json!([42]);
        device
    }

    fn sample_mixed_vendor_pedals_json_device() -> Value {
        serde_json::json!({
            "vendor_id": "0x1234",
            "product_id": "0xABCD",
            "product_name": "External Pedals",
            "manufacturer": "Example",
            "serial_number_present": true,
            "interface_number": 0,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "descriptor_source": "operator_supplied_hex",
            "report_descriptor_crc32": "0x456789AB",
            "report_metadata_source": "report_descriptor_parsed",
            "input_report_lengths": [8],
            "output_report_ids": [],
            "output_reports": [],
            "feature_report_ids": [],
            "output_capable": false
        })
    }

    fn trusted_r5_descriptor_receipt() -> Value {
        serde_json::json!({
            "success": true,
            "command": "hid-capture descriptor",
            "generated_at_utc": TEST_GENERATED_AT,
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "devices": [sample_trusted_r5_json_device()]
        })
    }

    fn write_trusted_descriptor_if_missing(root: &Path) -> TestResult {
        let descriptor = root.join("descriptor.json");
        if !descriptor.exists() {
            write_test_json_file(&descriptor, &trusted_r5_descriptor_receipt())?;
        }
        Ok(())
    }

    #[test]
    fn status_receipt_reports_observe_only_state() -> TestResult {
        let receipt = moza_status_receipt(vec![sample_device()], Some("0x0014"), None);

        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza status")
        );
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_bool(&receipt, "no_ffb_writes"), Some(true));
        assert_eq!(json_bool(&receipt, "no_serial_config_commands"), Some(true));
        assert_eq!(
            json_bool(&receipt, "no_firmware_or_dfu_commands"),
            Some(true)
        );
        assert_eq!(json_u64(&receipt, "device_count"), Some(1));

        let first = receipt
            .get("devices")
            .and_then(Value::as_array)
            .and_then(|devices| devices.first())
            .ok_or("expected device status")?;
        assert_eq!(json_bool(first, "ffb_ready"), Some(false));
        assert_eq!(json_string(first, "init_state"), Some("uninitialized"));
        assert_eq!(json_bool(first, "safe_to_send_torque"), Some(false));
        assert_eq!(json_bool(first, "high_torque_allowed"), Some(false));
        Ok(())
    }

    #[test]
    fn device_status_lane_readiness_adds_descriptor_metadata_without_enabling_torque() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("descriptor.json"),
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [sample_trusted_r5_json_device()]
            }),
        )?;
        let device = crate::client::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            source: None,
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            hid_path_present: None,
            device_type: crate::client::DeviceType::WheelBase,
            state: crate::client::DeviceState::Connected,
            capabilities: crate::client::DeviceCapabilities {
                supports_raw_torque_1khz: true,
                max_torque_nm: 5.5,
                ..crate::client::DeviceCapabilities::default()
            },
        };
        let moza = crate::client::MozaReadinessStatus::from_device(&device)
            .ok_or("missing Moza status")?;
        let mut status = crate::client::DeviceStatus {
            device,
            last_seen: Utc::now(),
            active_faults: Vec::new(),
            telemetry: crate::client::TelemetryData::default(),
            moza: Some(moza),
        };

        apply_lane_readiness_to_device_status(&mut status, dir.path());

        let readiness = status.moza.as_ref().ok_or("missing readiness")?;
        assert!(readiness.descriptor_trusted);
        assert_eq!(readiness.descriptor_crc32.as_deref(), Some("0xD8079D85"));
        assert_eq!(
            readiness.descriptor_source.as_deref(),
            Some("operator_supplied_hex")
        );
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn device_status_lane_readiness_reports_stored_stage_without_enabling_torque() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("passive-verification.json"),
            &stored_verification_receipt(dir.path(), MozaBundleStage::Passive),
        )?;
        write_test_json_file(
            &dir.path().join("zero-verification.json"),
            &stored_verification_receipt(dir.path(), MozaBundleStage::Zero),
        )?;
        write_test_json_file(&dir.path().join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &dir.path().join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;
        let mut smoke_ready = stored_verification_receipt(dir.path(), MozaBundleStage::SmokeReady);
        smoke_ready["success"] = serde_json::json!(false);
        smoke_ready["failed_gates"] = serde_json::json!(1);
        smoke_ready["gates"] = serde_json::json!([
            {"name": "low_torque_real_hardware", "status": "fail"}
        ]);
        write_test_json_file(
            &dir.path().join("smoke-ready-verification.json"),
            &smoke_ready,
        )?;
        let device = crate::client::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            source: None,
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            hid_path_present: None,
            device_type: crate::client::DeviceType::WheelBase,
            state: crate::client::DeviceState::Connected,
            capabilities: crate::client::DeviceCapabilities::default(),
        };
        let moza = crate::client::MozaReadinessStatus::from_device(&device)
            .ok_or("missing Moza status")?;
        let mut status = crate::client::DeviceStatus {
            device,
            last_seen: Utc::now(),
            active_faults: Vec::new(),
            telemetry: crate::client::TelemetryData::default(),
            moza: Some(moza),
        };

        apply_lane_readiness_to_device_status(&mut status, dir.path());

        let readiness = status.moza.as_ref().ok_or("missing readiness")?;
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
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn device_status_lane_readiness_requires_init_receipts_before_low_torque_state() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("passive-verification.json"),
            &stored_verification_receipt(dir.path(), MozaBundleStage::Passive),
        )?;
        write_test_json_file(
            &dir.path().join("zero-verification.json"),
            &stored_verification_receipt(dir.path(), MozaBundleStage::Zero),
        )?;
        let device = crate::client::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            source: None,
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            hid_path_present: None,
            device_type: crate::client::DeviceType::WheelBase,
            state: crate::client::DeviceState::Connected,
            capabilities: crate::client::DeviceCapabilities::default(),
        };
        let moza = crate::client::MozaReadinessStatus::from_device(&device)
            .ok_or("missing Moza status")?;
        let mut status = crate::client::DeviceStatus {
            device,
            last_seen: Utc::now(),
            active_faults: Vec::new(),
            telemetry: crate::client::TelemetryData::default(),
            moza: Some(moza),
        };

        apply_lane_readiness_to_device_status(&mut status, dir.path());

        let readiness = status.moza.as_ref().ok_or("missing readiness")?;
        assert_eq!(readiness.safety_state, "lane_zero_torque_verified");
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn device_status_lane_readiness_ignores_untrusted_stored_stage() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("zero-verification.json"),
            &serde_json::json!({
                "success": true,
                "requested_stage": "zero",
                "missing_artifacts": 0,
                "invalid_artifacts": 0,
                "failed_gates": 0,
                "gates": []
            }),
        )?;
        let device = crate::client::DeviceInfo {
            id: "moza-r5".to_string(),
            name: "Moza R5".to_string(),
            source: None,
            vendor_id: Some("0x346E".to_string()),
            product_id: Some("0x0014".to_string()),
            manufacturer: None,
            product_string: None,
            serial_number_present: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            hid_path_present: None,
            device_type: crate::client::DeviceType::WheelBase,
            state: crate::client::DeviceState::Connected,
            capabilities: crate::client::DeviceCapabilities::default(),
        };
        let moza = crate::client::MozaReadinessStatus::from_device(&device)
            .ok_or("missing Moza status")?;
        let mut status = crate::client::DeviceStatus {
            device,
            last_seen: Utc::now(),
            active_faults: Vec::new(),
            telemetry: crate::client::TelemetryData::default(),
            moza: Some(moza),
        };

        apply_lane_readiness_to_device_status(&mut status, dir.path());

        let readiness = status.moza.as_ref().ok_or("missing readiness")?;
        assert_eq!(readiness.safety_state, "pre_validation");
        assert!(!readiness.direct_mode_allowed);
        assert!(!readiness.high_torque_allowed);
        assert!(!readiness.safe_to_send_torque);
        Ok(())
    }

    #[test]
    fn support_status_summarizes_first_blocking_stage() -> TestResult {
        let dir = tempfile::tempdir()?;
        fs::create_dir_all(dir.path())?;

        let status = support_bundle_status(dir.path());

        let readiness = status
            .get("readiness")
            .and_then(Value::as_object)
            .ok_or("expected readiness object")?;
        assert_eq!(
            readiness
                .get("highest_passing_stage")
                .and_then(Value::as_str),
            Some("none")
        );
        assert_eq!(
            readiness.get("next_required_stage").and_then(Value::as_str),
            Some("passive")
        );
        assert_eq!(
            readiness
                .get("ready_for_real_hardware_smoke")
                .and_then(Value::as_bool),
            Some(false)
        );
        let first_blocking = readiness
            .get("first_blocking_stage")
            .and_then(Value::as_object)
            .ok_or("expected first blocking stage")?;
        assert_eq!(
            first_blocking
                .get("requested_stage")
                .and_then(Value::as_str),
            Some("passive")
        );
        let artifact_index = status
            .get("artifact_index")
            .and_then(Value::as_array)
            .ok_or("expected artifact index")?;
        for requirement in lane_artifact_index_requirements() {
            assert!(
                artifact_index.iter().any(|artifact| {
                    artifact.get("path").and_then(Value::as_str) == Some(requirement.relative_path)
                }),
                "expected support artifact index to include {}",
                requirement.relative_path
            );
        }
        Ok(())
    }

    #[test]
    fn support_status_requires_init_before_low_torque_ready() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &real_watchdog_receipt(3),
        )?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;

        let status = support_bundle_status(dir.path());
        let readiness = status
            .get("readiness")
            .and_then(Value::as_object)
            .ok_or("expected readiness object")?;

        assert_eq!(
            readiness
                .get("highest_passing_stage")
                .and_then(Value::as_str),
            Some("zero")
        );
        assert_eq!(
            readiness
                .get("ready_for_low_torque")
                .and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn support_status_reports_smoke_ready_without_release_claim() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_lane_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;

        let status = support_bundle_status(dir.path());

        let readiness = status
            .get("readiness")
            .and_then(Value::as_object)
            .ok_or("expected readiness object")?;
        assert_eq!(
            readiness
                .get("highest_passing_stage")
                .and_then(Value::as_str),
            Some("smoke_ready")
        );
        assert!(readiness.get("next_required_stage").map(Value::is_null) == Some(true));
        assert_eq!(
            readiness
                .get("ready_for_real_hardware_smoke")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            readiness
                .get("smoke_ready_lane_audit_passed")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            readiness.get("release_ready").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            readiness.get("claim_scope").and_then(Value::as_str),
            Some("diagnostic_context_only")
        );
        Ok(())
    }

    #[test]
    fn support_status_requires_lane_audit_before_real_hardware_smoke_ready() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;

        let status = support_bundle_status(dir.path());
        let readiness = status
            .get("readiness")
            .and_then(Value::as_object)
            .ok_or("expected readiness object")?;

        assert_eq!(
            readiness
                .get("highest_passing_stage")
                .and_then(Value::as_str),
            Some("smoke_ready")
        );
        assert_eq!(
            readiness
                .get("smoke_ready_lane_audit_passed")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            readiness
                .get("ready_for_real_hardware_smoke")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            readiness.get("release_ready").and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    fn sample_lane_manifest(
        completion_state: &str,
        hardware_validated: bool,
        simulator_validated: bool,
    ) -> Value {
        serde_json::json!({
            "schema_version": 1,
            "lane": "moza-r5-windows-usb",
            "completion_state": completion_state,
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "operator": "Steven",
            "platform": {
                "os": "Windows",
                "transport": {
                    "hid": true,
                    "serial_config": false
                }
            },
            "hardware": {
                "wheelbase": "Moza R5",
                "wheelbase_pid": "0x0014",
                "rims": ["KS", "ES"],
                "pedals": ["SR-P"],
                "handbrake": "HBP"
            },
            "topology": moza_lane_manifest_topology_value(product_ids::R5_V2),
            "claims": {
                "ffb": "staged",
                "high_torque": false,
                "pit_house_coexistence": "tested_separately"
            },
            "hardware_validated": hardware_validated,
            "simulator_validated": simulator_validated,
            "high_torque_validated": false,
            "release_ready": false,
            "artifacts": moza_lane_manifest_artifacts_value()
        })
    }

    fn capture_line(pid: u16, data_hex: &str) -> String {
        let report_id = data_hex.get(..2).unwrap_or("00");
        format!(
            r#"{{"ts_ns":1,"elapsed_us":0,"command":"wheelctl moza capture-input","no_ffb_writes":true,"no_output_reports":true,"no_feature_reports":true,"no_serial_config_commands":true,"no_firmware_or_dfu_commands":true,"vendor_id":"0x346E","product_id":"{}","product_name":"Moza test device","interface_number":0,"usage_page":"0x0001","path":"test-hid-path","report_id":"0x{}","report_len":{},"data_hex":"{}"}}"#,
            hex_u16(pid),
            report_id,
            data_hex.len() / 2,
            data_hex
        )
    }

    fn wheelbase_report_hex(steering: u16, throttle: u16, brake: u16, funky: u8) -> String {
        wheelbase_full_report_hex(steering, throttle, brake, 0, 0, 0, 0, funky, 0, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn wheelbase_full_report_hex(
        steering: u16,
        throttle: u16,
        brake: u16,
        clutch: u16,
        handbrake: u16,
        buttons0: u8,
        hat: u8,
        funky: u8,
        rotary0: u8,
        rotary1: u8,
    ) -> String {
        let mut report = [0u8; 31];
        report[0] = 0x01;
        report[1..3].copy_from_slice(&steering.to_le_bytes());
        report[3..5].copy_from_slice(&throttle.to_le_bytes());
        report[5..7].copy_from_slice(&brake.to_le_bytes());
        report[7..9].copy_from_slice(&clutch.to_le_bytes());
        report[9..11].copy_from_slice(&handbrake.to_le_bytes());
        report[11] = buttons0;
        report[27] = hat;
        report[28] = funky;
        report[29] = rotary0;
        report[30] = rotary1;
        bytes_hex_compact(&report)
    }

    fn live_r5_v1_ks_extended_report_hex(axis0: u16, button1: u8, direction: u8) -> String {
        let mut report = [0u8; 42];
        report[0] = 0x01;
        report[1..3].copy_from_slice(&0x7A37u16.to_le_bytes());
        report[3..5].copy_from_slice(&axis0.to_le_bytes());
        report[5..7].copy_from_slice(&0x8000u16.to_le_bytes());
        report[7..9].copy_from_slice(&0x8001u16.to_le_bytes());
        report[9..11].copy_from_slice(&0x8001u16.to_le_bytes());
        report[11..13].copy_from_slice(&0x8000u16.to_le_bytes());
        report[13..15].copy_from_slice(&0x8001u16.to_le_bytes());
        report[15..17].copy_from_slice(&0x8000u16.to_le_bytes());
        report[17] = 0x08;
        report[18] = button1;
        report[28] = direction;
        bytes_hex_compact(&report)
    }

    fn live_r5_v1_extended_aux_report_hex(aux0: u16, aux1: u16) -> String {
        let mut report = [0u8; 42];
        report[0] = 0x01;
        report[1..3].copy_from_slice(&0x7A37u16.to_le_bytes());
        report[3..5].copy_from_slice(&0x8001u16.to_le_bytes());
        report[5..7].copy_from_slice(&0x8000u16.to_le_bytes());
        report[7..9].copy_from_slice(&0x8001u16.to_le_bytes());
        report[9..11].copy_from_slice(&0x8001u16.to_le_bytes());
        report[11..13].copy_from_slice(&0x8000u16.to_le_bytes());
        report[13..15].copy_from_slice(&0x8001u16.to_le_bytes());
        report[15..17].copy_from_slice(&0x8000u16.to_le_bytes());
        report[17] = 0x08;
        report[34..36].copy_from_slice(&aux0.to_le_bytes());
        report[36..38].copy_from_slice(&aux1.to_le_bytes());
        bytes_hex_compact(&report)
    }

    fn live_r5_v1_trailer_report_hex(trailer: [u8; 4]) -> String {
        let mut report = [0u8; 42];
        report[0] = 0x01;
        report[1..3].copy_from_slice(&0x7A37u16.to_le_bytes());
        report[3..5].copy_from_slice(&0x8001u16.to_le_bytes());
        report[5..7].copy_from_slice(&0x8000u16.to_le_bytes());
        report[7..9].copy_from_slice(&0x8001u16.to_le_bytes());
        report[9..11].copy_from_slice(&0x8001u16.to_le_bytes());
        report[11..13].copy_from_slice(&0x8000u16.to_le_bytes());
        report[13..15].copy_from_slice(&0x8001u16.to_le_bytes());
        report[15..17].copy_from_slice(&0x8000u16.to_le_bytes());
        report[17] = 0x08;
        report[38..42].copy_from_slice(&trailer);
        bytes_hex_compact(&report)
    }

    fn write_minimal_passive_bundle(root: &Path) -> TestResult {
        let r5_device = sample_trusted_r5_json_device();
        let srp_device = sample_srp_json_device();
        let hbp_device = sample_hbp_json_device();
        write_test_json_file(
            &root.join("manifest.json"),
            &sample_lane_manifest("passive_capture_ready", false, false),
        )?;
        write_test_json_file(
            &root.join("device-list.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl device list",
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [r5_device.clone(), srp_device.clone(), hbp_device.clone()]
            }),
        )?;
        write_test_json_file(
            &root.join("moza-probe.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl moza probe",
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [r5_device.clone(), srp_device.clone(), hbp_device.clone()]
            }),
        )?;
        write_test_json_file(
            &root.join("hid-list.json"),
            &serde_json::json!({
                "success": true,
                "command": "hid-capture list",
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [r5_device.clone(), srp_device.clone(), hbp_device.clone()]
            }),
        )?;
        write_test_json_file(
            &root.join("hardware-doctor.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl hardware doctor",
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_output_reports": true,
                "no_feature_reports": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "hid": {
                    "api_available": true,
                    "enumeration_available": true,
                    "known_devices_visible": [r5_device.clone(), srp_device.clone(), hbp_device.clone()],
                    "moza_vid_visible": true
                },
                "windows_pnp": {
                    "scan_attempted": true,
                    "moza_vid_visible": true,
                    "hid_interface_count": 1,
                    "serial_interface_count": 1,
                    "devices": [
                        {
                            "class": "HIDClass",
                            "vendor_id": "0x346E",
                            "product_id": "0x0014",
                            "interface_number": 2,
                            "instance_id_present": true
                        },
                        {
                            "class": "Ports",
                            "vendor_id": "0x346E",
                            "product_id": "0x0014",
                            "interface_number": 0,
                            "instance_id_present": true
                        }
                    ]
                }
            }),
        )?;
        write_test_json_file(
            &root.join("descriptor.json"),
            &serde_json::json!({
                "success": true,
                "command": "hid-capture descriptor",
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [r5_device, srp_device, hbp_device]
            }),
        )?;
        write_text_file(
            &root.join("captures/r5-idle.jsonl"),
            &capture_line(product_ids::R5_V2, &wheelbase_report_hex(0x8000, 0, 0, 0)),
        )?;
        write_text_file(
            &root.join("captures/r5-steering-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(product_ids::R5_V2, &wheelbase_report_hex(0x0000, 0, 0, 0)),
                capture_line(product_ids::R5_V2, &wheelbase_report_hex(0xFFFF, 0, 0, 0))
            ),
        )?;
        write_text_file(
            &root.join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0x0000, 0, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0xFFFF, 0, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;
        write_text_file(
            &root.join("captures/r5-brake-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0x0000, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0xFFFF, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;
        write_text_file(
            &root.join("captures/r5-clutch-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0x0000, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0xFFFF, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;
        write_text_file(
            &root.join("captures/r5-handbrake-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0x0000, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0xFFFF, 0, 0, 0, 0, 0)
                )
            ),
        )?;
        write_text_file(
            &root.join("captures/r5-aggregated-idle-after-controls.jsonl"),
            &capture_line(
                product_ids::R5_V2,
                &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            ),
        )?;
        write_text_file(
            &root.join("captures/ks-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 8, rim_ids::KS, 0, 0),
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(
                        0x8000,
                        0,
                        0,
                        0x6000,
                        0,
                        0x01,
                        2,
                        rim_ids::KS,
                        0x10,
                        0x20
                    ),
                )
            ),
        )?;
        write_text_file(
            &root.join("captures/es-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0x02, 0, 0, 0, 0),
                )
            ),
        )?;

        write_test_json_file(
            &root.join("parser-fixture-validation.json"),
            &serde_json::to_value(validate_lane_captures(root)?)?,
        )?;
        write_fixture_promotion_set(root)?;
        Ok(())
    }

    fn write_fixture_promotion_set(root: &Path) -> TestResult {
        let mut fixtures = Vec::new();
        let requirements = passive_capture_requirements_for_lane(root);
        for requirement in &requirements {
            let fixture_out = format!("fixtures/{}.json", requirement.fixture_id);
            let expected_product_ids = expected_product_ids_for_requirement(requirement, root);
            let product_id = expected_product_ids
                .first()
                .copied()
                .map(hex_u16)
                .unwrap_or_else(|| "missing".to_string());
            let product_ids = serde_json::json!({ product_id.clone(): 1 });
            let parsed_by_category = serde_json::json!({ requirement.required_category: 1 });
            let report = promoted_fixture_report_for_requirement(requirement, &product_id)?;
            let report_len = json_u64(&report, "report_len")
                .ok_or("promoted fixture report must include report_len")?
                .to_string();
            write_test_json_file(
                &root.join(&fixture_out),
                &serde_json::json!({
                    "schema_version": 1,
                    "fixture_id": requirement.fixture_id,
                    "no_ffb_writes": true,
                    "total_reports": 1,
                    "included_reports": 1,
                    "product_ids": product_ids.clone(),
                    "reports": [report]
                }),
            )?;
            fixtures.push(serde_json::json!({
                "capture": root.join(requirement.relative_path).display().to_string(),
                "fixture_out": fixture_out,
                "fixture_id": requirement.fixture_id,
                "report_count": 1,
                "product_ids": product_ids,
                "parsed_by_category": parsed_by_category,
                "report_ids": {"0x01": 1},
                "report_lengths": {report_len: 1}
            }));
        }

        write_test_json_file(
            &root.join("fixture-promotion.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl moza promote-fixtures",
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "no_hid_device_opened": true,
                "required_fixture_count": requirements.len(),
                "fixture_count": fixtures.len(),
                "fixtures": fixtures
            }),
        )
    }

    fn refresh_passive_parser_receipts(root: &Path) -> TestResult {
        write_test_json_file(
            &root.join("parser-fixture-validation.json"),
            &serde_json::to_value(validate_lane_captures(root)?)?,
        )?;
        write_fixture_promotion_set(root)
    }

    fn declare_standalone_control_topology(
        root: &Path,
        control_key: &str,
        role: &str,
        endpoint_id: &str,
        endpoint_kind: &str,
        product_id: u16,
        evidence_capture: &str,
    ) -> TestResult {
        let mut manifest = read_json_path(&root.join("manifest.json"))?;
        let endpoint = serde_json::json!({
            "id": endpoint_id,
            "kind": endpoint_kind,
            "vendor_id": MOZA_VENDOR_HEX,
            "product_id": hex_u16(product_id),
            "interface_number": 0,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "output_capable": false
        });
        manifest["topology"]["endpoints"]
            .as_array_mut()
            .ok_or("expected topology endpoints")?
            .push(endpoint);
        manifest["topology"]["logical_controls"][control_key] = serde_json::json!({
            "role": role,
            "source_endpoint": endpoint_id,
            "connection": "standalone_usb",
            "required": true,
            "evidence_capture": evidence_capture,
            "semantic_status": "deferred"
        });
        write_test_json_file(&root.join("manifest.json"), &manifest)
    }

    fn add_topology_endpoint(
        root: &Path,
        endpoint_id: &str,
        endpoint_kind: &str,
        vendor_id: &str,
        product_id: &str,
        output_capable: bool,
    ) -> TestResult {
        let mut manifest = read_json_path(&root.join("manifest.json"))?;
        let endpoints = manifest["topology"]["endpoints"]
            .as_array_mut()
            .ok_or("expected topology endpoints")?;
        endpoints.push(serde_json::json!({
            "id": endpoint_id,
            "kind": endpoint_kind,
            "vendor_id": vendor_id,
            "product_id": product_id,
            "interface_number": 0,
            "usage_page": "0x0001",
            "usage": "0x0004",
            "output_capable": output_capable
        }));
        write_test_json_file(&root.join("manifest.json"), &manifest)
    }

    fn add_topology_control(
        root: &Path,
        control_key: &str,
        role: &str,
        endpoint_id: &str,
        connection: &str,
        evidence_capture: &str,
    ) -> TestResult {
        let mut manifest = read_json_path(&root.join("manifest.json"))?;
        manifest["topology"]["logical_controls"][control_key] = serde_json::json!({
            "role": role,
            "source_endpoint": endpoint_id,
            "connection": connection,
            "required": true,
            "evidence_capture": evidence_capture,
            "semantic_status": "deferred"
        });
        write_test_json_file(&root.join("manifest.json"), &manifest)
    }

    fn set_required_hub_controls(
        root: &Path,
        controls: &[(&str, &str, Option<&str>, &str)],
    ) -> TestResult {
        let mut manifest = read_json_path(&root.join("manifest.json"))?;
        let logical_controls = manifest
            .pointer_mut("/topology/logical_controls")
            .and_then(Value::as_object_mut)
            .ok_or("expected topology logical_controls object")?;
        logical_controls.clear();
        for (key, role, rim, evidence_capture) in controls {
            let mut control = serde_json::json!({
                "role": role,
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": evidence_capture,
                "semantic_status": "deferred"
            });
            if let Some(rim) = rim {
                control["rim"] = serde_json::json!(rim);
            }
            logical_controls.insert((*key).to_string(), control);
        }
        write_test_json_file(&root.join("manifest.json"), &manifest)
    }

    fn set_declared_hardware(
        root: &Path,
        rims: &[&str],
        pedals: &[&str],
        handbrake: Option<&str>,
    ) -> TestResult {
        let mut manifest = read_json_path(&root.join("manifest.json"))?;
        let hardware = manifest
            .get_mut("hardware")
            .and_then(Value::as_object_mut)
            .ok_or("expected manifest hardware object")?;
        hardware.insert("rims".to_string(), serde_json::json!(rims));
        hardware.insert("pedals".to_string(), serde_json::json!(pedals));
        match handbrake {
            Some(handbrake) => {
                hardware.insert("handbrake".to_string(), serde_json::json!(handbrake));
            }
            None => {
                hardware.remove("handbrake");
            }
        }
        write_test_json_file(&root.join("manifest.json"), &manifest)
    }

    fn append_device_to_observation_receipts(root: &Path, device: &Value) -> TestResult {
        for artifact in [
            "device-list.json",
            "moza-probe.json",
            "hid-list.json",
            "descriptor.json",
        ] {
            let mut receipt = read_json_path(&root.join(artifact))?;
            receipt["devices"]
                .as_array_mut()
                .ok_or("expected devices array")?
                .push(device.clone());
            write_test_json_file(&root.join(artifact), &receipt)?;
        }
        Ok(())
    }

    fn replace_observation_receipt_devices(root: &Path, devices: &[Value]) -> TestResult {
        for artifact in [
            "device-list.json",
            "moza-probe.json",
            "hid-list.json",
            "descriptor.json",
        ] {
            let mut receipt = read_json_path(&root.join(artifact))?;
            receipt["devices"] = serde_json::Value::Array(devices.to_vec());
            write_test_json_file(&root.join(artifact), &receipt)?;
        }
        Ok(())
    }

    fn remove_optional_capture_files(root: &Path, captures: &[&str]) -> TestResult {
        for capture in captures {
            let path = root.join(capture);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn promoted_fixture_report_for_requirement(
        requirement: &PassiveCaptureRequirement,
        product_id: &str,
    ) -> TestResult<Value> {
        let pid = parse_hex_selector(product_id).ok_or("fixture product id must be hex")?;
        let (data_hex, parsed) = match pid {
            product_ids::SR_P_PEDALS => (
                "0134127856".to_string(),
                parsed_fixture_state_json(0, 0x1234, 0x5678, 0, 0, 0, 0, 0, 0, 0),
            ),
            product_ids::HBP_HANDBRAKE => (
                "013412A5".to_string(),
                parsed_fixture_state_json(0, 0, 0, 0, 0x1234, 0xA5, 0, 0, 0, 0),
            ),
            _ => {
                let rim = if requirement.fixture_id == "es_controls" {
                    rim_ids::ES
                } else {
                    rim_ids::KS
                };
                (
                    wheelbase_report_hex(0x8000, 0x1234, 0x5678, rim),
                    parsed_fixture_state_json(0x8000, 0x1234, 0x5678, 0, 0, 0, 0, rim, 0, 0),
                )
            }
        };
        let report_id = data_hex.get(..2).ok_or("fixture data missing report id")?;
        Ok(serde_json::json!({
            "source_line": 1,
            "product_id": product_id,
            "product_category": requirement.required_category,
            "report_id": format!("0x{report_id}"),
            "report_len": data_hex.len() / 2,
            "data_hex": data_hex,
            "parsed": parsed
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn parsed_fixture_state_json(
        steering: u16,
        throttle: u16,
        brake: u16,
        clutch: u16,
        handbrake: u16,
        buttons0: u8,
        hat: u8,
        funky: u8,
        rotary0: u8,
        rotary1: u8,
    ) -> Value {
        let mut buttons = vec!["0x00".to_string(); 16];
        buttons[0] = hex_u8(buttons0);
        serde_json::json!({
            "steering_u16": steering,
            "throttle_u16": throttle,
            "brake_u16": brake,
            "clutch_u16": clutch,
            "handbrake_u16": handbrake,
            "buttons_hex": buttons,
            "hat": hat,
            "funky": funky,
            "rotary": [rotary0, rotary1],
            "tick": 0
        })
    }

    fn zero_command_log(repeat: u32) -> Vec<Value> {
        let mut records = Vec::new();
        for sequence in 0..repeat {
            records.push(serde_json::json!({
                "sequence": sequence,
                "kind": "scheduled_zero",
                "elapsed_us": u64::from(sequence),
                "payload_hex": "2000000000000000",
                "report_id": "0x20",
                "torque_raw": 0,
                "flags": 0,
                "motor_enabled": false,
                "result": "ok",
                "bytes_written": 8
            }));
        }
        records.push(serde_json::json!({
            "sequence": repeat,
            "kind": "final_zero",
            "elapsed_us": u64::from(repeat),
            "payload_hex": "2000000000000000",
            "report_id": "0x20",
            "torque_raw": 0,
            "flags": 0,
            "motor_enabled": false,
            "result": "ok",
            "bytes_written": 8
        }));
        records
    }

    fn real_zero_receipt(repeat: u32) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza zero",
            "generated_at_utc": TEST_GENERATED_AT,
            "no_feature_reports": true,
            "no_high_torque": true,
            "no_nonzero_torque": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "dry_run": false,
            "no_hid_device_opened": false,
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "repeat": repeat,
            "hz": 1000,
            "report_id": "0x20",
            "torque_raw": 0,
            "flags": 0,
            "motor_enabled": false,
            "write_attempts": repeat,
            "writes_ok": repeat + 1,
            "write_errors": 0,
            "watchdog_faults": 0,
            "final_zero_attempted": true,
            "final_zero_sent": true,
            "command_log": zero_command_log(repeat)
        })
    }

    fn watchdog_command_log(pre_zero_count: u32) -> Vec<Value> {
        let mut records = Vec::new();
        for sequence in 0..pre_zero_count {
            records.push(serde_json::json!({
                "sequence": sequence,
                "kind": "scheduled_zero",
                "elapsed_us": u64::from(sequence),
                "payload_hex": "2000000000000000",
                "report_id": "0x20",
                "torque_raw": 0,
                "flags": 0,
                "motor_enabled": false,
                "result": "ok",
                "bytes_written": 8
            }));
        }
        records.push(serde_json::json!({
            "sequence": pre_zero_count,
            "kind": "final_zero",
            "elapsed_us": u64::from(pre_zero_count),
            "payload_hex": "2000000000000000",
            "report_id": "0x20",
            "torque_raw": 0,
            "flags": 0,
            "motor_enabled": false,
            "result": "ok",
            "bytes_written": 8
        }));
        records
    }

    fn real_watchdog_receipt(pre_zero_count: u32) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza watchdog-proof",
            "test_kind": "watchdog_proof",
            "generated_at_utc": TEST_GENERATED_AT,
            "no_feature_reports": true,
            "no_high_torque": true,
            "no_nonzero_torque": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "dry_run": false,
            "no_hid_device_opened": false,
            "operator_confirmed": true,
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "repeat": pre_zero_count,
            "hz": 1000,
            "watchdog_timeout_ms": 100,
            "fault_injected": "watchdog_timeout",
            "watchdog_triggered": true,
            "disconnect_observed": false,
            "write_attempts": pre_zero_count,
            "writes_ok": pre_zero_count + 1,
            "write_errors": 0,
            "watchdog_faults": 1,
            "final_zero_attempted": true,
            "final_zero_sent": true,
            "command_log": watchdog_command_log(pre_zero_count)
        })
    }

    fn disconnect_command_log(scheduled_count: u32) -> Vec<Value> {
        let mut records = Vec::new();
        for sequence in 0..scheduled_count {
            records.push(serde_json::json!({
                "sequence": sequence,
                "kind": "scheduled_zero",
                "elapsed_us": u64::from(sequence),
                "payload_hex": "2000000000000000",
                "report_id": "0x20",
                "torque_raw": 0,
                "flags": 0,
                "motor_enabled": false,
                "result": "ok",
                "bytes_written": 8
            }));
        }
        records.push(serde_json::json!({
            "sequence": scheduled_count,
            "kind": "disconnect_probe",
            "elapsed_us": u64::from(scheduled_count),
            "payload_hex": "2000000000000000",
            "report_id": "0x20",
            "torque_raw": 0,
            "flags": 0,
            "motor_enabled": false,
            "result": "error",
            "error": "device disconnected"
        }));
        records.push(serde_json::json!({
            "sequence": scheduled_count + 1,
            "kind": "final_zero",
            "elapsed_us": u64::from(scheduled_count + 1),
            "payload_hex": "2000000000000000",
            "report_id": "0x20",
            "torque_raw": 0,
            "flags": 0,
            "motor_enabled": false,
            "result": "error",
            "error": "device disconnected"
        }));
        records
    }

    fn real_disconnect_receipt() -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza disconnect-proof",
            "test_kind": "disconnect_proof",
            "generated_at_utc": TEST_GENERATED_AT,
            "no_feature_reports": true,
            "no_high_torque": true,
            "no_nonzero_torque": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "dry_run": false,
            "no_hid_device_opened": false,
            "operator_confirmed": true,
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "repeat": 10000,
            "hz": 1000,
            "watchdog_timeout_ms": 100,
            "max_duration_ms": 10000,
            "fault_injected": "operator_disconnect",
            "watchdog_triggered": false,
            "disconnect_observed": true,
            "write_attempts": 3,
            "writes_ok": 2,
            "write_errors": 2,
            "watchdog_faults": 0,
            "final_zero_attempted": true,
            "final_zero_sent": false,
            "final_zero_error": "device disconnected",
            "command_log": disconnect_command_log(2)
        })
    }

    fn receipt_with_lane_path(root: &Path, relative_path: &str, mut receipt: Value) -> Value {
        receipt["receipt_path"] = serde_json::json!(root.join(relative_path).display().to_string());
        receipt
    }

    fn write_zero_stage_receipts(root: &Path) -> TestResult {
        write_test_json_file(
            &root.join("zero-torque-proof.json"),
            &receipt_with_lane_path(root, "zero-torque-proof.json", real_zero_receipt(100)),
        )?;
        write_test_json_file(
            &root.join("watchdog-proof.json"),
            &receipt_with_lane_path(root, "watchdog-proof.json", real_watchdog_receipt(3)),
        )?;
        write_test_json_file(
            &root.join("disconnect-proof.json"),
            &receipt_with_lane_path(root, "disconnect-proof.json", real_disconnect_receipt()),
        )
    }

    fn init_feature_reports(mode_payload: &str, result: &str) -> Vec<Value> {
        vec![
            serde_json::json!({
                "sequence": 0,
                "kind": "start_input_reports",
                "payload_hex": "03000000",
                "report_id": "0x03",
                "result": result,
                "bytes_written": 4
            }),
            serde_json::json!({
                "sequence": 1,
                "kind": "ffb_mode",
                "payload_hex": mode_payload,
                "report_id": "0x11",
                "result": result,
                "bytes_written": 4
            }),
        ]
    }

    fn real_init_receipt(mode: &str) -> Value {
        let mode_payload = if mode == "off" {
            "11FF0000"
        } else {
            "11000000"
        };
        let mode_wire_value = if mode == "off" { "0xFF" } else { "0x00" };
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza init",
            "generated_at_utc": TEST_GENERATED_AT,
            "no_output_reports": true,
            "no_direct_torque_reports": true,
            "no_high_torque": true,
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "dry_run": false,
            "no_hid_device_opened": false,
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "mode": mode,
            "mode_wire_value": mode_wire_value,
            "init_state": "ready",
            "ready": true,
            "feature_report_count": 2,
            "feature_write_errors": 0,
            "output_report_attempts": 0,
            "feature_reports": init_feature_reports(mode_payload, "ok")
        })
    }

    fn write_low_torque_prerequisite_receipts(root: &Path) -> TestResult {
        write_test_json_file(
            &root.join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(&root.join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &root.join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;
        Ok(())
    }

    fn real_low_torque_receipt_for_lane(root: &Path, max_percent: f32) -> TestResult<Value> {
        let zero_proof = validate_zero_proof_for_torque_test(&root.join("zero-torque-proof.json"))?;
        let init_proofs = validate_init_proofs_for_torque_test(Some(root), None, None, false)?
            .ok_or("expected init proofs")?;
        let mut receipt = LowTorqueProofReceipt::new(
            Some("0x346E:0x0014".to_string()),
            sample_device(),
            Some(zero_proof),
            max_percent,
            1,
            1000,
            false,
        );
        receipt.generated_at_utc = TEST_LOW_TORQUE_GENERATED_AT.to_string();
        receipt.apply_init_proofs(Some(init_proofs));
        receipt.apply_direct_mode_gate(DirectModeGateSummary::trusted_descriptor(
            Path::new("descriptor.json"),
            "0x0014",
        ));
        let started_at = Instant::now();
        let mut sequence = 0u32;
        for stage in receipt.ladder.clone() {
            for _ in 0..stage.write_count {
                receipt.write_attempts += 1;
                receipt.writes_ok += 1;
                receipt.bytes_written_total += REPORT_LEN;
                receipt.record_command(LowTorqueCommandRecord::ok(
                    sequence,
                    "low_torque",
                    started_at,
                    stage.percent,
                    stage.payload,
                    REPORT_LEN,
                ));
                sequence += 1;
            }
        }

        let final_zero = zero_torque_payload_for_pid(product_ids::R5_V2);
        receipt.final_zero_attempted = true;
        receipt.final_zero_sent = true;
        receipt.writes_ok += 1;
        receipt.bytes_written_total += REPORT_LEN;
        receipt.record_command(LowTorqueCommandRecord::ok(
            sequence,
            "final_zero",
            started_at,
            0.0,
            final_zero,
            REPORT_LEN,
        ));
        receipt.success = receipt.zero_proof_validated
            && receipt.confirmed
            && receipt.init_proofs_validated
            && receipt.no_high_torque
            && receipt.high_torque == Some(false)
            && receipt.direct_mode_gate_satisfied
            && receipt.no_nonzero_above_limit
            && receipt.write_errors == 0
            && receipt.final_zero_sent;

        Ok(serde_json::to_value(receipt)?)
    }

    fn pit_house_receipt() -> Value {
        serde_json::json!({
            "success": true,
            "template": false,
            "command": "wheelctl moza pit-house-proof",
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "evidence_status": "observed_on_real_hardware",
            "high_torque": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "direct_requires_ack": true,
            "firmware_page_blocks_high_risk": true,
            "shared_control_risk": "warned",
            "cases": [
                {
                    "case": "pit_house_closed",
                    "observed": true,
                    "result": "staged_handshake_ok",
                    "evidence": "Pit House closed; staged init remained ready.",
                    "artifact": "pit-house-closed.json",
                    "pit_house_observation_artifact": "pit-house-observation-closed.json",
                    "source_receipt": "init-off.json",
                    "source_gate": "init_off_handshake",
                    "source_log": "feature_reports",
                    "source_record_kinds": ["start_input_reports", "ffb_mode"],
                    "high_torque": false
                },
                {
                    "case": "pit_house_open_idle_standard",
                    "observed": true,
                    "result": "standard_ok",
                    "evidence": "Pit House open and idle; standard mode completed without conflict.",
                    "artifact": "pit-house-open-standard.json",
                    "pit_house_observation_artifact": "pit-house-observation-open-standard.json",
                    "source_receipt": "init-standard.json",
                    "source_gate": "init_standard_handshake",
                    "source_log": "feature_reports",
                    "source_record_kinds": ["start_input_reports", "ffb_mode"],
                    "high_torque": false
                },
                {
                    "case": "pit_house_open_direct",
                    "observed": true,
                    "result": "blocked",
                    "blocked": true,
                    "operator_ack_required": true,
                    "evidence": "Direct mode was blocked until explicit operator acknowledgement.",
                    "artifact": "pit-house-direct-blocked.json",
                    "pit_house_observation_artifact": "pit-house-observation-open-direct.json",
                    "source_receipt": "low-torque-proof.json",
                    "source_gate": "low_torque_bounded",
                    "source_log": "command_log",
                    "source_record_kind": "low_torque",
                    "high_torque": false
                },
                {
                    "case": "pit_house_mode_change_during_run",
                    "observed": true,
                    "result": "mismatch_detected",
                    "mismatch_detected": true,
                    "failed_safe": true,
                    "evidence": "Mode mismatch was detected and output failed safe.",
                    "artifact": "pit-house-mode-change.json",
                    "pit_house_observation_artifact": "pit-house-observation-mode-change.json",
                    "source_receipt": "simulator-ffb-smoke.json",
                    "source_gate": "simulator_ffb_bounded",
                    "source_log": "output_log_artifact",
                    "source_record_kind": "clear_zero",
                    "source_clear_event": "mode_mismatch",
                    "source_requires_final_zero": true,
                    "high_torque": false
                },
                {
                    "case": "pit_house_firmware_update_page_open",
                    "observed": true,
                    "result": "high_risk_refused",
                    "high_risk_refused": true,
                    "evidence": "Firmware/update page open; high-risk tests refused.",
                    "artifact": "pit-house-firmware-page.json",
                    "pit_house_observation_artifact": "pit-house-observation-firmware-page.json",
                    "source_receipt": "support-bundle.json",
                    "source_gate": "service_status_receipts",
                    "source_log": "device_statuses",
                    "high_torque": false
                }
            ]
        })
    }

    fn simulator_telemetry_receipt() -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza simulator-telemetry-proof",
            "game": "simhub-bridge",
            "telemetry_source": "simhub_bridge",
            "recorder_command": SIMULATOR_TELEMETRY_RECORDER_COMMAND,
            "recorder_session_id": "sim-telemetry-session-001",
            "hardware_output_enabled": false,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "normalized_snapshot_count": 120,
            "duration_ms": 5000,
            "recorder_artifact": "simulator-telemetry-recording.jsonl",
            "faults": []
        })
    }

    fn simulator_ffb_receipt() -> Value {
        let mut receipt = serde_json::json!({
            "success": true,
            "command": "wheelctl moza simulator-ffb-smoke",
            "game": "simhub-bridge",
            "telemetry_source": "simhub_bridge",
            "hardware": "moza-r5",
            "ffb_mode": "direct",
            "descriptor_trusted": true,
            "explicit_operator_override": false,
            "high_torque": false,
            "no_high_torque": true,
            "no_hid_device_opened": false,
            "no_ffb_writes": false,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "hardware_prerequisites_validated": true,
            "prerequisite_gates": simulator_ffb_prerequisite_gate_receipt_value(),
            "device": {
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "product_name": "Moza R5",
                "output_capable": true
            },
            "hardware_output_enabled": true,
            "max_output_percent": 5.0,
            "max_abs_output_percent": 4.2,
            "watchdog_active": true,
            "watchdog_timeout_ms": 100,
            "output_report_count": 240,
            "nonzero_output_count": 180,
            "zero_output_count": 60,
            "input_telemetry_artifact": "simulator-telemetry-recording.jsonl",
            "input_telemetry_snapshot_count": 120,
            "output_log_artifact": "simulator-ffb-output.jsonl",
            "output_log_provenance_valid": true,
            "writer_command": SIMULATOR_FFB_WRITER_COMMAND,
            "writer_session_id": "sim-ffb-session-001",
            "writer_device_path": "\\\\?\\hid#vid_346e&pid_0014&mi_00",
            "writer_product_id": "0x0014",
            "final_zero_attempted": true,
            "final_zero_sent": true,
            "final_zero_payload_hex": "2000000000000000",
            "stop_cleared_output": true,
            "pause_cleared_output": true,
            "game_exit_cleared_output": true,
            "mode_mismatch_cleared_output": true,
            "faults": []
        });
        if let Some(object) = receipt.as_object_mut() {
            object.insert(
                "input_telemetry_recorder_session_id".to_string(),
                serde_json::json!("sim-telemetry-session-001"),
            );
            object.insert(
                "writer_started_at_utc".to_string(),
                serde_json::json!("2026-05-06T00:00:02Z"),
            );
            object.insert(
                "writer_completed_at_utc".to_string(),
                serde_json::json!("2026-05-06T00:00:03Z"),
            );
            object.insert("writer_hardware_lane".to_string(), Value::Null);
        }
        receipt
    }

    fn simulator_ffb_prerequisite_gate_receipt_value() -> Value {
        serde_json::json!([
            {
                "name": "zero_torque_real_hardware",
                "status": "pass",
                "details": "test prerequisite fixture"
            },
            {
                "name": "watchdog_zero_output",
                "status": "pass",
                "details": "test prerequisite fixture"
            },
            {
                "name": "disconnect_final_zero",
                "status": "pass",
                "details": "test prerequisite fixture"
            },
            {
                "name": "init_off_handshake",
                "status": "pass",
                "details": "test prerequisite fixture"
            },
            {
                "name": "init_standard_handshake",
                "status": "pass",
                "details": "test prerequisite fixture"
            },
            {
                "name": "low_torque_bounded",
                "status": "pass",
                "details": "test prerequisite fixture"
            }
        ])
    }

    fn pit_house_observation_receipt(case_id: &str, observed_state: &str) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza pit-house-observation",
            "case": case_id,
            "observed": true,
            "pit_house_observed_state": observed_state,
            "evidence_kind": "process_window_snapshot",
            "observed_at_utc": "2026-05-06T00:00:00Z",
            "operator": "Steven",
            "evidence": format!("Pit House observation recorded for {observed_state}."),
            "evidence_artifact": pit_house_observation_evidence_artifact(observed_state),
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true
        })
    }

    fn pit_house_observation_evidence_artifact(observed_state: &str) -> String {
        format!("pit-house-evidence-{observed_state}.json")
    }

    fn write_pit_house_artifacts(root: &Path) -> TestResult {
        write_trusted_descriptor_if_missing(root)?;
        write_test_json_file(&root.join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &root.join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;
        write_test_json_file(
            &root.join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(&root.join("watchdog-proof.json"), &real_watchdog_receipt(3))?;
        write_test_json_file(
            &root.join("low-torque-proof.json"),
            &real_low_torque_receipt_for_lane(root, 2.0)?,
        )?;
        write_simulator_artifacts(root)?;
        for (artifact, case_id, observed_state) in [
            (
                "pit-house-observation-closed.json",
                "pit_house_closed",
                "closed",
            ),
            (
                "pit-house-observation-open-standard.json",
                "pit_house_open_idle_standard",
                "open_idle_standard",
            ),
            (
                "pit-house-observation-open-direct.json",
                "pit_house_open_direct",
                "open_direct",
            ),
            (
                "pit-house-observation-mode-change.json",
                "pit_house_mode_change_during_run",
                "mode_change_during_run",
            ),
            (
                "pit-house-observation-firmware-page.json",
                "pit_house_firmware_update_page_open",
                "firmware_update_page_open",
            ),
        ] {
            let evidence_artifact = pit_house_observation_evidence_artifact(observed_state);
            write_test_json_file(
                &root.join(evidence_artifact),
                &serde_json::json!({
                    "case": case_id,
                    "pit_house_observed_state": observed_state,
                    "captured_at_utc": "2026-05-06T00:00:00Z",
                    "source": "test_process_window_snapshot"
                }),
            )?;
            write_test_json_file(
                &root.join(artifact),
                &pit_house_observation_receipt(case_id, observed_state),
            )?;
        }
        for (artifact, receipt) in [
            (
                "pit-house-closed.json",
                serde_json::json!({
                    "case": "pit_house_closed",
                    "observed": true,
                    "result": "staged_handshake_ok",
                    "pit_house_state": "closed",
                    "staged_handshake_ready": true,
                    "conflict_detected": false,
                    "high_torque": false,
                    "no_serial_config_commands": true,
                    "no_firmware_or_dfu_commands": true,
                    "pit_house_observation_artifact": "pit-house-observation-closed.json",
                    "source_receipt": "init-off.json",
                    "source_gate": "init_off_handshake",
                    "source_log": "feature_reports",
                    "source_record_kinds": ["start_input_reports", "ffb_mode"],
                    "evidence": "Pit House closed; staged handshake remained ready."
                }),
            ),
            (
                "pit-house-open-standard.json",
                serde_json::json!({
                    "case": "pit_house_open_idle_standard",
                    "observed": true,
                    "result": "standard_ok",
                    "pit_house_state": "open_idle",
                    "ffb_mode": "standard",
                    "direct_mode_requested": false,
                    "high_torque": false,
                    "no_serial_config_commands": true,
                    "no_firmware_or_dfu_commands": true,
                    "pit_house_observation_artifact": "pit-house-observation-open-standard.json",
                    "source_receipt": "init-standard.json",
                    "source_gate": "init_standard_handshake",
                    "source_log": "feature_reports",
                    "source_record_kinds": ["start_input_reports", "ffb_mode"],
                    "evidence": "Pit House open and idle; standard mode completed without conflict."
                }),
            ),
            (
                "pit-house-direct-blocked.json",
                serde_json::json!({
                    "case": "pit_house_open_direct",
                    "observed": true,
                    "result": "blocked",
                    "pit_house_state": "open",
                    "direct_mode_requested": true,
                    "blocked": true,
                    "operator_ack_required": true,
                    "high_torque": false,
                    "no_serial_config_commands": true,
                    "no_firmware_or_dfu_commands": true,
                    "pit_house_observation_artifact": "pit-house-observation-open-direct.json",
                    "source_receipt": "low-torque-proof.json",
                    "source_gate": "low_torque_bounded",
                    "source_log": "command_log",
                    "source_record_kind": "low_torque",
                    "evidence": "Direct mode blocked until explicit operator acknowledgement."
                }),
            ),
            (
                "pit-house-mode-change.json",
                serde_json::json!({
                    "case": "pit_house_mode_change_during_run",
                    "observed": true,
                    "result": "mismatch_detected",
                    "mismatch_detected": true,
                    "failed_safe": true,
                    "output_cleared": true,
                    "final_zero_attempted": true,
                    "high_torque": false,
                    "no_serial_config_commands": true,
                    "no_firmware_or_dfu_commands": true,
                    "pit_house_observation_artifact": "pit-house-observation-mode-change.json",
                    "source_receipt": "simulator-ffb-smoke.json",
                    "source_gate": "simulator_ffb_bounded",
                    "source_log": "output_log_artifact",
                    "source_record_kind": "clear_zero",
                    "source_clear_event": "mode_mismatch",
                    "source_requires_final_zero": true,
                    "evidence": "Mode mismatch was detected and output failed safe."
                }),
            ),
            (
                "pit-house-firmware-page.json",
                serde_json::json!({
                    "case": "pit_house_firmware_update_page_open",
                    "observed": true,
                    "result": "high_risk_refused",
                    "firmware_update_page_open": true,
                    "high_risk_refused": true,
                    "high_torque": false,
                    "no_serial_config_commands": true,
                    "no_firmware_or_dfu_commands": true,
                    "pit_house_observation_artifact": "pit-house-observation-firmware-page.json",
                    "source_receipt": "support-bundle.json",
                    "source_gate": "service_status_receipts",
                    "source_log": "device_statuses",
                    "evidence": "Firmware/update page open; high-risk tests refused."
                }),
            ),
        ] {
            write_test_json_file(&root.join(artifact), &receipt)?;
        }
        write_test_json_file(
            &root.join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;
        write_service_status_artifacts(root)?;
        Ok(())
    }

    fn write_simulator_artifacts(root: &Path) -> TestResult {
        write_trusted_descriptor_if_missing(root)?;
        write_test_json_file(&root.join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &root.join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;
        write_test_json_file(
            &root.join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(&root.join("watchdog-proof.json"), &real_watchdog_receipt(3))?;
        write_test_json_file(
            &root.join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;
        write_test_json_file(
            &root.join("low-torque-proof.json"),
            &real_low_torque_receipt_for_lane(root, 2.0)?,
        )?;
        write_simulator_telemetry_jsonl(&root.join("simulator-telemetry-recording.jsonl"), 120)?;
        write_simulator_ffb_output_jsonl(&root.join("simulator-ffb-output.jsonl"), 240, 180, 60)?;
        write_test_json_file(
            &root.join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;
        Ok(())
    }

    fn service_device_status_value(root: &Path) -> Value {
        serde_json::json!({
            "device": {
                "id": "moza-r5",
                "name": "Moza R5",
                "vendor_id": "0x346E",
                "product_id": "0x0014",
                "device_type": "WheelBase",
                "state": "Connected",
                "capabilities": {
                    "supports_pid": false,
                    "supports_raw_torque_1khz": true,
                    "supports_health_stream": false,
                    "supports_led_bus": false,
                    "max_torque_nm": 5.5,
                    "encoder_cpr": 1024,
                    "min_report_period_us": 1000
                }
            },
            "last_seen": "2026-05-06T00:00:00Z",
            "active_faults": [],
            "telemetry": {
                "wheel_angle_deg": 0.0,
                "wheel_speed_rad_s": 0.0,
                "temperature_c": 25,
                "hands_on": false
            },
            "moza": {
                "model": "Moza R5",
                "product_id": "0x0014",
                "category": "wheelbase",
                "output_capable": true,
                "ffb_ready": false,
                "init_state": "uninitialized",
                "descriptor_trusted": false,
                "descriptor_crc32": "0x12345678",
                "descriptor_source": "operator_supplied_hex",
                "lane": root.display().to_string(),
                "direct_mode_allowed": false,
                "high_torque_allowed": false,
                "safe_to_send_torque": false,
                "safety_state": "lane_zero_torque_verified",
                "safety_reason": "stored Moza lane verification receipts report highest_passing_stage=zero, next_required_stage=smoke_ready; service status remains observe-only"
            }
        })
    }

    fn service_device_status_receipt(root: &Path) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl device status",
            "device_selector": "moza-r5",
            "moza_lane": root.display().to_string(),
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "status": service_device_status_value(root)
        })
    }

    fn support_bundle_receipt(root: &Path) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl support-bundle",
            "timestamp": "2026-05-06T00:00:00Z",
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "system_info": {
                "os": "windows",
                "arch": "x86_64",
                "version": "0.1.0"
            },
            "devices": [
                {
                    "id": "moza-r5",
                    "name": "Moza R5",
                    "vendor_id": "0x346E",
                    "product_id": "0x0014"
                }
            ],
            "device_statuses": [
                {
                    "device_id": "moza-r5",
                    "status": "ok",
                    "device_status": service_device_status_value(root)
                }
            ],
            "device_filter": "moza-r5",
            "blackbox_included": false,
            "moza_lane": support_bundle_status(root)
        })
    }

    fn write_service_status_artifacts(root: &Path) -> TestResult {
        write_test_json_file(
            &root.join("moza-status.json"),
            &moza_status_receipt(vec![sample_device()], Some("0x0014"), Some(root)),
        )?;
        write_test_json_file(
            &root.join("device-status.json"),
            &service_device_status_receipt(root),
        )?;
        write_test_json_file(
            &root.join("support-bundle.json"),
            &support_bundle_receipt(root),
        )?;
        write_test_json_file(
            &root.join("support-bundle.json"),
            &support_bundle_receipt(root),
        )?;
        Ok(())
    }

    fn write_smoke_ready_bundle(root: &Path) -> TestResult {
        write_minimal_passive_bundle(root)?;
        write_pit_house_artifacts(root)?;
        write_simulator_artifacts(root)?;
        write_test_json_file(
            &root.join("manifest.json"),
            &sample_lane_manifest("real_hardware_smoke_ready", true, true),
        )?;
        write_test_json_file(
            &root.join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(&root.join("watchdog-proof.json"), &real_watchdog_receipt(3))?;
        write_test_json_file(
            &root.join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;
        write_test_json_file(&root.join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &root.join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;
        write_test_json_file(
            &root.join("low-torque-proof.json"),
            &real_low_torque_receipt_for_lane(root, 2.0)?,
        )?;
        write_test_json_file(
            &root.join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;
        write_test_json_file(
            &root.join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;
        write_test_json_file(
            &root.join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;
        write_service_status_artifacts(root)?;

        Ok(())
    }

    fn stored_verification_receipt(root: &Path, stage: MozaBundleStage) -> Value {
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza verify-bundle",
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "lane": root.display().to_string(),
            "requested_stage": stage_label(stage),
            "missing_artifacts": 0,
            "invalid_artifacts": 0,
            "failed_gates": 0,
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "artifacts": [],
            "gates": []
        })
    }

    fn stored_promotion_receipt(root: &Path, stage: MozaBundleStage) -> Value {
        let (completion_state, hardware_validated, simulator_validated) =
            manifest_promotion_values(stage);
        let summary = serde_json::json!({
            "success": true,
            "requested_stage": stage_label(stage),
            "missing_artifacts": 0,
            "invalid_artifacts": 0,
            "failed_gates": 0,
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true
        });
        serde_json::json!({
            "success": true,
            "command": "wheelctl moza promote-manifest",
            "generated_at_utc": "2026-05-06T00:00:00Z",
            "lane": root.display().to_string(),
            "manifest": root.join("manifest.json").display().to_string(),
            "stage": stage_label(stage),
            "previous_completion_state": "not_started",
            "previous_hardware_validated": false,
            "previous_simulator_validated": false,
            "completion_state": completion_state,
            "hardware_validated": hardware_validated,
            "simulator_validated": simulator_validated,
            "high_torque_validated": false,
            "release_ready": false,
            "no_hid_device_opened": true,
            "no_ffb_writes": true,
            "no_serial_config_commands": true,
            "no_firmware_or_dfu_commands": true,
            "verification_before": summary,
            "verification_after": summary
        })
    }

    fn write_stage_audit_receipts(root: &Path, stage: MozaBundleStage) -> TestResult {
        for audited_stage in audit_stages_through(stage) {
            write_test_json_file(
                &root.join(verification_receipt_path(audited_stage)),
                &stored_verification_receipt(root, audited_stage),
            )?;
            write_test_json_file(
                &root.join(promotion_receipt_path(audited_stage)),
                &stored_promotion_receipt(root, audited_stage),
            )?;
        }
        Ok(())
    }

    fn write_lane_audit_receipts(root: &Path, stage: MozaBundleStage) -> TestResult {
        write_stage_audit_receipts(root, stage)?;
        for audited_stage in audit_stages_through(stage) {
            let receipt = audit_lane_dir(root, audited_stage);
            let value = serde_json::to_value(receipt)?;
            write_test_json_file(&root.join(audit_receipt_path(audited_stage)), &value)?;
        }
        Ok(())
    }

    #[test]
    fn pid_selector_matches_product_id() {
        let device = sample_device();
        assert!(selector_matches(&device, Some("0x0014")));
        assert!(selector_matches(&device, Some("0014")));
        assert!(!selector_matches(&device, Some("0x0004")));
    }

    #[test]
    fn vid_pid_selector_matches_device_identity() {
        let device = sample_device();
        assert!(selector_matches(&device, Some("0x346E:0x0014")));
        assert!(!selector_matches(&device, Some("0x346E:0x0004")));
        assert!(!selector_matches(&device, Some("0x046D:0x0014")));
    }

    #[test]
    fn hid_observe_selector_matches_full_endpoint_identity() {
        let device = sample_device();
        assert!(selector_matches(
            &device,
            Some("hid-0x346E-0x0014-if0-0x0001-0x0004")
        ));
        assert!(!selector_matches(
            &device,
            Some("hid-0x346E-0x0004-if0-0x0001-0x0004")
        ));
        assert!(!selector_matches(
            &device,
            Some("hid-0x346E-0x0014-if2-0x0001-0x0004")
        ));
        assert!(!selector_matches(
            &device,
            Some("hid-0x346E-0x0014-if0-0x0001-0x0005")
        ));
    }

    #[test]
    fn text_selector_matches_path_and_product_name() {
        let device = sample_device();
        assert!(selector_matches(&device, Some("pid_0014")));
        assert!(selector_matches(&device, Some("r5")));
        assert!(!selector_matches(&device, Some("hbp")));
    }

    #[test]
    fn known_moza_products_have_expected_protocol_hints() {
        assert_eq!(
            expected_input_report_lengths(product_ids::R5_V2),
            vec![7, 31]
        );
        assert_eq!(
            expected_output_report_ids(true),
            vec![DIRECT_TORQUE_REPORT_ID.to_string()]
        );
        assert_eq!(
            expected_feature_report_ids(true),
            vec![
                HIGH_TORQUE_FEATURE_REPORT_ID.to_string(),
                START_REPORTING_FEATURE_REPORT_ID.to_string(),
                FFB_MODE_FEATURE_REPORT_ID.to_string(),
            ]
        );
    }

    #[test]
    fn probe_receipt_serializes_without_raw_serial() -> TestResult {
        let receipt = ProbeReceipt {
            success: true,
            command: "wheelctl moza probe",
            generated_at_utc: "2026-05-06T00:00:00Z".to_string(),
            vendor_id: "0x346E".to_string(),
            no_hid_device_opened: true,
            no_ffb_writes: true,
            no_serial_config_commands: true,
            no_firmware_or_dfu_commands: true,
            devices: vec![sample_device()],
            notes: Vec::new(),
        };

        let value = serde_json::to_value(receipt)?;
        assert_eq!(value["success"], true);
        assert_eq!(value["devices"][0]["serial_number_present"], true);
        assert!(value["devices"][0].get("serial_number").is_none());
        Ok(())
    }

    #[test]
    fn validate_capture_accepts_wheelbase_jsonl() -> TestResult {
        let (_dir, path) = write_temp_capture(&[
            r#"{"product_id":"0x0014","report_len":7,"data":["0x01","0x00","0x80","0x34","0x12","0x78","0x56"]}"#,
        ])?;

        let receipt = validate_capture_file(&path, None)?;

        assert!(receipt.success);
        assert_eq!(receipt.total_reports, 1);
        assert_eq!(receipt.parsed_reports, 1);
        assert_eq!(receipt.rejected_reports, 0);
        assert_eq!(receipt.parsed_by_category.get("wheelbase"), Some(&1));
        let steering = receipt
            .axis_ranges
            .get("steering_u16")
            .ok_or("expected steering axis stats")?;
        assert_eq!(steering.min, Some(0x8000));
        assert_eq!(steering.max, Some(0x8000));
        Ok(())
    }

    #[test]
    fn validate_capture_rejects_short_wheelbase_report() -> TestResult {
        let (_dir, path) =
            write_temp_capture(&[r#"{"product_id":"0x0014","report_len":2,"data_hex":"0100"}"#])?;

        let receipt = validate_capture_file(&path, None)?;

        assert!(!receipt.success);
        assert_eq!(receipt.total_reports, 1);
        assert_eq!(receipt.parsed_reports, 0);
        assert_eq!(receipt.rejected_reports, 1);
        assert_eq!(receipt.line_errors.len(), 1);
        Ok(())
    }

    #[test]
    fn analyze_capture_reports_raw_byte_and_word_movement() -> TestResult {
        let (_dir, path) = write_temp_capture(&[
            &capture_line(
                product_ids::R5_V1,
                &live_r5_v1_extended_aux_report_hex(0x0001, 0x0100),
            ),
            &capture_line(
                product_ids::R5_V1,
                &live_r5_v1_extended_aux_report_hex(0x00FF, 0x0200),
            ),
        ])?;

        let receipt = analyze_capture_file(&path)?;

        assert!(receipt.success);
        assert_eq!(receipt.total_reports, 2);
        assert_eq!(receipt.decoded_reports, 2);
        assert_eq!(receipt.rejected_reports, 0);
        assert_eq!(receipt.product_ids.get("0x0004"), Some(&2));
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_ffb_writes);
        assert!(receipt.no_output_reports);
        assert!(receipt.no_feature_reports);
        assert!(receipt.no_serial_config_commands);
        assert!(receipt.no_firmware_or_dfu_commands);
        assert!(receipt.moving_bytes.contains(&34));
        assert!(receipt.moving_words_le.contains(&34));
        let aux0 = receipt
            .word_ranges_le
            .iter()
            .find(|range| range.start_index == 34)
            .ok_or("expected word range at byte 34")?;
        assert_eq!(aux0.min, 0x0001);
        assert_eq!(aux0.max, 0x00FF);
        assert!(aux0.changed);
        Ok(())
    }

    #[test]
    fn validate_capture_pid_override_accepts_standalone_hbp() -> TestResult {
        let (_dir, path) = write_temp_capture(&[r#"{"report_len":3,"data_hex":"FF7F01"}"#])?;

        let receipt = validate_capture_file(&path, Some(product_ids::HBP_HANDBRAKE))?;

        assert!(receipt.success);
        assert_eq!(receipt.parsed_by_category.get("handbrake"), Some(&1));
        let handbrake = receipt
            .axis_ranges
            .get("handbrake_u16")
            .ok_or("expected handbrake axis stats")?;
        assert_eq!(handbrake.min, Some(0x7FFF));
        Ok(())
    }

    #[test]
    fn validate_lane_captures_covers_every_required_capture() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let receipt = validate_lane_captures(dir.path())?;

        assert!(receipt.success);
        let requirements = passive_capture_requirements_for_lane(dir.path());
        assert_eq!(receipt.required_capture_count, requirements.len());
        assert_eq!(
            receipt.validated_capture_count,
            receipt.required_capture_count
        );
        assert!(receipt.total_reports > 0);
        assert_eq!(receipt.rejected_reports, 0);
        for requirement in requirements {
            assert!(receipt.captures.iter().any(|entry| {
                entry.fixture_id == requirement.fixture_id
                    && path_string_ends_with(&entry.capture, requirement.relative_path)
                    && entry.success
            }));
        }
        Ok(())
    }

    #[test]
    fn build_capture_fixture_writes_sanitized_parser_fixture() -> TestResult {
        let (_dir, path) = write_temp_capture(&[
            r#"{"product_id":"0x0014","report_len":7,"path":"hid-path","data":["0x01","0x00","0x80","0x34","0x12","0x78","0x56"]}"#,
        ])?;

        let fixture = build_capture_fixture(&path, "r5_v2_idle", None, 8)?;
        let fixture_json = serde_json::to_value(&fixture)?;

        assert_eq!(fixture.fixture_id, "r5_v2_idle");
        assert_eq!(fixture.total_reports, 1);
        assert_eq!(fixture.included_reports, 1);
        assert!(!fixture.fixture_truncated);
        assert_eq!(fixture.product_ids.get("0x0014"), Some(&1));
        assert_eq!(fixture.parsed_by_category.get("wheelbase"), Some(&1));
        assert_eq!(fixture.reports[0].data_hex, "01008034127856");
        assert_eq!(fixture.reports[0].parsed.steering_u16, 0x8000);
        assert!(fixture_json.get("path").is_none());
        assert!(!fixture_json.to_string().contains("hid-path"));
        assert!(!fixture_json.to_string().contains("serial"));
        Ok(())
    }

    #[test]
    fn build_capture_fixture_rejects_short_parser_input() -> TestResult {
        let (_dir, path) =
            write_temp_capture(&[r#"{"product_id":"0x0014","report_len":2,"data_hex":"0100"}"#])?;

        let result = build_capture_fixture(&path, "bad_short", None, 8);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn build_capture_fixture_respects_max_reports_after_validating_all() -> TestResult {
        let (_dir, path) = write_temp_capture(&[
            r#"{"product_id":"0x0014","report_len":7,"data_hex":"01008034127856"}"#,
            r#"{"product_id":"0x0014","report_len":7,"data_hex":"01008134127856"}"#,
        ])?;

        let fixture = build_capture_fixture(&path, "r5_v2_pair", None, 1)?;

        assert_eq!(fixture.total_reports, 2);
        assert_eq!(fixture.included_reports, 1);
        assert!(fixture.fixture_truncated);
        assert_eq!(fixture.product_ids.get("0x0014"), Some(&2));
        Ok(())
    }

    #[tokio::test]
    async fn promote_fixture_refuses_to_overwrite_without_flag() -> TestResult {
        let (_capture_dir, capture) = write_temp_capture(&[
            r#"{"product_id":"0x0014","report_len":7,"data_hex":"01008034127856"}"#,
        ])?;
        let fixture_dir = tempfile::tempdir()?;
        let fixture_out = fixture_dir.path().join("fixture.json");
        fs::write(&fixture_out, "{}")?;

        let result = promote_fixture(
            false,
            &capture,
            "r5_v2_idle",
            &fixture_out,
            None,
            8,
            false,
            None,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn promote_fixtures_writes_all_required_fixture_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let fixture_dir = dir.path().join("promoted-fixtures");
        let receipt_path = dir.path().join("fixture-promotion.json");

        promote_fixtures(
            true,
            dir.path(),
            &fixture_dir,
            16,
            true,
            Some(&receipt_path),
        )
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza promote-fixtures")
        );
        assert_eq!(
            json_u64(&receipt, "fixture_count"),
            Some(passive_capture_requirements_for_lane(dir.path()).len() as u64)
        );
        for requirement in passive_capture_requirements_for_lane(dir.path()) {
            assert!(
                fixture_dir
                    .join(format!("{}.json", requirement.fixture_id))
                    .is_file()
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn promote_fixtures_refuses_invalid_required_capture_before_writing() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;
        let fixture_dir = dir.path().join("promoted-fixtures");
        let receipt_path = dir.path().join("fixture-promotion.json");

        let result = promote_fixtures(
            false,
            dir.path(),
            &fixture_dir,
            16,
            true,
            Some(&receipt_path),
        )
        .await;

        assert!(result.is_err());
        let message = result
            .err()
            .map(|error| error.to_string())
            .ok_or("expected promote-fixtures to fail")?;
        assert!(
            message.contains("refusing to promote passive fixtures")
                && message.contains("captures/r5-throttle-only-sweep.jsonl"),
            "expected invalid throttle capture in promotion preflight error, got {message}"
        );
        assert!(
            !fixture_dir.join("r5_idle.json").exists(),
            "promote-fixtures must not write partial fixture outputs after preflight failure"
        );
        Ok(())
    }

    #[test]
    fn ci_lane_readme_lists_every_required_passive_capture() -> TestResult {
        let readme_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md");
        let readme = fs::read_to_string(&readme_path)?;

        for requirement in default_passive_capture_requirements() {
            assert!(
                readme.contains(requirement.relative_path),
                "expected {} to document {}",
                readme_path.display(),
                requirement.relative_path
            );
        }

        Ok(())
    }

    #[test]
    fn ci_lane_readme_orders_pit_house_after_simulator_ffb_smoke() -> TestResult {
        let readme_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md");
        let readme = fs::read_to_string(&readme_path)?;
        let ffb_index = readme
            .find("wheelctl moza simulator-ffb-smoke")
            .ok_or("README missing simulator FFB smoke command")?;
        let pit_house_index = readme
            .find("wheelctl moza pit-house-proof")
            .ok_or("README missing Pit House proof command")?;

        assert!(
            ffb_index < pit_house_index,
            "pit-house-proof must run after simulator-ffb-smoke because mode-change evidence links to simulator-ffb-smoke.json"
        );
        assert!(readme.contains("mode_mismatch"));
        Ok(())
    }

    #[test]
    fn ci_lane_docs_use_wheelctl_descriptor_receipt_producer() -> TestResult {
        let readme_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md");
        let validation_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware/moza-r5-validation.md");
        let readme = fs::read_to_string(&readme_path)?;
        let validation = fs::read_to_string(&validation_path)?;

        for (path, text) in [
            (readme_path.as_path(), readme.as_str()),
            (validation_path.as_path(), validation.as_str()),
        ] {
            assert!(
                text.contains("wheelctl moza descriptor --json-out"),
                "expected {} to document wheelctl descriptor as the lane receipt producer",
                path.display()
            );
            assert!(
                text.contains("wheelctl moza descriptor --device <r5> --report-descriptor-hex"),
                "expected {} to document operator-supplied descriptor hex through wheelctl",
                path.display()
            );
            assert!(
                text.contains("--report-descriptor-bin-file"),
                "expected {} to document operator-supplied binary descriptor bytes through wheelctl",
                path.display()
            );
        }

        Ok(())
    }

    #[test]
    fn ci_lane_readme_manifest_starter_matches_schema() -> TestResult {
        let readme_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md");
        let readme = fs::read_to_string(&readme_path)?;
        let manifest_block = fenced_block_after(&readme, "## Manifest Starter", "json")?;
        let manifest: Value = serde_json::from_str(&manifest_block)?;

        assert!(manifest_schema_validation_errors(&manifest).is_empty());
        assert!(manifest_contract_is_moza_r5_lane(&manifest));
        assert_eq!(
            json_string(&manifest, "completion_state"),
            Some("not_started")
        );
        assert_eq!(json_bool(&manifest, "hardware_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "simulator_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "high_torque_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "release_ready"), Some(false));
        Ok(())
    }

    #[test]
    fn manifest_topology_contract_uses_declared_roles_not_fixed_kit() -> TestResult {
        let mut manifest = sample_lane_manifest("not_started", false, false);
        let controls = manifest
            .pointer_mut("/topology/logical_controls")
            .and_then(Value::as_object_mut)
            .ok_or("expected topology logical_controls object")?;
        controls.clear();
        controls.insert(
            "steering".to_string(),
            serde_json::json!({
                "role": "steering",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-steering-sweep.jsonl",
                "semantic_status": "deferred"
            }),
        );
        controls.insert(
            "brake".to_string(),
            serde_json::json!({
                "role": "brake",
                "source_endpoint": "moza-r5-if2",
                "connection": "wheelbase_hub",
                "required": true,
                "evidence_capture": "captures/r5-brake-only-sweep.jsonl",
                "semantic_status": "generic_aux"
            }),
        );

        assert!(manifest_topology_is_logical_role_model(&manifest));

        {
            let controls = manifest
                .pointer_mut("/topology/logical_controls")
                .and_then(Value::as_object_mut)
                .ok_or("expected topology logical_controls object")?;
            controls.insert(
                "unmapped".to_string(),
                serde_json::json!({
                    "role": "unmapped",
                    "source_endpoint": "moza-r5-if2",
                    "connection": "wheelbase_hub",
                    "required": true,
                    "evidence_capture": "captures/r5-brake-only-sweep.jsonl",
                    "semantic_status": "deferred"
                }),
            );
        }

        assert!(!manifest_topology_is_logical_role_model(&manifest));

        {
            let controls = manifest
                .pointer_mut("/topology/logical_controls")
                .and_then(Value::as_object_mut)
                .ok_or("expected topology logical_controls object")?;
            let unmapped = controls
                .get_mut("unmapped")
                .ok_or("expected unmapped test control")?;
            unmapped["role"] = serde_json::json!("brake");
            unmapped["semantic_status"] = serde_json::json!("guessed");
        }

        assert!(!manifest_topology_is_logical_role_model(&manifest));
        Ok(())
    }

    #[test]
    fn manifest_schema_accepts_lane_states_and_rejects_overclaims() -> TestResult {
        for (completion_state, hardware_validated, simulator_validated) in [
            ("not_started", false, false),
            ("passive_capture_ready", false, false),
            ("zero_torque_ready", false, false),
            ("real_hardware_smoke_ready", true, true),
        ] {
            let manifest =
                sample_lane_manifest(completion_state, hardware_validated, simulator_validated);
            assert!(
                manifest_schema_validation_errors(&manifest).is_empty(),
                "expected schema to accept {completion_state}"
            );
            assert!(manifest_contract_is_moza_r5_lane(&manifest));
        }

        let mut missing_semantic_status = sample_lane_manifest("not_started", false, false);
        missing_semantic_status
            .pointer_mut("/topology/logical_controls/throttle")
            .and_then(Value::as_object_mut)
            .ok_or("expected throttle topology control")?
            .remove("semantic_status");
        assert!(!manifest_schema_validation_errors(&missing_semantic_status).is_empty());
        assert!(!manifest_topology_is_logical_role_model(
            &missing_semantic_status
        ));

        let mut invalid_semantic_status = sample_lane_manifest("not_started", false, false);
        invalid_semantic_status["topology"]["logical_controls"]["throttle"]["semantic_status"] =
            serde_json::json!("guessed");
        assert!(!manifest_schema_validation_errors(&invalid_semantic_status).is_empty());
        assert!(!manifest_topology_is_logical_role_model(
            &invalid_semantic_status
        ));

        let mut passive_overclaim = sample_lane_manifest("passive_capture_ready", true, false);
        assert!(!manifest_schema_validation_errors(&passive_overclaim).is_empty());

        passive_overclaim["hardware_validated"] = serde_json::json!(false);
        passive_overclaim["release_ready"] = serde_json::json!(true);
        assert!(!manifest_schema_validation_errors(&passive_overclaim).is_empty());

        let mut smoke_underclaim = sample_lane_manifest("real_hardware_smoke_ready", true, false);
        assert!(!manifest_schema_validation_errors(&smoke_underclaim).is_empty());

        smoke_underclaim["simulator_validated"] = serde_json::json!(true);
        smoke_underclaim["high_torque_validated"] = serde_json::json!(true);
        assert!(!manifest_schema_validation_errors(&smoke_underclaim).is_empty());
        Ok(())
    }

    #[test]
    fn manifest_artifacts_cover_support_lane_index() -> TestResult {
        let manifest = sample_lane_manifest("not_started", false, false);
        let artifacts = manifest
            .get("artifacts")
            .and_then(Value::as_object)
            .ok_or("expected manifest artifacts object")?;
        let artifact_values = artifacts
            .values()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        for requirement in lane_artifact_index_requirements() {
            assert!(
                artifact_values.contains(&requirement.relative_path),
                "expected manifest artifacts to cover {}",
                requirement.relative_path
            );
        }

        let schema: Value = serde_json::from_str(MOZA_R5_MANIFEST_SCHEMA_JSON)?;
        let schema_required = schema
            .pointer("/properties/artifacts/required")
            .and_then(Value::as_array)
            .ok_or("expected manifest schema artifacts.required")?;
        let mut schema_keys = schema_required
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        schema_keys.sort_unstable();

        let mut manifest_keys = artifacts.keys().map(String::as_str).collect::<Vec<_>>();
        manifest_keys.sort_unstable();

        assert_eq!(manifest_keys, schema_keys);
        Ok(())
    }

    #[test]
    fn artifact_checklist_covers_lane_artifacts_and_states() -> TestResult {
        let checklist_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/hardware/moza-r5-artifact-checklist.md");
        let checklist = fs::read_to_string(&checklist_path)?;

        for requirement in lane_artifact_index_requirements() {
            assert!(
                checklist.contains(requirement.relative_path),
                "expected {} to list {}",
                checklist_path.display(),
                requirement.relative_path
            );
        }

        for state in [
            "not_started",
            "passive_capture_ready",
            "zero_torque_ready",
            "real_hardware_smoke_ready",
        ] {
            assert!(
                checklist.contains(state),
                "expected {} to document manifest state {}",
                checklist_path.display(),
                state
            );
        }

        for command in [
            "wheelctl moza init-lane",
            "wheelctl moza verify-bundle",
            "wheelctl moza promote-manifest",
            "wheelctl moza audit-lane",
            "wheelctl moza simulator-ffb-smoke",
        ] {
            assert!(
                checklist.contains(command),
                "expected {} to document command {}",
                checklist_path.display(),
                command
            );
        }

        Ok(())
    }

    #[test]
    fn moza_validation_docs_link_checklist_and_remain_non_claiming() -> TestResult {
        let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root_readme_path = repo_root.join("README.md");
        let docs_readme_path = docs_root.join("README.md");
        let setup_path = docs_root.join("SETUP.md");
        let user_guide_path = docs_root.join("USER_GUIDE.md");
        let protocols_readme_path = docs_root.join("protocols/README.md");
        let device_support_path = docs_root.join("DEVICE_SUPPORT.md");
        let device_capabilities_path = docs_root.join("DEVICE_CAPABILITIES.md");
        let validation_path = docs_root.join("hardware/moza-r5-validation.md");
        let matrix_path = docs_root.join("hardware/moza-validation-matrix.md");
        let checklist_path = docs_root.join("hardware/moza-r5-artifact-checklist.md");
        let ci_readme_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md");

        let root_readme = fs::read_to_string(&root_readme_path)?;
        let docs_readme = fs::read_to_string(&docs_readme_path)?;
        let setup = fs::read_to_string(&setup_path)?;
        let user_guide = fs::read_to_string(&user_guide_path)?;
        let protocols_readme = fs::read_to_string(&protocols_readme_path)?;
        let device_support = fs::read_to_string(&device_support_path)?;
        let device_capabilities = fs::read_to_string(&device_capabilities_path)?;
        let validation = fs::read_to_string(&validation_path)?;
        let matrix = fs::read_to_string(&matrix_path)?;
        let checklist = fs::read_to_string(&checklist_path)?;
        let ci_readme = fs::read_to_string(&ci_readme_path)?;

        assert!(docs_readme.contains("hardware/moza-r5-artifact-checklist.md"));
        assert!(validation.contains("moza-r5-artifact-checklist.md"));
        assert!(matrix.contains("moza-r5-artifact-checklist.md"));
        assert!(ci_readme.contains("docs/hardware/moza-r5-artifact-checklist.md"));

        assert!(matrix.contains("| `moza-r5-windows-usb` | R5 + KS/ES + SR-P + HBP | Windows | HID only | Not started | No | No | No | No |"));
        for non_claim in [
            "Moza R5 compatibility on Steven's hardware",
            "Pit House coexistence safety",
            "Direct/high-torque readiness",
            "Release readiness",
        ] {
            assert!(
                matrix.contains(non_claim),
                "expected matrix non-claim '{non_claim}'"
            );
        }

        assert!(checklist.contains("It does not contain a dated real-hardware lane"));
        assert!(
            checklist.contains("release_ready` and `high_torque_validated` must remain `false`")
        );

        for (path, text) in [
            (root_readme_path.as_path(), root_readme.as_str()),
            (setup_path.as_path(), setup.as_str()),
            (user_guide_path.as_path(), user_guide.as_str()),
            (protocols_readme_path.as_path(), protocols_readme.as_str()),
            (device_support_path.as_path(), device_support.as_str()),
            (
                device_capabilities_path.as_path(),
                device_capabilities.as_str(),
            ),
        ] {
            assert!(
                text.contains("receipt") || text.contains("Source-backed"),
                "{} must keep Moza real-hardware claims receipt-gated",
                path.display()
            );
        }

        assert!(root_readme.contains("Source-backed; receipt-gated HID output"));
        assert!(setup.contains("Source-backed; hardware receipts required"));
        assert!(user_guide.contains("protocol-known, real-hardware output requires lane receipts"));
        assert!(protocols_readme.contains("Source-backed / receipt-gated"));
        assert!(device_support.contains("Status: **Source-backed / receipt-gated**"));
        assert!(device_capabilities.contains("status: **Source-backed / receipt-gated**"));

        for stale_claim in [
            "| **Moza Racing** | `0x346E` | R3, R5 V1/V2, R9 V1/V2, R12 V1/V2, R16, R21 | ✅ Serial/HID PIDFF |",
            "| **Moza Racing** | `0x346E` | R3, R5 V1/V2, R9 V1/V2, R12 V1/V2, R16, R21 | ✅ Serial / HID PIDFF |",
            "| [Moza](MOZA_PROTOCOL.md) | ✅ Supported | Serial/HID PIDFF |",
            "### 4. Moza Racing — VID `0x346E` · Status: **Verified**",
            "| R5 V1 | `0x0004` | Verified | universal-pidff |",
            "| R5 V2 | `0x0014` | Verified | universal-pidff |",
            "Source: `crates/hid-moza-protocol`; status: **Verified**",
            "| Moza Racing (R3–R21 V1/V2) | Verified |",
        ] {
            for (path, text) in [
                (root_readme_path.as_path(), root_readme.as_str()),
                (setup_path.as_path(), setup.as_str()),
                (user_guide_path.as_path(), user_guide.as_str()),
                (protocols_readme_path.as_path(), protocols_readme.as_str()),
                (device_support_path.as_path(), device_support.as_str()),
                (
                    device_capabilities_path.as_path(),
                    device_capabilities.as_str(),
                ),
            ] {
                assert!(
                    !text.contains(stale_claim),
                    "{} still contains stale Moza claim: {stale_claim}",
                    path.display()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn moza_operator_docs_wheelctl_commands_parse() -> TestResult {
        let docs = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware/moza-r5-validation.md"),
        ];
        let mut checked = 0usize;

        for path in docs {
            let text = fs::read_to_string(&path)?;
            for (line_index, line) in text.lines().enumerate() {
                let command = line.trim();
                if !command.starts_with("wheelctl ") {
                    continue;
                }

                let command = command_with_test_placeholders(command);
                let args = split_generated_command(&command)?;
                crate::Cli::try_parse_from(args).map_err(|error| {
                    format!(
                        "{}:{} documented wheelctl command failed to parse: {command}\n{error}",
                        path.display(),
                        line_index + 1
                    )
                })?;
                checked += 1;
            }
        }

        assert!(
            checked >= 70,
            "expected to parse the documented Moza operator transcript, checked {checked}"
        );
        Ok(())
    }

    #[test]
    fn moza_operator_docs_place_canonical_wheeld_before_simulator_ffb_smoke() -> TestResult {
        let docs = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware/moza-r5-validation.md"),
        ];

        for path in docs {
            let text = fs::read_to_string(&path)?;
            let lines = text.lines().map(str::trim).collect::<Vec<_>>();
            let ffb_index = lines
                .iter()
                .position(|line| line.starts_with("wheelctl moza simulator-ffb-smoke"))
                .ok_or_else(|| {
                    format!("{} is missing simulator FFB smoke command", path.display())
                })?;
            let previous_command = lines
                .iter()
                .take(ffb_index)
                .rev()
                .find(|line| {
                    line.starts_with("wheelctl ")
                        || line.starts_with("wheeld ")
                        || line.starts_with("hid-capture ")
                })
                .copied();

            let previous_command = previous_command.ok_or_else(|| {
                format!(
                    "{} is missing the wheeld simulator writer command before simulator-ffb-smoke",
                    path.display()
                )
            })?;
            assert!(
                previous_command.starts_with("wheeld --hardware-lane ci/hardware/moza-r5/"),
                "{} must place a dated-lane wheeld command immediately before simulator-ffb-smoke so output-log writer provenance matches the verifier, got {previous_command}",
                path.display()
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn init_lane_writes_contract_manifest_without_hid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("2026-05-06");

        init_lane(true, &lane, "0x0014", "Steven", false).await?;

        let manifest_path = lane.join("manifest.json");
        let manifest = read_json_path(&manifest_path)?;
        assert!(lane.join("captures").is_dir());
        assert!(manifest_contract_is_moza_r5_lane(&manifest));
        assert_eq!(
            json_string(&manifest, "completion_state"),
            Some("not_started")
        );
        assert_eq!(json_bool(&manifest, "hardware_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "simulator_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "release_ready"), Some(false));
        assert_eq!(
            manifest
                .get("hardware")
                .and_then(|hardware| json_string(hardware, "wheelbase_pid")),
            Some("0x0014")
        );
        Ok(())
    }

    #[tokio::test]
    async fn init_lane_rejects_non_r5_pid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let result = init_lane(true, dir.path(), "0x0003", "Steven", false).await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn init_lane_refuses_to_replace_manifest_without_overwrite() -> TestResult {
        let dir = tempfile::tempdir()?;
        init_lane(true, dir.path(), "0x0014", "Steven", false).await?;

        let result = init_lane(true, dir.path(), "0x0014", "Steven", false).await;

        assert!(result.is_err());
        Ok(())
    }

    fn fenced_block_after(
        document: &str,
        heading: &str,
        info: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let heading_index = document
            .find(heading)
            .ok_or_else(|| format!("missing heading {heading}"))?;
        let after_heading = document
            .get(heading_index..)
            .ok_or("heading index was not a valid string boundary")?;
        let fence = format!("```{info}");
        let fence_index = after_heading
            .find(&fence)
            .ok_or_else(|| format!("missing {info} fence after {heading}"))?;
        let after_fence = after_heading
            .get(fence_index + fence.len()..)
            .ok_or("fence index was not a valid string boundary")?;
        let content_start = after_fence
            .strip_prefix("\r\n")
            .or_else(|| after_fence.strip_prefix("\n"))
            .ok_or("expected newline after fenced block opener")?;
        let end_index = content_start
            .find("\n```")
            .ok_or("missing fenced block terminator")?;
        let block = content_start
            .get(..end_index)
            .ok_or("fence terminator index was not a valid string boundary")?;
        Ok(block.to_string())
    }

    #[test]
    fn zero_torque_payload_is_motor_disabled() {
        let payload = zero_torque_payload_for_pid(product_ids::R5_V2);
        assert_eq!(payload, [0x20, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn short_hid_write_error_requires_full_report_length() {
        assert!(short_hid_write_error(REPORT_LEN).is_none());
        assert!(short_hid_write_error(REPORT_LEN - 1).is_some());
        assert!(short_hid_write_error(REPORT_LEN + 1).is_some());
    }

    #[test]
    fn partial_zero_command_record_keeps_bytes_written() -> TestResult {
        let payload = zero_torque_payload_for_pid(product_ids::R5_V2);
        let record = ZeroTorqueCommandRecord::partial(
            0,
            "scheduled_zero",
            Instant::now(),
            payload,
            REPORT_LEN - 1,
            "short_hid_write: expected 8 bytes, wrote 7".to_string(),
        );
        let value = serde_json::to_value(record)?;

        assert_eq!(json_string(&value, "result"), Some("partial"));
        assert_eq!(json_u64(&value, "bytes_written"), Some(7));
        assert!(value.get("error").and_then(Value::as_str).is_some());
        Ok(())
    }

    #[test]
    fn partial_low_torque_command_record_keeps_bytes_written() -> TestResult {
        let payload = low_torque_payload_for_pid_percent(product_ids::R5_V2, 1.0).0;
        let record = LowTorqueCommandRecord::partial(
            0,
            "low_torque",
            Instant::now(),
            1.0,
            payload,
            REPORT_LEN - 1,
            "short_hid_write: expected 8 bytes, wrote 7".to_string(),
        );
        let value = serde_json::to_value(record)?;

        assert_eq!(json_string(&value, "result"), Some("partial"));
        assert_eq!(json_u64(&value, "bytes_written"), Some(7));
        assert!(value.get("error").and_then(Value::as_str).is_some());
        Ok(())
    }

    #[test]
    fn zero_torque_receipt_proves_no_nonzero_payload() {
        let payload = zero_torque_payload_for_pid(product_ids::R5_V2);
        let receipt = ZeroTorqueProofReceipt::new(
            "wheelctl moza zero",
            None,
            100,
            1000,
            100,
            sample_device(),
            payload,
            false,
        );

        assert_eq!(receipt.report_id, "0x20");
        assert_eq!(receipt.torque_raw, 0);
        assert_eq!(receipt.flags, 0);
        assert!(!receipt.motor_enabled);
        assert_eq!(receipt.non_zero_payloads, 0);
        assert!(receipt.no_high_torque);
        assert!(receipt.no_feature_reports);
        assert!(!receipt.dry_run);
        assert!(!receipt.no_ffb_writes);
        assert!(receipt.command_log.is_empty());
    }

    #[test]
    fn zero_torque_dry_run_receipt_proves_no_hid_open() -> TestResult {
        let pid = zero_torque_dry_run_pid(Some("0x346E:0x0014"), None)?;
        let payload = zero_torque_payload_for_pid(pid);
        let receipt = ZeroTorqueProofReceipt::new(
            "wheelctl moza zero",
            Some("0x346E:0x0014".to_string()),
            100,
            1000,
            100,
            synthetic_moza_device_record(pid),
            payload,
            true,
        );

        assert!(receipt.dry_run);
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_ffb_writes);
        assert_eq!(receipt.device.product_id, "0x0014");
        assert_eq!(receipt.non_zero_payloads, 0);
        Ok(())
    }

    #[test]
    fn torque_test_args_bound_low_torque() {
        assert!(validate_torque_test_args(2.0, 250, 1000).is_ok());
        assert!(validate_torque_test_args(2.1, 250, 1000).is_err());
        assert!(validate_torque_test_args(0.0, 250, 1000).is_err());
        assert!(validate_torque_test_args(2.0, 0, 1000).is_err());
        assert!(validate_torque_test_args(2.0, 250, 1001).is_err());
    }

    #[test]
    fn torque_test_ladder_clamps_to_max_percent() {
        let ladder = low_torque_ladder_for_pid(product_ids::R5_V2, 2.0, 250, 1000);

        assert_eq!(ladder.len(), 4);
        assert_eq!(ladder[0].percent, 0.1);
        assert_eq!(ladder[3].percent, 2.0);
        assert!(ladder.iter().all(|stage| stage.report_id == "0x20"));
        assert!(ladder.iter().all(|stage| stage.flags & 0x01 == 0x01));
        assert!(ladder.iter().all(|stage| stage.write_count == 250));
    }

    #[test]
    fn validate_zero_proof_for_torque_test_accepts_real_zero_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        write_test_json_file(&proof, &real_zero_receipt(100))?;

        let summary = validate_zero_proof_for_torque_test(&proof)?;

        assert_eq!(summary.product_id.as_deref(), Some("0x0014"));
        assert_eq!(summary.repeat, 100);
        assert!(summary.final_zero_sent);
        Ok(())
    }

    #[test]
    fn validate_zero_proof_for_torque_test_rejects_dry_run_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        write_test_json_file(
            &proof,
            &serde_json::json!({
                "success": true,
                "dry_run": true,
                "repeat": 100,
                "device": {"product_id": "0x0014"},
                "command_log": zero_command_log(100)
            }),
        )?;

        let result = validate_zero_proof_for_torque_test(&proof);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_zero_proof_for_torque_test_requires_out_of_scope_assertions() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        let mut receipt = real_zero_receipt(100);
        if let Some(map) = receipt.as_object_mut() {
            map.remove("no_firmware_or_dfu_commands");
        }
        write_test_json_file(&proof, &receipt)?;

        let result = validate_zero_proof_for_torque_test(&proof);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_zero_proof_for_torque_test_requires_exact_write_accounting() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        let mut receipt = real_zero_receipt(100);
        receipt["writes_ok"] = serde_json::json!(100);
        write_test_json_file(&proof, &receipt)?;

        let result = validate_zero_proof_for_torque_test(&proof);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_zero_proof_for_torque_test_requires_final_zero_last() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        let mut receipt = real_zero_receipt(100);
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let final_zero = command_log.pop().ok_or("expected final zero")?;
        command_log.insert(0, final_zero);
        for (index, record) in command_log.iter_mut().enumerate() {
            record["sequence"] = serde_json::json!(index);
        }
        write_test_json_file(&proof, &receipt)?;

        let result = validate_zero_proof_for_torque_test(&proof);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_zero_proof_for_torque_test_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        let proof = dir.path().join("zero.json");
        let mut receipt = real_zero_receipt(100);
        receipt["device"]["output_capable"] = serde_json::json!(false);
        write_test_json_file(&proof, &receipt)?;

        let result = validate_zero_proof_for_torque_test(&proof);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_accepts_trusted_descriptor_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [sample_trusted_r5_json_device()]
            }),
        )?;

        let gate = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false)?;

        assert!(gate.satisfied);
        assert!(gate.descriptor_trusted);
        assert!(!gate.explicit_operator_override);
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_accepts_live_r5_v1_extended_descriptor() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [sample_trusted_r5_v1_json_device()]
            }),
        )?;

        let gate = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0004", false)?;

        assert!(gate.satisfied);
        assert!(gate.descriptor_trusted);
        assert!(!gate.explicit_operator_override);
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_r5_v2_descriptor_with_v1_input_shape() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        let mut device = sample_trusted_r5_v1_json_device();
        device["product_id"] = serde_json::json!("0x0014");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [device]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_parsed_metadata_without_descriptor_hex() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        let mut device = sample_trusted_r5_json_device();
        if let Some(object) = device.as_object_mut() {
            object.remove("report_descriptor_hex");
        }
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [device]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_descriptor_crc_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        let mut device = sample_trusted_r5_json_device();
        device["report_descriptor_crc32"] = serde_json::json!("0x00000000");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [device]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_report_id_mismatch_from_descriptor_hex() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        let mut device = sample_trusted_r5_json_device();
        device["feature_report_ids"] = serde_json::json!(["0x02", "0x03", "0x11"]);
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [device]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_direct_output_report_length_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        let parsed_descriptor = report_descriptor_from_operator_hex(
            "85 01 75 08 95 06 81 02 85 02 75 08 95 1E 81 02 85 20 75 08 95 06 91 02 85 03 75 08 95 03 B1 02 85 11 75 08 95 03 B1 02",
        )?;
        let mut device = sample_device();
        device.apply_report_descriptor(parsed_descriptor, "operator_supplied_hex");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [device]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_protocol_expected_descriptor_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        let descriptor = dir.path().join("descriptor.json");
        write_test_json_file(
            &descriptor,
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [sample_r5_json_device()]
            }),
        )?;

        let result = validate_direct_mode_gate_for_torque_test(Some(&descriptor), "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn moza_status_descriptor_trust_is_scoped_to_descriptor_derived_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("descriptor.json"),
            &serde_json::json!({
                "success": true,
                "devices": [sample_r5_json_device()]
            }),
        )?;

        let receipt = moza_status_receipt(vec![sample_device()], Some("0x0014"), Some(dir.path()));
        let first = receipt
            .get("devices")
            .and_then(Value::as_array)
            .and_then(|devices| devices.first())
            .ok_or("expected device status")?;

        assert_eq!(json_bool(first, "descriptor_trusted"), Some(false));
        Ok(())
    }

    #[test]
    fn validate_direct_mode_gate_rejects_missing_descriptor_without_override() -> TestResult {
        let result = validate_direct_mode_gate_for_torque_test(None, "0x0014", false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_updates_device_descriptor_metadata() -> TestResult {
        let descriptor = report_descriptor_from_operator_hex("05 01 09 04")?;
        let mut device = sample_device();

        device.apply_report_descriptor(descriptor, "operator_supplied_hex");

        assert_eq!(device.descriptor_source, "operator_supplied_hex");
        assert_eq!(device.report_descriptor_len, Some(4));
        assert!(
            device
                .report_descriptor_crc32
                .as_deref()
                .map(|crc| crc.starts_with("0x"))
                .unwrap_or(false)
        );
        assert_eq!(device.report_descriptor_hex.as_deref(), Some("05010904"));
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_extracts_usbtreeview_style_bytes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-report-descriptor.txt");
        write_text_file(
            &path,
            "Report Descriptor:\n\
             0000: 05 01 09 04 A1 01 // Usage Page, Usage, Collection\n\
             0006: 85 01, 09 30\n",
        )?;

        let hex = read_report_descriptor_hex_file(&path)?;

        assert_eq!(hex, "05010904A10185010930");
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_ignores_usbtreeview_device_summary_fields() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-usbtreeview-device-summary.txt");
        write_text_file(
            &path,
            "Device ID: USB\\VID_346E&PID_0004\\6&00000000&0&2\n\
             bcdUSB: 2.00\n\
             Interface Descriptor:\n\
             bInterfaceNumber: 02\n\
             HID Descriptor:\n\
             bNumDescriptors: 01\n",
        )?;

        let result = read_report_descriptor_hex_file(&path);

        let message = match result {
            Ok(bytes) => format!("device summary parsed as descriptor bytes: {bytes}"),
            Err(err) => err.to_string(),
        };
        assert!(message.contains("no HID report descriptor bytes found"));
        assert!(message.contains("Report Descriptor byte block"));
        assert!(message.contains("0000: 05 01 09 04"));
        assert!(message.contains("USBTreeView device/interface summary"));
        assert!(message.contains("wDescriptorLength"));
        assert!(message.contains("ERROR_INVALID_PARAMETER"));
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_rejects_usbtreeview_report_read_error_without_summary_bytes()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-usbtreeview-report-with-read-error.txt");
        write_text_file(
            &path,
            "Data (HexDump)           : 03 00 00 00 12 01 00 02   ........\n\
                                      00 01 01 02 03 01 01 01   ........\n\
             bcdADC                   : 0x0100\n\
             Interface Descriptor:\n\
             bInterfaceNumber         : 0x02\n\
             HID Descriptor:\n\
             Descriptor 1:\n\
             bDescriptorType          : 0x22 (Class=Report)\n\
             wDescriptorLength        : 0x0523 (1315 bytes)\n\
             Error reading descriptor : ERROR_INVALID_PARAMETER\n\
             String Descriptor 1:\n\
             Data (HexDump)           : 10 03 47 00 75 00 64 00   ..G.u.d.\n\
                                      73 00 65 00 6E 00 00 00   s.e.n...\n",
        )?;

        let result = read_report_descriptor_hex_file(&path);

        let message = match result {
            Ok(bytes) => format!("USBTreeView summary parsed as descriptor bytes: {bytes}"),
            Err(err) => err.to_string(),
        };
        assert!(
            message.contains("no HID report descriptor bytes found"),
            "{message}"
        );
        assert!(
            message.contains("ERROR_INVALID_PARAMETER descriptor-read failure"),
            "{message}"
        );
        assert!(
            !message.contains("invalid descriptor byte line"),
            "{message}"
        );
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_reports_no_bytes_for_non_utf8_usbtreeview_summary() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-usbtreeview-report-with-ansi-byte.txt");
        fs::write(
            &path,
            b"HID Descriptor:\n\
              bDescriptorType          : 0x22 (Class=Report)\n\
              wDescriptorLength        : 0x0523 (1315 bytes)\n\
              Error reading descriptor : ERROR_INVALID_PARAMETER\n\
              Language 0x0409          : \"MOZA R5 Base\x90\"\n",
        )?;

        let result = read_report_descriptor_hex_file(&path);

        let message = match result {
            Ok(bytes) => format!("USBTreeView summary parsed as descriptor bytes: {bytes}"),
            Err(err) => err.to_string(),
        };
        assert!(
            message.contains("no HID report descriptor bytes found"),
            "{message}"
        );
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_accepts_descriptor_block_inside_summary() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir
            .path()
            .join("r5-usbtreeview-summary-with-report-descriptor.txt");
        write_text_file(
            &path,
            "Device ID: USB\\VID_346E&PID_0004\\6&00000000&0&2\n\
             bcdUSB: 2.00\n\
             Report Descriptor:\n\
             0000: 05 01 09 04 A1 01\n\
             0006: 85 20 75 08 95 06 91 02\n\
             Interface Descriptor:\n\
             bInterfaceNumber: 02\n",
        )?;

        let hex = read_report_descriptor_hex_file(&path)?;

        assert_eq!(hex, "05010904A1018520750895069102");
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_accepts_usbtreeview_report_descriptor_hexdump() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir
            .path()
            .join("r5-usbtreeview-report-descriptor-hexdump.txt");
        write_text_file(
            &path,
            "Interface Descriptor:\n\
             bInterfaceNumber         : 0x02\n\
             ------------------- Report Descriptor --------------------\n\
             Data (HexDump)           : 05 01 09 04 A1 01 85 20   ....... \n\
                                      75 08 95 06 91 02 C0      u......\n\
             Endpoint Descriptor:\n\
             bEndpointAddress         : 0x83\n",
        )?;

        let hex = read_report_descriptor_hex_file(&path)?;

        assert_eq!(hex, "05010904A1018520750895069102C0");
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_accepts_compact_hex_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-report-descriptor-compact.txt");
        write_text_file(&path, "05010904A101")?;

        let hex = read_report_descriptor_hex_file(&path)?;

        assert_eq!(hex, "05010904A101");
        Ok(())
    }

    #[test]
    fn operator_descriptor_bin_file_accepts_raw_sysfs_bytes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report_descriptor");
        fs::write(&path, [0x05, 0x01, 0x09, 0x04, 0xA1, 0x01])?;

        let hex = read_report_descriptor_bin_file(&path)?;

        assert_eq!(hex, "05010904A101");
        Ok(())
    }

    #[test]
    fn operator_descriptor_bin_file_rejects_empty_raw_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report_descriptor");
        fs::write(&path, [])?;

        let result = read_report_descriptor_bin_file(&path);

        let message = match result {
            Ok(bytes) => format!("empty descriptor parsed as bytes: {bytes}"),
            Err(err) => err.to_string(),
        };
        assert!(
            message.contains("raw binary HID report_descriptor file"),
            "{message}"
        );
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_source_rejects_inline_and_file_together() -> TestResult {
        let path = Path::new("r5-report-descriptor.txt");
        let result = operator_report_descriptor_hex(Some("05 01"), Some(path), None);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_source_rejects_text_and_binary_files_together() -> TestResult {
        let text_path = Path::new("r5-report-descriptor.txt");
        let binary_path = Path::new("report_descriptor");
        let result = operator_report_descriptor_hex(None, Some(text_path), Some(binary_path));

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_file_updates_device_descriptor_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("r5-report-descriptor.txt");
        write_text_file(&path, "05 01 09 04")?;
        let hex = operator_report_descriptor_hex(None, Some(&path), None)?
            .ok_or("expected descriptor hex from file")?;
        let descriptor = report_descriptor_from_operator_hex(&hex)?;
        let mut device = sample_device();

        device.apply_report_descriptor(descriptor, "operator_supplied_hex");

        assert_eq!(device.descriptor_source, "operator_supplied_hex");
        assert_eq!(device.report_descriptor_len, Some(4));
        assert_eq!(device.report_descriptor_hex.as_deref(), Some("05010904"));
        Ok(())
    }

    #[test]
    fn operator_descriptor_bin_file_updates_device_descriptor_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report_descriptor");
        fs::write(&path, [0x05, 0x01, 0x09, 0x04])?;
        let hex = operator_report_descriptor_hex(None, None, Some(&path))?
            .ok_or("expected descriptor hex from binary file")?;
        let descriptor = report_descriptor_from_operator_hex(&hex)?;
        let mut device = sample_device();

        device.apply_report_descriptor(descriptor, "operator_supplied_hex");

        assert_eq!(device.descriptor_source, "operator_supplied_hex");
        assert_eq!(device.report_descriptor_len, Some(4));
        assert_eq!(device.report_descriptor_hex.as_deref(), Some("05010904"));
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_preserves_vendor_wide_descriptor_records() -> TestResult {
        let mut devices = vec![
            synthetic_moza_device_record(product_ids::R5_V2),
            synthetic_moza_device_record(product_ids::SR_P_PEDALS),
            synthetic_moza_device_record(product_ids::HBP_HANDBRAKE),
        ];

        apply_operator_report_descriptor_to_selected_device(
            &mut devices,
            Some("0x0014"),
            "05 01 09 04",
        )?;

        assert_eq!(devices.len(), 3);
        let r5 = devices
            .iter()
            .find(|device| device.product_id == "0x0014")
            .ok_or("expected R5 device record")?;
        assert_eq!(r5.descriptor_source, "operator_supplied_hex");
        assert_eq!(r5.report_descriptor_len, Some(4));
        assert_eq!(r5.report_descriptor_hex.as_deref(), Some("05010904"));

        let srp = devices
            .iter()
            .find(|device| device.product_id == "0x0003")
            .ok_or("expected SR-P device record")?;
        assert_eq!(srp.descriptor_source, "dry_run");
        assert_eq!(srp.report_descriptor_len, None);
        assert_eq!(srp.input_report_lengths, vec![5]);

        let hbp = devices
            .iter()
            .find(|device| device.product_id == "0x0022")
            .ok_or("expected HBP device record")?;
        assert_eq!(hbp.descriptor_source, "dry_run");
        assert_eq!(hbp.report_descriptor_len, None);
        assert_eq!(hbp.input_report_lengths, vec![2, 3, 4]);
        Ok(())
    }

    #[test]
    fn operator_descriptor_hex_rejects_ambiguous_vendor_wide_selection() {
        let mut devices = vec![
            synthetic_moza_device_record(product_ids::R5_V2),
            synthetic_moza_device_record(product_ids::SR_P_PEDALS),
        ];

        let result =
            apply_operator_report_descriptor_to_selected_device(&mut devices, None, "05 01 09 04");

        assert!(result.is_err());
        assert!(
            devices
                .iter()
                .all(|device| device.report_descriptor_len.is_none())
        );
    }

    #[test]
    fn parsed_descriptor_metadata_enables_descriptor_trust_source() -> TestResult {
        let descriptor = report_descriptor_from_operator_hex(
            "85 01 75 08 95 06 81 02 85 02 75 08 95 1E 81 02 85 20 75 08 95 07 91 02 85 03 75 08 95 03 B1 02 85 11 75 08 95 03 B1 02",
        )?;
        let mut device = sample_device();

        device.apply_report_descriptor(descriptor, "operator_supplied_hex");

        assert_eq!(
            device.report_metadata_source.as_str(),
            "report_descriptor_parsed"
        );
        assert_eq!(device.input_report_lengths, vec![7, 31]);
        assert_eq!(
            device.output_report_ids,
            vec![DIRECT_TORQUE_REPORT_ID.to_string()]
        );
        assert_eq!(
            device.output_reports,
            vec![HidReportRecord {
                report_id: "0x20".to_string(),
                report_len: REPORT_LEN,
            }]
        );
        assert_eq!(
            device.feature_report_ids,
            vec![
                START_REPORTING_FEATURE_REPORT_ID.to_string(),
                FFB_MODE_FEATURE_REPORT_ID.to_string(),
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn torque_test_dry_run_writes_plan_without_hid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("low-torque.json");

        torque_test(TorqueTestRequest {
            json: true,
            selector: Some("0x346E:0x0014"),
            pid_override: None,
            zero_proof: None,
            descriptor: None,
            lane: None,
            init_off: None,
            init_standard: None,
            dry_run: true,
            confirm_low_torque: false,
            explicit_operator_override: false,
            max_percent: 2.0,
            duration_ms: 250,
            hz: 1000,
            json_out: Some(&receipt_path),
        })
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(json_bool(&receipt, "dry_run"), Some(true));
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_bool(&receipt, "no_ffb_writes"), Some(true));
        assert_eq!(json_bool(&receipt, "no_high_torque"), Some(true));
        assert_eq!(json_f64(&receipt, "max_percent"), Some(2.0));
        assert!(
            receipt
                .get("command_log")
                .and_then(Value::as_array)
                .map(|records| !records.is_empty())
                .unwrap_or(false)
        );
        Ok(())
    }

    #[tokio::test]
    async fn torque_test_actual_requires_confirmation() -> TestResult {
        let result = torque_test(TorqueTestRequest {
            json: false,
            selector: Some("0x346E:0x0014"),
            pid_override: None,
            zero_proof: None,
            descriptor: None,
            lane: None,
            init_off: None,
            init_standard: None,
            dry_run: false,
            confirm_low_torque: false,
            explicit_operator_override: false,
            max_percent: 2.0,
            duration_ms: 250,
            hz: 1000,
            json_out: None,
        })
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn torque_test_actual_requires_direct_mode_gate_before_hid_open() -> TestResult {
        let dir = tempfile::tempdir()?;
        let zero_proof = dir.path().join("zero-torque-proof.json");
        write_low_torque_prerequisite_receipts(dir.path())?;

        let result = torque_test(TorqueTestRequest {
            json: false,
            selector: Some("0x346E:0x0014"),
            pid_override: None,
            zero_proof: Some(&zero_proof),
            descriptor: None,
            lane: Some(dir.path()),
            init_off: None,
            init_standard: None,
            dry_run: false,
            confirm_low_torque: true,
            explicit_operator_override: false,
            max_percent: 2.0,
            duration_ms: 250,
            hz: 1000,
            json_out: None,
        })
        .await;

        let message = result
            .err()
            .map(|e| e.to_string())
            .ok_or("expected direct mode gate error")?;
        assert!(message.contains("--descriptor"));
        assert!(message.contains("--explicit-operator-override"));
        Ok(())
    }

    #[tokio::test]
    async fn torque_test_actual_requires_init_proofs_before_hid_open() -> TestResult {
        let dir = tempfile::tempdir()?;
        let zero_proof = dir.path().join("zero-torque-proof.json");
        write_test_json_file(&zero_proof, &real_zero_receipt(100))?;

        let result = torque_test(TorqueTestRequest {
            json: false,
            selector: Some("0x346E:0x0014"),
            pid_override: None,
            zero_proof: Some(&zero_proof),
            descriptor: None,
            lane: Some(dir.path()),
            init_off: None,
            init_standard: None,
            dry_run: false,
            confirm_low_torque: true,
            explicit_operator_override: true,
            max_percent: 2.0,
            duration_ms: 250,
            hz: 1000,
            json_out: None,
        })
        .await;

        let message = result
            .err()
            .map(|e| e.to_string())
            .ok_or("expected init proof gate error")?;
        assert!(message.contains("init-off.json"));
        Ok(())
    }

    #[test]
    fn validate_init_proofs_for_torque_test_accepts_lane_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(&dir.path().join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &dir.path().join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;

        let proofs = validate_init_proofs_for_torque_test(Some(dir.path()), None, None, false)?
            .ok_or("missing init proofs")?;

        assert_eq!(proofs.off.mode, "off");
        assert_eq!(proofs.standard.mode, "standard");
        assert!(proofs.match_product_id("0x0014"));
        Ok(())
    }

    #[test]
    fn validate_init_proofs_for_torque_test_rejects_high_torque_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut off = real_init_receipt("off");
        off["high_torque"] = serde_json::json!(true);
        write_test_json_file(&dir.path().join("init-off.json"), &off)?;
        write_test_json_file(
            &dir.path().join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;

        let result = validate_init_proofs_for_torque_test(Some(dir.path()), None, None, false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn validate_low_torque_preflight_accepts_same_lane_prerequisites() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_trusted_descriptor_if_missing(dir.path())?;

        let preflight = validate_low_torque_real_hardware_preflight(
            Some("0x0014"),
            None,
            Some(dir.path()),
            Some(&dir.path().join("zero-torque-proof.json")),
            None,
            None,
            Some(&dir.path().join("descriptor.json")),
            false,
        )?;

        assert_eq!(preflight.target_product_id, "0x0014");
        assert_eq!(preflight.zero_proof.product_id.as_deref(), Some("0x0014"));
        assert!(preflight.init_proofs.match_product_id("0x0014"));
        assert!(preflight.direct_mode_gate.descriptor_trusted);
        Ok(())
    }

    #[test]
    fn validate_low_torque_preflight_rejects_off_lane_zero_proof() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_low_torque_prerequisite_receipts(stale.path())?;
        write_trusted_descriptor_if_missing(dir.path())?;

        let result = validate_low_torque_real_hardware_preflight(
            Some("0x0014"),
            None,
            Some(dir.path()),
            Some(&stale.path().join("zero-torque-proof.json")),
            None,
            None,
            Some(&dir.path().join("descriptor.json")),
            false,
        );

        let message = match result {
            Ok(_) => return Err("expected off-lane zero proof preflight error".into()),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("--zero-proof"));
        assert!(message.contains("same-lane"));
        Ok(())
    }

    #[test]
    fn validate_low_torque_preflight_rejects_off_lane_init_proof() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_low_torque_prerequisite_receipts(stale.path())?;
        write_trusted_descriptor_if_missing(dir.path())?;

        let result = validate_low_torque_real_hardware_preflight(
            Some("0x0014"),
            None,
            Some(dir.path()),
            Some(&dir.path().join("zero-torque-proof.json")),
            Some(&stale.path().join("init-off.json")),
            None,
            Some(&dir.path().join("descriptor.json")),
            false,
        );

        let message = match result {
            Ok(_) => return Err("expected off-lane init proof preflight error".into()),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("--init-off"));
        assert!(message.contains("same-lane"));
        Ok(())
    }

    #[test]
    fn validate_low_torque_preflight_rejects_off_lane_descriptor() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_trusted_descriptor_if_missing(stale.path())?;

        let result = validate_low_torque_real_hardware_preflight(
            Some("0x0014"),
            None,
            Some(dir.path()),
            Some(&dir.path().join("zero-torque-proof.json")),
            None,
            None,
            Some(&stale.path().join("descriptor.json")),
            false,
        );

        let message = match result {
            Ok(_) => return Err("expected off-lane descriptor preflight error".into()),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("--descriptor"));
        assert!(message.contains("same-lane"));
        Ok(())
    }

    #[tokio::test]
    async fn watchdog_proof_dry_run_writes_receipt_without_hid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("watchdog-proof.json");

        watchdog_proof(
            true,
            Some("0x346E:0x0014"),
            None,
            true,
            3,
            1000,
            100,
            Some(&receipt_path),
        )
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza watchdog-proof")
        );
        assert_eq!(json_bool(&receipt, "dry_run"), Some(true));
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_bool(&receipt, "watchdog_triggered"), Some(true));
        Ok(())
    }

    #[tokio::test]
    async fn disconnect_proof_actual_requires_confirmation() -> TestResult {
        let result = disconnect_proof(
            false,
            Some("0x346E:0x0014"),
            None,
            false,
            false,
            1000,
            1000,
            None,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn disconnect_proof_dry_run_writes_receipt_without_hid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("disconnect-proof.json");

        disconnect_proof(
            true,
            Some("0x346E:0x0014"),
            None,
            true,
            false,
            1000,
            1000,
            Some(&receipt_path),
        )
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza disconnect-proof")
        );
        assert_eq!(json_bool(&receipt, "dry_run"), Some(true));
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_bool(&receipt, "disconnect_observed"), Some(true));
        Ok(())
    }

    #[tokio::test]
    async fn init_dry_run_writes_off_handshake_plan_without_hid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("init-off.json");

        init(
            true,
            Some("0x346E:0x0014"),
            None,
            MozaInitMode::Off,
            true,
            Some(&receipt_path),
        )
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(json_bool(&receipt, "dry_run"), Some(true));
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_string(&receipt, "mode"), Some("off"));
        assert_eq!(json_string(&receipt, "mode_wire_value"), Some("0xFF"));
        assert_eq!(json_bool(&receipt, "no_high_torque"), Some(true));
        assert!(
            receipt
                .get("feature_reports")
                .map(|reports| init_feature_reports_are_safe_value(reports, "off", true))
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_accepts_logged_real_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_test_json_file(
            &dir.path().join("low-torque-proof.json"),
            &real_low_torque_receipt_for_lane(dir.path(), 2.0)?,
        )?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_requires_lane_descriptor_for_trusted_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_test_json_file(
            &dir.path().join("low-torque-proof.json"),
            &real_low_torque_receipt_for_lane(dir.path(), 2.0)?,
        )?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("descriptor_trust_observed=false"),
            "expected descriptor trust cross-check failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_dry_run_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        receipt["dry_run"] = serde_json::json!(true);
        receipt["no_hid_device_opened"] = serde_json::json!(true);
        receipt["no_ffb_writes"] = serde_json::json!(true);
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_requires_torque_test_command() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        receipt["command"] = serde_json::json!("wheelctl moza zero");
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("command_ok=false"),
            "expected command provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_stale_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        receipt["receipt_path"] = serde_json::json!(
            dir.path()
                .join("other/low-torque-proof.json")
                .display()
                .to_string()
        );
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("receipt_path_ok=false"),
            "expected receipt path failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_command_above_limit() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let first = command_log.first_mut().ok_or("expected first command")?;
        first["percent"] = serde_json::json!(2.5);
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_payload_that_exceeds_declared_percent() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let first = command_log.first_mut().ok_or("expected first command")?;
        let payload = low_torque_payload_for_pid_percent(product_ids::R5_V2, 2.0).0;
        first["payload_hex"] = serde_json::json!(bytes_hex_compact(&payload));
        first["torque_raw"] = serde_json::json!(i16::from_le_bytes([payload[1], payload[2]]));
        first["flags"] = serde_json::json!(payload[3]);
        first["motor_enabled"] = serde_json::json!(payload[3] & 0x01 != 0);
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("command_log_safe=false"),
            "expected raw payload mismatch failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_missing_direct_mode_gate() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        receipt["direct_mode_gate_satisfied"] = serde_json::json!(false);
        receipt["descriptor_trusted"] = serde_json::json!(false);
        receipt["explicit_operator_override"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_missing_init_proofs() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let mut receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        receipt["init_proofs_validated"] = serde_json::json!(false);
        receipt["init_proofs"] = Value::Null;
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_requires_same_lane_prerequisite_summaries() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        write_low_torque_prerequisite_receipts(stale.path())?;
        let receipt = real_low_torque_receipt_for_lane(stale.path(), 2.0)?;
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("zero_proof_lane_match=false"),
            "expected stale prerequisite lane failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_low_torque_gate_rejects_newer_lane_prerequisite() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_low_torque_prerequisite_receipts(dir.path())?;
        let receipt = real_low_torque_receipt_for_lane(dir.path(), 2.0)?;
        let mut zero = real_zero_receipt(100);
        zero["generated_at_utc"] = serde_json::json!("2026-05-06T00:00:02Z");
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &zero)?;
        write_test_json_file(&dir.path().join("low-torque-proof.json"), &receipt)?;

        let gate = verify_low_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("zero_proof_lane_match=false"),
            "expected newer prerequisite failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_accepts_off_and_standard_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(&dir.path().join("init-off.json"), &real_init_receipt("off"))?;
        write_test_json_file(
            &dir.path().join("init-standard.json"),
            &real_init_receipt("standard"),
        )?;

        let off_gate =
            verify_init_receipt_gate(dir.path(), "init_off_handshake", "init-off.json", "off");
        let standard_gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(off_gate.status, "pass");
        assert_eq!(standard_gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_rejects_high_torque_report() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_init_receipt("standard");
        receipt["feature_reports"] = serde_json::json!([
            {
                "sequence": 0,
                "kind": "high_torque",
                "payload_hex": "02000000",
                "report_id": "0x02",
                "result": "ok",
                "bytes_written": 4
            },
            {
                "sequence": 1,
                "kind": "ffb_mode",
                "payload_hex": "11000000",
                "report_id": "0x11",
                "result": "ok",
                "bytes_written": 4
            }
        ]);
        write_test_json_file(&dir.path().join("init-standard.json"), &receipt)?;

        let gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_requires_full_feature_report_writes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_init_receipt("standard");
        let reports = receipt
            .get_mut("feature_reports")
            .and_then(Value::as_array_mut)
            .ok_or("expected feature reports")?;
        let report = reports.get_mut(1).ok_or("expected FFB mode report")?;
        if let Some(object) = report.as_object_mut() {
            object.remove("bytes_written");
        }
        write_test_json_file(&dir.path().join("init-standard.json"), &receipt)?;

        let gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("feature_reports_safe=false"),
            "expected feature-report accounting failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_requires_ordered_feature_report_sequence() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_init_receipt("standard");
        let reports = receipt
            .get_mut("feature_reports")
            .and_then(Value::as_array_mut)
            .ok_or("expected feature reports")?;
        let report = reports.get_mut(0).ok_or("expected start report")?;
        report["sequence"] = serde_json::json!(1);
        write_test_json_file(&dir.path().join("init-standard.json"), &receipt)?;

        let gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("feature_reports_safe=false"),
            "expected feature-report sequence failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_init_receipt("standard");
        receipt["device"]["product_id"] = serde_json::json!("0x0008");
        write_test_json_file(&dir.path().join("init-standard.json"), &receipt)?;

        let gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_init_receipt_gate_rejects_stale_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_init_receipt("standard");
        receipt["receipt_path"] = serde_json::json!(
            dir.path()
                .join("other/init-standard.json")
                .display()
                .to_string()
        );
        write_test_json_file(&dir.path().join("init-standard.json"), &receipt)?;

        let gate = verify_init_receipt_gate(
            dir.path(),
            "init_standard_handshake",
            "init-standard.json",
            "standard",
        );

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("receipt_path_ok=false"),
            "expected receipt path failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_accepts_detailed_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &real_watchdog_receipt(3),
        )?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_rejects_placeholder_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &serde_json::json!({"success": true}),
        )?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_requires_exact_write_accounting() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_watchdog_receipt(3);
        receipt["writes_ok"] = serde_json::json!(3);
        write_test_json_file(&dir.path().join("watchdog-proof.json"), &receipt)?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_watchdog_receipt(3);
        receipt["device"]["output_capable"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("watchdog-proof.json"), &receipt)?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_rejects_stale_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_watchdog_receipt(3);
        receipt["receipt_path"] = serde_json::json!(
            dir.path()
                .join("other/watchdog-proof.json")
                .display()
                .to_string()
        );
        write_test_json_file(&dir.path().join("watchdog-proof.json"), &receipt)?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("receipt_path_ok=false"),
            "expected receipt path failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_zero_torque_gate_requires_no_out_of_scope_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_zero_receipt(100);
        if let Some(map) = receipt.as_object_mut() {
            map.remove("no_firmware_or_dfu_commands");
        }
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &receipt)?;

        let gate = verify_zero_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_zero_torque_gate_requires_zero_command() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_zero_receipt(100);
        receipt["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &receipt)?;

        let gate = verify_zero_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("command_ok=false"),
            "expected command provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_zero_torque_gate_rejects_stale_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_zero_receipt(100);
        receipt["receipt_path"] = serde_json::json!(
            dir.path()
                .join("other/zero-torque-proof.json")
                .display()
                .to_string()
        );
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &receipt)?;

        let gate = verify_zero_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("receipt_path_ok=false"),
            "expected receipt path failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_zero_torque_gate_requires_final_zero_last() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_zero_receipt(100);
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let final_zero = command_log.pop().ok_or("expected final zero")?;
        command_log.insert(0, final_zero);
        for (index, record) in command_log.iter_mut().enumerate() {
            record["sequence"] = serde_json::json!(index);
        }
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &receipt)?;

        let gate = verify_zero_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_zero_torque_gate_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_zero_receipt(100);
        receipt["device"]["product_id"] = serde_json::json!("0x0008");
        write_test_json_file(&dir.path().join("zero-torque-proof.json"), &receipt)?;

        let gate = verify_zero_torque_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_watchdog_proof_gate_requires_final_zero_last() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_watchdog_receipt(3);
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let final_zero = command_log.pop().ok_or("expected final zero")?;
        command_log.insert(0, final_zero);
        for (index, record) in command_log.iter_mut().enumerate() {
            record["sequence"] = serde_json::json!(index);
        }
        write_test_json_file(&dir.path().join("watchdog-proof.json"), &receipt)?;

        let gate = verify_watchdog_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_accepts_detailed_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_rejects_placeholder_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &serde_json::json!({"success": true}),
        )?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_disconnect_receipt();
        receipt["device"]["product_id"] = serde_json::json!("0x0008");
        write_test_json_file(&dir.path().join("disconnect-proof.json"), &receipt)?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_requires_matching_write_accounting() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_disconnect_receipt();
        receipt["writes_ok"] = serde_json::json!(3);
        write_test_json_file(&dir.path().join("disconnect-proof.json"), &receipt)?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_rejects_stale_receipt_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_disconnect_receipt();
        receipt["receipt_path"] = serde_json::json!(
            dir.path()
                .join("other/disconnect-proof.json")
                .display()
                .to_string()
        );
        write_test_json_file(&dir.path().join("disconnect-proof.json"), &receipt)?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("receipt_path_ok=false"),
            "expected receipt path failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_disconnect_proof_gate_rejects_partial_final_zero_record() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut receipt = real_disconnect_receipt();
        let command_log = receipt
            .get_mut("command_log")
            .and_then(Value::as_array_mut)
            .ok_or("expected command log")?;
        let final_zero = command_log.last_mut().ok_or("expected final zero")?;
        final_zero["result"] = serde_json::json!("partial");
        final_zero["bytes_written"] = serde_json::json!(7);
        write_test_json_file(&dir.path().join("disconnect-proof.json"), &receipt)?;

        let gate = verify_disconnect_proof_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_accepts_complete_matrix() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_placeholder_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &serde_json::json!({"success": true}),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_case_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut receipt = pit_house_receipt();
        let cases = receipt
            .get_mut("cases")
            .and_then(Value::as_array_mut)
            .ok_or("expected cases")?;
        let first = cases.first_mut().ok_or("expected first case")?;
        first["evidence"] = serde_json::json!("");
        write_test_json_file(&dir.path().join("pit-house-coexistence.json"), &receipt)?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_case_artifact_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt = pit_house_receipt();
        write_test_json_file(&dir.path().join("pit-house-coexistence.json"), &receipt)?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_placeholder_case_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        write_test_json_file(
            &dir.path().join("pit-house-closed.json"),
            &serde_json::json!({
                "artifact": "pit-house-closed.json",
                "evidence": "placeholder evidence"
            }),
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_case_artifact_result_match() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut artifact = read_json_path(&dir.path().join("pit-house-open-standard.json"))?;
        artifact["result"] = serde_json::json!("conflict_documented");
        write_test_json_file(&dir.path().join("pit-house-open-standard.json"), &artifact)?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_case_artifact_high_torque() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut artifact = read_json_path(&dir.path().join("pit-house-direct-blocked.json"))?;
        artifact["high_torque"] = serde_json::json!(true);
        write_test_json_file(&dir.path().join("pit-house-direct-blocked.json"), &artifact)?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_case_source_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut artifact = read_json_path(&dir.path().join("pit-house-open-standard.json"))?;
        if let Some(object) = artifact.as_object_mut() {
            object.remove("source_receipt");
        }
        write_test_json_file(&dir.path().join("pit-house-open-standard.json"), &artifact)?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_case_observation_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut artifact = read_json_path(&dir.path().join("pit-house-open-standard.json"))?;
        if let Some(object) = artifact.as_object_mut() {
            object.remove("pit_house_observation_artifact");
        }
        write_test_json_file(&dir.path().join("pit-house-open-standard.json"), &artifact)?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_stale_observation_state() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["pit_house_observed_state"] = serde_json::json!("closed");
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_stale_observation_case() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["case"] = serde_json::json!("pit_house_closed");
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_notes_only_observation() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["evidence_kind"] = serde_json::json!("operator_notes");
        if let Some(object) = observation.as_object_mut() {
            object.remove("evidence_artifact");
        }
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_observation_evidence_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["evidence_artifact"] = serde_json::json!("missing-window-snapshot.json");
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_observation_command_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_wrong_source_gate() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut artifact = read_json_path(&dir.path().join("pit-house-closed.json"))?;
        artifact["source_gate"] = serde_json::json!("init_standard_handshake");
        write_test_json_file(&dir.path().join("pit-house-closed.json"), &artifact)?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_rejects_stale_source_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        fs::remove_file(dir.path().join("low-torque-proof.json"))?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_mode_mismatch_clear_record() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |_, record| {
                if json_string(record, "clear_event") == Some("mode_mismatch") {
                    record["clear_event"] = serde_json::json!("extra_zero");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("pit-house-coexistence.json"),
            &pit_house_receipt(),
        )?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[tokio::test]
    async fn receipt_template_writes_verifier_rejected_pit_house_template() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("pit-house-coexistence.json");

        receipt_template(
            false,
            MozaReceiptTemplateKind::PitHouse,
            &receipt_path,
            false,
        )
        .await?;

        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(json_bool(&receipt, "success"), Some(false));
        assert_eq!(json_bool(&receipt, "template"), Some(true));
        assert_eq!(
            json_string(&receipt, "evidence_status"),
            Some("operator_pending")
        );

        let gate = verify_pit_house_coexistence_gate(dir.path());
        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_observation_writes_verifier_accepted_state_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let observation_path = dir.path().join("pit-house-observation-open-standard.json");
        write_text_file(
            &dir.path().join("pit-house-open-standard.png"),
            "screenshot",
        )?;

        pit_house_observation(PitHouseObservationRequest {
            json: false,
            case: MozaPitHouseObservationCase::OpenStandard,
            evidence_kind: MozaPitHouseEvidenceKind::OperatorScreenshot,
            evidence_artifact: Some(Path::new("pit-house-open-standard.png")),
            operator: "Steven",
            evidence: "Pit House open idle screenshot captured.",
            json_out: &observation_path,
            overwrite: false,
        })
        .await?;

        let receipt = read_json_path(&observation_path)?;
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza pit-house-observation")
        );
        assert_eq!(
            json_string(&receipt, "case"),
            Some("pit_house_open_idle_standard")
        );
        assert_eq!(
            json_string(&receipt, "pit_house_observed_state"),
            Some("open_idle_standard")
        );
        assert_eq!(
            json_string(&receipt, "evidence_kind"),
            Some("operator_screenshot")
        );
        assert_eq!(
            json_string(&receipt, "evidence_artifact"),
            Some("pit-house-open-standard.png")
        );
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(true));
        assert_eq!(json_bool(&receipt, "no_ffb_writes"), Some(true));

        let case_artifact = serde_json::json!({
            "pit_house_observation_artifact": "pit-house-observation-open-standard.json"
        });
        assert!(pit_house_case_observation_is_safe(
            dir.path(),
            &case_artifact,
            "pit_house_open_idle_standard"
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_observation_rejects_empty_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        let observation_path = dir.path().join("pit-house-observation-closed.json");
        let result = pit_house_observation(PitHouseObservationRequest {
            json: false,
            case: MozaPitHouseObservationCase::Closed,
            evidence_kind: MozaPitHouseEvidenceKind::OperatorNotes,
            evidence_artifact: Some(Path::new("pit-house-closed.txt")),
            operator: "Steven",
            evidence: " ",
            json_out: &observation_path,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!observation_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_observation_rejects_notes_only_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        let observation_path = dir.path().join("pit-house-observation-closed.json");
        let result = pit_house_observation(PitHouseObservationRequest {
            json: false,
            case: MozaPitHouseObservationCase::Closed,
            evidence_kind: MozaPitHouseEvidenceKind::OperatorNotes,
            evidence_artifact: Some(Path::new("pit-house-closed.txt")),
            operator: "Steven",
            evidence: "Pit House was closed.",
            json_out: &observation_path,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!observation_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_observation_requires_evidence_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        let observation_path = dir.path().join("pit-house-observation-closed.json");
        let result = pit_house_observation(PitHouseObservationRequest {
            json: false,
            case: MozaPitHouseObservationCase::Closed,
            evidence_kind: MozaPitHouseEvidenceKind::ProcessWindowSnapshot,
            evidence_artifact: None,
            operator: "Steven",
            evidence: "Pit House closed process snapshot recorded.",
            json_out: &observation_path,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!observation_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_observation_rejects_missing_evidence_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        let observation_path = dir.path().join("pit-house-observation-closed.json");
        let result = pit_house_observation(PitHouseObservationRequest {
            json: false,
            case: MozaPitHouseObservationCase::Closed,
            evidence_kind: MozaPitHouseEvidenceKind::ProcessWindowSnapshot,
            evidence_artifact: Some(Path::new("pit-house-evidence-closed.json")),
            operator: "Steven",
            evidence: "Pit House closed process snapshot recorded.",
            json_out: &observation_path,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!observation_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_writes_verifier_accepted_case_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let output = dir.path().join("generated-open-standard.json");
        let observation = dir.path().join("pit-house-observation-open-standard.json");

        pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: dir.path(),
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: &observation,
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await?;

        let artifact = read_json_path(&output)?;
        assert_eq!(
            json_string(&artifact, "case"),
            Some("pit_house_open_idle_standard")
        );
        assert_eq!(json_string(&artifact, "result"), Some("standard_ok"));
        assert_eq!(
            json_string(&artifact, "source_receipt"),
            Some("init-standard.json")
        );
        assert_eq!(
            json_string(&artifact, "pit_house_observation_artifact"),
            Some("pit-house-observation-open-standard.json")
        );
        assert!(pit_house_case_artifact_is_safe(
            dir.path(),
            "generated-open-standard.json",
            "pit_house_open_idle_standard",
            Some("standard_ok"),
            SupportBundleValidationMode::Fresh
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_accepts_lane_prefixed_relative_artifacts() -> TestResult {
        let (lane_root, lane) = temp_lane_under_cwd()?;
        write_pit_house_artifacts(lane_root.path())?;
        let observation = lane.join("pit-house-observation-open-standard.json");
        let output = lane.join("generated-lane-prefixed-open-standard.json");

        pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: &lane,
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: &observation,
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await?;

        let output_path = lane_root
            .path()
            .join("generated-lane-prefixed-open-standard.json");
        let artifact = read_json_path(&output_path)?;
        assert_eq!(
            json_string(&artifact, "pit_house_observation_artifact"),
            Some("pit-house-observation-open-standard.json")
        );
        assert!(pit_house_case_artifact_is_safe(
            &lane,
            "generated-lane-prefixed-open-standard.json",
            "pit_house_open_idle_standard",
            Some("standard_ok"),
            SupportBundleValidationMode::Fresh
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_accepts_absolute_artifacts_with_relative_lane() -> TestResult {
        let (lane_root, lane) = temp_lane_under_cwd()?;
        write_pit_house_artifacts(lane_root.path())?;
        let observation = lane_root
            .path()
            .join("pit-house-observation-open-standard.json");
        let output = lane_root
            .path()
            .join("generated-absolute-open-standard.json");

        pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: &lane,
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: &observation,
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await?;

        let artifact = read_json_path(&output)?;
        assert_eq!(
            json_string(&artifact, "pit_house_observation_artifact"),
            Some("pit-house-observation-open-standard.json")
        );
        assert!(pit_house_case_artifact_is_safe(
            &lane,
            "generated-absolute-open-standard.json",
            "pit_house_open_idle_standard",
            Some("standard_ok"),
            SupportBundleValidationMode::Fresh
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_simple_relative_output_writes_under_lane() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let output = Path::new("generated-simple-relative-open-standard.json");

        pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: dir.path(),
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: Path::new("pit-house-observation-open-standard.json"),
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: output,
            overwrite: false,
        })
        .await?;

        let output_path = dir
            .path()
            .join("generated-simple-relative-open-standard.json");
        assert!(output_path.is_file());
        assert!(pit_house_case_artifact_is_safe(
            dir.path(),
            "generated-simple-relative-open-standard.json",
            "pit_house_open_idle_standard",
            Some("standard_ok"),
            SupportBundleValidationMode::Fresh
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_strips_cwd_relative_artifact_under_absolute_lane() -> TestResult {
        let (lane_root, lane) = temp_lane_under_cwd()?;
        write_pit_house_artifacts(lane_root.path())?;
        let observation = lane.join("pit-house-observation-open-standard.json");
        let output = lane.join("generated-absolute-lane-open-standard.json");

        pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: lane_root.path(),
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: &observation,
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await?;

        let output_path = lane_root
            .path()
            .join("generated-absolute-lane-open-standard.json");
        let artifact = read_json_path(&output_path)?;
        assert_eq!(
            json_string(&artifact, "pit_house_observation_artifact"),
            Some("pit-house-observation-open-standard.json")
        );
        assert!(pit_house_case_artifact_is_safe(
            lane_root.path(),
            "generated-absolute-lane-open-standard.json",
            "pit_house_open_idle_standard",
            Some("standard_ok"),
            SupportBundleValidationMode::Fresh
        ));
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_rejects_stale_observation_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let output = dir.path().join("generated-open-standard.json");
        let mut observation =
            read_json_path(&dir.path().join("pit-house-observation-open-standard.json"))?;
        observation["pit_house_observed_state"] = serde_json::json!("closed");
        write_test_json_file(
            &dir.path().join("pit-house-observation-open-standard.json"),
            &observation,
        )?;

        let result = pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: dir.path(),
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: Path::new("pit-house-observation-open-standard.json"),
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_case_rejects_missing_source_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        fs::remove_file(dir.path().join("init-standard.json"))?;
        let output = dir.path().join("generated-open-standard.json");

        let result = pit_house_case(PitHouseCaseRequest {
            json: false,
            lane: dir.path(),
            case: MozaPitHouseObservationCase::OpenStandard,
            observation_artifact: Path::new("pit-house-observation-open-standard.json"),
            evidence: "Pit House open idle state linked to standard init proof.",
            json_out: &output,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn receipt_template_refuses_overwrite_without_flag() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("pit-house-coexistence.json");

        receipt_template(
            false,
            MozaReceiptTemplateKind::PitHouse,
            &receipt_path,
            false,
        )
        .await?;
        let result = receipt_template(
            false,
            MozaReceiptTemplateKind::PitHouse,
            &receipt_path,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn pit_house_proof_writes_verifier_accepted_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let closed = dir.path().join("pit-house-closed.json");
        let open_standard = dir.path().join("pit-house-open-standard.json");
        let direct = dir.path().join("pit-house-direct-blocked.json");
        let mode_change = dir.path().join("pit-house-mode-change.json");
        let firmware_page = dir.path().join("pit-house-firmware-page.json");

        pit_house_proof(PitHouseProofRequest {
            json: false,
            lane: dir.path(),
            closed_artifact: &closed,
            open_standard_artifact: &open_standard,
            direct_artifact: &direct,
            mode_change_artifact: &mode_change,
            firmware_page_artifact: &firmware_page,
            shared_control_risk: "warned",
            json_out: None,
            overwrite: false,
        })
        .await?;

        let receipt = read_json_path(&dir.path().join("pit-house-coexistence.json"))?;
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza pit-house-proof")
        );
        assert_eq!(
            json_string(&receipt, "evidence_status"),
            Some("observed_on_real_hardware")
        );
        let gate = verify_pit_house_coexistence_gate(dir.path());
        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_pit_house_gate_requires_proof_command() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_pit_house_artifacts(dir.path())?;
        let mut receipt = pit_house_receipt();
        receipt["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(&dir.path().join("pit-house-coexistence.json"), &receipt)?;

        let gate = verify_pit_house_coexistence_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(gate.details.contains("command_ok=false"));
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_accepts_telemetry_only_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_rejects_placeholder_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &serde_json::json!({"success": true}),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_requires_recorder_artifact_records() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_repeated_jsonl(&dir.path().join("simulator-telemetry-recording.jsonl"), 119)?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_rejects_sequence_only_recorder_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_repeated_jsonl(&dir.path().join("simulator-telemetry-recording.jsonl"), 120)?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_requires_duration_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut lines = String::new();
        for sequence in 0..120 {
            let mut record = simulator_telemetry_snapshot(sequence);
            record["recording_duration_ms"] = serde_json::json!(1000);
            lines.push_str(&serde_json::to_string(&record)?);
            lines.push('\n');
        }
        write_text_file(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            &lines,
        )?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("recorder_artifact_valid=false"),
            "expected duration evidence failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_accepts_canonical_recorder_json() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-recording.json"),
            &simulator_telemetry_recording_json(120),
        )?;
        let mut receipt = simulator_telemetry_receipt();
        receipt["recorder_artifact"] = serde_json::json!("simulator-telemetry-recording.json");
        write_test_json_file(&dir.path().join("simulator-telemetry-proof.json"), &receipt)?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_rejects_absolute_artifact_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let outside_artifact = outside.path().join("simulator-telemetry-recording.jsonl");
        write_simulator_telemetry_jsonl(&outside_artifact, 120)?;
        let mut receipt = simulator_telemetry_receipt();
        receipt["recorder_artifact"] = serde_json::json!(outside_artifact.display().to_string());
        write_test_json_file(&dir.path().join("simulator-telemetry-proof.json"), &receipt)?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("recorder_artifact_valid=false"),
            "expected lane-relative artifact failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_rejects_out_of_range_snapshot() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut lines = String::new();
        for sequence in 0..120 {
            let mut record = simulator_telemetry_snapshot(sequence);
            if sequence == 8 {
                record["throttle"] = serde_json::json!(1.5);
            }
            lines.push_str(&serde_json::to_string(&record)?);
            lines.push('\n');
        }
        write_text_file(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            &lines,
        )?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_requires_recorder_artifact_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut lines = String::new();
        for sequence in 0..120 {
            let mut record = simulator_telemetry_snapshot(sequence);
            if sequence == 8 {
                record
                    .as_object_mut()
                    .ok_or("expected telemetry snapshot object")?
                    .remove("recorder_session_id");
            }
            lines.push_str(&serde_json::to_string(&record)?);
            lines.push('\n');
        }
        write_text_file(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            &lines,
        )?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &simulator_telemetry_receipt(),
        )?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("recorder_provenance_valid=false"),
            "expected recorder provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_requires_matching_recorder_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_telemetry_jsonl(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            120,
        )?;
        let mut receipt = simulator_telemetry_receipt();
        receipt["game"] = serde_json::json!("iracing");
        write_test_json_file(&dir.path().join("simulator-telemetry-proof.json"), &receipt)?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("recorder_provenance_valid=false"),
            "expected receipt/artifact provenance mismatch, got {}",
            gate.details
        );
        assert!(
            gate.details.contains("recorder_artifact_valid=true"),
            "expected artifact to remain structurally valid, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_rejects_json_object_without_records() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("simulator-telemetry-recording.json"),
            &serde_json::json!({
                "metadata": "not a recorder record array"
            }),
        )?;
        let mut receipt = simulator_telemetry_receipt();
        {
            let object = receipt
                .as_object_mut()
                .ok_or("expected simulator telemetry receipt object")?;
            object.insert(
                "normalized_snapshot_count".to_string(),
                serde_json::json!(1),
            );
            object.insert(
                "recorder_artifact".to_string(),
                serde_json::json!("simulator-telemetry-recording.json"),
            );
        }
        write_test_json_file(&dir.path().join("simulator-telemetry-proof.json"), &receipt)?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[tokio::test]
    async fn simulator_telemetry_proof_writes_verifier_accepted_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_telemetry_jsonl(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            120,
        )?;

        simulator_telemetry_proof(
            false,
            dir.path(),
            "simhub-bridge",
            "simhub_bridge",
            Path::new("simulator-telemetry-recording.jsonl"),
            5000,
            None,
            false,
        )
        .await?;

        let receipt = read_json_path(&dir.path().join("simulator-telemetry-proof.json"))?;
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza simulator-telemetry-proof")
        );
        assert_eq!(json_bool(&receipt, "hardware_output_enabled"), Some(false));
        let gate = verify_simulator_telemetry_gate(dir.path());
        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[tokio::test]
    async fn simulator_telemetry_proof_rejects_artifact_provenance_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_telemetry_jsonl(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            120,
        )?;

        let result = simulator_telemetry_proof(
            false,
            dir.path(),
            "iracing",
            "simhub_bridge",
            Path::new("simulator-telemetry-recording.jsonl"),
            5000,
            None,
            false,
        )
        .await;

        assert!(result.is_err());
        let receipt = read_json_path(&dir.path().join("simulator-telemetry-proof.json"))?;
        assert_eq!(json_bool(&receipt, "success"), Some(false));
        assert_eq!(
            json_string(&receipt, "recorder_session_id"),
            Some("sim-telemetry-session-001")
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_telemetry_gate_requires_proof_command() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_telemetry_receipt();
        receipt["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(&dir.path().join("simulator-telemetry-proof.json"), &receipt)?;

        let gate = verify_simulator_telemetry_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(gate.details.contains("command_ok=false"));
        Ok(())
    }

    #[tokio::test]
    async fn receipt_template_writes_verifier_rejected_simulator_templates() -> TestResult {
        let dir = tempfile::tempdir()?;
        let telemetry_path = dir.path().join("simulator-telemetry-proof.json");
        let ffb_path = dir.path().join("simulator-ffb-smoke.json");

        receipt_template(
            false,
            MozaReceiptTemplateKind::SimulatorTelemetry,
            &telemetry_path,
            false,
        )
        .await?;
        receipt_template(
            false,
            MozaReceiptTemplateKind::SimulatorFfb,
            &ffb_path,
            false,
        )
        .await?;

        let telemetry = read_json_path(&telemetry_path)?;
        let ffb = read_json_path(&ffb_path)?;
        assert_eq!(json_bool(&telemetry, "success"), Some(false));
        assert_eq!(json_bool(&ffb, "success"), Some(false));
        assert_eq!(json_bool(&telemetry, "no_ffb_writes"), Some(true));
        assert_eq!(
            json_string(&telemetry, "recorder_command"),
            Some(SIMULATOR_TELEMETRY_RECORDER_COMMAND)
        );
        assert_eq!(json_string(&telemetry, "recorder_session_id"), Some(""));
        assert_eq!(json_bool(&ffb, "high_torque"), Some(false));
        assert_eq!(
            json_bool(&ffb, "hardware_prerequisites_validated"),
            Some(false)
        );
        assert_eq!(
            json_string(&ffb, "writer_command"),
            Some(SIMULATOR_FFB_WRITER_COMMAND)
        );
        assert_eq!(json_string(&ffb, "writer_started_at_utc"), Some(""));
        assert_eq!(
            json_string(&ffb, "input_telemetry_recorder_session_id"),
            Some("")
        );

        let Some(prerequisite_gates) = ffb.get("prerequisite_gates").and_then(Value::as_array)
        else {
            return Err("simulator FFB template missing prerequisite_gates".into());
        };
        assert_eq!(
            prerequisite_gates.len(),
            SIMULATOR_FFB_PREREQUISITE_ARTIFACTS.len()
        );
        for ((gate_name, _), gate) in SIMULATOR_FFB_PREREQUISITE_ARTIFACTS
            .iter()
            .zip(prerequisite_gates)
        {
            assert_eq!(json_string(gate, "name"), Some(*gate_name));
            assert_eq!(json_string(gate, "status"), Some("operator_pending"));
        }

        let Some(prerequisite_artifacts) =
            ffb.get("prerequisite_artifacts").and_then(Value::as_array)
        else {
            return Err("simulator FFB template missing prerequisite_artifacts".into());
        };
        assert_eq!(
            prerequisite_artifacts.len(),
            SIMULATOR_FFB_PREREQUISITE_ARTIFACTS.len()
        );
        for ((gate_name, path), artifact) in SIMULATOR_FFB_PREREQUISITE_ARTIFACTS
            .iter()
            .zip(prerequisite_artifacts)
        {
            assert_eq!(json_string(artifact, "gate"), Some(*gate_name));
            assert_eq!(json_string(artifact, "path"), Some(*path));
            assert_eq!(json_string(artifact, "generated_at_utc"), Some(""));
            assert_eq!(json_string(artifact, "receipt_crc32"), Some(""));
        }

        let telemetry_gate = verify_simulator_telemetry_gate(dir.path());
        let ffb_gate = verify_simulator_ffb_gate(dir.path());
        assert_eq!(telemetry_gate.status, "fail");
        assert_eq!(ffb_gate.status, "fail");
        Ok(())
    }

    #[tokio::test]
    async fn simulator_ffb_smoke_writes_verifier_accepted_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;

        simulator_ffb_smoke(SimulatorFfbSmokeRequest {
            json: false,
            lane: dir.path(),
            game: "simhub-bridge",
            telemetry_source: "simhub_bridge",
            output_log_artifact: Path::new("simulator-ffb-output.jsonl"),
            descriptor_trusted: true,
            explicit_operator_override: false,
            watchdog_timeout_ms: 100,
            stop_cleared_output: true,
            pause_cleared_output: true,
            game_exit_cleared_output: true,
            json_out: None,
            overwrite: false,
        })
        .await?;

        let receipt = read_json_path(&dir.path().join("simulator-ffb-smoke.json"))?;
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza simulator-ffb-smoke")
        );
        assert_eq!(json_string(&receipt, "ffb_mode"), Some("direct"));
        assert_eq!(
            json_string(&receipt, "input_telemetry_artifact"),
            Some("simulator-telemetry-recording.jsonl")
        );
        assert_eq!(
            json_string(&receipt, "writer_command"),
            Some(SIMULATOR_FFB_WRITER_COMMAND)
        );
        assert!(path_value_matches(
            dir.path(),
            json_string(&receipt, "writer_hardware_lane")
        ));
        assert_eq!(
            json_bool(&receipt, "hardware_prerequisites_validated"),
            Some(true)
        );
        assert_eq!(json_bool(&receipt, "no_hid_device_opened"), Some(false));
        assert_eq!(json_bool(&receipt, "no_ffb_writes"), Some(false));
        assert_eq!(
            json_bool(&receipt, "mode_mismatch_cleared_output"),
            Some(true)
        );
        let gate = verify_simulator_ffb_gate(dir.path());
        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[tokio::test]
    async fn simulator_ffb_smoke_rejects_missing_hardware_prerequisites() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_trusted_descriptor_if_missing(dir.path())?;
        write_simulator_telemetry_jsonl(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            120,
        )?;
        simulator_telemetry_proof(
            false,
            dir.path(),
            "simhub-bridge",
            "simhub_bridge",
            Path::new("simulator-telemetry-recording.jsonl"),
            5000,
            None,
            false,
        )
        .await?;
        write_simulator_ffb_output_jsonl(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
        )?;

        let result = simulator_ffb_smoke(SimulatorFfbSmokeRequest {
            json: false,
            lane: dir.path(),
            game: "simhub-bridge",
            telemetry_source: "simhub_bridge",
            output_log_artifact: Path::new("simulator-ffb-output.jsonl"),
            descriptor_trusted: true,
            explicit_operator_override: false,
            watchdog_timeout_ms: 100,
            stop_cleared_output: true,
            pause_cleared_output: true,
            game_exit_cleared_output: true,
            json_out: None,
            overwrite: false,
        })
        .await;

        assert!(result.is_err());
        let receipt = read_json_path(&dir.path().join("simulator-ffb-smoke.json"))?;
        assert_eq!(
            json_bool(&receipt, "hardware_prerequisites_validated"),
            Some(false)
        );
        let gate = verify_simulator_ffb_gate(dir.path());
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("prerequisite_gates_valid=false"),
            "expected prerequisite gate failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_smoke_command() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(gate.details.contains("command_ok=false"));
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_clear_output_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        if let Some(map) = receipt.as_object_mut() {
            map.remove("mode_mismatch_cleared_output");
        }
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("mode_mismatch_cleared_output=None"),
            "expected missing mode-mismatch receipt failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_output_writer_lane_to_match() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |_, record| {
                record["writer_hardware_lane"] =
                    serde_json::json!(stale.path().display().to_string());
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_provenance_valid=false"),
            "expected stale writer lane provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_clear_event_records() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 180
                    && let Some(object) = record.as_object_mut()
                {
                    object.remove("clear_event");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected clear-event artifact failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_legacy_event_clear_records() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 180
                    && let Some(object) = record.as_object_mut()
                {
                    object.remove("clear_event");
                    object.insert("event".to_string(), serde_json::json!("stop"));
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected clear-zero records to require clear_event, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_mode_mismatch_clear_record() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 183 {
                    record["clear_event"] = serde_json::json!("extra_zero");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected mode-mismatch clear-event artifact failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_legacy_event_mode_mismatch_clear() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 183
                    && let Some(object) = record.as_object_mut()
                {
                    object.remove("clear_event");
                    object.insert("event".to_string(), serde_json::json!("mode_mismatch"));
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected mode-mismatch to require clear_event, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_ordered_clear_events() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 180 {
                    record["clear_event"] = serde_json::json!("pause");
                } else if sequence == 181 {
                    record["clear_event"] = serde_json::json!("stop");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected clear-event ordering failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_hardware_output_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["hardware_output_enabled"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_virtual_output_log_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |_, record| {
                if let Some(object) = record.as_object_mut() {
                    object.insert(
                        "writer_command".to_string(),
                        serde_json::json!("wheelctl telemetry virtual-ffb-log"),
                    );
                    object.insert(
                        "producer_command".to_string(),
                        serde_json::json!("wheelctl telemetry virtual-ffb-log"),
                    );
                    object.insert("hardware_source".to_string(), serde_json::json!("virtual"));
                    object.insert(
                        "real_hardware_validated".to_string(),
                        serde_json::json!(false),
                    );
                    object.insert(
                        "real_simulator_validated".to_string(),
                        serde_json::json!(false),
                    );
                    object.insert(
                        "hardware_output_enabled".to_string(),
                        serde_json::json!(false),
                    );
                    object.insert("no_hid_device_opened".to_string(), serde_json::json!(true));
                    object.insert("no_ffb_writes".to_string(), serde_json::json!(true));
                    object.insert(
                        "virtual_output_enabled".to_string(),
                        serde_json::json!(true),
                    );
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_provenance_valid=false")
                || gate.details.contains("output_log_artifact_valid=false"),
            "expected virtual output evidence rejection, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_writer_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 0
                    && let Some(object) = record.as_object_mut()
                {
                    object.remove("writer_session_id");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_provenance_valid=false"),
            "expected writer provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_writer_timing_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 0
                    && let Some(object) = record.as_object_mut()
                {
                    object.remove("writer_started_at_utc");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_provenance_valid=false"),
            "expected writer timing provenance failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_receipt_to_match_writer_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["writer_session_id"] = serde_json::json!("stale-session");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_provenance_valid=false"),
            "expected writer session mismatch failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_hardware_prerequisites() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        fs::remove_file(dir.path().join("zero-torque-proof.json"))?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("prerequisite_gates_valid=false"),
            "expected prerequisite gate mismatch failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_receipt_prerequisite_attestation() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["hardware_prerequisites_validated"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details
                .contains("hardware_prerequisites_validated=Some(false)"),
            "expected prerequisite attestation failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_matching_prerequisite_gate_names() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        if let Some(gates) = receipt
            .get_mut("prerequisite_gates")
            .and_then(Value::as_array_mut)
            && let Some(first_gate) = gates.first_mut()
        {
            first_gate["name"] = serde_json::json!("stale_zero_gate");
        }
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("prerequisite_gates_valid=false"),
            "expected prerequisite gate name mismatch, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_prerequisite_artifact_attestation() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["prerequisite_artifacts"] = serde_json::json!([]);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("prerequisite_artifacts_valid=false"),
            "expected prerequisite artifact attestation failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_newer_prerequisite_artifact() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;
        let mut watchdog = real_watchdog_receipt(3);
        watchdog["generated_at_utc"] = serde_json::json!("2026-05-06T00:00:04Z");
        write_test_json_file(&dir.path().join("watchdog-proof.json"), &watchdog)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("prerequisite_artifacts_valid=false"),
            "expected stale/newer prerequisite artifact failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_standard_mode_direct_torque_log() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["ffb_mode"] = serde_json::json!("standard");
        receipt["descriptor_trusted"] = serde_json::json!(false);
        receipt["explicit_operator_override"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(gate.details.contains("direct_mode_allowed=false"));
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_lane_descriptor_for_trusted_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        fs::remove_file(dir.path().join("descriptor.json"))?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("descriptor_trust_observed=false"),
            "expected descriptor trust cross-check failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_allows_explicit_operator_override_without_descriptor() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        fs::remove_file(dir.path().join("descriptor.json"))?;
        let mut low_torque_receipt = read_json_path(&dir.path().join("low-torque-proof.json"))?;
        low_torque_receipt["descriptor_trusted"] = serde_json::json!(false);
        low_torque_receipt["explicit_operator_override"] = serde_json::json!(true);
        low_torque_receipt["direct_mode_gate_reason"] =
            serde_json::json!("explicit_operator_override");
        write_test_json_file(
            &dir.path().join("low-torque-proof.json"),
            &low_torque_receipt,
        )?;
        let mut receipt = simulator_ffb_receipt();
        receipt["descriptor_trusted"] = serde_json::json!(false);
        receipt["explicit_operator_override"] = serde_json::json!(true);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_telemetry_link() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["input_telemetry_artifact"] = serde_json::json!("stale-telemetry.jsonl");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details
                .contains("linked_telemetry_snapshot_count=None"),
            "expected telemetry link failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_accepted_telemetry_proof() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut telemetry_receipt = simulator_telemetry_receipt();
        telemetry_receipt["command"] = serde_json::json!("wheelctl moza receipt-template");
        write_test_json_file(
            &dir.path().join("simulator-telemetry-proof.json"),
            &telemetry_receipt,
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details
                .contains("simulator_telemetry_gate_valid=false"),
            "expected accepted telemetry proof dependency failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_receipt_telemetry_session_link() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt
            .as_object_mut()
            .ok_or("expected simulator FFB receipt object")?
            .remove("input_telemetry_recorder_session_id");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details
                .contains("linked_telemetry_snapshot_count=None"),
            "expected missing telemetry session link failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_output_records_to_reference_telemetry() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 4 {
                    record["telemetry_sequence"] = serde_json::json!(999);
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected output-log telemetry reference failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_output_input_scalar_to_match_telemetry() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut lines = String::new();
        for sequence in 0..120 {
            let mut record = simulator_telemetry_snapshot(sequence);
            if sequence == 4 {
                record["ffb_scalar"] = serde_json::json!(-0.4);
            }
            lines.push_str(&serde_json::to_string(&record)?);
            lines.push('\n');
        }
        write_text_file(
            &dir.path().join("simulator-telemetry-recording.jsonl"),
            &lines,
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected output-log telemetry scalar mismatch failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_output_records_to_reference_telemetry_session()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 4 {
                    record["input_telemetry_recorder_session_id"] =
                        serde_json::json!("stale-telemetry-session");
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected output-log telemetry-session reference failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_hid_write_metadata_per_record() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 4 {
                    record["hid_write_attempted"] = serde_json::json!(false);
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected per-record HID write metadata failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_monotonic_output_elapsed_us() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 5 {
                    record["elapsed_us"] = serde_json::json!(0);
                }
            },
        )?;
        write_test_json_file(
            &dir.path().join("simulator-ffb-smoke.json"),
            &simulator_ffb_receipt(),
        )?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("output_log_artifact_valid=false"),
            "expected elapsed_us monotonicity failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_nonzero_bounded_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["nonzero_output_count"] = serde_json::json!(0);
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_r5_output_device() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["device"]["vendor_id"] = serde_json::json!("0x1234");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_safe_final_zero_payload() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_artifacts(dir.path())?;
        let mut receipt = simulator_ffb_receipt();
        receipt["final_zero_payload_hex"] = serde_json::json!("2001000000000000");
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_output_log_artifact_records() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_repeated_jsonl(&dir.path().join("simulator-ffb-output.jsonl"), 239)?;
        let receipt = simulator_ffb_receipt();
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_sequence_only_output_log() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_repeated_jsonl(&dir.path().join("simulator-ffb-output.jsonl"), 240)?;
        let receipt = simulator_ffb_receipt();
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_rejects_output_above_bound() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().to_path_buf();
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 0 {
                    *record = simulator_ffb_output_record(sequence, "sim_output", 6.0, &lane);
                }
            },
        )?;
        let receipt = simulator_ffb_receipt();
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_final_zero_last() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_ffb_output_jsonl_mutated(
            &dir.path().join("simulator-ffb-output.jsonl"),
            240,
            180,
            60,
            |sequence, record| {
                if sequence == 239 {
                    record["kind"] = serde_json::json!("clear_zero");
                }
            },
        )?;
        let receipt = simulator_ffb_receipt();
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_simulator_ffb_gate_requires_artifact_counts_to_match_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_simulator_ffb_output_jsonl(
            &dir.path().join("simulator-ffb-output.jsonl"),
            241,
            180,
            61,
        )?;
        let receipt = simulator_ffb_receipt();
        write_test_json_file(&dir.path().join("simulator-ffb-smoke.json"), &receipt)?;

        let gate = verify_simulator_ffb_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_manifest_gate_requires_lane_contract() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("manifest.json"),
            &serde_json::json!({
                "completion_state": "passive_capture_ready",
                "hardware_validated": false,
                "simulator_validated": false,
                "high_torque_validated": false,
                "release_ready": false
            }),
        )?;

        let gate = verify_manifest_gate(dir.path(), MozaBundleStage::Passive);

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_manifest_gate_rejects_schema_extra_fields() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut manifest = sample_lane_manifest("passive_capture_ready", false, false);
        manifest
            .as_object_mut()
            .ok_or("expected manifest object")?
            .insert(
                "unsupported_claim".to_string(),
                serde_json::json!("should fail additionalProperties"),
            );
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;

        let gate = verify_manifest_gate(dir.path(), MozaBundleStage::Passive);

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("schema_ok=false"),
            "expected schema validation failure message, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_manifest_gate_accepts_non_claiming_manifest_for_smoke_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("manifest.json"),
            &sample_lane_manifest("zero_torque_ready", false, false),
        )?;

        let gate = verify_manifest_gate(dir.path(), MozaBundleStage::SmokeReady);

        assert_eq!(gate.status, "pass");
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_rejects_manifest_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest["hardware"]["wheelbase_pid"] = serde_json::json!("0x0004");
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "manifest_r5_pid_consistency")
            .ok_or("expected manifest PID gate")?;
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("0x0004") && gate.details.contains("0x0014"),
            "expected manifest/receipt PID mismatch details, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_pid_gate_does_not_fail_on_missing_artifacts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        fs::remove_file(dir.path().join("captures/es-controls.jsonl"))?;
        fs::remove_file(dir.path().join("fixture-promotion.json"))?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "manifest_r5_pid_consistency")
            .ok_or("expected manifest PID gate")?;
        assert_eq!(gate.status, "pass");
        assert!(
            gate.details.contains("matches available lane R5 receipts")
                && gate.details.contains("unavailable PID evidence")
                && gate.details.contains("captures/es-controls.jsonl")
                && gate.details.contains("fixture:r5_idle"),
            "expected missing PID evidence to be reported without failing the PID gate, got {}",
            gate.details
        );
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| gate.name == "passive_captures_parse" && gate.status == "fail"),
            "missing ES capture should still fail the passive capture gate"
        );
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| gate.name == "fixture_promotion" && gate.status == "fail"),
            "missing fixture promotion should still fail the fixture gate"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_rejects_promoted_fixture_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let fixture_path = dir.path().join("fixtures/r5_idle.json");
        let mut fixture = read_json_path(&fixture_path)?;
        fixture["reports"][0]["product_id"] = serde_json::json!("0x0004");
        write_test_json_file(&fixture_path, &fixture)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let fixture_gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "fixture_promotion")
            .ok_or("expected fixture promotion gate")?;
        assert_eq!(fixture_gate.status, "fail");
        assert!(
            fixture_gate.details.contains("report_product_ids_ok=false")
                && fixture_gate.details.contains("0x0004")
                && fixture_gate.details.contains("0x0014"),
            "expected promoted fixture PID mismatch details, got {}",
            fixture_gate.details
        );
        let manifest_gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "manifest_r5_pid_consistency")
            .ok_or("expected manifest PID gate")?;
        assert_eq!(manifest_gate.status, "fail");
        assert!(
            manifest_gate.details.contains("fixture:r5_idle")
                && manifest_gate.details.contains("0x0004")
                && manifest_gate.details.contains("0x0014"),
            "expected promoted fixture PID mismatch details, got {}",
            manifest_gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_rejects_promoted_standalone_hbp_fixture_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        declare_standalone_control_topology(
            dir.path(),
            "handbrake",
            "handbrake",
            "moza-hbp-standalone",
            "standalone_handbrake",
            product_ids::HBP_HANDBRAKE,
            "captures/hbp-standalone-sweep.jsonl",
        )?;
        write_text_file(
            &dir.path().join("captures/hbp-standalone-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(product_ids::HBP_HANDBRAKE, "01000000"),
                capture_line(product_ids::HBP_HANDBRAKE, "01FFFF01")
            ),
        )?;
        refresh_passive_parser_receipts(dir.path())?;
        let fixture_path = dir.path().join("fixtures/hbp_standalone_sweep.json");
        let mut fixture = read_json_path(&fixture_path)?;
        fixture["product_ids"] = serde_json::json!({"0x0014": 1});
        fixture["reports"][0]["product_id"] = serde_json::json!("0x0014");
        write_test_json_file(&fixture_path, &fixture)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "fixture_promotion")
            .ok_or("expected fixture promotion gate")?;
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("captures/hbp-standalone-sweep.jsonl")
                && gate.details.contains("PID 0x0014")
                && gate.details.contains("parser rejected"),
            "expected standalone HBP fixture parser/PID mismatch details, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_complete_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(receipt.success);
        assert_eq!(receipt.missing_artifacts, 0);
        assert_eq!(receipt.invalid_artifacts, 0);
        assert_eq!(receipt.failed_gates, 0);
        assert!(receipt.next_commands.is_empty());
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_r5_in_each_enumeration_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("hid-list.json"),
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": []
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "moza_r5_observed" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_r5_in_device_list_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("device-list.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl device list",
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [{
                    "id": "name-only-moza-r5",
                    "name": "Moza R5",
                    "device_type": "WheelBase",
                    "state": "Connected"
                }]
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "moza_r5_observed" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_hub_topology_without_standalone_enumeration() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let r5_device = sample_trusted_r5_json_device();
        for artifact in [
            "device-list.json",
            "moza-probe.json",
            "hid-list.json",
            "descriptor.json",
        ] {
            let mut receipt = read_json_path(&dir.path().join(artifact))?;
            receipt["devices"] = serde_json::json!([r5_device.clone()]);
            write_test_json_file(&dir.path().join(artifact), &receipt)?;
        }

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "moza_topology_observed" && gate.status == "pass" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_uses_declared_topology_without_placeholder_artifacts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest
            .pointer_mut("/topology/logical_controls")
            .and_then(Value::as_object_mut)
            .ok_or("expected topology logical_controls object")?
            .remove("clutch");
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;
        fs::remove_file(dir.path().join("captures/r5-clutch-only-sweep.jsonl"))?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            receipt
                .artifacts
                .iter()
                .all(|artifact| artifact.path != "captures/r5-clutch-only-sweep.jsonl")
        );
        Ok(())
    }

    struct TopologyMatrixCase {
        name: &'static str,
        rims: &'static [&'static str],
        pedals: &'static [&'static str],
        handbrake: Option<&'static str>,
        controls: &'static [(
            &'static str,
            &'static str,
            Option<&'static str>,
            &'static str,
        )],
        removed_captures: &'static [&'static str],
        absent_artifacts: &'static [&'static str],
    }

    #[test]
    fn verify_bundle_passive_accepts_declared_topology_matrix_without_fixed_kit_leakage()
    -> TestResult {
        let cases = [
            TopologyMatrixCase {
                name: "r5_ks_only",
                rims: &["KS"],
                pedals: &[],
                handbrake: None,
                controls: &[
                    (
                        "steering",
                        "steering",
                        None,
                        "captures/r5-steering-sweep.jsonl",
                    ),
                    (
                        "ks_rim_controls",
                        "rim_controls",
                        Some("KS"),
                        "captures/ks-controls.jsonl",
                    ),
                ],
                removed_captures: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/es-controls.jsonl",
                ],
                absent_artifacts: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/es-controls.jsonl",
                    "captures/srp-standalone-sweep.jsonl",
                    "captures/hbp-standalone-sweep.jsonl",
                ],
            },
            TopologyMatrixCase {
                name: "r5_es_only",
                rims: &["ES"],
                pedals: &[],
                handbrake: None,
                controls: &[
                    (
                        "steering",
                        "steering",
                        None,
                        "captures/r5-steering-sweep.jsonl",
                    ),
                    (
                        "es_rim_controls",
                        "rim_controls",
                        Some("ES"),
                        "captures/es-controls.jsonl",
                    ),
                ],
                removed_captures: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                ],
                absent_artifacts: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                    "captures/srp-standalone-sweep.jsonl",
                    "captures/hbp-standalone-sweep.jsonl",
                ],
            },
            TopologyMatrixCase {
                name: "r5_throttle_brake_no_clutch_or_handbrake",
                rims: &[],
                pedals: &["SR-P"],
                handbrake: None,
                controls: &[
                    (
                        "steering",
                        "steering",
                        None,
                        "captures/r5-steering-sweep.jsonl",
                    ),
                    (
                        "throttle",
                        "throttle",
                        None,
                        "captures/r5-throttle-only-sweep.jsonl",
                    ),
                    ("brake", "brake", None, "captures/r5-brake-only-sweep.jsonl"),
                ],
                removed_captures: &[
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                    "captures/es-controls.jsonl",
                ],
                absent_artifacts: &[
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/r5-handbrake-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                    "captures/es-controls.jsonl",
                    "captures/srp-standalone-sweep.jsonl",
                    "captures/hbp-standalone-sweep.jsonl",
                ],
            },
            TopologyMatrixCase {
                name: "r5_hbp_through_hub",
                rims: &[],
                pedals: &[],
                handbrake: Some("HBP"),
                controls: &[
                    (
                        "steering",
                        "steering",
                        None,
                        "captures/r5-steering-sweep.jsonl",
                    ),
                    (
                        "handbrake",
                        "handbrake",
                        None,
                        "captures/r5-handbrake-only-sweep.jsonl",
                    ),
                ],
                removed_captures: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                    "captures/es-controls.jsonl",
                ],
                absent_artifacts: &[
                    "captures/r5-throttle-only-sweep.jsonl",
                    "captures/r5-brake-only-sweep.jsonl",
                    "captures/r5-clutch-only-sweep.jsonl",
                    "captures/ks-controls.jsonl",
                    "captures/es-controls.jsonl",
                    "captures/srp-standalone-sweep.jsonl",
                    "captures/hbp-standalone-sweep.jsonl",
                ],
            },
        ];

        for case in cases {
            let dir = tempfile::tempdir()?;
            write_minimal_passive_bundle(dir.path())?;
            replace_observation_receipt_devices(dir.path(), &[sample_trusted_r5_json_device()])?;
            set_declared_hardware(dir.path(), case.rims, case.pedals, case.handbrake)?;
            set_required_hub_controls(dir.path(), case.controls)?;
            remove_optional_capture_files(dir.path(), case.removed_captures)?;
            refresh_passive_parser_receipts(dir.path())?;

            let manifest = read_json_path(&dir.path().join("manifest.json"))?;
            let schema_errors = manifest_schema_validation_errors(&manifest);
            assert!(
                schema_errors.is_empty(),
                "{} schema errors: {schema_errors:?}",
                case.name
            );

            let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

            assert!(
                receipt.success,
                "{} failed: {}",
                case.name,
                serde_json::to_string_pretty(&receipt)?
            );
            let selected_requirements = passive_capture_requirements_for_lane(dir.path());
            for artifact in case.absent_artifacts {
                assert!(
                    receipt
                        .artifacts
                        .iter()
                        .all(|receipt_artifact| receipt_artifact.path != *artifact),
                    "{} unexpectedly required artifact {}",
                    case.name,
                    artifact
                );
                assert!(
                    selected_requirements
                        .iter()
                        .all(|requirement| requirement.relative_path != *artifact),
                    "{} unexpectedly selected capture {}",
                    case.name,
                    artifact
                );
            }
        }
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_allows_optional_absent_roles_without_placeholder_artifacts()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        replace_observation_receipt_devices(dir.path(), &[sample_trusted_r5_json_device()])?;
        set_declared_hardware(dir.path(), &[], &["SR-P"], None)?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "throttle",
                    "throttle",
                    None,
                    "captures/r5-throttle-only-sweep.jsonl",
                ),
                ("brake", "brake", None, "captures/r5-brake-only-sweep.jsonl"),
            ],
        )?;
        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest["topology"]["logical_controls"]["clutch"] = serde_json::json!({
            "role": "clutch",
            "source_endpoint": "moza-r5-if2",
            "connection": "wheelbase_hub",
            "required": false,
            "evidence_capture": "captures/r5-clutch-only-sweep.jsonl",
            "semantic_status": "deferred"
        });
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            passive_capture_requirements_for_lane(dir.path())
                .iter()
                .all(|requirement| requirement.relative_path
                    != "captures/r5-clutch-only-sweep.jsonl")
        );
        assert!(
            receipt
                .artifacts
                .iter()
                .all(|artifact| artifact.path != "captures/r5-clutch-only-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_allows_optional_absent_handbrake_without_placeholder_artifacts()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        replace_observation_receipt_devices(dir.path(), &[sample_trusted_r5_json_device()])?;
        set_declared_hardware(dir.path(), &[], &["SR-P"], None)?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "throttle",
                    "throttle",
                    None,
                    "captures/r5-throttle-only-sweep.jsonl",
                ),
                ("brake", "brake", None, "captures/r5-brake-only-sweep.jsonl"),
            ],
        )?;
        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest["topology"]["logical_controls"]["handbrake"] = serde_json::json!({
            "role": "handbrake",
            "source_endpoint": "moza-r5-if2",
            "connection": "wheelbase_hub",
            "required": false,
            "evidence_capture": "captures/r5-handbrake-only-sweep.jsonl",
            "semantic_status": "deferred"
        });
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            passive_capture_requirements_for_lane(dir.path())
                .iter()
                .all(|requirement| requirement.relative_path
                    != "captures/r5-handbrake-only-sweep.jsonl"
                    && requirement.relative_path != "captures/hbp-standalone-sweep.jsonl")
        );
        assert!(receipt.artifacts.iter().all(|artifact| artifact.path
            != "captures/r5-handbrake-only-sweep.jsonl"
            && artifact.path != "captures/hbp-standalone-sweep.jsonl"));
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_r5_ks_only_topology() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "ks_rim_controls",
                    "rim_controls",
                    Some("KS"),
                    "captures/ks-controls.jsonl",
                ),
            ],
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-throttle-only-sweep.jsonl",
                "captures/r5-brake-only-sweep.jsonl",
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            receipt
                .artifacts
                .iter()
                .all(|artifact| artifact.path != "captures/es-controls.jsonl"
                    && artifact.path != "captures/r5-throttle-only-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_r5_es_only_topology() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "es_rim_controls",
                    "rim_controls",
                    Some("ES"),
                    "captures/es-controls.jsonl",
                ),
            ],
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-throttle-only-sweep.jsonl",
                "captures/r5-brake-only-sweep.jsonl",
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            receipt
                .artifacts
                .iter()
                .all(|artifact| artifact.path != "captures/ks-controls.jsonl"
                    && artifact.path != "captures/r5-clutch-only-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_r5_throttle_brake_only_topology() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "throttle",
                    "throttle",
                    None,
                    "captures/r5-throttle-only-sweep.jsonl",
                ),
                ("brake", "brake", None, "captures/r5-brake-only-sweep.jsonl"),
            ],
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(receipt.artifacts.iter().all(|artifact| artifact.path
            != "captures/r5-clutch-only-sweep.jsonl"
            && artifact.path != "captures/r5-handbrake-only-sweep.jsonl"));
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_r5_hub_handbrake_without_standalone_hbp() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[
                (
                    "steering",
                    "steering",
                    None,
                    "captures/r5-steering-sweep.jsonl",
                ),
                (
                    "handbrake",
                    "handbrake",
                    None,
                    "captures/r5-handbrake-only-sweep.jsonl",
                ),
            ],
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-throttle-only-sweep.jsonl",
                "captures/r5-brake-only-sweep.jsonl",
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            receipt
                .artifacts
                .iter()
                .all(|artifact| artifact.path != "captures/hbp-standalone-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_standalone_hbp_when_topology_declares_it() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[(
                "steering",
                "steering",
                None,
                "captures/r5-steering-sweep.jsonl",
            )],
        )?;
        declare_standalone_control_topology(
            dir.path(),
            "standalone_handbrake",
            "handbrake",
            "moza-hbp-standalone",
            "standalone_handbrake",
            product_ids::HBP_HANDBRAKE,
            "captures/hbp-standalone-sweep.jsonl",
        )?;
        write_text_file(
            &dir.path().join("captures/hbp-standalone-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(product_ids::HBP_HANDBRAKE, "01000000"),
                capture_line(product_ids::HBP_HANDBRAKE, "01FFFF01")
            ),
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-throttle-only-sweep.jsonl",
                "captures/r5-brake-only-sweep.jsonl",
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            passive_capture_requirements_for_lane(dir.path())
                .iter()
                .any(|requirement| requirement.relative_path
                    == "captures/hbp-standalone-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_standalone_pedals_when_topology_declares_them() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        set_required_hub_controls(
            dir.path(),
            &[(
                "steering",
                "steering",
                None,
                "captures/r5-steering-sweep.jsonl",
            )],
        )?;
        declare_standalone_control_topology(
            dir.path(),
            "standalone_throttle",
            "throttle",
            "moza-srp-standalone",
            "standalone_pedals",
            product_ids::SR_P_PEDALS,
            "captures/srp-standalone-sweep.jsonl",
        )?;
        add_topology_control(
            dir.path(),
            "standalone_brake",
            "brake",
            "moza-srp-standalone",
            "standalone_usb",
            "captures/srp-standalone-sweep.jsonl",
        )?;
        write_text_file(
            &dir.path().join("captures/srp-standalone-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(product_ids::SR_P_PEDALS, "0100000000"),
                capture_line(product_ids::SR_P_PEDALS, "01FFFFFFFF")
            ),
        )?;
        remove_optional_capture_files(
            dir.path(),
            &[
                "captures/r5-throttle-only-sweep.jsonl",
                "captures/r5-brake-only-sweep.jsonl",
                "captures/r5-clutch-only-sweep.jsonl",
                "captures/r5-handbrake-only-sweep.jsonl",
                "captures/ks-controls.jsonl",
                "captures/es-controls.jsonl",
            ],
        )?;
        refresh_passive_parser_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.success,
            "{}",
            serde_json::to_string_pretty(&receipt)?
        );
        assert!(
            passive_capture_requirements_for_lane(dir.path())
                .iter()
                .any(|requirement| requirement.relative_path
                    == "captures/srp-standalone-sweep.jsonl")
        );
        Ok(())
    }

    #[test]
    fn verify_moza_topology_observed_accepts_mixed_vendor_endpoint() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let external_pedals = sample_mixed_vendor_pedals_json_device();
        add_topology_endpoint(
            dir.path(),
            "external-pedals",
            "standalone_pedals",
            "0x1234",
            "0xABCD",
            false,
        )?;
        add_topology_control(
            dir.path(),
            "external_throttle",
            "throttle",
            "external-pedals",
            "cross_device",
            "captures/srp-standalone-sweep.jsonl",
        )?;
        append_device_to_observation_receipts(dir.path(), &external_pedals)?;

        let gate = verify_moza_topology_observed_gate(dir.path());

        assert_eq!(gate.status, "pass", "{}", gate.details);
        assert!(
            gate.details.contains("external-pedals:0x1234:0xABCD"),
            "{}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_moza_topology_observed_accepts_multiple_r5_endpoints() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let r5_v1 = sample_trusted_r5_v1_json_device();
        add_topology_endpoint(
            dir.path(),
            "moza-r5-v1-if2",
            "wheelbase_hub",
            MOZA_VENDOR_HEX,
            "0x0004",
            true,
        )?;
        add_topology_control(
            dir.path(),
            "v1_ks_rim_controls",
            "rim_controls",
            "moza-r5-v1-if2",
            "wheelbase_hub",
            "captures/ks-controls.jsonl",
        )?;
        append_device_to_observation_receipts(dir.path(), &r5_v1)?;

        let gate = verify_moza_topology_observed_gate(dir.path());

        assert_eq!(gate.status, "pass", "{}", gate.details);
        assert!(
            gate.details.contains("moza-r5-v1-if2:0x346E:0x0004"),
            "{}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_device_list_no_ffb_writes() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("device-list.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl device list",
                "no_ffb_writes": false,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": []
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(receipt.gates.iter().any(|gate| {
            gate.name == "passive_receipts_no_ffb_writes" && gate.status == "fail"
        }));
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_no_out_of_scope_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("moza-probe.json"),
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "devices": [sample_r5_json_device()]
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(receipt.gates.iter().any(|gate| {
            gate.name == "passive_receipts_no_ffb_writes" && gate.status == "fail"
        }));
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_expected_observation_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("moza-probe.json"))?;
        receipt["command"] = serde_json::json!("wheelctl moza init");
        write_test_json_file(&dir.path().join("moza-probe.json"), &receipt)?;

        let gate = verify_passive_no_writes_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("wrong_command"),
            "expected wrong-command failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_successful_observation_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("hid-list.json"))?;
        receipt["success"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("hid-list.json"), &receipt)?;

        let gate = verify_passive_receipts_success_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert_eq!(gate.name, "passive_receipts_successful");
        assert!(
            gate.details.contains("unsuccessful"),
            "expected unsuccessful receipt failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_no_writes_accepts_unsuccessful_safe_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("parser-fixture-validation.json"))?;
        receipt["success"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("parser-fixture-validation.json"), &receipt)?;

        let gate = verify_passive_no_writes_gate(dir.path());

        assert_eq!(gate.status, "pass", "{}", gate.details);
        assert_eq!(gate.name, "passive_receipts_no_ffb_writes");
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_no_hid_open_for_pure_observation_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("descriptor.json"))?;
        receipt["no_hid_device_opened"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("descriptor.json"), &receipt)?;

        let gate = verify_passive_no_writes_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("opened_hid"),
            "expected no-HID-open failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_hardware_doctor_no_hid_open() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("hardware-doctor.json"))?;
        receipt["no_hid_device_opened"] = serde_json::json!(false);
        write_test_json_file(&dir.path().join("hardware-doctor.json"), &receipt)?;

        let gate = verify_passive_no_writes_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("hardware-doctor.json"),
            "expected hardware doctor no-HID-open failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_parser_validation_gate_rejects_single_capture_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("parser-fixture-validation.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl moza validate-capture",
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "no_hid_device_opened": true,
                "total_reports": 1,
                "parsed_reports": 1,
                "rejected_reports": 0
            }),
        )?;

        let gate = verify_parser_validation_gate(dir.path());

        assert_eq!(gate.status, "fail");
        Ok(())
    }

    #[test]
    fn verify_bundle_missing_required_artifacts_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("manifest.json"),
            &sample_lane_manifest("passive_capture_ready", false, false),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(receipt.missing_artifacts > 0);
        assert!(receipt.failed_gates > 0);
        Ok(())
    }

    #[test]
    fn verify_bundle_missing_passive_receipts_suggests_observe_only_next_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(!receipt.next_commands.is_empty());
        assert!(
            receipt
                .next_commands
                .iter()
                .any(|command| command.contains("wheelctl moza init-lane"))
        );
        assert!(receipt.next_commands.iter().any(|command| {
            command.contains("wheelctl moza capture-input")
                && command.contains("captures/r5-steering-sweep.jsonl")
        }));
        assert!(
            receipt.next_commands.iter().any(|command| command.contains(
                "wheelctl moza descriptor --device 0x346E:0x0014 --report-descriptor-hex-file"
            )),
            "passive next_commands should include the descriptor file fallback"
        );
        assert!(
            receipt.next_commands.iter().any(|command| command.contains(
                "wheelctl moza descriptor --device 0x346E:0x0014 --report-descriptor-bin-file"
            )),
            "passive next_commands should include the binary descriptor file fallback"
        );
        assert!(
            receipt.operator_actions.iter().any(|action| action
                .contains("Export the R5 HID report descriptor byte block")
                && action.contains("wDescriptorLength")
                && action.contains("ERROR_INVALID_PARAMETER")
                && action.contains("Do not run firmware or DFU")),
            "passive operator actions should explain the descriptor export fallback: {:?}",
            receipt.operator_actions
        );
        assert!(
            receipt
                .next_commands
                .iter()
                .any(|command| command.contains("wheelctl hardware doctor")
                    && command.contains("hardware-doctor.json")),
            "passive next_commands should include hardware doctor receipt generation"
        );
        assert!(
            receipt
                .next_commands
                .iter()
                .any(|command| command.contains("wheelctl moza verify-bundle")
                    && command.contains("--stage passive"))
        );
        let joined = receipt.next_commands.join("\n").to_ascii_lowercase();
        for forbidden in ["torque-test", "zero", "direct", "high-torque"] {
            assert!(
                !joined.contains(forbidden),
                "passive next_commands must not include {forbidden}: {joined}"
            );
        }
        Ok(())
    }

    #[test]
    fn verify_bundle_failed_existing_capture_suggests_analysis_not_recapture() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n");
        assert!(
            joined.contains("wheelctl moza analyze-lane"),
            "failed existing captures should suggest offline lane analysis: {joined}"
        );
        assert!(
            joined.contains("wheelctl moza sync-role-status"),
            "failed existing captures should suggest semantic-status sync: {joined}"
        );
        assert!(
            receipt.operator_actions.iter().any(|action| action
                .contains("Throttle capture parsed")
                && action.contains("check throttle pedal cable")
                && action.contains("Pit House is unavailable")
                && action.contains("must not be probed or configured")),
            "failed throttle role should include a physical-path diagnostic action: {:?}",
            receipt.operator_actions
        );
        assert!(
            !joined.contains("wheelctl moza capture-input"),
            "existing parseable captures should not be blindly recaptured from next_commands: {joined}"
        );
        for passing_observation_command in [
            "wheelctl moza init-lane",
            "wheelctl device list",
            "wheelctl moza probe",
            "hid-capture list",
            "wheelctl hardware doctor",
            "wheelctl moza descriptor",
        ] {
            assert!(
                !joined.contains(passing_observation_command),
                "failed parser-visible role evidence should not suggest already-passing observation command {passing_observation_command}: {joined}"
            );
        }
        assert!(
            !joined.contains("wheelctl moza promote-fixtures"),
            "failed parser-visible role evidence should not suggest fixture promotion: {joined}"
        );
        assert!(
            !joined.contains("promoted_capture_fixtures_replay_through_moza_parser"),
            "failed parser-visible role evidence should not suggest promoted-fixture replay tests: {joined}"
        );
        assert!(
            !joined.contains("wheelctl moza promote-manifest"),
            "failed passive verification should not suggest manifest promotion: {joined}"
        );
        assert!(
            !joined.contains("wheelctl moza audit-lane"),
            "failed passive verification should not suggest lane audit: {joined}"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_reports_declared_endpoint_observations() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        let r5 = receipt
            .endpoint_observations
            .iter()
            .find(|endpoint| endpoint.id == "moza-r5-if2")
            .ok_or("expected R5 topology endpoint observation")?;
        assert_eq!(r5.kind.as_deref(), Some("wheelbase_hub"));
        assert_eq!(r5.vendor_id.as_deref(), Some("0x346E"));
        assert_eq!(r5.product_id.as_deref(), Some("0x0014"));
        assert_eq!(r5.interface_number, Some(2));
        assert_eq!(r5.usage_page.as_deref(), Some("0x0001"));
        assert_eq!(r5.usage.as_deref(), Some("0x0004"));
        assert_eq!(r5.output_capable, Some(true));
        assert!(
            r5.required_logical_controls
                .iter()
                .any(|control| control == "throttle"),
            "expected declared throttle role in endpoint observation: {:?}",
            r5.required_logical_controls
        );
        assert_eq!(r5.observed_artifact_count, 4);
        assert_eq!(r5.metadata_match_artifact_count, 4);
        assert!(r5.artifacts.iter().all(|artifact| {
            artifact.status == "read"
                && artifact.vid_pid_count == 1
                && artifact.metadata_match_count == 1
        }));
        Ok(())
    }

    #[test]
    fn verify_bundle_validated_captures_suggest_fixture_promotion_when_missing() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        fs::remove_file(dir.path().join("fixture-promotion.json"))?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n");
        assert!(
            joined.contains("wheelctl moza promote-fixtures"),
            "missing fixture promotion should be suggested after capture validation passes: {joined}"
        );
        assert!(
            joined.contains("promoted_capture_fixtures_replay_through_moza_parser"),
            "promoted fixture replay test should follow fixture promotion when validation passes: {joined}"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_zero_next_commands_wait_for_passive_gates() -> TestResult {
        let dir = tempfile::tempdir()?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Zero);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n");
        assert!(
            joined.contains("--stage passive"),
            "blocked zero-stage guidance should return operators to passive verification: {joined}"
        );
        for forbidden in [
            "wheelctl moza zero --device",
            "wheelctl moza watchdog-proof",
            "wheelctl moza disconnect-proof",
            "wheelctl moza torque-test",
            "--stage zero",
            "manifest-promotion-zero.json",
            "lane-audit-zero.json",
        ] {
            assert!(
                !joined.contains(forbidden),
                "zero-stage next_commands must not include {forbidden} before passive gates pass: {joined}"
            );
        }
        assert!(
            receipt
                .operator_actions
                .iter()
                .any(|action| action.contains("Export the R5 HID report descriptor byte block")),
            "later-stage receipts should still surface passive descriptor operator action: {:?}",
            receipt.operator_actions
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_zero_next_commands_start_zero_only_after_passive_gates_pass() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Zero);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n");
        for expected in [
            "wheelctl moza zero --device",
            "wheelctl moza watchdog-proof",
            "wheelctl moza disconnect-proof",
            "--stage zero",
        ] {
            assert!(
                joined.contains(expected),
                "zero-stage next_commands should include {expected} after passive gates pass: {joined}"
            );
        }
        for forbidden in [
            "wheelctl moza torque-test",
            "manifest-promotion-zero.json",
            "lane-audit-zero.json",
        ] {
            assert!(
                !joined.contains(forbidden),
                "zero-stage next_commands must not include {forbidden} before zero gates pass: {joined}"
            );
        }
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_next_commands_wait_for_zero_gates() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n");
        assert!(
            joined.contains("wheelctl moza zero --device"),
            "smoke-ready guidance should first suggest the missing zero-stage proof: {joined}"
        );
        for forbidden in [
            "wheelctl moza torque-test",
            "wheelctl moza init --device",
            "wheelctl moza simulator-ffb-smoke",
            "wheelctl moza pit-house-proof",
            "--stage smoke-ready",
            "manifest-promotion-smoke-ready.json",
            "lane-audit-smoke-ready.json",
        ] {
            assert!(
                !joined.contains(forbidden),
                "smoke-ready next_commands must not include {forbidden} before zero gates pass: {joined}"
            );
        }
        Ok(())
    }

    #[test]
    fn verify_bundle_next_commands_use_manifest_wheelbase_pid() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut manifest = sample_lane_manifest("not_started", false, false);
        manifest["hardware"]["wheelbase_pid"] = Value::String("0x0004".to_string());
        manifest["topology"] = moza_lane_manifest_topology_value(product_ids::R5_V1);
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .next_commands
                .iter()
                .all(|command| !command.contains("wheelctl moza init-lane")),
            "valid manifest should not suggest reinitializing the lane: {}",
            receipt.next_commands.join("\n")
        );
        let descriptor_fallback_command = receipt
            .next_commands
            .iter()
            .find(|command| command.contains("--report-descriptor-hex-file"))
            .ok_or("expected descriptor file fallback next command")?;
        assert!(
            descriptor_fallback_command.contains("--device 0x346E:0x0004"),
            "descriptor file fallback should use manifest PID: {descriptor_fallback_command}"
        );
        assert!(
            !descriptor_fallback_command.contains("--device 0x346E:0x0014"),
            "descriptor file fallback must not suggest the default V2 PID for a V1 lane: {descriptor_fallback_command}"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_root_lane_next_commands_target_dated_child() -> TestResult {
        let lane_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5");
        let receipt = verify_bundle_dir(&lane_root, MozaBundleStage::Passive);

        assert!(!receipt.success);
        let joined = receipt.next_commands.join("\n").replace('\\', "/");
        assert!(
            joined.contains("ci/hardware/moza-r5/YYYY-MM-DD"),
            "root lane next_commands should point operators at a dated child lane: {joined}"
        );
        assert!(
            !joined.contains("wheelctl moza promote-fixtures"),
            "root lane next_commands should wait for dated-lane capture validation before fixture promotion: {joined}"
        );
        assert!(
            !joined.contains("wheelctl moza promote-manifest"),
            "root lane next_commands should wait for a passing dated-lane verifier before manifest promotion: {joined}"
        );
        assert!(
            !joined.contains("--lane ci/hardware/moza-r5 --wheelbase-pid"),
            "root lane next_commands must not write manifest.json into the lane docs/schema root: {joined}"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_next_commands_match_dependency_order() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_zero_stage_receipts(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        let commands = &receipt.next_commands;
        let index = |needle: &str| -> Result<usize, &'static str> {
            commands
                .iter()
                .position(|command| command.contains(needle))
                .ok_or("expected command not found")
        };

        let telemetry_proof = index("wheelctl moza simulator-telemetry-proof")?;
        let canonical_writer = commands
            .iter()
            .enumerate()
            .skip(telemetry_proof.saturating_add(1))
            .find_map(|(index, command)| {
                command
                    .starts_with("wheeld --hardware-lane ")
                    .then_some(index)
            })
            .ok_or("expected lane-bound simulator writer command")?;
        let ffb_smoke = index("wheelctl moza simulator-ffb-smoke")?;
        let mode_change_observation =
            index("wheelctl moza pit-house-observation --case mode-change")?;
        let mode_change_case = commands
            .iter()
            .position(|command| {
                command.contains("wheelctl moza pit-house-case")
                    && command.contains("--case mode-change")
            })
            .ok_or("expected mode-change Pit House case command")?;
        let pit_house_proof = index("wheelctl moza pit-house-proof")?;
        let smoke_verification = index("--stage smoke-ready")?;

        assert!(
            telemetry_proof < ffb_smoke,
            "simulator FFB smoke must run after telemetry proof"
        );
        assert!(
            telemetry_proof < canonical_writer && canonical_writer < ffb_smoke,
            "lane-bound simulator writer command must run between telemetry proof and simulator FFB smoke"
        );
        assert!(
            ffb_smoke < mode_change_observation,
            "mode-change Pit House observation must run after simulator FFB smoke"
        );
        assert!(
            mode_change_observation < mode_change_case,
            "mode-change Pit House case must run after its observation"
        );
        assert!(
            mode_change_case < pit_house_proof,
            "Pit House proof must run after the mode-change case"
        );
        assert!(
            pit_house_proof < smoke_verification,
            "smoke-ready verification must run after Pit House proof"
        );

        let observation_commands = commands
            .iter()
            .filter(|command| command.contains("wheelctl moza pit-house-observation"))
            .count();
        assert_eq!(observation_commands, 5);
        assert!(
            commands
                .iter()
                .filter(|command| command.contains("wheelctl moza pit-house-observation"))
                .all(|command| command.contains("--evidence-artifact pit-house-evidence-")),
            "every Pit House observation next_command must include a verifier-required evidence artifact"
        );
        let joined = commands.join("\n").replace('\\', "/");
        for artifact in [
            "pit-house-observation-closed.json",
            "pit-house-closed.json",
            "pit-house-direct-blocked.json",
            "pit-house-observation-mode-change.json",
            "pit-house-mode-change.json",
            "pit-house-firmware-page.json",
        ] {
            assert!(
                joined.contains(artifact),
                "smoke-ready next_commands should use documented Pit House artifact name {artifact}: {joined}"
            );
        }
        for stale_artifact in [
            "pit-house-closed-observation.json",
            "pit-house-closed-case.json",
            "pit-house-open-direct-case.json",
            "pit-house-firmware-page-case.json",
        ] {
            assert!(
                !joined.contains(stale_artifact),
                "smoke-ready next_commands should not use stale Pit House artifact name {stale_artifact}: {joined}"
            );
        }
        Ok(())
    }

    #[test]
    fn verify_bundle_next_commands_parse_as_wheelctl_commands() -> TestResult {
        let lane_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5");
        let blocked_root_receipt = verify_bundle_dir(&lane_root, MozaBundleStage::SmokeReady);
        let staged = tempfile::tempdir()?;
        write_minimal_passive_bundle(staged.path())?;
        write_zero_stage_receipts(staged.path())?;
        let smoke_ready_receipt = verify_bundle_dir(staged.path(), MozaBundleStage::SmokeReady);

        assert!(!blocked_root_receipt.success);
        assert!(!smoke_ready_receipt.success);
        let mut checked = 0usize;
        for receipt in [&blocked_root_receipt, &smoke_ready_receipt] {
            for command in receipt
                .next_commands
                .iter()
                .filter(|command| command.starts_with("wheelctl "))
            {
                let command = command_with_test_placeholders(command);
                let args = split_generated_command(&command)?;
                crate::Cli::try_parse_from(args).map_err(|error| {
                    format!("generated next_command failed to parse: {command}\n{error}")
                })?;
                checked += 1;
            }
        }

        assert!(
            checked >= 40,
            "expected to parse the generated wheelctl bring-up commands, checked {checked}"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_next_commands_use_known_external_command_forms() -> TestResult {
        let lane_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5");
        let blocked_root_receipt = verify_bundle_dir(&lane_root, MozaBundleStage::SmokeReady);
        let staged = tempfile::tempdir()?;
        write_minimal_passive_bundle(staged.path())?;
        write_zero_stage_receipts(staged.path())?;
        let smoke_ready_receipt = verify_bundle_dir(staged.path(), MozaBundleStage::SmokeReady);

        assert!(!blocked_root_receipt.success);
        assert!(!smoke_ready_receipt.success);
        let mut checked = 0usize;
        for receipt in [&blocked_root_receipt, &smoke_ready_receipt] {
            for command in receipt
                .next_commands
                .iter()
                .filter(|command| !command.starts_with("wheelctl "))
            {
                let command = command_with_test_placeholders(command);
                let args = split_generated_command(&command)?;
                let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

                match arg_refs.as_slice() {
                    [
                        "hid-capture",
                        "list",
                        "--vendor",
                        "0x346E",
                        "--json-out",
                        path,
                    ] => {
                        assert!(
                            path.replace('\\', "/").ends_with("/hid-list.json"),
                            "generated hid-capture next_command should write hid-list.json: {command}"
                        );
                    }
                    ["wheeld", "--hardware-lane", "moza-r5"] => {}
                    ["wheeld", "--hardware-lane", lane] => {
                        assert!(
                            Path::new(lane).ends_with(staged.path())
                                || lane.replace('\\', "/").contains("ci/hardware/moza-r5/"),
                            "generated wheeld next_command should target the staged or dated Moza lane: {command}"
                        );
                    }
                    [
                        "cargo",
                        "test",
                        "-p",
                        "racing-wheel-hid-moza-protocol",
                        "promoted_capture_fixtures_replay_through_moza_parser",
                    ] => {}
                    _ => {
                        return Err(format!(
                            "unexpected external generated next_command: {command}"
                        )
                        .into());
                    }
                }
                checked += 1;
            }
        }

        assert_eq!(
            checked, 3,
            "expected hid-capture, lane-status wheeld, and canonical simulator-writer wheeld next_commands"
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_complete_descriptor_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("descriptor.json"),
            &serde_json::json!({
                "success": true,
                "no_ffb_writes": true,
                "devices": [{
                    "vendor_id": "0x346E",
                    "product_id": "0x0014",
                    "product_name": "Moza R5",
                    "serial_number_present": true
                }]
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "descriptor_metadata")
            .ok_or("missing descriptor metadata gate")?;

        assert!(!receipt.success);
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("diagnostics=")
                && gate.details.contains("descriptor_source")
                && gate.details.contains("report_descriptor_crc32")
                && gate.details.contains("identity_metadata")
                && gate.details.contains("interface_usage_metadata")
                && gate.details.contains("input_report_lengths")
                && gate.details.contains("output_report_0x20_len_8")
                && gate.details.contains("feature_report_ids_0x03_0x11"),
            "expected actionable descriptor diagnostics, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_live_r5_v1_descriptor_input_length() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_test_json_file(
            &dir.path().join("descriptor.json"),
            &serde_json::json!({
                "success": true,
                "command": "wheelctl moza descriptor",
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "devices": [sample_trusted_r5_v1_json_device()]
            }),
        )?;

        let gate = verify_descriptor_metadata_gate(dir.path());

        assert_eq!(gate.status, "pass", "{}", gate.details);
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_parseable_capture_sweeps() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-steering-sweep.jsonl"),
            &capture_line(product_ids::R5_V2, &wheelbase_report_hex(0x8000, 0, 0, 0)),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "passive_captures_parse" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_isolated_clutch_role_movement() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-clutch-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0x0000, 0x0000, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8001, 0x0000, 0x0000, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "passive_captures_parse" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_standalone_srp_capture_pid() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        declare_standalone_control_topology(
            dir.path(),
            "throttle",
            "throttle",
            "moza-srp-standalone",
            "standalone_pedals",
            product_ids::SR_P_PEDALS,
            "captures/srp-standalone-sweep.jsonl",
        )?;
        write_text_file(
            &dir.path().join("captures/srp-standalone-sweep.jsonl"),
            &format!(
                "{}\n{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0x0000, 0x0000, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0xFFFF, 0x8000, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(product_ids::SR_P_PEDALS, "0100000000")
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "passive_captures_parse")
            .ok_or("expected passive capture parse gate")?;
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("captures/srp-standalone-sweep.jsonl")
                && gate.details.contains("product_ids_ok=false")
                && gate.details.contains("0x0003")
                && gate.details.contains("0x0014"),
            "expected standalone SR-P product ID failure details, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_standalone_hbp_capture_pid() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        declare_standalone_control_topology(
            dir.path(),
            "handbrake",
            "handbrake",
            "moza-hbp-standalone",
            "standalone_handbrake",
            product_ids::HBP_HANDBRAKE,
            "captures/hbp-standalone-sweep.jsonl",
        )?;
        write_text_file(
            &dir.path().join("captures/hbp-standalone-sweep.jsonl"),
            &format!(
                "{}\n{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0x0000, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0xFFFF, 0, 0, 0, 0, 0)
                ),
                capture_line(product_ids::HBP_HANDBRAKE, "01000000")
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "passive_captures_parse")
            .ok_or("expected passive capture parse gate")?;
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("captures/hbp-standalone-sweep.jsonl")
                && gate.details.contains("product_ids_ok=false")
                && gate.details.contains("0x0022")
                && gate.details.contains("0x0014"),
            "expected standalone HBP product ID failure details, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_capture_input_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-idle.jsonl"),
            r#"{"product_id":"0x0014","report_len":31,"data_hex":"01008000000000000000000000000000000000000000000000000000000000"}"#,
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "passive_captures_parse" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_documented_repo_fixture_paths() -> TestResult {
        let repo = tempfile::tempdir()?;
        let lane = repo.path().join("ci/hardware/moza-r5/2026-05-06");
        write_minimal_passive_bundle(&lane)?;

        let mut receipt = read_json_path(&lane.join("fixture-promotion.json"))?;
        let fixtures = receipt
            .get_mut("fixtures")
            .and_then(Value::as_array_mut)
            .ok_or("expected fixture entries")?;
        for entry in fixtures {
            let fixture_id = json_string(entry, "fixture_id")
                .ok_or("expected fixture id")?
                .to_string();
            let current_fixture_out = json_string(entry, "fixture_out")
                .ok_or("expected fixture out")?
                .to_string();
            let repo_relative =
                format!("crates/hid-moza-protocol/fixtures/moza-r5-2026-05-06/{fixture_id}.json");
            let source = lane.join(current_fixture_out);
            let target = repo.path().join(&repo_relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source, &target)?;
            entry["fixture_out"] = serde_json::json!(repo_relative);
        }
        fs::remove_dir_all(lane.join("fixtures"))?;
        write_test_json_file(&lane.join("fixture-promotion.json"), &receipt)?;

        let verify = verify_bundle_dir(&lane, MozaBundleStage::Passive);

        assert!(
            verify.success,
            "expected documented repo fixture paths to verify: {:?}",
            verify.gates
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_ks_es_full_control_reports() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/ks-controls.jsonl"),
            &capture_line(product_ids::R5_V2, "01008000000000"),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "passive_captures_parse" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_ks_control_variation() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/ks-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 8, rim_ids::KS, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 8, rim_ids::KS, 0, 0)
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "passive_captures_parse" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn validate_lane_captures_accepts_live_r5_ks_extended_controls() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/ks-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &live_r5_v1_ks_extended_report_hex(0x8001, 0x00, 0x00)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &live_r5_v1_ks_extended_report_hex(0x1234, 0x04, 0x03)
                )
            ),
        )?;

        let receipt = validate_lane_captures(dir.path())?;
        let entry = receipt
            .captures
            .iter()
            .find(|entry| entry.capture == "captures/ks-controls.jsonl")
            .ok_or("missing ks-controls validation entry")?;
        let direction_requirement = entry
            .required_any_axis_variation
            .iter()
            .find(|requirement| requirement.group == "direction")
            .ok_or("missing direction requirement")?;

        assert!(entry.success, "expected KS extended controls to validate");
        assert!(entry.required_axis_values.is_empty());
        assert!(entry.axis_ranges.contains_key("ks_buttons_any_u8"));
        assert!(entry.axis_ranges.contains_key("ks_hat_u8"));
        assert!(
            entry
                .axis_ranges
                .contains_key("r5_v1_extended_ks_axis0_u16")
        );
        assert!(
            direction_requirement
                .axes
                .iter()
                .any(|axis| axis == "ks_hat_u8")
        );
        Ok(())
    }

    #[test]
    fn validate_capture_records_live_r5_v1_auxiliary_hub_signals() -> TestResult {
        let low = format!(
            r#"{{"product_id":"0x0004","report_len":42,"data_hex":"{}"}}"#,
            live_r5_v1_extended_aux_report_hex(0x0000, 0x8000)
        );
        let high = format!(
            r#"{{"product_id":"0x0004","report_len":42,"data_hex":"{}"}}"#,
            live_r5_v1_extended_aux_report_hex(0x1234, 0xFEDC)
        );
        let (_dir, path) = write_temp_capture(&[low.as_str(), high.as_str()])?;

        let receipt = validate_capture_file(&path, None)?;

        assert!(receipt.success);
        let aux0 = receipt
            .axis_ranges
            .get("r5_v1_extended_aux0_u16")
            .ok_or("expected aux0 axis stats")?;
        let aux1 = receipt
            .axis_ranges
            .get("r5_v1_extended_aux1_u16")
            .ok_or("expected aux1 axis stats")?;
        assert_eq!(aux0.min, Some(0x0000));
        assert_eq!(aux0.max, Some(0x1234));
        assert_eq!(aux1.min, Some(0x8000));
        assert_eq!(aux1.max, Some(0xFEDC));
        assert_eq!(
            receipt
                .axis_ranges
                .get("throttle_u16")
                .and_then(|axis| axis.max),
            Some(0)
        );
        assert_eq!(
            receipt
                .axis_ranges
                .get("clutch_u16")
                .and_then(|axis| axis.max),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn analyze_lane_reports_trailer_only_throttle_capture_without_control_evidence() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let receipt = analyze_lane_captures(dir.path())?;
        let throttle = receipt
            .captures
            .iter()
            .find(|entry| entry.capture == "captures/r5-throttle-only-sweep.jsonl")
            .ok_or("missing throttle analysis entry")?;
        let throttle_role = receipt
            .role_evidence
            .iter()
            .find(|entry| entry.control == "throttle")
            .ok_or("missing throttle role evidence entry")?;

        assert!(!receipt.success);
        assert!(
            receipt
                .missing_control_evidence
                .iter()
                .any(|capture| capture == "captures/r5-throttle-only-sweep.jsonl")
        );
        assert!(throttle.success);
        assert!(!throttle.control_evidence_ok);
        assert!(throttle.moving_required_axes.is_empty());
        assert_eq!(throttle.unique_moving_bytes_vs_idle, vec![38, 39, 40, 41]);
        assert!(
            throttle
                .missing_requirements
                .iter()
                .any(|requirement| requirement.contains("variation in hub_control_axis group"))
        );
        assert_eq!(throttle_role.role, "throttle");
        assert_eq!(throttle_role.semantic_status, "missing");
        assert!(!throttle_role.parser_visible);
        assert_eq!(
            throttle_role.evidence_capture.as_deref(),
            Some("captures/r5-throttle-only-sweep.jsonl")
        );
        assert!(
            receipt.safe_diagnostics.iter().any(|diagnostic| diagnostic
                .contains("only idle/trailer bytes moved")
                && diagnostic.contains("do not recapture blindly")),
            "expected throttle trailer-only diagnostic, got {:?}",
            receipt.safe_diagnostics
        );
        Ok(())
    }

    #[test]
    fn analyze_lane_throttle_diagnostic_reports_single_observed_moza_hid_endpoint() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        replace_observation_receipt_devices(dir.path(), &[sample_trusted_r5_v1_json_device()])?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let receipt = analyze_lane_captures(dir.path())?;

        assert!(
            receipt.safe_diagnostics.iter().any(|diagnostic| diagnostic
                .contains("only the R5 HID game-controller endpoint")
                && diagnostic.contains("must not be probed or configured")),
            "expected single-endpoint throttle diagnostic, got {:?}",
            receipt.safe_diagnostics
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_operator_action_uses_observed_single_hid_endpoint_context() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        replace_observation_receipt_devices(dir.path(), &[sample_trusted_r5_v1_json_device()])?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(
            receipt.operator_actions.iter().any(|action| action
                .contains("already show only the R5 HID game-controller endpoint")
                && action.contains("must not be probed or configured")),
            "expected single-endpoint operator action, got {:?}",
            receipt.operator_actions
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_includes_safe_analyzer_diagnostics() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "passive_captures_parse")
            .ok_or("missing passive capture parse gate")?;

        assert!(!receipt.success);
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("safe_diagnostics=")
                && gate.details.contains("only idle/trailer bytes moved")
                && gate.details.contains("do not recapture blindly"),
            "expected passive verifier to include analyzer diagnostic, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn validate_lane_captures_includes_safe_analyzer_diagnostics() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let receipt = validate_lane_captures(dir.path())?;

        assert!(!receipt.success);
        assert!(
            receipt.safe_diagnostics.iter().any(|diagnostic| diagnostic
                .contains("only idle/trailer bytes moved")
                && diagnostic.contains("do not recapture blindly")),
            "expected validate-captures to include analyzer diagnostic, got {:?}",
            receipt.safe_diagnostics
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_parser_validation_gate_includes_safe_diagnostics() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;
        let validation = validate_lane_captures(dir.path())?;
        write_test_json_file(
            &dir.path().join("parser-fixture-validation.json"),
            &serde_json::to_value(validation)?,
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "parser_fixture_validation")
            .ok_or("missing parser fixture validation gate")?;

        assert!(!receipt.success);
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("safe_diagnostics=")
                && gate.details.contains("only idle/trailer bytes moved")
                && gate.details.contains("do not recapture blindly"),
            "expected parser validation gate to include analyzer diagnostic, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn analyze_lane_role_status_marks_generic_extended_axes() {
        let entry = LaneCaptureAnalysisEntry {
            capture: "captures/r5-clutch-only-sweep.jsonl".to_string(),
            fixture_id: "r5_clutch_only_sweep".to_string(),
            success: true,
            total_reports: 2,
            decoded_reports: 2,
            rejected_reports: 0,
            moving_bytes: Vec::new(),
            unique_moving_bytes_vs_idle: Vec::new(),
            moving_words_le: Vec::new(),
            unique_moving_words_le_vs_idle: Vec::new(),
            moving_required_axes: vec!["r5_v1_extended_aux0_u16".to_string()],
            control_evidence_ok: true,
            missing_requirements: Vec::new(),
        };
        let mut notes = Vec::new();

        let status = role_semantic_status(true, Some(&entry), &mut notes);

        assert_eq!(status, "generic_aux");
        assert!(
            notes
                .iter()
                .any(|note| note.contains("semantic control naming remains unproven"))
        );
    }

    #[test]
    fn sync_role_status_updates_manifest_from_lane_analysis() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/r5-throttle-only-sweep.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0x00, 0x00, 0x00, 0x00])
                ),
                capture_line(
                    product_ids::R5_V1,
                    &live_r5_v1_trailer_report_hex([0xFF, 0xAA, 0x55, 0x40])
                )
            ),
        )?;

        let stale_check = sync_role_status_receipt(dir.path(), true)?;
        assert_eq!(json_bool(&stale_check, "success"), Some(false));
        assert!(
            json_u64(&stale_check, "stale_control_count").ok_or("missing stale control count")? > 0
        );

        let receipt = sync_role_status_receipt(dir.path(), false)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(json_bool(&receipt, "manifest_written"), Some(true));
        assert_eq!(json_bool(&receipt, "lane_analysis_success"), Some(false));

        let manifest = read_json_path(&dir.path().join("manifest.json"))?;
        assert_eq!(
            json_string(
                manifest
                    .pointer("/topology/logical_controls/steering")
                    .ok_or("missing steering control")?,
                "semantic_status"
            ),
            Some("proven")
        );
        assert_eq!(
            json_string(
                manifest
                    .pointer("/topology/logical_controls/throttle")
                    .ok_or("missing throttle control")?,
                "semantic_status"
            ),
            Some("missing")
        );

        let fresh_check = sync_role_status_receipt(dir.path(), true)?;
        assert_eq!(json_bool(&fresh_check, "success"), Some(true));
        assert_eq!(json_u64(&fresh_check, "stale_control_count"), Some(0));
        assert_eq!(json_bool(&fresh_check, "artifact_map_changed"), Some(false));
        assert_eq!(json_bool(&fresh_check, "manifest_written"), Some(false));
        Ok(())
    }

    #[test]
    fn sync_role_status_refreshes_manifest_artifact_contract() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        sync_role_status_receipt(dir.path(), false)?;

        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest["artifacts"]
            .as_object_mut()
            .ok_or("expected artifacts map")?
            .remove("hardware_doctor");
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;

        let stale_check = sync_role_status_receipt(dir.path(), true)?;
        assert_eq!(json_bool(&stale_check, "success"), Some(false));
        assert_eq!(json_bool(&stale_check, "artifact_map_changed"), Some(true));
        assert_eq!(json_u64(&stale_check, "stale_control_count"), Some(0));

        let receipt = sync_role_status_receipt(dir.path(), false)?;
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(json_bool(&receipt, "manifest_written"), Some(true));
        assert_eq!(json_bool(&receipt, "artifact_map_changed"), Some(true));

        let refreshed = read_json_path(&dir.path().join("manifest.json"))?;
        assert_eq!(
            json_string(
                refreshed
                    .get("artifacts")
                    .ok_or("missing artifacts map after refresh")?,
                "hardware_doctor"
            ),
            Some("hardware-doctor.json")
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_accepts_es_button_only_controls() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/es-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0x02, 0, 0, 0, 0)
                )
            ),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(receipt.success);
        Ok(())
    }

    #[test]
    fn validate_lane_captures_reports_missing_control_requirements() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_text_file(
            &dir.path().join("captures/es-controls.jsonl"),
            &format!(
                "{}\n{}",
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                ),
                capture_line(
                    product_ids::R5_V2,
                    &wheelbase_full_report_hex(0x8000, 0, 0, 0, 0, 0, 0, 0, 0, 0)
                )
            ),
        )?;

        let receipt = validate_lane_captures(dir.path())?;
        let entry = receipt
            .captures
            .iter()
            .find(|entry| entry.capture == "captures/es-controls.jsonl")
            .ok_or("missing es-controls validation entry")?;
        let button_requirement = entry
            .required_any_axis_variation
            .iter()
            .find(|requirement| requirement.group == "buttons")
            .ok_or("missing ES button requirement")?;

        assert!(!receipt.success);
        assert!(!entry.success);
        assert_eq!(button_requirement.axes, vec!["buttons_any_u8".to_string()]);
        assert!(entry.required_axis_values.is_empty());
        assert!(entry.missing_requirements.iter().any(|requirement| {
            requirement.contains("buttons group") && requirement.contains("buttons_any_u8")
        }));
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_fixture_promotion() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        fs::remove_file(dir.path().join("fixture-promotion.json"))?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(receipt.missing_artifacts > 0);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "fixture_promotion" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_rejects_unsanitized_fixture() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let fixture_id = passive_capture_requirements()
            .first()
            .ok_or("expected passive capture requirement")?
            .fixture_id;
        write_test_json_file(
            &dir.path().join(format!("fixtures/{fixture_id}.json")),
            &serde_json::json!({
                "schema_version": 1,
                "fixture_id": fixture_id,
                "no_ffb_writes": true,
                "reports": [{
                    "path": "hid-path-that-should-not-be-promoted",
                    "data_hex": "01008034127856"
                }]
            }),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "fixture_promotion" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_rejects_fixture_parser_replay_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let fixture_path = dir.path().join("fixtures/r5_idle.json");
        let mut fixture = read_json_path(&fixture_path)?;
        fixture["reports"][0]["parsed"]["steering_u16"] = serde_json::json!(0);
        write_test_json_file(&fixture_path, &fixture)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        let fixture_gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "fixture_promotion")
            .ok_or("expected fixture promotion gate")?;
        assert_eq!(fixture_gate.status, "fail");
        assert!(
            fixture_gate.details.contains("parser produced"),
            "expected parser replay mismatch details, got {}",
            fixture_gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_passive_requires_all_fixture_promotions() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        let mut receipt = read_json_path(&dir.path().join("fixture-promotion.json"))?;
        let fixture_count = {
            let fixtures = receipt
                .get_mut("fixtures")
                .and_then(Value::as_array_mut)
                .ok_or("expected fixtures array")?;
            fixtures.pop();
            fixtures.len()
        };
        receipt["fixture_count"] = serde_json::json!(fixture_count);
        write_test_json_file(&dir.path().join("fixture-promotion.json"), &receipt)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Passive);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "fixture_promotion" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_zero_rejects_dry_run_zero_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("zero-torque-proof.json"),
            &serde_json::json!({
                "success": true,
                "dry_run": true,
                "no_high_torque": true,
                "no_nonzero_torque": true,
                "report_id": "0x20",
                "torque_raw": 0,
                "flags": 0,
                "motor_enabled": false,
                "final_zero_sent": false
            }),
        )?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &real_watchdog_receipt(3),
        )?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Zero);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "zero_torque_real_hardware" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_zero_accepts_logged_real_zero_proof() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("zero-torque-proof.json"),
            &real_zero_receipt(100),
        )?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &real_watchdog_receipt(3),
        )?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Zero);

        assert!(receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "zero_torque_real_hardware" && gate.status == "pass" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_zero_requires_minimum_logged_writes() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("zero-torque-proof.json"),
            &real_zero_receipt(99),
        )?;
        write_test_json_file(
            &dir.path().join("watchdog-proof.json"),
            &real_watchdog_receipt(3),
        )?;
        write_test_json_file(
            &dir.path().join("disconnect-proof.json"),
            &real_disconnect_receipt(),
        )?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::Zero);

        assert!(!receipt.success);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "zero_torque_real_hardware" && gate.status == "fail" })
        );
        Ok(())
    }

    #[tokio::test]
    async fn promote_manifest_updates_smoke_claims_after_live_verification() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_test_json_file(
            &dir.path().join("manifest.json"),
            &sample_lane_manifest("zero_torque_ready", false, false),
        )?;
        let receipt_path = dir.path().join("manifest-promotion-smoke-ready.json");

        promote_manifest(
            false,
            dir.path(),
            MozaBundleStage::SmokeReady,
            Some(&receipt_path),
        )
        .await?;

        let manifest = read_json_path(&dir.path().join("manifest.json"))?;
        let receipt = read_json_path(&receipt_path)?;
        assert_eq!(
            json_string(&manifest, "completion_state"),
            Some("real_hardware_smoke_ready")
        );
        assert_eq!(json_bool(&manifest, "hardware_validated"), Some(true));
        assert_eq!(json_bool(&manifest, "simulator_validated"), Some(true));
        assert_eq!(json_bool(&manifest, "high_torque_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "release_ready"), Some(false));
        assert_eq!(json_bool(&receipt, "success"), Some(true));
        assert_eq!(
            json_string(&receipt, "command"),
            Some("wheelctl moza promote-manifest")
        );
        Ok(())
    }

    #[tokio::test]
    async fn promote_manifest_rejects_failed_live_verification() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;

        let result = promote_manifest(false, dir.path(), MozaBundleStage::Zero, None).await;

        assert!(result.is_err());
        let manifest = read_json_path(&dir.path().join("manifest.json"))?;
        assert_eq!(
            json_string(&manifest, "completion_state"),
            Some("passive_capture_ready")
        );
        assert_eq!(json_bool(&manifest, "hardware_validated"), Some(false));
        assert_eq!(json_bool(&manifest, "simulator_validated"), Some(false));
        Ok(())
    }

    #[test]
    fn promote_manifest_restores_previous_manifest_when_post_verification_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let manifest_path = dir.path().join("manifest.json");
        let original = sample_lane_manifest("zero_torque_ready", false, false);
        write_test_json_file(&manifest_path, &original)?;
        let mut manifest = read_json_path(&manifest_path)?;

        let result = promote_manifest_with_post_verification(
            &manifest_path,
            &mut manifest,
            "real_hardware_smoke_ready",
            true,
            true,
            || BundleVerificationReceipt {
                success: false,
                command: "wheelctl moza verify-bundle",
                generated_at_utc: now_utc(),
                lane: dir.path().display().to_string(),
                requested_stage: "smoke-ready".to_string(),
                missing_artifacts: 0,
                invalid_artifacts: 0,
                failed_gates: 1,
                artifacts: Vec::new(),
                gates: vec![BundleGateCheck::fail(
                    "manifest_no_overclaim",
                    "forced post-promotion verifier failure".to_string(),
                )],
                endpoint_observations: Vec::new(),
                operator_actions: Vec::new(),
                next_commands: Vec::new(),
                no_hid_device_opened: true,
                no_ffb_writes: true,
                no_serial_config_commands: true,
                no_firmware_or_dfu_commands: true,
                notes: Vec::new(),
            },
        );

        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|error| error.to_string().contains("previous manifest restored"))
                == Some(true)
        );
        let restored = read_json_path(&manifest_path)?;
        assert_eq!(restored, original);
        assert_eq!(manifest, original);
        Ok(())
    }

    #[test]
    fn audit_lane_accepts_complete_smoke_ready_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(receipt.success);
        assert!(receipt.live_verification_success);
        assert_eq!(receipt.missing_receipts, 0);
        assert_eq!(receipt.invalid_receipts, 0);
        assert_eq!(receipt.receipt_checks.len(), 6);
        Ok(())
    }

    #[test]
    fn audit_lane_requires_stored_promotion_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        fs::remove_file(dir.path().join("manifest-promotion-zero.json"))?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.missing_receipts, 1);
        assert!(
            receipt
                .receipt_checks
                .iter()
                .any(|check| check.path == "manifest-promotion-zero.json"
                    && check.status == "missing")
        );
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_failed_stored_verification_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut verification = stored_verification_receipt(dir.path(), MozaBundleStage::Zero);
        {
            let object = verification
                .as_object_mut()
                .ok_or("expected verification receipt object")?;
            object.insert("success".to_string(), serde_json::json!(false));
            object.insert("failed_gates".to_string(), serde_json::json!(1));
        }
        write_test_json_file(&dir.path().join("zero-verification.json"), &verification)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(
            receipt
                .receipt_checks
                .iter()
                .any(|check| check.path == "zero-verification.json" && check.status == "invalid")
        );
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_stored_verification_without_safety_contract() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut verification = stored_verification_receipt(dir.path(), MozaBundleStage::Zero);
        verification
            .as_object_mut()
            .ok_or("expected verification receipt object")?
            .remove("no_firmware_or_dfu_commands");
        write_test_json_file(&dir.path().join("zero-verification.json"), &verification)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(receipt.receipt_checks.iter().any(|check| {
            check.path == "zero-verification.json"
                && check.status == "invalid"
                && check.details.contains("no_out_of_scope=false")
        }));
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_stored_promotion_without_safety_contract() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut promotion = stored_promotion_receipt(dir.path(), MozaBundleStage::Zero);
        promotion
            .as_object_mut()
            .ok_or("expected promotion receipt object")?
            .remove("no_serial_config_commands");
        write_test_json_file(&dir.path().join("manifest-promotion-zero.json"), &promotion)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(receipt.receipt_checks.iter().any(|check| {
            check.path == "manifest-promotion-zero.json"
                && check.status == "invalid"
                && check.details.contains("no_out_of_scope=false")
        }));
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_stored_promotion_with_stale_embedded_verification() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut promotion = stored_promotion_receipt(dir.path(), MozaBundleStage::Zero);
        promotion["verification_before"]["requested_stage"] = serde_json::json!("passive");
        promotion["verification_after"]["failed_gates"] = serde_json::json!(1);
        write_test_json_file(&dir.path().join("manifest-promotion-zero.json"), &promotion)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(receipt.receipt_checks.iter().any(|check| {
            check.path == "manifest-promotion-zero.json"
                && check.status == "invalid"
                && check.details.contains("verification_before_ok=false")
                && check.details.contains("verification_after_ok=false")
        }));
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_stale_stored_verification_lane() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut verification = stored_verification_receipt(dir.path(), MozaBundleStage::Zero);
        verification["lane"] = serde_json::json!("ci/hardware/moza-r5/other-run");
        write_test_json_file(&dir.path().join("zero-verification.json"), &verification)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(receipt.receipt_checks.iter().any(|check| {
            check.path == "zero-verification.json"
                && check.status == "invalid"
                && check.details.contains("lane_ok=false")
        }));
        Ok(())
    }

    #[test]
    fn audit_lane_rejects_stale_stored_promotion_manifest_path() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        let mut promotion = stored_promotion_receipt(dir.path(), MozaBundleStage::Zero);
        promotion["manifest"] = serde_json::json!("ci/hardware/moza-r5/other-run/manifest.json");
        write_test_json_file(&dir.path().join("manifest-promotion-zero.json"), &promotion)?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert_eq!(receipt.invalid_receipts, 1);
        assert!(receipt.receipt_checks.iter().any(|check| {
            check.path == "manifest-promotion-zero.json"
                && check.status == "invalid"
                && check.details.contains("manifest_ok=false")
        }));
        Ok(())
    }

    #[test]
    fn audit_lane_fails_when_live_stage_verification_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        write_stage_audit_receipts(dir.path(), MozaBundleStage::SmokeReady)?;
        fs::remove_file(dir.path().join("simulator-ffb-smoke.json"))?;

        let receipt = audit_lane_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert!(!receipt.live_verification_success);
        assert_eq!(receipt.missing_receipts, 0);
        assert_eq!(receipt.invalid_receipts, 0);
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_ready_requires_service_status_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        fs::remove_file(dir.path().join("device-status.json"))?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        assert!(receipt.missing_artifacts > 0);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "service_status_receipts" && gate.status == "fail" })
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_torque_ready_status() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let mut receipt = service_device_status_receipt(dir.path());
        receipt["status"]["moza"]["safe_to_send_torque"] = serde_json::json!(true);
        write_test_json_file(&dir.path().join("device-status.json"), &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("observe_only=false"),
            "expected observe-only failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_requires_complete_support_artifact_index() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let mut receipt = support_bundle_receipt(dir.path());
        let artifact_index = receipt
            .pointer_mut("/moza_lane/artifact_index")
            .and_then(Value::as_array_mut)
            .ok_or("expected support bundle artifact index")?;
        artifact_index.retain(|artifact| {
            artifact.get("path").and_then(Value::as_str) != Some("zero-verification.json")
        });
        write_test_json_file(&dir.path().join("support-bundle.json"), &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false"),
            "expected support bundle artifact-index failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_requires_support_readiness_fields() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = support_bundle_receipt(dir.path());
        let readiness = receipt
            .pointer_mut("/moza_lane/readiness")
            .and_then(Value::as_object_mut)
            .ok_or("expected support readiness")?;
        readiness.remove("ready_for_low_torque");
        write_test_json_file(&path, &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false"),
            "expected readiness completeness failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_stale_support_readiness_claim() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_service_status_artifacts(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = read_json_path(&path)?;
        let readiness = receipt
            .pointer_mut("/moza_lane/readiness")
            .and_then(Value::as_object_mut)
            .ok_or("expected support readiness")?;
        readiness.insert(
            "highest_passing_stage".to_string(),
            serde_json::json!("smoke_ready"),
        );
        readiness.insert("next_required_stage".to_string(), Value::Null);
        readiness.insert("ready_for_zero_torque".to_string(), serde_json::json!(true));
        readiness.insert("ready_for_low_torque".to_string(), serde_json::json!(true));
        readiness.insert(
            "ready_for_real_hardware_smoke".to_string(),
            serde_json::json!(true),
        );
        readiness.insert(
            "passive_lane_audit_passed".to_string(),
            serde_json::json!(true),
        );
        readiness.insert(
            "zero_lane_audit_passed".to_string(),
            serde_json::json!(true),
        );
        readiness.insert(
            "smoke_ready_lane_audit_passed".to_string(),
            serde_json::json!(true),
        );
        write_test_json_file(&path, &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false"),
            "expected stale readiness failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_requires_support_artifact_status_consistency() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = support_bundle_receipt(dir.path());
        let artifact_index = receipt
            .pointer_mut("/moza_lane/artifact_index")
            .and_then(Value::as_array_mut)
            .ok_or("expected support bundle artifact index")?;
        let artifact = artifact_index
            .iter_mut()
            .find(|artifact| artifact.get("path").and_then(Value::as_str) == Some("manifest.json"))
            .ok_or("expected manifest artifact entry")?;
        artifact["status"] = serde_json::json!("pass");
        artifact["exists"] = serde_json::json!(false);
        artifact["valid"] = serde_json::json!(true);
        write_test_json_file(&path, &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false"),
            "expected artifact status consistency failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_stale_support_artifact_index() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_minimal_passive_bundle(dir.path())?;
        write_service_status_artifacts(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = read_json_path(&path)?;
        let artifact_index = receipt
            .pointer_mut("/moza_lane/artifact_index")
            .and_then(Value::as_array_mut)
            .ok_or("expected support bundle artifact index")?;
        let artifact = artifact_index
            .iter_mut()
            .find(|artifact| {
                artifact.get("path").and_then(Value::as_str) == Some("zero-torque-proof.json")
            })
            .ok_or("expected zero proof artifact entry")?;
        artifact["status"] = serde_json::json!("pass");
        artifact["exists"] = serde_json::json!(true);
        artifact["valid"] = serde_json::json!(true);
        write_test_json_file(&path, &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false"),
            "expected stale artifact-index failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_stale_moza_status_lane() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let receipt =
            moza_status_receipt(vec![sample_device()], Some("0x0014"), Some(stale.path()));
        write_test_json_file(&dir.path().join("moza-status.json"), &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("lane_ok=false"),
            "expected stale moza-status lane failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_stale_device_status_lane() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let receipt = service_device_status_receipt(stale.path());
        write_test_json_file(&dir.path().join("device-status.json"), &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_ok=false")
                || gate.details.contains("observe_only=false"),
            "expected stale device-status lane failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_stale_support_bundle_lane() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stale = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let receipt = support_bundle_receipt(stale.path());
        write_test_json_file(&dir.path().join("support-bundle.json"), &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("moza_lane_status_ok=false")
                || gate.details.contains("observe_only=false"),
            "expected stale support-bundle lane failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_service_status_gate_rejects_support_bundle_top_level_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = read_json_path(&path)?;
        let devices = receipt
            .get_mut("devices")
            .and_then(Value::as_array_mut)
            .ok_or("expected support bundle devices")?;
        let device = devices
            .get_mut(0)
            .ok_or("expected support bundle R5 device")?;
        device["product_id"] = serde_json::json!("0x0004");
        write_test_json_file(&path, &receipt)?;

        let gate = verify_service_status_gate(dir.path());

        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("top_level_pid_matches_status=false"),
            "expected top-level support-bundle PID mismatch failure, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_ready_accepts_detailed_receipts() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(
            receipt.success,
            "expected smoke-ready bundle to pass, gates={:?}",
            receipt.gates
        );
        assert_eq!(receipt.failed_gates, 0);
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "service_status_receipts" && gate.status == "pass" })
        );
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "pit_house_coexistence" && gate.status == "pass" })
        );
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "simulator_telemetry" && gate.status == "pass" })
        );
        assert!(
            receipt
                .gates
                .iter()
                .any(|gate| { gate.name == "simulator_ffb_bounded" && gate.status == "pass" })
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_ready_rejects_manifest_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let mut manifest = read_json_path(&dir.path().join("manifest.json"))?;
        manifest["hardware"]["wheelbase_pid"] = serde_json::json!("0x0004");
        write_test_json_file(&dir.path().join("manifest.json"), &manifest)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        let gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "manifest_r5_pid_consistency")
            .ok_or("expected manifest PID gate")?;
        assert_eq!(gate.status, "fail");
        assert!(
            gate.details.contains("simulator-ffb-smoke.json:0x0014")
                && gate
                    .details
                    .contains("service-status:0x0014,0x0014,0x0014,0x0014"),
            "expected smoke-ready receipt PID mismatch details, got {}",
            gate.details
        );
        Ok(())
    }

    #[test]
    fn verify_bundle_smoke_ready_rejects_support_bundle_top_level_pid_mismatch() -> TestResult {
        let dir = tempfile::tempdir()?;
        write_smoke_ready_bundle(dir.path())?;
        let path = dir.path().join("support-bundle.json");
        let mut receipt = read_json_path(&path)?;
        let devices = receipt
            .get_mut("devices")
            .and_then(Value::as_array_mut)
            .ok_or("expected support bundle devices")?;
        let device = devices
            .get_mut(0)
            .ok_or("expected support bundle R5 device")?;
        device["product_id"] = serde_json::json!("0x0004");
        write_test_json_file(&path, &receipt)?;

        let receipt = verify_bundle_dir(dir.path(), MozaBundleStage::SmokeReady);

        assert!(!receipt.success);
        let manifest_gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "manifest_r5_pid_consistency")
            .ok_or("expected manifest PID gate")?;
        assert_eq!(manifest_gate.status, "fail");
        assert!(
            manifest_gate
                .details
                .contains("service-status:0x0014,0x0014,0x0014,0x0004"),
            "expected manifest PID gate to include support-bundle top-level PID, got {}",
            manifest_gate.details
        );
        let service_gate = receipt
            .gates
            .iter()
            .find(|gate| gate.name == "service_status_receipts")
            .ok_or("expected service status gate")?;
        assert_eq!(service_gate.status, "fail");
        assert!(
            service_gate
                .details
                .contains("top_level_pid_matches_status=false"),
            "expected service gate to reject support-bundle top-level PID mismatch, got {}",
            service_gate.details
        );
        Ok(())
    }
}
