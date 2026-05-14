//! Hardware environment diagnostics.
//!
//! The doctor command is observe-only. It initializes HID enumeration when
//! available, records tool/platform readiness, and never opens devices or sends
//! output, feature, serial, firmware, or DFU commands.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use hidapi::{DeviceInfo, HidApi};
use openracing_hardware_core::{DeviceCapabilityRegistry, DeviceFamily};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::commands::{HardwareCommands, HardwareLaneCommands};

pub async fn execute(cmd: &HardwareCommands, json: bool) -> Result<()> {
    match cmd {
        HardwareCommands::Doctor { json_out } => doctor(json, json_out.as_deref()).await,
        HardwareCommands::BringupRail { family, json_out } => {
            bringup_rail(json, family, json_out.as_deref()).await
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
            let missing = artifacts.len().saturating_sub(present);
            HardwareLaneStageStatus {
                id: stage.id,
                order: stage.order,
                purpose: stage.purpose,
                artifacts_present: present,
                artifacts_missing: missing,
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
    let next_blocked_stage = lane_status_next_blocked_stage(
        first_missing_artifact_stage,
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
        "bounded_ffb" => vec![
            lane_artifact_status(lane, "receipt", "low-torque-proof.json"),
            lane_artifact_status(lane, "receipt", "pit-house-coexistence.json"),
            lane_artifact_status(lane, "receipt", "simulator-telemetry-proof.json"),
            lane_artifact_status(lane, "receipt", "simulator-ffb-smoke.json"),
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
        "bounded_ffb" => Some(8),
        "ffb_extended" => Some(9),
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
    missing_required_role_endpoint: bool,
    verifier_stage_blocker: Option<&str>,
) -> &'static str {
    let mut earliest = first_missing_artifact_stage;
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
        "bounded_ffb" => Some(("bounded_ffb", 8)),
        "ffb_extended" => Some(("ffb_extended", 9)),
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
        (_, "zero_torque" | "watchdog" | "disconnect" | "bounded_ffb" | "ffb_extended") => {
            Vec::new()
        }
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
    let guidance = match ready_for_usbpcap_descriptor_capture {
        Some(true) => {
            "USBPcap/Wireshark capture interfaces are available for descriptor enumeration capture"
                .to_string()
        }
        Some(false) => {
            "USBPcap/Wireshark capture interfaces are unavailable; use native Linux/sysfs, install USBPcap intentionally, or import descriptor bytes from another trusted raw HID descriptor source"
                .to_string()
        }
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
        warnings.push("USBPcap/Wireshark descriptor capture is not ready on this host".to_string());
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
    let Some(tshark_path) = find_tshark_path() else {
        return UsbPcapDescriptorCaptureChecks {
            tshark_present: false,
            tshark_path: None,
            interface_scan_attempted: false,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            ready_for_usbpcap_descriptor_capture: false,
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
            interface_scan_attempted: true,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            ready_for_usbpcap_descriptor_capture: false,
            error: Some("failed to run tshark -D".to_string()),
        };
    };

    if !output.status.success() {
        return UsbPcapDescriptorCaptureChecks {
            tshark_present: true,
            tshark_path: Some(tshark_path.display().to_string()),
            interface_scan_attempted: true,
            usbpcap_interfaces_present: false,
            usbpcap_interface_count: 0,
            usbpcap_interfaces: Vec::new(),
            ready_for_usbpcap_descriptor_capture: false,
            error: Some(format!(
                "tshark -D failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let interfaces = usbpcap_interfaces_from_tshark_list(&stdout);
    let ready_for_usbpcap_descriptor_capture = !interfaces.is_empty();
    UsbPcapDescriptorCaptureChecks {
        tshark_present: true,
        tshark_path: Some(tshark_path.display().to_string()),
        interface_scan_attempted: true,
        usbpcap_interfaces_present: ready_for_usbpcap_descriptor_capture,
        usbpcap_interface_count: interfaces.len(),
        usbpcap_interfaces: interfaces,
        ready_for_usbpcap_descriptor_capture,
        error: None,
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
    interface_scan_attempted: bool,
    usbpcap_interfaces_present: bool,
    usbpcap_interface_count: usize,
    usbpcap_interfaces: Vec<String>,
    ready_for_usbpcap_descriptor_capture: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
                    interface_scan_attempted: true,
                    usbpcap_interfaces_present: false,
                    usbpcap_interface_count: 0,
                    usbpcap_interfaces: Vec::new(),
                    ready_for_usbpcap_descriptor_capture: false,
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
        fs::write(
            lane.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "completion_state": "passive_in_progress",
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
    fn doctor_warns_when_usbpcap_descriptor_capture_is_unavailable() {
        let receipt = sample_receipt();

        assert!(
            receipt
                .warnings
                .iter()
                .any(|warning| warning.contains("USBPcap/Wireshark descriptor capture"))
        );
        assert!(
            !receipt
                .tools
                .usbpcap_descriptor_capture
                .ready_for_usbpcap_descriptor_capture
        );
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
