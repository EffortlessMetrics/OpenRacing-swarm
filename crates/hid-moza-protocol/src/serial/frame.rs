//! Fixture-only Moza serial frame decoder.
//!
//! This module validates checked-in fixture bytes against the semantic command
//! registry. It does not encode frames, open serial devices, send queries, or
//! authorize hardware writes.

use crate::serial::vendor_authority::{
    MozaSerialCodecStatus, MozaVendorCommand, command_by_group_command,
};
use std::fmt;

pub const MESSAGE_START: u8 = 0x7e;
pub const CHECKSUM_MAGIC: u8 = 13;
pub const MIN_FRAME_LEN: usize = 6;
pub const SERIAL_FIXTURE_CODEC_STATUS: MozaSerialCodecStatus =
    MozaSerialCodecStatus::FixtureDecodeOnly;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MozaSerialFrameError {
    TooShort {
        actual_len: usize,
    },
    BadStart {
        actual: u8,
    },
    LengthMismatch {
        declared_len: usize,
        expected_len: usize,
        actual_len: usize,
    },
    MissingCommandId,
    ChecksumMismatch {
        expected: u8,
        actual: u8,
    },
    UnknownCommand {
        group: u8,
        command: u8,
    },
}

impl fmt::Display for MozaSerialFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual_len } => {
                write!(formatter, "serial frame too short: {actual_len} bytes")
            }
            Self::BadStart { actual } => {
                write!(formatter, "invalid serial frame start byte: 0x{actual:02X}")
            }
            Self::LengthMismatch {
                declared_len,
                expected_len,
                actual_len,
            } => write!(
                formatter,
                "serial frame length mismatch: declared {declared_len}, expected {expected_len} bytes, got {actual_len}"
            ),
            Self::MissingCommandId => write!(formatter, "serial frame is missing a command id"),
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "serial frame checksum mismatch: expected 0x{expected:02X}, got 0x{actual:02X}"
            ),
            Self::UnknownCommand { group, command } => write!(
                formatter,
                "unknown serial command tuple: group 0x{group:02X}, command 0x{command:02X}"
            ),
        }
    }
}

impl std::error::Error for MozaSerialFrameError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MozaSerialObservedFrame<'a> {
    pub group: u8,
    pub device_id: u8,
    pub command_id: u8,
    pub payload: &'a [u8],
    pub checksum: u8,
    pub command: Option<&'static MozaVendorCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MozaSerialDecodedFrame<'a> {
    pub group: u8,
    pub device_id: u8,
    pub command_id: u8,
    pub payload: &'a [u8],
    pub checksum: u8,
    pub command: &'static MozaVendorCommand,
}

pub fn serial_checksum(frame_without_checksum: &[u8]) -> u8 {
    frame_without_checksum
        .iter()
        .fold(CHECKSUM_MAGIC, |sum, byte| sum.wrapping_add(*byte))
}

pub fn decode_observed_frame_shape(
    frame: &[u8],
) -> Result<MozaSerialObservedFrame<'_>, MozaSerialFrameError> {
    if frame.len() < MIN_FRAME_LEN {
        return Err(MozaSerialFrameError::TooShort {
            actual_len: frame.len(),
        });
    }

    if frame[0] != MESSAGE_START {
        return Err(MozaSerialFrameError::BadStart { actual: frame[0] });
    }

    let declared_len = usize::from(frame[1]);
    if declared_len == 0 {
        return Err(MozaSerialFrameError::MissingCommandId);
    }

    let expected_len = 5 + declared_len;
    if frame.len() != expected_len {
        return Err(MozaSerialFrameError::LengthMismatch {
            declared_len,
            expected_len,
            actual_len: frame.len(),
        });
    }

    let checksum_index = expected_len - 1;
    let expected_checksum = serial_checksum(&frame[..checksum_index]);
    let actual_checksum = frame[checksum_index];
    if expected_checksum != actual_checksum {
        return Err(MozaSerialFrameError::ChecksumMismatch {
            expected: expected_checksum,
            actual: actual_checksum,
        });
    }

    let group = frame[2];
    let device_id = frame[3];
    let command_id = frame[4];
    let payload_end = 4 + declared_len;
    let payload = &frame[5..payload_end];
    let command = command_by_group_command(group, command_id);

    Ok(MozaSerialObservedFrame {
        group,
        device_id,
        command_id,
        payload,
        checksum: actual_checksum,
        command,
    })
}

pub fn decode_fixture_frame(
    frame: &[u8],
) -> Result<MozaSerialDecodedFrame<'_>, MozaSerialFrameError> {
    let observed = decode_observed_frame_shape(frame)?;
    let command = observed
        .command
        .ok_or(MozaSerialFrameError::UnknownCommand {
            group: observed.group,
            command: observed.command_id,
        })?;

    Ok(MozaSerialDecodedFrame {
        group: observed.group,
        device_id: observed.device_id,
        command_id: observed.command_id,
        payload: observed.payload,
        checksum: observed.checksum,
        command,
    })
}
