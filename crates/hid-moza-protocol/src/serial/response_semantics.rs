//! Fixture-only review for correlated passive Moza response frames.
//!
//! This module decodes observed device-to-host response frame shapes from
//! checked-in passive evidence. It does not register commands, encode frames,
//! open devices, send queries, or authorize writes.

use crate::serial::frame::{
    MozaSerialFrameError, MozaSerialObservedFrame, decode_observed_frame_shape,
};
use crate::serial::vendor_authority::MozaRiskClass;
use std::fmt;

pub const STATUS_MODE_TRIAD_RESPONSE_GROUP_ID: &str = "passive_status_mode_triad_0x25_0x19";
pub const SESSION_AUTHORITY_PAIR_RESPONSE_GROUP_ID: &str =
    "passive_session_authority_pair_0x5a_0x5d";

const STATUS_MODE_TRIAD_RESPONSE_SEMANTICS: &[&str] = &[
    "status_mode_response_shape_question",
    "standard_pidff_mode_state_question",
    "game_control_mode_state_question",
];
const SESSION_AUTHORITY_RESPONSE_SEMANTICS: &[&str] = &[
    "session_authority_response_shape_question",
    "volatile_ffb_session_state_question",
    "authority_keepalive_response_question",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MozaPassiveResponseSemanticError {
    Frame(MozaSerialFrameError),
    UnreviewedResponseTuple {
        group: u8,
        device_id: u8,
        command: u8,
    },
}

impl fmt::Display for MozaPassiveResponseSemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(error) => write!(formatter, "{error}"),
            Self::UnreviewedResponseTuple {
                group,
                device_id,
                command,
            } => write!(
                formatter,
                "unreviewed passive response tuple 0x{group:02X}/0x{device_id:02X}/0x{command:02X}"
            ),
        }
    }
}

impl std::error::Error for MozaPassiveResponseSemanticError {}

impl From<MozaSerialFrameError> for MozaPassiveResponseSemanticError {
    fn from(error: MozaSerialFrameError) -> Self {
        Self::Frame(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MozaPassiveResponsePayloadClass {
    Empty,
    ZeroFilled,
    NonZero,
}

impl MozaPassiveResponsePayloadClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::ZeroFilled => "zero_filled",
            Self::NonZero => "nonzero",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MozaPassiveResponseSemanticObservation {
    pub group_id: &'static str,
    pub tuple_id: String,
    pub group: u8,
    pub device_id: u8,
    pub command: u8,
    pub payload_len: usize,
    pub payload_class: MozaPassiveResponsePayloadClass,
    pub checksum: u8,
    pub candidate_semantics: &'static [&'static str],
    pub risk_class: MozaRiskClass,
    pub fixture_decoder_coverage: bool,
    pub payload_variation_observed: bool,
    pub semantic_decode_claim: bool,
    pub registry_promotion_claim: bool,
    pub read_only_probe_allowed: bool,
    pub corrected_read_only_probe_ready: bool,
    pub hardware_output_authorized: bool,
    pub native_control_evidence: bool,
    pub output_sendability_claim: bool,
}

pub fn decode_passive_response_semantic_fixture(
    frame: &[u8],
) -> Result<MozaPassiveResponseSemanticObservation, MozaPassiveResponseSemanticError> {
    let observed = decode_observed_frame_shape(frame)?;
    let group_id = passive_response_group_id(&observed).ok_or(
        MozaPassiveResponseSemanticError::UnreviewedResponseTuple {
            group: observed.group,
            device_id: observed.device_id,
            command: observed.command_id,
        },
    )?;
    let candidate_semantics = match group_id {
        STATUS_MODE_TRIAD_RESPONSE_GROUP_ID => STATUS_MODE_TRIAD_RESPONSE_SEMANTICS,
        SESSION_AUTHORITY_PAIR_RESPONSE_GROUP_ID => SESSION_AUTHORITY_RESPONSE_SEMANTICS,
        _ => &[],
    };

    Ok(MozaPassiveResponseSemanticObservation {
        group_id,
        tuple_id: format!(
            "0x{:02X}/0x{:02X}/0x{:02X}",
            observed.group, observed.device_id, observed.command_id
        ),
        group: observed.group,
        device_id: observed.device_id,
        command: observed.command_id,
        payload_len: observed.payload.len(),
        payload_class: classify_payload(observed.payload),
        checksum: observed.checksum,
        candidate_semantics,
        risk_class: MozaRiskClass::UnknownDoNotSend,
        fixture_decoder_coverage: true,
        payload_variation_observed: false,
        semantic_decode_claim: false,
        registry_promotion_claim: false,
        read_only_probe_allowed: false,
        corrected_read_only_probe_ready: false,
        hardware_output_authorized: false,
        native_control_evidence: false,
        output_sendability_claim: false,
    })
}

fn passive_response_group_id(observed: &MozaSerialObservedFrame<'_>) -> Option<&'static str> {
    match (observed.group, observed.device_id, observed.command_id) {
        (0xA5, 0x91, 0x01..=0x03) => Some(STATUS_MODE_TRIAD_RESPONSE_GROUP_ID),
        (0xDA, 0xB1, 0x00) | (0xDD, 0xB1, 0x01) => Some(SESSION_AUTHORITY_PAIR_RESPONSE_GROUP_ID),
        _ => None,
    }
}

fn classify_payload(payload: &[u8]) -> MozaPassiveResponsePayloadClass {
    if payload.is_empty() {
        MozaPassiveResponsePayloadClass::Empty
    } else if payload.iter().all(|byte| *byte == 0) {
        MozaPassiveResponsePayloadClass::ZeroFilled
    } else {
        MozaPassiveResponsePayloadClass::NonZero
    }
}
