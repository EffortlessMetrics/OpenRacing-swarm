//! Software-only Moza serial fake transport.
//!
//! This module replays checked-in fixture bytes through the serial decoder and
//! records deterministic synthetic exchanges. It does not open serial devices,
//! send queries, encode hardware frames, or authorize writes.

use crate::serial::frame::{
    MozaSerialFrameError, decode_fixture_frame, decode_observed_frame_shape,
};
use crate::serial::vendor_authority::{MozaRiskClass, MozaSerialCodecStatus, MozaVendorCommand};
use std::fmt;

pub const FAKE_TRANSPORT_CODEC_STATUS: MozaSerialCodecStatus =
    MozaSerialCodecStatus::RoundTripVerified;

const SESSION_OR_STATUS_KEEPALIVE_SEMANTICS: &[&str] =
    &["authority_keepalive", "volatile_ffb_session_enable"];
const BASE_STATUS_OR_MODE_POLL_SEMANTICS: &[&str] = &[
    "status_query",
    "standard_pidff_mode_enable",
    "game_control_mode_select",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MozaFakeSerialTransportError {
    Frame(MozaSerialFrameError),
    AuthorizationRequired {
        command_id: &'static str,
        risk_class: MozaRiskClass,
    },
    ModeEnableCandidateNotReviewed {
        group: u8,
        device_id: u8,
        command: u8,
    },
}

impl fmt::Display for MozaFakeSerialTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(error) => write!(formatter, "{error}"),
            Self::AuthorizationRequired {
                command_id,
                risk_class,
            } => write!(
                formatter,
                "fake serial transport refused `{command_id}` because {risk_class:?} requires a later authorization stage"
            ),
            Self::ModeEnableCandidateNotReviewed {
                group,
                device_id,
                command,
            } => write!(
                formatter,
                "fake serial transport refused unreviewed mode/enable candidate tuple 0x{group:02X}/0x{device_id:02X}/0x{command:02X}"
            ),
        }
    }
}

impl std::error::Error for MozaFakeSerialTransportError {}

impl From<MozaSerialFrameError> for MozaFakeSerialTransportError {
    fn from(error: MozaSerialFrameError) -> Self {
        Self::Frame(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MozaFakeSerialExchange {
    pub command_id: &'static str,
    pub command_name: &'static str,
    pub group: u8,
    pub device_id: u8,
    pub command: u8,
    pub risk_class: MozaRiskClass,
    pub synthetic_response_payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MozaFakeModeEnableCandidateObservation {
    pub candidate_id: &'static str,
    pub semantic_hypothesis: &'static str,
    pub candidate_semantics: &'static [&'static str],
    pub tuple_id: String,
    pub group: u8,
    pub device_id: u8,
    pub command: u8,
    pub payload_len: usize,
    pub checksum: u8,
    pub risk_class: MozaRiskClass,
    pub semantic_decode_claim: bool,
    pub registry_promotion_claim: bool,
    pub hardware_output_authorized: bool,
    pub native_control_evidence: bool,
    pub output_sendability_claim: bool,
}

#[derive(Default, Debug)]
pub struct MozaFakeSerialTransport {
    exchanges: Vec<MozaFakeSerialExchange>,
    mode_enable_candidate_observations: Vec<MozaFakeModeEnableCandidateObservation>,
}

impl MozaFakeSerialTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_read_only_fixture_frame(
        &mut self,
        frame: &[u8],
    ) -> Result<&MozaFakeSerialExchange, MozaFakeSerialTransportError> {
        let decoded = decode_fixture_frame(frame)?;
        ensure_read_only_fake_allowed(decoded.command)?;

        let exchange = MozaFakeSerialExchange {
            command_id: decoded.command.id,
            command_name: decoded.command.name,
            group: decoded.group,
            device_id: decoded.device_id,
            command: decoded.command_id,
            risk_class: decoded.command.risk_class,
            synthetic_response_payload: vec![0],
        };
        self.exchanges.push(exchange);

        let exchange_index = self.exchanges.len() - 1;
        Ok(&self.exchanges[exchange_index])
    }

    pub fn observe_mode_enable_candidate_fixture_frame(
        &mut self,
        frame: &[u8],
    ) -> Result<&MozaFakeModeEnableCandidateObservation, MozaFakeSerialTransportError> {
        let observed = decode_observed_frame_shape(frame)?;
        let route =
            mode_enable_candidate_route(observed.group, observed.device_id, observed.command_id)
                .ok_or(
                    MozaFakeSerialTransportError::ModeEnableCandidateNotReviewed {
                        group: observed.group,
                        device_id: observed.device_id,
                        command: observed.command_id,
                    },
                )?;

        let observation = MozaFakeModeEnableCandidateObservation {
            candidate_id: route.candidate_id,
            semantic_hypothesis: route.semantic_hypothesis,
            candidate_semantics: route.candidate_semantics,
            tuple_id: format!(
                "0x{:02X}/0x{:02X}/0x{:02X}",
                observed.group, observed.device_id, observed.command_id
            ),
            group: observed.group,
            device_id: observed.device_id,
            command: observed.command_id,
            payload_len: observed.payload.len(),
            checksum: observed.checksum,
            risk_class: MozaRiskClass::UnknownDoNotSend,
            semantic_decode_claim: false,
            registry_promotion_claim: false,
            hardware_output_authorized: false,
            native_control_evidence: false,
            output_sendability_claim: false,
        };
        self.mode_enable_candidate_observations.push(observation);

        let observation_index = self.mode_enable_candidate_observations.len() - 1;
        Ok(&self.mode_enable_candidate_observations[observation_index])
    }

    pub fn exchanges(&self) -> &[MozaFakeSerialExchange] {
        &self.exchanges
    }

    pub fn mode_enable_candidate_observations(&self) -> &[MozaFakeModeEnableCandidateObservation] {
        &self.mode_enable_candidate_observations
    }
}

fn ensure_read_only_fake_allowed(
    command: &'static MozaVendorCommand,
) -> Result<(), MozaFakeSerialTransportError> {
    if command.risk_class == MozaRiskClass::VendorStatus && command.read_only_status_probe_allowed {
        return Ok(());
    }

    Err(MozaFakeSerialTransportError::AuthorizationRequired {
        command_id: command.id,
        risk_class: command.risk_class,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModeEnableCandidateRoute {
    candidate_id: &'static str,
    semantic_hypothesis: &'static str,
    candidate_semantics: &'static [&'static str],
}

fn mode_enable_candidate_route(
    group: u8,
    device_id: u8,
    command: u8,
) -> Option<ModeEnableCandidateRoute> {
    match (group, device_id, command) {
        (0x25, 0x19, 0x01..=0x03) => Some(ModeEnableCandidateRoute {
            candidate_id: "base_status_or_mode_poll_candidate",
            semantic_hypothesis: "base_status_or_mode_poll_candidate",
            candidate_semantics: BASE_STATUS_OR_MODE_POLL_SEMANTICS,
        }),
        (0x5A, 0x1B, 0x00) | (0x5D, 0x1B, 0x01) => Some(ModeEnableCandidateRoute {
            candidate_id: "session_or_status_keepalive_candidate",
            semantic_hypothesis: "session_or_status_keepalive_candidate",
            candidate_semantics: SESSION_OR_STATUS_KEEPALIVE_SEMANTICS,
        }),
        _ => None,
    }
}
