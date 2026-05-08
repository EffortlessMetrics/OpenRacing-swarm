//! Telemetry discovery and capture commands.

use crate::commands::TelemetryCommands;
use crate::error::CliError;
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const REGISTER_COMMAND_APPLICATION: u8 = 1;
const PROTOCOL_VERSION: u8 = 4;
const MSG_REGISTRATION_RESULT: u8 = 1;
const MAX_PACKET_SIZE: usize = 4096;
const CAPTURE_MAGIC: &[u8; 8] = b"ORACAPv1";
const RECORD_COMMAND: &str = "wheelctl telemetry record";
const DEFAULT_RECORD_FRAME_PERIOD_NS: u64 = 16_666_667;

#[derive(Debug, Serialize)]
struct ProbeAttempt {
    attempt: u32,
    status: String,
    elapsed_ms: u64,
    response_size: usize,
    message_type: Option<u8>,
    registration_connection_id: Option<i32>,
    registration_success: Option<bool>,
    registration_readonly: Option<bool>,
    registration_error: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProbeSummary {
    game_id: String,
    endpoint: String,
    attempts: u32,
    any_response: bool,
    attempts_detail: Vec<ProbeAttempt>,
}

#[derive(Debug, Serialize)]
struct CaptureSummary {
    game_id: String,
    listen: String,
    duration_seconds: u64,
    packets_captured: u64,
    bytes_written: u64,
    output: String,
}

#[derive(Debug, Serialize)]
struct RecordSummary {
    command: &'static str,
    game: String,
    telemetry_source: String,
    input: String,
    output: String,
    recorder_session_id: String,
    normalized_snapshot_count: u64,
    duration_ms: u64,
    hardware_output_enabled: bool,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
}

/// Execute telemetry command.
pub async fn execute(cmd: &TelemetryCommands, json: bool) -> Result<()> {
    match cmd {
        TelemetryCommands::Probe {
            game,
            endpoint,
            timeout_ms,
            attempts,
        } => probe(game, endpoint, *timeout_ms, *attempts, json).await,
        TelemetryCommands::Capture {
            game,
            port,
            duration,
            out,
            max_payload,
        } => capture(game, *port, *duration, out, *max_payload, json).await,
        TelemetryCommands::Record {
            game,
            telemetry_source,
            input,
            out,
            session_id,
            duration_ms,
        } => {
            record_normalized_snapshots(
                game,
                telemetry_source,
                input,
                out,
                session_id.as_deref(),
                *duration_ms,
                json,
            )
            .await
        }
    }
}

async fn probe(
    game_id: &str,
    endpoint: &str,
    timeout_ms: u64,
    attempts: u32,
    json: bool,
) -> Result<()> {
    ensure_probe_game(game_id)?;
    let endpoint_addr: SocketAddr = endpoint.parse().map_err(|error| {
        CliError::InvalidConfiguration(format!("Invalid --endpoint '{}': {}", endpoint, error))
    })?;

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let total_attempts = attempts.max(1);
    let mut detail = Vec::with_capacity(total_attempts as usize);
    let mut any_response = false;

    for attempt in 1..=total_attempts {
        let started = Instant::now();
        let result = probe_once(endpoint_addr, timeout).await;
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        let probe_attempt = match result {
            Ok(ProbeOutcome::Registration(result)) => {
                any_response = true;
                ProbeAttempt {
                    attempt,
                    status: "registration_result".to_string(),
                    elapsed_ms,
                    response_size: result.raw_size,
                    message_type: Some(MSG_REGISTRATION_RESULT),
                    registration_connection_id: Some(result.connection_id),
                    registration_success: Some(result.success),
                    registration_readonly: Some(result.readonly),
                    registration_error: Some(result.error),
                    error: None,
                }
            }
            Ok(ProbeOutcome::Response {
                message_type,
                raw_size,
            }) => {
                any_response = true;
                ProbeAttempt {
                    attempt,
                    status: "response".to_string(),
                    elapsed_ms,
                    response_size: raw_size,
                    message_type: Some(message_type),
                    registration_connection_id: None,
                    registration_success: None,
                    registration_readonly: None,
                    registration_error: None,
                    error: None,
                }
            }
            Ok(ProbeOutcome::Timeout) => ProbeAttempt {
                attempt,
                status: "timeout".to_string(),
                elapsed_ms,
                response_size: 0,
                message_type: None,
                registration_connection_id: None,
                registration_success: None,
                registration_readonly: None,
                registration_error: None,
                error: None,
            },
            Err(error) => ProbeAttempt {
                attempt,
                status: "error".to_string(),
                elapsed_ms,
                response_size: 0,
                message_type: None,
                registration_connection_id: None,
                registration_success: None,
                registration_readonly: None,
                registration_error: None,
                error: Some(error.to_string()),
            },
        };

        detail.push(probe_attempt);
    }

    let summary = ProbeSummary {
        game_id: game_id.to_string(),
        endpoint: endpoint_addr.to_string(),
        attempts: total_attempts,
        any_response,
        attempts_detail: detail,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Telemetry probe for {} at {}",
            summary.game_id, summary.endpoint
        );
        println!("Attempts: {}", summary.attempts);
        println!("Any response: {}", summary.any_response);
        for attempt in &summary.attempts_detail {
            println!(
                "  attempt {} -> {} ({} ms)",
                attempt.attempt, attempt.status, attempt.elapsed_ms
            );
            if let Some(error) = &attempt.error {
                println!("    error: {}", error);
            }
            if let Some(message_type) = attempt.message_type {
                println!("    message_type: {}", message_type);
            }
            if let Some(connection_id) = attempt.registration_connection_id {
                println!("    registration_connection_id: {}", connection_id);
            }
            if let Some(success) = attempt.registration_success {
                println!("    registration_success: {}", success);
            }
            if let Some(readonly) = attempt.registration_readonly {
                println!("    registration_readonly: {}", readonly);
            }
            if let Some(error) = &attempt.registration_error
                && !error.is_empty()
            {
                println!("    registration_error: {}", error);
            }
        }
    }

    Ok(())
}

async fn capture(
    game_id: &str,
    port: u16,
    duration_seconds: u64,
    output_path: &str,
    max_payload: usize,
    json: bool,
) -> Result<()> {
    ensure_probe_game(game_id)?;
    if max_payload == 0 {
        return Err(CliError::InvalidConfiguration("--max-payload must be > 0".to_string()).into());
    }

    let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    let socket = UdpSocket::bind(bind_addr).await.with_context(|| {
        format!(
            "failed to bind UDP capture socket at {} (is another process using this port?)",
            bind_addr
        )
    })?;

    let mut file = File::create(output_path)
        .with_context(|| format!("failed to create capture output file '{}'", output_path))?;
    file.write_all(CAPTURE_MAGIC)?;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(duration_seconds.max(1));
    let mut packets_captured = 0u64;
    let mut buf = [0u8; MAX_PACKET_SIZE];

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = remaining.min(Duration::from_millis(250));
        let recv = tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await;
        let (len, source) = match recv {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => return Err(anyhow!("capture receive failed: {}", error)),
            Err(_) => continue,
        };

        let stored_len = len.min(max_payload);
        let timestamp_ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let source_bytes = source.to_string();
        let source_raw = source_bytes.as_bytes();
        let source_len = u16::try_from(source_raw.len()).map_err(|_| {
            anyhow!(
                "source endpoint string too long to encode: {}",
                source_bytes
            )
        })?;

        file.write_all(&timestamp_ns.to_le_bytes())?;
        file.write_all(&source_len.to_le_bytes())?;
        file.write_all(source_raw)?;
        file.write_all(&(len as u32).to_le_bytes())?;
        file.write_all(&(stored_len as u32).to_le_bytes())?;
        file.write_all(&buf[..stored_len])?;

        packets_captured = packets_captured.saturating_add(1);
    }

    file.flush()?;
    let bytes_written = file.metadata()?.len();

    let summary = CaptureSummary {
        game_id: game_id.to_string(),
        listen: bind_addr.to_string(),
        duration_seconds,
        packets_captured,
        bytes_written,
        output: output_path.to_string(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Telemetry capture complete");
        println!("  game: {}", summary.game_id);
        println!("  listen: {}", summary.listen);
        println!("  duration_s: {}", summary.duration_seconds);
        println!("  packets: {}", summary.packets_captured);
        println!("  bytes_written: {}", summary.bytes_written);
        println!("  output: {}", summary.output);
    }

    Ok(())
}

async fn record_normalized_snapshots(
    game_id: &str,
    telemetry_source: &str,
    input_path: &str,
    output_path: &str,
    session_id: Option<&str>,
    duration_ms: u64,
    json: bool,
) -> Result<()> {
    if game_id.trim().is_empty() {
        return Err(CliError::InvalidConfiguration("--game must not be empty".to_string()).into());
    }
    if !matches!(telemetry_source, "real_game" | "simhub_bridge") {
        return Err(CliError::InvalidConfiguration(
            "--telemetry-source must be real_game or simhub_bridge".to_string(),
        )
        .into());
    }
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_recorder_session_id(game_id));

    let records = read_normalized_telemetry_records(input_path)?;
    if records.is_empty() {
        return Err(anyhow!(
            "normalized telemetry input '{}' did not contain any snapshots",
            input_path
        ));
    }

    if let Some(parent) = Path::new(output_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let mut file = File::create(output_path)
        .with_context(|| format!("failed to create recorder output '{}'", output_path))?;

    for (sequence, record) in records.iter().enumerate() {
        let mut snapshot = normalized_telemetry_payload(record)
            .ok_or_else(|| anyhow!("record {sequence} is not a JSON object"))?
            .clone();
        if !normalized_telemetry_payload_is_valid(&snapshot) {
            return Err(anyhow!(
                "record {sequence} is missing valid normalized telemetry fields"
            ));
        }
        let Some(object) = snapshot.as_object_mut() else {
            return Err(anyhow!("record {sequence} is not a JSON object"));
        };
        object
            .entry("sequence")
            .or_insert_with(|| serde_json::json!(sequence));
        object.entry("timestamp_ns").or_insert_with(|| {
            serde_json::json!((sequence as u64).saturating_mul(DEFAULT_RECORD_FRAME_PERIOD_NS))
        });
        object.insert(
            "recorder_command".to_string(),
            serde_json::json!(RECORD_COMMAND),
        );
        object.insert(
            "recorder_session_id".to_string(),
            serde_json::json!(session_id),
        );
        object.insert(
            "recording_duration_ms".to_string(),
            serde_json::json!(duration_ms),
        );
        object.insert("game".to_string(), serde_json::json!(game_id));
        object.insert(
            "telemetry_source".to_string(),
            serde_json::json!(telemetry_source),
        );
        object.insert(
            "hardware_output_enabled".to_string(),
            serde_json::json!(false),
        );
        object.insert("no_ffb_writes".to_string(), serde_json::json!(true));
        object.insert(
            "no_serial_config_commands".to_string(),
            serde_json::json!(true),
        );
        object.insert(
            "no_firmware_or_dfu_commands".to_string(),
            serde_json::json!(true),
        );

        let line = serde_json::to_string(&snapshot)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()?;

    let normalized_snapshot_count =
        u64::try_from(records.len()).context("too many normalized telemetry records")?;
    let summary = RecordSummary {
        command: RECORD_COMMAND,
        game: game_id.to_string(),
        telemetry_source: telemetry_source.to_string(),
        input: input_path.to_string(),
        output: output_path.to_string(),
        recorder_session_id: session_id,
        normalized_snapshot_count,
        duration_ms,
        hardware_output_enabled: false,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Telemetry recording complete");
        println!("  game: {}", summary.game);
        println!("  telemetry_source: {}", summary.telemetry_source);
        println!("  snapshots: {}", summary.normalized_snapshot_count);
        println!("  session: {}", summary.recorder_session_id);
        println!("  output: {}", summary.output);
    }

    Ok(())
}

fn default_recorder_session_id(game_id: &str) -> String {
    let sanitized: String = game_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let elapsed_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{sanitized}-{elapsed_ns}")
}

fn read_normalized_telemetry_records(path: &str) -> Result<Vec<Value>> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("failed to read '{}'", path))?;
    let trimmed = contents.trim_start();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && let Ok(value) = serde_json::from_str::<Value>(&contents)
    {
        return normalized_records_from_value(&value);
    }

    let mut records = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSONL record {} in '{}'", line_index + 1, path))?;
        records.push(value);
    }
    Ok(records)
}

fn normalized_records_from_value(value: &Value) -> Result<Vec<Value>> {
    if let Some(array) = value.as_array() {
        return Ok(array.clone());
    }
    for key in ["frames", "records", "snapshots"] {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            return Ok(array.clone());
        }
    }
    Ok(vec![value.clone()])
}

fn normalized_telemetry_payload(record: &Value) -> Option<&Value> {
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

fn normalized_f64(record: &Value, key: &str) -> Option<f64> {
    record.get(key).and_then(Value::as_f64)
}

fn normalized_i64(record: &Value, key: &str) -> Option<i64> {
    record.get(key).and_then(Value::as_i64)
}

fn normalized_telemetry_payload_is_valid(record: &Value) -> bool {
    normalized_f64(record, "speed_ms")
        .map(|value| value.is_finite() && (0.0..=200.0).contains(&value))
        .unwrap_or(false)
        && normalized_f64(record, "steering_angle")
            .map(|value| value.is_finite() && (-40.0..=40.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "throttle")
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "brake")
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "rpm")
            .map(|value| value.is_finite() && (0.0..=30_000.0).contains(&value))
            .unwrap_or(false)
        && normalized_i64(record, "gear")
            .map(|value| (-1..=15).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "ffb_scalar")
            .map(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
            .unwrap_or(false)
}

fn ensure_probe_game(game_id: &str) -> Result<()> {
    let allowed = ["acc", "ac_rally"];
    if allowed.iter().any(|id| id == &game_id) {
        return Ok(());
    }

    Err(CliError::InvalidConfiguration(format!(
        "Telemetry probe currently supports: {}",
        allowed.join(", ")
    ))
    .into())
}

enum ProbeOutcome {
    Registration(RegistrationResult),
    Response { message_type: u8, raw_size: usize },
    Timeout,
}

#[derive(Debug)]
struct RegistrationResult {
    connection_id: i32,
    success: bool,
    readonly: bool,
    error: String,
    raw_size: usize,
}

async fn probe_once(endpoint: SocketAddr, timeout: Duration) -> Result<ProbeOutcome> {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)))
        .await
        .context("probe bind failed")?;
    socket
        .connect(endpoint)
        .await
        .context("probe connect failed")?;

    let packet = build_register_packet("OpenRacing Probe", "", Duration::from_millis(16), "")?;
    socket.send(&packet).await.context("probe send failed")?;

    let mut buf = [0u8; MAX_PACKET_SIZE];
    let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
    let len = match recv {
        Ok(Ok(len)) => len,
        Ok(Err(error)) => return Err(anyhow!("probe receive failed: {}", error)),
        Err(_) => return Ok(ProbeOutcome::Timeout),
    };

    if let Ok(result) = parse_registration_result(&buf[..len]) {
        return Ok(ProbeOutcome::Registration(RegistrationResult {
            raw_size: len,
            ..result
        }));
    }

    Ok(ProbeOutcome::Response {
        message_type: buf[0],
        raw_size: len,
    })
}

fn build_register_packet(
    display_name: &str,
    connection_password: &str,
    update_rate: Duration,
    command_password: &str,
) -> Result<Vec<u8>> {
    let interval_ms = update_rate
        .as_millis()
        .try_into()
        .unwrap_or(i32::MAX)
        .max(1);

    let mut packet = Vec::with_capacity(128);
    packet.push(REGISTER_COMMAND_APPLICATION);
    packet.push(PROTOCOL_VERSION);
    write_acc_string(&mut packet, display_name)?;
    write_acc_string(&mut packet, connection_password)?;
    packet.extend_from_slice(&interval_ms.to_le_bytes());
    write_acc_string(&mut packet, command_password)?;
    Ok(packet)
}

fn parse_registration_result(data: &[u8]) -> Result<RegistrationResult> {
    let mut reader = PacketReader::new(data);
    let message_type = reader.read_u8()?;
    if message_type != MSG_REGISTRATION_RESULT {
        return Err(anyhow!(
            "unexpected message type {message_type}, expected {MSG_REGISTRATION_RESULT}"
        ));
    }

    Ok(RegistrationResult {
        connection_id: reader.read_i32_le()?,
        success: reader.read_bool_u8()?,
        readonly: reader.read_bool_u8()?,
        error: read_acc_string(&mut reader)?,
        raw_size: data.len(),
    })
}

fn write_acc_string(buffer: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| anyhow!("probe string too long: {} bytes", bytes.len()))?;
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn read_acc_string(reader: &mut PacketReader<'_>) -> Result<String> {
    let len = usize::from(reader.read_u16_le()?);
    let raw = reader.read_exact(len)?;
    String::from_utf8(raw.to_vec()).context("probe string is not valid UTF-8")
}

struct PacketReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PacketReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| anyhow!("packet offset overflow"))?;
        if end > self.data.len() {
            return Err(anyhow!(
                "packet too short: need {len} bytes at offset {}, total {}",
                self.offset,
                self.data.len()
            ));
        }
        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_bool_u8(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_u16_le(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32_le(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn normalized_snapshot(sequence: usize) -> Value {
        serde_json::json!({
            "sequence": sequence,
            "timestamp_ns": sequence as u64 * DEFAULT_RECORD_FRAME_PERIOD_NS,
            "speed_ms": 12.5,
            "steering_angle": 0.05,
            "throttle": 0.25,
            "brake": 0.0,
            "rpm": 3200.0,
            "gear": 3,
            "ffb_scalar": 0.2
        })
    }

    fn write_normalized_jsonl(path: &Path, count: usize) -> TestResult {
        let mut lines = String::new();
        for sequence in 0..count {
            lines.push_str(&serde_json::to_string(&normalized_snapshot(sequence))?);
            lines.push('\n');
        }
        fs::write(path, lines)?;
        Ok(())
    }

    #[test]
    fn test_ensure_probe_game_accepts_acc_and_ac_rally() {
        assert!(ensure_probe_game("acc").is_ok());
        assert!(ensure_probe_game("ac_rally").is_ok());
    }

    #[test]
    fn test_ensure_probe_game_rejects_unsupported_game() {
        let result = ensure_probe_game("iracing");
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_probe_game_rejects_empty_string() {
        let result = ensure_probe_game("");
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_probe_game_error_message_lists_supported() {
        let result = ensure_probe_game("ams2");
        assert!(result.is_err());
        let msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(msg.contains("acc"));
        assert!(msg.contains("ac_rally"));
    }

    #[tokio::test]
    async fn record_normalized_snapshots_writes_moza_compatible_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 2)?;

        record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            5000,
            false,
        )
        .await?;

        let contents = fs::read_to_string(&output)?;
        let mut lines = contents.lines();
        let first_line = lines.next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        assert_eq!(
            first.get("recorder_command"),
            Some(&serde_json::json!(RECORD_COMMAND))
        );
        assert_eq!(
            first.get("recorder_session_id"),
            Some(&serde_json::json!("session-001"))
        );
        assert_eq!(first.get("game"), Some(&serde_json::json!("simhub-bridge")));
        assert_eq!(
            first.get("telemetry_source"),
            Some(&serde_json::json!("simhub_bridge"))
        );
        assert_eq!(
            first.get("hardware_output_enabled"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(first.get("no_ffb_writes"), Some(&serde_json::json!(true)));
        assert!(lines.next().is_some());
        assert!(lines.next().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_unsupported_source() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "synthetic",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            5000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn execute_record_dispatches_to_json_summary() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;
        let command = TelemetryCommands::Record {
            game: "simhub-bridge".to_string(),
            telemetry_source: "simhub_bridge".to_string(),
            input: input.to_str().ok_or("input path not UTF-8")?.to_string(),
            out: output.to_str().ok_or("output path not UTF-8")?.to_string(),
            session_id: None,
            duration_ms: 1000,
        };

        execute(&command, true).await?;

        let contents = fs::read_to_string(&output)?;
        let first_line = contents.lines().next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        let recorder_session_id = first
            .get("recorder_session_id")
            .and_then(Value::as_str)
            .ok_or("missing recorder session id")?;
        assert!(recorder_session_id.starts_with("simhub-bridge-"));
        assert_eq!(
            first.get("telemetry_source").and_then(Value::as_str),
            Some("simhub_bridge")
        );
        assert_eq!(
            first.get("no_ffb_writes").and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_empty_game() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;

        let result = record_normalized_snapshots(
            " ",
            "real_game",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            None,
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_empty_input() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("empty.jsonl");
        let output = dir.path().join("recording.jsonl");
        fs::write(&input, "\n\n")?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn normalized_record_reader_accepts_wrapped_json_shapes() -> TestResult {
        let dir = tempfile::tempdir()?;
        for (file_name, contents) in [
            (
                "array.json",
                serde_json::json!([normalized_snapshot(0), normalized_snapshot(1)]),
            ),
            (
                "frames.json",
                serde_json::json!({"frames": [normalized_snapshot(0)]}),
            ),
            (
                "records.json",
                serde_json::json!({"records": [normalized_snapshot(0)]}),
            ),
            (
                "snapshots.json",
                serde_json::json!({"snapshots": [normalized_snapshot(0)]}),
            ),
        ] {
            let path = dir.path().join(file_name);
            fs::write(&path, serde_json::to_string(&contents)?)?;
            let records =
                read_normalized_telemetry_records(path.to_str().ok_or("path not UTF-8")?)?;
            assert!(!records.is_empty());
        }
        Ok(())
    }

    #[test]
    fn normalized_record_reader_accepts_single_json_object() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("snapshot.json");
        fs::write(&path, serde_json::to_string(&normalized_snapshot(7))?)?;

        let records = read_normalized_telemetry_records(path.to_str().ok_or("path not UTF-8")?)?;

        assert_eq!(records.len(), 1);
        assert_eq!(
            records
                .first()
                .and_then(|record| record.get("gear"))
                .and_then(Value::as_i64),
            Some(3)
        );
        Ok(())
    }

    #[test]
    fn default_recorder_session_id_sanitizes_game_id() {
        let session_id = default_recorder_session_id("sim hub/bridge");

        assert!(session_id.starts_with("sim-hub-bridge-"));
    }

    #[tokio::test]
    async fn record_normalized_snapshots_inserts_sequence_and_timestamp_for_nested_payload()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("nested.jsonl");
        let output = dir.path().join("recording.jsonl");
        let mut snapshot = normalized_snapshot(0);
        if let Some(object) = snapshot.as_object_mut() {
            object.remove("sequence");
            object.remove("timestamp_ns");
        }
        fs::write(&input, serde_json::json!({"data": snapshot}).to_string())?;

        record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await?;

        let contents = fs::read_to_string(&output)?;
        let first_line = contents.lines().next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        assert_eq!(first.get("sequence").and_then(Value::as_u64), Some(0));
        assert_eq!(first.get("timestamp_ns").and_then(Value::as_u64), Some(0));
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_out_of_range_payload() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("invalid.jsonl");
        let output = dir.path().join("recording.jsonl");
        let mut snapshot = normalized_snapshot(0);
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("speed_ms".to_string(), serde_json::json!(999.0));
        }
        fs::write(&input, snapshot.to_string())?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn build_register_packet_structure() -> TestResult {
        let packet = build_register_packet("Test", "", Duration::from_millis(16), "")?;
        assert_eq!(packet[0], REGISTER_COMMAND_APPLICATION);
        assert_eq!(packet[1], PROTOCOL_VERSION);
        // display_name "Test" length = 4 as u16 LE
        assert_eq!(packet[2], 4);
        assert_eq!(packet[3], 0);
        assert_eq!(&packet[4..8], b"Test");
        Ok(())
    }

    #[test]
    fn build_register_packet_empty_name() -> TestResult {
        let packet = build_register_packet("", "", Duration::from_millis(16), "")?;
        assert_eq!(packet[0], REGISTER_COMMAND_APPLICATION);
        // name length = 0
        assert_eq!(packet[2], 0);
        assert_eq!(packet[3], 0);
        Ok(())
    }

    #[test]
    fn build_register_packet_interval_encoded() -> TestResult {
        let packet = build_register_packet("X", "", Duration::from_millis(50), "")?;
        // After header (2 bytes), display_name (2+1), connection_password (2+0)
        // interval is at offset 2 + (2+1) + (2+0) = 7
        let interval_offset = 2 + 2 + 1 + 2;
        let interval_bytes = &packet[interval_offset..interval_offset + 4];
        let interval = i32::from_le_bytes([
            interval_bytes[0],
            interval_bytes[1],
            interval_bytes[2],
            interval_bytes[3],
        ]);
        assert_eq!(interval, 50);
        Ok(())
    }

    #[test]
    fn parse_registration_result_valid() -> TestResult {
        let mut data = Vec::new();
        data.push(MSG_REGISTRATION_RESULT);
        data.extend_from_slice(&42i32.to_le_bytes());
        data.push(1); // success
        data.push(0); // readonly
        data.extend_from_slice(&0u16.to_le_bytes()); // empty error string

        let result = parse_registration_result(&data)?;
        assert_eq!(result.connection_id, 42);
        assert!(result.success);
        assert!(!result.readonly);
        assert!(result.error.is_empty());
        Ok(())
    }

    #[test]
    fn parse_registration_result_with_error_string() -> TestResult {
        let mut data = Vec::new();
        data.push(MSG_REGISTRATION_RESULT);
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(0); // not success
        data.push(0); // not readonly
        let error_msg = b"connection limit reached";
        data.extend_from_slice(&(error_msg.len() as u16).to_le_bytes());
        data.extend_from_slice(error_msg);

        let result = parse_registration_result(&data)?;
        assert_eq!(result.connection_id, -1);
        assert!(!result.success);
        assert_eq!(result.error, "connection limit reached");
        Ok(())
    }

    #[test]
    fn parse_registration_result_wrong_message_type() {
        let data = vec![255u8, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = parse_registration_result(&data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_registration_result_truncated() {
        let data = vec![MSG_REGISTRATION_RESULT, 0]; // too short
        let result = parse_registration_result(&data);
        assert!(result.is_err());
    }

    #[test]
    fn packet_reader_read_exact() -> TestResult {
        let data = [1, 2, 3, 4, 5];
        let mut reader = PacketReader::new(&data);
        let chunk = reader.read_exact(3)?;
        assert_eq!(chunk, &[1, 2, 3]);
        let chunk2 = reader.read_exact(2)?;
        assert_eq!(chunk2, &[4, 5]);
        Ok(())
    }

    #[test]
    fn packet_reader_overflow() {
        let data = [1, 2];
        let mut reader = PacketReader::new(&data);
        let result = reader.read_exact(5);
        assert!(result.is_err());
    }

    #[test]
    fn packet_reader_u16_le() -> TestResult {
        let data = [0x34, 0x12];
        let mut reader = PacketReader::new(&data);
        let val = reader.read_u16_le()?;
        assert_eq!(val, 0x1234);
        Ok(())
    }

    #[test]
    fn packet_reader_i32_le() -> TestResult {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut reader = PacketReader::new(&data);
        let val = reader.read_i32_le()?;
        assert_eq!(val, 0x12345678);
        Ok(())
    }

    #[test]
    fn packet_reader_bool_u8() -> TestResult {
        let data = [0, 1, 255];
        let mut reader = PacketReader::new(&data);
        assert!(!reader.read_bool_u8()?);
        assert!(reader.read_bool_u8()?);
        assert!(reader.read_bool_u8()?);
        Ok(())
    }

    #[test]
    fn write_and_read_acc_string_roundtrip() -> TestResult {
        let mut buf = Vec::new();
        write_acc_string(&mut buf, "hello")?;

        let mut reader = PacketReader::new(&buf);
        let result = read_acc_string(&mut reader)?;
        assert_eq!(result, "hello");
        Ok(())
    }

    #[test]
    fn write_acc_string_empty() -> TestResult {
        let mut buf = Vec::new();
        write_acc_string(&mut buf, "")?;
        assert_eq!(buf.len(), 2); // just the length prefix
        assert_eq!(buf[0], 0);
        assert_eq!(buf[1], 0);
        Ok(())
    }
}
