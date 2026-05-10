//! Structured capture format for USB HID reports.
//!
//! Enables protocol reverse-engineering and validation **without** physical
//! hardware by recording, serialising, and replaying USB HID traffic.
//!
//! # Format overview
//!
//! A [`CaptureSession`] contains device metadata, optional free-form notes, and
//! a sequence of [`CaptureRecord`]s.  Each record carries a nanosecond
//! timestamp, transfer direction, report ID, and raw payload bytes.
//!
//! Both JSON (via `serde_json`) and in-memory round-trips are supported.

#![deny(static_mut_refs)]

mod fixture;
pub mod replay;
mod synthetic;

use serde::{Deserialize, Serialize};

pub use fixture::{
    CaptureEvidenceSource, CaptureFixtureMetadata, CaptureFixtureMetadataError, CaptureKind,
};

// ── Core types ───────────────────────────────────────────────────────────────

/// Transfer direction of a USB HID report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Host → Device (OUT / SET_REPORT).
    HostToDevice,
    /// Device → Host (IN / interrupt transfer).
    DeviceToHost,
}

/// USB device identity (vendor + product IDs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceId {
    /// USB Vendor ID.
    pub vid: u16,
    /// USB Product ID.
    pub pid: u16,
    /// Optional human-readable device name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A single captured USB HID report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRecord {
    /// Monotonically increasing timestamp in nanoseconds since capture start.
    pub timestamp_ns: u64,
    /// Transfer direction.
    pub direction: Direction,
    /// HID report ID (first byte of the report, or 0 if not present).
    pub report_id: u8,
    /// Raw payload bytes (excluding report ID if it was already parsed out).
    pub payload: Vec<u8>,
}

/// Metadata about how / when the capture was produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureMetadata {
    /// Format version string (currently `"1.0"`).
    pub format_version: String,
    /// ISO-8601 timestamp of capture (or `"synthetic"` for generated data).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    /// Platform that produced the capture (`"windows"`, `"linux"`, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Freeform tool / version identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Set to `true` for machine-generated captures that do not originate from
    /// real hardware.
    #[serde(default)]
    pub synthetic: bool,
    /// Optional shared fixture metadata used by parser replay and validation
    /// lanes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture: Option<CaptureFixtureMetadata>,
}

/// A complete capture session with device identity, metadata, and records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSession {
    /// Device that was captured.
    pub device: DeviceId,
    /// Capture metadata.
    pub metadata: CaptureMetadata,
    /// Ordered sequence of captured reports.
    pub records: Vec<CaptureRecord>,
}

// ── Errors ───────────────────────────────────────────────────────────────────

/// Errors that can occur when working with capture data.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// JSON (de)serialisation failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Timestamps are not monotonically increasing.
    #[error("non-monotonic timestamp at record index {index}")]
    NonMonotonicTimestamp {
        /// Index of the first out-of-order record.
        index: usize,
    },

    /// Format version is not supported.
    #[error("unsupported format version: {0}")]
    UnsupportedVersion(String),

    /// Capture fixture metadata is structurally invalid.
    #[error("invalid fixture metadata: {0}")]
    FixtureMetadata(#[from] CaptureFixtureMetadataError),
}

// ── Constructors ─────────────────────────────────────────────────────────────

impl CaptureMetadata {
    /// Create metadata for a synthetic (non-hardware) capture.
    #[must_use]
    pub fn synthetic(description: &str) -> Self {
        Self {
            format_version: "1.0".to_owned(),
            captured_at: None,
            platform: None,
            tool: Some("openracing-capture-format".to_owned()),
            description: Some(description.to_owned()),
            synthetic: true,
            fixture: None,
        }
    }

    /// Attach shared fixture metadata to this capture.
    #[must_use]
    pub fn with_fixture(mut self, fixture: CaptureFixtureMetadata) -> Self {
        self.fixture = Some(fixture);
        self
    }
}

impl CaptureSession {
    /// Serialise the session to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, CaptureError> {
        serde_json::to_string_pretty(self).map_err(CaptureError::Json)
    }

    /// Deserialise a session from JSON.
    pub fn from_json(json: &str) -> Result<Self, CaptureError> {
        let session: Self = serde_json::from_str(json)?;
        if session.metadata.format_version != "1.0" {
            return Err(CaptureError::UnsupportedVersion(
                session.metadata.format_version.clone(),
            ));
        }
        if let Some(fixture) = &session.metadata.fixture {
            fixture.validate()?;
        }
        Ok(session)
    }

    /// Validate that record timestamps are monotonically increasing.
    pub fn validate_timestamps(&self) -> Result<(), CaptureError> {
        if let Some(pos) = self
            .records
            .windows(2)
            .position(|w| w[1].timestamp_ns < w[0].timestamp_ns)
        {
            return Err(CaptureError::NonMonotonicTimestamp { index: pos + 1 });
        }
        Ok(())
    }

    /// Return only records that flow in `direction`.
    #[must_use]
    pub fn filter_direction(&self, direction: Direction) -> Vec<&CaptureRecord> {
        self.records
            .iter()
            .filter(|r| r.direction == direction)
            .collect()
    }

    /// Return only records with a specific `report_id`.
    #[must_use]
    pub fn filter_report_id(&self, report_id: u8) -> Vec<&CaptureRecord> {
        self.records
            .iter()
            .filter(|r| r.report_id == report_id)
            .collect()
    }

    /// Total capture duration in nanoseconds (0 if fewer than 2 records).
    #[must_use]
    pub fn duration_ns(&self) -> u64 {
        match (self.records.first(), self.records.last()) {
            (Some(first), Some(last)) => last.timestamp_ns.saturating_sub(first.timestamp_ns),
            _ => 0,
        }
    }
}

// ── Synthetic capture builders (re-exported from internal module) ─────────

pub use synthetic::build_synthetic_session;
pub use synthetic::supported_vendors;

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session() -> CaptureSession {
        CaptureSession {
            device: DeviceId {
                vid: 0x346E,
                pid: 0x0001,
                name: Some("Test Device".to_owned()),
            },
            metadata: CaptureMetadata::synthetic("unit test"),
            records: vec![
                CaptureRecord {
                    timestamp_ns: 0,
                    direction: Direction::DeviceToHost,
                    report_id: 0x01,
                    payload: vec![0u8; 16],
                },
                CaptureRecord {
                    timestamp_ns: 1_000_000,
                    direction: Direction::DeviceToHost,
                    report_id: 0x01,
                    payload: vec![0xAA; 16],
                },
                CaptureRecord {
                    timestamp_ns: 2_000_000,
                    direction: Direction::HostToDevice,
                    report_id: 0x20,
                    payload: vec![0x55; 8],
                },
            ],
        }
    }

    #[test]
    fn json_roundtrip() -> Result<(), CaptureError> {
        let session = sample_session();
        let json = session.to_json()?;
        let restored = CaptureSession::from_json(&json)?;
        assert_eq!(session, restored);
        Ok(())
    }

    #[test]
    fn session_with_fixture_metadata_roundtrips() -> Result<(), CaptureError> {
        let mut session = sample_session();
        session.metadata = session.metadata.with_fixture(
            CaptureFixtureMetadata::new(
                0x346E,
                0x0014,
                CaptureKind::Idle,
                CaptureEvidenceSource::Real,
            )
            .with_report_descriptor_crc32("0xD8079D85")?,
        );

        let json = session.to_json()?;
        let restored = CaptureSession::from_json(&json)?;
        assert_eq!(
            restored
                .metadata
                .fixture
                .as_ref()
                .map(CaptureFixtureMetadata::vendor_id),
            Some("0x346E")
        );
        Ok(())
    }

    #[test]
    fn from_json_rejects_invalid_fixture_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{
          "device": {"vid": 13422, "pid": 20},
          "metadata": {
            "format_version": "1.0",
            "synthetic": false,
            "fixture": {
              "vendor_id": "0x346e",
              "product_id": "0x0014",
              "capture_kind": "idle",
              "hardware_source": "real",
              "real_hardware_validated": false
            }
          },
          "records": []
        }"#;

        let result = CaptureSession::from_json(json);
        assert!(matches!(
            result,
            Err(CaptureError::FixtureMetadata(
                CaptureFixtureMetadataError::InvalidHex {
                    field: "vendor_id",
                    ..
                }
            ))
        ));
        Ok(())
    }

    #[test]
    fn validate_timestamps_ok() -> Result<(), CaptureError> {
        let session = sample_session();
        session.validate_timestamps()
    }

    #[test]
    fn validate_timestamps_bad() {
        let mut session = sample_session();
        session.records[2].timestamp_ns = 0; // out-of-order
        let err = session.validate_timestamps();
        assert!(err.is_err());
    }

    #[test]
    fn filter_direction_works() -> Result<(), CaptureError> {
        let session = sample_session();
        let device_to_host = session.filter_direction(Direction::DeviceToHost);
        assert_eq!(device_to_host.len(), 2);
        let host_to_device = session.filter_direction(Direction::HostToDevice);
        assert_eq!(host_to_device.len(), 1);
        Ok(())
    }

    #[test]
    fn filter_report_id_works() -> Result<(), CaptureError> {
        let session = sample_session();
        let id_01 = session.filter_report_id(0x01);
        assert_eq!(id_01.len(), 2);
        let id_20 = session.filter_report_id(0x20);
        assert_eq!(id_20.len(), 1);
        Ok(())
    }

    #[test]
    fn duration_ns_correct() -> Result<(), CaptureError> {
        let session = sample_session();
        assert_eq!(session.duration_ns(), 2_000_000);
        Ok(())
    }

    #[test]
    fn empty_session_duration() -> Result<(), CaptureError> {
        let session = CaptureSession {
            device: DeviceId {
                vid: 0,
                pid: 0,
                name: None,
            },
            metadata: CaptureMetadata::synthetic("empty"),
            records: vec![],
        };
        assert_eq!(session.duration_ns(), 0);
        Ok(())
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut session = sample_session();
        session.metadata.format_version = "99.0".to_owned();
        let json = serde_json::to_string(&session);
        assert!(json.is_ok());
        let result = CaptureSession::from_json(
            &json.unwrap_or_default(), // only in test helper scope
        );
        assert!(result.is_err());
    }

    #[test]
    fn direction_serde() -> Result<(), CaptureError> {
        let json = serde_json::to_string(&Direction::HostToDevice)?;
        assert_eq!(json, r#""host_to_device""#);
        let json = serde_json::to_string(&Direction::DeviceToHost)?;
        assert_eq!(json, r#""device_to_host""#);
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_direction() -> impl Strategy<Value = Direction> {
        prop_oneof![Just(Direction::HostToDevice), Just(Direction::DeviceToHost),]
    }

    fn arb_record() -> impl Strategy<Value = CaptureRecord> {
        (
            any::<u64>(),
            arb_direction(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..128),
        )
            .prop_map(|(ts, dir, rid, payload)| CaptureRecord {
                timestamp_ns: ts,
                direction: dir,
                report_id: rid,
                payload,
            })
    }

    fn arb_session() -> impl Strategy<Value = CaptureSession> {
        (
            any::<u16>(),
            any::<u16>(),
            proptest::collection::vec(arb_record(), 0..32),
        )
            .prop_map(|(vid, pid, records)| CaptureSession {
                device: DeviceId {
                    vid,
                    pid,
                    name: None,
                },
                metadata: CaptureMetadata::synthetic("proptest"),
                records,
            })
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        #[test]
        fn prop_json_roundtrip(session in arb_session()) {
            let json = session.to_json().map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            let restored = CaptureSession::from_json(&json).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            prop_assert_eq!(&session, &restored);
        }

        #[test]
        fn prop_record_roundtrip(record in arb_record()) {
            let json = serde_json::to_string(&record).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            let restored: CaptureRecord = serde_json::from_str(&json).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            prop_assert_eq!(&record, &restored);
        }

        #[test]
        fn prop_duration_never_panics(session in arb_session()) {
            let _ = session.duration_ns();
        }
    }
}
