//! Read-only Moza vendor status query framing.
//!
//! This module encodes only status-query frames that are explicitly allowed by
//! the vendor command registry. It does not encode output, configuration,
//! firmware, DFU, or unknown host-to-device commands.

use crate::serial::frame::{
    MESSAGE_START, MozaSerialDecodedFrame, MozaSerialFrameError, decode_fixture_frame,
    serial_checksum,
};
use crate::serial::vendor_authority::{
    MozaRiskClass, MozaSerialCodecStatus, MozaVendorCommand, REQUIRED_VENDOR_COMMANDS,
};
use std::fmt;

pub const READ_ONLY_STATUS_CODEC_STATUS: MozaSerialCodecStatus =
    MozaSerialCodecStatus::RoundTripVerified;
const DEBUG_LOG_GROUP: u8 = 0x0e;
const DEBUG_LOG_DEVICE_ID: u8 = 0x71;
const DEBUG_LOG_COMMAND_ID: u8 = 0x05;
const NRFLOSS_MARKER: &[u8] = b"NRFloss";
const RECV_GAP_MARKER: &[u8] = b"recvGap";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MozaReadOnlyStatusProbeError {
    NotReadOnlyStatusProbeAllowed {
        command_id: &'static str,
        risk_class: MozaRiskClass,
    },
    ResponseFrame(MozaSerialFrameError),
    ResponseCommandMismatch {
        expected_command_id: &'static str,
        actual_command_id: &'static str,
    },
    ResponseDeviceMismatch {
        expected_device_id: u8,
        actual_device_id: u8,
    },
}

impl fmt::Display for MozaReadOnlyStatusProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReadOnlyStatusProbeAllowed {
                command_id,
                risk_class,
            } => write!(
                formatter,
                "command `{command_id}` is {risk_class:?} and is not read-only status probe eligible"
            ),
            Self::ResponseFrame(error) => write!(formatter, "{error}"),
            Self::ResponseCommandMismatch {
                expected_command_id,
                actual_command_id,
            } => write!(
                formatter,
                "status response command mismatch: expected `{expected_command_id}`, got `{actual_command_id}`"
            ),
            Self::ResponseDeviceMismatch {
                expected_device_id,
                actual_device_id,
            } => write!(
                formatter,
                "status response device mismatch: expected 0x{expected_device_id:02X}, got 0x{actual_device_id:02X}"
            ),
        }
    }
}

impl std::error::Error for MozaReadOnlyStatusProbeError {}

impl From<MozaSerialFrameError> for MozaReadOnlyStatusProbeError {
    fn from(error: MozaSerialFrameError) -> Self {
        Self::ResponseFrame(error)
    }
}

pub fn read_only_status_commands() -> impl Iterator<Item = &'static MozaVendorCommand> {
    REQUIRED_VENDOR_COMMANDS
        .iter()
        .filter(|command| command.risk_class == MozaRiskClass::VendorStatus)
        .filter(|command| command.read_only_status_probe_allowed)
}

pub fn encode_read_only_status_query(
    command: &'static MozaVendorCommand,
) -> Result<Vec<u8>, MozaReadOnlyStatusProbeError> {
    ensure_read_only_status_allowed(command)?;

    let mut frame = vec![
        MESSAGE_START,
        1,
        command.group,
        command.device_id,
        command.command,
    ];
    let checksum = serial_checksum(&frame);
    frame.push(checksum);
    Ok(frame)
}

pub fn decode_read_only_status_response<'a>(
    expected_command: &'static MozaVendorCommand,
    frame: &'a [u8],
) -> Result<MozaSerialDecodedFrame<'a>, MozaReadOnlyStatusProbeError> {
    ensure_read_only_status_allowed(expected_command)?;
    let decoded = decode_fixture_frame(frame)?;
    if decoded.command.id != expected_command.id {
        return Err(MozaReadOnlyStatusProbeError::ResponseCommandMismatch {
            expected_command_id: expected_command.id,
            actual_command_id: decoded.command.id,
        });
    }
    if decoded.device_id != expected_command.device_id {
        return Err(MozaReadOnlyStatusProbeError::ResponseDeviceMismatch {
            expected_device_id: expected_command.device_id,
            actual_device_id: decoded.device_id,
        });
    }
    Ok(decoded)
}

pub fn ensure_read_only_status_allowed(
    command: &'static MozaVendorCommand,
) -> Result<(), MozaReadOnlyStatusProbeError> {
    if command.risk_class == MozaRiskClass::VendorStatus && command.read_only_status_probe_allowed {
        return Ok(());
    }

    Err(
        MozaReadOnlyStatusProbeError::NotReadOnlyStatusProbeAllowed {
            command_id: command.id,
            risk_class: command.risk_class,
        },
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MozaReadOnlyStatusResponseFrameClass {
    RegistryStatusResponse,
    FramedAsciiTelemetryLog,
    StreamDesynchronizedOrPartialLogFrame,
    UnknownNonRegistryFrame,
    MalformedFrame,
}

impl MozaReadOnlyStatusResponseFrameClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegistryStatusResponse => "registry_status_response",
            Self::FramedAsciiTelemetryLog => "framed_ascii_telemetry_log",
            Self::StreamDesynchronizedOrPartialLogFrame => {
                "stream_desynchronized_or_partial_log_frame"
            }
            Self::UnknownNonRegistryFrame => "unknown_non_registry_frame",
            Self::MalformedFrame => "malformed_frame",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MozaReadOnlyStatusResponseFrameDiagnosis {
    pub classification: MozaReadOnlyStatusResponseFrameClass,
    pub actual_len: usize,
    pub declared_len: Option<usize>,
    pub expected_len: Option<usize>,
    pub length_matches: bool,
    pub checksum_valid: Option<bool>,
    pub expected_checksum: Option<u8>,
    pub actual_checksum: Option<u8>,
    pub group: Option<u8>,
    pub device_id: Option<u8>,
    pub command: Option<u8>,
    pub registry_command_known: bool,
    pub payload_len: Option<usize>,
    pub printable_ascii_payload: bool,
    pub nrfloss_recv_gap_payload: bool,
    pub embedded_start_byte_count: usize,
}

pub fn diagnose_read_only_status_response_frame(
    frame: &[u8],
) -> MozaReadOnlyStatusResponseFrameDiagnosis {
    let actual_len = frame.len();
    if actual_len < 2 || frame.first().copied() != Some(MESSAGE_START) {
        return MozaReadOnlyStatusResponseFrameDiagnosis {
            classification: MozaReadOnlyStatusResponseFrameClass::MalformedFrame,
            actual_len,
            declared_len: frame.get(1).map(|byte| usize::from(*byte)),
            expected_len: None,
            length_matches: false,
            checksum_valid: None,
            expected_checksum: None,
            actual_checksum: None,
            group: frame.get(2).copied(),
            device_id: frame.get(3).copied(),
            command: frame.get(4).copied(),
            registry_command_known: false,
            payload_len: None,
            printable_ascii_payload: false,
            nrfloss_recv_gap_payload: false,
            embedded_start_byte_count: count_embedded_start_bytes(frame),
        };
    }

    let declared_len = usize::from(frame[1]);
    let expected_len = Some(5 + declared_len);
    let length_matches = expected_len == Some(actual_len);
    let group = frame.get(2).copied();
    let device_id = frame.get(3).copied();
    let command = frame.get(4).copied();
    let registry_command_known = group
        .zip(command)
        .and_then(|(group, command)| {
            REQUIRED_VENDOR_COMMANDS
                .iter()
                .find(|candidate| candidate.group == group && candidate.command == command)
        })
        .is_some();

    let mut expected_checksum = None;
    let actual_checksum = frame.last().copied();
    let mut checksum_valid = None;
    let payload = if length_matches && actual_len >= 6 {
        expected_checksum = Some(serial_checksum(&frame[..actual_len - 1]));
        checksum_valid = expected_checksum
            .zip(actual_checksum)
            .map(|(expected, actual)| expected == actual);
        Some(&frame[5..actual_len - 1])
    } else {
        None
    };

    let printable_ascii_payload = payload.is_some_and(payload_is_printable_ascii_or_newline);
    let nrfloss_recv_gap_payload = payload.is_some_and(payload_has_nrfloss_recv_gap);
    let embedded_start_byte_count = count_embedded_start_bytes(frame);
    let is_debug_log_tuple = group == Some(DEBUG_LOG_GROUP)
        && device_id == Some(DEBUG_LOG_DEVICE_ID)
        && command == Some(DEBUG_LOG_COMMAND_ID);

    let classification = if !length_matches {
        MozaReadOnlyStatusResponseFrameClass::MalformedFrame
    } else if registry_command_known && checksum_valid == Some(true) {
        MozaReadOnlyStatusResponseFrameClass::RegistryStatusResponse
    } else if is_debug_log_tuple && nrfloss_recv_gap_payload && checksum_valid == Some(true) {
        MozaReadOnlyStatusResponseFrameClass::FramedAsciiTelemetryLog
    } else if embedded_start_byte_count > 0
        || (is_debug_log_tuple && payload.is_some())
        || checksum_valid == Some(false)
    {
        MozaReadOnlyStatusResponseFrameClass::StreamDesynchronizedOrPartialLogFrame
    } else {
        MozaReadOnlyStatusResponseFrameClass::UnknownNonRegistryFrame
    };

    MozaReadOnlyStatusResponseFrameDiagnosis {
        classification,
        actual_len,
        declared_len: Some(declared_len),
        expected_len,
        length_matches,
        checksum_valid,
        expected_checksum,
        actual_checksum,
        group,
        device_id,
        command,
        registry_command_known,
        payload_len: payload.map(|payload| payload.len()),
        printable_ascii_payload,
        nrfloss_recv_gap_payload,
        embedded_start_byte_count,
    }
}

fn count_embedded_start_bytes(frame: &[u8]) -> usize {
    frame
        .iter()
        .skip(1)
        .filter(|byte| **byte == MESSAGE_START)
        .count()
}

fn payload_is_printable_ascii_or_newline(payload: &[u8]) -> bool {
    !payload.is_empty()
        && payload
            .iter()
            .all(|byte| matches!(*byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e))
}

fn payload_has_nrfloss_recv_gap(payload: &[u8]) -> bool {
    bytes_contains(payload, NRFLOSS_MARKER) && bytes_contains(payload, RECV_GAP_MARKER)
}

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}
