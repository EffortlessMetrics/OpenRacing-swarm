//! Formatting and timestamp helpers for Moza JSON receipts.
//!
//! These routines are intentionally pure/lightweight so command code can share
//! canonical timestamp and hex rendering without duplicating string formatting.

use chrono::{SecondsFormat, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

mod timestamp {
    use super::*;

    pub(super) fn now_utc() -> String {
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    pub(super) fn utc_timestamp_pair_is_ordered(start: &str, end: &str) -> bool {
        if !start.ends_with('Z') || !end.ends_with('Z') {
            return false;
        }
        let Ok(start) = chrono::DateTime::parse_from_rfc3339(start) else {
            return false;
        };
        let Ok(end) = chrono::DateTime::parse_from_rfc3339(end) else {
            return false;
        };
        start <= end
    }

    pub(super) fn unix_now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}

mod hex {
    pub(super) fn hex_u16(value: u16) -> String {
        format!("0x{value:04X}")
    }

    pub(super) fn hex_u8(value: u8) -> String {
        format!("0x{value:02X}")
    }

    pub(super) fn bytes_hex_compact(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    pub(super) fn bytes_hex_array(bytes: &[u8]) -> Vec<String> {
        bytes.iter().map(|b| hex_u8(*b)).collect()
    }
}

pub(super) use hex::{bytes_hex_array, bytes_hex_compact, hex_u8, hex_u16};
pub(super) use timestamp::{now_utc, unix_now_ns, utc_timestamp_pair_is_ordered};
