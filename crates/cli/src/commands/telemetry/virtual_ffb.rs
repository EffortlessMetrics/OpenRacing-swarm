//! Virtual force-feedback telemetry log generation.

use super::{
    CliError, default_recorder_session_id, normalized_f64, now_utc, validated_normalized_snapshots,
    write_jsonl_values,
};
use anyhow::{Context, Result, anyhow};
use openracing_hardware_core::{
    DescriptorTrustEvidence, Disconnected, EnumerationEvidence, EvidenceSource, FinalZeroEvidence,
    FinalZeroPolicy, LowTorqueArmEvidence, LowTorqueEvidence, OutputBarrierDecisionReason,
    OutputCommand, OutputWatchdogState, OutputWriteBarrier, PassiveVerificationEvidence,
    SimulatorTelemetryEvidence, VirtualHidDescriptor, VirtualHidIdentity, VirtualHidReplay,
    ZeroOutputEvidence,
};
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write as FmtWrite;
use std::path::Path;

const VIRTUAL_FFB_LOG_COMMAND: &str = "wheelctl telemetry virtual-ffb-log";
const VIRTUAL_FFB_REPORT_FORMAT: &str = "openracing_virtual_ffb_v1";
const VIRTUAL_FFB_VENDOR_ID: u16 = 0xFFFF;
const VIRTUAL_FFB_PRODUCT_ID: u16 = 0x0001;

#[derive(Debug, Serialize)]
struct VirtualFfbLogSummary {
    command: &'static str,
    input: String,
    output: String,
    writer_session_id: String,
    hardware_source: &'static str,
    real_hardware_validated: bool,
    real_simulator_validated: bool,
    hardware_output_enabled: bool,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    virtual_output_enabled: bool,
    max_output_percent: f32,
    watchdog_timeout_ms: u64,
    telemetry_snapshot_count: u64,
    virtual_output_report_count: u64,
    nonzero_output_count: u64,
    zero_output_count: u64,
    clear_zero_count: u64,
    final_zero_appended: bool,
}

pub(super) async fn write_virtual_ffb_log(
    input_path: &str,
    output_path: &str,
    session_id: Option<&str>,
    max_percent: f32,
    watchdog_timeout_ms: u64,
    json: bool,
) -> Result<()> {
    validate_virtual_ffb_args(output_path, max_percent)?;

    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_recorder_session_id("virtual-ffb"));
    let snapshots = validated_normalized_snapshots(input_path)?;
    let telemetry_snapshot_count =
        u64::try_from(snapshots.len()).context("too many normalized telemetry records")?;

    let capability = virtual_simulator_smoke_capability(max_percent)?;
    let watchdog = OutputWatchdogState::active(watchdog_timeout_ms)?;
    let mut barrier = OutputWriteBarrier::new(capability, watchdog, FinalZeroPolicy::required());
    let mut replay = VirtualHidReplay::new(virtual_ffb_identity()?, virtual_ffb_descriptor()?);
    let writer_started_at_utc = now_utc();
    let mut output_records = Vec::new();
    let mut nonzero_output_count = 0u64;
    let mut zero_output_count = 0u64;
    let mut last_timestamp_us = 0u64;
    let mut last_telemetry_sequence = 0u64;

    for snapshot in &snapshots {
        let sequence = snapshot
            .get("sequence")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("validated snapshot is missing sequence"))?;
        let timestamp_ns = snapshot
            .get("timestamp_ns")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("validated snapshot is missing timestamp_ns"))?;
        let ffb_scalar = normalized_f64(snapshot, "ffb_scalar")
            .ok_or_else(|| anyhow!("validated snapshot is missing ffb_scalar"))?;
        let output_percent = ffb_scalar * f64::from(max_percent);
        let command = OutputCommand::new(output_percent as f32)?;
        let decision = barrier.evaluate(command)?;
        let timestamp_us = timestamp_ns / 1_000;
        replay.set_timestamp_us(timestamp_us);
        let report = encode_virtual_ffb_report(output_percent)?;
        let bytes_written = replay
            .write_output_report(&report)
            .context("virtual FFB output write unexpectedly failed")?;
        if command.is_zero() {
            zero_output_count = zero_output_count.saturating_add(1);
        } else {
            nonzero_output_count = nonzero_output_count.saturating_add(1);
        }
        last_timestamp_us = timestamp_us;
        last_telemetry_sequence = sequence;
        output_records.push(virtual_ffb_output_record(VirtualFfbRecordRequest {
            sequence: u64::try_from(output_records.len()).context("too many output records")?,
            kind: if command.is_zero() {
                "zero_output"
            } else {
                "sim_output"
            },
            clear_event: None,
            elapsed_us: timestamp_us,
            telemetry_sequence: sequence,
            input_ffb_scalar: ffb_scalar,
            output_percent,
            report: &report,
            bytes_written,
            barrier_reason: decision.reason(),
            writer_session_id: &session_id,
            writer_started_at_utc: &writer_started_at_utc,
            watchdog_timeout_ms,
            max_percent,
        }));
    }

    let mut clear_zero_count = 0u64;
    for clear_event in ["stop", "pause", "game_exit", "mode_mismatch"] {
        let record = write_virtual_zero_record(
            &mut barrier,
            &mut replay,
            &mut output_records,
            VirtualZeroRecordRequest {
                kind: "clear_zero",
                clear_event: Some(clear_event),
                elapsed_us: last_timestamp_us
                    .saturating_add(clear_zero_count.saturating_add(1).saturating_mul(1_000)),
                telemetry_sequence: last_telemetry_sequence,
                writer_session_id: &session_id,
                writer_started_at_utc: &writer_started_at_utc,
                watchdog_timeout_ms,
                max_percent,
            },
        )?;
        if record {
            zero_output_count = zero_output_count.saturating_add(1);
            clear_zero_count = clear_zero_count.saturating_add(1);
        }
    }

    write_virtual_zero_record(
        &mut barrier,
        &mut replay,
        &mut output_records,
        VirtualZeroRecordRequest {
            kind: "final_zero",
            clear_event: None,
            elapsed_us: last_timestamp_us
                .saturating_add(clear_zero_count.saturating_add(1).saturating_mul(1_000)),
            telemetry_sequence: last_telemetry_sequence,
            writer_session_id: &session_id,
            writer_started_at_utc: &writer_started_at_utc,
            watchdog_timeout_ms,
            max_percent,
        },
    )?;
    zero_output_count = zero_output_count.saturating_add(1);

    write_jsonl_values(output_path, &output_records)?;

    let virtual_output_report_count =
        u64::try_from(output_records.len()).context("too many virtual output records")?;
    let summary = VirtualFfbLogSummary {
        command: VIRTUAL_FFB_LOG_COMMAND,
        input: input_path.to_string(),
        output: output_path.to_string(),
        writer_session_id: session_id,
        hardware_source: "virtual",
        real_hardware_validated: false,
        real_simulator_validated: false,
        hardware_output_enabled: false,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        virtual_output_enabled: true,
        max_output_percent: max_percent,
        watchdog_timeout_ms,
        telemetry_snapshot_count,
        virtual_output_report_count,
        nonzero_output_count,
        zero_output_count,
        clear_zero_count,
        final_zero_appended: output_records
            .last()
            .and_then(|record| record.get("kind"))
            .and_then(Value::as_str)
            == Some("final_zero"),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Virtual telemetry FFB log complete");
        println!("  input: {}", summary.input);
        println!("  output: {}", summary.output);
        println!("  snapshots: {}", summary.telemetry_snapshot_count);
        println!(
            "  virtual_output_reports: {}",
            summary.virtual_output_report_count
        );
        println!("  hardware_source: {}", summary.hardware_source);
        println!(
            "  real_hardware_validated: {}",
            summary.real_hardware_validated
        );
    }

    Ok(())
}

fn validate_virtual_ffb_args(output_path: &str, max_percent: f32) -> Result<()> {
    if path_is_under_ci_hardware(Path::new(output_path)) {
        return Err(CliError::InvalidConfiguration(
            "virtual FFB logs must not be written under ci/hardware/**".to_string(),
        )
        .into());
    }

    if !max_percent.is_finite() || max_percent <= 0.0 || max_percent > 5.0 {
        return Err(CliError::InvalidConfiguration(
            "--max-percent must be finite and in the 0 < value <= 5 range".to_string(),
        )
        .into());
    }

    Ok(())
}

fn path_is_under_ci_hardware(path: &Path) -> bool {
    let mut previous_ci = false;
    for component in path.components() {
        let ComponentName::Normal(name) = component_name(component) else {
            previous_ci = false;
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if previous_ci && lower == "hardware" {
            return true;
        }
        previous_ci = lower == "ci";
    }
    false
}

enum ComponentName {
    Normal(String),
    Other,
}

fn component_name(component: std::path::Component<'_>) -> ComponentName {
    match component {
        std::path::Component::Normal(value) => {
            ComponentName::Normal(value.to_string_lossy().to_string())
        }
        _ => ComponentName::Other,
    }
}

fn virtual_simulator_smoke_capability(
    max_percent: f32,
) -> Result<openracing_hardware_core::OutputCapability> {
    let simulator_smoke_armed = Disconnected::new()
        .enumerate(EnumerationEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/device-list.json",
            VIRTUAL_FFB_VENDOR_ID,
            VIRTUAL_FFB_PRODUCT_ID,
            "openracing-virtual-ffb",
        )?)
        .trust_descriptor(DescriptorTrustEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/descriptor.json",
        )?)
        .verify_passive(PassiveVerificationEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/passive-verification.json",
        )?)
        .verify_zero_output(ZeroOutputEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/zero-output-proof.json",
        )?)
        .arm_low_torque(LowTorqueArmEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/low-torque-arm.json",
        )?)
        .verify_low_torque(
            LowTorqueEvidence::new(
                EvidenceSource::Virtual,
                "target/openracing/virtual/low-torque-proof.json",
            )?,
            FinalZeroEvidence::new(
                EvidenceSource::Virtual,
                "target/openracing/virtual/low-torque-final-zero.json",
            )?,
        )
        .arm_simulator_smoke(SimulatorTelemetryEvidence::new(
            EvidenceSource::Virtual,
            "target/openracing/virtual/simulator-telemetry-proof.json",
        )?);

    Ok(simulator_smoke_armed.simulator_smoke_output_capability(max_percent)?)
}

fn virtual_ffb_identity() -> Result<VirtualHidIdentity> {
    Ok(VirtualHidIdentity::new(
        VIRTUAL_FFB_VENDOR_ID,
        VIRTUAL_FFB_PRODUCT_ID,
        "openracing-virtual-ffb",
    )?
    .with_manufacturer("OpenRacing")?
    .with_product_name("Virtual FFB Replay")?
    .with_serial_number_present(false)
    .with_interface(0)
    .with_usage(0x0001, 0x0004))
}

fn virtual_ffb_descriptor() -> Result<VirtualHidDescriptor> {
    Ok(VirtualHidDescriptor::new("virtual-ffb-v1")?
        .with_input_report_lengths([1])?
        .with_output_report_ids([0x00])
        .with_feature_report_ids([]))
}

struct VirtualFfbRecordRequest<'a> {
    sequence: u64,
    kind: &'a str,
    clear_event: Option<&'a str>,
    elapsed_us: u64,
    telemetry_sequence: u64,
    input_ffb_scalar: f64,
    output_percent: f64,
    report: &'a [u8],
    bytes_written: usize,
    barrier_reason: OutputBarrierDecisionReason,
    writer_session_id: &'a str,
    writer_started_at_utc: &'a str,
    watchdog_timeout_ms: u64,
    max_percent: f32,
}

fn virtual_ffb_output_record(request: VirtualFfbRecordRequest<'_>) -> Value {
    serde_json::json!({
        "sequence": request.sequence,
        "kind": request.kind,
        "clear_event": request.clear_event,
        "elapsed_us": request.elapsed_us,
        "telemetry_sequence": request.telemetry_sequence,
        "input_ffb_scalar": request.input_ffb_scalar,
        "output_percent": request.output_percent,
        "signed_percent": request.output_percent,
        "hardware_source": "virtual",
        "real_hardware_validated": false,
        "real_simulator_validated": false,
        "hardware_output_enabled": false,
        "virtual_output_enabled": true,
        "no_hid_device_opened": true,
        "no_ffb_writes": true,
        "no_serial_config_commands": true,
        "no_firmware_or_dfu_commands": true,
        "virtual_write_attempted": true,
        "virtual_write_result": "ok",
        "bytes_written": request.bytes_written,
        "report_format": VIRTUAL_FFB_REPORT_FORMAT,
        "virtual_report_hex": hex_bytes(request.report),
        "barrier_reason": request.barrier_reason,
        "watchdog_active": true,
        "watchdog_timeout_ms": request.watchdog_timeout_ms,
        "max_output_percent": request.max_percent,
        "writer_command": VIRTUAL_FFB_LOG_COMMAND,
        "writer_session_id": request.writer_session_id,
        "writer_started_at_utc": request.writer_started_at_utc,
    })
}

struct VirtualZeroRecordRequest<'a> {
    kind: &'a str,
    clear_event: Option<&'a str>,
    elapsed_us: u64,
    telemetry_sequence: u64,
    writer_session_id: &'a str,
    writer_started_at_utc: &'a str,
    watchdog_timeout_ms: u64,
    max_percent: f32,
}

fn write_virtual_zero_record(
    barrier: &mut OutputWriteBarrier,
    replay: &mut VirtualHidReplay,
    output_records: &mut Vec<Value>,
    request: VirtualZeroRecordRequest<'_>,
) -> Result<bool> {
    let decision = barrier.evaluate(OutputCommand::ZERO)?;
    replay.set_timestamp_us(request.elapsed_us);
    let report = encode_virtual_ffb_report(0.0)?;
    let bytes_written = replay
        .write_output_report(&report)
        .context("virtual zero output write unexpectedly failed")?;
    output_records.push(virtual_ffb_output_record(VirtualFfbRecordRequest {
        sequence: u64::try_from(output_records.len()).context("too many output records")?,
        kind: request.kind,
        clear_event: request.clear_event,
        elapsed_us: request.elapsed_us,
        telemetry_sequence: request.telemetry_sequence,
        input_ffb_scalar: 0.0,
        output_percent: 0.0,
        report: &report,
        bytes_written,
        barrier_reason: decision.reason(),
        writer_session_id: request.writer_session_id,
        writer_started_at_utc: request.writer_started_at_utc,
        watchdog_timeout_ms: request.watchdog_timeout_ms,
        max_percent: request.max_percent,
    }));
    Ok(true)
}

fn encode_virtual_ffb_report(percent: f64) -> Result<[u8; 8]> {
    if !percent.is_finite() {
        return Err(anyhow!("virtual FFB percent must be finite"));
    }
    let raw = (percent.clamp(-100.0, 100.0) / 100.0) * f64::from(i16::MAX);
    let raw_i32 = raw.round() as i32;
    let raw_i16 = i16::try_from(raw_i32).context("virtual FFB raw value out of range")?;
    let raw_bytes = raw_i16.to_le_bytes();
    Ok([
        0x00,
        raw_bytes[0],
        raw_bytes[1],
        if raw_i16 == 0 { 0x00 } else { 0x01 },
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
