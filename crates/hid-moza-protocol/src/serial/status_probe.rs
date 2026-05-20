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
