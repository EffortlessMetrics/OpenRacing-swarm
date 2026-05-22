//! Hardware environment diagnostics.
//!
//! The doctor command is observe-only. It initializes HID enumeration when
//! available, records tool/platform readiness, and never opens devices or sends
//! output, feature, serial, firmware, or DFU commands.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use hidapi::{DeviceInfo, HidApi};
use openracing_hardware_core::{DeviceCapabilityRegistry, DeviceFamily};
use openracing_pidff_common::report_ids as pidff_report_ids;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::commands::{
    HardwareCommands, HardwareLaneCommands, HardwareSniffCaptureTool, HardwareSniffPlatformHint,
    HardwareSniffScenario,
};

pub async fn execute(cmd: &HardwareCommands, json: bool) -> Result<()> {
    match cmd {
        HardwareCommands::Doctor { json_out } => doctor(json, json_out.as_deref()).await,
        HardwareCommands::BringupRail { family, json_out } => {
            bringup_rail(json, family, json_out.as_deref()).await
        }
        HardwareCommands::SniffPlan {
            family,
            scenario,
            lane,
            operator,
            device_note,
            capture_tools,
            platform_hint,
            json_out,
            md_out,
        } => {
            let request = HardwareSniffPlanRequest {
                family,
                scenario: *scenario,
                lane,
                operator,
                device_note,
                capture_tools,
                platform_hint: *platform_hint,
            };
            sniff_plan(json, &request, json_out.as_deref(), md_out.as_deref()).await
        }
        HardwareCommands::SniffReceipt {
            plan,
            pcapng,
            operator,
            app,
            scenario,
            device_note,
            evidence,
            json_out,
        } => {
            let request = HardwareSniffReceiptRequest {
                plan,
                pcapng: pcapng.as_deref(),
                operator: operator.as_deref(),
                app,
                scenario: *scenario,
                device_note: device_note.as_deref(),
                evidence,
            };
            sniff_receipt(json, &request, json_out.as_deref()).await
        }
        HardwareCommands::SniffNotesTemplate {
            plan,
            hardware_doctor,
            out,
            json_out,
        } => {
            sniff_notes_template(
                json,
                plan,
                hardware_doctor.as_deref(),
                out,
                json_out.as_deref(),
            )
            .await
        }
        HardwareCommands::SniffCapture {
            usbpcapcmd,
            usbpcap_interface,
            devices,
            duration_ms,
            out,
            overwrite,
            confirm_external_passive_capture,
            json_out,
        } => {
            let request = HardwareSniffCaptureRequest {
                usbpcapcmd,
                usbpcap_interface,
                devices,
                duration_ms: *duration_ms,
                out,
                overwrite: *overwrite,
                confirm_external_passive_capture: *confirm_external_passive_capture,
            };
            sniff_capture(json, &request, json_out.as_deref()).await
        }
        HardwareCommands::SniffSummary {
            pcapng,
            vendor,
            product,
            interface,
            include_payload_samples,
            max_samples_per_report,
            json_out,
            md_out,
        } => {
            let request = HardwareSniffSummaryRequest {
                pcapng,
                vendor: vendor.as_deref(),
                product: product.as_deref(),
                interface: *interface,
                include_payload_samples: *include_payload_samples,
                max_samples_per_report: *max_samples_per_report,
            };
            sniff_summary(json, &request, json_out.as_deref(), md_out.as_deref()).await
        }
        HardwareCommands::SniffBundle {
            plan,
            receipt,
            summary,
            operator_notes,
            operator_notes_receipt,
            include_pcapng,
            out,
            json_out,
        } => {
            let request = HardwareSniffBundleRequest {
                plan,
                receipt,
                summary,
                operator_notes,
                operator_notes_receipt: operator_notes_receipt.as_deref(),
                include_pcapng: include_pcapng.as_deref(),
                out,
            };
            sniff_bundle(json, &request, json_out.as_deref()).await
        }
        HardwareCommands::Lane(command) => execute_lane(command, json).await,
    }
}

async fn execute_lane(cmd: &HardwareLaneCommands, json: bool) -> Result<()> {
    match cmd {
        HardwareLaneCommands::Init {
            lane,
            family,
            topology,
            operator,
            required_roles,
            optional_roles,
            role_artifacts,
            role_endpoints,
            role_connections,
            overwrite,
            json_out,
        } => {
            let role_overrides = HardwareLaneRoleOverrides::from_cli(
                required_roles,
                optional_roles,
                role_artifacts,
                role_endpoints,
                role_connections,
            )?;
            init_lane(
                json,
                lane,
                family,
                topology,
                operator,
                &role_overrides,
                *overwrite,
                json_out.as_deref(),
            )
            .await
        }
        HardwareLaneCommands::Status { lane, json_out } => {
            lane_status(json, lane, json_out.as_deref()).await
        }
        HardwareLaneCommands::SetRoleEndpoint {
            lane,
            role,
            endpoint,
            json_out,
        } => lane_set_role_endpoint(json, lane, role, endpoint, json_out.as_deref()).await,
    }
}

async fn init_lane(
    json: bool,
    lane: &Path,
    family: &str,
    topology: &str,
    operator: &str,
    role_overrides: &HardwareLaneRoleOverrides,
    overwrite: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = scaffold_hardware_lane_with_overrides(
        lane,
        family,
        topology,
        operator,
        role_overrides,
        overwrite,
        json_out,
    )?;
    print_lane_init_receipt(json, &receipt)
}

async fn lane_status(json: bool, lane: &Path, json_out: Option<&Path>) -> Result<()> {
    let receipt = build_hardware_lane_status_receipt(lane)?;
    write_json_receipt(json_out, &receipt)?;
    print_lane_status_receipt(json, json_out, &receipt)
}

async fn lane_set_role_endpoint(
    json: bool,
    lane: &Path,
    role: &str,
    endpoint: &str,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = set_hardware_lane_role_endpoint(lane, role, endpoint, json_out)?;
    print_lane_role_endpoint_receipt(json, json_out, &receipt)
}

async fn bringup_rail(json: bool, family: &str, json_out: Option<&Path>) -> Result<()> {
    let receipt = build_bringup_rail_receipt(family)?;
    write_json_receipt(json_out, &receipt)?;
    print_bringup_rail_receipt(json, json_out, &receipt)?;
    Ok(())
}

async fn doctor(json: bool, json_out: Option<&Path>) -> Result<()> {
    let receipt = build_doctor_receipt();
    write_json_receipt(json_out, &receipt)?;
    print_doctor_receipt(json, json_out, &receipt)?;
    Ok(())
}

async fn sniff_plan(
    json: bool,
    request: &HardwareSniffPlanRequest<'_>,
    json_out: Option<&Path>,
    md_out: Option<&Path>,
) -> Result<()> {
    let plan = build_hardware_sniff_plan(request)?;
    write_json_receipt(json_out, &plan)?;
    if let Some(path) = md_out {
        write_text_file(path, &render_sniff_plan_markdown(&plan))?;
    }
    print_sniff_plan(json, json_out, md_out, &plan)
}

async fn sniff_receipt(
    json: bool,
    request: &HardwareSniffReceiptRequest<'_>,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = build_hardware_sniff_receipt(request)?;
    write_json_receipt(json_out, &receipt)?;
    print_sniff_receipt(json, json_out, &receipt)
}

async fn sniff_notes_template(
    json: bool,
    plan: &Path,
    hardware_doctor: Option<&Path>,
    out: &Path,
    json_out: Option<&Path>,
) -> Result<()> {
    let stored_plan = read_and_validate_sniff_plan(plan)?;
    let capture_hints = sniff_notes_capture_hints_from_hardware_doctor(hardware_doctor)?;
    let template = render_sniff_operator_notes_template(plan, &stored_plan, capture_hints.as_ref());
    write_text_file(out, &template)?;
    let receipt = HardwareSniffNotesTemplateReceipt {
        schema_version: 1,
        success: true,
        command: "wheelctl hardware sniff-notes-template",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        plan_path: required_path_display(plan, "plan")?,
        hardware_doctor_path: hardware_doctor
            .map(|path| required_path_display(path, "hardware-doctor"))
            .transpose()?,
        out_path: required_path_display(out, "out")?,
        scenario: stored_plan.scenario,
        operator: stored_plan.operator,
        device_note: stored_plan.device_note,
        capture_hints,
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
    };
    write_json_receipt(json_out, &receipt)?;
    print_sniff_notes_template(json, out, json_out, &receipt)
}

async fn sniff_capture(
    json: bool,
    request: &HardwareSniffCaptureRequest<'_>,
    json_out: Option<&Path>,
) -> Result<()> {
    let receipt = run_hardware_sniff_capture(request)?;
    write_json_receipt(json_out, &receipt)?;
    print_sniff_capture(json, json_out, &receipt)
}

async fn sniff_summary(
    json: bool,
    request: &HardwareSniffSummaryRequest<'_>,
    json_out: Option<&Path>,
    md_out: Option<&Path>,
) -> Result<()> {
    let summary = build_hardware_sniff_summary(request)?;
    write_json_receipt(json_out, &summary)?;
    if let Some(path) = md_out {
        write_text_file(path, &render_sniff_summary_markdown(&summary))?;
    }
    print_sniff_summary(json, json_out, md_out, &summary)
}

async fn sniff_bundle(
    json: bool,
    request: &HardwareSniffBundleRequest<'_>,
    json_out: Option<&Path>,
) -> Result<()> {
    let manifest = build_hardware_sniff_bundle(request)?;
    write_json_receipt(json_out, &manifest)?;
    print_sniff_bundle(json, request.out, json_out, &manifest)
}

fn build_hardware_sniff_plan(
    request: &HardwareSniffPlanRequest<'_>,
) -> Result<HardwareSniffPlanArtifact> {
    let family = required_text(request.family, "family")?;
    let lane = required_path_display(request.lane, "lane")?;
    let operator = required_text(request.operator, "operator")?;
    let device_note = required_text(request.device_note, "device-note")?;
    let platform_hint = request
        .platform_hint
        .unwrap_or_else(current_sniff_platform_hint);
    let capture_tools = normalized_sniff_capture_tools(request.capture_tools, platform_hint);
    Ok(HardwareSniffPlanArtifact {
        schema_version: 1,
        success: true,
        command: "wheelctl hardware sniff-plan",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        family,
        scenario: request.scenario.as_str().to_string(),
        lane,
        operator,
        device_note,
        capture_kind: SNIFF_CAPTURE_KIND,
        capture_tools,
        platform_hint: platform_hint.as_str().to_string(),
        allowed_actions: SNIFF_ALLOWED_ACTIONS.to_vec(),
        forbidden_actions: SNIFF_FORBIDDEN_ACTIONS.to_vec(),
        pre_capture_checklist: SNIFF_PRE_CAPTURE_CHECKLIST
            .iter()
            .copied()
            .map(str::to_string)
            .collect(),
        post_capture_checklist: SNIFF_POST_CAPTURE_CHECKLIST
            .iter()
            .copied()
            .map(str::to_string)
            .collect(),
        operator_notes_required: sniff_operator_notes_required(request.scenario),
        raw_pcap_commit_default: false,
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        external_app_may_have_sent_output: true,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
        notes: vec![
            "passive sniffing observes host-side USB traffic only".to_string(),
            "this plan is protocol research/support evidence, not OpenRacing hardware output"
                .to_string(),
            "sniff artifacts cannot satisfy native response, native visible, smoke, or release gates"
                .to_string(),
        ],
    })
}

fn build_hardware_sniff_receipt(
    request: &HardwareSniffReceiptRequest<'_>,
) -> Result<HardwareSniffReceiptArtifact> {
    let plan = read_and_validate_sniff_plan(request.plan)?;
    let pcapng_path = request.pcapng.ok_or_else(|| {
        anyhow::anyhow!(
            "missing required pcapng capture: pass --pcapng <path-to-capture.pcapng> after saving the passive USB observation"
        )
    })?;
    let pcapng_path_text = required_pcapng_path_display(pcapng_path)?;
    let (pcapng_sha256, pcapng_size_bytes) = hash_existing_pcapng(pcapng_path)?;
    let operator = request.operator.map_or_else(
        || required_text(&plan.operator, "operator"),
        |value| required_text(value, "operator"),
    )?;
    let scenario = request.scenario.map_or_else(
        || plan.scenario.clone(),
        |scenario| scenario.as_str().to_string(),
    );
    validate_sniff_scenario(&scenario)?;
    let device_note = request.device_note.map_or_else(
        || required_text(&plan.device_note, "device-note"),
        |value| required_text(value, "device-note"),
    )?;
    let app = required_text(request.app, "app")?;
    let evidence = required_text(request.evidence, "evidence")?;

    Ok(HardwareSniffReceiptArtifact {
        schema_version: 1,
        success: true,
        command: "wheelctl hardware sniff-receipt",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        plan_path: required_path_display(request.plan, "plan")?,
        pcapng_path: pcapng_path_text,
        pcapng_sha256,
        pcapng_size_bytes,
        operator,
        app,
        scenario,
        device_note,
        evidence,
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        openracing_hid_device_opened: false,
        openracing_ffb_writes: false,
        openracing_output_reports: false,
        openracing_feature_reports: false,
        openracing_serial_config_commands: false,
        openracing_firmware_or_dfu_commands: false,
        external_app_observed: true,
        external_app_may_have_sent_output: true,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
    })
}

fn run_hardware_sniff_capture(
    request: &HardwareSniffCaptureRequest<'_>,
) -> Result<HardwareSniffCaptureReceipt> {
    validate_hardware_sniff_capture_request(request)?;
    if request.out.exists() && request.overwrite {
        fs::remove_file(request.out)
            .with_context(|| format!("failed to remove existing '{}'", request.out.display()))?;
    }
    if let Some(parent) = request.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let (stdout_path, stderr_path) = sniff_capture_tool_log_paths(request.out);
    let stdout = File::create(&stdout_path)
        .with_context(|| format!("failed to create '{}'", stdout_path.display()))?;
    let stderr = File::create(&stderr_path)
        .with_context(|| format!("failed to create '{}'", stderr_path.display()))?;

    let mut child = Command::new(request.usbpcapcmd)
        .args(sniff_capture_usbpcapcmd_args(request))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| {
            format!(
                "failed to start USBPcapCMD capture tool '{}'",
                request.usbpcapcmd.display()
            )
        })?;
    let process_id = child.id();
    let started_at_utc = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let deadline = Instant::now() + Duration::from_millis(request.duration_ms);

    let (exit_status, terminated_after_duration) = loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to poll USBPcapCMD capture process")?
        {
            break (Some(status.to_string()), false);
        }
        let now = Instant::now();
        if now >= deadline {
            child
                .kill()
                .context("failed to stop USBPcapCMD capture process after duration elapsed")?;
            let status = child
                .wait()
                .context("failed to wait for stopped USBPcapCMD capture process")?;
            break (Some(status.to_string()), true);
        }
        let remaining = deadline.saturating_duration_since(now);
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    };

    let completed_at_utc = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let pcapng = sniff_capture_output_metadata(request.out)?;
    let stdout_size_bytes = sniff_capture_tool_log_size(&stdout_path)?;
    let stderr_size_bytes = sniff_capture_tool_log_size(&stderr_path)?;
    build_hardware_sniff_capture_receipt(
        request,
        HardwareSniffCaptureOutcome {
            process_started: true,
            process_id: Some(process_id),
            started_at_utc,
            completed_at_utc,
            exit_status,
            terminated_after_duration,
            pcapng_exists: pcapng.exists,
            pcapng_size_bytes: pcapng.size_bytes,
            pcapng_sha256: pcapng.sha256,
            stdout_path,
            stdout_size_bytes,
            stderr_path,
            stderr_size_bytes,
        },
    )
}

fn validate_hardware_sniff_capture_request(
    request: &HardwareSniffCaptureRequest<'_>,
) -> Result<()> {
    if !request.confirm_external_passive_capture {
        anyhow::bail!("refusing to run passive capture without --confirm-external-passive-capture");
    }
    if request.duration_ms == 0 || request.duration_ms > 600_000 {
        anyhow::bail!("--duration-ms must be in 1..=600000");
    }
    if request.usbpcap_interface.trim().is_empty() {
        anyhow::bail!("--usbpcap-interface must not be blank");
    }
    if request.devices.trim().is_empty() {
        anyhow::bail!("--devices must not be blank");
    }
    if !request.usbpcapcmd.is_file() {
        anyhow::bail!(
            "--usbpcapcmd '{}' does not exist or is not a file",
            request.usbpcapcmd.display()
        );
    }
    if request.out.extension().and_then(|ext| ext.to_str()) != Some("pcapng") {
        anyhow::bail!("--out must end in .pcapng");
    }
    if path_contains_ci_hardware(request.out) {
        anyhow::bail!("--out must stay in local scratch storage, not ci/hardware/**");
    }
    if request.out.exists() && !request.overwrite {
        anyhow::bail!(
            "--out '{}' already exists; pass --overwrite to replace a local scratch capture",
            request.out.display()
        );
    }
    Ok(())
}

fn sniff_capture_usbpcapcmd_args(request: &HardwareSniffCaptureRequest<'_>) -> Vec<String> {
    vec![
        "-d".to_string(),
        request.usbpcap_interface.to_string(),
        "--devices".to_string(),
        request.devices.to_string(),
        "--inject-descriptors".to_string(),
        "-o".to_string(),
        request.out.display().to_string(),
    ]
}

fn sniff_capture_tool_log_paths(out: &Path) -> (PathBuf, PathBuf) {
    let parent = out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(""));
    let stem = out
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("capture");
    (
        parent.join(format!("{stem}.usbpcapcmd.stdout.txt")),
        parent.join(format!("{stem}.usbpcapcmd.stderr.txt")),
    )
}

fn sniff_capture_tool_log_size(path: &Path) -> Result<u64> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect USBPcapCMD log '{}'", path.display()))?;
    Ok(metadata.len())
}

fn path_contains_ci_hardware(path: &Path) -> bool {
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|text| text.to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    parts
        .windows(2)
        .any(|window| window[0] == "ci" && window[1] == "hardware")
}

fn sniff_capture_output_metadata(path: &Path) -> Result<HardwareSniffCaptureOutputMetadata> {
    if !path.exists() {
        return Ok(HardwareSniffCaptureOutputMetadata {
            exists: false,
            size_bytes: 0,
            sha256: None,
        });
    }
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect pcapng capture '{}'", path.display()))?;
    let size_bytes = metadata.len();
    let sha256 = if size_bytes > 0 {
        Some(hash_existing_file(path)?)
    } else {
        None
    };
    Ok(HardwareSniffCaptureOutputMetadata {
        exists: true,
        size_bytes,
        sha256,
    })
}

fn build_hardware_sniff_capture_receipt(
    request: &HardwareSniffCaptureRequest<'_>,
    outcome: HardwareSniffCaptureOutcome,
) -> Result<HardwareSniffCaptureReceipt> {
    Ok(HardwareSniffCaptureReceipt {
        schema_version: 1,
        success: outcome.process_started && outcome.pcapng_exists && outcome.pcapng_size_bytes > 0,
        command: "wheelctl hardware sniff-capture",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        capture_tool: "USBPcapCMD",
        usbpcapcmd_path: required_path_display(request.usbpcapcmd, "usbpcapcmd")?,
        usbpcap_interface: request.usbpcap_interface.to_string(),
        devices: request.devices.to_string(),
        duration_ms: request.duration_ms,
        out_path: required_path_display(request.out, "out")?,
        overwrite: request.overwrite,
        process_started: outcome.process_started,
        process_id: outcome.process_id,
        started_at_utc: outcome.started_at_utc,
        completed_at_utc: outcome.completed_at_utc,
        exit_status: outcome.exit_status,
        terminated_after_duration: outcome.terminated_after_duration,
        pcapng_exists: outcome.pcapng_exists,
        pcapng_size_bytes: outcome.pcapng_size_bytes,
        pcapng_sha256: outcome.pcapng_sha256,
        usbpcapcmd_stdout_path: required_path_display(&outcome.stdout_path, "stdout")?,
        usbpcapcmd_stdout_size_bytes: outcome.stdout_size_bytes,
        usbpcapcmd_stderr_path: required_path_display(&outcome.stderr_path, "stderr")?,
        usbpcapcmd_stderr_size_bytes: outcome.stderr_size_bytes,
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        openracing_hid_device_opened: false,
        openracing_ffb_writes: false,
        openracing_output_reports: false,
        openracing_feature_reports: false,
        openracing_serial_config_commands: false,
        openracing_firmware_or_dfu_commands: false,
        external_capture_tool_invoked: true,
        external_app_may_have_sent_output: true,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
        next_allowed_actions: vec![
            "fill operator notes for the observed scenario".to_string(),
            "run wheelctl hardware sniff-receipt with the saved pcapng".to_string(),
            "run wheelctl hardware sniff-summary with the saved pcapng".to_string(),
            "run wheelctl hardware sniff-bundle only after notes, receipt, and summary exist"
                .to_string(),
        ],
        notes: vec![
            "sniff-capture launches only the external passive USBPcapCMD capture tool".to_string(),
            "OpenRacing does not open HID, serial, feature, output, firmware, or DFU paths for this command".to_string(),
            "a capture receipt is not a sniff receipt, sniff summary, native-control proof, or readiness claim".to_string(),
            "raw pcapng remains local scratch evidence unless separately reviewed for bundling or commit".to_string(),
        ],
    })
}

fn build_hardware_sniff_summary(
    request: &HardwareSniffSummaryRequest<'_>,
) -> Result<HardwareSniffSummaryArtifact> {
    let tshark_path = find_tshark_path();
    build_hardware_sniff_summary_with_tshark_path(request, tshark_path.as_deref())
}

fn build_hardware_sniff_summary_with_tshark_path(
    request: &HardwareSniffSummaryRequest<'_>,
    tshark_path: Option<&Path>,
) -> Result<HardwareSniffSummaryArtifact> {
    let config = validate_sniff_summary_request(request)?;
    let _pcapng_path_text = required_pcapng_path_display(request.pcapng)?;
    let (pcapng_sha256, _) = hash_existing_pcapng(request.pcapng)?;
    let Some(tshark_path) = tshark_path else {
        anyhow::bail!(
            "tshark was not found; install Wireshark/tshark or set WIRESHARK_TSHARK to the tshark executable before running wheelctl hardware sniff-summary"
        );
    };
    let tshark_version = run_tshark_version(tshark_path)?;
    let tshark_json = run_tshark_summary_json(tshark_path, request.pcapng)?;
    build_hardware_sniff_summary_from_tshark_json(
        config,
        pcapng_sha256,
        true,
        Some(tshark_version),
        &tshark_json,
    )
}

fn validate_sniff_summary_request(
    request: &HardwareSniffSummaryRequest<'_>,
) -> Result<HardwareSniffSummaryConfig> {
    let vendor_id = request
        .vendor
        .map(|value| parse_sniff_hex16_filter(value, "vendor"))
        .transpose()?;
    let product_id = request
        .product
        .map(|value| parse_sniff_hex16_filter(value, "product"))
        .transpose()?;
    if product_id.is_some() && vendor_id.is_none() {
        anyhow::bail!(
            "sniff product filter is ambiguous without --vendor; pass both --vendor 0x.... and --product 0x...."
        );
    }

    let max_samples_per_report = request
        .max_samples_per_report
        .unwrap_or(DEFAULT_SNIFF_MAX_SAMPLES_PER_REPORT);
    if !(1..=MAX_SNIFF_MAX_SAMPLES_PER_REPORT).contains(&max_samples_per_report) {
        anyhow::bail!(
            "sniff max-samples-per-report must be between 1 and {MAX_SNIFF_MAX_SAMPLES_PER_REPORT}"
        );
    }

    Ok(HardwareSniffSummaryConfig {
        filters: HardwareSniffSummaryFilters {
            vendor_id,
            product_id,
            interface_number: request.interface,
        },
        include_payload_samples: request.include_payload_samples,
        max_samples_per_report,
    })
}

fn parse_sniff_hex16_filter(value: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    else {
        anyhow::bail!("sniff {field} filter must use 0x0000 format: {value}");
    };
    if hex.len() != 4 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        anyhow::bail!("sniff {field} filter must use 0x0000 format: {value}");
    }
    Ok(format!("0x{}", hex.to_ascii_uppercase()))
}

fn run_tshark_version(tshark_path: &Path) -> Result<String> {
    let output = Command::new(tshark_path)
        .arg("-v")
        .output()
        .with_context(|| {
            format!(
                "failed to run tshark -v from '{}'; check WIRESHARK_TSHARK or install Wireshark/tshark",
                tshark_path.display()
            )
        })?;
    if !output.status.success() {
        anyhow::bail!(
            "tshark -v failed from '{}': {}",
            tshark_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("tshark -v returned no version text"))
}

fn run_tshark_summary_json(tshark_path: &Path, pcapng: &Path) -> Result<String> {
    let mut command = Command::new(tshark_path);
    command.arg("-r").arg(pcapng).args(["-T", "json"]);

    let output = command.output().with_context(|| {
        format!(
            "failed to run tshark against '{}'; check that tshark can read the pcapng",
            pcapng.display()
        )
    })?;
    if !output.status.success() {
        anyhow::bail!(
            "tshark failed while reading '{}': {}",
            pcapng.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("tshark JSON was not UTF-8")?;
    if stdout.trim().is_empty() {
        anyhow::bail!("tshark produced no JSON for '{}'", pcapng.display());
    }
    Ok(stdout)
}

fn build_hardware_sniff_summary_from_tshark_json(
    config: HardwareSniffSummaryConfig,
    pcapng_sha256: String,
    tshark_present: bool,
    tshark_version: Option<String>,
    tshark_json: &str,
) -> Result<HardwareSniffSummaryArtifact> {
    let packets = parse_tshark_usb_packets(tshark_json)?;
    let packets = enrich_tshark_usb_packets(packets);
    let matched_packets: Vec<TsharkUsbPacket> = packets
        .into_iter()
        .filter(|packet| sniff_packet_matches_filters(packet, &config.filters))
        .collect();

    let mut transfer_summary = HardwareSniffUsbTransferSummary::default();
    let mut host_to_device_payload_coverage = HardwareSniffHostToDevicePayloadCoverage::default();
    let mut usbcom_serial_frame_summary = HardwareSniffUsbComSerialFrameSummaryBuilder::default();
    let mut devices: BTreeMap<(String, String), HardwareSniffObservedDeviceBuilder> =
        BTreeMap::new();
    let mut reports: BTreeMap<(SniffUsbDirection, u8), HardwareSniffObservedReportBuilder> =
        BTreeMap::new();
    let mut descriptor_candidates: BTreeMap<
        (SniffDescriptorKind, Option<u16>, String),
        HardwareSniffDescriptorCandidate,
    > = BTreeMap::new();

    for packet in &matched_packets {
        if matches!(packet.direction, Some(SniffUsbDirection::HostToDevice)) {
            transfer_summary.host_to_device += 1;
            host_to_device_payload_coverage.observe(packet);
            if let Some(payload) = &packet.payload {
                usbcom_serial_frame_summary.observe_payload(payload);
            }
        }
        if matches!(packet.direction, Some(SniffUsbDirection::DeviceToHost)) {
            transfer_summary.device_to_host += 1;
        }
        match packet.transfer_type {
            Some(SniffUsbTransferType::Control) => transfer_summary.control += 1,
            Some(SniffUsbTransferType::Interrupt) => transfer_summary.interrupt += 1,
            Some(SniffUsbTransferType::Other) | None => {}
        }

        if let (Some(vendor_id), Some(product_id)) = (&packet.vendor_id, &packet.product_id) {
            let device = devices
                .entry((vendor_id.clone(), product_id.clone()))
                .or_insert_with(|| HardwareSniffObservedDeviceBuilder {
                    vendor_id: vendor_id.clone(),
                    product_id: product_id.clone(),
                    interfaces: BTreeSet::new(),
                    endpoints: BTreeSet::new(),
                });
            if let Some(interface_number) = packet.interface_number {
                device.interfaces.insert(interface_number);
            }
            if let Some(endpoint_address) = packet.endpoint_address {
                device.endpoints.insert(endpoint_address);
            }
        }

        if let (Some(direction), Some(report_id)) = (packet.direction, packet.report_id) {
            let report = reports.entry((direction, report_id)).or_insert_with(|| {
                HardwareSniffObservedReportBuilder {
                    direction,
                    report_id,
                    count: 0,
                    payload_sha256_examples: Vec::new(),
                    payload_hex_samples: Vec::new(),
                }
            });
            report.count += 1;
            if let Some(payload) = &packet.payload
                && report.payload_sha256_examples.len() < config.max_samples_per_report
            {
                report.payload_sha256_examples.push(sha256_hex(payload));
                if config.include_payload_samples {
                    report
                        .payload_hex_samples
                        .push(bytes_to_hex_sample(payload));
                }
            }
        }

        if let (Some(kind), Some(payload)) = (packet.descriptor_kind, &packet.payload) {
            let payload_sha256 = sha256_hex(payload);
            descriptor_candidates
                .entry((kind, packet.interface_number, payload_sha256.clone()))
                .or_insert_with(|| HardwareSniffDescriptorCandidate {
                    kind: kind.as_str().to_string(),
                    interface_number: packet.interface_number,
                    payload_sha256,
                    payload_len: payload.len(),
                    extractable: true,
                });
        }
    }

    let matched_count = matched_packets.len();
    let reason = (matched_count == 0).then(|| {
        "no USB packets matched the supplied pcapng and vendor/product/interface filters"
            .to_string()
    });
    let mut notes = vec![
        "passive sniff summary is protocol research/support evidence only".to_string(),
        "OpenRacing opened no HID device and sent no output, feature, serial, firmware, or DFU commands"
            .to_string(),
        "sniff artifacts cannot satisfy native response, native visible, smoke, or release gates"
            .to_string(),
    ];
    if !config.include_payload_samples {
        notes.push("payload examples are represented as sha256 hashes only".to_string());
    }
    if let Some(reason) = &reason {
        notes.push(reason.clone());
    }

    let observed_reports: Vec<_> = reports
        .into_values()
        .map(|report| report.build(config.include_payload_samples))
        .collect();
    let report_classification_summary = summarize_sniff_report_classifications(
        &observed_reports,
        transfer_summary.host_to_device,
        host_to_device_payload_coverage,
        usbcom_serial_frame_summary.build(),
    );

    Ok(HardwareSniffSummaryArtifact {
        schema_version: 1,
        success: matched_count > 0,
        command: "wheelctl hardware sniff-summary",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        pcapng_sha256,
        reason,
        tool: HardwareSniffSummaryTool {
            tshark_present,
            tshark_version,
        },
        filters: config.filters,
        matched_packets: matched_count,
        usb_transfer_summary: transfer_summary,
        observed_devices: devices
            .into_values()
            .map(HardwareSniffObservedDeviceBuilder::build)
            .collect(),
        observed_reports,
        report_classification_summary,
        descriptor_candidates: descriptor_candidates.into_values().collect(),
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        external_app_may_have_sent_output: true,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
        notes,
    })
}

fn build_hardware_sniff_bundle(
    request: &HardwareSniffBundleRequest<'_>,
) -> Result<HardwareSniffBundleManifest> {
    let plan = read_and_validate_sniff_plan(request.plan)?;
    let receipt = read_and_validate_sniff_receipt(request.receipt)?;
    let summary = read_and_validate_sniff_summary(request.summary)?;
    let plan_path_text = required_path_display(request.plan, "plan")?;
    if receipt.plan_path != plan_path_text {
        anyhow::bail!(
            "sniff receipt '{}' was created for plan '{}' but bundle supplied plan '{}'",
            request.receipt.display(),
            receipt.plan_path,
            plan_path_text
        );
    }
    if summary.pcapng_sha256 != receipt.pcapng_sha256 {
        anyhow::bail!(
            "sniff summary '{}' pcapng_sha256 does not match sniff receipt '{}'",
            request.summary.display(),
            request.receipt.display()
        );
    }
    let operator_notes_receipt_bytes = if let Some(operator_notes_receipt) =
        request.operator_notes_receipt
    {
        let notes_receipt = read_and_validate_sniff_notes_template_receipt(operator_notes_receipt)?;
        if notes_receipt.plan_path != plan_path_text {
            anyhow::bail!(
                "sniff notes template receipt '{}' was created for plan '{}' but bundle supplied plan '{}'",
                operator_notes_receipt.display(),
                notes_receipt.plan_path,
                plan_path_text
            );
        }
        let operator_notes_path_text =
            required_path_display(request.operator_notes, "operator notes")?;
        if notes_receipt.out_path != operator_notes_path_text {
            anyhow::bail!(
                "sniff notes template receipt '{}' was created for operator notes '{}' but bundle supplied operator notes '{}'",
                operator_notes_receipt.display(),
                notes_receipt.out_path,
                operator_notes_path_text
            );
        }
        if notes_receipt.scenario != plan.scenario {
            anyhow::bail!(
                "sniff notes template receipt '{}' scenario '{}' does not match sniff plan scenario '{}'",
                operator_notes_receipt.display(),
                notes_receipt.scenario,
                plan.scenario
            );
        }
        if notes_receipt.operator != plan.operator {
            anyhow::bail!(
                "sniff notes template receipt '{}' operator '{}' does not match sniff plan operator '{}'",
                operator_notes_receipt.display(),
                notes_receipt.operator,
                plan.operator
            );
        }
        if notes_receipt.device_note != plan.device_note {
            anyhow::bail!(
                "sniff notes template receipt '{}' device_note does not match sniff plan device_note",
                operator_notes_receipt.display()
            );
        }
        Some(read_required_artifact(
            operator_notes_receipt,
            "operator notes receipt",
        )?)
    } else {
        None
    };

    let operator_notes_bytes =
        read_and_validate_sniff_bundle_operator_notes(request.operator_notes, &plan)?;

    let mut entries = vec![
        SniffBundleZipEntry::bytes(
            sniff_bundle_path("README.md"),
            render_sniff_bundle_readme(request.include_pcapng.is_some()).into_bytes(),
        ),
        SniffBundleZipEntry::bytes(
            sniff_bundle_path("sniff-plan.json"),
            read_required_artifact(request.plan, "sniff plan")?,
        ),
        SniffBundleZipEntry::bytes(
            sniff_bundle_path("sniff-receipt.json"),
            read_required_artifact(request.receipt, "sniff receipt")?,
        ),
        SniffBundleZipEntry::bytes(
            sniff_bundle_path("sniff-summary.json"),
            read_required_artifact(request.summary, "sniff summary")?,
        ),
        SniffBundleZipEntry::bytes(sniff_bundle_path("operator-notes.md"), operator_notes_bytes),
    ];
    if let Some(bytes) = operator_notes_receipt_bytes {
        entries.push(SniffBundleZipEntry::bytes(
            sniff_bundle_path("sniff-notes-template-receipt.json"),
            bytes,
        ));
    }
    entries.push(SniffBundleZipEntry::bytes(
        sniff_bundle_path("pcapng-sha256.txt"),
        format!("{}\n", receipt.pcapng_sha256).into_bytes(),
    ));

    if let Some(pcapng) = request.include_pcapng {
        let pcapng_path_text = required_pcapng_path_display(pcapng)?;
        if receipt.pcapng_path != pcapng_path_text {
            anyhow::bail!(
                "raw pcapng '{}' does not match sniff receipt pcapng_path '{}'",
                pcapng.display(),
                receipt.pcapng_path
            );
        }
        let (raw_sha256, raw_size_bytes) = hash_existing_pcapng(pcapng)?;
        if raw_sha256 != receipt.pcapng_sha256 {
            anyhow::bail!(
                "raw pcapng '{}' sha256 {raw_sha256} does not match sniff receipt pcapng_sha256 {}",
                pcapng.display(),
                receipt.pcapng_sha256
            );
        }
        if raw_size_bytes != receipt.pcapng_size_bytes {
            anyhow::bail!(
                "raw pcapng '{}' size {raw_size_bytes} does not match sniff receipt pcapng_size_bytes {}",
                pcapng.display(),
                receipt.pcapng_size_bytes
            );
        }
        entries.push(SniffBundleZipEntry::file(
            sniff_bundle_path("capture.pcapng"),
            pcapng.to_path_buf(),
            raw_sha256,
        ));
    }

    let artifacts = entries
        .iter()
        .map(|entry| HardwareSniffBundleArtifactHash {
            path: entry.archive_path.clone(),
            sha256: entry.sha256.clone(),
        })
        .collect();
    let manifest = HardwareSniffBundleManifest {
        schema_version: 1,
        success: true,
        command: "wheelctl hardware sniff-bundle",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        bundle_kind: SNIFF_BUNDLE_KIND,
        includes_raw_pcapng: request.include_pcapng.is_some(),
        artifacts,
        evidence_status: SNIFF_EVIDENCE_STATUS,
        native_control_evidence: false,
        openracing_hardware_output: false,
        external_app_may_have_sent_output: true,
        satisfies_native_response_ready: false,
        satisfies_native_visible_ready: false,
        satisfies_smoke_ready: false,
        satisfies_release_ready: false,
        readiness_claims: HardwareSniffReadinessClaims::none(),
    };

    write_sniff_bundle_zip(request.out, &entries, &manifest)?;
    Ok(manifest)
}

fn render_sniff_bundle_readme(includes_raw_pcapng: bool) -> String {
    let raw_capture_line = if includes_raw_pcapng {
        "- capture.pcapng is included because --include-pcapng was supplied."
    } else {
        "- capture.pcapng is not included; pcapng-sha256.txt records the receipt hash only."
    };
    format!(
        "# OpenRacing Passive USB Sniff Bundle\n\n\
This bundle packages passive USB sniff artifacts for protocol research and support review.\n\n\
Non-claiming invariants:\n\
- OpenRacing native control evidence is false.\n\
- OpenRacing hardware output is false.\n\
- Native response, native visible, smoke, and release readiness claims are false.\n\
- External applications may have sent output during the observed session.\n\
{raw_capture_line}\n"
    )
}

fn write_sniff_bundle_zip(
    out: &Path,
    entries: &[SniffBundleZipEntry],
    manifest: &HardwareSniffBundleManifest,
) -> Result<()> {
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let file = File::create(out)
        .with_context(|| format!("failed to create sniff bundle '{}'", out.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));

    for entry in entries {
        zip.start_file(&entry.archive_path, options)
            .with_context(|| format!("failed to start ZIP entry '{}'", entry.archive_path))?;
        match &entry.source {
            SniffBundleZipSource::Bytes(bytes) => {
                zip.write_all(bytes).with_context(|| {
                    format!("failed to write ZIP entry '{}'", entry.archive_path)
                })?;
            }
            SniffBundleZipSource::File(path) => {
                let mut file = File::open(path)
                    .with_context(|| format!("failed to open '{}'", path.display()))?;
                io::copy(&mut file, &mut zip).with_context(|| {
                    format!("failed to copy '{}' into sniff bundle", path.display())
                })?;
            }
        }
    }

    let manifest_json =
        serde_json::to_vec_pretty(manifest).context("failed to serialize sniff bundle manifest")?;
    let manifest_path = sniff_bundle_path("sniff-bundle-manifest.json");
    zip.start_file(&manifest_path, options)
        .with_context(|| format!("failed to start ZIP entry '{manifest_path}'"))?;
    zip.write_all(&manifest_json)
        .context("failed to write sniff bundle manifest")?;
    zip.finish()
        .context("failed to finish sniff bundle ZIP archive")?;
    Ok(())
}

fn sniff_bundle_path(file_name: &str) -> String {
    format!("{SNIFF_BUNDLE_ROOT}/{file_name}")
}

fn read_required_artifact(path: &Path, label: &str) -> Result<Vec<u8>> {
    if !path.is_file() {
        anyhow::bail!("{label} artifact not found: {}", path.display());
    }
    fs::read(path).with_context(|| format!("failed to read {label} artifact '{}'", path.display()))
}

fn read_and_validate_sniff_bundle_operator_notes(
    path: &Path,
    plan: &StoredHardwareSniffPlan,
) -> Result<Vec<u8>> {
    let bytes = read_required_artifact(path, "operator notes")?;
    validate_sniff_bundle_operator_notes(path, plan, &bytes)?;
    Ok(bytes)
}

fn validate_sniff_bundle_operator_notes(
    path: &Path,
    plan: &StoredHardwareSniffPlan,
    bytes: &[u8],
) -> Result<()> {
    let required_fields = bundle_operator_note_value_fields(plan);
    if required_fields.is_empty() {
        return Ok(());
    }

    let notes = std::str::from_utf8(bytes)
        .with_context(|| format!("operator notes '{}' are not valid UTF-8", path.display()))?;
    for field in required_fields {
        if operator_note_field_value(notes, field).is_none() {
            anyhow::bail!(
                "operator notes '{}' are missing completed required field '{}' for scenario '{}'",
                path.display(),
                field,
                plan.scenario
            );
        }
    }
    Ok(())
}

fn bundle_operator_note_value_fields(plan: &StoredHardwareSniffPlan) -> Vec<&'static str> {
    if plan.scenario == "pit-house-setting-change" {
        PIT_HOUSE_SETTING_CHANGE_OPERATOR_NOTES_REQUIRED.to_vec()
    } else {
        Vec::new()
    }
}

fn operator_note_field_value<'a>(notes: &'a str, field: &str) -> Option<&'a str> {
    notes.lines().find_map(|line| {
        let line = line.trim();
        let line = line
            .strip_prefix("- [ ] ")
            .or_else(|| line.strip_prefix("- [x] "))
            .or_else(|| line.strip_prefix("- [X] "))
            .unwrap_or(line);
        let rest = line.strip_prefix(field)?.trim_start();
        let value = rest.strip_prefix(':')?.trim();
        (!value.is_empty()).then_some(value)
    })
}

fn parse_tshark_usb_packets(tshark_json: &str) -> Result<Vec<TsharkUsbPacket>> {
    let value: serde_json::Value =
        serde_json::from_str(tshark_json).context("failed to parse tshark JSON output")?;
    let Some(packets) = value.as_array() else {
        anyhow::bail!("tshark JSON output was not a packet array");
    };

    packets
        .iter()
        .enumerate()
        .map(|(index, packet)| parse_tshark_usb_packet(packet, index.saturating_add(1)))
        .collect::<Result<Vec<_>>>()
}

fn parse_tshark_usb_packet(
    packet: &serde_json::Value,
    packet_ordinal: usize,
) -> Result<TsharkUsbPacket> {
    let layers = packet
        .pointer("/_source/layers")
        .or_else(|| packet.get("layers"))
        .unwrap_or(packet);
    let mut fields = BTreeMap::new();
    collect_tshark_fields(layers, &mut fields);

    let endpoint_address = first_u8_field(
        &fields,
        &[
            "usb.endpoint_address",
            "usb.endpoint_address.endpoint",
            "usb.endpoint_number",
        ],
    );
    let direction = parse_packet_direction(&fields, endpoint_address);
    let payload = first_payload_field(&fields).and_then(|value| parse_payload_hex(&value));
    let report_id = first_u8_field(&fields, &["usbhid.report_id", "hid.report_id"])
        .or_else(|| payload.as_ref().and_then(|bytes| bytes.first().copied()));
    let data_len = first_usize_field(&fields, &["usb.data_len"]);

    let control_setup_stage = tshark_usb_setup_fields_present(&fields);
    let transfer_type = if control_setup_stage {
        Some(SniffUsbTransferType::Control)
    } else {
        parse_packet_transfer_type(&fields, endpoint_address)
    };

    Ok(TsharkUsbPacket {
        packet_ordinal,
        frame_number: first_u64_field(&fields, &["frame.number"]),
        device_key: packet_device_key(&fields),
        vendor_id: first_hex16_field(
            &fields,
            &[
                "usb.idVendor",
                "usb.device_descriptor.idVendor",
                "usb.vendor_id",
            ],
        ),
        product_id: first_hex16_field(
            &fields,
            &[
                "usb.idProduct",
                "usb.device_descriptor.idProduct",
                "usb.product_id",
            ],
        ),
        interface_number: first_u16_field(
            &fields,
            &[
                "usb.interface_number",
                "usb.bInterfaceNumber",
                "usb.interface.descriptor.bInterfaceNumber",
            ],
        ),
        endpoint_address,
        direction,
        transfer_type,
        data_len,
        report_id,
        payload,
        control_setup_stage,
        descriptor_kind: parse_packet_descriptor_kind(&fields),
    })
}

fn collect_tshark_fields(value: &serde_json::Value, fields: &mut BTreeMap<String, Vec<String>>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key.contains('.') {
                    let values = leaf_strings(child);
                    if !values.is_empty() {
                        fields.entry(key.clone()).or_default().extend(values);
                    }
                }
                collect_tshark_fields(child, fields);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_tshark_fields(child, fields);
            }
        }
        _ => {}
    }
}

fn leaf_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(value) => vec![value.clone()],
        serde_json::Value::Number(value) => vec![value.to_string()],
        serde_json::Value::Bool(value) => vec![value.to_string()],
        serde_json::Value::Array(values) => values.iter().flat_map(leaf_strings).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(leaf_strings).collect(),
        serde_json::Value::Null => Vec::new(),
    }
}

fn enrich_tshark_usb_packets(mut packets: Vec<TsharkUsbPacket>) -> Vec<TsharkUsbPacket> {
    let mut device_ids_by_key = BTreeMap::new();
    let mut interfaces_by_key = BTreeMap::new();
    for packet in &packets {
        if let (Some(device_key), Some(vendor_id), Some(product_id)) =
            (&packet.device_key, &packet.vendor_id, &packet.product_id)
        {
            device_ids_by_key.insert(device_key.clone(), (vendor_id.clone(), product_id.clone()));
        }
        if let (Some(device_key), Some(interface_number)) =
            (&packet.device_key, packet.interface_number)
        {
            interfaces_by_key.insert(device_key.clone(), interface_number);
        }
    }

    for packet in &mut packets {
        if let Some(device_key) = &packet.device_key {
            if (packet.vendor_id.is_none() || packet.product_id.is_none())
                && let Some((vendor_id, product_id)) = device_ids_by_key.get(device_key)
            {
                packet.vendor_id = packet.vendor_id.clone().or_else(|| Some(vendor_id.clone()));
                packet.product_id = packet
                    .product_id
                    .clone()
                    .or_else(|| Some(product_id.clone()));
            }
            if packet.interface_number.is_none()
                && let Some(interface_number) = interfaces_by_key.get(device_key)
            {
                packet.interface_number = Some(*interface_number);
            }
        }
    }

    packets
}

fn sniff_packet_matches_filters(
    packet: &TsharkUsbPacket,
    filters: &HardwareSniffSummaryFilters,
) -> bool {
    if let Some(vendor_id) = &filters.vendor_id
        && packet.vendor_id.as_deref() != Some(vendor_id.as_str())
    {
        return false;
    }
    if let Some(product_id) = &filters.product_id
        && packet.product_id.as_deref() != Some(product_id.as_str())
    {
        return false;
    }
    if let Some(interface_number) = filters.interface_number
        && packet.interface_number != Some(interface_number)
    {
        return false;
    }
    true
}

fn first_field_value(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        fields.get(*key).and_then(|values| {
            values
                .iter()
                .map(|value| value.trim())
                .find(|value| !value.is_empty())
                .map(str::to_string)
        })
    })
}

fn first_u16_field(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<u16> {
    first_field_value(fields, keys)
        .and_then(|value| parse_u64_field(&value))
        .and_then(|value| u16::try_from(value).ok())
}

fn first_u8_field(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<u8> {
    first_field_value(fields, keys)
        .and_then(|value| parse_u64_field(&value))
        .and_then(|value| u8::try_from(value).ok())
}

fn first_usize_field(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<usize> {
    first_field_value(fields, keys)
        .and_then(|value| parse_u64_field(&value))
        .and_then(|value| usize::try_from(value).ok())
}

fn first_u64_field(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<u64> {
    first_field_value(fields, keys).and_then(|value| parse_u64_field(&value))
}

fn first_hex16_field(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<String> {
    first_u16_field(fields, keys).map(hex_u16)
}

fn parse_u64_field(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if let Some(index) = trimmed.find("0x").or_else(|| trimmed.find("0X")) {
        let hex = trimmed[index + 2..]
            .chars()
            .take_while(|ch| ch.is_ascii_hexdigit())
            .collect::<String>();
        if !hex.is_empty() {
            return u64::from_str_radix(&hex, 16).ok();
        }
    }

    let digits = trimmed
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn packet_device_key(fields: &BTreeMap<String, Vec<String>>) -> Option<String> {
    let address = first_field_value(
        fields,
        &[
            "usb.device_address",
            "usb.addr",
            "usb.device_address.device",
        ],
    )?;
    let bus = first_field_value(fields, &["usb.bus_id", "usb.bus", "usb.bus_id.bus"])
        .unwrap_or_else(|| "unknown-bus".to_string());
    Some(format!("{bus}:{address}"))
}

fn parse_packet_direction(
    fields: &BTreeMap<String, Vec<String>>,
    endpoint_address: Option<u8>,
) -> Option<SniffUsbDirection> {
    first_field_value(
        fields,
        &[
            "usb.endpoint_direction",
            "usb.endpoint_address.direction",
            "usb.bmRequestType.direction",
        ],
    )
    .and_then(|value| parse_direction_text(&value))
    .or_else(|| endpoint_address.and_then(direction_from_endpoint_address))
}

fn parse_direction_text(value: &str) -> Option<SniffUsbDirection> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "1"
        || normalized == "in"
        || normalized.contains("device-to-host")
        || normalized.contains("device to host")
    {
        Some(SniffUsbDirection::DeviceToHost)
    } else if normalized == "0"
        || normalized == "out"
        || normalized.contains("host-to-device")
        || normalized.contains("host to device")
    {
        Some(SniffUsbDirection::HostToDevice)
    } else {
        None
    }
}

fn direction_from_endpoint_address(endpoint_address: u8) -> Option<SniffUsbDirection> {
    if endpoint_address == 0 {
        None
    } else if endpoint_address & 0x80 != 0 {
        Some(SniffUsbDirection::DeviceToHost)
    } else {
        Some(SniffUsbDirection::HostToDevice)
    }
}

fn parse_packet_transfer_type(
    fields: &BTreeMap<String, Vec<String>>,
    endpoint_address: Option<u8>,
) -> Option<SniffUsbTransferType> {
    first_field_value(
        fields,
        &[
            "usb.transfer_type",
            "usb.transfer_type_text",
            "usb.endpoint.transfer_type",
        ],
    )
    .and_then(|value| parse_transfer_type_text(&value))
    .or_else(|| {
        endpoint_address
            .filter(|endpoint| *endpoint == 0)
            .map(|_| SniffUsbTransferType::Control)
    })
}

fn parse_transfer_type_text(value: &str) -> Option<SniffUsbTransferType> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("control") || normalized == "0" {
        Some(SniffUsbTransferType::Control)
    } else if normalized.contains("interrupt") || normalized == "3" {
        Some(SniffUsbTransferType::Interrupt)
    } else if normalized.is_empty() {
        None
    } else {
        Some(SniffUsbTransferType::Other)
    }
}

fn tshark_usb_setup_fields_present(fields: &BTreeMap<String, Vec<String>>) -> bool {
    [
        "usb.setup.bmRequestType",
        "usb.setup.bRequest",
        "usb.setup.wValue",
        "usb.setup.wIndex",
        "usb.setup.wLength",
    ]
    .iter()
    .any(|field| fields.contains_key(*field))
}

fn first_payload_field(fields: &BTreeMap<String, Vec<String>>) -> Option<String> {
    first_field_value(
        fields,
        &[
            "usbhid.data",
            "hid.data",
            "usbcom.data.out_payload",
            "usbcom.data.in_payload",
            "usb.capdata",
            "usb.data_fragment",
            "data.data",
            "usb.descriptor",
        ],
    )
}

fn parse_payload_hex(value: &str) -> Option<Vec<u8>> {
    let hex = value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    if hex.is_empty() || hex.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[index..index + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

fn parse_packet_descriptor_kind(
    fields: &BTreeMap<String, Vec<String>>,
) -> Option<SniffDescriptorKind> {
    first_field_value(
        fields,
        &[
            "usb.descriptor_type",
            "usb.bDescriptorType",
            "usb.setup.wValue.descriptor_type",
        ],
    )
    .and_then(|value| parse_descriptor_kind_text(&value))
}

fn parse_descriptor_kind_text(value: &str) -> Option<SniffDescriptorKind> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("report") || normalized == "0x22" || normalized == "34" {
        Some(SniffDescriptorKind::HidReportDescriptor)
    } else if normalized.contains("device") || normalized == "0x01" || normalized == "1" {
        Some(SniffDescriptorKind::UsbDeviceDescriptor)
    } else if normalized.contains("configuration") || normalized == "0x02" || normalized == "2" {
        Some(SniffDescriptorKind::UsbConfigurationDescriptor)
    } else if normalized.is_empty() {
        None
    } else {
        Some(SniffDescriptorKind::Other)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn bytes_to_hex_sample(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_and_validate_sniff_plan(path: &Path) -> Result<StoredHardwareSniffPlan> {
    let plan: StoredHardwareSniffPlan = read_json_file(path)?;
    if plan.schema_version != 1 {
        anyhow::bail!(
            "sniff plan '{}' has unsupported schema_version {}; expected 1",
            path.display(),
            plan.schema_version
        );
    }
    if plan.command != "wheelctl hardware sniff-plan" {
        anyhow::bail!(
            "sniff plan '{}' was not produced by wheelctl hardware sniff-plan",
            path.display()
        );
    }
    if plan.evidence_status != SNIFF_EVIDENCE_STATUS {
        anyhow::bail!(
            "sniff plan '{}' has evidence_status '{}'; expected '{}'",
            path.display(),
            plan.evidence_status,
            SNIFF_EVIDENCE_STATUS
        );
    }
    if !plan.success {
        anyhow::bail!(
            "sniff plan '{}' did not succeed; refresh the plan before creating a receipt",
            path.display()
        );
    }
    required_text(&plan.family, "family")?;
    required_text(&plan.lane, "lane")?;
    validate_sniff_scenario(&plan.scenario)
        .with_context(|| format!("sniff plan '{}' has invalid scenario", path.display()))?;
    if plan.pre_capture_checklist.is_empty()
        || plan.post_capture_checklist.is_empty()
        || plan.operator_notes_required.is_empty()
        || plan.raw_pcap_commit_default
        || !plan
            .post_capture_checklist
            .iter()
            .any(|item| item == SNIFF_POST_CAPTURE_EVIDENCE_COMMANDS_CHECKLIST_ITEM)
    {
        anyhow::bail!(
            "sniff plan '{}' is missing the passive capture operator handoff; refresh it with wheelctl hardware sniff-plan",
            path.display()
        );
    }
    if plan.native_control_evidence
        || plan.openracing_hardware_output
        || !plan.external_app_may_have_sent_output
        || plan.satisfies_native_response_ready
        || plan.satisfies_native_visible_ready
        || plan.satisfies_smoke_ready
        || plan.satisfies_release_ready
        || !plan.readiness_claims.all_false()
    {
        anyhow::bail!(
            "sniff plan '{}' violates passive sniff invariants; sniff receipts require a non-claiming plan that records external-app output as possible",
            path.display()
        );
    }
    Ok(plan)
}

fn read_and_validate_sniff_receipt(path: &Path) -> Result<StoredHardwareSniffReceipt> {
    let receipt: StoredHardwareSniffReceipt = read_json_file(path)?;
    if receipt.schema_version != 1 {
        anyhow::bail!(
            "sniff receipt '{}' has unsupported schema_version {}; expected 1",
            path.display(),
            receipt.schema_version
        );
    }
    if receipt.command != "wheelctl hardware sniff-receipt" {
        anyhow::bail!(
            "sniff receipt '{}' was not produced by wheelctl hardware sniff-receipt",
            path.display()
        );
    }
    if receipt.evidence_status != SNIFF_EVIDENCE_STATUS {
        anyhow::bail!(
            "sniff receipt '{}' has evidence_status '{}'; expected '{}'",
            path.display(),
            receipt.evidence_status,
            SNIFF_EVIDENCE_STATUS
        );
    }
    if !receipt.success {
        anyhow::bail!(
            "sniff receipt '{}' did not succeed; refresh the receipt before creating a bundle",
            path.display()
        );
    }
    required_text(&receipt.generated_at_utc, "generated-at-utc")?;
    required_text(&receipt.plan_path, "plan-path")?;
    required_pcapng_path_display(Path::new(&receipt.pcapng_path))?;
    if receipt.pcapng_size_bytes == 0 {
        anyhow::bail!(
            "sniff receipt '{}' has pcapng_size_bytes 0; expected a non-empty capture",
            path.display()
        );
    }
    required_text(&receipt.operator, "operator")?;
    required_text(&receipt.app, "app")?;
    validate_sniff_scenario(&receipt.scenario)
        .with_context(|| format!("sniff receipt '{}' has invalid scenario", path.display()))?;
    required_text(&receipt.device_note, "device-note")?;
    required_text(&receipt.evidence, "evidence")?;
    validate_sniff_sha256(&receipt.pcapng_sha256, "sniff receipt pcapng_sha256").with_context(
        || {
            format!(
                "sniff receipt '{}' has invalid pcapng_sha256",
                path.display()
            )
        },
    )?;
    if receipt.native_control_evidence
        || receipt.openracing_hardware_output
        || receipt.openracing_hid_device_opened
        || receipt.openracing_ffb_writes
        || receipt.openracing_output_reports
        || receipt.openracing_feature_reports
        || receipt.openracing_serial_config_commands
        || receipt.openracing_firmware_or_dfu_commands
        || !receipt.external_app_observed
        || !receipt.external_app_may_have_sent_output
        || receipt.satisfies_native_response_ready
        || receipt.satisfies_native_visible_ready
        || receipt.satisfies_smoke_ready
        || receipt.satisfies_release_ready
        || !receipt.readiness_claims.all_false()
    {
        anyhow::bail!(
            "sniff receipt '{}' violates passive sniff invariants; sniff bundles require non-claiming receipt evidence",
            path.display()
        );
    }
    Ok(receipt)
}

fn read_and_validate_sniff_notes_template_receipt(
    path: &Path,
) -> Result<StoredHardwareSniffNotesTemplateReceipt> {
    let receipt: StoredHardwareSniffNotesTemplateReceipt = read_json_file(path)?;
    if receipt.schema_version != 1 {
        anyhow::bail!(
            "sniff notes template receipt '{}' has unsupported schema_version {}; expected 1",
            path.display(),
            receipt.schema_version
        );
    }
    if receipt.command != "wheelctl hardware sniff-notes-template" {
        anyhow::bail!(
            "sniff notes template receipt '{}' was not produced by wheelctl hardware sniff-notes-template",
            path.display()
        );
    }
    if receipt.evidence_status != SNIFF_EVIDENCE_STATUS {
        anyhow::bail!(
            "sniff notes template receipt '{}' has evidence_status '{}'; expected '{}'",
            path.display(),
            receipt.evidence_status,
            SNIFF_EVIDENCE_STATUS
        );
    }
    if !receipt.success {
        anyhow::bail!(
            "sniff notes template receipt '{}' did not succeed; refresh the receipt before creating a bundle",
            path.display()
        );
    }
    required_text(&receipt.generated_at_utc, "notes-template generated-at-utc")?;
    required_text(&receipt.plan_path, "notes-template plan-path")?;
    required_text(&receipt.out_path, "notes-template out-path")?;
    validate_sniff_scenario(&receipt.scenario).with_context(|| {
        format!(
            "sniff notes template receipt '{}' has invalid scenario",
            path.display()
        )
    })?;
    required_text(&receipt.operator, "notes-template operator")?;
    required_text(&receipt.device_note, "notes-template device-note")?;
    if receipt.native_control_evidence
        || receipt.openracing_hardware_output
        || receipt.satisfies_native_response_ready
        || receipt.satisfies_native_visible_ready
        || receipt.satisfies_smoke_ready
        || receipt.satisfies_release_ready
        || !receipt.readiness_claims.all_false()
    {
        anyhow::bail!(
            "sniff notes template receipt '{}' violates passive sniff invariants; sniff bundles require non-claiming operator notes receipt evidence",
            path.display()
        );
    }
    Ok(receipt)
}

fn read_and_validate_sniff_summary(path: &Path) -> Result<StoredHardwareSniffSummary> {
    let summary: StoredHardwareSniffSummary = read_json_file(path)?;
    if summary.schema_version != 1 {
        anyhow::bail!(
            "sniff summary '{}' has unsupported schema_version {}; expected 1",
            path.display(),
            summary.schema_version
        );
    }
    if summary.command != "wheelctl hardware sniff-summary" {
        anyhow::bail!(
            "sniff summary '{}' was not produced by wheelctl hardware sniff-summary",
            path.display()
        );
    }
    if summary.evidence_status != SNIFF_EVIDENCE_STATUS {
        anyhow::bail!(
            "sniff summary '{}' has evidence_status '{}'; expected '{}'",
            path.display(),
            summary.evidence_status,
            SNIFF_EVIDENCE_STATUS
        );
    }
    let _summary_success = summary.success;
    validate_sniff_sha256(&summary.pcapng_sha256, "sniff summary pcapng_sha256").with_context(
        || {
            format!(
                "sniff summary '{}' has invalid pcapng_sha256",
                path.display()
            )
        },
    )?;
    if summary.native_control_evidence
        || summary.openracing_hardware_output
        || !summary.external_app_may_have_sent_output
        || summary.satisfies_native_response_ready
        || summary.satisfies_native_visible_ready
        || summary.satisfies_smoke_ready
        || summary.satisfies_release_ready
        || !summary.readiness_claims.all_false()
    {
        anyhow::bail!(
            "sniff summary '{}' violates passive sniff invariants; sniff bundles require non-claiming summary evidence",
            path.display()
        );
    }
    Ok(summary)
}

fn validate_sniff_sha256(value: &str, field: &str) -> Result<()> {
    if value.len() != 64 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        anyhow::bail!("{field} must be a 64-character sha256 hex digest");
    }
    if value.chars().any(|ch| ch.is_ascii_uppercase()) {
        anyhow::bail!("{field} must use lowercase sha256 hex");
    }
    Ok(())
}

fn required_text(value: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("sniff {field} must not be empty");
    }
    Ok(trimmed.to_string())
}

fn required_path_display(path: &Path, field: &str) -> Result<String> {
    let text = path.display().to_string();
    if text.trim().is_empty() {
        anyhow::bail!("sniff {field} path must not be empty");
    }
    Ok(text)
}

fn required_pcapng_path_display(path: &Path) -> Result<String> {
    let text = required_path_display(path, "pcapng")?;
    if !text.ends_with(".pcapng") {
        anyhow::bail!("pcapng capture path must end with .pcapng: {text}");
    }
    Ok(text)
}

fn hash_existing_pcapng(path: &Path) -> Result<(String, u64)> {
    if !path.is_file() {
        anyhow::bail!(
            "pcapng capture not found: {}; save the passive USB observation as .pcapng and pass it with --pcapng",
            path.display()
        );
    }
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect pcapng capture '{}'", path.display()))?;
    let size = metadata.len();
    if size == 0 {
        anyhow::bail!(
            "pcapng capture is empty: {}; provide a non-empty passive capture",
            path.display()
        );
    }

    Ok((hash_existing_file(path)?, size))
}

fn hash_existing_file(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open '{}'", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read '{}'", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn current_sniff_platform_hint() -> HardwareSniffPlatformHint {
    if cfg!(target_os = "windows") {
        HardwareSniffPlatformHint::Windows
    } else if cfg!(target_os = "linux") {
        HardwareSniffPlatformHint::Linux
    } else if cfg!(target_os = "macos") {
        HardwareSniffPlatformHint::Macos
    } else {
        HardwareSniffPlatformHint::Unknown
    }
}

fn normalized_sniff_capture_tools(
    capture_tools: &[HardwareSniffCaptureTool],
    platform_hint: HardwareSniffPlatformHint,
) -> Vec<String> {
    let tools = if capture_tools.is_empty() {
        default_sniff_capture_tools(platform_hint)
    } else {
        capture_tools.to_vec()
    };
    tools
        .into_iter()
        .map(HardwareSniffCaptureTool::as_str)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn default_sniff_capture_tools(
    platform_hint: HardwareSniffPlatformHint,
) -> Vec<HardwareSniffCaptureTool> {
    match platform_hint {
        HardwareSniffPlatformHint::Windows => vec![
            HardwareSniffCaptureTool::Wireshark,
            HardwareSniffCaptureTool::UsbPcap,
            HardwareSniffCaptureTool::Tshark,
        ],
        HardwareSniffPlatformHint::Linux => vec![
            HardwareSniffCaptureTool::Wireshark,
            HardwareSniffCaptureTool::Tshark,
            HardwareSniffCaptureTool::Usbmon,
        ],
        HardwareSniffPlatformHint::Macos | HardwareSniffPlatformHint::Unknown => {
            vec![
                HardwareSniffCaptureTool::Wireshark,
                HardwareSniffCaptureTool::Tshark,
            ]
        }
    }
}

fn validate_sniff_scenario(value: &str) -> Result<()> {
    match value {
        "enumeration"
        | "vendor-app-closed-idle"
        | "pit-house-open-idle"
        | "pit-house-full-controls"
        | "pit-house-setting-change"
        | "pit-house-firmware-page-observed"
        | "simhub-open-idle"
        | "simhub-device-detect"
        | "simhub-output-session"
        | "simulator-session-start-stop"
        | "custom" => Ok(()),
        _ => anyhow::bail!("unsupported passive sniff scenario '{value}'"),
    }
}

fn sniff_notes_capture_hints_from_hardware_doctor(
    path: Option<&Path>,
) -> Result<Option<HardwareSniffNotesCaptureHints>> {
    let Some(path) = path else {
        return Ok(None);
    };

    let receipt: serde_json::Value = read_json_file(path)?;
    let receipt_flags = HardwareSniffNotesDoctorFlags {
        no_hid_device_opened: json_bool_field(&receipt, "no_hid_device_opened"),
        no_ffb_writes: json_bool_field(&receipt, "no_ffb_writes"),
        no_output_reports: json_bool_field(&receipt, "no_output_reports"),
        no_feature_reports: json_bool_field(&receipt, "no_feature_reports"),
        no_serial_config_commands: json_bool_field(&receipt, "no_serial_config_commands"),
        no_firmware_or_dfu_commands: json_bool_field(&receipt, "no_firmware_or_dfu_commands"),
    };

    let hints = receipt
        .pointer("/tools/usbpcap_descriptor_capture/usbpcap_moza_device_hints")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(sniff_notes_capture_hint_from_value)
        .collect::<Vec<_>>();
    let usbpcap_extcap_path = receipt
        .pointer("/tools/usbpcap_descriptor_capture/usbpcap_extcap_path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string);
    let active_usbpcap_processes =
        sniff_notes_active_usbpcap_processes_from_hardware_doctor(&receipt);

    Ok(Some(HardwareSniffNotesCaptureHints {
        source: required_path_display(path, "hardware-doctor")?,
        receipt_flags,
        usbpcap_extcap_path,
        hint_count: hints.len(),
        hints,
        active_usbpcap_process_count: active_usbpcap_processes.len(),
        active_usbpcap_processes,
        notes: vec![
            "hardware doctor is observe-only; these hints only identify the passive capture interface and device filter".to_string(),
            "operator notes do not prove a pcap capture exists and do not authorize OpenRacing output".to_string(),
        ],
    }))
}

fn sniff_notes_capture_hint_from_value(
    value: &serde_json::Value,
) -> Option<HardwareSniffNotesUsbPcapHint> {
    let usbpcap_interface = value
        .get("usbpcap_interface")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let capture_devices_value = value
        .get("capture_devices_value")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let matched_device_displays = value
        .get("matched_device_displays")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let suggested_capture_filter = value
        .get("suggested_capture_filter")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!("select {usbpcap_interface} with USBPcap --devices {capture_devices_value}")
        });

    Some(HardwareSniffNotesUsbPcapHint {
        usbpcap_interface,
        capture_devices_value,
        matched_device_displays,
        suggested_capture_filter,
    })
}

fn sniff_notes_active_usbpcap_processes_from_hardware_doctor(
    receipt: &serde_json::Value,
) -> Vec<HardwareSniffNotesActiveUsbPcapProcess> {
    receipt
        .pointer("/tools/usbpcap_descriptor_capture/active_usbpcap_processes/processes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let process_id = u32::try_from(value.get("process_id")?.as_u64()?).ok()?;
            Some(HardwareSniffNotesActiveUsbPcapProcess {
                process_id,
                command_line: value
                    .get("command_line")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

fn sniff_notes_local_capture_path(plan: &StoredHardwareSniffPlan) -> String {
    format!("target\\sniff\\{}\\capture.pcapng", plan.scenario)
}

fn usbpcapcmd_capture_command(
    extcap_path: &str,
    hint: &HardwareSniffNotesUsbPcapHint,
    capture_path: &str,
) -> String {
    format!(
        "& {} -d {} --devices {} --inject-descriptors -o {}",
        powershell_double_quoted_arg(extcap_path),
        powershell_double_quoted_arg(&hint.usbpcap_interface),
        hint.capture_devices_value,
        powershell_double_quoted_arg(capture_path)
    )
}

fn wheelctl_sniff_capture_command(
    extcap_path: &str,
    hint: &HardwareSniffNotesUsbPcapHint,
    capture_path: &str,
    duration_ms: u64,
) -> String {
    format!(
        "wheelctl hardware sniff-capture --usbpcapcmd {} --usbpcap-interface {} --devices {} --duration-ms {} --out {} --confirm-external-passive-capture --json-out {}",
        powershell_double_quoted_arg(extcap_path),
        powershell_double_quoted_arg(&hint.usbpcap_interface),
        hint.capture_devices_value,
        duration_ms,
        powershell_double_quoted_arg(capture_path),
        powershell_double_quoted_arg(&sniff_capture_receipt_path(capture_path))
    )
}

fn sniff_capture_receipt_path(capture_path: &str) -> String {
    let path = Path::new(capture_path);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent
        .join("sniff-capture-receipt.json")
        .display()
        .to_string()
}

fn powershell_double_quoted_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('`', "``").replace('"', "`\""))
}

fn json_bool_field(value: &serde_json::Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn render_sniff_plan_markdown(plan: &HardwareSniffPlanArtifact) -> String {
    let mut out = String::new();
    out.push_str("# Passive USB Sniff Plan\n\n");
    out.push_str("This plan is non-claiming protocol research/support evidence.\n\n");
    out.push_str(&format!("Family: `{}`\n", plan.family));
    out.push_str(&format!("Scenario: `{}`\n", plan.scenario));
    out.push_str(&format!("Lane: `{}`\n", plan.lane));
    out.push_str(&format!("Operator: `{}`\n", plan.operator));
    out.push_str(&format!("Device: `{}`\n", plan.device_note));
    out.push_str(&format!("Platform: `{}`\n\n", plan.platform_hint));

    out.push_str("## Allowed Actions\n\n");
    for action in &plan.allowed_actions {
        out.push_str(&format!("- {action}\n"));
    }
    out.push_str("\n## Forbidden Actions\n\n");
    for action in &plan.forbidden_actions {
        out.push_str(&format!("- {action}\n"));
    }

    out.push_str("\n## Pre-Capture Checklist\n\n");
    for item in &plan.pre_capture_checklist {
        out.push_str(&format!("- {item}\n"));
    }

    out.push_str("\n## Post-Capture Checklist\n\n");
    for item in &plan.post_capture_checklist {
        out.push_str(&format!("- {item}\n"));
    }

    out.push_str("\n## Operator Notes Required\n\n");
    for item in &plan.operator_notes_required {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str(&format!(
        "\nRaw pcapng commit default: `{}`\n",
        plan.raw_pcap_commit_default
    ));

    out.push_str("\n## Readiness Claims\n\n");
    out.push_str("- native response ready: false\n");
    out.push_str("- native visible ready: false\n");
    out.push_str("- smoke ready: false\n");
    out.push_str("- release ready: false\n\n");
    out.push_str("OpenRacing hardware output: false\n");
    out.push_str("External app may have sent output: true\n");
    out
}

fn render_sniff_operator_notes_template(
    plan_path: &Path,
    plan: &StoredHardwareSniffPlan,
    capture_hints: Option<&HardwareSniffNotesCaptureHints>,
) -> String {
    let mut out = String::new();
    out.push_str("# Passive USB Sniff Operator Notes\n\n");
    out.push_str("This template is non-claiming protocol research/support evidence.\n\n");
    out.push_str(&format!("Plan: `{}`\n", plan_path.display()));
    out.push_str(&format!("Family: `{}`\n", plan.family));
    out.push_str(&format!("Scenario: `{}`\n", plan.scenario));
    out.push_str(&format!("Lane: `{}`\n", plan.lane));
    out.push_str(&format!("Operator: `{}`\n", plan.operator));
    out.push_str(&format!("Device: `{}`\n\n", plan.device_note));

    out.push_str("## Required Notes\n\n");
    for field in &plan.operator_notes_required {
        out.push_str(&format!("- [ ] {field}:\n"));
    }

    if let Some(capture_hints) = capture_hints {
        out.push_str("\n## Capture Tool Hints\n\n");
        out.push_str(&format!(
            "Hardware doctor receipt: `{}`\n\n",
            capture_hints.source
        ));
        out.push_str(&format!(
            "- [ ] Hardware doctor no-HID/no-output/no-feature/no-serial-config/no-firmware flags stayed true: `{}`\n",
            capture_hints.receipt_flags.all_no_output_flags_true()
        ));
        if capture_hints.hints.is_empty() {
            out.push_str(
                "- [ ] No Moza USBPcap device hint was present in the hardware doctor receipt.\n",
            );
        } else {
            let local_capture_path = sniff_notes_local_capture_path(plan);
            for hint in &capture_hints.hints {
                out.push_str(&format!(
                    "- [ ] USBPcap interface used: `{}`\n",
                    hint.usbpcap_interface
                ));
                out.push_str(&format!(
                    "- [ ] USBPcap device filter used: `--devices {}`\n",
                    hint.capture_devices_value
                ));
                if let Some(extcap_path) = &capture_hints.usbpcap_extcap_path {
                    out.push_str(
                        "- [ ] Bounded wheelctl USBPcapCMD capture helper; run this while performing the scenario:\n\n",
                    );
                    out.push_str("```powershell\n");
                    out.push_str(&wheelctl_sniff_capture_command(
                        extcap_path,
                        hint,
                        &local_capture_path,
                        60_000,
                    ));
                    out.push_str("\n```\n");
                    out.push_str(
                        "- [ ] External USBPcapCMD capture command; run outside OpenRacing and stop it after the scenario:\n\n",
                    );
                    out.push_str("```powershell\n");
                    out.push_str(&usbpcapcmd_capture_command(
                        extcap_path,
                        hint,
                        &local_capture_path,
                    ));
                    out.push_str("\n```\n");
                }
                out.push_str(&format!(
                    "- [ ] Suggested capture filter: `{}`\n",
                    hint.suggested_capture_filter
                ));
                if !hint.matched_device_displays.is_empty() {
                    out.push_str(&format!(
                        "- [ ] Matched device stack: `{}`\n",
                        hint.matched_device_displays.join("`, `")
                    ));
                }
            }
        }
        if capture_hints.active_usbpcap_process_count > 0 {
            out.push_str(&format!(
                "- [ ] Active USBPcapCMD processes detected before capture: `{}`; confirm they are stopped or are the intended current capture before starting this scenario.\n",
                capture_hints.active_usbpcap_process_count
            ));
            for process in &capture_hints.active_usbpcap_processes {
                let command_line = process.command_line.as_deref().unwrap_or("unknown");
                out.push_str(&format!(
                    "- [ ] Active USBPcapCMD process `{}`: `{}`\n",
                    process.process_id, command_line
                ));
            }
        }
        for note in &capture_hints.notes {
            out.push_str(&format!("- [ ] Hint boundary: {note}\n"));
        }
    }

    out.push_str("\n## Capture Safety Confirmations\n\n");
    for item in &plan.pre_capture_checklist {
        out.push_str(&format!("- [ ] Pre-capture: {item}\n"));
    }
    for item in &plan.post_capture_checklist {
        out.push_str(&format!("- [ ] Post-capture: {item}\n"));
    }
    out.push_str(&format!(
        "- [ ] Raw pcapng commit default remained `{}`\n",
        plan.raw_pcap_commit_default
    ));

    out.push_str("\n## Claim Boundaries\n\n");
    out.push_str("- [ ] OpenRacing opened no HID device for this capture.\n");
    out.push_str("- [ ] OpenRacing sent no output, feature, serial, firmware, or DFU commands.\n");
    out.push_str("- [ ] This note does not claim native response, native visible, smoke, or release readiness.\n");
    out
}

fn render_sniff_summary_markdown(summary: &HardwareSniffSummaryArtifact) -> String {
    let mut out = String::new();
    out.push_str("# Passive USB Sniff Summary\n\n");
    out.push_str("This summary is non-claiming protocol research/support evidence.\n\n");
    out.push_str(&format!("Success: `{}`\n", summary.success));
    if let Some(reason) = &summary.reason {
        out.push_str(&format!("Reason: `{reason}`\n"));
    }
    out.push_str(&format!("Matched packets: `{}`\n", summary.matched_packets));
    out.push_str(&format!("PCAPNG sha256: `{}`\n\n", summary.pcapng_sha256));

    out.push_str("## Filters\n\n");
    out.push_str(&format!(
        "- vendor: `{}`\n",
        summary.filters.vendor_id.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!(
        "- product: `{}`\n",
        summary.filters.product_id.as_deref().unwrap_or("none")
    ));
    let interface = summary
        .filters
        .interface_number
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    out.push_str(&format!("- interface: `{interface}`\n\n"));

    out.push_str("## Transfer Summary\n\n");
    out.push_str(&format!(
        "- host to device: `{}`\n",
        summary.usb_transfer_summary.host_to_device
    ));
    out.push_str(&format!(
        "- device to host: `{}`\n",
        summary.usb_transfer_summary.device_to_host
    ));
    out.push_str(&format!(
        "- control: `{}`\n",
        summary.usb_transfer_summary.control
    ));
    out.push_str(&format!(
        "- interrupt: `{}`\n\n",
        summary.usb_transfer_summary.interrupt
    ));

    out.push_str("## Host-to-Device Payload Coverage\n\n");
    out.push_str(&format!(
        "- data-length packets: `{}`\n",
        summary
            .report_classification_summary
            .host_to_device_data_len_packet_count
    ));
    out.push_str(&format!(
        "- declared data bytes: `{}`\n",
        summary
            .report_classification_summary
            .host_to_device_data_len_bytes
    ));
    out.push_str(&format!(
        "- payload-extracted packets: `{}`\n",
        summary
            .report_classification_summary
            .host_to_device_payload_extracted_packet_count
    ));
    out.push_str(&format!(
        "- payload-missing packets: `{}`\n",
        summary
            .report_classification_summary
            .host_to_device_payload_missing_packet_count
    ));
    out.push_str(&format!(
        "- payload export gap: `{}`\n\n",
        summary
            .report_classification_summary
            .host_to_device_payload_export_gap
    ));
    let missing_examples = &summary
        .report_classification_summary
        .host_to_device_payload_missing_packet_examples;
    if !missing_examples.is_empty() {
        out.push_str("| Packet | Frame | Interface | Endpoint | Data len | Payload extracted |\n");
        out.push_str("| ---: | ---: | ---: | --- | ---: | --- |\n");
        for example in missing_examples {
            let frame = example
                .frame_number
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let interface = example
                .interface_number
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let endpoint = example.endpoint_address.as_deref().unwrap_or("unknown");
            out.push_str(&format!(
                "| {} | {} | {} | `{}` | {} | `{}` |\n",
                example.packet_ordinal,
                frame,
                interface,
                endpoint,
                example.data_len,
                example.payload_extracted
            ));
        }
        out.push('\n');
    }

    out.push_str("## Observed Devices\n\n");
    if summary.observed_devices.is_empty() {
        out.push_str("- none\n");
    } else {
        for device in &summary.observed_devices {
            out.push_str(&format!(
                "- {}:{} interfaces={:?} endpoints={:?}\n",
                device.vendor_id, device.product_id, device.interfaces, device.endpoints
            ));
        }
    }

    out.push_str("\n## Observed Reports\n\n");
    if summary.observed_reports.is_empty() {
        out.push_str("- none\n");
    } else {
        out.push_str(&format!(
            "- standard PIDFF output/control report IDs: {:?}\n",
            summary
                .report_classification_summary
                .standard_pidff_output_report_ids
        ));
        out.push_str(&format!(
            "- vendor/device-specific candidate report IDs: {:?}\n",
            summary
                .report_classification_summary
                .vendor_or_device_specific_output_candidate_report_ids
        ));
        out.push_str(&format!(
            "- decode recommended: `{}`\n",
            summary.report_classification_summary.decode_recommended
        ));
        for report in &summary.observed_reports {
            out.push_str(&format!(
                "- {} report {} count={} samples={} classification={}\n",
                report.direction,
                report.report_id,
                report.count,
                report.payload_sample_count,
                report.classification.label
            ));
        }
    }

    out.push_str("\n## Readiness Claims\n\n");
    out.push_str("- native response ready: false\n");
    out.push_str("- native visible ready: false\n");
    out.push_str("- smoke ready: false\n");
    out.push_str("- release ready: false\n");
    out
}

fn build_bringup_rail_receipt(family: &str) -> Result<HardwareBringupRailReceipt> {
    let adapter = hardware_family_adapter_contract(family)
        .with_context(|| format!("unknown hardware bring-up family '{family}'"))?;
    Ok(HardwareBringupRailReceipt {
        success: true,
        command: "wheelctl hardware bringup-rail",
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        rail_version: 1,
        family: adapter.id,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        stages: hardware_bringup_stages(),
        adapter,
        notes: vec![
            "hardware bring-up rail is read-only; it opens no HID device and sends no reports"
                .to_string(),
            "device-family adapters provide requirements, while the stage ordering and safety boundaries stay common"
                .to_string(),
            "FFB is not a discovery or passive-stage action; zero-torque and fail-closed receipts come first"
                .to_string(),
        ],
    })
}

#[cfg(test)]
fn scaffold_hardware_lane(
    lane: &Path,
    family: &str,
    topology: &str,
    operator: &str,
    overwrite: bool,
    json_out: Option<&Path>,
) -> Result<HardwareLaneInitReceipt> {
    scaffold_hardware_lane_with_overrides(
        lane,
        family,
        topology,
        operator,
        &HardwareLaneRoleOverrides::default(),
        overwrite,
        json_out,
    )
}

fn scaffold_hardware_lane_with_overrides(
    lane: &Path,
    family: &str,
    topology: &str,
    operator: &str,
    role_overrides: &HardwareLaneRoleOverrides,
    overwrite: bool,
    json_out: Option<&Path>,
) -> Result<HardwareLaneInitReceipt> {
    let adapter = hardware_family_adapter_contract(family)
        .with_context(|| format!("unknown hardware bring-up family '{family}'"))?;
    let stages = hardware_bringup_stages();
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let captures_dir = lane.join("captures");
    let manifest_path = lane.join("hardware-lane-manifest.json");
    let checklist_path = lane.join("artifact-checklist.md");
    let capture_plan_path = lane.join("capture-plan.md");
    let stage_gates_path = lane.join("stage-gates.json");
    let receipt_path = json_out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| lane.join("lane-init.json"));

    let planned_files = [
        manifest_path.as_path(),
        checklist_path.as_path(),
        capture_plan_path.as_path(),
        stage_gates_path.as_path(),
        receipt_path.as_path(),
    ];
    if !overwrite {
        let existing: Vec<_> = planned_files
            .iter()
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
            .collect();
        if !existing.is_empty() {
            anyhow::bail!(
                "hardware lane scaffold files already exist; pass --overwrite to replace: {}",
                existing.join(", ")
            );
        }
    }

    fs::create_dir_all(&captures_dir)
        .with_context(|| format!("failed to create '{}'", captures_dir.display()))?;

    let roles = lane_roles(&adapter, topology, role_overrides)?;
    let manifest = HardwareLaneScaffoldManifest {
        schema_version: 1,
        generated_at_utc: generated_at.clone(),
        lane: lane.display().to_string(),
        family: adapter.id,
        topology: topology.to_string(),
        operator: operator.to_string(),
        completion_state: "not_started",
        rail_stage_order: stages.iter().map(|stage| stage.id).collect(),
        declared_logical_roles: roles.clone(),
        adapter_known_vid_pids: adapter.known_vid_pids.clone(),
        notes: vec![
            "scaffold records intended topology and required evidence; it is not hardware evidence"
                .to_string(),
            "no fake pass/fail receipts are created by lane init".to_string(),
        ],
    };
    let stage_gates = HardwareLaneStageGates {
        schema_version: 1,
        generated_at_utc: generated_at.clone(),
        family: adapter.id,
        topology: topology.to_string(),
        stages: stages.clone(),
        adapter: adapter.clone(),
        notes: vec![
            "stage gates are copied from the common bring-up rail".to_string(),
            "device-family adapter requirements refine evidence; they do not bypass gates"
                .to_string(),
        ],
    };

    write_json_file(&manifest_path, &manifest)?;
    write_text_file(
        &checklist_path,
        &render_artifact_checklist(&adapter, &stages, &roles),
    )?;
    write_text_file(
        &capture_plan_path,
        &render_capture_plan(&adapter, topology, &roles),
    )?;
    write_json_file(&stage_gates_path, &stage_gates)?;

    let receipt = HardwareLaneInitReceipt {
        success: true,
        command: "wheelctl hardware lane init",
        generated_at_utc: generated_at,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        lane: lane.display().to_string(),
        family: adapter.id,
        topology: topology.to_string(),
        operator: operator.to_string(),
        captures_dir: captures_dir.display().to_string(),
        created_files: vec![
            manifest_path.display().to_string(),
            checklist_path.display().to_string(),
            capture_plan_path.display().to_string(),
            stage_gates_path.display().to_string(),
            receipt_path.display().to_string(),
        ],
        notes: vec![
            "hardware lane init creates local scaffold files only; it opens no HID device"
                .to_string(),
            "capture/checklist entries are planned artifact paths, not evidence".to_string(),
            "output-adjacent stages remain blocked until earlier receipts pass".to_string(),
        ],
    };
    write_json_file(&receipt_path, &receipt)?;
    Ok(receipt)
}

fn build_hardware_lane_status_receipt(lane: &Path) -> Result<HardwareLaneStatusReceipt> {
    let manifest = read_hardware_lane_status_manifest(lane)?;
    let adapter = hardware_family_adapter_contract(&manifest.family)
        .with_context(|| format!("unknown hardware bring-up family '{}'", manifest.family))?;
    let scaffold_files = hardware_lane_scaffold_files(lane);
    let scaffold_required = manifest.manifest_source == "hardware-lane-manifest.json";
    let scaffold_complete = scaffold_files.iter().all(|artifact| artifact.present);
    let role_evidence: Vec<_> = manifest
        .declared_logical_roles
        .iter()
        .map(|role| HardwareLaneRoleEvidenceStatus {
            id: role.id.clone(),
            required: role.required,
            connection_path: role.connection_path.clone(),
            expected_endpoint: role.expected_endpoint.clone(),
            evidence_artifact: role.evidence_artifact.clone(),
            artifact_present: lane.join(&role.evidence_artifact).exists(),
            semantic_status: role.semantic_status.clone(),
            validation_status: "not_validated_by_status".to_string(),
        })
        .collect();
    let stage_status: Vec<_> = hardware_bringup_stages()
        .into_iter()
        .map(|stage| {
            let artifacts = stage_expected_artifacts(lane, &stage, &manifest.declared_logical_roles);
            let present = artifacts.iter().filter(|artifact| artifact.present).count();
            let failed = artifacts
                .iter()
                .filter(|artifact| lane_artifact_explicit_failure(lane, stage.id, artifact))
                .count();
            let missing = artifacts.len().saturating_sub(present);
            HardwareLaneStageStatus {
                id: stage.id,
                order: stage.order,
                purpose: stage.purpose,
                artifacts_present: present,
                artifacts_missing: missing,
                artifacts_failed: failed,
                expected_artifacts: artifacts,
                gate_status: "not_validated_by_status",
                notes: vec![
                    "status inventories artifact presence only; run the family verifier for evidence claims"
                        .to_string(),
                ],
            }
        })
        .collect();
    let missing_role_endpoints =
        required_roles_with_placeholder_endpoints(&manifest.declared_logical_roles);
    let verifier_receipt = lane_verifier_receipt_status(lane);
    let blocking_items = lane_status_blocking_items(
        &stage_status,
        scaffold_required,
        scaffold_complete,
        &missing_role_endpoints,
        &verifier_receipt,
    );
    let first_missing_artifact_stage = stage_status
        .iter()
        .find(|stage| stage.artifacts_missing > 0)
        .map(|stage| (stage.id, stage.order));
    let first_failed_artifact_stage = stage_status
        .iter()
        .find(|stage| stage.artifacts_failed > 0)
        .map(|stage| (stage.id, stage.order));
    let next_blocked_stage = lane_status_next_blocked_stage(
        first_missing_artifact_stage,
        first_failed_artifact_stage,
        !missing_role_endpoints.is_empty(),
        verifier_receipt.stage_blocker.as_deref(),
    );
    let descriptor_capture_tooling = lane_descriptor_capture_tooling_status(lane);
    let safe_next_commands = lane_status_safe_next_commands(
        lane,
        adapter.id,
        next_blocked_stage,
        &manifest.declared_logical_roles,
        &descriptor_capture_tooling,
    );

    Ok(HardwareLaneStatusReceipt {
        success: true,
        command: "wheelctl hardware lane status",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        lane: lane.display().to_string(),
        manifest_source: manifest.manifest_source,
        family: adapter.id,
        topology: manifest.topology,
        completion_state: manifest.completion_state,
        scaffold_required,
        scaffold_complete,
        evidence_claims_validated: false,
        ready_for_zero_torque: false,
        ready_for_ffb: false,
        next_blocked_stage,
        safe_next_commands,
        blocking_items,
        verifier_receipt,
        descriptor_capture_tooling,
        scaffold_files,
        role_evidence,
        stages: stage_status,
        notes: vec![
            "lane status is read-only and validates no hardware claims".to_string(),
            "artifact presence is not proof; verifier receipts remain authoritative".to_string(),
            if scaffold_required {
                "scaffold files are required because this lane uses hardware-lane-manifest.json"
                    .to_string()
            } else {
                "legacy manifest lanes are adapted for status; missing scaffold files are inventoried, not blockers"
                    .to_string()
            },
            "ready_for_zero_torque and ready_for_ffb stay false in this inventory receipt"
                .to_string(),
        ],
    })
}

fn read_hardware_lane_status_manifest(lane: &Path) -> Result<StoredHardwareLaneScaffoldManifest> {
    let scaffold_path = lane.join("hardware-lane-manifest.json");
    if scaffold_path.exists() {
        let mut manifest: StoredHardwareLaneScaffoldManifest = read_json_file(&scaffold_path)?;
        manifest.manifest_source = "hardware-lane-manifest.json".to_string();
        return Ok(manifest);
    }

    let legacy_path = lane.join("manifest.json");
    let legacy: serde_json::Value = read_json_file(&legacy_path)?;
    legacy_moza_manifest_to_lane_status_manifest(&legacy)
        .with_context(|| format!("failed to adapt legacy '{}'", legacy_path.display()))
}

fn legacy_moza_manifest_to_lane_status_manifest(
    manifest: &serde_json::Value,
) -> Result<StoredHardwareLaneScaffoldManifest> {
    let wheelbase = manifest
        .get("hardware")
        .and_then(|hardware| hardware.get("wheelbase"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !wheelbase.to_ascii_lowercase().contains("moza r5") {
        anyhow::bail!("legacy manifest.json is not a Moza R5 lane manifest");
    }

    let topology = manifest
        .get("topology")
        .ok_or_else(|| anyhow::anyhow!("legacy manifest.json missing topology"))?;
    let topology_name = topology
        .get("primary_input_path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("wheelbase_hub")
        .to_string();
    let completion_state = manifest
        .get("completion_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let endpoints = topology
        .get("endpoints")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let logical_controls = topology
        .get("logical_controls")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("legacy manifest.json missing topology.logical_controls"))?;
    let mut declared_logical_roles = Vec::new();
    for (id, control) in logical_controls {
        let role_id = control
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(id);
        let source_endpoint = control
            .get("source_endpoint")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let expected_endpoint = legacy_endpoint_selector_for_source(&endpoints, source_endpoint)
            .unwrap_or_else(|| {
                if source_endpoint.is_empty() {
                    "declare-observed-endpoint".to_string()
                } else {
                    source_endpoint.to_string()
                }
            });
        declared_logical_roles.push(StoredHardwareLaneLogicalRole {
            id: id.clone(),
            required: control
                .get("required")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            connection_path: control
                .get("connection")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            expected_endpoint,
            evidence_artifact: control
                .get("evidence_capture")
                .and_then(serde_json::Value::as_str)
                .map_or_else(
                    || default_role_evidence_artifact("moza-r5", role_id),
                    str::to_string,
                ),
            semantic_status: control
                .get("semantic_status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("pending_capture")
                .to_string(),
        });
    }
    declared_logical_roles.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(StoredHardwareLaneScaffoldManifest {
        manifest_source: "manifest.json".to_string(),
        family: "moza-r5".to_string(),
        topology: topology_name,
        completion_state,
        declared_logical_roles,
    })
}

fn legacy_endpoint_selector_for_source(
    endpoints: &[serde_json::Value],
    source_endpoint: &str,
) -> Option<String> {
    let endpoint = endpoints.iter().find(|endpoint| {
        endpoint
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == source_endpoint)
    })?;
    let vendor_id = endpoint
        .get("vendor_id")
        .and_then(serde_json::Value::as_str)?;
    let product_id = endpoint
        .get("product_id")
        .and_then(serde_json::Value::as_str)?;
    let interface_number = endpoint
        .get("interface_number")
        .and_then(serde_json::Value::as_u64)?;
    let usage_page = endpoint
        .get("usage_page")
        .and_then(serde_json::Value::as_str)?;
    let usage = endpoint.get("usage").and_then(serde_json::Value::as_str)?;
    Some(format!(
        "hid-{}-{}-if{}-{}-{}",
        normalize_selector_hex(vendor_id),
        normalize_selector_hex(product_id),
        interface_number,
        normalize_selector_hex(usage_page),
        normalize_selector_hex(usage)
    ))
}

fn normalize_selector_hex(value: &str) -> String {
    format!(
        "0x{}",
        value.trim().trim_start_matches("0x").to_ascii_uppercase()
    )
}

fn set_hardware_lane_role_endpoint(
    lane: &Path,
    role: &str,
    endpoint: &str,
    json_out: Option<&Path>,
) -> Result<HardwareLaneRoleEndpointReceipt> {
    let role = normalize_role_id(role, "--role")?;
    let endpoint = validate_role_endpoint(endpoint, "--endpoint")?;
    let manifest_path = lane.join("hardware-lane-manifest.json");
    let mut manifest: serde_json::Value = read_json_file(&manifest_path)?;
    let family = manifest
        .get("family")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("hardware-lane-manifest.json missing family"))?
        .to_string();
    let topology = manifest
        .get("topology")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("hardware-lane-manifest.json missing topology"))?
        .to_string();
    let roles = manifest
        .get_mut("declared_logical_roles")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            anyhow::anyhow!("hardware-lane-manifest.json missing declared_logical_roles array")
        })?;
    let role_value = roles
        .iter_mut()
        .find(|candidate| {
            candidate
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|candidate_id| candidate_id == role)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "role '{role}' is not declared in {}; add it with hardware lane init role overrides before setting an endpoint",
                manifest_path.display()
            )
        })?;
    let previous_endpoint = role_value
        .get("expected_endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("declare-observed-endpoint")
        .to_string();
    role_value["expected_endpoint"] = serde_json::Value::String(endpoint.clone());
    write_json_file(&manifest_path, &manifest)?;

    let stored: StoredHardwareLaneScaffoldManifest = serde_json::from_value(manifest)
        .with_context(|| format!("failed to re-read updated '{}'", manifest_path.display()))?;
    let adapter = hardware_family_adapter_contract(&family)
        .with_context(|| format!("unknown hardware bring-up family '{family}'"))?;
    let logical_roles = stored_lane_roles_to_logical(&stored.declared_logical_roles);
    let checklist_path = lane.join("artifact-checklist.md");
    let capture_plan_path = lane.join("capture-plan.md");
    write_text_file(
        &checklist_path,
        &render_artifact_checklist(&adapter, &hardware_bringup_stages(), &logical_roles),
    )?;
    write_text_file(
        &capture_plan_path,
        &render_capture_plan(&adapter, &topology, &logical_roles),
    )?;

    let receipt = HardwareLaneRoleEndpointReceipt {
        success: true,
        command: "wheelctl hardware lane set-role-endpoint",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_output_reports: true,
        no_feature_reports: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
        lane: lane.display().to_string(),
        family,
        topology,
        role,
        previous_endpoint,
        expected_endpoint: endpoint,
        manifest_path: manifest_path.display().to_string(),
        updated_files: vec![
            manifest_path.display().to_string(),
            checklist_path.display().to_string(),
            capture_plan_path.display().to_string(),
        ],
        notes: vec![
            "role endpoint update edits lane scaffold metadata only".to_string(),
            "no HID device was opened and no output, feature, serial, firmware, or DFU command was sent".to_string(),
            "run hardware lane status again to refresh safe next commands".to_string(),
        ],
    };
    write_json_receipt(json_out, &receipt)?;
    Ok(receipt)
}

fn hardware_lane_scaffold_files(lane: &Path) -> Vec<HardwareLaneArtifactStatus> {
    [
        ("manifest", "hardware-lane-manifest.json"),
        ("stage_gates", "stage-gates.json"),
        ("artifact_checklist", "artifact-checklist.md"),
        ("capture_plan", "capture-plan.md"),
        ("lane_init_receipt", "lane-init.json"),
        ("captures_dir", "captures"),
    ]
    .into_iter()
    .map(|(kind, rel)| lane_artifact_status(lane, kind, rel))
    .collect()
}

fn stage_expected_artifacts(
    lane: &Path,
    stage: &HardwareBringupStage,
    roles: &[StoredHardwareLaneLogicalRole],
) -> Vec<HardwareLaneArtifactStatus> {
    let mut artifacts = match stage.id {
        "discovery" => vec![
            lane_artifact_status(lane, "receipt", "device-list.json"),
            lane_artifact_status(lane, "receipt", "hid-list.json"),
            lane_artifact_status(lane, "receipt", "hardware-doctor.json"),
            lane_artifact_status(lane, "receipt", "moza-probe.json"),
        ],
        "passive" => {
            let mut artifacts = vec![
                lane_artifact_status(lane, "receipt", "lane-capture-analysis.json"),
                lane_artifact_status(lane, "receipt", "parser-fixture-validation.json"),
            ];
            artifacts.extend(
                roles
                    .iter()
                    .filter(|role| role.required)
                    .map(|role| lane_artifact_status(lane, "capture", &role.evidence_artifact)),
            );
            artifacts
        }
        "descriptor_trust" => vec![lane_artifact_status(lane, "receipt", "descriptor.json")],
        "fixture_promotion" => {
            vec![lane_artifact_status(
                lane,
                "receipt",
                "fixture-promotion.json",
            )]
        }
        "pre_output_readiness" => vec![
            lane_artifact_status(lane, "receipt", "passive-verification.json"),
            lane_artifact_status(lane, "receipt", "lane-audit-passive.json"),
            lane_artifact_status(lane, "receipt", "pre-output-readiness.json"),
        ],
        "zero_torque" => vec![lane_artifact_status(
            lane,
            "receipt",
            "zero-torque-proof.json",
        )],
        "watchdog" => vec![lane_artifact_status(lane, "receipt", "watchdog-proof.json")],
        "disconnect" => vec![lane_artifact_status(
            lane,
            "receipt",
            "disconnect-proof.json",
        )],
        "openracing_control_ready" => vec![
            lane_artifact_status(lane, "receipt", "init-off.json"),
            lane_artifact_status(lane, "receipt", "init-standard.json"),
            lane_artifact_status(lane, "receipt", "moza-status.json"),
            lane_artifact_status(lane, "receipt", "device-status.json"),
            lane_artifact_status(lane, "receipt", "support-bundle.json"),
            lane_artifact_status(lane, "receipt", "low-torque-proof.json"),
            lane_artifact_status(lane, "receipt", "steering-angle-stream-proof.json"),
            lane_artifact_status(lane, "receipt", "native-actuator-profile-smoke.json"),
            lane_artifact_status(lane, "receipt", "openracing-control-verification.json"),
            lane_artifact_status(
                lane,
                "receipt",
                "manifest-promotion-openracing-control.json",
            ),
            lane_artifact_status(lane, "receipt", "lane-audit-openracing-control.json"),
        ],
        "native_response_ready" => vec![
            lane_artifact_status(lane, "receipt", "native-actuator-visible-smoke.json"),
            lane_artifact_status(lane, "receipt", "native-response-verification.json"),
            lane_artifact_status(lane, "receipt", "manifest-promotion-native-response.json"),
            lane_artifact_status(lane, "receipt", "lane-audit-native-response.json"),
        ],
        "native_visible_ready" => vec![
            lane_artifact_status(lane, "receipt", "native-visible-verification.json"),
            lane_artifact_status(lane, "receipt", "manifest-promotion-native-visible.json"),
            lane_artifact_status(lane, "receipt", "lane-audit-native-visible.json"),
            lane_artifact_status(
                lane,
                "receipt",
                "native-controlled-angle-attempt-03-smoke.json",
            ),
        ],
        "external_compat_ready" => vec![
            lane_artifact_status(lane, "receipt", "pit-house-coexistence.json"),
            lane_artifact_status(lane, "receipt", "simulator-telemetry-proof.json"),
        ],
        "bounded_ffb" => vec![lane_artifact_status(
            lane,
            "receipt",
            "simulator-ffb-smoke.json",
        )],
        "smoke_ready" => vec![
            lane_artifact_status(lane, "receipt", "smoke-ready-verification.json"),
            lane_artifact_status(lane, "receipt", "manifest-promotion-smoke-ready.json"),
            lane_artifact_status(lane, "receipt", "lane-audit-smoke-ready.json"),
        ],
        "ffb_extended" => vec![
            lane_artifact_status(lane, "receipt", "simulator-ffb-smoke.json"),
            lane_artifact_status(lane, "artifact", "regression-fixtures"),
        ],
        _ => Vec::new(),
    };
    artifacts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    artifacts
}

fn lane_artifact_status(lane: &Path, kind: &str, rel: &str) -> HardwareLaneArtifactStatus {
    HardwareLaneArtifactStatus {
        kind: kind.to_string(),
        relative_path: rel.to_string(),
        present: lane.join(rel).exists(),
    }
}

fn lane_artifact_explicit_failure(
    lane: &Path,
    stage_id: &str,
    artifact: &HardwareLaneArtifactStatus,
) -> bool {
    if !artifact.present || artifact.kind != "receipt" {
        return false;
    }
    if stage_id == "native_response_ready"
        && artifact.relative_path == "native-actuator-visible-smoke.json"
    {
        return false;
    }
    let Ok(value) = read_json_file::<serde_json::Value>(&lane.join(&artifact.relative_path)) else {
        return false;
    };
    value.get("success").and_then(serde_json::Value::as_bool) == Some(false)
}

fn lane_status_blocking_items(
    stage_status: &[HardwareLaneStageStatus],
    scaffold_required: bool,
    scaffold_complete: bool,
    missing_role_endpoints: &[String],
    verifier_receipt: &HardwareLaneVerifierReceiptStatus,
) -> Vec<String> {
    let mut items = Vec::new();
    if scaffold_required && !scaffold_complete {
        items.push("scaffold_files_missing".to_string());
    }
    if let Some(stage) = stage_status.iter().find(|stage| stage.artifacts_failed > 0) {
        items.push(format!("{}:failed_artifacts", stage.id));
    }
    if let Some(stage) = stage_status
        .iter()
        .find(|stage| stage.artifacts_missing > 0)
    {
        items.push(format!("{}:missing_artifacts", stage.id));
    }
    if !missing_role_endpoints.is_empty() {
        items.push("passive:missing_role_endpoints".to_string());
        items.extend(
            missing_role_endpoints
                .iter()
                .map(|role| format!("role_endpoint:{role}:missing")),
        );
    }
    if !verifier_receipt.present {
        items.push("verifier_receipt:passive-verification.json:missing".to_string());
    } else if !verifier_receipt.parseable {
        items.push("verifier_receipt:passive-verification.json:unparseable".to_string());
    } else {
        items.extend(
            verifier_receipt
                .failed_gates
                .iter()
                .map(|gate| format!("verifier_gate:{gate}:fail")),
        );
    }
    items.push("verifier_receipts_not_evaluated_by_status".to_string());
    items
}

fn lane_verifier_receipt_status(lane: &Path) -> HardwareLaneVerifierReceiptStatus {
    let relative_path = "passive-verification.json";
    let path = lane.join(relative_path);
    if !path.exists() {
        return HardwareLaneVerifierReceiptStatus {
            path: relative_path.to_string(),
            present: false,
            parseable: false,
            success: None,
            failed_gates: Vec::new(),
            stage_blocker: None,
            guidance: "passive verifier receipt is missing; run wheelctl moza verify-bundle before trusting later-stage guidance".to_string(),
        };
    }

    let Ok(receipt) = read_json_file::<serde_json::Value>(&path) else {
        return HardwareLaneVerifierReceiptStatus {
            path: relative_path.to_string(),
            present: true,
            parseable: false,
            success: None,
            failed_gates: Vec::new(),
            stage_blocker: None,
            guidance: "passive verifier receipt could not be parsed; refresh it before trusting later-stage guidance".to_string(),
        };
    };

    let success = receipt.get("success").and_then(serde_json::Value::as_bool);
    let failed_gates: Vec<_> = receipt
        .get("gates")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|gate| {
            let name = gate.get("name").and_then(serde_json::Value::as_str)?;
            let status = gate
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            (status != "pass").then(|| name.to_string())
        })
        .collect();
    let stage_blocker = failed_gates
        .iter()
        .filter_map(|gate| verifier_gate_stage_blocker(gate))
        .min_by_key(|stage| lane_stage_order(stage).unwrap_or(u8::MAX))
        .map(str::to_string);
    let guidance = if success == Some(true) {
        "passive verifier receipt reports success; lane status still inventories artifacts only"
            .to_string()
    } else if let Some(stage) = &stage_blocker {
        format!("passive verifier receipt has failing gates; earliest mapped blocker is {stage}")
    } else if failed_gates.is_empty() {
        "passive verifier receipt is present but has no gate summary; refresh it before trusting later-stage guidance".to_string()
    } else {
        "passive verifier receipt has failing gates that are not mapped to a rail stage; inspect verify-bundle output".to_string()
    };

    HardwareLaneVerifierReceiptStatus {
        path: relative_path.to_string(),
        present: true,
        parseable: true,
        success,
        failed_gates,
        stage_blocker,
        guidance,
    }
}

fn verifier_gate_stage_blocker(gate: &str) -> Option<&'static str> {
    match gate {
        "lane_directory"
        | "manifest_no_overclaim"
        | "manifest_r5_pid_consistency"
        | "moza_r5_observed"
        | "moza_topology_observed" => Some("discovery"),
        "topology_required_evidence_supported"
        | "passive_captures_parse"
        | "parser_fixture_validation" => Some("passive"),
        "descriptor_metadata" => Some("descriptor_trust"),
        "fixture_promotion" => Some("fixture_promotion"),
        "init_off_handshake"
        | "init_standard_handshake"
        | "service_status_receipts"
        | "low_torque_bounded"
        | "steering_angle_stream_proof"
        | "native_actuator_profile_smoke" => Some("openracing_control_ready"),
        "native_actuator_response_smoke" => Some("native_response_ready"),
        "native_actuator_visible_smoke" => Some("native_visible_ready"),
        "pit_house_coexistence" | "simulator_telemetry" => Some("external_compat_ready"),
        "simulator_ffb_bounded" => Some("bounded_ffb"),
        _ => None,
    }
}

fn lane_stage_order(stage: &str) -> Option<u8> {
    match stage {
        "discovery" => Some(0),
        "passive" => Some(1),
        "descriptor_trust" => Some(2),
        "fixture_promotion" => Some(3),
        "pre_output_readiness" => Some(4),
        "zero_torque" => Some(5),
        "watchdog" => Some(6),
        "disconnect" => Some(7),
        "openracing_control_ready" => Some(8),
        "native_response_ready" => Some(9),
        "native_visible_ready" => Some(10),
        "external_compat_ready" => Some(11),
        "bounded_ffb" => Some(12),
        "smoke_ready" => Some(13),
        "ffb_extended" => Some(14),
        _ => None,
    }
}

fn required_roles_with_placeholder_endpoints(
    roles: &[StoredHardwareLaneLogicalRole],
) -> Vec<String> {
    roles
        .iter()
        .filter(|role| role.required && !has_declared_endpoint(&role.expected_endpoint))
        .map(|role| role.id.clone())
        .collect()
}

fn lane_status_next_blocked_stage(
    first_missing_artifact_stage: Option<(&'static str, u8)>,
    first_failed_artifact_stage: Option<(&'static str, u8)>,
    missing_required_role_endpoint: bool,
    verifier_stage_blocker: Option<&str>,
) -> &'static str {
    let mut earliest = first_missing_artifact_stage;
    if let Some(stage) = first_failed_artifact_stage {
        earliest = earlier_stage(earliest, stage);
    }
    if missing_required_role_endpoint {
        earliest = earlier_stage(earliest, ("passive", 1));
    }
    if let Some(stage) = verifier_stage_blocker.and_then(verifier_stage_with_order) {
        earliest = earlier_stage(earliest, stage);
    }
    earliest.map_or("verifier_receipts", |(stage, _)| stage)
}

fn earlier_stage(
    current: Option<(&'static str, u8)>,
    candidate: (&'static str, u8),
) -> Option<(&'static str, u8)> {
    match current {
        Some((_, order)) if order <= candidate.1 => current,
        _ => Some(candidate),
    }
}

fn verifier_stage_with_order(stage: &str) -> Option<(&'static str, u8)> {
    match stage {
        "discovery" => Some(("discovery", 0)),
        "passive" => Some(("passive", 1)),
        "descriptor_trust" => Some(("descriptor_trust", 2)),
        "fixture_promotion" => Some(("fixture_promotion", 3)),
        "pre_output_readiness" => Some(("pre_output_readiness", 4)),
        "zero_torque" => Some(("zero_torque", 5)),
        "watchdog" => Some(("watchdog", 6)),
        "disconnect" => Some(("disconnect", 7)),
        "openracing_control_ready" => Some(("openracing_control_ready", 8)),
        "native_response_ready" => Some(("native_response_ready", 9)),
        "native_visible_ready" => Some(("native_visible_ready", 10)),
        "external_compat_ready" => Some(("external_compat_ready", 11)),
        "bounded_ffb" => Some(("bounded_ffb", 12)),
        "smoke_ready" => Some(("smoke_ready", 13)),
        "ffb_extended" => Some(("ffb_extended", 14)),
        _ => None,
    }
}

fn lane_status_safe_next_commands(
    lane: &Path,
    family: &str,
    next_blocked_stage: &str,
    roles: &[StoredHardwareLaneLogicalRole],
    descriptor_capture_tooling: &HardwareLaneDescriptorCaptureToolingStatus,
) -> Vec<String> {
    match (family, next_blocked_stage) {
        ("moza-r5", "discovery") => vec![
            format!(
                "wheelctl hardware doctor --json-out {}",
                lane_path_arg(lane, "hardware-doctor.json")
            ),
            format!(
                "wheelctl device list --hid-observe-only --json-out {} --json",
                lane_path_arg(lane, "device-list.json")
            ),
            format!(
                "hid-capture list --vendor 0x346E --json-out {}",
                lane_path_arg(lane, "hid-list.json")
            ),
            format!(
                "wheelctl moza probe --json-out {} --json",
                lane_path_arg(lane, "moza-probe.json")
            ),
        ],
        (_, "discovery") => vec![
            format!(
                "wheelctl hardware doctor --json-out {}",
                lane_path_arg(lane, "hardware-doctor.json")
            ),
            format!(
                "wheelctl device list --hid-observe-only --json-out {} --json",
                lane_path_arg(lane, "device-list.json")
            ),
        ],
        ("moza-r5", "passive") => {
            let mut commands = vec![
                format!(
                    "wheelctl moza analyze-lane --lane {} --json-out {} --json",
                    shell_path_arg(lane),
                    lane_path_arg(lane, "lane-capture-analysis.json")
                ),
                format!(
                    "wheelctl moza validate-captures --lane {} --json-out {} --json",
                    shell_path_arg(lane),
                    lane_path_arg(lane, "parser-fixture-validation.json")
                ),
            ];
            commands.extend(
                roles
                    .iter()
                    .filter(|role| {
                        role.required
                            && !lane.join(&role.evidence_artifact).exists()
                            && has_declared_endpoint(&role.expected_endpoint)
                    })
                    .map(|role| {
                        format!(
                            "wheelctl moza capture-input --device {} --duration-ms {} --json-out {} --json",
                            role.expected_endpoint,
                            passive_capture_duration_ms(role),
                            lane_path_arg(lane, &role.evidence_artifact)
                        )
                    }),
            );
            commands.extend(
                roles
                    .iter()
                    .filter(|role| {
                        role.required && !has_declared_endpoint(&role.expected_endpoint)
                    })
                    .map(|role| {
                        format!(
                            "wheelctl hardware lane set-role-endpoint --lane {} --role {} --endpoint <observed-endpoint-selector> --json-out {} --json",
                            shell_path_arg(lane),
                            role.id,
                            lane_path_arg(lane, &format!("role-endpoint-{}.json", role.id))
                        )
                    }),
            );
            commands
        }
        ("moza-r5", "descriptor_trust") => {
            let selector = moza_descriptor_selector(roles);
            let mut commands = Vec::new();
            if descriptor_capture_tooling.usbpcap_extractor_guidance_available() {
                commands.push(
                    "powershell -ExecutionPolicy Bypass -File scripts/extract_usbpcap_report_descriptor.ps1 -InputPcapng target/moza-r5-usbpcap-enumeration.pcapng -Output target/moza-r5-report-descriptor.txt -InterfaceNumber 2".to_string(),
                );
            }
            commands.extend([
                format!(
                    "wheelctl moza descriptor --device {selector} --report-descriptor-hex-file target/moza-r5-report-descriptor.txt --json-out {} --json",
                    lane_path_arg(lane, "descriptor.json")
                ),
                format!(
                    "wheelctl moza descriptor --device {selector} --report-descriptor-bin-file target/moza-r5-report-descriptor.bin --json-out {} --json",
                    lane_path_arg(lane, "descriptor.json")
                ),
            ]);
            commands
        }
        ("moza-r5", "fixture_promotion") => vec![
            format!(
                "wheelctl moza validate-captures --lane {} --json-out {} --json",
                shell_path_arg(lane),
                lane_path_arg(lane, "parser-fixture-validation.json")
            ),
            format!(
                "wheelctl moza verify-bundle --lane {} --stage passive --json-out {} --json",
                shell_path_arg(lane),
                lane_path_arg(lane, "passive-verification.json")
            ),
        ],
        ("moza-r5", "pre_output_readiness") => vec![
            format!(
                "wheelctl moza verify-bundle --lane {} --stage passive --json-out {} --json",
                shell_path_arg(lane),
                lane_path_arg(lane, "passive-verification.json")
            ),
            format!(
                "wheelctl moza audit-lane --lane {} --stage passive --json-out {} --json",
                shell_path_arg(lane),
                lane_path_arg(lane, "lane-audit-passive.json")
            ),
            format!(
                "wheelctl moza pre-output-readiness --lane {} --json-out {} --json",
                shell_path_arg(lane),
                lane_path_arg(lane, "pre-output-readiness.json")
            ),
        ],
        (
            _,
            "zero_torque"
            | "watchdog"
            | "disconnect"
            | "openracing_control_ready"
            | "native_response_ready"
            | "native_visible_ready"
            | "external_compat_ready"
            | "bounded_ffb"
            | "smoke_ready"
            | "ffb_extended",
        ) => Vec::new(),
        _ => Vec::new(),
    }
}

fn lane_descriptor_capture_tooling_status(
    lane: &Path,
) -> HardwareLaneDescriptorCaptureToolingStatus {
    let path = lane.join("hardware-doctor.json");
    if !path.exists() {
        return HardwareLaneDescriptorCaptureToolingStatus {
            hardware_doctor_present: false,
            hardware_doctor_parseable: false,
            tshark_present: None,
            usbpcap_interfaces_present: None,
            usbpcap_interface_count: None,
            ready_for_usbpcap_descriptor_capture: None,
            guidance: "run wheelctl hardware doctor to inventory descriptor capture tooling"
                .to_string(),
        };
    }

    let Ok(receipt) = read_json_file::<serde_json::Value>(&path) else {
        return HardwareLaneDescriptorCaptureToolingStatus {
            hardware_doctor_present: true,
            hardware_doctor_parseable: false,
            tshark_present: None,
            usbpcap_interfaces_present: None,
            usbpcap_interface_count: None,
            ready_for_usbpcap_descriptor_capture: None,
            guidance: "hardware-doctor.json could not be parsed; refresh it before descriptor capture planning"
                .to_string(),
        };
    };

    let capture = receipt
        .get("tools")
        .and_then(|tools| tools.get("usbpcap_descriptor_capture"));
    let tshark_present = capture
        .and_then(|value| value.get("tshark_present"))
        .and_then(serde_json::Value::as_bool);
    let usbpcap_interfaces_present = capture
        .and_then(|value| value.get("usbpcap_interfaces_present"))
        .and_then(serde_json::Value::as_bool);
    let usbpcap_interface_count = capture
        .and_then(|value| value.get("usbpcap_interface_count"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok());
    let ready_for_usbpcap_descriptor_capture = capture
        .and_then(|value| value.get("ready_for_usbpcap_descriptor_capture"))
        .and_then(serde_json::Value::as_bool);
    let access_guidance = capture
        .and_then(|value| value.get("access_guidance"))
        .and_then(serde_json::Value::as_str);
    let guidance = match ready_for_usbpcap_descriptor_capture {
        Some(true) => {
            "USBPcap/Wireshark capture interfaces are available for descriptor enumeration capture"
                .to_string()
        }
        Some(false) => access_guidance
            .unwrap_or("USBPcap/Wireshark capture interfaces are unavailable; use native Linux/sysfs, install USBPcap intentionally, or import descriptor bytes from another trusted raw HID descriptor source")
            .to_string(),
        None => {
            "hardware-doctor.json does not include USBPcap descriptor tooling status; refresh hardware doctor for host-aware guidance"
                .to_string()
        }
    };

    HardwareLaneDescriptorCaptureToolingStatus {
        hardware_doctor_present: true,
        hardware_doctor_parseable: true,
        tshark_present,
        usbpcap_interfaces_present,
        usbpcap_interface_count,
        ready_for_usbpcap_descriptor_capture,
        guidance,
    }
}

fn passive_capture_duration_ms(role: &StoredHardwareLaneLogicalRole) -> u64 {
    match role.id.as_str() {
        "idle" | "aggregated_idle" => 5_000,
        _ => 10_000,
    }
}

fn moza_descriptor_selector(roles: &[StoredHardwareLaneLogicalRole]) -> &str {
    roles
        .iter()
        .find(|role| {
            role.id == "steering"
                && role.connection_path == "wheelbase_hub"
                && has_declared_endpoint(&role.expected_endpoint)
        })
        .or_else(|| {
            roles.iter().find(|role| {
                role.connection_path == "wheelbase_hub"
                    && has_declared_endpoint(&role.expected_endpoint)
            })
        })
        .map_or("hid-0x346E-0x0004-if2-0x0001-0x0004", |role| {
            role.expected_endpoint.as_str()
        })
}

fn has_declared_endpoint(endpoint: &str) -> bool {
    let endpoint = endpoint.trim();
    !endpoint.is_empty()
        && endpoint != "declare-observed-endpoint"
        && endpoint != "<observed-endpoint-selector>"
}

fn lane_path_arg(lane: &Path, relative: &str) -> String {
    shell_path_arg(&lane.join(relative))
}

fn shell_path_arg(path: &Path) -> String {
    let text = path.display().to_string();
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text
    }
}

#[derive(Debug, Default)]
struct HardwareLaneRoleOverrides {
    required_roles: BTreeSet<String>,
    optional_roles: BTreeSet<String>,
    role_artifacts: BTreeMap<String, String>,
    role_endpoints: BTreeMap<String, String>,
    role_connections: BTreeMap<String, String>,
}

impl HardwareLaneRoleOverrides {
    fn from_cli(
        required_roles: &[String],
        optional_roles: &[String],
        role_artifacts: &[String],
        role_endpoints: &[String],
        role_connections: &[String],
    ) -> Result<Self> {
        let required_roles = parse_role_set(required_roles, "--required-role")?;
        let optional_roles = parse_role_set(optional_roles, "--optional-role")?;
        if let Some(role) = required_roles.intersection(&optional_roles).next() {
            anyhow::bail!("role '{role}' cannot be both required and optional");
        }
        let role_artifacts = parse_role_kv_entries(role_artifacts, "--role-artifact")?;
        let role_endpoints = parse_role_kv_entries(role_endpoints, "--role-endpoint")?;
        let role_connections = parse_role_kv_entries(role_connections, "--role-connection")?;
        for artifact in role_artifacts.values() {
            validate_relative_artifact_path(artifact)?;
        }
        for connection in role_connections.values() {
            validate_connection_path(connection)?;
        }
        Ok(Self {
            required_roles,
            optional_roles,
            role_artifacts,
            role_endpoints,
            role_connections,
        })
    }

    fn referenced_roles(&self) -> BTreeSet<String> {
        self.required_roles
            .iter()
            .chain(self.optional_roles.iter())
            .chain(self.role_artifacts.keys())
            .chain(self.role_endpoints.keys())
            .chain(self.role_connections.keys())
            .cloned()
            .collect()
    }
}

fn parse_role_set(values: &[String], flag: &str) -> Result<BTreeSet<String>> {
    values
        .iter()
        .map(|value| normalize_role_id(value, flag))
        .collect()
}

fn parse_role_kv_entries(values: &[String], flag: &str) -> Result<BTreeMap<String, String>> {
    let mut entries = BTreeMap::new();
    for value in values {
        let (role, item) = value
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("{flag} entries must use role=value syntax"))?;
        let role = normalize_role_id(role, flag)?;
        let item = item.trim();
        if item.is_empty() {
            anyhow::bail!("{flag} entry for role '{role}' has an empty value");
        }
        if entries.insert(role.clone(), item.to_string()).is_some() {
            anyhow::bail!("{flag} specified more than once for role '{role}'");
        }
    }
    Ok(entries)
}

fn normalize_role_id(value: &str, flag: &str) -> Result<String> {
    let role = value.trim();
    if role.is_empty() {
        anyhow::bail!("{flag} role id cannot be empty");
    }
    if !role
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        anyhow::bail!(
            "{flag} role id '{role}' may contain only ASCII letters, numbers, '_' or '-'"
        );
    }
    Ok(role.to_string())
}

fn validate_role_endpoint(value: &str, flag: &str) -> Result<String> {
    let endpoint = value.trim();
    if !has_declared_endpoint(endpoint) {
        anyhow::bail!("{flag} must be an observed endpoint selector, not a placeholder");
    }
    if endpoint.chars().any(|ch| ch == '\r' || ch == '\n') {
        anyhow::bail!("{flag} must not contain line breaks");
    }
    Ok(endpoint.to_string())
}

fn stored_lane_roles_to_logical(
    roles: &[StoredHardwareLaneLogicalRole],
) -> Vec<HardwareLaneLogicalRole> {
    roles
        .iter()
        .map(|role| HardwareLaneLogicalRole {
            id: role.id.clone(),
            required: role.required,
            connection_path: role.connection_path.clone(),
            expected_endpoint: role.expected_endpoint.clone(),
            evidence_artifact: role.evidence_artifact.clone(),
            semantic_status: role.semantic_status.clone(),
        })
        .collect()
}

fn validate_relative_artifact_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute() {
        anyhow::bail!("role artifact paths must be relative to the lane directory");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        anyhow::bail!("role artifact paths must stay within the lane directory");
    }
    Ok(())
}

fn validate_connection_path(connection: &str) -> Result<()> {
    if matches!(
        connection,
        "wheelbase_hub" | "standalone_usb" | "cross_device" | "unknown"
    ) {
        return Ok(());
    }
    anyhow::bail!(
        "role connection '{connection}' must be one of wheelbase_hub, standalone_usb, cross_device, unknown"
    )
}

fn lane_roles(
    adapter: &HardwareFamilyAdapterContract,
    topology: &str,
    overrides: &HardwareLaneRoleOverrides,
) -> Result<Vec<HardwareLaneLogicalRole>> {
    let mut roles = default_lane_roles(adapter, topology);
    let mut known: BTreeSet<String> = roles.iter().map(|role| role.id.clone()).collect();
    for role_id in overrides.referenced_roles() {
        if !known.contains(&role_id)
            && !overrides.required_roles.contains(&role_id)
            && !overrides.optional_roles.contains(&role_id)
        {
            anyhow::bail!(
                "role override references unknown role '{role_id}'; declare it with --required-role or --optional-role"
            );
        }
        if !known.contains(&role_id) {
            let required = overrides.required_roles.contains(&role_id);
            roles.push(default_lane_role(adapter.id, topology, &role_id, required));
            known.insert(role_id);
        }
    }

    for role in &mut roles {
        if overrides.required_roles.contains(&role.id) {
            role.required = true;
        }
        if overrides.optional_roles.contains(&role.id) {
            role.required = false;
        }
        if let Some(connection) = overrides.role_connections.get(&role.id) {
            role.connection_path.clone_from(connection);
        }
        if let Some(endpoint) = overrides.role_endpoints.get(&role.id) {
            role.expected_endpoint.clone_from(endpoint);
        }
        if let Some(artifact) = overrides.role_artifacts.get(&role.id) {
            role.evidence_artifact.clone_from(artifact);
        }
    }

    Ok(roles)
}

fn default_lane_roles(
    adapter: &HardwareFamilyAdapterContract,
    topology: &str,
) -> Vec<HardwareLaneLogicalRole> {
    adapter
        .default_logical_controls
        .iter()
        .map(|control| {
            let (role_id, required) = control
                .strip_suffix("_optional")
                .map_or((*control, true), |role| (role, false));
            default_lane_role(adapter.id, topology, role_id, required)
        })
        .collect()
}

fn default_lane_role(
    adapter_id: &str,
    topology: &str,
    role_id: &str,
    required: bool,
) -> HardwareLaneLogicalRole {
    HardwareLaneLogicalRole {
        id: role_id.to_string(),
        required,
        connection_path: default_connection_path(adapter_id, topology, role_id),
        expected_endpoint: default_expected_endpoint(adapter_id, role_id),
        evidence_artifact: default_role_evidence_artifact(adapter_id, role_id),
        semantic_status: "pending_capture".to_string(),
    }
}

fn default_connection_path(adapter_id: &str, topology: &str, role_id: &str) -> String {
    let normalized = topology.replace('-', "_");
    if normalized == "wheelbase_hub" || normalized == "r5_hub" {
        return "wheelbase_hub".to_string();
    }
    if normalized == "standalone_usb" {
        return "standalone_usb".to_string();
    }
    if adapter_id == "moza-r5"
        && matches!(
            role_id,
            "steering" | "rim_controls" | "throttle" | "brake" | "clutch" | "handbrake"
        )
    {
        return "wheelbase_hub".to_string();
    }
    "unknown".to_string()
}

fn default_expected_endpoint(adapter_id: &str, role_id: &str) -> String {
    if adapter_id == "moza-r5"
        && matches!(
            role_id,
            "steering" | "rim_controls" | "throttle" | "brake" | "clutch" | "handbrake"
        )
    {
        return "hid-0x346E-0x0004-if2-0x0001-0x0004".to_string();
    }
    "declare-observed-endpoint".to_string()
}

fn default_role_evidence_artifact(adapter_id: &str, role_id: &str) -> String {
    if adapter_id == "moza-r5" {
        match role_id {
            "steering" => "captures/r5-steering-sweep.jsonl",
            "rim_controls" => "captures/declared-rim-controls.jsonl",
            "throttle" => "captures/r5-throttle-only-sweep.jsonl",
            "brake" => "captures/r5-brake-only-sweep.jsonl",
            "clutch" => "captures/r5-clutch-only-sweep.jsonl",
            "handbrake" => "captures/r5-handbrake-only-sweep.jsonl",
            _ => return format!("captures/{role_id}.jsonl"),
        }
        .to_string()
    } else {
        format!("captures/{role_id}.jsonl")
    }
}

fn render_artifact_checklist(
    adapter: &HardwareFamilyAdapterContract,
    stages: &[HardwareBringupStage],
    roles: &[HardwareLaneLogicalRole],
) -> String {
    let mut out = String::new();
    out.push_str("# Hardware Lane Artifact Checklist\n\n");
    out.push_str("This file is a scaffold. It is not evidence by itself.\n\n");
    out.push_str(&format!("Device family: `{}`\n\n", adapter.id));
    out.push_str("## Logical Roles\n\n");
    out.push_str("| Role | Required | Connection path | Endpoint | Evidence artifact | Status |\n");
    out.push_str("|------|----------|-----------------|----------|-------------------|--------|\n");
    for role in roles {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            role.id,
            role.required,
            role.connection_path,
            role.expected_endpoint,
            role.evidence_artifact,
            role.semantic_status
        ));
    }
    out.push_str("\n## Stage Artifacts\n\n");
    for stage in stages {
        out.push_str(&format!("### {}. `{}`\n\n", stage.order, stage.id));
        out.push_str(&format!("{}\n\n", stage.purpose));
        out.push_str("Required artifacts:\n");
        for artifact in &stage.required_artifacts {
            out.push_str(&format!("- `{artifact}`\n"));
        }
        out.push('\n');
    }
    out.push_str("Do not create fake receipt files to satisfy this checklist.\n");
    out
}

fn render_capture_plan(
    adapter: &HardwareFamilyAdapterContract,
    topology: &str,
    roles: &[HardwareLaneLogicalRole],
) -> String {
    let mut out = String::new();
    out.push_str("# Hardware Lane Capture Plan\n\n");
    out.push_str(&format!("Device family: `{}`\n", adapter.id));
    out.push_str(&format!("Topology: `{topology}`\n\n"));
    out.push_str("Capture one declared role at a time. Keep output paths closed.\n\n");
    for role in roles {
        out.push_str(&format!("## `{}`\n\n", role.id));
        out.push_str(&format!("Required: `{}`\n\n", role.required));
        out.push_str(&format!(
            "Expected endpoint: `{}`\n\n",
            role.expected_endpoint
        ));
        out.push_str(&format!(
            "Evidence artifact: `{}`\n\n",
            role.evidence_artifact
        ));
        out.push_str("Gesture: idle, move only this role slowly through its range, idle.\n\n");
    }
    out.push_str("Forbidden during capture: torque, FFB, direct mode, output reports, feature reports, serial config, firmware, and DFU.\n");
    out
}

fn hardware_bringup_stages() -> Vec<HardwareBringupStage> {
    vec![
        HardwareBringupStage {
            id: "discovery",
            order: 0,
            purpose: "observe attached endpoints and stable identity before any device-specific claim",
            required_artifacts: vec![
                "device-list.json",
                "hid-list.json",
                "hardware-doctor.json",
                "probe/status/support receipts",
            ],
            required_gates: vec![
                "endpoint_identity_observed",
                "output_capable_endpoint_selection_explicit",
            ],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "wheelctl hardware doctor --json-out <lane>/hardware-doctor.json",
                "wheelctl device list --hid-observe-only --json-out <lane>/device-list.json",
            ],
            operator_actions: vec!["declare topology and logical roles for this lane"],
            ready_outputs: vec!["stable_endpoint_selector"],
            adapter_requirement_refs: vec!["known_vid_pids", "known_endpoint_roles"],
        },
        HardwareBringupStage {
            id: "passive",
            order: 1,
            purpose: "prove declared logical controls with observe-only captures",
            required_artifacts: vec![
                "idle capture",
                "per-role captures",
                "lane-capture-analysis.json",
                "parser-fixture-validation.json",
            ],
            required_gates: vec![
                "declared_required_roles_parser_visible",
                "optional_absent_roles_not_required",
                "virtual_evidence_rejected",
            ],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "capture each declared role through its declared endpoint/path",
                "validate captures with the family parser",
            ],
            operator_actions: vec!["move exactly one declared control per isolated capture"],
            ready_outputs: vec!["role_evidence_complete", "parser_validation_passed"],
            adapter_requirement_refs: vec![
                "default_logical_controls",
                "passive_capture_requirements",
            ],
        },
        HardwareBringupStage {
            id: "descriptor_trust",
            order: 2,
            purpose: "trust raw HID report descriptor bytes and report metadata before output-adjacent work",
            required_artifacts: vec![
                "descriptor.json",
                "raw report descriptor bytes",
                "descriptor CRC",
            ],
            required_gates: vec![
                "descriptor_source_trusted",
                "report_descriptor_crc32_present",
                "metadata_matches_selected_endpoint",
                "invalid_descriptor_blobs_rejected",
            ],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "import raw descriptor bytes or trusted descriptor hex for the selected endpoint",
            ],
            operator_actions: vec![
                "obtain raw descriptor bytes from OS/tooling without firmware or config changes",
            ],
            ready_outputs: vec!["descriptor_metadata_trusted"],
            adapter_requirement_refs: vec!["report_descriptor_expectations"],
        },
        HardwareBringupStage {
            id: "fixture_promotion",
            order: 3,
            purpose: "freeze known-good passive evidence as parser fixtures after descriptor trust",
            required_artifacts: vec!["fixture-promotion.json", "protocol parser fixtures"],
            required_gates: vec![
                "descriptor_trust_passed",
                "fixtures_replay_through_parser",
                "fixture_pid_topology_consistency",
            ],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec!["promote validated passive captures into protocol fixtures"],
            operator_actions: vec![],
            ready_outputs: vec!["fixture_replay_green"],
            adapter_requirement_refs: vec!["parser_fixture_requirements"],
        },
        HardwareBringupStage {
            id: "pre_output_readiness",
            order: 4,
            purpose: "collate passive, descriptor, fixtures, status, support, and audit state before any output-adjacent stage",
            required_artifacts: vec![
                "passive-verification.json",
                "lane-audit-passive.json",
                "pre-output-readiness.json",
                "status/support no-output receipts",
            ],
            required_gates: vec![
                "passive_verification_passed",
                "passive_audit_passed",
                "status_receipts_no_output",
                "ready_for_zero_torque_true",
                "ready_for_ffb_false",
            ],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec!["wheelctl <family> pre-output-readiness --lane <lane>"],
            operator_actions: vec!["stop if ready_for_zero_torque is false"],
            ready_outputs: vec!["ready_for_zero_torque"],
            adapter_requirement_refs: vec!["zero_torque_eligibility"],
        },
        HardwareBringupStage {
            id: "zero_torque",
            order: 5,
            purpose: "prove output plumbing with zero torque only",
            required_artifacts: vec![
                "zero-torque-proof.json",
                "explicit endpoint selector",
                "write log",
            ],
            required_gates: vec![
                "operator_confirmed",
                "zero_output_only",
                "no_nonzero_torque",
                "bounded_duration",
                "watchdog_armed",
            ],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "run the family zero-torque command only after pre-output readiness passes",
            ],
            operator_actions: vec!["operator present, wheel clear, kill path known"],
            ready_outputs: vec!["zero_torque_verified"],
            adapter_requirement_refs: vec!["zero_torque_eligibility", "known_output_reports"],
        },
        HardwareBringupStage {
            id: "watchdog",
            order: 6,
            purpose: "prove timeout/fail-closed behavior for the zero-output path",
            required_artifacts: vec!["watchdog-proof.json"],
            required_gates: vec!["watchdog_triggered", "final_zero_last", "no_nonzero_torque"],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec!["run watchdog proof after zero-torque proof"],
            operator_actions: vec!["keep wheel clear and observe fail-closed behavior"],
            ready_outputs: vec!["watchdog_fail_closed"],
            adapter_requirement_refs: vec!["watchdog_expectations"],
        },
        HardwareBringupStage {
            id: "disconnect",
            order: 7,
            purpose: "prove device-loss behavior cannot leave stale output state",
            required_artifacts: vec!["disconnect-proof.json"],
            required_gates: vec![
                "disconnect_observed",
                "final_zero_attempted",
                "no_nonzero_torque",
            ],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec!["run disconnect proof after zero-torque proof"],
            operator_actions: vec!["perform only the declared disconnect action"],
            ready_outputs: vec!["disconnect_fail_closed"],
            adapter_requirement_refs: vec!["disconnect_expectations"],
        },
        HardwareBringupStage {
            id: "openracing_control_ready",
            order: 8,
            purpose: "prove the OpenRacing-owned native control foundation before visible motion",
            required_artifacts: vec![
                "init-off.json",
                "init-standard.json",
                "low-torque-proof.json",
                "steering-angle-stream-proof.json",
                "native-actuator-profile-smoke.json",
                "status/support receipts",
                "openracing-control verification, promotion, and audit",
            ],
            required_gates: vec![
                "init_off_handshake",
                "init_standard_handshake",
                "service_status_receipts",
                "low_torque_bounded",
                "steering_angle_stream_proof",
                "native_actuator_profile_smoke",
            ],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "run family native-control commands only through their verifier-generated gates",
            ],
            operator_actions: vec!["preserve native-control receipts and status/support evidence"],
            ready_outputs: vec!["openracing_control_ready"],
            adapter_requirement_refs: vec!["zero_torque_eligibility", "known_output_reports"],
        },
        HardwareBringupStage {
            id: "native_response_ready",
            order: 9,
            purpose: "prove bounded native PIDFF output creates measurable steering response",
            required_artifacts: vec![
                "native-actuator-visible-smoke.json",
                "native-response-verification.json",
                "manifest-promotion-native-response.json",
                "lane-audit-native-response.json",
            ],
            required_gates: vec!["native_actuator_response_smoke"],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "verify and promote native-response only after response receipt passes",
            ],
            operator_actions: vec!["do not claim visible motion from response-only movement"],
            ready_outputs: vec!["native_response_ready"],
            adapter_requirement_refs: vec!["native_response_evidence"],
        },
        HardwareBringupStage {
            id: "native_visible_ready",
            order: 10,
            purpose: "prove operator-visible native controlled movement without external app prerequisites",
            required_artifacts: vec![
                "native-visible-verification.json",
                "manifest-promotion-native-visible.json",
                "lane-audit-native-visible.json",
                "native controlled-angle output receipt",
            ],
            required_gates: vec!["native_actuator_visible_smoke"],
            forbidden_actions: POST_PASSIVE_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "authorize exactly one reviewed native visible-motion attempt only after command-bound bench-clear",
            ],
            operator_actions: vec!["stop after one output attempt and preserve the receipt"],
            ready_outputs: vec!["native_visible_ready"],
            adapter_requirement_refs: vec!["native_visible_evidence"],
        },
        HardwareBringupStage {
            id: "external_compat_ready",
            order: 11,
            purpose: "prove external app and telemetry compatibility without making it a native-control prerequisite",
            required_artifacts: vec![
                "pit-house-coexistence.json",
                "simulator-telemetry-proof.json",
            ],
            required_gates: vec!["pit_house_coexistence", "simulator_telemetry"],
            forbidden_actions: COMMON_FORBIDDEN_ACTIONS.to_vec(),
            next_commands: vec![
                "record Pit House coexistence and simulator telemetry as external compatibility evidence",
            ],
            operator_actions: vec![
                "do not use external compatibility receipts as native-control proof",
            ],
            ready_outputs: vec!["external_compat_ready"],
            adapter_requirement_refs: vec!["external_compatibility_requirements"],
        },
        HardwareBringupStage {
            id: "bounded_ffb",
            order: 12,
            purpose: "first real-force smoke under explicit force and duration caps",
            required_artifacts: vec!["simulator-ffb-smoke.json", "bounded FFB output log"],
            required_gates: vec![
                "zero_watchdog_disconnect_passed",
                "native_visible_ready",
                "simulator_telemetry",
                "low_force_cap",
                "short_duration_cap",
                "manual_operator_present",
                "no_escalation",
            ],
            forbidden_actions: vec![
                "direct_mode_without_gate",
                "high_torque_without_stage",
                "feature_reports_without_stage",
                "firmware_dfu",
                "serial_config",
            ],
            next_commands: vec!["run bounded FFB only after zero/watchdog/disconnect gates pass"],
            operator_actions: vec!["operator present, wheel clear, kill path known"],
            ready_outputs: vec!["bounded_ffb_smoke_ready"],
            adapter_requirement_refs: vec!["ffb_eligibility", "known_unsafe_surfaces"],
        },
        HardwareBringupStage {
            id: "smoke_ready",
            order: 13,
            purpose: "promote the bounded hardware smoke lane after native, external, telemetry, and FFB gates pass",
            required_artifacts: vec![
                "smoke-ready-verification.json",
                "manifest-promotion-smoke-ready.json",
                "lane-audit-smoke-ready.json",
            ],
            required_gates: vec![
                "native_visible_ready",
                "pit_house_coexistence",
                "simulator_telemetry",
                "simulator_ffb_bounded",
            ],
            forbidden_actions: vec![
                "release_ready_claim",
                "high_torque_without_stage",
                "firmware_dfu",
                "serial_config_without_stage",
            ],
            next_commands: vec![
                "promote smoke-ready only after verify-bundle --stage smoke-ready passes",
            ],
            operator_actions: vec!["audit all smoke-ready claims before promotion"],
            ready_outputs: vec!["real_hardware_smoke_ready"],
            adapter_requirement_refs: vec!["smoke_ready_requirements"],
        },
        HardwareBringupStage {
            id: "ffb_extended",
            order: 14,
            purpose: "expand from smoke to longer simulator and effect coverage",
            required_artifacts: vec![
                "simulator-ffb-smoke.json",
                "timing/latency receipts",
                "regression fixtures",
            ],
            required_gates: vec![
                "bounded_ffb_passed",
                "effect_matrix_covered",
                "timing_within_bounds",
                "release_claims_audited",
            ],
            forbidden_actions: vec![
                "direct_mode_without_gate",
                "high_torque_without_stage",
                "feature_reports_without_stage",
                "firmware_dfu",
                "serial_config_without_stage",
            ],
            next_commands: vec!["extend coverage only after bounded FFB smoke is green"],
            operator_actions: vec!["monitor thermals/power where relevant"],
            ready_outputs: vec!["release_candidate_hardware_evidence"],
            adapter_requirement_refs: vec!["extended_ffb_requirements"],
        },
    ]
}

fn hardware_family_adapter_contract(family: &str) -> Result<HardwareFamilyAdapterContract> {
    match family {
        "generic-wheelbase" => Ok(generic_wheelbase_adapter_contract()),
        "moza-r5" => Ok(moza_r5_adapter_contract()),
        _ => anyhow::bail!("supported families: generic-wheelbase, moza-r5"),
    }
}

fn generic_wheelbase_adapter_contract() -> HardwareFamilyAdapterContract {
    HardwareFamilyAdapterContract {
        id: "generic-wheelbase",
        display_name: "Generic FFB-capable wheelbase",
        known_vid_pids: Vec::new(),
        known_endpoint_roles: vec!["wheelbase_output_endpoint", "input_endpoint"],
        default_logical_controls: vec!["steering", "rim_controls", "throttle", "brake"],
        report_descriptor_expectations: vec![
            "raw HID report descriptor bytes required before output-adjacent work",
            "input/output/feature report IDs must come from trusted descriptor or protocol adapter",
        ],
        passive_capture_requirements: vec![
            "idle capture",
            "one isolated capture per required logical role declared by the lane profile",
        ],
        parser_fixture_requirements: vec![
            "parser-visible movement for declared roles",
            "fixtures replay without virtual/synthetic hardware claims",
        ],
        output_capability: "adapter-declared; output endpoints must be explicitly selected",
        zero_torque_eligibility: "requires descriptor trust, passive/audit green, and adapter zero-output encoder",
        ffb_eligibility: "requires zero/watchdog/disconnect proof plus bounded-force adapter support",
        known_unsafe_surfaces: vec![
            "nonzero_torque",
            "direct_mode",
            "feature_reports",
            "serial_config",
            "firmware_dfu",
        ],
    }
}

fn moza_r5_adapter_contract() -> HardwareFamilyAdapterContract {
    HardwareFamilyAdapterContract {
        id: "moza-r5",
        display_name: "Moza R5 wheelbase hub",
        known_vid_pids: vec!["0x346E:0x0004", "0x346E:0x0014"],
        known_endpoint_roles: vec![
            "wheelbase_hub",
            "steering",
            "rim_controls",
            "pedals_through_hub",
            "handbrake_through_hub",
        ],
        default_logical_controls: vec![
            "steering",
            "rim_controls",
            "throttle",
            "brake",
            "clutch_optional",
            "handbrake_optional",
        ],
        report_descriptor_expectations: vec![
            "selected R5 HID endpoint must have trusted raw report descriptor bytes and CRC",
            "Windows HidP KDR collection blobs are not report descriptor evidence",
            "R5 V1 live input report 0x01 is 42 bytes when using the observed extended hub path",
        ],
        passive_capture_requirements: vec![
            "R5 idle",
            "steering sweep",
            "isolated through-R5 captures for declared pedals/handbrake roles",
            "rim controls only for the mounted rim declared by the lane profile",
        ],
        parser_fixture_requirements: vec![
            "R5 V1 throttle bytes 5-6 replay as throttle when present",
            "generic aux evidence remains generic unless isolated role captures prove semantics",
            "fixture promotion waits for descriptor trust",
        ],
        output_capability: "R5 wheelbase is output-capable, but output is locked behind explicit endpoint selection and staged receipts",
        zero_torque_eligibility: "requires passive verify/audit, descriptor CRC, fixture promotion, pre-output readiness, and zero report 0x20 encoder",
        ffb_eligibility: "native bounded FFB requires zero-torque, watchdog, disconnect, low-torque, native-visible, and simulator telemetry receipts; Pit House is external compatibility for smoke-ready, not a native FFB prerequisite",
        known_unsafe_surfaces: vec![
            "nonzero_torque",
            "direct_mode",
            "high_torque",
            "feature_reports",
            "serial_config",
            "firmware_dfu",
            "operator_override_for_output",
        ],
    }
}

fn build_doctor_receipt() -> HardwareDoctorReceipt {
    let registry = DeviceCapabilityRegistry::openracing_defaults();
    let tools = ToolChecks {
        hid_capture_on_path: executable_on_path("hid-capture"),
        wheelctl_self_check: true,
        usbpcap_descriptor_capture: inspect_usbpcap_descriptor_capture_tools(),
    };
    let hid = inspect_hid(&registry);
    let vendor_apps = detect_vendor_apps();
    let windows_pnp = inspect_windows_pnp();

    build_doctor_receipt_from_checks(tools, hid, vendor_apps, windows_pnp)
}

fn build_doctor_receipt_from_checks(
    tools: ToolChecks,
    hid: HidChecks,
    vendor_apps: VendorAppChecks,
    windows_pnp: WindowsPnpChecks,
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
        windows_pnp,
        vendor_apps,
        warnings,
        notes: vec![
            "hardware doctor is observe-only and does not open HID device handles".to_string(),
            "missing hardware is diagnostic information, not hardware validation evidence"
                .to_string(),
            "virtual or synthetic evidence must not satisfy real hardware receipt gates"
                .to_string(),
            "Windows PnP inspection records redacted interface topology only; it does not open or configure serial devices".to_string(),
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
    if !tools
        .usbpcap_descriptor_capture
        .ready_for_usbpcap_descriptor_capture
    {
        warnings.push(tools.usbpcap_descriptor_capture.access_guidance.clone());
    }
    if tools
        .usbpcap_descriptor_capture
        .active_usbpcap_processes
        .active_process_count
        > 0
    {
        warnings.push(
            "active USBPcapCMD process(es) detected; stop stale captures before starting a new passive capture unless one is the intended current capture"
                .to_string(),
        );
    }
    if !hid.api_available {
        warnings.push("HID API initialization failed".to_string());
    }
    if hid.api_available && !hid.moza_vid_visible {
        warnings.push("no Moza VID 0x346E devices are currently visible".to_string());
    }

    warnings
}

fn inspect_usbpcap_descriptor_capture_tools() -> UsbPcapDescriptorCaptureChecks {
    let usbpcap_extcap_path = find_usbpcap_extcap_path().map(|path| path.display().to_string());
    let usbpcap_extcap_present = usbpcap_extcap_path.is_some();
    let usbpcap_driver_installed = usbpcap_driver_installed();
    let usbpcap_driver_service_state = usbpcap_driver_service_state();
    let active_usbpcap_processes = inspect_active_usbpcap_processes();
    let Some(tshark_path) = find_tshark_path() else {
        return UsbPcapDescriptorCaptureChecks {
            tshark_present: false,
            tshark_path: None,
            usbpcap_extcap_present,
            usbpcap_extcap_path: usbpcap_extcap_path.clone(),
            usbpcap_driver_installed,
            usbpcap_driver_service_state: usbpcap_driver_service_state.clone(),
            interface_scan_attempted: false,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            usbpcap_device_scan_attempted: false,
            usbpcap_moza_device_hint_count: 0,
            usbpcap_moza_device_hints: Vec::new(),
            usbpcap_device_scan_errors: Vec::new(),
            active_usbpcap_processes,
            ready_for_usbpcap_descriptor_capture: false,
            access_guidance: usbpcap_descriptor_capture_guidance(
                false,
                false,
                usbpcap_extcap_present,
                usbpcap_driver_installed,
                usbpcap_driver_service_state.as_deref(),
            ),
            error: Some(
                "tshark was not found; install Wireshark or set WIRESHARK_TSHARK".to_string(),
            ),
        };
    };

    let output = Command::new(&tshark_path).arg("-D").output();
    let Ok(output) = output else {
        return UsbPcapDescriptorCaptureChecks {
            tshark_present: true,
            tshark_path: Some(tshark_path.display().to_string()),
            usbpcap_extcap_present,
            usbpcap_extcap_path: usbpcap_extcap_path.clone(),
            usbpcap_driver_installed,
            usbpcap_driver_service_state: usbpcap_driver_service_state.clone(),
            interface_scan_attempted: true,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            usbpcap_device_scan_attempted: false,
            usbpcap_moza_device_hint_count: 0,
            usbpcap_moza_device_hints: Vec::new(),
            usbpcap_device_scan_errors: Vec::new(),
            active_usbpcap_processes,
            ready_for_usbpcap_descriptor_capture: false,
            access_guidance: usbpcap_descriptor_capture_guidance(
                true,
                false,
                usbpcap_extcap_present,
                usbpcap_driver_installed,
                usbpcap_driver_service_state.as_deref(),
            ),
            error: Some("failed to run tshark -D".to_string()),
        };
    };

    if !output.status.success() {
        return UsbPcapDescriptorCaptureChecks {
            tshark_present: true,
            tshark_path: Some(tshark_path.display().to_string()),
            usbpcap_extcap_present,
            usbpcap_extcap_path: usbpcap_extcap_path.clone(),
            usbpcap_driver_installed,
            usbpcap_driver_service_state: usbpcap_driver_service_state.clone(),
            interface_scan_attempted: true,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            usbpcap_device_scan_attempted: false,
            usbpcap_moza_device_hint_count: 0,
            usbpcap_moza_device_hints: Vec::new(),
            usbpcap_device_scan_errors: Vec::new(),
            active_usbpcap_processes,
            ready_for_usbpcap_descriptor_capture: false,
            access_guidance: usbpcap_descriptor_capture_guidance(
                true,
                false,
                usbpcap_extcap_present,
                usbpcap_driver_installed,
                usbpcap_driver_service_state.as_deref(),
            ),
            error: Some(format!(
                "tshark -D failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let interfaces = usbpcap_interfaces_from_tshark_list(&stdout);
    let ready_for_usbpcap_descriptor_capture = !interfaces.is_empty();
    let (usbpcap_device_scan_attempted, usbpcap_moza_device_hints, usbpcap_device_scan_errors) =
        usbpcap_moza_device_hints_from_extcap(usbpcap_extcap_path.as_deref(), &interfaces);
    UsbPcapDescriptorCaptureChecks {
        tshark_present: true,
        tshark_path: Some(tshark_path.display().to_string()),
        usbpcap_extcap_present,
        usbpcap_extcap_path,
        usbpcap_driver_installed,
        usbpcap_driver_service_state: usbpcap_driver_service_state.clone(),
        interface_scan_attempted: true,
        usbpcap_interfaces_present: ready_for_usbpcap_descriptor_capture,
        usbpcap_interface_count: interfaces.len(),
        usbpcap_interfaces: interfaces,
        usbpcap_device_scan_attempted,
        usbpcap_moza_device_hint_count: usbpcap_moza_device_hints.len(),
        usbpcap_moza_device_hints,
        usbpcap_device_scan_errors,
        active_usbpcap_processes,
        ready_for_usbpcap_descriptor_capture,
        access_guidance: usbpcap_descriptor_capture_guidance(
            true,
            ready_for_usbpcap_descriptor_capture,
            usbpcap_extcap_present,
            usbpcap_driver_installed,
            usbpcap_driver_service_state.as_deref(),
        ),
        error: None,
    }
}

fn usbpcap_moza_device_hints_from_extcap(
    usbpcap_extcap_path: Option<&str>,
    interfaces: &[String],
) -> (bool, Vec<UsbPcapMozaDeviceHint>, Vec<String>) {
    let Some(usbpcap_extcap_path) = usbpcap_extcap_path else {
        return (false, Vec::new(), Vec::new());
    };

    let mut hints = Vec::new();
    let mut errors = Vec::new();
    for interface in interfaces {
        let Some(interface_value) = usbpcap_interface_value_from_tshark_line(interface) else {
            errors.push(format!(
                "could not derive USBPcap extcap interface value from `{interface}`"
            ));
            continue;
        };
        let output = Command::new(usbpcap_extcap_path)
            .args([
                "--extcap-config",
                "--extcap-interface",
                interface_value.as_str(),
            ])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                hints.extend(usbpcap_moza_device_hints_from_extcap_config(
                    &interface_value,
                    &stdout,
                ));
            }
            Ok(output) => errors.push(format!(
                "USBPcap extcap config failed for {interface_value}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!(
                "failed to run USBPcap extcap config for {interface_value}: {error}"
            )),
        }
    }

    (true, hints, errors)
}

fn usbpcap_interface_value_from_tshark_line(line: &str) -> Option<String> {
    if let Some(start) = line.find(r"\\.\USBPcap") {
        let value = line[start..]
            .split(|ch: char| ch.is_whitespace() || ch == '(')
            .next()
            .unwrap_or_default();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    let marker = "USBPcap";
    let start = line.find(marker)?;
    let suffix = line[start + marker.len()..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if suffix.is_empty() {
        None
    } else {
        Some(format!(r"\\.\USBPcap{suffix}"))
    }
}

fn usbpcap_moza_device_hints_from_extcap_config(
    interface_value: &str,
    output: &str,
) -> Vec<UsbPcapMozaDeviceHint> {
    let devices = output
        .lines()
        .filter_map(usbpcap_extcap_device_from_config_line)
        .collect::<Vec<_>>();
    let mut grouped: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();

    for device in devices {
        let display_lower = device.display.to_ascii_lowercase();
        let moza_relevant =
            display_lower.contains("moza") || display_lower.contains("usb serial device (com");
        if !moza_relevant {
            continue;
        }
        let capture_value = device
            .value
            .split_once('_')
            .map(|(root, _)| root)
            .unwrap_or(device.value.as_str())
            .to_string();
        let entry = grouped
            .entry(capture_value)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        entry.0.push(device.value);
        entry.1.push(device.display);
    }

    grouped
        .into_iter()
        .map(
            |(capture_devices_value, (matched_device_values, matched_device_displays))| {
                UsbPcapMozaDeviceHint {
                    usbpcap_interface: interface_value.to_string(),
                    capture_devices_value: capture_devices_value.clone(),
                    matched_device_values,
                    matched_device_displays,
                    suggested_capture_filter: format!(
                        "select {interface_value} with USBPcap --devices {capture_devices_value}"
                    ),
                }
            },
        )
        .collect()
}

fn usbpcap_extcap_device_from_config_line(line: &str) -> Option<UsbPcapExtcapDevice> {
    if !line.starts_with("value ") || !line.contains("{arg=99}") {
        return None;
    }
    let fields = usbpcap_extcap_braced_fields(line);
    Some(UsbPcapExtcapDevice {
        value: fields.get("value")?.to_string(),
        display: fields.get("display")?.to_string(),
    })
}

fn usbpcap_extcap_braced_fields(line: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut rest = line;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            break;
        };
        let field = &rest[..end];
        if let Some((key, value)) = field.split_once('=') {
            fields.insert(key.to_string(), value.to_string());
        }
        rest = &rest[end + 1..];
    }
    fields
}

fn find_usbpcap_extcap_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("USBPCAP_CMD").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }

    if cfg!(windows) {
        for path in [
            PathBuf::from(r"C:\Program Files\Wireshark\extcap\USBPcapCMD.exe"),
            PathBuf::from(r"C:\Program Files\USBPcap\USBPcapCMD.exe"),
            PathBuf::from(r"C:\Program Files (x86)\USBPcap\USBPcapCMD.exe"),
        ] {
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

fn usbpcap_driver_installed() -> bool {
    cfg!(windows)
        && [
            PathBuf::from(r"C:\Windows\System32\drivers\USBPcap.sys"),
            PathBuf::from(r"C:\Program Files\USBPcap\USBPcap.sys"),
            PathBuf::from(r"C:\Program Files (x86)\USBPcap\USBPcap.sys"),
        ]
        .iter()
        .any(|path| path.is_file())
}

fn usbpcap_driver_service_state() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }

    let output = Command::new("sc.exe")
        .args(["query", "USBPcap"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    usbpcap_service_state_from_sc_query(&stdout)
}

fn usbpcap_service_state_from_sc_query(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("STATE")?;
        value
            .split_whitespace()
            .find(|token| token.chars().any(|ch| ch.is_ascii_alphabetic()))
            .map(|state| state.to_ascii_lowercase())
    })
}

fn inspect_active_usbpcap_processes() -> UsbPcapActiveProcessChecks {
    if !cfg!(windows) {
        return UsbPcapActiveProcessChecks {
            process_scan_attempted: false,
            active_process_count: 0,
            processes: Vec::new(),
            error: Some(
                "active USBPcap process scan is currently implemented only on Windows".to_string(),
            ),
        };
    }

    let script = "Get-CimInstance Win32_Process -Filter \"name='USBPcapCMD.exe'\" | Select-Object ProcessId,CreationDate,CommandLine | ConvertTo-Json -Compress";
    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match usbpcap_active_processes_from_json(&stdout) {
                Ok(processes) => UsbPcapActiveProcessChecks {
                    process_scan_attempted: true,
                    active_process_count: processes.len(),
                    processes,
                    error: None,
                },
                Err(error) => UsbPcapActiveProcessChecks {
                    process_scan_attempted: true,
                    active_process_count: 0,
                    processes: Vec::new(),
                    error: Some(error.to_string()),
                },
            }
        }
        Ok(output) => UsbPcapActiveProcessChecks {
            process_scan_attempted: true,
            active_process_count: 0,
            processes: Vec::new(),
            error: Some(format!(
                "USBPcapCMD process scan failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        },
        Err(error) => UsbPcapActiveProcessChecks {
            process_scan_attempted: true,
            active_process_count: 0,
            processes: Vec::new(),
            error: Some(format!("failed to scan USBPcapCMD processes: {error}")),
        },
    }
}

fn usbpcap_active_processes_from_json(output: &str) -> Result<Vec<UsbPcapActiveProcess>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let value: serde_json::Value =
        serde_json::from_str(trimmed).context("failed to parse USBPcapCMD process JSON")?;
    let values = match &value {
        serde_json::Value::Array(values) => values.iter().collect::<Vec<_>>(),
        serde_json::Value::Object(_) => vec![&value],
        _ => Vec::new(),
    };

    let mut processes = Vec::new();
    for value in values {
        let Some(process_id) = value.get("ProcessId").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Ok(process_id) = u32::try_from(process_id) else {
            continue;
        };
        processes.push(UsbPcapActiveProcess {
            process_id,
            creation_date: value
                .get("CreationDate")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            command_line: value
                .get("CommandLine")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
        });
    }

    Ok(processes)
}

fn usbpcap_descriptor_capture_guidance(
    tshark_present: bool,
    usbpcap_interfaces_present: bool,
    usbpcap_extcap_present: bool,
    usbpcap_driver_installed: bool,
    usbpcap_driver_service_state: Option<&str>,
) -> String {
    match (
        tshark_present,
        usbpcap_interfaces_present,
        usbpcap_extcap_present,
        usbpcap_driver_installed,
    ) {
        (true, true, _, _) => {
            "USBPcap/Wireshark capture interfaces are available for descriptor enumeration capture"
                .to_string()
        }
        (true, false, true, true) => {
            match usbpcap_driver_service_state {
                Some("stopped") => {
                    "USBPcap is installed, but its driver service is stopped; run `sc start USBPcap` from an elevated shell or reboot after driver installation, then rerun hardware doctor".to_string()
                }
                Some("running") => {
                    "USBPcap is installed and its driver service is running, but Wireshark/tshark exposes no USBPcap interfaces; reboot after driver installation or run the descriptor capture from elevated Wireshark, then rerun hardware doctor".to_string()
                }
                _ => {
                    "USBPcap is installed, but Wireshark/tshark exposes no USBPcap interfaces; run the descriptor capture from an elevated shell or elevated Wireshark, reboot after driver installation if needed, then rerun hardware doctor".to_string()
                }
            }
        }
        (true, false, true, false) => {
            "USBPcap extcap is installed, but the USBPcap driver is not visible; repair the USBPcap install or reboot, then rerun hardware doctor".to_string()
        }
        (true, false, false, true) => {
            "USBPcap driver is installed, but Wireshark cannot find USBPcapCMD extcap; repair the Wireshark/USBPcap integration or import descriptor bytes from another trusted raw HID descriptor source".to_string()
        }
        (true, false, false, false) => {
            "USBPcap/Wireshark capture interfaces are unavailable; install USBPcap intentionally, use native Linux/sysfs, or import descriptor bytes from another trusted raw HID descriptor source".to_string()
        }
        (false, _, _, _) => {
            "tshark was not found; install Wireshark or set WIRESHARK_TSHARK before USBPcap descriptor capture".to_string()
        }
    }
}

fn find_tshark_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("WIRESHARK_TSHARK").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }

    if cfg!(windows) {
        for path in [
            PathBuf::from(r"C:\Program Files\Wireshark\tshark.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Wireshark\tshark.exe"),
        ] {
            if path.is_file() {
                return Some(path);
            }
        }
    }

    executable_path_on_path("tshark")
}

fn executable_path_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            executable_candidates(name).find_map(|candidate| {
                let path = dir.join(candidate);
                path.is_file().then_some(path)
            })
        })
    })
}

fn usbpcap_interfaces_from_tshark_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| line.to_ascii_lowercase().contains("usbpcap"))
        .map(str::to_string)
        .collect()
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

fn inspect_windows_pnp() -> WindowsPnpChecks {
    if !cfg!(windows) {
        return WindowsPnpChecks {
            scan_attempted: false,
            tool: "Get-PnpDevice",
            moza_vid_visible: None,
            hid_interface_count: 0,
            hid_pnp_device_count: 0,
            serial_interface_count: 0,
            devices: Vec::new(),
            error: Some("PnP inspection is currently implemented only on Windows".to_string()),
        };
    }

    let script = "Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -like '*VID_346E*' } | Select-Object Status,Class,FriendlyName,InstanceId | ConvertTo-Json -Compress";
    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut receipt = windows_pnp_checks_from_json(&stdout);
            receipt.scan_attempted = true;
            receipt
        }
        Ok(output) => WindowsPnpChecks {
            scan_attempted: true,
            tool: "Get-PnpDevice",
            moza_vid_visible: None,
            hid_interface_count: 0,
            hid_pnp_device_count: 0,
            serial_interface_count: 0,
            devices: Vec::new(),
            error: Some(format!(
                "Get-PnpDevice exited with status {}",
                output.status
            )),
        },
        Err(error) => WindowsPnpChecks {
            scan_attempted: true,
            tool: "Get-PnpDevice",
            moza_vid_visible: None,
            hid_interface_count: 0,
            hid_pnp_device_count: 0,
            serial_interface_count: 0,
            devices: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn windows_pnp_checks_from_json(text: &str) -> WindowsPnpChecks {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return WindowsPnpChecks {
            scan_attempted: true,
            tool: "Get-PnpDevice",
            moza_vid_visible: Some(false),
            hid_interface_count: 0,
            hid_pnp_device_count: 0,
            serial_interface_count: 0,
            devices: Vec::new(),
            error: None,
        };
    }

    let value = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => value,
        Err(error) => {
            return WindowsPnpChecks {
                scan_attempted: true,
                tool: "Get-PnpDevice",
                moza_vid_visible: None,
                hid_interface_count: 0,
                hid_pnp_device_count: 0,
                serial_interface_count: 0,
                devices: Vec::new(),
                error: Some(format!("failed to parse Get-PnpDevice JSON: {error}")),
            };
        }
    };

    let devices = match value {
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(windows_pnp_device_from_value)
            .collect::<Vec<_>>(),
        other => windows_pnp_device_from_value(&other).into_iter().collect(),
    };
    let hid_pnp_device_count = devices
        .iter()
        .filter(|device| device.class_name.as_deref() == Some("HIDClass"))
        .count();
    let hid_interface_count = unique_windows_pnp_hid_interface_count(&devices);
    let serial_interface_count = devices
        .iter()
        .filter(|device| {
            device.class_name.as_deref() == Some("Ports")
                || device
                    .friendly_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("serial"))
        })
        .count();
    let moza_vid_visible = Some(
        devices
            .iter()
            .any(|device| device.vendor_id.as_deref() == Some("0x346E")),
    );

    WindowsPnpChecks {
        scan_attempted: true,
        tool: "Get-PnpDevice",
        moza_vid_visible,
        hid_interface_count,
        hid_pnp_device_count,
        serial_interface_count,
        devices,
        error: None,
    }
}

fn unique_windows_pnp_hid_interface_count(devices: &[WindowsPnpDevice]) -> usize {
    let mut interfaces = BTreeSet::new();
    let mut hid_without_interface = 0usize;
    for device in devices
        .iter()
        .filter(|device| device.class_name.as_deref() == Some("HIDClass"))
    {
        if let Some(interface_number) = device.interface_number {
            interfaces.insert((
                device.vendor_id.as_deref().unwrap_or_default(),
                device.product_id.as_deref().unwrap_or_default(),
                interface_number,
            ));
        } else {
            hid_without_interface += 1;
        }
    }
    interfaces.len() + hid_without_interface
}

fn windows_pnp_device_from_value(value: &serde_json::Value) -> Option<WindowsPnpDevice> {
    let object = value.as_object()?;
    let instance_id = object
        .get("InstanceId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    Some(WindowsPnpDevice {
        status: object
            .get("Status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        class_name: object
            .get("Class")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        friendly_name: object
            .get("FriendlyName")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        vendor_id: extract_instance_hex(instance_id, "VID_"),
        product_id: extract_instance_hex(instance_id, "PID_"),
        interface_number: extract_interface_number(instance_id),
        instance_id_present: !instance_id.is_empty(),
    })
}

fn extract_instance_hex(instance_id: &str, marker: &str) -> Option<String> {
    let start = instance_id.find(marker)? + marker.len();
    let hex = instance_id.get(start..start + 4)?;
    if hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!("0x{}", hex.to_ascii_uppercase()))
    } else {
        None
    }
}

fn extract_interface_number(instance_id: &str) -> Option<i32> {
    let start = instance_id.find("MI_")? + 3;
    let hex = instance_id.get(start..start + 2)?;
    i32::from_str_radix(hex, 16).ok()
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

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("failed to serialize JSON file")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse '{}'", path.display()))
}

fn write_text_file(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    fs::write(path, value).with_context(|| format!("failed to write '{}'", path.display()))
}

fn print_sniff_plan(
    json: bool,
    json_out: Option<&Path>,
    md_out: Option<&Path>,
    plan: &HardwareSniffPlanArtifact,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(plan)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Passive USB sniff plan created for {} / {}.",
        plan.family, plan.scenario
    ))?;
    write_stdout_line(
        "Non-claiming: native response, native visible, smoke, release, and OpenRacing hardware output are false.",
    )?;
    write_stdout_line("Forbidden actions:")?;
    for action in &plan.forbidden_actions {
        write_stdout_line(&format!("- {action}"))?;
    }
    if let Some(path) = json_out {
        write_stdout_line(&format!("JSON plan: {}", path.display()))?;
    }
    if let Some(path) = md_out {
        write_stdout_line(&format!("Markdown plan: {}", path.display()))?;
    }
    Ok(())
}

fn print_sniff_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareSniffReceiptArtifact,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Passive USB sniff receipt created for {} / {}.",
        receipt.app, receipt.scenario
    ))?;
    write_stdout_line(&format!(
        "PCAPNG sha256: {} ({} bytes)",
        receipt.pcapng_sha256, receipt.pcapng_size_bytes
    ))?;
    write_stdout_line(
        "Non-claiming: OpenRacing opened no HID device and sent no FFB, output, feature, serial, firmware, or DFU commands.",
    )?;
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn print_sniff_notes_template(
    json: bool,
    out: &Path,
    json_out: Option<&Path>,
    receipt: &HardwareSniffNotesTemplateReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Passive USB sniff operator notes template created: {}",
        out.display()
    ))?;
    write_stdout_line(
        "Non-claiming: native response, native visible, smoke, release, and OpenRacing hardware output are false.",
    )?;
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn print_sniff_capture(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareSniffCaptureReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Passive USB sniff capture {}: {}",
        if receipt.success {
            "recorded"
        } else {
            "attempted"
        },
        receipt.out_path
    ))?;
    write_stdout_line(&format!(
        "Duration: {} ms; stopped after duration: {}; pcapng bytes: {}",
        receipt.duration_ms, receipt.terminated_after_duration, receipt.pcapng_size_bytes
    ))?;
    if !receipt.success {
        write_stdout_line(&format!(
            "Capture did not produce a non-empty pcapng; inspect USBPcapCMD logs: stdout={}, stderr={}",
            receipt.usbpcapcmd_stdout_path, receipt.usbpcapcmd_stderr_path
        ))?;
    }
    write_stdout_line(
        "Non-claiming: this launched only the external passive USBPcapCMD capture tool; OpenRacing sent no hardware output and made no readiness claim.",
    )?;
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn print_sniff_summary(
    json: bool,
    json_out: Option<&Path>,
    md_out: Option<&Path>,
    summary: &HardwareSniffSummaryArtifact,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(summary)?)?;
        return Ok(());
    }

    if summary.success {
        write_stdout_line(&format!(
            "Passive USB sniff summary created with {} matched packet(s).",
            summary.matched_packets
        ))?;
    } else {
        write_stdout_line(&format!(
            "Passive USB sniff summary completed with no matched packets: {}",
            summary.reason.as_deref().unwrap_or("unknown reason")
        ))?;
    }
    write_stdout_line(&format!(
        "Transfers: host->device={}, device->host={}, control={}, interrupt={}",
        summary.usb_transfer_summary.host_to_device,
        summary.usb_transfer_summary.device_to_host,
        summary.usb_transfer_summary.control,
        summary.usb_transfer_summary.interrupt
    ))?;
    write_stdout_line(
        "Non-claiming: native response, native visible, smoke, release, and OpenRacing hardware output are false.",
    )?;
    if let Some(path) = json_out {
        write_stdout_line(&format!("JSON summary: {}", path.display()))?;
    }
    if let Some(path) = md_out {
        write_stdout_line(&format!("Markdown summary: {}", path.display()))?;
    }
    Ok(())
}

fn print_sniff_bundle(
    json: bool,
    out: &Path,
    json_out: Option<&Path>,
    manifest: &HardwareSniffBundleManifest,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(manifest)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Passive USB sniff bundle created: {}",
        out.display()
    ))?;
    write_stdout_line(&format!(
        "Artifacts: {}; raw pcapng included: {}",
        manifest.artifacts.len(),
        manifest.includes_raw_pcapng
    ))?;
    write_stdout_line(
        "Non-claiming: native response, native visible, smoke, release, and OpenRacing hardware output are false.",
    )?;
    if let Some(path) = json_out {
        write_stdout_line(&format!("JSON manifest: {}", path.display()))?;
    }
    Ok(())
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
    write_stdout_line(&format!(
        "Windows PnP Moza devices: scanned={} visible={} hid_interfaces={} hid_pnp_devices={} serial_interfaces={}",
        receipt.windows_pnp.scan_attempted,
        receipt.windows_pnp.moza_vid_visible.unwrap_or(false),
        receipt.windows_pnp.hid_interface_count,
        receipt.windows_pnp.hid_pnp_device_count,
        receipt.windows_pnp.serial_interface_count
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

fn print_lane_init_receipt(json: bool, receipt: &HardwareLaneInitReceipt) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Hardware lane scaffold created for {} at {}.",
        receipt.family, receipt.lane
    ))?;
    write_stdout_line(
        "No HID devices were opened and no output, feature, serial, firmware, or DFU commands were sent.",
    )?;
    for path in &receipt.created_files {
        write_stdout_line(&format!("Created: {path}"))?;
    }
    Ok(())
}

fn print_lane_status_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareLaneStatusReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Hardware lane status for {} at {}.",
        receipt.family, receipt.lane
    ))?;
    write_stdout_line(&format!("Manifest source: {}", receipt.manifest_source))?;
    write_stdout_line(&format!(
        "Scaffold required: {}; scaffold complete: {}; evidence claims validated: {}; ready_for_zero_torque: {}; ready_for_ffb: {}",
        receipt.scaffold_required,
        receipt.scaffold_complete,
        receipt.evidence_claims_validated,
        receipt.ready_for_zero_torque,
        receipt.ready_for_ffb
    ))?;
    write_stdout_line(&format!(
        "Next blocked stage: {}",
        receipt.next_blocked_stage
    ))?;
    write_stdout_line(&format!(
        "Descriptor capture tooling: {}",
        receipt.descriptor_capture_tooling.guidance
    ))?;
    write_stdout_line(&format!(
        "Verifier receipt: {}",
        receipt.verifier_receipt.guidance
    ))?;
    for item in &receipt.blocking_items {
        write_stdout_line(&format!("Blocked: {item}"))?;
    }
    for command in &receipt.safe_next_commands {
        write_stdout_line(&format!("Next: {command}"))?;
    }
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn print_lane_role_endpoint_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareLaneRoleEndpointReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Hardware lane role endpoint updated for {} at {}.",
        receipt.role, receipt.lane
    ))?;
    write_stdout_line(&format!(
        "Endpoint: {} -> {}",
        receipt.previous_endpoint, receipt.expected_endpoint
    ))?;
    write_stdout_line(
        "No HID devices were opened and no output, feature, serial, firmware, or DFU commands were sent.",
    )?;
    for path in &receipt.updated_files {
        write_stdout_line(&format!("Updated: {path}"))?;
    }
    if let Some(path) = json_out {
        write_stdout_line(&format!("Receipt: {}", path.display()))?;
    }
    Ok(())
}

fn print_bringup_rail_receipt(
    json: bool,
    json_out: Option<&Path>,
    receipt: &HardwareBringupRailReceipt,
) -> Result<()> {
    if json {
        write_stdout_line(&serde_json::to_string_pretty(receipt)?)?;
        return Ok(());
    }

    write_stdout_line(&format!(
        "Hardware bring-up rail for {}: {} stages, no HID devices opened.",
        receipt.adapter.display_name,
        receipt.stages.len()
    ))?;
    for stage in &receipt.stages {
        write_stdout_line(&format!(
            "{}. {}: {}",
            stage.order + 1,
            stage.id,
            stage.purpose
        ))?;
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

fn hex_u8(value: u8) -> String {
    format!("0x{value:02X}")
}

const SNIFF_CAPTURE_KIND: &str = "software_usb_urb_capture";
const SNIFF_EVIDENCE_STATUS: &str = "passive_external_usb_observation";
const SNIFF_BUNDLE_ROOT: &str = "openracing-sniff-bundle";
const SNIFF_BUNDLE_KIND: &str = "openracing_passive_usb_sniff_bundle";
const DEFAULT_SNIFF_MAX_SAMPLES_PER_REPORT: usize = 3;
const MAX_SNIFF_MAX_SAMPLES_PER_REPORT: usize = 32;
const USB_COM_SERIAL_TUPLE_SAMPLE_LIMIT: usize = 3;
const SNIFF_ALLOWED_ACTIONS: &[&str] = &[
    "capture host-side USB URBs with Wireshark, USBPcap, tshark, or usbmon",
    "observe operating-system, vendor-app, simulator, or bridge traffic",
    "record operator notes, scenario details, pcapng hashes, and support evidence",
    "stop capture and save the passive observation as .pcapng",
];
const SNIFF_FORBIDDEN_ACTIONS: &[&str] = &[
    "install Zadig",
    "replace HID driver",
    "switch device to WinUSB",
    "run OpenRacing output commands",
    "send OpenRacing HID output reports",
    "send OpenRacing HID feature reports",
    "touch serial configuration",
    "open firmware update flows",
    "run firmware or DFU tools",
];
const SNIFF_PRE_CAPTURE_CHECKLIST: &[&str] = &[
    "confirm the target device stack is attached before starting capture",
    "start USBPcap, Wireshark, tshark, or usbmon before launching or changing the external app",
    "keep OpenRacing hardware output commands stopped for this passive capture",
    "keep firmware, update, DFU, driver replacement, Zadig, and WinUSB conversion flows closed",
];
const SNIFF_POST_CAPTURE_CHECKLIST: &[&str] = &[
    "stop capture and save the pcapng in local scratch storage",
    "record operator notes before bundling",
    SNIFF_POST_CAPTURE_EVIDENCE_COMMANDS_CHECKLIST_ITEM,
    "do not commit raw pcapng unless it is separately reviewed for size, sensitivity, and operator consent",
];
const SNIFF_POST_CAPTURE_EVIDENCE_COMMANDS_CHECKLIST_ITEM: &str = "run sniff-receipt, sniff-notes-template, and sniff-summary before treating the capture as lane evidence";
const SNIFF_OPERATOR_NOTES_REQUIRED: &[&str] = &[
    "scenario performed",
    "external app, simulator, or OS stack observed",
    "capture duration or start/stop times",
    "device stack attached",
    "whether firmware/update/DFU pages stayed closed",
    "whether raw pcapng was kept local or reviewed for bundling",
];
const PIT_HOUSE_SETTING_CHANGE_OPERATOR_NOTES_REQUIRED: &[&str] = &[
    "exact Pit House setting changed",
    "starting setting value",
    "ending setting value",
    "whether the setting value was restored",
];

fn sniff_operator_notes_required(scenario: HardwareSniffScenario) -> Vec<String> {
    let mut required = SNIFF_OPERATOR_NOTES_REQUIRED
        .iter()
        .copied()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if scenario == HardwareSniffScenario::PitHouseSettingChange {
        required.extend(
            PIT_HOUSE_SETTING_CHANGE_OPERATOR_NOTES_REQUIRED
                .iter()
                .copied()
                .map(str::to_string),
        );
    }
    required
}

#[derive(Debug)]
struct HardwareSniffPlanRequest<'a> {
    family: &'a str,
    scenario: HardwareSniffScenario,
    lane: &'a Path,
    operator: &'a str,
    device_note: &'a str,
    capture_tools: &'a [HardwareSniffCaptureTool],
    platform_hint: Option<HardwareSniffPlatformHint>,
}

#[derive(Debug)]
struct HardwareSniffReceiptRequest<'a> {
    plan: &'a Path,
    pcapng: Option<&'a Path>,
    operator: Option<&'a str>,
    app: &'a str,
    scenario: Option<HardwareSniffScenario>,
    device_note: Option<&'a str>,
    evidence: &'a str,
}

#[derive(Debug)]
struct HardwareSniffSummaryRequest<'a> {
    pcapng: &'a Path,
    vendor: Option<&'a str>,
    product: Option<&'a str>,
    interface: Option<u16>,
    include_payload_samples: bool,
    max_samples_per_report: Option<usize>,
}

#[derive(Debug)]
struct HardwareSniffCaptureRequest<'a> {
    usbpcapcmd: &'a Path,
    usbpcap_interface: &'a str,
    devices: &'a str,
    duration_ms: u64,
    out: &'a Path,
    overwrite: bool,
    confirm_external_passive_capture: bool,
}

#[derive(Debug)]
struct HardwareSniffBundleRequest<'a> {
    plan: &'a Path,
    receipt: &'a Path,
    summary: &'a Path,
    operator_notes: &'a Path,
    operator_notes_receipt: Option<&'a Path>,
    include_pcapng: Option<&'a Path>,
    out: &'a Path,
}

#[derive(Debug, Clone)]
struct HardwareSniffSummaryConfig {
    filters: HardwareSniffSummaryFilters,
    include_payload_samples: bool,
    max_samples_per_report: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffPlanArtifact {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    family: String,
    scenario: String,
    lane: String,
    operator: String,
    device_note: String,
    capture_kind: &'static str,
    capture_tools: Vec<String>,
    platform_hint: String,
    allowed_actions: Vec<&'static str>,
    forbidden_actions: Vec<&'static str>,
    pre_capture_checklist: Vec<String>,
    post_capture_checklist: Vec<String>,
    operator_notes_required: Vec<String>,
    raw_pcap_commit_default: bool,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffReceiptArtifact {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    plan_path: String,
    pcapng_path: String,
    pcapng_sha256: String,
    pcapng_size_bytes: u64,
    operator: String,
    app: String,
    scenario: String,
    device_note: String,
    evidence: String,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    openracing_hid_device_opened: bool,
    openracing_ffb_writes: bool,
    openracing_output_reports: bool,
    openracing_feature_reports: bool,
    openracing_serial_config_commands: bool,
    openracing_firmware_or_dfu_commands: bool,
    external_app_observed: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Serialize)]
struct HardwareSniffNotesTemplateReceipt {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    plan_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware_doctor_path: Option<String>,
    out_path: String,
    scenario: String,
    operator: String,
    device_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_hints: Option<HardwareSniffNotesCaptureHints>,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Serialize)]
struct HardwareSniffNotesCaptureHints {
    source: String,
    receipt_flags: HardwareSniffNotesDoctorFlags,
    #[serde(skip_serializing_if = "Option::is_none")]
    usbpcap_extcap_path: Option<String>,
    hint_count: usize,
    hints: Vec<HardwareSniffNotesUsbPcapHint>,
    active_usbpcap_process_count: usize,
    active_usbpcap_processes: Vec<HardwareSniffNotesActiveUsbPcapProcess>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HardwareSniffNotesDoctorFlags {
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
}

impl HardwareSniffNotesDoctorFlags {
    fn all_no_output_flags_true(&self) -> bool {
        self.no_hid_device_opened
            && self.no_ffb_writes
            && self.no_output_reports
            && self.no_feature_reports
            && self.no_serial_config_commands
            && self.no_firmware_or_dfu_commands
    }
}

#[derive(Debug, Serialize)]
struct HardwareSniffNotesUsbPcapHint {
    usbpcap_interface: String,
    capture_devices_value: String,
    matched_device_displays: Vec<String>,
    suggested_capture_filter: String,
}

#[derive(Debug, Serialize)]
struct HardwareSniffNotesActiveUsbPcapProcess {
    process_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_line: Option<String>,
}

#[derive(Debug)]
struct HardwareSniffCaptureOutcome {
    process_started: bool,
    process_id: Option<u32>,
    started_at_utc: String,
    completed_at_utc: String,
    exit_status: Option<String>,
    terminated_after_duration: bool,
    pcapng_exists: bool,
    pcapng_size_bytes: u64,
    pcapng_sha256: Option<String>,
    stdout_path: PathBuf,
    stdout_size_bytes: u64,
    stderr_path: PathBuf,
    stderr_size_bytes: u64,
}

#[derive(Debug)]
struct HardwareSniffCaptureOutputMetadata {
    exists: bool,
    size_bytes: u64,
    sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct HardwareSniffCaptureReceipt {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    capture_tool: &'static str,
    usbpcapcmd_path: String,
    usbpcap_interface: String,
    devices: String,
    duration_ms: u64,
    out_path: String,
    overwrite: bool,
    process_started: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_id: Option<u32>,
    started_at_utc: String,
    completed_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_status: Option<String>,
    terminated_after_duration: bool,
    pcapng_exists: bool,
    pcapng_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pcapng_sha256: Option<String>,
    usbpcapcmd_stdout_path: String,
    usbpcapcmd_stdout_size_bytes: u64,
    usbpcapcmd_stderr_path: String,
    usbpcapcmd_stderr_size_bytes: u64,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    openracing_hid_device_opened: bool,
    openracing_ffb_writes: bool,
    openracing_output_reports: bool,
    openracing_feature_reports: bool,
    openracing_serial_config_commands: bool,
    openracing_firmware_or_dfu_commands: bool,
    external_capture_tool_invoked: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
    next_allowed_actions: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffSummaryArtifact {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    pcapng_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    tool: HardwareSniffSummaryTool,
    filters: HardwareSniffSummaryFilters,
    matched_packets: usize,
    usb_transfer_summary: HardwareSniffUsbTransferSummary,
    observed_devices: Vec<HardwareSniffObservedDevice>,
    observed_reports: Vec<HardwareSniffObservedReport>,
    report_classification_summary: HardwareSniffReportClassificationSummary,
    descriptor_candidates: Vec<HardwareSniffDescriptorCandidate>,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffBundleManifest {
    schema_version: u32,
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    bundle_kind: &'static str,
    includes_raw_pcapng: bool,
    artifacts: Vec<HardwareSniffBundleArtifactHash>,
    evidence_status: &'static str,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffBundleArtifactHash {
    path: String,
    sha256: String,
}

#[derive(Debug)]
struct SniffBundleZipEntry {
    archive_path: String,
    source: SniffBundleZipSource,
    sha256: String,
}

impl SniffBundleZipEntry {
    fn bytes(archive_path: String, bytes: Vec<u8>) -> Self {
        let sha256 = sha256_hex(&bytes);
        Self {
            archive_path,
            source: SniffBundleZipSource::Bytes(bytes),
            sha256,
        }
    }

    fn file(archive_path: String, path: PathBuf, sha256: String) -> Self {
        Self {
            archive_path,
            source: SniffBundleZipSource::File(path),
            sha256,
        }
    }
}

#[derive(Debug)]
enum SniffBundleZipSource {
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffSummaryTool {
    tshark_present: bool,
    tshark_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareSniffSummaryFilters {
    vendor_id: Option<String>,
    product_id: Option<String>,
    interface_number: Option<u16>,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
struct HardwareSniffUsbTransferSummary {
    host_to_device: usize,
    device_to_host: usize,
    control: usize,
    interrupt: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffObservedDevice {
    vendor_id: String,
    product_id: String,
    interfaces: Vec<u16>,
    endpoints: Vec<String>,
}

#[derive(Debug)]
struct HardwareSniffObservedDeviceBuilder {
    vendor_id: String,
    product_id: String,
    interfaces: BTreeSet<u16>,
    endpoints: BTreeSet<u8>,
}

impl HardwareSniffObservedDeviceBuilder {
    fn build(self) -> HardwareSniffObservedDevice {
        HardwareSniffObservedDevice {
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            interfaces: self.interfaces.into_iter().collect(),
            endpoints: self.endpoints.into_iter().map(hex_u8).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffObservedReport {
    direction: String,
    report_id: String,
    classification: HardwareSniffReportClassification,
    count: usize,
    payload_sample_count: usize,
    payload_sha256_examples: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_hex_samples: Option<Vec<String>>,
}

#[derive(Debug)]
struct HardwareSniffObservedReportBuilder {
    direction: SniffUsbDirection,
    report_id: u8,
    count: usize,
    payload_sha256_examples: Vec<String>,
    payload_hex_samples: Vec<String>,
}

impl HardwareSniffObservedReportBuilder {
    fn build(self, include_payload_samples: bool) -> HardwareSniffObservedReport {
        let classification = classify_sniff_observed_report(self.direction, self.report_id);
        let payload_hex_samples = include_payload_samples.then_some(self.payload_hex_samples);
        HardwareSniffObservedReport {
            direction: self.direction.as_str().to_string(),
            report_id: hex_u8(self.report_id),
            classification,
            count: self.count,
            payload_sample_count: self.payload_sha256_examples.len(),
            payload_sha256_examples: self.payload_sha256_examples,
            payload_hex_samples,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffReportClassification {
    category: String,
    label: String,
    vendor_specific_candidate: bool,
    native_control_evidence: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffReportClassificationSummary {
    standard_pidff_output_report_count: usize,
    vendor_or_device_specific_output_candidate_count: usize,
    input_or_status_report_count: usize,
    host_to_device_packet_count: usize,
    host_to_device_classified_packet_count: usize,
    host_to_device_unclassified_packet_count: usize,
    host_to_device_decode_gap: bool,
    host_to_device_data_len_packet_count: usize,
    host_to_device_data_len_bytes: usize,
    host_to_device_payload_extracted_packet_count: usize,
    host_to_device_payload_extracted_bytes: usize,
    host_to_device_payload_missing_packet_count: usize,
    host_to_device_payload_export_gap: bool,
    host_to_device_payload_missing_packet_examples:
        Vec<HardwareSniffHostToDeviceMissingPayloadExample>,
    host_to_device_report_ids: Vec<String>,
    standard_pidff_output_report_ids: Vec<String>,
    vendor_or_device_specific_output_candidate_report_ids: Vec<String>,
    usbcom_serial_frame_summary: HardwareSniffUsbComSerialFrameSummary,
    decode_recommended: bool,
    native_control_evidence: bool,
    readiness_claim: bool,
    notes: Vec<String>,
}

fn summarize_sniff_report_classifications(
    reports: &[HardwareSniffObservedReport],
    host_to_device_packet_count: usize,
    host_to_device_payload_coverage: HardwareSniffHostToDevicePayloadCoverage,
    usbcom_serial_frame_summary: HardwareSniffUsbComSerialFrameSummary,
) -> HardwareSniffReportClassificationSummary {
    let mut standard_pidff_output_report_count = 0;
    let mut vendor_or_device_specific_output_candidate_count = 0;
    let mut input_or_status_report_count = 0;
    let mut host_to_device_classified_packet_count = 0usize;
    let mut host_to_device_report_ids = BTreeSet::new();
    let mut standard_pidff_output_report_ids = BTreeSet::new();
    let mut vendor_or_device_specific_output_candidate_report_ids = BTreeSet::new();

    for report in reports {
        if report.direction == "host_to_device" {
            host_to_device_report_ids.insert(report.report_id.clone());
            host_to_device_classified_packet_count =
                host_to_device_classified_packet_count.saturating_add(report.count);
        }
        match report.classification.category.as_str() {
            "standard_pidff_output_report" => {
                standard_pidff_output_report_count += 1;
                standard_pidff_output_report_ids.insert(report.report_id.clone());
            }
            "vendor_or_device_specific_output_candidate" => {
                vendor_or_device_specific_output_candidate_count += 1;
                vendor_or_device_specific_output_candidate_report_ids
                    .insert(report.report_id.clone());
            }
            "input_or_status_report" => {
                input_or_status_report_count += 1;
            }
            _ => {}
        }
    }

    let host_to_device_unclassified_packet_count =
        host_to_device_packet_count.saturating_sub(host_to_device_classified_packet_count);
    let host_to_device_decode_gap =
        host_to_device_packet_count > 0 && host_to_device_unclassified_packet_count > 0;
    let host_to_device_payload_export_gap =
        host_to_device_payload_coverage.payload_missing_packet_count > 0;
    let decode_recommended = vendor_or_device_specific_output_candidate_count > 0
        || host_to_device_decode_gap
        || host_to_device_payload_export_gap;
    let notes = if vendor_or_device_specific_output_candidate_count > 0 {
        vec![
            "vendor/device-specific host-to-device report candidates are present; decode them before designing any future OpenRacing output plan".to_string(),
            "classification summary is protocol navigation only and does not prove native control or readiness".to_string(),
        ]
    } else if host_to_device_payload_export_gap {
        vec![
            "host-to-device USB transfers declare data length but no payload bytes were extracted; inspect tshark payload export or raw pcap filtering before designing any future OpenRacing output plan".to_string(),
            "host-to-device payload export gaps are protocol navigation only and do not prove native control or readiness".to_string(),
        ]
    } else if host_to_device_decode_gap {
        vec![
            "host-to-device USB transfers were observed without extractable HID report IDs; inspect raw payload export or dissector output before designing any future OpenRacing output plan".to_string(),
            "host-to-device decode gaps are protocol navigation only and do not prove native control or readiness".to_string(),
        ]
    } else {
        vec![
            "no vendor/device-specific host-to-device report candidates were classified in the matched packets".to_string(),
            "absence of candidates in one passive capture does not prove standard PIDFF is sufficient".to_string(),
        ]
    };

    HardwareSniffReportClassificationSummary {
        standard_pidff_output_report_count,
        vendor_or_device_specific_output_candidate_count,
        input_or_status_report_count,
        host_to_device_packet_count,
        host_to_device_classified_packet_count,
        host_to_device_unclassified_packet_count,
        host_to_device_decode_gap,
        host_to_device_data_len_packet_count: host_to_device_payload_coverage.data_len_packet_count,
        host_to_device_data_len_bytes: host_to_device_payload_coverage.data_len_bytes,
        host_to_device_payload_extracted_packet_count: host_to_device_payload_coverage
            .payload_extracted_packet_count,
        host_to_device_payload_extracted_bytes: host_to_device_payload_coverage
            .payload_extracted_bytes,
        host_to_device_payload_missing_packet_count: host_to_device_payload_coverage
            .payload_missing_packet_count,
        host_to_device_payload_export_gap,
        host_to_device_payload_missing_packet_examples: host_to_device_payload_coverage
            .payload_missing_packet_examples,
        host_to_device_report_ids: host_to_device_report_ids.into_iter().collect(),
        standard_pidff_output_report_ids: standard_pidff_output_report_ids.into_iter().collect(),
        vendor_or_device_specific_output_candidate_report_ids:
            vendor_or_device_specific_output_candidate_report_ids
                .into_iter()
                .collect(),
        usbcom_serial_frame_summary,
        decode_recommended,
        native_control_evidence: false,
        readiness_claim: false,
        notes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareSniffUsbComSerialFrameSummary {
    frame_start: String,
    checksum_model: String,
    packet_count: usize,
    payload_bytes: usize,
    parsed_frame_count: usize,
    checksum_valid_frame_count: usize,
    checksum_invalid_frame_count: usize,
    commandless_frame_count: usize,
    truncated_frame_count: usize,
    non_frame_byte_count: usize,
    max_frames_per_packet: usize,
    frame_shape_decode_gap: bool,
    tuple_counts: Vec<HardwareSniffUsbComSerialTupleCount>,
    tuple_sample_limit: usize,
    native_control_evidence: bool,
    readiness_claim: bool,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareSniffUsbComSerialTupleCount {
    group: String,
    device_id: String,
    command: Option<String>,
    payload_len_min: usize,
    payload_len_max: usize,
    count: usize,
    checksum_valid_count: usize,
    checksum_invalid_count: usize,
    sample_frames: Vec<HardwareSniffUsbComSerialTupleSampleFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareSniffUsbComSerialTupleSampleFrame {
    sample_index: usize,
    packet_ordinal: usize,
    frame_ordinal_in_packet: usize,
    frame_hex: String,
    declared_len: usize,
    payload_len: usize,
    payload_hex: String,
    checksum_hex: String,
    checksum_valid: bool,
    hardware_output_authorized: bool,
    output_sendability_claim: bool,
}

#[derive(Default)]
struct HardwareSniffUsbComSerialFrameSummaryBuilder {
    packet_count: usize,
    payload_bytes: usize,
    parsed_frame_count: usize,
    checksum_valid_frame_count: usize,
    checksum_invalid_frame_count: usize,
    commandless_frame_count: usize,
    truncated_frame_count: usize,
    non_frame_byte_count: usize,
    max_frames_per_packet: usize,
    tuple_counts: BTreeMap<(u8, u8, Option<u8>), HardwareSniffUsbComSerialTupleBuilder>,
}

#[derive(Default)]
struct HardwareSniffUsbComSerialTupleBuilder {
    payload_len_min: Option<usize>,
    payload_len_max: usize,
    count: usize,
    checksum_valid_count: usize,
    checksum_invalid_count: usize,
    sample_frames: Vec<HardwareSniffUsbComSerialTupleSampleFrame>,
}

impl HardwareSniffUsbComSerialFrameSummaryBuilder {
    fn observe_payload(&mut self, payload: &[u8]) {
        if !payload.contains(&USB_COM_SERIAL_FRAME_START) {
            return;
        }

        self.packet_count = self.packet_count.saturating_add(1);
        let packet_ordinal = self.packet_count;
        self.payload_bytes = self.payload_bytes.saturating_add(payload.len());
        let mut offset = 0usize;
        let mut frames_in_packet = 0usize;

        while offset < payload.len() {
            if payload[offset] != USB_COM_SERIAL_FRAME_START {
                self.non_frame_byte_count = self.non_frame_byte_count.saturating_add(1);
                offset = offset.saturating_add(1);
                continue;
            }

            if payload.len().saturating_sub(offset) < USB_COM_SERIAL_MIN_FRAME_LEN {
                self.truncated_frame_count = self.truncated_frame_count.saturating_add(1);
                break;
            }

            let declared_len = usize::from(payload[offset + 1]);
            let expected_len = USB_COM_SERIAL_BASE_FRAME_LEN.saturating_add(declared_len);
            if payload.len().saturating_sub(offset) < expected_len {
                self.truncated_frame_count = self.truncated_frame_count.saturating_add(1);
                break;
            }

            let frame = &payload[offset..offset + expected_len];
            let actual_checksum = frame[expected_len - 1];
            let expected_checksum = usbcom_serial_checksum(&frame[..expected_len - 1]);
            let checksum_valid = actual_checksum == expected_checksum;
            let group = frame[2];
            let device_id = frame[3];
            let command = (declared_len > 0).then_some(frame[4]);
            let payload_len = declared_len.saturating_sub(1);
            let frame_ordinal_in_packet = frames_in_packet.saturating_add(1);

            self.parsed_frame_count = self.parsed_frame_count.saturating_add(1);
            frames_in_packet = frame_ordinal_in_packet;
            if checksum_valid {
                self.checksum_valid_frame_count = self.checksum_valid_frame_count.saturating_add(1);
            } else {
                self.checksum_invalid_frame_count =
                    self.checksum_invalid_frame_count.saturating_add(1);
            }
            if command.is_none() {
                self.commandless_frame_count = self.commandless_frame_count.saturating_add(1);
            }

            let tuple = self
                .tuple_counts
                .entry((group, device_id, command))
                .or_default();
            tuple.count = tuple.count.saturating_add(1);
            tuple.payload_len_min = Some(
                tuple
                    .payload_len_min
                    .map_or(payload_len, |current| current.min(payload_len)),
            );
            tuple.payload_len_max = tuple.payload_len_max.max(payload_len);
            if checksum_valid {
                tuple.checksum_valid_count = tuple.checksum_valid_count.saturating_add(1);
            } else {
                tuple.checksum_invalid_count = tuple.checksum_invalid_count.saturating_add(1);
            }
            if checksum_valid && tuple.sample_frames.len() < USB_COM_SERIAL_TUPLE_SAMPLE_LIMIT {
                let payload_start = if command.is_some() { 5 } else { 4 };
                let payload_end = expected_len.saturating_sub(1);
                let semantic_payload = if payload_start <= payload_end {
                    &frame[payload_start..payload_end]
                } else {
                    &[]
                };
                tuple
                    .sample_frames
                    .push(HardwareSniffUsbComSerialTupleSampleFrame {
                        sample_index: tuple.sample_frames.len().saturating_add(1),
                        packet_ordinal,
                        frame_ordinal_in_packet,
                        frame_hex: bytes_hex_compact_upper(frame),
                        declared_len,
                        payload_len,
                        payload_hex: bytes_hex_compact_upper(semantic_payload),
                        checksum_hex: hex_u8(actual_checksum),
                        checksum_valid,
                        hardware_output_authorized: false,
                        output_sendability_claim: false,
                    });
            }

            offset = offset.saturating_add(expected_len);
        }

        self.max_frames_per_packet = self.max_frames_per_packet.max(frames_in_packet);
    }

    fn build(self) -> HardwareSniffUsbComSerialFrameSummary {
        let frame_shape_decode_gap = self.truncated_frame_count > 0
            || self.non_frame_byte_count > 0
            || self.checksum_invalid_frame_count > 0;
        let notes = if self.parsed_frame_count > 0 {
            vec![
                "USB CDC payloads contain length-prefixed 0x7E serial-frame candidates; this is protocol-shape evidence only".to_string(),
                "Tuple counts are not send authorization and do not prove native OpenRacing control".to_string(),
            ]
        } else {
            vec![
                "No USB CDC 0x7E serial-frame candidates were parsed from host-to-device payloads".to_string(),
                "Absence of parsed frames in one passive summary does not prove standard PIDFF is sufficient".to_string(),
            ]
        };

        HardwareSniffUsbComSerialFrameSummary {
            frame_start: hex_u8(USB_COM_SERIAL_FRAME_START),
            checksum_model: "magic_13_wrapping_sum_over_frame_without_checksum".to_string(),
            packet_count: self.packet_count,
            payload_bytes: self.payload_bytes,
            parsed_frame_count: self.parsed_frame_count,
            checksum_valid_frame_count: self.checksum_valid_frame_count,
            checksum_invalid_frame_count: self.checksum_invalid_frame_count,
            commandless_frame_count: self.commandless_frame_count,
            truncated_frame_count: self.truncated_frame_count,
            non_frame_byte_count: self.non_frame_byte_count,
            max_frames_per_packet: self.max_frames_per_packet,
            frame_shape_decode_gap,
            tuple_counts: self
                .tuple_counts
                .into_iter()
                .map(
                    |((group, device_id, command), tuple)| HardwareSniffUsbComSerialTupleCount {
                        group: hex_u8(group),
                        device_id: hex_u8(device_id),
                        command: command.map(hex_u8),
                        payload_len_min: tuple.payload_len_min.unwrap_or(0),
                        payload_len_max: tuple.payload_len_max,
                        count: tuple.count,
                        checksum_valid_count: tuple.checksum_valid_count,
                        checksum_invalid_count: tuple.checksum_invalid_count,
                        sample_frames: tuple.sample_frames,
                    },
                )
                .collect(),
            tuple_sample_limit: USB_COM_SERIAL_TUPLE_SAMPLE_LIMIT,
            native_control_evidence: false,
            readiness_claim: false,
            notes,
        }
    }
}

const USB_COM_SERIAL_FRAME_START: u8 = 0x7e;
const USB_COM_SERIAL_CHECKSUM_MAGIC: u8 = 13;
const USB_COM_SERIAL_MIN_FRAME_LEN: usize = 5;
const USB_COM_SERIAL_BASE_FRAME_LEN: usize = 5;

fn usbcom_serial_checksum(frame_without_checksum: &[u8]) -> u8 {
    frame_without_checksum
        .iter()
        .fold(USB_COM_SERIAL_CHECKSUM_MAGIC, |sum, byte| {
            sum.wrapping_add(*byte)
        })
}

fn bytes_hex_compact_upper(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02X}");
    }
    out
}

const HOST_TO_DEVICE_PAYLOAD_MISSING_PACKET_EXAMPLE_LIMIT: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareSniffHostToDeviceMissingPayloadExample {
    packet_ordinal: usize,
    frame_number: Option<u64>,
    device_key: Option<String>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    interface_number: Option<u16>,
    endpoint_address: Option<String>,
    transfer_type: Option<String>,
    data_len: usize,
    payload_extracted: bool,
    native_control_evidence: bool,
    hardware_output_authorized: bool,
}

#[derive(Default, Debug, Clone)]
struct HardwareSniffHostToDevicePayloadCoverage {
    data_len_packet_count: usize,
    data_len_bytes: usize,
    payload_extracted_packet_count: usize,
    payload_extracted_bytes: usize,
    payload_missing_packet_count: usize,
    payload_missing_packet_examples: Vec<HardwareSniffHostToDeviceMissingPayloadExample>,
}

impl HardwareSniffHostToDevicePayloadCoverage {
    fn observe(&mut self, packet: &TsharkUsbPacket) {
        if let Some(data_len) = packet.data_len.filter(|data_len| *data_len > 0) {
            self.data_len_packet_count = self.data_len_packet_count.saturating_add(1);
            self.data_len_bytes = self.data_len_bytes.saturating_add(data_len);
            if packet.payload.is_none() && !packet.control_setup_stage {
                self.payload_missing_packet_count =
                    self.payload_missing_packet_count.saturating_add(1);
                if self.payload_missing_packet_examples.len()
                    < HOST_TO_DEVICE_PAYLOAD_MISSING_PACKET_EXAMPLE_LIMIT
                {
                    self.payload_missing_packet_examples.push(
                        HardwareSniffHostToDeviceMissingPayloadExample {
                            packet_ordinal: packet.packet_ordinal,
                            frame_number: packet.frame_number,
                            device_key: packet.device_key.clone(),
                            vendor_id: packet.vendor_id.clone(),
                            product_id: packet.product_id.clone(),
                            interface_number: packet.interface_number,
                            endpoint_address: packet.endpoint_address.map(hex_u8),
                            transfer_type: packet
                                .transfer_type
                                .map(|transfer_type| transfer_type.as_str().to_string()),
                            data_len,
                            payload_extracted: false,
                            native_control_evidence: false,
                            hardware_output_authorized: false,
                        },
                    );
                }
            }
        }

        if let Some(payload) = &packet.payload {
            self.payload_extracted_packet_count =
                self.payload_extracted_packet_count.saturating_add(1);
            self.payload_extracted_bytes =
                self.payload_extracted_bytes.saturating_add(payload.len());
        }
    }
}

fn classify_sniff_observed_report(
    direction: SniffUsbDirection,
    report_id: u8,
) -> HardwareSniffReportClassification {
    match direction {
        SniffUsbDirection::HostToDevice => {
            if let Some(label) = standard_pidff_output_report_label(report_id) {
                HardwareSniffReportClassification {
                    category: "standard_pidff_output_report".to_string(),
                    label: label.to_string(),
                    vendor_specific_candidate: false,
                    native_control_evidence: false,
                    notes: vec![
                        "host-to-device report ID matches a standard PIDFF output/control report"
                            .to_string(),
                        "passive sniff classification is protocol research only and is not OpenRacing output evidence"
                            .to_string(),
                    ],
                }
            } else {
                HardwareSniffReportClassification {
                    category: "vendor_or_device_specific_output_candidate".to_string(),
                    label: format!("unknown_host_to_device_report_{}", hex_u8(report_id)),
                    vendor_specific_candidate: true,
                    native_control_evidence: false,
                    notes: vec![
                        "host-to-device report ID is not in the standard PIDFF output/control report set"
                            .to_string(),
                        "decode this external-app traffic before designing any future OpenRacing output plan"
                            .to_string(),
                    ],
                }
            }
        }
        SniffUsbDirection::DeviceToHost => HardwareSniffReportClassification {
            category: "input_or_status_report".to_string(),
            label: format!("device_to_host_report_{}", hex_u8(report_id)),
            vendor_specific_candidate: false,
            native_control_evidence: false,
            notes: vec![
                "device-to-host report observed in passive traffic".to_string(),
                "passive input/status traffic does not prove native control or visible motion"
                    .to_string(),
            ],
        },
    }
}

fn standard_pidff_output_report_label(report_id: u8) -> Option<&'static str> {
    match report_id {
        pidff_report_ids::SET_EFFECT => Some("pidff_set_effect"),
        pidff_report_ids::SET_ENVELOPE => Some("pidff_set_envelope"),
        pidff_report_ids::SET_CONDITION => Some("pidff_set_condition"),
        pidff_report_ids::SET_PERIODIC => Some("pidff_set_periodic"),
        pidff_report_ids::SET_CONSTANT_FORCE => Some("pidff_set_constant_force"),
        pidff_report_ids::SET_RAMP_FORCE => Some("pidff_set_ramp_force"),
        pidff_report_ids::EFFECT_OPERATION => Some("pidff_effect_operation"),
        pidff_report_ids::BLOCK_FREE => Some("pidff_block_free"),
        pidff_report_ids::DEVICE_CONTROL => Some("pidff_device_control"),
        pidff_report_ids::DEVICE_GAIN => Some("pidff_device_gain"),
        pidff_report_ids::CREATE_NEW_EFFECT => Some("pidff_create_new_effect"),
        _ => None,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffDescriptorCandidate {
    kind: String,
    interface_number: Option<u16>,
    payload_sha256: String,
    payload_len: usize,
    extractable: bool,
}

#[derive(Debug, Clone)]
struct TsharkUsbPacket {
    packet_ordinal: usize,
    frame_number: Option<u64>,
    device_key: Option<String>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    interface_number: Option<u16>,
    endpoint_address: Option<u8>,
    direction: Option<SniffUsbDirection>,
    transfer_type: Option<SniffUsbTransferType>,
    data_len: Option<usize>,
    report_id: Option<u8>,
    payload: Option<Vec<u8>>,
    control_setup_stage: bool,
    descriptor_kind: Option<SniffDescriptorKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SniffUsbDirection {
    HostToDevice,
    DeviceToHost,
}

impl SniffUsbDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::HostToDevice => "host_to_device",
            Self::DeviceToHost => "device_to_host",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SniffUsbTransferType {
    Control,
    Interrupt,
    Other,
}

impl SniffUsbTransferType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Interrupt => "interrupt",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SniffDescriptorKind {
    HidReportDescriptor,
    UsbDeviceDescriptor,
    UsbConfigurationDescriptor,
    Other,
}

impl SniffDescriptorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::HidReportDescriptor => "hid_report_descriptor",
            Self::UsbDeviceDescriptor => "usb_device_descriptor",
            Self::UsbConfigurationDescriptor => "usb_configuration_descriptor",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareSniffReadinessClaims {
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
}

impl HardwareSniffReadinessClaims {
    fn none() -> Self {
        Self {
            satisfies_native_response_ready: false,
            satisfies_native_visible_ready: false,
            satisfies_smoke_ready: false,
            satisfies_release_ready: false,
        }
    }

    fn all_false(&self) -> bool {
        !self.satisfies_native_response_ready
            && !self.satisfies_native_visible_ready
            && !self.satisfies_smoke_ready
            && !self.satisfies_release_ready
    }
}

#[derive(Debug, Deserialize)]
struct StoredHardwareSniffPlan {
    schema_version: u32,
    success: bool,
    command: String,
    family: String,
    scenario: String,
    lane: String,
    operator: String,
    device_note: String,
    pre_capture_checklist: Vec<String>,
    post_capture_checklist: Vec<String>,
    operator_notes_required: Vec<String>,
    raw_pcap_commit_default: bool,
    evidence_status: String,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Deserialize)]
struct StoredHardwareSniffReceipt {
    schema_version: u32,
    success: bool,
    command: String,
    generated_at_utc: String,
    plan_path: String,
    pcapng_path: String,
    pcapng_sha256: String,
    pcapng_size_bytes: u64,
    operator: String,
    app: String,
    scenario: String,
    device_note: String,
    evidence: String,
    evidence_status: String,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    openracing_hid_device_opened: bool,
    openracing_ffb_writes: bool,
    openracing_output_reports: bool,
    openracing_feature_reports: bool,
    openracing_serial_config_commands: bool,
    openracing_firmware_or_dfu_commands: bool,
    external_app_observed: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Deserialize)]
struct StoredHardwareSniffNotesTemplateReceipt {
    schema_version: u32,
    success: bool,
    command: String,
    generated_at_utc: String,
    plan_path: String,
    out_path: String,
    scenario: String,
    operator: String,
    device_note: String,
    evidence_status: String,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
}

#[derive(Debug, Deserialize)]
struct StoredHardwareSniffSummary {
    schema_version: u32,
    success: bool,
    command: String,
    pcapng_sha256: String,
    evidence_status: String,
    native_control_evidence: bool,
    openracing_hardware_output: bool,
    external_app_may_have_sent_output: bool,
    satisfies_native_response_ready: bool,
    satisfies_native_visible_ready: bool,
    satisfies_smoke_ready: bool,
    satisfies_release_ready: bool,
    readiness_claims: HardwareSniffReadinessClaims,
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
    windows_pnp: WindowsPnpChecks,
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
    usbpcap_descriptor_capture: UsbPcapDescriptorCaptureChecks,
}

#[derive(Debug, Serialize, Deserialize)]
struct UsbPcapDescriptorCaptureChecks {
    tshark_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tshark_path: Option<String>,
    usbpcap_extcap_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    usbpcap_extcap_path: Option<String>,
    usbpcap_driver_installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    usbpcap_driver_service_state: Option<String>,
    interface_scan_attempted: bool,
    usbpcap_interfaces_present: bool,
    usbpcap_interface_count: usize,
    usbpcap_interfaces: Vec<String>,
    usbpcap_device_scan_attempted: bool,
    usbpcap_moza_device_hint_count: usize,
    usbpcap_moza_device_hints: Vec<UsbPcapMozaDeviceHint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    usbpcap_device_scan_errors: Vec<String>,
    active_usbpcap_processes: UsbPcapActiveProcessChecks,
    ready_for_usbpcap_descriptor_capture: bool,
    access_guidance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UsbPcapMozaDeviceHint {
    usbpcap_interface: String,
    capture_devices_value: String,
    matched_device_values: Vec<String>,
    matched_device_displays: Vec<String>,
    suggested_capture_filter: String,
}

#[derive(Debug)]
struct UsbPcapExtcapDevice {
    value: String,
    display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsbPcapActiveProcessChecks {
    process_scan_attempted: bool,
    active_process_count: usize,
    processes: Vec<UsbPcapActiveProcess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsbPcapActiveProcess {
    process_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    creation_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_line: Option<String>,
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
struct WindowsPnpChecks {
    scan_attempted: bool,
    tool: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    moza_vid_visible: Option<bool>,
    hid_interface_count: usize,
    hid_pnp_device_count: usize,
    serial_interface_count: usize,
    devices: Vec<WindowsPnpDevice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WindowsPnpDevice {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(rename = "class", skip_serializing_if = "Option::is_none")]
    class_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    friendly_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interface_number: Option<i32>,
    instance_id_present: bool,
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

const COMMON_FORBIDDEN_ACTIONS: &[&str] = &[
    "ffb",
    "direct_mode",
    "nonzero_torque",
    "output_reports",
    "feature_reports",
    "serial_config",
    "firmware_dfu",
];

const POST_PASSIVE_FORBIDDEN_ACTIONS: &[&str] = &[
    "ffb",
    "direct_mode",
    "nonzero_torque",
    "high_torque",
    "feature_reports_without_stage",
    "serial_config",
    "firmware_dfu",
];

#[derive(Debug, Serialize, Deserialize)]
struct HardwareBringupRailReceipt {
    success: bool,
    command: &'static str,
    generated_at: String,
    rail_version: u32,
    family: &'static str,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    stages: Vec<HardwareBringupStage>,
    adapter: HardwareFamilyAdapterContract,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareBringupStage {
    id: &'static str,
    order: u8,
    purpose: &'static str,
    required_artifacts: Vec<&'static str>,
    required_gates: Vec<&'static str>,
    forbidden_actions: Vec<&'static str>,
    next_commands: Vec<&'static str>,
    operator_actions: Vec<&'static str>,
    ready_outputs: Vec<&'static str>,
    adapter_requirement_refs: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareFamilyAdapterContract {
    id: &'static str,
    display_name: &'static str,
    known_vid_pids: Vec<&'static str>,
    known_endpoint_roles: Vec<&'static str>,
    default_logical_controls: Vec<&'static str>,
    report_descriptor_expectations: Vec<&'static str>,
    passive_capture_requirements: Vec<&'static str>,
    parser_fixture_requirements: Vec<&'static str>,
    output_capability: &'static str,
    zero_torque_eligibility: &'static str,
    ffb_eligibility: &'static str,
    known_unsafe_surfaces: Vec<&'static str>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneInitReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    lane: String,
    family: &'static str,
    topology: String,
    operator: String,
    captures_dir: String,
    created_files: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneRoleEndpointReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    lane: String,
    family: String,
    topology: String,
    role: String,
    previous_endpoint: String,
    expected_endpoint: String,
    manifest_path: String,
    updated_files: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneScaffoldManifest {
    schema_version: u32,
    generated_at_utc: String,
    lane: String,
    family: &'static str,
    topology: String,
    operator: String,
    completion_state: &'static str,
    rail_stage_order: Vec<&'static str>,
    declared_logical_roles: Vec<HardwareLaneLogicalRole>,
    adapter_known_vid_pids: Vec<&'static str>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneStageGates {
    schema_version: u32,
    generated_at_utc: String,
    family: &'static str,
    topology: String,
    stages: Vec<HardwareBringupStage>,
    adapter: HardwareFamilyAdapterContract,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareLaneLogicalRole {
    id: String,
    required: bool,
    connection_path: String,
    expected_endpoint: String,
    evidence_artifact: String,
    semantic_status: String,
}

#[derive(Debug, Deserialize)]
struct StoredHardwareLaneScaffoldManifest {
    #[serde(default)]
    manifest_source: String,
    family: String,
    topology: String,
    completion_state: String,
    declared_logical_roles: Vec<StoredHardwareLaneLogicalRole>,
}

#[derive(Debug, Deserialize)]
struct StoredHardwareLaneLogicalRole {
    id: String,
    required: bool,
    connection_path: String,
    expected_endpoint: String,
    evidence_artifact: String,
    semantic_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneStatusReceipt {
    success: bool,
    command: &'static str,
    generated_at_utc: String,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_output_reports: bool,
    no_feature_reports: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
    lane: String,
    manifest_source: String,
    family: &'static str,
    topology: String,
    completion_state: String,
    scaffold_required: bool,
    scaffold_complete: bool,
    evidence_claims_validated: bool,
    ready_for_zero_torque: bool,
    ready_for_ffb: bool,
    next_blocked_stage: &'static str,
    safe_next_commands: Vec<String>,
    blocking_items: Vec<String>,
    verifier_receipt: HardwareLaneVerifierReceiptStatus,
    descriptor_capture_tooling: HardwareLaneDescriptorCaptureToolingStatus,
    scaffold_files: Vec<HardwareLaneArtifactStatus>,
    role_evidence: Vec<HardwareLaneRoleEvidenceStatus>,
    stages: Vec<HardwareLaneStageStatus>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneVerifierReceiptStatus {
    path: String,
    present: bool,
    parseable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
    failed_gates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage_blocker: Option<String>,
    guidance: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneDescriptorCaptureToolingStatus {
    hardware_doctor_present: bool,
    hardware_doctor_parseable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tshark_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usbpcap_interfaces_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usbpcap_interface_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_for_usbpcap_descriptor_capture: Option<bool>,
    guidance: String,
}

impl HardwareLaneDescriptorCaptureToolingStatus {
    fn usbpcap_extractor_guidance_available(&self) -> bool {
        self.ready_for_usbpcap_descriptor_capture != Some(false)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneArtifactStatus {
    kind: String,
    relative_path: String,
    present: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneRoleEvidenceStatus {
    id: String,
    required: bool,
    connection_path: String,
    expected_endpoint: String,
    evidence_artifact: String,
    artifact_present: bool,
    semantic_status: String,
    validation_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HardwareLaneStageStatus {
    id: &'static str,
    order: u8,
    purpose: &'static str,
    artifacts_present: usize,
    artifacts_missing: usize,
    artifacts_failed: usize,
    expected_artifacts: Vec<HardwareLaneArtifactStatus>,
    gate_status: &'static str,
    notes: Vec<String>,
}

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
                usbpcap_descriptor_capture: UsbPcapDescriptorCaptureChecks {
                    tshark_present: true,
                    tshark_path: Some("tshark".to_string()),
                    usbpcap_extcap_present: false,
                    usbpcap_extcap_path: None,
                    usbpcap_driver_installed: false,
                    usbpcap_driver_service_state: None,
                    interface_scan_attempted: true,
                    usbpcap_interfaces_present: false,
                    usbpcap_interface_count: 0,
                    usbpcap_interfaces: Vec::new(),
                    usbpcap_device_scan_attempted: false,
                    usbpcap_moza_device_hint_count: 0,
                    usbpcap_moza_device_hints: Vec::new(),
                    usbpcap_device_scan_errors: Vec::new(),
                    active_usbpcap_processes: UsbPcapActiveProcessChecks {
                        process_scan_attempted: true,
                        active_process_count: 0,
                        processes: Vec::new(),
                        error: None,
                    },
                    ready_for_usbpcap_descriptor_capture: false,
                    access_guidance: usbpcap_descriptor_capture_guidance(
                        true, false, false, false, None,
                    ),
                    error: None,
                },
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
            WindowsPnpChecks {
                scan_attempted: true,
                tool: "Get-PnpDevice",
                moza_vid_visible: Some(false),
                hid_interface_count: 0,
                hid_pnp_device_count: 0,
                serial_interface_count: 0,
                devices: Vec::new(),
                error: None,
            },
        )
    }

    fn write_legacy_moza_manifest(lane: &Path, wheelbase: &str) -> TestResult {
        write_legacy_moza_manifest_with_completion(lane, wheelbase, "passive_in_progress")
    }

    fn write_legacy_moza_manifest_with_completion(
        lane: &Path,
        wheelbase: &str,
        completion_state: &str,
    ) -> TestResult {
        fs::write(
            lane.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "completion_state": completion_state,
                "hardware": {
                    "wheelbase": wheelbase,
                    "wheelbase_pid": "0x0004"
                },
                "topology": {
                    "primary_input_path": "wheelbase_hub",
                    "endpoints": [
                        {
                            "id": "moza-r5-if2",
                            "kind": "wheelbase_hub",
                            "vendor_id": "0x346E",
                            "product_id": "0x0004",
                            "interface_number": 2,
                            "usage_page": "0x0001",
                            "usage": "0x0004",
                            "output_capable": true
                        }
                    ],
                    "logical_controls": {
                        "steering": {
                            "role": "steering",
                            "required": true,
                            "connection": "wheelbase_hub",
                            "source_endpoint": "moza-r5-if2",
                            "evidence_capture": "captures/r5-steering-sweep.jsonl",
                            "semantic_status": "proven"
                        },
                        "throttle": {
                            "role": "throttle",
                            "required": true,
                            "connection": "wheelbase_hub",
                            "source_endpoint": "moza-r5-if2",
                            "evidence_capture": "captures/r5-throttle-only-sweep.jsonl",
                            "semantic_status": "proven"
                        },
                        "brake": {
                            "role": "brake",
                            "required": true,
                            "connection": "wheelbase_hub",
                            "source_endpoint": "moza-r5-if2",
                            "evidence_capture": "captures/r5-brake-only-sweep.jsonl",
                            "semantic_status": "generic_aux"
                        },
                        "ks_rim_controls": {
                            "role": "rim_controls",
                            "required": true,
                            "connection": "wheelbase_hub",
                            "source_endpoint": "moza-r5-if2",
                            "evidence_capture": "captures/ks-controls.jsonl",
                            "semantic_status": "proven"
                        },
                        "clutch": {
                            "role": "clutch",
                            "required": false,
                            "connection": "wheelbase_hub",
                            "source_endpoint": "moza-r5-if2",
                            "evidence_capture": "captures/r5-clutch-only-sweep.jsonl",
                            "semantic_status": "generic_aux"
                        }
                    }
                }
            }))?,
        )?;
        Ok(())
    }

    fn write_passive_verification_receipt(lane: &Path, gates: &[(&str, &str)]) -> TestResult {
        fs::write(
            lane.join("passive-verification.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "success": gates.iter().all(|(_, status)| *status == "pass"),
                "command": "wheelctl moza verify-bundle",
                "requested_stage": "passive",
                "gates": gates
                    .iter()
                    .map(|(name, status)| serde_json::json!({
                        "name": name,
                        "status": status,
                        "details": "unit test"
                    }))
                    .collect::<Vec<_>>()
            }))?,
        )?;
        Ok(())
    }

    mod hardware_sniff {
        use super::*;
        use std::path::PathBuf;

        fn sniff_schema_path(file_name: &str) -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../ci/hardware/sniffing")
                .join(file_name)
        }

        fn assert_schema_valid(file_name: &str, value: &serde_json::Value) -> TestResult {
            let schema_text = fs::read_to_string(sniff_schema_path(file_name))?;
            let schema: serde_json::Value = serde_json::from_str(&schema_text)?;
            let validator = jsonschema::Validator::new(&schema)?;
            if let Err(error) = validator.validate(value) {
                return Err(format!("{file_name} validation failed: {error}").into());
            }
            Ok(())
        }

        fn sample_plan(lane: &Path) -> Result<HardwareSniffPlanArtifact> {
            let capture_tools = vec![
                HardwareSniffCaptureTool::Wireshark,
                HardwareSniffCaptureTool::UsbPcap,
                HardwareSniffCaptureTool::Tshark,
            ];
            build_hardware_sniff_plan(&HardwareSniffPlanRequest {
                family: "moza-r5",
                scenario: HardwareSniffScenario::PitHouseOpenIdle,
                lane,
                operator: "Steven",
                device_note: "R5 V2 with KS rim, SR-P pedals, and HBP handbrake",
                capture_tools: &capture_tools,
                platform_hint: Some(HardwareSniffPlatformHint::Windows),
            })
        }

        fn sample_plan_for_scenario(
            lane: &Path,
            scenario: HardwareSniffScenario,
        ) -> Result<HardwareSniffPlanArtifact> {
            let capture_tools = vec![
                HardwareSniffCaptureTool::Wireshark,
                HardwareSniffCaptureTool::UsbPcap,
                HardwareSniffCaptureTool::Tshark,
            ];
            build_hardware_sniff_plan(&HardwareSniffPlanRequest {
                family: "moza-r5",
                scenario,
                lane,
                operator: "Steven",
                device_note: "R5 V2 with KS rim, SR-P pedals, and HBP handbrake",
                capture_tools: &capture_tools,
                platform_hint: Some(HardwareSniffPlatformHint::Windows),
            })
        }

        fn write_sample_plan(dir: &Path) -> Result<PathBuf> {
            let plan = sample_plan(dir)?;
            let plan_path = dir.join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;
            Ok(plan_path)
        }

        fn sample_receipt(dir: &Path, pcapng: &Path) -> Result<HardwareSniffReceiptArtifact> {
            let plan_path = write_sample_plan(dir)?;
            build_hardware_sniff_receipt(&HardwareSniffReceiptRequest {
                plan: &plan_path,
                pcapng: Some(pcapng),
                operator: None,
                app: "MOZA Pit House",
                scenario: None,
                device_note: None,
                evidence: "Pit House was open and idle while host-side USB URBs were captured.",
            })
        }

        fn sample_sniff_capture_request<'a>(
            usbpcapcmd: &'a Path,
            out: &'a Path,
            confirm: bool,
        ) -> HardwareSniffCaptureRequest<'a> {
            HardwareSniffCaptureRequest {
                usbpcapcmd,
                usbpcap_interface: r"\\.\USBPcap2",
                devices: "3",
                duration_ms: 60_000,
                out,
                overwrite: false,
                confirm_external_passive_capture: confirm,
            }
        }

        #[test]
        fn sniff_capture_rejects_missing_confirmation() -> TestResult {
            let dir = tempfile::tempdir()?;
            let usbpcapcmd = dir.path().join("USBPcapCMD.exe");
            fs::write(&usbpcapcmd, b"fake")?;
            let out = dir.path().join("capture.pcapng");
            let request = sample_sniff_capture_request(&usbpcapcmd, &out, false);

            let message = validate_hardware_sniff_capture_request(&request)
                .expect_err("missing confirmation should be rejected")
                .to_string();

            assert!(message.contains("--confirm-external-passive-capture"));
            Ok(())
        }

        #[test]
        fn sniff_capture_rejects_ci_hardware_output_path() -> TestResult {
            let dir = tempfile::tempdir()?;
            let usbpcapcmd = dir.path().join("USBPcapCMD.exe");
            fs::write(&usbpcapcmd, b"fake")?;
            let out = Path::new("ci/hardware/sniff/moza-r5/capture.pcapng");
            let request = sample_sniff_capture_request(&usbpcapcmd, out, true);

            let message = validate_hardware_sniff_capture_request(&request)
                .expect_err("ci/hardware raw capture output should be rejected")
                .to_string();

            assert!(message.contains("ci/hardware"));
            Ok(())
        }

        #[test]
        fn sniff_capture_receipt_is_non_claiming() -> TestResult {
            let dir = tempfile::tempdir()?;
            let usbpcapcmd = dir.path().join("USBPcapCMD.exe");
            fs::write(&usbpcapcmd, b"fake")?;
            let out = dir.path().join("capture.pcapng");
            let request = sample_sniff_capture_request(&usbpcapcmd, &out, true);
            let stdout_path = dir.path().join("capture.usbpcapcmd.stdout.txt");
            let stderr_path = dir.path().join("capture.usbpcapcmd.stderr.txt");

            let receipt = build_hardware_sniff_capture_receipt(
                &request,
                HardwareSniffCaptureOutcome {
                    process_started: true,
                    process_id: Some(123),
                    started_at_utc: "2026-05-22T00:00:00Z".to_string(),
                    completed_at_utc: "2026-05-22T00:01:00Z".to_string(),
                    exit_status: Some("exit code: 1".to_string()),
                    terminated_after_duration: true,
                    pcapng_exists: true,
                    pcapng_size_bytes: 42,
                    pcapng_sha256: Some("abc123".to_string()),
                    stdout_path: stdout_path.clone(),
                    stdout_size_bytes: 7,
                    stderr_path: stderr_path.clone(),
                    stderr_size_bytes: 11,
                },
            )?;

            assert!(receipt.success);
            assert_eq!(receipt.command, "wheelctl hardware sniff-capture");
            assert_eq!(
                receipt.usbpcapcmd_stdout_path,
                required_path_display(&stdout_path, "stdout")?
            );
            assert_eq!(receipt.usbpcapcmd_stdout_size_bytes, 7);
            assert_eq!(
                receipt.usbpcapcmd_stderr_path,
                required_path_display(&stderr_path, "stderr")?
            );
            assert_eq!(receipt.usbpcapcmd_stderr_size_bytes, 11);
            assert!(!receipt.native_control_evidence);
            assert!(!receipt.openracing_hardware_output);
            assert!(!receipt.openracing_hid_device_opened);
            assert!(!receipt.openracing_ffb_writes);
            assert!(!receipt.openracing_output_reports);
            assert!(!receipt.openracing_feature_reports);
            assert!(!receipt.openracing_serial_config_commands);
            assert!(!receipt.openracing_firmware_or_dfu_commands);
            assert!(!receipt.satisfies_native_response_ready);
            assert!(!receipt.satisfies_native_visible_ready);
            assert!(!receipt.satisfies_smoke_ready);
            assert!(!receipt.satisfies_release_ready);
            assert!(!receipt.readiness_claims.satisfies_native_visible_ready);
            Ok(())
        }

        #[test]
        fn sniff_plan_is_non_claiming() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan = sample_plan(dir.path())?;

            assert_eq!(plan.evidence_status, SNIFF_EVIDENCE_STATUS);
            assert!(plan.external_app_may_have_sent_output);
            assert!(!plan.native_control_evidence);
            assert!(!plan.openracing_hardware_output);
            assert!(!plan.satisfies_native_response_ready);
            assert!(!plan.satisfies_native_visible_ready);
            assert!(!plan.satisfies_smoke_ready);
            assert!(!plan.satisfies_release_ready);
            assert!(plan.readiness_claims.all_false());
            assert_schema_valid("sniff-plan.schema.json", &serde_json::to_value(&plan)?)?;
            Ok(())
        }

        #[test]
        fn sniff_plan_lists_forbidden_actions() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan = sample_plan(dir.path())?;
            let markdown = render_sniff_plan_markdown(&plan);

            for action in SNIFF_FORBIDDEN_ACTIONS {
                assert!(
                    plan.forbidden_actions.contains(action),
                    "plan missing forbidden action: {action}"
                );
                assert!(
                    markdown.contains(action),
                    "markdown missing forbidden action: {action}"
                );
            }
            Ok(())
        }

        #[test]
        fn sniff_plan_carries_operator_capture_handoff() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan = sample_plan(dir.path())?;
            let markdown = render_sniff_plan_markdown(&plan);

            assert!(!plan.raw_pcap_commit_default);
            assert!(plan.pre_capture_checklist.iter().any(|item| {
                item == "keep OpenRacing hardware output commands stopped for this passive capture"
            }));
            assert!(plan.post_capture_checklist.iter().any(|item| {
                item == "do not commit raw pcapng unless it is separately reviewed for size, sensitivity, and operator consent"
            }));
            assert!(
                plan.post_capture_checklist
                    .iter()
                    .any(|item| { item == SNIFF_POST_CAPTURE_EVIDENCE_COMMANDS_CHECKLIST_ITEM })
            );
            assert!(
                plan.operator_notes_required
                    .iter()
                    .any(|item| item == "scenario performed")
            );
            assert!(markdown.contains("## Operator Notes Required"));
            assert!(markdown.contains("sniff-notes-template"));
            assert!(markdown.contains("Raw pcapng commit default: `false`"));
            assert_schema_valid("sniff-plan.schema.json", &serde_json::to_value(&plan)?)?;
            Ok(())
        }

        #[test]
        fn setting_change_sniff_plan_requires_exact_setting_notes() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan =
                sample_plan_for_scenario(dir.path(), HardwareSniffScenario::PitHouseSettingChange)?;
            let plan_path = dir.path().join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;
            let stored = read_and_validate_sniff_plan(&plan_path)?;
            let notes = render_sniff_operator_notes_template(&plan_path, &stored, None);

            for field in PIT_HOUSE_SETTING_CHANGE_OPERATOR_NOTES_REQUIRED {
                assert!(
                    plan.operator_notes_required
                        .iter()
                        .any(|required| required == field),
                    "plan missing setting-change operator note: {field}"
                );
                assert!(
                    notes.contains(&format!("- [ ] {field}:")),
                    "operator notes template missing setting-change field: {field}"
                );
            }
            assert_schema_valid("sniff-plan.schema.json", &serde_json::to_value(&plan)?)?;
            Ok(())
        }

        #[test]
        fn setting_change_sniff_plan_schema_rejects_missing_setting_notes() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan =
                sample_plan_for_scenario(dir.path(), HardwareSniffScenario::PitHouseSettingChange)?;
            let mut value = serde_json::to_value(&plan)?;
            let Some(notes) = value
                .get_mut("operator_notes_required")
                .and_then(serde_json::Value::as_array_mut)
            else {
                return Err("expected operator_notes_required array".into());
            };
            notes.retain(|item| item.as_str() != Some("starting setting value"));

            let schema_text = fs::read_to_string(sniff_schema_path("sniff-plan.schema.json"))?;
            let schema: serde_json::Value = serde_json::from_str(&schema_text)?;
            let validator = jsonschema::Validator::new(&schema)?;
            if validator.validate(&value).is_ok() {
                return Err("setting-change plan without starting value should fail schema".into());
            }
            Ok(())
        }

        #[test]
        fn sniff_notes_template_renders_required_operator_fields() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan_path = write_sample_plan(dir.path())?;
            let plan = read_and_validate_sniff_plan(&plan_path)?;
            let notes = render_sniff_operator_notes_template(&plan_path, &plan, None);

            assert!(notes.contains("# Passive USB Sniff Operator Notes"));
            assert!(notes.contains("- [ ] scenario performed:"));
            assert!(notes.contains("- [ ] device stack attached:"));
            assert!(notes.contains("OpenRacing sent no output"));
            assert!(notes.contains("native visible"));
            assert!(notes.contains("sniff-notes-template"));
            assert!(notes.contains("Raw pcapng commit default remained `false`"));
            Ok(())
        }

        #[test]
        fn sniff_notes_template_renders_hardware_doctor_usbpcap_hints() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan =
                sample_plan_for_scenario(dir.path(), HardwareSniffScenario::PitHouseSettingChange)?;
            let plan_path = dir.path().join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;
            let plan = read_and_validate_sniff_plan(&plan_path)?;
            let doctor_path = dir.path().join("hardware-doctor.json");
            let doctor = serde_json::json!({
                "no_hid_device_opened": true,
                "no_ffb_writes": true,
                "no_output_reports": true,
                "no_feature_reports": true,
                "no_serial_config_commands": true,
                "no_firmware_or_dfu_commands": true,
                "tools": {
                    "usbpcap_descriptor_capture": {
                        "usbpcap_extcap_path": "C:\\Program Files\\Wireshark\\extcap\\USBPcapCMD.exe",
                        "active_usbpcap_processes": {
                            "process_scan_attempted": true,
                            "active_process_count": 1,
                            "processes": [
                                {
                                    "process_id": 508388,
                                    "command_line": "\"C:\\Program Files\\Wireshark\\extcap\\USBPcapCMD.exe\" -d \\\\.\\USBPcap2 --devices 3 -o target\\sniff\\old-probe\\capture.pcap"
                                }
                            ]
                        },
                        "usbpcap_moza_device_hints": [
                            {
                                "usbpcap_interface": "\\\\.\\USBPcap2",
                                "capture_devices_value": "3",
                                "matched_device_displays": [
                                    "USB Serial Device (COM4)",
                                    "MOZA Windows Driver"
                                ],
                                "suggested_capture_filter": "select \\\\.\\USBPcap2 with USBPcap --devices 3"
                            }
                        ]
                    }
                }
            });
            write_json_file(&doctor_path, &doctor)?;

            let capture_hints = sniff_notes_capture_hints_from_hardware_doctor(Some(&doctor_path))?
                .ok_or("expected capture hints")?;
            let notes =
                render_sniff_operator_notes_template(&plan_path, &plan, Some(&capture_hints));

            assert!(capture_hints.receipt_flags.all_no_output_flags_true());
            assert_eq!(
                capture_hints.usbpcap_extcap_path.as_deref(),
                Some(r"C:\Program Files\Wireshark\extcap\USBPcapCMD.exe")
            );
            assert_eq!(capture_hints.hint_count, 1);
            assert_eq!(capture_hints.active_usbpcap_process_count, 1);
            assert!(notes.contains("## Capture Tool Hints"));
            assert!(notes.contains("USBPcap interface used: `\\\\.\\USBPcap2`"));
            assert!(notes.contains("USBPcap device filter used: `--devices 3`"));
            let expected_command = r#"& "C:\Program Files\Wireshark\extcap\USBPcapCMD.exe" -d "\\.\USBPcap2" --devices 3 --inject-descriptors -o "target\sniff\pit-house-setting-change\capture.pcapng""#;
            assert!(
                notes.contains(expected_command),
                "operator notes missing capture command:\n{notes}"
            );
            assert!(notes.contains("wheelctl hardware sniff-capture"));
            assert!(notes.contains("--duration-ms 60000"));
            assert!(notes.contains("--confirm-external-passive-capture"));
            assert!(notes.contains("sniff-capture-receipt.json"));
            assert!(notes.contains("MOZA Windows Driver"));
            assert!(notes.contains("Active USBPcapCMD processes detected before capture: `1`"));
            assert!(notes.contains("old-probe"));
            assert!(notes.contains("OpenRacing output"));
            Ok(())
        }

        #[tokio::test]
        async fn sniff_notes_template_writes_json_receipt_when_requested() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan =
                sample_plan_for_scenario(dir.path(), HardwareSniffScenario::PitHouseSettingChange)?;
            let plan_path = dir.path().join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;
            let notes_path = dir.path().join("operator-notes.md");
            let json_out = dir.path().join("sniff-notes-template-receipt.json");

            sniff_notes_template(false, &plan_path, None, &notes_path, Some(&json_out)).await?;

            let value: serde_json::Value = read_json_file(&json_out)?;
            assert!(notes_path.is_file());
            assert_eq!(
                value.get("command").and_then(serde_json::Value::as_str),
                Some("wheelctl hardware sniff-notes-template")
            );
            assert_eq!(
                value
                    .get("openracing_hardware_output")
                    .and_then(serde_json::Value::as_bool),
                Some(false)
            );
            assert_eq!(
                value
                    .get("satisfies_native_visible_ready")
                    .and_then(serde_json::Value::as_bool),
                Some(false)
            );
            assert_eq!(
                value.get("scenario").and_then(serde_json::Value::as_str),
                Some("pit-house-setting-change")
            );
            Ok(())
        }

        #[test]
        fn sniff_plan_validation_rejects_stale_notes_template_handoff() -> TestResult {
            let dir = tempfile::tempdir()?;
            let mut plan = serde_json::to_value(sample_plan(dir.path())?)?;
            let Some(items) = plan
                .get_mut("post_capture_checklist")
                .and_then(serde_json::Value::as_array_mut)
            else {
                return Err("expected post_capture_checklist array".into());
            };
            items.retain(|item| {
                item.as_str() != Some(SNIFF_POST_CAPTURE_EVIDENCE_COMMANDS_CHECKLIST_ITEM)
            });
            let plan_path = dir.path().join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;

            let error = read_and_validate_sniff_plan(&plan_path)
                .err()
                .ok_or("expected stale sniff plan to be rejected")?;
            assert!(
                error
                    .to_string()
                    .contains("missing the passive capture operator handoff"),
                "unexpected error: {error}"
            );
            Ok(())
        }

        #[test]
        fn sniff_receipt_hashes_pcapng() -> TestResult {
            let dir = tempfile::tempdir()?;
            let pcapng = dir.path().join("capture.pcapng");
            fs::write(&pcapng, b"abc")?;

            let receipt = sample_receipt(dir.path(), &pcapng)?;

            assert_eq!(
                receipt.pcapng_sha256,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
            assert_eq!(receipt.pcapng_size_bytes, 3);
            assert_schema_valid(
                "sniff-receipt.schema.json",
                &serde_json::to_value(&receipt)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_receipt_rejects_missing_pcapng_unless_no_pcapng() -> TestResult {
            let dir = tempfile::tempdir()?;
            let plan_path = write_sample_plan(dir.path())?;
            let request_without_pcapng = HardwareSniffReceiptRequest {
                plan: &plan_path,
                pcapng: None,
                operator: None,
                app: "MOZA Pit House",
                scenario: None,
                device_note: None,
                evidence: "Operator recorded notes but no pcapng was supplied.",
            };

            let no_pcapng_error = match build_hardware_sniff_receipt(&request_without_pcapng) {
                Ok(_) => return Err("missing pcapng should be rejected".into()),
                Err(error) => error.to_string(),
            };
            assert!(
                no_pcapng_error.contains("missing required pcapng capture"),
                "{no_pcapng_error}"
            );

            let missing_pcapng = dir.path().join("missing.pcapng");
            let request_with_missing_pcapng = HardwareSniffReceiptRequest {
                plan: &plan_path,
                pcapng: Some(&missing_pcapng),
                operator: None,
                app: "MOZA Pit House",
                scenario: None,
                device_note: None,
                evidence: "Operator supplied a path that does not exist.",
            };
            let missing_file_error =
                match build_hardware_sniff_receipt(&request_with_missing_pcapng) {
                    Ok(_) => return Err("missing pcapng file should be rejected".into()),
                    Err(error) => error.to_string(),
                };
            assert!(
                missing_file_error.contains("pcapng capture not found"),
                "{missing_file_error}"
            );
            Ok(())
        }

        #[test]
        fn sniff_receipt_cannot_satisfy_native_or_smoke_gates() -> TestResult {
            let dir = tempfile::tempdir()?;
            let pcapng = dir.path().join("capture.pcapng");
            fs::write(&pcapng, b"passive capture bytes")?;

            let receipt = sample_receipt(dir.path(), &pcapng)?;

            assert_eq!(receipt.evidence_status, SNIFF_EVIDENCE_STATUS);
            assert!(receipt.external_app_observed);
            assert!(receipt.external_app_may_have_sent_output);
            assert!(!receipt.native_control_evidence);
            assert!(!receipt.openracing_hardware_output);
            assert!(!receipt.openracing_hid_device_opened);
            assert!(!receipt.openracing_ffb_writes);
            assert!(!receipt.openracing_output_reports);
            assert!(!receipt.openracing_feature_reports);
            assert!(!receipt.openracing_serial_config_commands);
            assert!(!receipt.openracing_firmware_or_dfu_commands);
            assert!(!receipt.satisfies_native_response_ready);
            assert!(!receipt.satisfies_native_visible_ready);
            assert!(!receipt.satisfies_smoke_ready);
            assert!(!receipt.satisfies_release_ready);
            assert!(receipt.readiness_claims.all_false());
            Ok(())
        }
    }

    mod hardware_sniff_bundle {
        use super::*;
        use std::path::{Path, PathBuf};

        struct BundleFixturePaths {
            plan: PathBuf,
            receipt: PathBuf,
            summary: PathBuf,
            operator_notes: PathBuf,
            pcapng: PathBuf,
            out: PathBuf,
        }

        fn sniff_schema_path(file_name: &str) -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../ci/hardware/sniffing")
                .join(file_name)
        }

        fn assert_schema_valid(file_name: &str, value: &serde_json::Value) -> TestResult {
            let schema_text = fs::read_to_string(sniff_schema_path(file_name))?;
            let schema: serde_json::Value = serde_json::from_str(&schema_text)?;
            let validator = jsonschema::Validator::new(&schema)?;
            if let Err(error) = validator.validate(value) {
                return Err(format!("{file_name} validation failed: {error}").into());
            }
            Ok(())
        }

        fn sample_plan_for_scenario(
            lane: &Path,
            scenario: HardwareSniffScenario,
        ) -> Result<HardwareSniffPlanArtifact> {
            let capture_tools = vec![
                HardwareSniffCaptureTool::Wireshark,
                HardwareSniffCaptureTool::UsbPcap,
                HardwareSniffCaptureTool::Tshark,
            ];
            build_hardware_sniff_plan(&HardwareSniffPlanRequest {
                family: "moza-r5",
                scenario,
                lane,
                operator: "Steven",
                device_note: "R5 V2 with KS rim, SR-P pedals, and HBP handbrake",
                capture_tools: &capture_tools,
                platform_hint: Some(HardwareSniffPlatformHint::Windows),
            })
        }

        fn sample_summary(pcapng_sha256: String) -> HardwareSniffSummaryArtifact {
            let observed_reports = vec![HardwareSniffObservedReport {
                direction: "device_to_host".to_string(),
                report_id: "0x05".to_string(),
                classification: classify_sniff_observed_report(
                    SniffUsbDirection::DeviceToHost,
                    0x05,
                ),
                count: 1,
                payload_sample_count: 1,
                payload_sha256_examples: vec![sha256_hex(b"synthetic report payload")],
                payload_hex_samples: None,
            }];
            let report_classification_summary = summarize_sniff_report_classifications(
                &observed_reports,
                0,
                HardwareSniffHostToDevicePayloadCoverage::default(),
                HardwareSniffUsbComSerialFrameSummaryBuilder::default().build(),
            );

            HardwareSniffSummaryArtifact {
                schema_version: 1,
                success: true,
                command: "wheelctl hardware sniff-summary",
                generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
                pcapng_sha256,
                reason: None,
                tool: HardwareSniffSummaryTool {
                    tshark_present: true,
                    tshark_version: Some("TShark synthetic fixture".to_string()),
                },
                filters: HardwareSniffSummaryFilters {
                    vendor_id: Some("0x346E".to_string()),
                    product_id: Some("0x0014".to_string()),
                    interface_number: Some(2),
                },
                matched_packets: 1,
                usb_transfer_summary: HardwareSniffUsbTransferSummary {
                    host_to_device: 0,
                    device_to_host: 1,
                    control: 0,
                    interrupt: 1,
                },
                observed_devices: vec![HardwareSniffObservedDevice {
                    vendor_id: "0x346E".to_string(),
                    product_id: "0x0014".to_string(),
                    interfaces: vec![2],
                    endpoints: vec!["0x81".to_string()],
                }],
                observed_reports,
                report_classification_summary,
                descriptor_candidates: Vec::new(),
                evidence_status: SNIFF_EVIDENCE_STATUS,
                native_control_evidence: false,
                openracing_hardware_output: false,
                external_app_may_have_sent_output: true,
                satisfies_native_response_ready: false,
                satisfies_native_visible_ready: false,
                satisfies_smoke_ready: false,
                satisfies_release_ready: false,
                readiness_claims: HardwareSniffReadinessClaims::none(),
                notes: vec![
                    "passive sniff summary is protocol research/support evidence only".to_string(),
                ],
            }
        }

        fn write_bundle_fixture_for_scenario(
            dir: &Path,
            scenario: HardwareSniffScenario,
            pcapng_bytes: &[u8],
            operator_notes_text: &str,
        ) -> Result<BundleFixturePaths> {
            let pcapng = dir.join("capture.pcapng");
            fs::write(&pcapng, pcapng_bytes)?;

            let plan = sample_plan_for_scenario(dir, scenario)?;
            let plan_path = dir.join("sniff-plan.json");
            write_json_file(&plan_path, &plan)?;

            let receipt = build_hardware_sniff_receipt(&HardwareSniffReceiptRequest {
                plan: &plan_path,
                pcapng: Some(&pcapng),
                operator: None,
                app: "MOZA Pit House",
                scenario: None,
                device_note: None,
                evidence: "Pit House was open and idle while host-side USB URBs were captured.",
            })?;
            let receipt_path = dir.join("sniff-receipt.json");
            write_json_file(&receipt_path, &receipt)?;

            let summary = sample_summary(receipt.pcapng_sha256.clone());
            let summary_path = dir.join("sniff-summary.json");
            write_json_file(&summary_path, &summary)?;

            let operator_notes = dir.join("operator-notes.md");
            fs::write(&operator_notes, operator_notes_text)?;

            Ok(BundleFixturePaths {
                plan: plan_path,
                receipt: receipt_path,
                summary: summary_path,
                operator_notes,
                pcapng,
                out: dir.join("openracing-sniff-bundle.zip"),
            })
        }

        fn write_bundle_fixture(dir: &Path, pcapng_bytes: &[u8]) -> Result<BundleFixturePaths> {
            write_bundle_fixture_for_scenario(
                dir,
                HardwareSniffScenario::PitHouseOpenIdle,
                pcapng_bytes,
                "# Operator Notes\n\nPassive USB observation fixture.\n",
            )
        }

        fn build_bundle(
            paths: &BundleFixturePaths,
            include_raw: bool,
        ) -> Result<HardwareSniffBundleManifest> {
            build_hardware_sniff_bundle(&HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: None,
                include_pcapng: include_raw.then_some(paths.pcapng.as_path()),
                out: &paths.out,
            })
        }

        fn write_notes_template_receipt(paths: &BundleFixturePaths) -> Result<PathBuf> {
            let plan = read_and_validate_sniff_plan(&paths.plan)?;
            let Some(parent) = paths.operator_notes.parent() else {
                anyhow::bail!("operator notes path should have parent");
            };
            let operator_notes_receipt = parent.join("sniff-notes-template-receipt.json");
            let receipt = HardwareSniffNotesTemplateReceipt {
                schema_version: 1,
                success: true,
                command: "wheelctl hardware sniff-notes-template",
                generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
                plan_path: required_path_display(&paths.plan, "plan")?,
                hardware_doctor_path: None,
                out_path: required_path_display(&paths.operator_notes, "operator notes")?,
                scenario: plan.scenario,
                operator: plan.operator,
                device_note: plan.device_note,
                capture_hints: None,
                evidence_status: SNIFF_EVIDENCE_STATUS,
                native_control_evidence: false,
                openracing_hardware_output: false,
                satisfies_native_response_ready: false,
                satisfies_native_visible_ready: false,
                satisfies_smoke_ready: false,
                satisfies_release_ready: false,
                readiness_claims: HardwareSniffReadinessClaims::none(),
            };
            write_json_file(&operator_notes_receipt, &receipt)?;
            Ok(operator_notes_receipt)
        }

        fn zip_entry_names(path: &Path) -> Result<Vec<String>> {
            let file = File::open(path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            let mut names = Vec::new();
            for index in 0..archive.len() {
                let entry = archive.by_index(index)?;
                names.push(entry.name().to_string());
            }
            Ok(names)
        }

        fn read_zip_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
            let file = File::open(path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            let mut entry = archive.by_name(name)?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            Ok(bytes)
        }

        #[test]
        fn sniff_bundle_excludes_raw_pcapng_by_default() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;

            let manifest = build_bundle(&paths, false)?;
            let names = zip_entry_names(&paths.out)?;

            assert!(!manifest.includes_raw_pcapng);
            assert!(!names.contains(&sniff_bundle_path("capture.pcapng")));
            assert!(names.contains(&sniff_bundle_path("README.md")));
            assert!(names.contains(&sniff_bundle_path("sniff-plan.json")));
            assert!(names.contains(&sniff_bundle_path("sniff-receipt.json")));
            assert!(names.contains(&sniff_bundle_path("sniff-summary.json")));
            assert!(names.contains(&sniff_bundle_path("operator-notes.md")));
            assert!(names.contains(&sniff_bundle_path("pcapng-sha256.txt")));
            assert!(names.contains(&sniff_bundle_path("sniff-bundle-manifest.json")));
            Ok(())
        }

        #[test]
        fn sniff_bundle_includes_operator_notes_receipt_when_requested() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let operator_notes_receipt = write_notes_template_receipt(&paths)?;
            let receipt_archive_path = sniff_bundle_path("sniff-notes-template-receipt.json");

            let manifest = build_hardware_sniff_bundle(&HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: Some(&operator_notes_receipt),
                include_pcapng: None,
                out: &paths.out,
            })?;
            let names = zip_entry_names(&paths.out)?;
            let receipt_bytes = read_zip_entry(&paths.out, &receipt_archive_path)?;

            assert!(names.contains(&receipt_archive_path));
            assert_eq!(receipt_bytes, fs::read(&operator_notes_receipt)?);
            assert!(manifest.artifacts.iter().any(|artifact| {
                artifact.path == receipt_archive_path
                    && artifact.sha256 == sha256_hex(&receipt_bytes)
            }));
            assert!(!manifest.openracing_hardware_output);
            assert!(!manifest.satisfies_native_visible_ready);
            Ok(())
        }

        #[test]
        fn rejects_setting_change_bundle_with_blank_required_operator_notes() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture_for_scenario(
                dir.path(),
                HardwareSniffScenario::PitHouseSettingChange,
                b"synthetic pcapng bytes",
                "# Operator Notes\n\n- [ ] exact Pit House setting changed:\n- [ ] starting setting value:\n- [ ] ending setting value:\n- [ ] whether the setting value was restored:\n",
            )?;

            let error = match build_bundle(&paths, false) {
                Ok(_) => {
                    return Err(
                        "setting-change bundle with blank operator notes should be rejected".into(),
                    );
                }
                Err(error) => error.to_string(),
            };

            assert!(
                error
                    .contains("missing completed required field 'exact Pit House setting changed'"),
                "{error}"
            );
            Ok(())
        }

        #[test]
        fn accepts_setting_change_bundle_with_completed_required_operator_notes() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture_for_scenario(
                dir.path(),
                HardwareSniffScenario::PitHouseSettingChange,
                b"synthetic pcapng bytes",
                "# Operator Notes\n\n- [x] exact Pit House setting changed: Road Sensitivity\n- [x] starting setting value: 5\n- [x] ending setting value: 6\n- [x] whether the setting value was restored: yes, restored to 5\n",
            )?;

            let manifest = build_bundle(&paths, false)?;
            let names = zip_entry_names(&paths.out)?;

            assert!(manifest.success);
            assert!(names.contains(&sniff_bundle_path("operator-notes.md")));
            assert!(!manifest.openracing_hardware_output);
            assert!(!manifest.satisfies_native_visible_ready);
            Ok(())
        }

        #[test]
        fn rejects_operator_notes_receipt_plan_path_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let operator_notes_receipt = write_notes_template_receipt(&paths)?;
            let mut receipt: serde_json::Value = read_json_file(&operator_notes_receipt)?;
            receipt["plan_path"] = serde_json::json!("target/sniff/other/sniff-plan.json");
            write_json_file(&operator_notes_receipt, &receipt)?;

            let error = match build_hardware_sniff_bundle(&HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: Some(&operator_notes_receipt),
                include_pcapng: None,
                out: &paths.out,
            }) {
                Ok(_) => {
                    return Err(
                        "operator notes receipt plan_path mismatch should be rejected".into(),
                    );
                }
                Err(error) => error.to_string(),
            };

            assert!(error.contains("was created for plan"), "{error}");
            Ok(())
        }

        #[test]
        fn rejects_operator_notes_receipt_out_path_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let operator_notes_receipt = write_notes_template_receipt(&paths)?;
            let mut receipt: serde_json::Value = read_json_file(&operator_notes_receipt)?;
            receipt["out_path"] = serde_json::json!("target/sniff/other/operator-notes.md");
            write_json_file(&operator_notes_receipt, &receipt)?;

            let error = match build_hardware_sniff_bundle(&HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: Some(&operator_notes_receipt),
                include_pcapng: None,
                out: &paths.out,
            }) {
                Ok(_) => {
                    return Err(
                        "operator notes receipt out_path mismatch should be rejected".into(),
                    );
                }
                Err(error) => error.to_string(),
            };

            assert!(error.contains("was created for operator notes"), "{error}");
            Ok(())
        }

        #[test]
        fn rejects_operator_notes_receipt_claiming_readiness() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let operator_notes_receipt = write_notes_template_receipt(&paths)?;
            let mut receipt: serde_json::Value = read_json_file(&operator_notes_receipt)?;
            receipt["satisfies_native_visible_ready"] = serde_json::json!(true);
            write_json_file(&operator_notes_receipt, &receipt)?;

            let error = match build_hardware_sniff_bundle(&HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: Some(&operator_notes_receipt),
                include_pcapng: None,
                out: &paths.out,
            }) {
                Ok(_) => {
                    return Err("operator notes receipt readiness claim should be rejected".into());
                }
                Err(error) => error.to_string(),
            };

            assert!(
                error.contains("violates passive sniff invariants"),
                "{error}"
            );
            Ok(())
        }

        #[test]
        fn sniff_bundle_includes_raw_pcapng_only_when_requested() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;

            let _manifest_without_raw = build_bundle(&paths, false)?;
            let default_names = zip_entry_names(&paths.out)?;
            assert!(!default_names.contains(&sniff_bundle_path("capture.pcapng")));

            let manifest_with_raw = build_bundle(&paths, true)?;
            let requested_names = zip_entry_names(&paths.out)?;
            let capture_path = sniff_bundle_path("capture.pcapng");
            let capture_bytes = read_zip_entry(&paths.out, &capture_path)?;

            assert!(manifest_with_raw.includes_raw_pcapng);
            assert!(requested_names.contains(&capture_path));
            assert_eq!(capture_bytes, b"synthetic pcapng bytes");
            Ok(())
        }

        #[test]
        fn sniff_bundle_manifest_hashes_all_artifacts() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;

            let manifest = build_bundle(&paths, true)?;
            let manifest_path = sniff_bundle_path("sniff-bundle-manifest.json");
            let mut zip_hashes = BTreeMap::new();
            for name in zip_entry_names(&paths.out)? {
                if name == manifest_path {
                    continue;
                }
                let bytes = read_zip_entry(&paths.out, &name)?;
                zip_hashes.insert(name, sha256_hex(&bytes));
            }
            let manifest_hashes = manifest
                .artifacts
                .iter()
                .map(|artifact| (artifact.path.clone(), artifact.sha256.clone()))
                .collect::<BTreeMap<_, _>>();

            assert!(!manifest_hashes.contains_key(&manifest_path));
            assert_eq!(manifest_hashes, zip_hashes);
            Ok(())
        }

        #[test]
        fn sniff_bundle_manifest_is_non_claiming() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;

            let manifest = build_bundle(&paths, false)?;
            let value = serde_json::to_value(&manifest)?;
            assert_schema_valid("sniff-bundle-manifest.schema.json", &value)?;

            assert_eq!(manifest.bundle_kind, SNIFF_BUNDLE_KIND);
            assert_eq!(manifest.evidence_status, SNIFF_EVIDENCE_STATUS);
            assert!(!manifest.native_control_evidence);
            assert!(!manifest.openracing_hardware_output);
            assert!(manifest.external_app_may_have_sent_output);
            assert!(!manifest.satisfies_native_response_ready);
            assert!(!manifest.satisfies_native_visible_ready);
            assert!(!manifest.satisfies_smoke_ready);
            assert!(!manifest.satisfies_release_ready);
            assert!(manifest.readiness_claims.all_false());
            Ok(())
        }

        #[tokio::test]
        async fn sniff_bundle_writes_json_manifest_when_requested() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let json_out = dir.path().join("sniff-bundle-manifest.json");
            let request = HardwareSniffBundleRequest {
                plan: &paths.plan,
                receipt: &paths.receipt,
                summary: &paths.summary,
                operator_notes: &paths.operator_notes,
                operator_notes_receipt: None,
                include_pcapng: None,
                out: &paths.out,
            };

            sniff_bundle(false, &request, Some(&json_out)).await?;

            let value: serde_json::Value = read_json_file(&json_out)?;
            assert_schema_valid("sniff-bundle-manifest.schema.json", &value)?;
            assert_eq!(
                value.get("command").and_then(serde_json::Value::as_str),
                Some("wheelctl hardware sniff-bundle")
            );
            assert_eq!(
                value
                    .get("openracing_hardware_output")
                    .and_then(serde_json::Value::as_bool),
                Some(false)
            );
            assert_eq!(
                value
                    .get("satisfies_native_visible_ready")
                    .and_then(serde_json::Value::as_bool),
                Some(false)
            );
            assert!(paths.out.is_file());
            Ok(())
        }

        #[test]
        fn sniff_bundle_accepts_zero_match_summary() -> TestResult {
            let dir = tempfile::tempdir()?;
            let pcapng_bytes = b"synthetic pcapng bytes";
            let paths = write_bundle_fixture(dir.path(), pcapng_bytes)?;
            let mut summary = sample_summary(sha256_hex(pcapng_bytes));
            summary.success = false;
            summary.reason = Some("no USB packets matched the requested filters".to_string());
            summary.matched_packets = 0;
            summary.usb_transfer_summary = HardwareSniffUsbTransferSummary {
                host_to_device: 0,
                device_to_host: 0,
                control: 0,
                interrupt: 0,
            };
            summary.observed_devices.clear();
            summary.observed_reports.clear();
            summary.descriptor_candidates.clear();

            let summary_value = serde_json::to_value(&summary)?;
            assert_schema_valid("sniff-summary.schema.json", &summary_value)?;
            write_json_file(&paths.summary, &summary)?;

            let manifest = build_bundle(&paths, false)?;
            let bundled_summary: serde_json::Value = serde_json::from_slice(&read_zip_entry(
                &paths.out,
                &sniff_bundle_path("sniff-summary.json"),
            )?)?;

            assert!(manifest.success);
            assert!(!manifest.includes_raw_pcapng);
            assert_eq!(
                bundled_summary.get("success"),
                Some(&serde_json::json!(false))
            );
            assert_eq!(
                bundled_summary.get("reason"),
                Some(&serde_json::json!(
                    "no USB packets matched the requested filters"
                ))
            );
            Ok(())
        }

        #[test]
        fn rejects_raw_pcapng_hash_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"original pcapng bytes")?;
            fs::write(&paths.pcapng, b"changed pcapng bytes")?;

            let error = match build_bundle(&paths, true) {
                Ok(_) => return Err("raw pcapng hash mismatch should be rejected".into()),
                Err(error) => error.to_string(),
            };

            assert!(error.contains("does not match sniff receipt"), "{error}");
            Ok(())
        }

        #[test]
        fn rejects_receipt_plan_path_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let mut receipt: serde_json::Value =
                serde_json::from_slice(&fs::read(&paths.receipt)?)?;
            receipt["plan_path"] = serde_json::json!("target/sniff/other/sniff-plan.json");
            write_json_file(&paths.receipt, &receipt)?;

            let error = match build_bundle(&paths, false) {
                Ok(_) => return Err("receipt plan_path mismatch should be rejected".into()),
                Err(error) => error.to_string(),
            };

            assert!(error.contains("was created for plan"), "{error}");
            Ok(())
        }

        #[test]
        fn rejects_raw_pcapng_path_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let mut receipt: serde_json::Value =
                serde_json::from_slice(&fs::read(&paths.receipt)?)?;
            receipt["pcapng_path"] = serde_json::json!("target/sniff/other/capture.pcapng");
            write_json_file(&paths.receipt, &receipt)?;

            let error = match build_bundle(&paths, true) {
                Ok(_) => return Err("raw pcapng path mismatch should be rejected".into()),
                Err(error) => error.to_string(),
            };

            assert!(
                error.contains("does not match sniff receipt pcapng_path"),
                "{error}"
            );
            Ok(())
        }

        #[test]
        fn rejects_raw_pcapng_size_mismatch() -> TestResult {
            let dir = tempfile::tempdir()?;
            let paths = write_bundle_fixture(dir.path(), b"synthetic pcapng bytes")?;
            let mut receipt: serde_json::Value =
                serde_json::from_slice(&fs::read(&paths.receipt)?)?;
            receipt["pcapng_size_bytes"] = serde_json::json!(1);
            write_json_file(&paths.receipt, &receipt)?;

            let error = match build_bundle(&paths, true) {
                Ok(_) => return Err("raw pcapng size mismatch should be rejected".into()),
                Err(error) => error.to_string(),
            };

            assert!(error.contains("pcapng_size_bytes"), "{error}");
            Ok(())
        }
    }

    mod hardware_sniff_summary {
        use super::*;
        use std::path::PathBuf;

        const TSHARK_JSON_FIXTURE: &str = r#"[
          {
            "_source": {
              "layers": {
                "frame": { "frame.number": "1" },
                "usb": {
                  "usb.bus_id": "1",
                  "usb.device_address": "12",
                  "usb.idVendor": "0x346e",
                  "usb.idProduct": "0x0014",
                  "usb.interface_number": "2",
                  "usb.endpoint_address": "0x81",
                  "usb.endpoint_direction": "IN",
                  "usb.transfer_type": "Interrupt"
                },
                "usbhid": {
                  "usbhid.report_id": "0x05",
                  "usbhid.data": "05:10:20:30"
                }
              }
            }
          },
          {
            "_source": {
              "layers": {
                "frame": { "frame.number": "2" },
                "usb": {
                  "usb.bus_id": "1",
                  "usb.device_address": "12",
                  "usb.idVendor": "0x346e",
                  "usb.idProduct": "0x0014",
                  "usb.interface_number": "2",
                  "usb.endpoint_address": "0x02",
                  "usb.endpoint_direction": "OUT",
                  "usb.transfer_type": "Interrupt"
                },
                "usbhid": {
                  "usbhid.report_id": "0x20",
                  "usbhid.data": "20:00:01:02:03:04:05:06"
                }
              }
            }
          },
          {
            "_source": {
              "layers": {
                "frame": { "frame.number": "3" },
                "usb": {
                  "usb.bus_id": "1",
                  "usb.device_address": "12",
                  "usb.idVendor": "0x346e",
                  "usb.idProduct": "0x0014",
                  "usb.interface_number": "2",
                  "usb.endpoint_address": "0x02",
                  "usb.endpoint_direction": "OUT",
                  "usb.transfer_type": "Interrupt"
                },
                "usbhid": {
                  "usbhid.report_id": "0x05",
                  "usbhid.data": "05:01:00"
                }
              }
            }
          },
          {
            "_source": {
              "layers": {
                "frame": { "frame.number": "4" },
                "usb": {
                  "usb.bus_id": "1",
                  "usb.device_address": "12",
                  "usb.idVendor": "0x346e",
                  "usb.idProduct": "0x0014",
                  "usb.interface_number": "2",
                  "usb.endpoint_address": "0x00",
                  "usb.transfer_type": "Control",
                  "usb.descriptor_type": "0x22",
                  "usb.capdata": "05:01:09:04"
                }
              }
            }
          }
        ]"#;

        fn sniff_schema_path(file_name: &str) -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../ci/hardware/sniffing")
                .join(file_name)
        }

        fn assert_schema_valid(file_name: &str, value: &serde_json::Value) -> TestResult {
            let schema_text = fs::read_to_string(sniff_schema_path(file_name))?;
            let schema: serde_json::Value = serde_json::from_str(&schema_text)?;
            let validator = jsonschema::Validator::new(&schema)?;
            if let Err(error) = validator.validate(value) {
                return Err(format!("{file_name} validation failed: {error}").into());
            }
            Ok(())
        }

        fn fixture_summary(include_payload_samples: bool) -> Result<HardwareSniffSummaryArtifact> {
            build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0x346E".to_string()),
                        product_id: Some("0x0014".to_string()),
                        interface_number: Some(2),
                    },
                    include_payload_samples,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"synthetic pcapng fixture"),
                true,
                Some("TShark synthetic fixture".to_string()),
                TSHARK_JSON_FIXTURE,
            )
        }

        #[test]
        fn sniff_summary_fails_closed_without_tshark_when_required() -> TestResult {
            let dir = tempfile::tempdir()?;
            let pcapng = dir.path().join("capture.pcapng");
            fs::write(&pcapng, b"synthetic pcapng bytes")?;
            let request = HardwareSniffSummaryRequest {
                pcapng: &pcapng,
                vendor: Some("0x346E"),
                product: Some("0x0014"),
                interface: Some(2),
                include_payload_samples: false,
                max_samples_per_report: None,
            };

            let error = match build_hardware_sniff_summary_with_tshark_path(&request, None) {
                Ok(_) => return Err("missing tshark should fail closed".into()),
                Err(error) => error.to_string(),
            };

            assert!(
                error.contains("tshark was not found"),
                "unexpected error: {error}"
            );
            assert!(
                error.contains("WIRESHARK_TSHARK"),
                "error should explain how to point at tshark: {error}"
            );
            Ok(())
        }

        #[test]
        fn sniff_summary_rejects_ambiguous_or_invalid_filters() -> TestResult {
            let pcapng = Path::new("capture.pcapng");
            let product_without_vendor = HardwareSniffSummaryRequest {
                pcapng,
                vendor: None,
                product: Some("0x0014"),
                interface: Some(2),
                include_payload_samples: false,
                max_samples_per_report: None,
            };
            let error = match validate_sniff_summary_request(&product_without_vendor) {
                Ok(_) => return Err("product without vendor should be rejected".into()),
                Err(error) => error.to_string(),
            };
            assert!(
                error.contains("product filter is ambiguous without --vendor"),
                "{error}"
            );

            let invalid_vendor = HardwareSniffSummaryRequest {
                pcapng,
                vendor: Some("346E"),
                product: None,
                interface: Some(2),
                include_payload_samples: false,
                max_samples_per_report: None,
            };
            let error = match validate_sniff_summary_request(&invalid_vendor) {
                Ok(_) => return Err("invalid vendor filter should be rejected".into()),
                Err(error) => error.to_string(),
            };
            assert!(
                error.contains("vendor filter must use 0x0000 format"),
                "{error}"
            );

            let invalid_sample_limit = HardwareSniffSummaryRequest {
                pcapng,
                vendor: Some("0x346E"),
                product: Some("0x0014"),
                interface: Some(2),
                include_payload_samples: false,
                max_samples_per_report: Some(0),
            };
            let error = match validate_sniff_summary_request(&invalid_sample_limit) {
                Ok(_) => return Err("invalid sample limit should be rejected".into()),
                Err(error) => error.to_string(),
            };
            assert!(
                error.contains("max-samples-per-report must be between"),
                "{error}"
            );
            Ok(())
        }

        #[test]
        fn sniff_summary_parses_device_endpoint_records() -> TestResult {
            let summary = fixture_summary(false)?;

            assert!(summary.success);
            assert_eq!(summary.matched_packets, 4);
            assert_eq!(summary.usb_transfer_summary.host_to_device, 2);
            assert_eq!(summary.usb_transfer_summary.device_to_host, 1);
            assert_eq!(summary.usb_transfer_summary.control, 1);
            assert_eq!(summary.usb_transfer_summary.interrupt, 3);
            assert_eq!(summary.observed_devices.len(), 1);
            let device = summary
                .observed_devices
                .first()
                .ok_or("missing observed device")?;
            assert_eq!(device.vendor_id, "0x346E");
            assert_eq!(device.product_id, "0x0014");
            assert_eq!(device.interfaces, vec![2]);
            assert!(device.endpoints.contains(&"0x81".to_string()));
            assert!(device.endpoints.contains(&"0x02".to_string()));
            assert_eq!(summary.descriptor_candidates.len(), 1);
            assert_eq!(
                summary
                    .descriptor_candidates
                    .first()
                    .ok_or("missing descriptor candidate")?
                    .kind,
                "hid_report_descriptor"
            );
            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_summary_matches_full_tshark_descriptor_tree() -> TestResult {
            let full_tshark_json = r#"[
              {
                "_source": {
                  "layers": {
                    "frame": { "frame.number": "2" },
                    "usb": {
                      "usb.bus_id": "2",
                      "usb.device_address": "3",
                      "usb.endpoint_address": "0x80",
                      "usb.transfer_type": "0x02"
                    },
                    "DEVICE DESCRIPTOR": {
                      "usb.bDescriptorType": "0x01",
                      "usb.idVendor": "0x346e",
                      "usb.idProduct": "0x0004"
                    }
                  }
                }
              }
            ]"#;

            let summary = build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0x346E".to_string()),
                        product_id: Some("0x0004".to_string()),
                        interface_number: None,
                    },
                    include_payload_samples: false,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"full tshark descriptor fixture"),
                true,
                Some("TShark synthetic full descriptor fixture".to_string()),
                full_tshark_json,
            )?;

            assert!(summary.success);
            assert_eq!(summary.matched_packets, 1);
            let device = summary
                .observed_devices
                .first()
                .ok_or("missing observed device")?;
            assert_eq!(device.vendor_id, "0x346E");
            assert_eq!(device.product_id, "0x0004");
            Ok(())
        }

        #[test]
        fn sniff_summary_hashes_payloads_by_default() -> TestResult {
            let summary = fixture_summary(false)?;
            let value = serde_json::to_value(&summary)?;

            let report = summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "device_to_host" && report.report_id == "0x05")
                .ok_or("missing device-to-host report 0x05")?;

            assert_eq!(report.payload_sample_count, 1);
            assert_eq!(
                report.payload_sha256_examples,
                vec![sha256_hex(&[0x05, 0x10, 0x20, 0x30])]
            );
            assert!(report.payload_hex_samples.is_none());
            assert!(value.get("observed_reports").is_some());
            assert!(
                !serde_json::to_string(&value)?.contains("payload_hex_samples"),
                "raw payload samples must not serialize by default"
            );
            Ok(())
        }

        #[test]
        fn sniff_summary_includes_payload_samples_only_when_explicit() -> TestResult {
            let default_summary = fixture_summary(false)?;
            let explicit_summary = fixture_summary(true)?;

            assert!(
                default_summary
                    .observed_reports
                    .iter()
                    .all(|report| report.payload_hex_samples.is_none())
            );
            let report = explicit_summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "host_to_device" && report.report_id == "0x20")
                .ok_or("missing host-to-device report 0x20")?;
            assert_eq!(
                report.payload_hex_samples.as_ref(),
                Some(&vec!["20 00 01 02 03 04 05 06".to_string()])
            );
            Ok(())
        }

        #[test]
        fn sniff_summary_classifies_pidff_and_vendor_candidate_reports() -> TestResult {
            let summary = fixture_summary(true)?;

            let pidff = summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "host_to_device" && report.report_id == "0x05")
                .ok_or("missing host-to-device PIDFF report 0x05")?;
            assert_eq!(
                pidff.classification.category,
                "standard_pidff_output_report"
            );
            assert_eq!(pidff.classification.label, "pidff_set_constant_force");
            assert!(!pidff.classification.vendor_specific_candidate);
            assert!(!pidff.classification.native_control_evidence);

            let vendor_candidate = summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "host_to_device" && report.report_id == "0x20")
                .ok_or("missing host-to-device report 0x20")?;
            assert_eq!(
                vendor_candidate.classification.category,
                "vendor_or_device_specific_output_candidate"
            );
            assert_eq!(
                vendor_candidate.classification.label,
                "unknown_host_to_device_report_0x20"
            );
            assert!(vendor_candidate.classification.vendor_specific_candidate);
            assert!(!vendor_candidate.classification.native_control_evidence);

            let speculative = classify_sniff_observed_report(SniffUsbDirection::HostToDevice, 0x09);
            assert_eq!(
                speculative.category,
                "vendor_or_device_specific_output_candidate"
            );
            assert!(
                speculative.vendor_specific_candidate,
                "non-canonical PIDFF report IDs must stay conservative"
            );

            let input = summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "device_to_host" && report.report_id == "0x05")
                .ok_or("missing device-to-host report 0x05")?;
            assert_eq!(input.classification.category, "input_or_status_report");
            assert!(!input.classification.vendor_specific_candidate);
            assert!(!input.classification.native_control_evidence);

            let classification_summary = &summary.report_classification_summary;
            assert_eq!(classification_summary.standard_pidff_output_report_count, 1);
            assert_eq!(
                classification_summary.vendor_or_device_specific_output_candidate_count,
                1
            );
            assert_eq!(classification_summary.input_or_status_report_count, 1);
            assert_eq!(
                classification_summary.host_to_device_report_ids,
                vec!["0x05".to_string(), "0x20".to_string()]
            );
            assert_eq!(
                classification_summary.standard_pidff_output_report_ids,
                vec!["0x05".to_string()]
            );
            assert_eq!(
                classification_summary.vendor_or_device_specific_output_candidate_report_ids,
                vec!["0x20".to_string()]
            );
            assert!(classification_summary.decode_recommended);
            assert!(!classification_summary.native_control_evidence);
            assert!(!classification_summary.readiness_claim);

            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_summary_surfaces_host_to_device_decode_gaps() -> TestResult {
            let undecoded_host_output_fixture = r#"[
              {
                "_source": {
                  "layers": {
                    "frame": { "frame.number": "42" },
                    "usb": {
                      "usb.bus_id": "1",
                      "usb.device_address": "12",
                      "usb.idVendor": "0x346e",
                      "usb.idProduct": "0x0014",
                      "usb.interface_number": "2",
                      "usb.endpoint_address": "0x02",
                      "usb.endpoint_direction": "OUT",
                      "usb.transfer_type": "Interrupt",
                      "usb.data_len": "20"
                    }
                  }
                }
              }
            ]"#;
            let summary = build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0x346E".to_string()),
                        product_id: Some("0x0014".to_string()),
                        interface_number: Some(2),
                    },
                    include_payload_samples: false,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"undecoded host output fixture"),
                true,
                Some("TShark synthetic undecoded host fixture".to_string()),
                undecoded_host_output_fixture,
            )?;

            assert!(summary.success);
            assert_eq!(summary.usb_transfer_summary.host_to_device, 1);
            assert!(summary.observed_reports.is_empty());
            let classification_summary = &summary.report_classification_summary;
            assert_eq!(classification_summary.host_to_device_packet_count, 1);
            assert_eq!(
                classification_summary.host_to_device_classified_packet_count,
                0
            );
            assert_eq!(
                classification_summary.host_to_device_unclassified_packet_count,
                1
            );
            assert!(classification_summary.host_to_device_decode_gap);
            assert_eq!(
                classification_summary.host_to_device_data_len_packet_count,
                1
            );
            assert_eq!(classification_summary.host_to_device_data_len_bytes, 20);
            assert_eq!(
                classification_summary.host_to_device_payload_extracted_packet_count,
                0
            );
            assert_eq!(
                classification_summary.host_to_device_payload_extracted_bytes,
                0
            );
            assert_eq!(
                classification_summary.host_to_device_payload_missing_packet_count,
                1
            );
            assert!(classification_summary.host_to_device_payload_export_gap);
            let missing_example = classification_summary
                .host_to_device_payload_missing_packet_examples
                .first()
                .ok_or("expected missing host-to-device payload example")?;
            assert_eq!(missing_example.packet_ordinal, 1);
            assert_eq!(missing_example.frame_number, Some(42));
            assert_eq!(missing_example.device_key.as_deref(), Some("1:12"));
            assert_eq!(missing_example.vendor_id.as_deref(), Some("0x346E"));
            assert_eq!(missing_example.product_id.as_deref(), Some("0x0014"));
            assert_eq!(missing_example.interface_number, Some(2));
            assert_eq!(missing_example.endpoint_address.as_deref(), Some("0x02"));
            assert_eq!(missing_example.transfer_type.as_deref(), Some("interrupt"));
            assert_eq!(missing_example.data_len, 20);
            assert!(!missing_example.payload_extracted);
            assert!(!missing_example.native_control_evidence);
            assert!(!missing_example.hardware_output_authorized);
            assert!(classification_summary.decode_recommended);
            assert!(!classification_summary.native_control_evidence);
            assert!(!classification_summary.readiness_claim);
            assert!(
                classification_summary
                    .notes
                    .iter()
                    .any(|note| note.contains("declare data length but no payload bytes")),
                "decode-gap note should explain why a raw protocol review is still needed"
            );
            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_summary_does_not_treat_usb_setup_stage_as_payload_gap() -> TestResult {
            let usb_setup_stage_fixture = r#"[
              {
                "_source": {
                  "layers": {
                    "frame": { "frame.number": "5" },
                    "usb": {
                      "usb.bus_id": "2",
                      "usb.device_address": "3",
                      "usb.idVendor": "0x346e",
                      "usb.idProduct": "0x0004",
                      "usb.interface_number": "0",
                      "usb.endpoint_address": "0x00",
                      "usb.endpoint_direction": "OUT",
                      "usb.transfer_type": "URB_CONTROL out",
                      "usb.data_len": "8",
                      "usb.setup.bmRequestType": "0x21",
                      "usb.setup.bRequest": "0x09",
                      "usb.setup.wValue": "0x0200",
                      "usb.setup.wIndex": "0x0000",
                      "usb.setup.wLength": "0"
                    }
                  }
                }
              }
            ]"#;
            let summary = build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0x346E".to_string()),
                        product_id: Some("0x0004".to_string()),
                        interface_number: Some(0),
                    },
                    include_payload_samples: false,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"usb setup stage fixture"),
                true,
                Some("TShark synthetic setup fixture".to_string()),
                usb_setup_stage_fixture,
            )?;

            assert!(summary.success);
            assert_eq!(summary.usb_transfer_summary.host_to_device, 1);
            assert_eq!(summary.usb_transfer_summary.control, 1);
            assert!(summary.observed_reports.is_empty());
            let classification_summary = &summary.report_classification_summary;
            assert_eq!(classification_summary.host_to_device_packet_count, 1);
            assert_eq!(
                classification_summary.host_to_device_data_len_packet_count,
                1
            );
            assert_eq!(classification_summary.host_to_device_data_len_bytes, 8);
            assert_eq!(
                classification_summary.host_to_device_payload_extracted_packet_count,
                0
            );
            assert_eq!(
                classification_summary.host_to_device_payload_missing_packet_count,
                0
            );
            assert!(
                classification_summary
                    .host_to_device_payload_missing_packet_examples
                    .is_empty()
            );
            assert!(!classification_summary.host_to_device_payload_export_gap);
            assert!(classification_summary.host_to_device_decode_gap);
            assert!(classification_summary.decode_recommended);
            assert!(!classification_summary.native_control_evidence);
            assert!(!classification_summary.readiness_claim);
            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_summary_extracts_usbcom_host_to_device_payloads() -> TestResult {
            let usbcom_host_output_fixture = r#"[
              {
                "_source": {
                  "layers": {
                    "usb": {
                      "usb.bus_id": "1",
                      "usb.device_address": "12",
                      "usb.idVendor": "0x346e",
                      "usb.idProduct": "0x0014",
                      "usb.interface_number": "2",
                      "usb.endpoint_address": "0x02",
                      "usb.endpoint_direction": "OUT",
                      "usb.transfer_type": "Interrupt",
                      "usb.data_len": "14"
                    },
                    "usbcom": {
                      "usbcom.data.out_payload": "7e:01:5a:1b:00:01:7e:03:5d:1b:01:00:00:07"
                    }
                  }
                }
              }
            ]"#;
            let summary = build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0x346E".to_string()),
                        product_id: Some("0x0014".to_string()),
                        interface_number: Some(2),
                    },
                    include_payload_samples: true,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"usbcom host output fixture"),
                true,
                Some("TShark synthetic usbcom fixture".to_string()),
                usbcom_host_output_fixture,
            )?;

            assert!(summary.success);
            assert_eq!(summary.usb_transfer_summary.host_to_device, 1);
            let report = summary
                .observed_reports
                .iter()
                .find(|report| report.direction == "host_to_device" && report.report_id == "0x7E")
                .ok_or("missing extracted usbcom host-to-device report")?;
            assert_eq!(report.count, 1);
            assert_eq!(
                report.payload_hex_samples.as_ref(),
                Some(&vec![
                    "7E 01 5A 1B 00 01 7E 03 5D 1B 01 00 00 07".to_string()
                ])
            );
            assert_eq!(
                report.classification.category,
                "vendor_or_device_specific_output_candidate"
            );
            assert!(!report.classification.native_control_evidence);

            let classification_summary = &summary.report_classification_summary;
            assert_eq!(classification_summary.host_to_device_packet_count, 1);
            assert_eq!(
                classification_summary.host_to_device_payload_extracted_packet_count,
                1
            );
            assert_eq!(
                classification_summary.host_to_device_payload_extracted_bytes,
                14
            );
            assert_eq!(
                classification_summary.host_to_device_payload_missing_packet_count,
                0
            );
            assert!(
                classification_summary
                    .host_to_device_payload_missing_packet_examples
                    .is_empty()
            );
            assert!(!classification_summary.host_to_device_payload_export_gap);
            assert!(classification_summary.decode_recommended);
            let frame_summary = &classification_summary.usbcom_serial_frame_summary;
            assert_eq!(frame_summary.packet_count, 1);
            assert_eq!(frame_summary.payload_bytes, 14);
            assert_eq!(frame_summary.parsed_frame_count, 2);
            assert_eq!(frame_summary.checksum_valid_frame_count, 2);
            assert_eq!(frame_summary.checksum_invalid_frame_count, 0);
            assert_eq!(frame_summary.truncated_frame_count, 0);
            assert!(!frame_summary.frame_shape_decode_gap);
            assert_eq!(frame_summary.tuple_sample_limit, 3);
            assert_eq!(frame_summary.tuple_counts.len(), 2);
            let tuple_5a = frame_summary
                .tuple_counts
                .iter()
                .find(|tuple| {
                    tuple.group == "0x5A"
                        && tuple.device_id == "0x1B"
                        && tuple.command.as_deref() == Some("0x00")
                })
                .ok_or("expected decoded 0x5A/0x1B/0x00 tuple")?;
            assert_eq!(tuple_5a.count, 1);
            let sample_5a = tuple_5a
                .sample_frames
                .first()
                .ok_or("expected 0x5A sample frame")?;
            assert_eq!(sample_5a.frame_hex, "7E015A1B0001");
            assert_eq!(sample_5a.payload_hex, "");
            assert_eq!(sample_5a.payload_len, 0);
            assert!(!sample_5a.hardware_output_authorized);
            assert!(!sample_5a.output_sendability_claim);
            let tuple_5d = frame_summary
                .tuple_counts
                .iter()
                .find(|tuple| {
                    tuple.group == "0x5D"
                        && tuple.device_id == "0x1B"
                        && tuple.command.as_deref() == Some("0x01")
                })
                .ok_or("expected decoded 0x5D/0x1B/0x01 tuple")?;
            let sample_5d = tuple_5d
                .sample_frames
                .first()
                .ok_or("expected 0x5D sample frame")?;
            assert_eq!(sample_5d.frame_hex, "7E035D1B01000007");
            assert_eq!(sample_5d.payload_hex, "0000");
            assert_eq!(sample_5d.payload_len, 2);
            assert!(!sample_5d.hardware_output_authorized);
            assert!(!sample_5d.output_sendability_claim);
            assert!(
                frame_summary
                    .tuple_counts
                    .iter()
                    .all(|tuple| tuple.checksum_invalid_count == 0),
                "decoded fixture tuples should have valid checksums"
            );
            assert!(!frame_summary.native_control_evidence);
            assert!(!frame_summary.readiness_claim);
            assert!(!classification_summary.native_control_evidence);
            assert!(!classification_summary.readiness_claim);
            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }

        #[test]
        fn sniff_summary_zero_matches_returns_unsuccessful_receipt() -> TestResult {
            let summary = build_hardware_sniff_summary_from_tshark_json(
                HardwareSniffSummaryConfig {
                    filters: HardwareSniffSummaryFilters {
                        vendor_id: Some("0xFFFF".to_string()),
                        product_id: Some("0xEEEE".to_string()),
                        interface_number: Some(2),
                    },
                    include_payload_samples: false,
                    max_samples_per_report: 2,
                },
                sha256_hex(b"synthetic pcapng fixture"),
                true,
                Some("TShark synthetic fixture".to_string()),
                TSHARK_JSON_FIXTURE,
            )?;

            assert!(!summary.success);
            assert_eq!(summary.matched_packets, 0);
            assert_eq!(
                summary
                    .report_classification_summary
                    .vendor_or_device_specific_output_candidate_count,
                0
            );
            assert!(!summary.report_classification_summary.decode_recommended);
            assert!(
                summary
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("no USB packets matched"))
            );
            let value = serde_json::to_value(&summary)?;
            assert!(value.get("reason").is_some());
            assert_schema_valid("sniff-summary.schema.json", &value)?;
            Ok(())
        }

        #[test]
        fn sniff_summary_never_sets_readiness_claims() -> TestResult {
            let summary = fixture_summary(true)?;

            assert!(!summary.native_control_evidence);
            assert!(!summary.openracing_hardware_output);
            assert!(summary.external_app_may_have_sent_output);
            assert!(!summary.satisfies_native_response_ready);
            assert!(!summary.satisfies_native_visible_ready);
            assert!(!summary.satisfies_smoke_ready);
            assert!(!summary.satisfies_release_ready);
            assert!(summary.readiness_claims.all_false());
            assert_schema_valid(
                "sniff-summary.schema.json",
                &serde_json::to_value(&summary)?,
            )?;
            Ok(())
        }
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
    fn tshark_interface_parser_detects_usbpcap_interfaces() {
        let output =
            "1. USBPcap1 (USBPcap1)\n2. \\Device\\NPF_Loopback (Loopback)\n3. USBPcap2 (USBPcap2)";

        let interfaces = usbpcap_interfaces_from_tshark_list(output);

        assert_eq!(
            interfaces,
            vec![
                "1. USBPcap1 (USBPcap1)".to_string(),
                "3. USBPcap2 (USBPcap2)".to_string()
            ]
        );
    }

    #[test]
    fn usbpcap_interface_value_parser_handles_tshark_display_forms() {
        assert_eq!(
            usbpcap_interface_value_from_tshark_line("10. \\\\.\\USBPcap2 (USBPcap2)").as_deref(),
            Some(r"\\.\USBPcap2")
        );
        assert_eq!(
            usbpcap_interface_value_from_tshark_line("3. USBPcap1 (USBPcap1)").as_deref(),
            Some(r"\\.\USBPcap1")
        );
    }

    #[test]
    fn usbpcap_active_process_parser_accepts_array_and_single_object() -> TestResult {
        let array = r#"[
            {
                "ProcessId": 508388,
                "CreationDate": "20260521144237.123456-240",
                "CommandLine": "\"C:\\Program Files\\Wireshark\\extcap\\USBPcapCMD.exe\" -d \\\\.\\USBPcap2 --devices 3 -o target\\sniff\\old-probe\\capture.pcap"
            },
            {
                "ProcessId": 511740,
                "CreationDate": "20260521144256.123456-240",
                "CommandLine": "\"C:\\Program Files\\Wireshark\\extcap\\USBPcapCMD.exe\" -d \\\\.\\USBPcap2 -A -o target\\sniff\\old-probe\\all.pcap"
            }
        ]"#;
        let processes = usbpcap_active_processes_from_json(array)?;
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].process_id, 508388);
        assert!(
            processes[0]
                .command_line
                .as_deref()
                .is_some_and(|line| line.contains("old-probe")),
            "expected first process command line to preserve the capture path"
        );

        let single = r#"{
            "ProcessId": 514764,
            "CreationDate": "20260521144323.123456-240",
            "CommandLine": "\"C:\\Program Files\\Wireshark\\extcap\\USBPcapCMD.exe\" -d \\\\.\\USBPcap2 -A -o target\\sniff\\old-probe\\ps-all.pcap"
        }"#;
        let processes = usbpcap_active_processes_from_json(single)?;
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].process_id, 514764);
        Ok(())
    }

    #[test]
    fn usbpcap_extcap_config_parser_surfaces_moza_device_filter_hint() {
        let output = r#"arg {number=99}{call=--devices}{display=Attached USB Devices}{type=multicheck}
value {arg=99}{value=3}{display=[3] USB Composite Device}{enabled=true}
value {arg=99}{value=3_1}{display=USB Serial Device (COM4)}{enabled=false}{parent=3}
value {arg=99}{value=3_2}{display=USB Input Device}{enabled=false}{parent=3}
value {arg=99}{value=3_3}{display=MOZA Windows Driver}{enabled=false}{parent=3_2}
value {arg=99}{value=2}{display=[2] Generic USB Hub}{enabled=true}"#;

        let hints = usbpcap_moza_device_hints_from_extcap_config(r"\\.\USBPcap2", output);

        assert_eq!(hints.len(), 1);
        let hint = &hints[0];
        assert_eq!(hint.usbpcap_interface, r"\\.\USBPcap2");
        assert_eq!(hint.capture_devices_value, "3");
        assert!(
            hint.matched_device_values
                .iter()
                .any(|value| value == "3_1")
        );
        assert!(
            hint.matched_device_values
                .iter()
                .any(|value| value == "3_3")
        );
        assert!(
            hint.matched_device_displays
                .iter()
                .any(|display| display.contains("MOZA Windows Driver"))
        );
        assert!(hint.suggested_capture_filter.contains("--devices 3"));
    }

    #[test]
    fn doctor_warns_when_usbpcap_descriptor_capture_is_unavailable() {
        let receipt = sample_receipt();

        assert!(
            receipt
                .warnings
                .iter()
                .any(|warning| warning.contains("USBPcap/Wireshark capture interfaces"))
        );
        assert!(
            !receipt
                .tools
                .usbpcap_descriptor_capture
                .ready_for_usbpcap_descriptor_capture
        );
    }

    #[test]
    fn doctor_warns_when_active_usbpcap_processes_are_detected() {
        let mut receipt = sample_receipt();
        receipt
            .tools
            .usbpcap_descriptor_capture
            .active_usbpcap_processes = UsbPcapActiveProcessChecks {
            process_scan_attempted: true,
            active_process_count: 1,
            processes: vec![UsbPcapActiveProcess {
                process_id: 508388,
                creation_date: Some("20260521144237.123456-240".to_string()),
                command_line: Some(
                    r#""C:\Program Files\Wireshark\extcap\USBPcapCMD.exe" -d \\.\USBPcap2 --devices 3 -o target\sniff\old-probe\capture.pcap"#
                        .to_string(),
                ),
            }],
            error: None,
        };

        let warnings = doctor_warnings(&receipt.tools, &receipt.hid);

        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("active USBPcapCMD process")),
            "expected active USBPcap warning in {warnings:?}"
        );
    }

    #[test]
    fn usbpcap_guidance_reports_installed_but_inaccessible_capture_driver() {
        let guidance =
            usbpcap_descriptor_capture_guidance(true, false, true, true, Some("stopped"));

        assert!(guidance.contains("USBPcap is installed"));
        assert!(guidance.contains("driver service is stopped"));
        assert!(guidance.contains("sc start USBPcap"));
        assert!(guidance.contains("rerun hardware doctor"));
    }

    #[test]
    fn usbpcap_guidance_reports_running_service_without_interfaces() {
        let guidance =
            usbpcap_descriptor_capture_guidance(true, false, true, true, Some("running"));

        assert!(guidance.contains("driver service is running"));
        assert!(guidance.contains("reboot after driver installation"));
        assert!(guidance.contains("rerun hardware doctor"));
    }

    #[test]
    fn usbpcap_service_state_parser_reads_sc_query_state() {
        let output = "\r\nSERVICE_NAME: USBPcap\r\n        TYPE               : 1  KERNEL_DRIVER\r\n        STATE              : 4  RUNNING\r\n";

        assert_eq!(
            usbpcap_service_state_from_sc_query(output),
            Some("running".to_string())
        );
    }

    #[test]
    fn lane_status_uses_hardware_doctor_usbpcap_access_guidance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        fs::write(lane.join("device-list.json"), "{}\n")?;
        fs::write(lane.join("hid-list.json"), "{}\n")?;
        fs::write(lane.join("moza-probe.json"), "{}\n")?;
        fs::write(
            lane.join("hardware-doctor.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "tools": {
                    "usbpcap_descriptor_capture": {
                        "tshark_present": true,
                        "usbpcap_extcap_present": true,
                        "usbpcap_driver_installed": true,
                        "usbpcap_interfaces_present": false,
                        "usbpcap_interface_count": 0,
                        "ready_for_usbpcap_descriptor_capture": false,
                        "access_guidance": "USBPcap is installed, but Wireshark/tshark exposes no USBPcap interfaces; run the descriptor capture from an elevated shell or elevated Wireshark, reboot after driver installation if needed, then rerun hardware doctor"
                    }
                }
            }))?,
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert!(
            status
                .descriptor_capture_tooling
                .guidance
                .contains("elevated")
        );
        Ok(())
    }

    #[test]
    fn bringup_rail_is_ordered_and_read_only() -> TestResult {
        let receipt = build_bringup_rail_receipt("generic-wheelbase")?;

        assert!(receipt.success);
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_ffb_writes);
        assert!(receipt.no_output_reports);
        assert!(receipt.no_feature_reports);
        assert!(receipt.no_serial_config_commands);
        assert!(receipt.no_firmware_or_dfu_commands);
        assert_eq!(
            receipt.stages.first().map(|stage| stage.id),
            Some("discovery")
        );
        assert_eq!(
            receipt.stages.last().map(|stage| stage.id),
            Some("ffb_extended")
        );
        assert!(
            receipt
                .stages
                .windows(2)
                .all(|pair| pair[0].order < pair[1].order)
        );
        let pre_output = receipt
            .stages
            .iter()
            .find(|stage| stage.id == "pre_output_readiness")
            .ok_or_else(|| io::Error::other("missing pre-output stage"))?;
        assert!(
            pre_output
                .required_gates
                .contains(&"ready_for_zero_torque_true")
        );
        assert!(pre_output.required_gates.contains(&"ready_for_ffb_false"));
        assert!(pre_output.forbidden_actions.contains(&"output_reports"));
        Ok(())
    }

    #[test]
    fn bringup_rail_keeps_output_after_passive_and_descriptor() -> TestResult {
        let receipt = build_bringup_rail_receipt("moza-r5")?;
        let passive = receipt
            .stages
            .iter()
            .find(|stage| stage.id == "passive")
            .ok_or_else(|| io::Error::other("missing passive stage"))?;
        let descriptor = receipt
            .stages
            .iter()
            .find(|stage| stage.id == "descriptor_trust")
            .ok_or_else(|| io::Error::other("missing descriptor stage"))?;
        let zero = receipt
            .stages
            .iter()
            .find(|stage| stage.id == "zero_torque")
            .ok_or_else(|| io::Error::other("missing zero stage"))?;

        assert!(passive.forbidden_actions.contains(&"output_reports"));
        assert!(
            descriptor
                .required_gates
                .contains(&"report_descriptor_crc32_present")
        );
        assert!(zero.order > descriptor.order);
        assert!(zero.required_gates.contains(&"zero_output_only"));
        assert!(!zero.required_gates.contains(&"low_force_cap"));
        Ok(())
    }

    #[test]
    fn bringup_rail_uses_family_adapter_contracts() -> TestResult {
        let generic = build_bringup_rail_receipt("generic-wheelbase")?;
        let moza = build_bringup_rail_receipt("moza-r5")?;

        assert_eq!(generic.stages.len(), moza.stages.len());
        assert_eq!(generic.stages[0].id, moza.stages[0].id);
        assert!(generic.adapter.known_vid_pids.is_empty());
        assert!(moza.adapter.known_vid_pids.contains(&"0x346E:0x0004"));
        assert!(moza.adapter.known_vid_pids.contains(&"0x346E:0x0014"));
        assert!(
            generic
                .adapter
                .default_logical_controls
                .contains(&"rim_controls")
        );
        assert!(
            moza.adapter
                .default_logical_controls
                .contains(&"clutch_optional")
        );
        assert!(
            !generic
                .adapter
                .default_logical_controls
                .contains(&"clutch_optional")
        );
        assert!(
            moza.adapter.ffb_eligibility.contains("native-visible")
                && moza.adapter.ffb_eligibility.contains("simulator telemetry"),
            "Moza bounded FFB eligibility must name native-visible and simulator telemetry gates: {}",
            moza.adapter.ffb_eligibility
        );
        assert!(
            moza.adapter
                .ffb_eligibility
                .contains("Pit House is external compatibility")
                && !moza
                    .adapter
                    .ffb_eligibility
                    .contains("requires zero-torque, watchdog, disconnect, low-torque, Pit House"),
            "Moza native FFB eligibility must not make Pit House a native prerequisite: {}",
            moza.adapter.ffb_eligibility
        );
        Ok(())
    }

    #[test]
    fn bringup_rail_rejects_unknown_adapter() {
        let err = build_bringup_rail_receipt("unknown-family").expect_err("expected error");
        assert!(err.to_string().contains("unknown hardware bring-up family"));
    }

    #[test]
    fn lane_scaffold_creates_read_only_planning_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");

        let receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;

        assert!(receipt.success);
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_ffb_writes);
        assert!(receipt.no_output_reports);
        assert!(receipt.no_feature_reports);
        assert!(receipt.no_serial_config_commands);
        assert!(receipt.no_firmware_or_dfu_commands);
        assert_eq!(receipt.family, "moza-r5");
        assert!(lane.join("captures").is_dir());
        assert!(lane.join("hardware-lane-manifest.json").is_file());
        assert!(lane.join("artifact-checklist.md").is_file());
        assert!(lane.join("capture-plan.md").is_file());
        assert!(lane.join("stage-gates.json").is_file());
        assert!(lane.join("lane-init.json").is_file());
        assert!(!lane.join("passive-verification.json").exists());
        assert!(!lane.join("zero-torque-proof.json").exists());

        let manifest_text = fs::read_to_string(lane.join("hardware-lane-manifest.json"))?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_text)?;
        assert_eq!(manifest["completion_state"], "not_started");
        assert_eq!(manifest["family"], "moza-r5");
        assert_eq!(manifest["topology"], "wheelbase-hub");
        let roles = manifest["declared_logical_roles"]
            .as_array()
            .ok_or_else(|| io::Error::other("logical roles should be an array"))?;
        assert!(roles.iter().any(|role| role["id"] == "throttle"));
        assert!(roles.iter().any(|role| {
            role["id"] == "clutch"
                && role["required"] == false
                && role["connection_path"] == "wheelbase_hub"
        }));

        let gates_text = fs::read_to_string(lane.join("stage-gates.json"))?;
        let gates: serde_json::Value = serde_json::from_str(&gates_text)?;
        let stages = gates["stages"]
            .as_array()
            .ok_or_else(|| io::Error::other("stages should be an array"))?;
        assert!(stages.iter().any(|stage| {
            stage["id"] == "pre_output_readiness"
                && stage["required_gates"]
                    .as_array()
                    .is_some_and(|gates| gates.iter().any(|gate| gate == "ready_for_ffb_false"))
        }));
        Ok(())
    }

    #[test]
    fn lane_scaffold_refuses_to_overwrite_existing_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("generic-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "generic-wheelbase", "unknown", "Steven", false, None)?;

        let err =
            scaffold_hardware_lane(&lane, "generic-wheelbase", "unknown", "Steven", false, None)
                .err()
                .ok_or_else(|| io::Error::other("expected overwrite refusal"))?;
        assert!(err.to_string().contains("--overwrite"));

        let receipt =
            scaffold_hardware_lane(&lane, "generic-wheelbase", "unknown", "Steven", true, None)?;
        assert_eq!(receipt.family, "generic-wheelbase");
        Ok(())
    }

    #[test]
    fn lane_scaffold_role_overrides_declare_bench_profile_without_fixed_defaults() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let overrides = HardwareLaneRoleOverrides::from_cli(
            &[
                "handbrake".to_string(),
                "ks_controls".to_string(),
                "es_controls".to_string(),
            ],
            &[],
            &[
                "ks_controls=captures/ks-controls.jsonl".to_string(),
                "es_controls=captures/es-controls.jsonl".to_string(),
            ],
            &[
                "ks_controls=hid-0x346E-0x0004-if2-0x0001-0x0004".to_string(),
                "es_controls=hid-0x346E-0x0004-if2-0x0001-0x0004".to_string(),
            ],
            &[
                "ks_controls=wheelbase_hub".to_string(),
                "es_controls=wheelbase_hub".to_string(),
            ],
        )?;

        let _receipt = scaffold_hardware_lane_with_overrides(
            &lane,
            "moza-r5",
            "wheelbase-hub",
            "Steven",
            &overrides,
            false,
            None,
        )?;
        let manifest_text = fs::read_to_string(lane.join("hardware-lane-manifest.json"))?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_text)?;
        let roles = manifest["declared_logical_roles"]
            .as_array()
            .ok_or_else(|| io::Error::other("logical roles should be an array"))?;

        assert!(roles.iter().any(|role| {
            role["id"] == "handbrake"
                && role["required"] == true
                && role["evidence_artifact"] == "captures/r5-handbrake-only-sweep.jsonl"
        }));
        assert!(roles.iter().any(|role| {
            role["id"] == "ks_controls"
                && role["required"] == true
                && role["connection_path"] == "wheelbase_hub"
                && role["expected_endpoint"] == "hid-0x346E-0x0004-if2-0x0001-0x0004"
                && role["evidence_artifact"] == "captures/ks-controls.jsonl"
        }));
        assert!(roles.iter().any(|role| {
            role["id"] == "es_controls"
                && role["required"] == true
                && role["connection_path"] == "wheelbase_hub"
                && role["expected_endpoint"] == "hid-0x346E-0x0004-if2-0x0001-0x0004"
                && role["evidence_artifact"] == "captures/es-controls.jsonl"
        }));

        let status = build_hardware_lane_status_receipt(&lane)?;
        let passive = status
            .stages
            .iter()
            .find(|stage| stage.id == "passive")
            .ok_or_else(|| io::Error::other("missing passive stage"))?;
        assert!(passive.expected_artifacts.iter().any(|artifact| {
            artifact.kind == "capture" && artifact.relative_path == "captures/ks-controls.jsonl"
        }));
        assert!(passive.expected_artifacts.iter().any(|artifact| {
            artifact.kind == "capture" && artifact.relative_path == "captures/es-controls.jsonl"
        }));
        Ok(())
    }

    #[test]
    fn lane_scaffold_role_overrides_reject_ambiguous_or_unsafe_specs() -> TestResult {
        let ambiguous = HardwareLaneRoleOverrides::from_cli(
            &["handbrake".to_string()],
            &["handbrake".to_string()],
            &[],
            &[],
            &[],
        )
        .err()
        .ok_or_else(|| io::Error::other("expected ambiguous role failure"))?;
        assert!(ambiguous.to_string().contains("both required and optional"));

        let unsafe_artifact = HardwareLaneRoleOverrides::from_cli(
            &["ks_controls".to_string()],
            &[],
            &["ks_controls=../ks-controls.jsonl".to_string()],
            &[],
            &[],
        )
        .err()
        .ok_or_else(|| io::Error::other("expected unsafe artifact failure"))?;
        assert!(
            unsafe_artifact
                .to_string()
                .contains("within the lane directory")
        );

        let invalid_connection = HardwareLaneRoleOverrides::from_cli(
            &["ks_controls".to_string()],
            &[],
            &[],
            &[],
            &["ks_controls=wheelbase-hub".to_string()],
        )
        .err()
        .ok_or_else(|| io::Error::other("expected invalid connection failure"))?;
        assert!(
            invalid_connection
                .to_string()
                .contains("must be one of wheelbase_hub")
        );

        let unknown_role = HardwareLaneRoleOverrides::from_cli(
            &[],
            &[],
            &["ks_controls=captures/ks-controls.jsonl".to_string()],
            &[],
            &[],
        )?;
        let err = lane_roles(&moza_r5_adapter_contract(), "wheelbase-hub", &unknown_role)
            .err()
            .ok_or_else(|| io::Error::other("expected unknown role failure"))?;
        assert!(
            err.to_string()
                .contains("--required-role or --optional-role")
        );
        Ok(())
    }

    #[test]
    fn lane_status_inventories_scaffold_without_validating_claims() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert!(status.success);
        assert!(status.no_hid_device_opened);
        assert!(status.no_ffb_writes);
        assert!(status.no_output_reports);
        assert!(status.no_feature_reports);
        assert!(status.no_serial_config_commands);
        assert!(status.no_firmware_or_dfu_commands);
        assert!(status.scaffold_required);
        assert!(status.scaffold_complete);
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        assert_eq!(status.next_blocked_stage, "discovery");
        assert!(
            status
                .blocking_items
                .contains(&"discovery:missing_artifacts".to_string())
        );
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("wheelctl hardware doctor"))
        );
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("wheelctl moza probe"))
        );
        assert!(
            status
                .safe_next_commands
                .iter()
                .all(|command| !command.contains("torque")
                    && !command.contains("ffb")
                    && !command.contains("output"))
        );
        assert!(status.role_evidence.iter().any(|role| {
            role.id == "throttle"
                && role.required
                && !role.artifact_present
                && role.validation_status == "not_validated_by_status"
        }));
        let pre_output = status
            .stages
            .iter()
            .find(|stage| stage.id == "pre_output_readiness")
            .ok_or_else(|| io::Error::other("missing pre-output status"))?;
        assert_eq!(pre_output.gate_status, "not_validated_by_status");
        assert!(
            pre_output
                .expected_artifacts
                .iter()
                .any(|artifact| artifact.relative_path == "pre-output-readiness.json")
        );
        Ok(())
    }

    #[test]
    fn lane_status_reads_legacy_moza_manifest_without_scaffold_manifest() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("legacy-moza-r5-lane");
        fs::create_dir_all(lane.join("captures"))?;
        write_legacy_moza_manifest(&lane, "Moza R5")?;
        for artifact in [
            "device-list.json",
            "hid-list.json",
            "hardware-doctor.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for capture in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
            "ks-controls.jsonl",
        ] {
            fs::write(lane.join("captures").join(capture), "{}\n")?;
        }

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert!(status.success);
        assert_eq!(status.manifest_source, "manifest.json");
        assert_eq!(status.family, "moza-r5");
        assert_eq!(status.topology, "wheelbase_hub");
        assert_eq!(status.completion_state, "passive_in_progress");
        assert!(!status.scaffold_required);
        assert!(!status.scaffold_complete);
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        assert_eq!(status.next_blocked_stage, "fixture_promotion");
        assert!(
            status
                .blocking_items
                .iter()
                .all(|item| item != "scaffold_files_missing")
        );
        assert!(status.role_evidence.iter().any(|role| {
            role.id == "steering"
                && role.required
                && role.expected_endpoint == "hid-0x346E-0x0004-if2-0x0001-0x0004"
                && role.artifact_present
                && role.semantic_status == "proven"
        }));
        assert!(status.role_evidence.iter().any(|role| {
            role.id == "clutch"
                && !role.required
                && role.expected_endpoint == "hid-0x346E-0x0004-if2-0x0001-0x0004"
                && !role.artifact_present
                && role.semantic_status == "generic_aux"
        }));
        assert!(joined.contains("wheelctl moza validate-captures"));
        assert!(joined.contains("wheelctl moza verify-bundle"));
        assert!(
            status
                .safe_next_commands
                .iter()
                .all(|command| !command.contains("torque")
                    && !command.contains("ffb")
                    && !command.contains("output"))
        );
        Ok(())
    }

    #[test]
    fn lane_status_points_native_response_lane_at_native_visible_frontier() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("legacy-moza-r5-lane");
        fs::create_dir_all(lane.join("captures"))?;
        write_legacy_moza_manifest_with_completion(&lane, "Moza R5", "native_response_ready")?;
        for artifact in [
            "device-list.json",
            "hid-list.json",
            "hardware-doctor.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
            "fixture-promotion.json",
            "lane-audit-passive.json",
            "pre-output-readiness.json",
            "zero-torque-proof.json",
            "watchdog-proof.json",
            "disconnect-proof.json",
            "init-off.json",
            "init-standard.json",
            "moza-status.json",
            "device-status.json",
            "support-bundle.json",
            "low-torque-proof.json",
            "steering-angle-stream-proof.json",
            "native-actuator-profile-smoke.json",
            "openracing-control-verification.json",
            "manifest-promotion-openracing-control.json",
            "lane-audit-openracing-control.json",
            "native-response-verification.json",
            "manifest-promotion-native-response.json",
            "lane-audit-native-response.json",
        ] {
            fs::write(lane.join(artifact), "{\"success\":true}\n")?;
        }
        // The first native-visible smoke receipt can fail the visible threshold
        // while still proving the native response stage.
        fs::write(
            lane.join("native-actuator-visible-smoke.json"),
            "{\"success\":false}\n",
        )?;
        write_passive_verification_receipt(
            &lane,
            &[
                ("lane_directory", "pass"),
                ("passive_captures_parse", "pass"),
                ("descriptor_metadata", "pass"),
                ("fixture_promotion", "pass"),
            ],
        )?;
        fs::write(
            lane.join("native-visible-verification.json"),
            "{\"success\":false}\n",
        )?;
        for capture in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
            "ks-controls.jsonl",
        ] {
            fs::write(lane.join("captures").join(capture), "{}\n")?;
        }

        let status = build_hardware_lane_status_receipt(&lane)?;
        let native_response = status
            .stages
            .iter()
            .find(|stage| stage.id == "native_response_ready")
            .ok_or_else(|| io::Error::other("missing native response stage"))?;
        let native_visible = status
            .stages
            .iter()
            .find(|stage| stage.id == "native_visible_ready")
            .ok_or_else(|| io::Error::other("missing native visible stage"))?;

        assert_eq!(status.completion_state, "native_response_ready");
        assert_eq!(status.next_blocked_stage, "native_visible_ready");
        assert_eq!(native_response.artifacts_missing, 0);
        assert_eq!(native_response.artifacts_failed, 0);
        assert!(native_visible.artifacts_missing > 0);
        assert_eq!(native_visible.artifacts_failed, 1);
        assert!(
            status
                .blocking_items
                .contains(&"native_visible_ready:failed_artifacts".to_string())
        );
        assert!(
            status
                .blocking_items
                .contains(&"native_visible_ready:missing_artifacts".to_string())
        );
        assert!(status.safe_next_commands.is_empty());
        Ok(())
    }

    #[test]
    fn lane_status_blocks_missing_scaffold_files_for_scaffold_manifest() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        fs::remove_file(lane.join("stage-gates.json"))?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert!(status.scaffold_required);
        assert!(!status.scaffold_complete);
        assert!(
            status
                .blocking_items
                .contains(&"scaffold_files_missing".to_string())
        );
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_rejects_non_moza_legacy_manifest() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("legacy-other-lane");
        fs::create_dir_all(&lane)?;
        write_legacy_moza_manifest(&lane, "Other Wheelbase")?;

        let err = build_hardware_lane_status_receipt(&lane)
            .err()
            .ok_or_else(|| io::Error::other("expected non-Moza legacy manifest failure"))?;
        let error_chain = format!("{err:#}");

        assert!(
            error_chain.contains("legacy manifest.json is not a Moza R5 lane manifest"),
            "{error_chain}"
        );
        Ok(())
    }

    #[test]
    fn lane_status_marks_presence_without_treating_it_as_proof() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        let throttle = lane.join("captures").join("r5-throttle-only-sweep.jsonl");
        fs::write(&throttle, "{}\n")?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        let throttle_role = status
            .role_evidence
            .iter()
            .find(|role| role.id == "throttle")
            .ok_or_else(|| io::Error::other("missing throttle role"))?;
        assert!(throttle_role.artifact_present);
        assert_eq!(throttle_role.validation_status, "not_validated_by_status");
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        Ok(())
    }

    #[test]
    fn lane_status_generic_discovery_avoids_moza_specific_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("generic-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "generic-wheelbase", "unknown", "Steven", false, None)?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "discovery");
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("wheelctl hardware doctor"))
        );
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("wheelctl device list"))
        );
        assert!(status.safe_next_commands.iter().all(|command| {
            !command.contains("moza") && !command.contains("0x346E") && !command.contains("torque")
        }));
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_passive_capture_guidance_includes_duration() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "passive");
        let capture_commands: Vec<_> = status
            .safe_next_commands
            .iter()
            .filter(|command| command.contains("wheelctl moza capture-input"))
            .collect();
        assert!(!capture_commands.is_empty());
        assert!(capture_commands.iter().all(|command| {
            command.contains("--duration-ms 10000")
                && command.contains("--json-out")
                && command.contains("--json")
        }));
        assert!(
            status
                .safe_next_commands
                .iter()
                .all(|command| !command.contains("torque")
                    && !command.contains("ffb")
                    && !command.contains("output"))
        );
        Ok(())
    }

    #[test]
    fn lane_status_passive_capture_guidance_skips_present_role_artifacts() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("r5-throttle-only-sweep.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "passive");
        let capture_commands: Vec<_> = status
            .safe_next_commands
            .iter()
            .filter(|command| command.contains("wheelctl moza capture-input"))
            .collect();
        assert!(
            capture_commands
                .iter()
                .all(|command| !command.contains("r5-throttle-only-sweep.jsonl"))
        );
        assert!(
            capture_commands
                .iter()
                .any(|command| command.contains("r5-steering-sweep.jsonl"))
        );
        Ok(())
    }

    #[test]
    fn lane_status_passive_capture_guidance_skips_placeholder_endpoints() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let role_overrides = HardwareLaneRoleOverrides::from_cli(
            &["button_box".to_string()],
            &[],
            &["button_box=captures/button-box.jsonl".to_string()],
            &[],
            &["button_box=wheelbase_hub".to_string()],
        )?;
        let _receipt = scaffold_hardware_lane_with_overrides(
            &lane,
            "moza-r5",
            "wheelbase-hub",
            "Steven",
            &role_overrides,
            false,
            None,
        )?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "passive");
        assert!(
            status
                .role_evidence
                .iter()
                .any(|role| role.id == "button_box"
                    && role.expected_endpoint == "declare-observed-endpoint"
                    && !role.artifact_present)
        );
        assert!(
            status
                .blocking_items
                .contains(&"passive:missing_role_endpoints".to_string())
        );
        assert!(
            status
                .blocking_items
                .contains(&"role_endpoint:button_box:missing".to_string())
        );
        assert!(!joined.contains("declare-observed-endpoint"), "{joined}");
        assert!(
            joined.contains("wheelctl hardware lane set-role-endpoint"),
            "{joined}"
        );
        assert!(joined.contains("--role button_box"), "{joined}");
        assert!(joined.contains("wheelctl moza capture-input --device hid-0x346E-0x0004"));
        Ok(())
    }

    #[test]
    fn lane_status_blocks_passive_when_present_capture_has_placeholder_endpoint() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let role_overrides = HardwareLaneRoleOverrides::from_cli(
            &["button_box".to_string()],
            &[],
            &["button_box=captures/button-box.jsonl".to_string()],
            &[],
            &["button_box=wheelbase_hub".to_string()],
        )?;
        let _receipt = scaffold_hardware_lane_with_overrides(
            &lane,
            "moza-r5",
            "wheelbase-hub",
            "Steven",
            &role_overrides,
            false,
            None,
        )?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
            "declared-rim-controls.jsonl",
            "button-box.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "passive");
        assert!(
            status
                .blocking_items
                .contains(&"passive:missing_role_endpoints".to_string())
        );
        assert!(
            status
                .blocking_items
                .contains(&"role_endpoint:button_box:missing".to_string())
        );
        assert!(
            joined.contains("wheelctl hardware lane set-role-endpoint"),
            "{joined}"
        );
        assert!(joined.contains("--role button_box"), "{joined}");
        assert!(
            !joined.contains("captures/button-box.jsonl"),
            "capture should not be suggested again when only the endpoint is missing: {joined}"
        );
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_set_role_endpoint_updates_manifest_and_capture_guidance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let role_overrides = HardwareLaneRoleOverrides::from_cli(
            &["button_box".to_string()],
            &[],
            &["button_box=captures/button-box.jsonl".to_string()],
            &[],
            &["button_box=wheelbase_hub".to_string()],
        )?;
        let _receipt = scaffold_hardware_lane_with_overrides(
            &lane,
            "moza-r5",
            "wheelbase-hub",
            "Steven",
            &role_overrides,
            false,
            None,
        )?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }

        let receipt = set_hardware_lane_role_endpoint(
            &lane,
            "button_box",
            "hid-0x1234-0x5678-if0-0x0001-0x0004",
            Some(&lane.join("role-endpoint-button_box.json")),
        )?;
        assert!(receipt.success);
        assert!(receipt.no_hid_device_opened);
        assert!(receipt.no_output_reports);
        assert!(receipt.no_feature_reports);
        assert_eq!(receipt.role, "button_box");
        assert_eq!(receipt.previous_endpoint, "declare-observed-endpoint");
        assert_eq!(
            receipt.expected_endpoint,
            "hid-0x1234-0x5678-if0-0x0001-0x0004"
        );
        assert!(lane.join("role-endpoint-button_box.json").exists());

        let manifest_text = fs::read_to_string(lane.join("hardware-lane-manifest.json"))?;
        assert!(manifest_text.contains("hid-0x1234-0x5678-if0-0x0001-0x0004"));
        let checklist_text = fs::read_to_string(lane.join("artifact-checklist.md"))?;
        assert!(checklist_text.contains("hid-0x1234-0x5678-if0-0x0001-0x0004"));
        let capture_plan_text = fs::read_to_string(lane.join("capture-plan.md"))?;
        assert!(capture_plan_text.contains("hid-0x1234-0x5678-if0-0x0001-0x0004"));

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");
        assert!(joined.contains("wheelctl moza capture-input --device hid-0x1234-0x5678"));
        assert!(!joined.contains("wheelctl hardware lane set-role-endpoint"));
        Ok(())
    }

    #[test]
    fn lane_set_role_endpoint_rejects_unknown_role_or_placeholder() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;

        let placeholder =
            set_hardware_lane_role_endpoint(&lane, "steering", "declare-observed-endpoint", None)
                .err()
                .ok_or_else(|| io::Error::other("expected placeholder endpoint failure"))?;
        assert!(
            placeholder
                .to_string()
                .contains("must be an observed endpoint selector")
        );

        let unknown = set_hardware_lane_role_endpoint(
            &lane,
            "button_box",
            "hid-0x1234-0x5678-if0-0x0001-0x0004",
            None,
        )
        .err()
        .ok_or_else(|| io::Error::other("expected unknown role failure"))?;
        assert!(unknown.to_string().contains("is not declared"));
        Ok(())
    }

    #[test]
    fn lane_status_suggests_descriptor_import_without_output_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "descriptor_trust");
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("--report-descriptor-bin-file"))
        );
        assert!(
            status
                .safe_next_commands
                .iter()
                .all(|command| !command.contains("torque")
                    && !command.contains("ffb")
                    && !command.contains("output"))
        );
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_descriptor_guidance_uses_declared_wheelbase_endpoint() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let role_overrides = HardwareLaneRoleOverrides::from_cli(
            &[],
            &[],
            &[],
            &["steering=hid-0x346E-0x0014-if2-0x0001-0x0004".to_string()],
            &[],
        )?;
        let _receipt = scaffold_hardware_lane_with_overrides(
            &lane,
            "moza-r5",
            "wheelbase-hub",
            "Steven",
            &role_overrides,
            false,
            None,
        )?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "descriptor_trust");
        assert!(
            joined.contains("--device hid-0x346E-0x0014-if2-0x0001-0x0004"),
            "{joined}"
        );
        assert!(
            joined.contains("scripts/extract_usbpcap_report_descriptor.ps1"),
            "{joined}"
        );
        assert!(
            joined.contains("-InputPcapng target/moza-r5-usbpcap-enumeration.pcapng"),
            "{joined}"
        );
        assert!(
            joined.contains("-Output target/moza-r5-report-descriptor.txt"),
            "{joined}"
        );
        assert!(
            !joined.contains("--device hid-0x346E-0x0004-if2-0x0001-0x0004"),
            "{joined}"
        );
        assert!(!joined.contains("torque"));
        assert!(!joined.contains("ffb"));
        Ok(())
    }

    #[test]
    fn lane_status_descriptor_guidance_uses_hardware_doctor_usbpcap_readiness() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        fs::write(
            lane.join("hardware-doctor.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "tools": {
                    "usbpcap_descriptor_capture": {
                        "tshark_present": true,
                        "usbpcap_interfaces_present": false,
                        "usbpcap_interface_count": 0,
                        "ready_for_usbpcap_descriptor_capture": false
                    }
                }
            }))?,
        )?;
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "descriptor_trust");
        assert_eq!(
            status
                .descriptor_capture_tooling
                .ready_for_usbpcap_descriptor_capture,
            Some(false)
        );
        assert!(
            status
                .descriptor_capture_tooling
                .guidance
                .contains("USBPcap/Wireshark capture interfaces are unavailable")
        );
        assert!(
            !joined.contains("scripts/extract_usbpcap_report_descriptor.ps1"),
            "{joined}"
        );
        assert!(
            joined.contains("--report-descriptor-hex-file target/moza-r5-report-descriptor.txt"),
            "{joined}"
        );
        assert!(
            joined.contains("--report-descriptor-bin-file target/moza-r5-report-descriptor.bin"),
            "{joined}"
        );
        assert!(!joined.contains("torque"));
        assert!(!joined.contains("ffb"));
        Ok(())
    }

    #[test]
    fn lane_status_requires_verifier_before_fixture_promotion_guidance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "fixture_promotion");
        assert!(
            status
                .safe_next_commands
                .iter()
                .any(|command| command.contains("verify-bundle --lane"))
        );
        assert!(status.safe_next_commands.iter().all(
            |command| !command.contains("promote-fixtures")
                && !command.contains("torque")
                && !command.contains("ffb")
                && !command.contains("output")
        ));
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_uses_failed_descriptor_verifier_as_descriptor_blocker() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;
        write_passive_verification_receipt(
            &lane,
            &[
                ("lane_directory", "pass"),
                ("passive_captures_parse", "pass"),
                ("descriptor_metadata", "fail"),
                ("fixture_promotion", "fail"),
            ],
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "descriptor_trust");
        assert_eq!(
            status.verifier_receipt.stage_blocker.as_deref(),
            Some("descriptor_trust")
        );
        assert!(
            status
                .blocking_items
                .contains(&"verifier_gate:descriptor_metadata:fail".to_string())
        );
        assert!(
            joined.contains("--report-descriptor-bin-file target/moza-r5-report-descriptor.bin"),
            "{joined}"
        );
        assert!(
            !joined.contains("verify-bundle --lane"),
            "descriptor guidance should come before fixture-promotion verifier reruns: {joined}"
        );
        assert!(!joined.contains("torque"));
        assert!(!joined.contains("ffb"));
        assert!(!joined.contains("output"));
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_uses_fixture_blocker_after_descriptor_verifier_passes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;
        write_passive_verification_receipt(
            &lane,
            &[
                ("lane_directory", "pass"),
                ("passive_captures_parse", "pass"),
                ("descriptor_metadata", "pass"),
                ("fixture_promotion", "fail"),
            ],
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;
        let joined = status.safe_next_commands.join("\n");

        assert_eq!(status.next_blocked_stage, "fixture_promotion");
        assert_eq!(
            status.verifier_receipt.stage_blocker.as_deref(),
            Some("fixture_promotion")
        );
        assert!(joined.contains("wheelctl moza validate-captures"));
        assert!(joined.contains("wheelctl moza verify-bundle"));
        assert!(!joined.contains("promote-fixtures"));
        assert!(!joined.contains("torque"));
        assert!(!joined.contains("ffb"));
        assert!(!joined.contains("output"));
        Ok(())
    }

    #[test]
    fn lane_status_withholds_output_stage_commands() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
            "fixture-promotion.json",
            "passive-verification.json",
            "lane-audit-passive.json",
            "pre-output-readiness.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;

        assert_eq!(status.next_blocked_stage, "zero_torque");
        assert!(status.safe_next_commands.is_empty());
        assert!(!status.evidence_claims_validated);
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn lane_status_blocks_on_failed_zero_receipt_before_watchdog() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().join("moza-r5-lane");
        let _receipt =
            scaffold_hardware_lane(&lane, "moza-r5", "wheelbase-hub", "Steven", false, None)?;
        for artifact in [
            "device-list.json",
            "hardware-doctor.json",
            "hid-list.json",
            "moza-probe.json",
            "lane-capture-analysis.json",
            "parser-fixture-validation.json",
            "descriptor.json",
            "fixture-promotion.json",
            "passive-verification.json",
            "lane-audit-passive.json",
            "pre-output-readiness.json",
        ] {
            fs::write(lane.join(artifact), "{}\n")?;
        }
        fs::write(
            lane.join("zero-torque-proof.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "success": false,
                "command": "wheelctl moza zero"
            }))?,
        )?;
        for role in [
            "r5-steering-sweep.jsonl",
            "r5-throttle-only-sweep.jsonl",
            "r5-brake-only-sweep.jsonl",
        ] {
            fs::write(lane.join("captures").join(role), "{}\n")?;
        }
        fs::write(
            lane.join("captures").join("declared-rim-controls.jsonl"),
            "{}\n",
        )?;

        let status = build_hardware_lane_status_receipt(&lane)?;
        let zero_stage = status
            .stages
            .iter()
            .find(|stage| stage.id == "zero_torque")
            .ok_or("expected zero torque stage")?;

        assert_eq!(status.next_blocked_stage, "zero_torque");
        assert_eq!(zero_stage.artifacts_failed, 1);
        assert!(
            status
                .blocking_items
                .contains(&"zero_torque:failed_artifacts".to_string())
        );
        assert!(status.safe_next_commands.is_empty());
        assert!(!status.ready_for_zero_torque);
        assert!(!status.ready_for_ffb);
        Ok(())
    }

    #[test]
    fn windows_pnp_parser_extracts_moza_composite_interfaces() -> TestResult {
        let text = r#"[
            {
                "Status": "OK",
                "Class": "HIDClass",
                "FriendlyName": "HID-compliant game controller",
                "InstanceId": "HID\\VID_346E&PID_0004&MI_02\\8&6C29B84&0&0000"
            },
            {
                "Status": "OK",
                "Class": "Ports",
                "FriendlyName": "USB Serial Device (COM4)",
                "InstanceId": "USB\\VID_346E&PID_0004&MI_00\\7&13CD44B0&0&0000"
            },
            {
                "Status": "OK",
                "Class": "HIDClass",
                "FriendlyName": "USB Input Device",
                "InstanceId": "HID\\VID_346E&PID_0004&MI_02\\8&6C29B84&0&0001"
            },
            {
                "Status": "OK",
                "Class": "USB",
                "FriendlyName": "USB Composite Device",
                "InstanceId": "USB\\VID_346E&PID_0004\\410051000251333135363734"
            }
        ]"#;

        let checks = windows_pnp_checks_from_json(text);

        assert_eq!(checks.moza_vid_visible, Some(true));
        assert_eq!(checks.hid_interface_count, 1);
        assert_eq!(checks.hid_pnp_device_count, 2);
        assert_eq!(checks.serial_interface_count, 1);
        let serial = checks
            .devices
            .iter()
            .find(|device| device.class_name.as_deref() == Some("Ports"))
            .ok_or_else(|| io::Error::other("missing serial-class PnP device"))?;
        assert_eq!(serial.vendor_id.as_deref(), Some("0x346E"));
        assert_eq!(serial.product_id.as_deref(), Some("0x0004"));
        assert_eq!(serial.interface_number, Some(0));

        let json = serde_json::to_string(&checks)?;
        assert!(!json.contains("InstanceId"));
        assert!(!json.contains("410051000251333135363734"));
        Ok(())
    }

    #[test]
    fn windows_pnp_parser_accepts_single_device_json_object() -> TestResult {
        let text = r#"{
            "Status": "OK",
            "Class": "HIDClass",
            "FriendlyName": "USB Input Device",
            "InstanceId": "USB\\VID_346E&PID_0004&MI_02\\7&13CD44B0&0&0002"
        }"#;

        let checks = windows_pnp_checks_from_json(text);

        assert_eq!(checks.moza_vid_visible, Some(true));
        assert_eq!(checks.hid_interface_count, 1);
        assert_eq!(checks.hid_pnp_device_count, 1);
        assert_eq!(checks.serial_interface_count, 0);
        assert_eq!(checks.devices.len(), 1);
        let device = checks
            .devices
            .first()
            .ok_or_else(|| io::Error::other("missing PnP device"))?;
        assert_eq!(device.interface_number, Some(2));
        Ok(())
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
