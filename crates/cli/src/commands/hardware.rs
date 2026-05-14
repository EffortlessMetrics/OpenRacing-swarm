//! Hardware environment diagnostics.
//!
//! The doctor command is observe-only. It initializes HID enumeration when
//! available, records tool/platform readiness, and never opens devices or sends
//! output, feature, serial, firmware, or DFU commands.

use std::collections::BTreeSet;
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
        HardwareCommands::BringupRail { family, json_out } => {
            bringup_rail(json, family, json_out.as_deref()).await
        }
    }
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
            id: "bounded_ffb",
            order: 8,
            purpose: "first real-force smoke under explicit force and duration caps",
            required_artifacts: vec![
                "low-torque-proof.json",
                "pit-house-coexistence.json",
                "simulator-telemetry-proof.json",
                "bounded FFB output log",
            ],
            required_gates: vec![
                "zero_watchdog_disconnect_passed",
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
            id: "ffb_extended",
            order: 9,
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
        ffb_eligibility: "requires zero-torque, watchdog, disconnect, low-torque, Pit House, and simulator telemetry receipts",
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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
        Ok(())
    }

    #[test]
    fn bringup_rail_rejects_unknown_adapter() {
        let err = build_bringup_rail_receipt("unknown-family").expect_err("expected error");
        assert!(err.to_string().contains("unknown hardware bring-up family"));
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
