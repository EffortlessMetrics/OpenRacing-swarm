//! Software-only Moza serial fake transport.
//!
//! This module replays checked-in fixture bytes through the serial decoder and
//! records deterministic synthetic exchanges. It does not open serial devices,
//! send queries, encode hardware frames, or authorize writes.

use crate::serial::frame::{MozaSerialFrameError, decode_fixture_frame};
use crate::serial::vendor_authority::{MozaRiskClass, MozaSerialCodecStatus, MozaVendorCommand};
use std::fmt;

pub const FAKE_TRANSPORT_CODEC_STATUS: MozaSerialCodecStatus =
    MozaSerialCodecStatus::RoundTripVerified;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MozaFakeSerialTransportError {
    Frame(MozaSerialFrameError),
    AuthorizationRequired {
        command_id: &'static str,
        risk_class: MozaRiskClass,
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

#[derive(Default, Debug)]
pub struct MozaFakeSerialTransport {
    exchanges: Vec<MozaFakeSerialExchange>,
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

    pub fn exchanges(&self) -> &[MozaFakeSerialExchange] {
        &self.exchanges
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
