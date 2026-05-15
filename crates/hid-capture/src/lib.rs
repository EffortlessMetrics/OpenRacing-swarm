//! Shared library for hid-capture.
//!
//! Exposes data types, parsing helpers, timing analysis, protocol detection,
//! and capture format utilities for integration testing and community sharing.

#![deny(static_mut_refs)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureReport {
    pub timestamp_us: u64,
    pub report_id: u8,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureFile {
    pub vendor_id: String,
    pub product_id: String,
    pub captures: Vec<CaptureReport>,
}

/// Report metadata derived directly from a HID report descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidReportDescriptorMetadata {
    pub input_report_lengths: Vec<usize>,
    pub output_report_ids: Vec<u8>,
    pub output_reports: Vec<HidReportDescriptorReport>,
    pub feature_report_ids: Vec<u8>,
    #[serde(default)]
    pub feature_reports: Vec<HidReportDescriptorReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidReportDescriptorReport {
    pub report_id: u8,
    pub report_len: usize,
}

impl HidReportDescriptorMetadata {
    pub fn is_empty(&self) -> bool {
        self.input_report_lengths.is_empty()
            && self.output_report_ids.is_empty()
            && self.output_reports.is_empty()
            && self.feature_report_ids.is_empty()
            && self.feature_reports.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ReportDescriptorGlobalState {
    report_size_bits: usize,
    report_count: usize,
    report_id: u8,
}

/// Parse a HID report descriptor enough to derive report IDs and byte lengths.
///
/// This intentionally extracts only the bounded metadata needed for validation
/// receipts: report size, report count, report ID, and Input/Output/Feature
/// main items. Malformed or truncated descriptors return `None`.
pub fn parse_hid_report_descriptor_metadata(bytes: &[u8]) -> Option<HidReportDescriptorMetadata> {
    let mut state = ReportDescriptorGlobalState::default();
    let mut stack = Vec::new();
    let mut input_bits = BTreeMap::new();
    let mut output_bits = BTreeMap::new();
    let mut feature_bits = BTreeMap::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let prefix = bytes[index];
        index += 1;

        if prefix == 0xFE {
            let size = usize::from(*bytes.get(index)?);
            index = index.checked_add(2)?;
            index = index.checked_add(size)?;
            if index > bytes.len() {
                return None;
            }
            continue;
        }

        let data_len = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => return None,
        };
        let end = index.checked_add(data_len)?;
        let data = bytes.get(index..end)?;
        index = end;

        let item_type = (prefix >> 2) & 0x03;
        let tag = (prefix >> 4) & 0x0F;
        let value = little_endian_u32(data)?;

        match item_type {
            0 => match tag {
                0x08 => record_descriptor_main_item(&mut input_bits, state)?,
                0x09 => record_descriptor_main_item(&mut output_bits, state)?,
                0x0B => record_descriptor_main_item(&mut feature_bits, state)?,
                _ => {}
            },
            1 => match tag {
                0x07 => state.report_size_bits = usize::try_from(value).ok()?,
                0x08 => state.report_id = u8::try_from(value).ok().filter(|id| *id != 0)?,
                0x09 => state.report_count = usize::try_from(value).ok()?,
                0x0A => stack.push(state),
                0x0B => state = stack.pop()?,
                _ => {}
            },
            _ => {}
        }
    }

    let metadata = HidReportDescriptorMetadata {
        input_report_lengths: report_lengths_from_bits(&input_bits),
        output_report_ids: report_ids_from_bits(&output_bits),
        output_reports: report_metadata_from_bits(&output_bits),
        feature_report_ids: report_ids_from_bits(&feature_bits),
        feature_reports: report_metadata_from_bits(&feature_bits),
    };
    (!metadata.is_empty()).then_some(metadata)
}

fn little_endian_u32(bytes: &[u8]) -> Option<u32> {
    let mut buffer = [0u8; 4];
    let len = bytes.len();
    if len > buffer.len() {
        return None;
    }
    buffer[..len].copy_from_slice(bytes);
    Some(u32::from_le_bytes(buffer))
}

fn record_descriptor_main_item(
    reports: &mut BTreeMap<u8, usize>,
    state: ReportDescriptorGlobalState,
) -> Option<()> {
    let bits = state.report_size_bits.checked_mul(state.report_count)?;
    if bits == 0 {
        return Some(());
    }
    let entry = reports.entry(state.report_id).or_insert(0);
    *entry = entry.checked_add(bits)?;
    Some(())
}

fn report_lengths_from_bits(reports: &BTreeMap<u8, usize>) -> Vec<usize> {
    reports
        .iter()
        .filter_map(|(report_id, bits)| {
            let data_bytes = bits.div_ceil(8);
            data_bytes.checked_add(usize::from(*report_id != 0))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn report_metadata_from_bits(reports: &BTreeMap<u8, usize>) -> Vec<HidReportDescriptorReport> {
    reports
        .iter()
        .map(|(report_id, bits)| {
            let data_bytes = bits.div_ceil(8);
            HidReportDescriptorReport {
                report_id: *report_id,
                report_len: data_bytes + usize::from(*report_id != 0),
            }
        })
        .collect()
}

fn report_ids_from_bits(reports: &BTreeMap<u8, usize>) -> Vec<u8> {
    reports
        .keys()
        .copied()
        .filter(|report_id| *report_id != 0)
        .collect()
}

// ── Capture metadata for community sharing format ────────────────────────────

/// Metadata for the community sharing capture format (versioned).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureMetadata {
    /// Format version string, e.g. "1.0".
    pub format_version: String,
    /// ISO-8601 timestamp when the capture was recorded.
    pub captured_at: String,
    /// Platform the capture was taken on (e.g. "windows", "linux", "macos").
    pub platform: String,
    /// Freeform tool name / version that produced the capture.
    pub tool_version: String,
    /// Optional description provided by the user.
    pub description: String,
}

/// Community-sharing capture file with versioned metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedCaptureFile {
    pub metadata: CaptureMetadata,
    pub vendor_id: String,
    pub product_id: String,
    pub captures: Vec<CaptureReport>,
}

// ── Timing analysis ──────────────────────────────────────────────────────────

/// Statistical summary of inter-report timing from a capture session.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingStats {
    /// Number of inter-report intervals analysed.
    pub count: usize,
    /// Mean interval in microseconds.
    pub mean_us: f64,
    /// Median interval in microseconds.
    pub median_us: f64,
    /// Minimum interval in microseconds.
    pub min_us: f64,
    /// Maximum interval in microseconds.
    pub max_us: f64,
    /// Standard deviation in microseconds.
    pub std_dev_us: f64,
    /// P99 interval in microseconds.
    pub p99_us: f64,
    /// Jitter: max - min interval in microseconds.
    pub jitter_us: f64,
    /// Estimated capture rate in Hz.
    pub estimated_rate_hz: f64,
}

/// Compute timing statistics from a slice of [`CaptureReport`]s.
///
/// Returns `None` if fewer than 2 reports are provided.
pub fn compute_timing_stats(captures: &[CaptureReport]) -> Option<TimingStats> {
    if captures.len() < 2 {
        return None;
    }

    let mut intervals: Vec<f64> = captures
        .windows(2)
        .map(|w| (w[1].timestamp_us as f64) - (w[0].timestamp_us as f64))
        .collect();

    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = intervals.len();
    let sum: f64 = intervals.iter().sum();
    let mean = sum / count as f64;
    let min = intervals[0];
    let max = intervals[count - 1];
    let median = if count.is_multiple_of(2) {
        (intervals[count / 2 - 1] + intervals[count / 2]) / 2.0
    } else {
        intervals[count / 2]
    };

    let p99_idx = ((count as f64) * 0.99).ceil() as usize;
    let p99 = intervals[p99_idx.min(count - 1)];

    let variance: f64 = intervals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();

    let estimated_rate_hz = if mean > 0.0 { 1_000_000.0 / mean } else { 0.0 };

    Some(TimingStats {
        count,
        mean_us: mean,
        median_us: median,
        min_us: min,
        max_us: max,
        std_dev_us: std_dev,
        p99_us: p99,
        jitter_us: max - min,
        estimated_rate_hz,
    })
}

/// Check that all timestamps in a capture are monotonically increasing.
///
/// Returns the index of the first violation, or `None` if all timestamps are
/// monotonic.
pub fn validate_monotonic_timestamps(captures: &[CaptureReport]) -> Option<usize> {
    captures
        .windows(2)
        .position(|w| w[1].timestamp_us <= w[0].timestamp_us)
        .map(|i| i + 1)
}

/// Filter captures to only those matching a specific `report_id`.
pub fn filter_by_report_id(captures: &[CaptureReport], report_id: u8) -> Vec<&CaptureReport> {
    captures
        .iter()
        .filter(|c| c.report_id == report_id)
        .collect()
}

/// Compute the total capture duration in microseconds. Returns 0 for empty/single-element captures.
pub fn capture_duration_us(captures: &[CaptureReport]) -> u64 {
    match (captures.first(), captures.last()) {
        (Some(first), Some(last)) => last.timestamp_us.saturating_sub(first.timestamp_us),
        _ => 0,
    }
}

// ── Vendor / protocol detection ──────────────────────────────────────────────

/// Known HID racing wheel vendor IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownVendor {
    Moza,
    Logitech,
    Fanatec,
    Thrustmaster,
    Simagic,
    Simucube,
    CammusDirect,
    AccuForce,
    VRS,
    Heusinkveld,
}

/// Attempt to identify a known racing wheel vendor from a VID string.
///
/// Returns `None` for unrecognised vendor IDs.
pub fn detect_vendor(vid_str: &str) -> Option<KnownVendor> {
    let vid = parse_hex_u16(vid_str).ok()?;
    detect_vendor_by_id(vid)
}

/// Identify a known racing wheel vendor from a numeric VID.
pub fn detect_vendor_by_id(vid: u16) -> Option<KnownVendor> {
    match vid {
        0x346E => Some(KnownVendor::Moza),
        0x046D => Some(KnownVendor::Logitech),
        0x0EB7 => Some(KnownVendor::Fanatec),
        0x044F => Some(KnownVendor::Thrustmaster),
        0x0483 => Some(KnownVendor::Simagic),
        0x16D0 => Some(KnownVendor::Simucube),
        0x3416 => Some(KnownVendor::CammusDirect),
        0x1FC9 => Some(KnownVendor::AccuForce),
        0x35F0 => Some(KnownVendor::VRS),
        0x04D8 => Some(KnownVendor::Heusinkveld),
        _ => None,
    }
}

/// Return a human-readable vendor name for a known vendor.
pub fn vendor_name(vendor: KnownVendor) -> &'static str {
    match vendor {
        KnownVendor::Moza => "MOZA Racing",
        KnownVendor::Logitech => "Logitech",
        KnownVendor::Fanatec => "Fanatec",
        KnownVendor::Thrustmaster => "Thrustmaster",
        KnownVendor::Simagic => "Simagic",
        KnownVendor::Simucube => "Simucube",
        KnownVendor::CammusDirect => "Cammus",
        KnownVendor::AccuForce => "AccuForce",
        KnownVendor::VRS => "VRS DirectForce",
        KnownVendor::Heusinkveld => "Heusinkveld",
    }
}

// ── Capture format validation ────────────────────────────────────────────────

/// Errors that can occur when validating a capture file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureValidationError {
    /// VID string could not be parsed.
    InvalidVendorId(String),
    /// PID string could not be parsed.
    InvalidProductId(String),
    /// Timestamp at the given index is not monotonically increasing.
    NonMonotonicTimestamp { index: usize },
    /// Metadata format version is unsupported.
    UnsupportedFormatVersion(String),
}

/// Validate a [`CaptureFile`] for structural correctness.
pub fn validate_capture_file(file: &CaptureFile) -> Vec<CaptureValidationError> {
    let mut errors = Vec::new();

    if parse_hex_u16(&file.vendor_id).is_err() {
        errors.push(CaptureValidationError::InvalidVendorId(
            file.vendor_id.clone(),
        ));
    }
    if parse_hex_u16(&file.product_id).is_err() {
        errors.push(CaptureValidationError::InvalidProductId(
            file.product_id.clone(),
        ));
    }
    if let Some(idx) = validate_monotonic_timestamps(&file.captures) {
        errors.push(CaptureValidationError::NonMonotonicTimestamp { index: idx });
    }

    errors
}

/// Validate a [`SharedCaptureFile`] including metadata checks.
pub fn validate_shared_capture_file(file: &SharedCaptureFile) -> Vec<CaptureValidationError> {
    let mut errors = Vec::new();

    if file.metadata.format_version != "1.0" {
        errors.push(CaptureValidationError::UnsupportedFormatVersion(
            file.metadata.format_version.clone(),
        ));
    }
    if parse_hex_u16(&file.vendor_id).is_err() {
        errors.push(CaptureValidationError::InvalidVendorId(
            file.vendor_id.clone(),
        ));
    }
    if parse_hex_u16(&file.product_id).is_err() {
        errors.push(CaptureValidationError::InvalidProductId(
            file.product_id.clone(),
        ));
    }
    if let Some(idx) = validate_monotonic_timestamps(&file.captures) {
        errors.push(CaptureValidationError::NonMonotonicTimestamp { index: idx });
    }

    errors
}

/// Convert a basic [`CaptureFile`] to the community [`SharedCaptureFile`] format.
pub fn to_shared_format(
    file: &CaptureFile,
    platform: &str,
    tool_version: &str,
    captured_at: &str,
    description: &str,
) -> SharedCaptureFile {
    SharedCaptureFile {
        metadata: CaptureMetadata {
            format_version: "1.0".to_string(),
            captured_at: captured_at.to_string(),
            platform: platform.to_string(),
            tool_version: tool_version.to_string(),
            description: description.to_string(),
        },
        vendor_id: file.vendor_id.clone(),
        product_id: file.product_id.clone(),
        captures: file
            .captures
            .iter()
            .map(|c| CaptureReport {
                timestamp_us: c.timestamp_us,
                report_id: c.report_id,
                data: c.data.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn parse_hid_report_descriptor_metadata_extracts_reports() -> TestResult {
        let descriptor = [
            0x85, 0x01, 0x75, 0x08, 0x95, 0x06, 0x81, 0x02, 0x85, 0x02, 0x75, 0x08, 0x95, 0x1E,
            0x81, 0x02, 0x85, 0x20, 0x75, 0x08, 0x95, 0x07, 0x91, 0x02, 0x85, 0x03, 0x75, 0x08,
            0x95, 0x03, 0xB1, 0x02, 0x85, 0x11, 0x75, 0x08, 0x95, 0x03, 0xB1, 0x02,
        ];

        let metadata = parse_hid_report_descriptor_metadata(&descriptor)
            .ok_or("expected descriptor metadata")?;

        assert_eq!(metadata.input_report_lengths, vec![7, 31]);
        assert_eq!(metadata.output_report_ids, vec![0x20]);
        assert_eq!(
            metadata.output_reports,
            vec![HidReportDescriptorReport {
                report_id: 0x20,
                report_len: 8,
            }]
        );
        assert_eq!(metadata.feature_report_ids, vec![0x03, 0x11]);
        assert_eq!(
            metadata.feature_reports,
            vec![
                HidReportDescriptorReport {
                    report_id: 0x03,
                    report_len: 4,
                },
                HidReportDescriptorReport {
                    report_id: 0x11,
                    report_len: 4,
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_rejects_truncated_items() {
        let descriptor = [0x85, 0x01, 0x75];

        assert!(parse_hid_report_descriptor_metadata(&descriptor).is_none());
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_skips_long_items_and_restores_globals() -> TestResult {
        let descriptor = [
            0xFE, 0x02, 0x00, 0xAA, 0xBB, // long item ignored
            0x85, 0x01, // report ID 1
            0x75, 0x08, // report size 8 bits
            0x95, 0x01, // report count 1
            0x81, 0x02, // input: report 1 length 2 including report ID
            0xA4, // push globals
            0x85, 0x20, // report ID 0x20
            0x95, 0x07, // report count 7
            0x91, 0x02, // output: report 0x20 length 8 including report ID
            0xB4, // pop globals, restoring report ID 1/count 1
            0xB1, 0x02, // feature: report 1
        ];

        let metadata = parse_hid_report_descriptor_metadata(&descriptor)
            .ok_or("expected descriptor metadata")?;

        assert_eq!(metadata.input_report_lengths, vec![2]);
        assert_eq!(metadata.output_report_ids, vec![0x20]);
        assert_eq!(
            metadata.output_reports,
            vec![HidReportDescriptorReport {
                report_id: 0x20,
                report_len: 8,
            }]
        );
        assert_eq!(metadata.feature_report_ids, vec![0x01]);
        assert_eq!(
            metadata.feature_reports,
            vec![HidReportDescriptorReport {
                report_id: 0x01,
                report_len: 2,
            }]
        );
        Ok(())
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_rejects_truncated_long_item() {
        let descriptor = [0xFE, 0x04, 0x00, 0xAA];

        assert!(parse_hid_report_descriptor_metadata(&descriptor).is_none());
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_rejects_zero_report_id() {
        let descriptor = [0x85, 0x00];

        assert!(parse_hid_report_descriptor_metadata(&descriptor).is_none());
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_rejects_unmatched_pop() {
        let descriptor = [0xB4];

        assert!(parse_hid_report_descriptor_metadata(&descriptor).is_none());
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_ignores_zero_bit_main_item() {
        let descriptor = [0x85, 0x01, 0x75, 0x00, 0x95, 0x04, 0x81, 0x02];

        assert!(parse_hid_report_descriptor_metadata(&descriptor).is_none());
    }

    #[test]
    fn parse_hid_report_descriptor_metadata_accepts_wide_items_and_unknown_main_tags() -> TestResult
    {
        let descriptor = [
            0x86, 0x01, 0x00, // report ID 1, encoded as a 2-byte value
            0x76, 0x08, 0x00, // report size 8 bits, encoded as a 2-byte value
            0x97, 0x01, 0x00, 0x00, 0x00, // report count 1, encoded as a 4-byte value
            0xA0, // unknown main item tag, ignored
            0x81, 0x02, // input report
        ];

        let metadata = parse_hid_report_descriptor_metadata(&descriptor)
            .ok_or("expected descriptor metadata")?;

        assert_eq!(metadata.input_report_lengths, vec![2]);
        assert!(metadata.output_report_ids.is_empty());
        assert!(metadata.feature_report_ids.is_empty());
        assert!(metadata.feature_reports.is_empty());
        Ok(())
    }

    #[test]
    fn little_endian_u32_rejects_more_than_four_bytes() {
        assert!(little_endian_u32(&[1, 2, 3, 4, 5]).is_none());
    }
}
