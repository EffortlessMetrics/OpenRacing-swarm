use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ProbeAttempt {
    pub(super) attempt: u32,
    pub(super) status: String,
    pub(super) elapsed_ms: u64,
    pub(super) response_size: usize,
    pub(super) message_type: Option<u8>,
    pub(super) registration_connection_id: Option<i32>,
    pub(super) registration_success: Option<bool>,
    pub(super) registration_readonly: Option<bool>,
    pub(super) registration_error: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ProbeSummary {
    pub(super) game_id: String,
    pub(super) endpoint: String,
    pub(super) attempts: u32,
    pub(super) any_response: bool,
    pub(super) attempts_detail: Vec<ProbeAttempt>,
}

#[derive(Debug, Serialize)]
pub(super) struct CaptureSummary {
    pub(super) game_id: String,
    pub(super) listen: String,
    pub(super) duration_seconds: u64,
    pub(super) packets_captured: u64,
    pub(super) bytes_written: u64,
    pub(super) output: String,
}

#[derive(Debug, Serialize)]
pub(super) struct RecordSummary {
    pub(super) command: &'static str,
    pub(super) game: String,
    pub(super) telemetry_source: String,
    pub(super) input: String,
    pub(super) output: String,
    pub(super) recorder_session_id: String,
    pub(super) normalized_snapshot_count: u64,
    pub(super) duration_ms: u64,
    pub(super) hardware_output_enabled: bool,
    pub(super) no_hid_device_opened: bool,
    pub(super) no_ffb_writes: bool,
    pub(super) no_serial_config_commands: bool,
    pub(super) no_firmware_or_dfu_commands: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct LiveRecordSummary {
    pub(super) command: &'static str,
    pub(super) game: String,
    pub(super) telemetry_source: String,
    pub(super) input: String,
    pub(super) output: String,
    pub(super) recorder_session_id: String,
    pub(super) normalized_snapshot_count: u64,
    pub(super) duration_ms: u64,
    pub(super) packets_received: u64,
    pub(super) bytes_received: u64,
    pub(super) parse_errors: u64,
    pub(super) hardware_output_enabled: bool,
    pub(super) no_hid_device_opened: bool,
    pub(super) no_ffb_writes: bool,
    pub(super) no_serial_config_commands: bool,
    pub(super) no_firmware_or_dfu_commands: bool,
}
