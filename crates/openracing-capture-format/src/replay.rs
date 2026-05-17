//! Replay adapter — feeds captured data into protocol parsers.
//!
//! The [`ReplayIterator`] reads a [`CaptureSession`] and yields raw byte
//! slices in timestamp order, suitable for passing directly to vendor-specific
//! `parse_*` functions.

use crate::{CaptureRecord, CaptureSession, Direction};

/// Reconstructs the raw HID report bytes (report_id prepended to payload)
/// from a [`CaptureRecord`].
///
/// Most protocol parsers expect the report ID as the first byte of the slice.
#[must_use]
pub fn reconstruct_report(record: &CaptureRecord) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + record.payload.len());
    buf.push(record.report_id);
    buf.extend_from_slice(&record.payload);
    buf
}

/// Iterator over device-to-host reports in a capture session.
///
/// Yields `(timestamp_ns, raw_report_bytes)` tuples in capture order.
pub struct ReplayIterator<'a> {
    records: &'a [CaptureRecord],
    index: usize,
    direction_filter: Option<Direction>,
    report_id_filter: Option<u8>,
}

impl<'a> ReplayIterator<'a> {
    /// Create a replay iterator over all records in the session.
    #[must_use]
    pub fn new(session: &'a CaptureSession) -> Self {
        Self {
            records: &session.records,
            index: 0,
            direction_filter: None,
            report_id_filter: None,
        }
    }

    /// Only yield records flowing in the given direction.
    #[must_use]
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction_filter = Some(direction);
        self
    }

    /// Only yield records with the given report ID.
    #[must_use]
    pub fn with_report_id(mut self, report_id: u8) -> Self {
        self.report_id_filter = Some(report_id);
        self
    }
}

impl<'a> Iterator for ReplayIterator<'a> {
    /// `(timestamp_ns, raw_report_bytes)` — report ID is the first byte.
    type Item = (u64, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.records.len() {
            let record = &self.records[self.index];
            self.index += 1;

            if let Some(dir) = self.direction_filter
                && record.direction != dir
            {
                continue;
            }
            if let Some(rid) = self.report_id_filter
                && record.report_id != rid
            {
                continue;
            }

            return Some((record.timestamp_ns, reconstruct_report(record)));
        }
        None
    }
}

/// Feed all device-to-host input records from a session through a parser
/// function and collect the results.
///
/// The `parser` receives the raw report bytes (report_id + payload) and returns
/// `Some(T)` on success.  Records that do not parse are counted as failures.
pub fn replay_parse<T, F>(session: &CaptureSession, parser: F) -> ReplayResult<T>
where
    F: Fn(&[u8]) -> Option<T>,
{
    let mut parsed = Vec::new();
    let mut failed = 0usize;

    for record in &session.records {
        if record.direction != Direction::DeviceToHost {
            continue;
        }
        let raw = reconstruct_report(record);
        match parser(&raw) {
            Some(val) => parsed.push(ReplayEntry {
                timestamp_ns: record.timestamp_ns,
                value: val,
            }),
            None => failed += 1,
        }
    }

    ReplayResult { parsed, failed }
}

/// A single successfully parsed entry from a replay.
#[derive(Debug, Clone)]
pub struct ReplayEntry<T> {
    /// Capture timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Parsed value produced by the user-supplied parser.
    pub value: T,
}

/// Result of replaying a capture session through a parser.
#[derive(Debug, Clone)]
pub struct ReplayResult<T> {
    /// Successfully parsed entries in capture order.
    pub parsed: Vec<ReplayEntry<T>>,
    /// Number of device-to-host records that did not parse.
    pub failed: usize,
}

impl<T> ReplayResult<T> {
    /// Total number of device-to-host records attempted.
    #[must_use]
    pub fn total(&self) -> usize {
        self.parsed.len() + self.failed
    }

    /// Fraction of records that parsed successfully (0.0–1.0).
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.parsed.len() as f64 / total as f64
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureMetadata, DeviceId};

    fn make_session() -> CaptureSession {
        CaptureSession {
            device: DeviceId {
                vid: 0x0EB7,
                pid: 0x0001,
                name: None,
            },
            metadata: CaptureMetadata::synthetic("replay test"),
            records: vec![
                CaptureRecord {
                    timestamp_ns: 0,
                    direction: Direction::DeviceToHost,
                    report_id: 0x01,
                    payload: vec![0xAA; 8],
                },
                CaptureRecord {
                    timestamp_ns: 1_000_000,
                    direction: Direction::HostToDevice,
                    report_id: 0x20,
                    payload: vec![0x55; 4],
                },
                CaptureRecord {
                    timestamp_ns: 2_000_000,
                    direction: Direction::DeviceToHost,
                    report_id: 0x02,
                    payload: vec![0xBB; 8],
                },
            ],
        }
    }

    #[test]
    fn reconstruct_report_prepends_id() -> Result<(), String> {
        let record = CaptureRecord {
            timestamp_ns: 0,
            direction: Direction::DeviceToHost,
            report_id: 0x42,
            payload: vec![1, 2, 3],
        };
        let raw = reconstruct_report(&record);
        assert_eq!(raw, vec![0x42, 1, 2, 3]);
        Ok(())
    }

    #[test]
    fn replay_iterator_all() -> Result<(), String> {
        let session = make_session();
        let items: Vec<_> = ReplayIterator::new(&session).collect();
        assert_eq!(items.len(), 3);
        Ok(())
    }

    #[test]
    fn replay_iterator_direction_filter() -> Result<(), String> {
        let session = make_session();
        let items: Vec<_> = ReplayIterator::new(&session)
            .with_direction(Direction::DeviceToHost)
            .collect();
        assert_eq!(items.len(), 2);
        Ok(())
    }

    #[test]
    fn replay_iterator_report_id_filter() -> Result<(), String> {
        let session = make_session();
        let items: Vec<_> = ReplayIterator::new(&session).with_report_id(0x01).collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, 0); // timestamp
        Ok(())
    }

    #[test]
    fn replay_parse_basic() -> Result<(), String> {
        let session = make_session();
        // Parser that accepts any report starting with 0x01
        let result = replay_parse(&session, |data: &[u8]| {
            if !data.is_empty() && data[0] == 0x01 {
                Some(data.len())
            } else {
                None
            }
        });
        assert_eq!(result.parsed.len(), 1);
        assert_eq!(result.failed, 1); // report_id 0x02 doesn't match
        assert_eq!(result.total(), 2); // only device-to-host counted
        Ok(())
    }

    #[test]
    fn replay_result_success_rate() -> Result<(), String> {
        let result: ReplayResult<()> = ReplayResult {
            parsed: vec![ReplayEntry {
                timestamp_ns: 0,
                value: (),
            }],
            failed: 1,
        };
        let rate = result.success_rate();
        assert!((rate - 0.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn replay_result_empty() -> Result<(), String> {
        let result: ReplayResult<()> = ReplayResult {
            parsed: vec![],
            failed: 0,
        };
        assert_eq!(result.success_rate(), 0.0);
        Ok(())
    }

    fn empty_session() -> CaptureSession {
        CaptureSession {
            device: DeviceId {
                vid: 0,
                pid: 0,
                name: None,
            },
            metadata: CaptureMetadata::synthetic("empty"),
            records: vec![],
        }
    }

    #[test]
    fn replay_iterator_combined_filters_direction_and_report_id() -> Result<(), String> {
        // The sample session has two DeviceToHost records (ids 0x01, 0x02) and one
        // HostToDevice (id 0x20). Combining direction+id 0x01 must yield exactly one.
        let session = make_session();
        let items: Vec<_> = ReplayIterator::new(&session)
            .with_direction(Direction::DeviceToHost)
            .with_report_id(0x01)
            .collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, 0); // timestamp

        // The HostToDevice record has report_id 0x20, but specifying DeviceToHost
        // combined with 0x20 must yield zero matches.
        let items: Vec<_> = ReplayIterator::new(&session)
            .with_direction(Direction::DeviceToHost)
            .with_report_id(0x20)
            .collect();
        assert!(items.is_empty());
        Ok(())
    }

    #[test]
    fn replay_iterator_on_empty_session_yields_nothing() -> Result<(), String> {
        let session = empty_session();
        let items: Vec<_> = ReplayIterator::new(&session).collect();
        assert!(items.is_empty());

        // Filter combinations on empty input must also yield nothing.
        let items: Vec<_> = ReplayIterator::new(&session)
            .with_direction(Direction::DeviceToHost)
            .with_report_id(0x01)
            .collect();
        assert!(items.is_empty());
        Ok(())
    }

    #[test]
    fn reconstruct_report_with_empty_payload() -> Result<(), String> {
        let record = CaptureRecord {
            timestamp_ns: 0,
            direction: Direction::DeviceToHost,
            report_id: 0x42,
            payload: vec![],
        };
        let raw = reconstruct_report(&record);
        assert_eq!(raw, vec![0x42]);
        Ok(())
    }

    #[test]
    fn reconstruct_report_with_zero_report_id() -> Result<(), String> {
        // report_id 0 must still be prepended verbatim.
        let record = CaptureRecord {
            timestamp_ns: 0,
            direction: Direction::DeviceToHost,
            report_id: 0,
            payload: vec![0xAA, 0xBB],
        };
        let raw = reconstruct_report(&record);
        assert_eq!(raw, vec![0x00, 0xAA, 0xBB]);
        Ok(())
    }

    #[test]
    fn replay_parse_ignores_host_to_device_records() -> Result<(), String> {
        // Build a session that only contains HostToDevice records. The parser
        // would have accepted them by content, but they must be skipped.
        let session = CaptureSession {
            device: DeviceId {
                vid: 0,
                pid: 0,
                name: None,
            },
            metadata: CaptureMetadata::synthetic("h2d only"),
            records: vec![
                CaptureRecord {
                    timestamp_ns: 0,
                    direction: Direction::HostToDevice,
                    report_id: 0x01,
                    payload: vec![0xAA],
                },
                CaptureRecord {
                    timestamp_ns: 1_000,
                    direction: Direction::HostToDevice,
                    report_id: 0x01,
                    payload: vec![0xBB],
                },
            ],
        };
        let result = replay_parse(&session, |_data: &[u8]| Some(()));
        assert!(result.parsed.is_empty());
        assert_eq!(result.failed, 0);
        assert_eq!(result.total(), 0);
        Ok(())
    }

    #[test]
    fn replay_parse_empty_session_returns_zero_totals() -> Result<(), String> {
        let session = empty_session();
        let result = replay_parse(&session, |_data: &[u8]| Some(42usize));
        assert!(result.parsed.is_empty());
        assert_eq!(result.failed, 0);
        assert_eq!(result.total(), 0);
        Ok(())
    }

    #[test]
    fn replay_result_total_when_empty_is_zero() -> Result<(), String> {
        let result: ReplayResult<u8> = ReplayResult {
            parsed: vec![],
            failed: 0,
        };
        assert_eq!(result.total(), 0);
        Ok(())
    }

    #[test]
    fn replay_result_success_rate_zero_when_all_fail() -> Result<(), String> {
        let result: ReplayResult<()> = ReplayResult {
            parsed: vec![],
            failed: 5,
        };
        assert!((result.success_rate() - 0.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn replay_result_success_rate_one_when_all_pass() -> Result<(), String> {
        let result: ReplayResult<()> = ReplayResult {
            parsed: vec![
                ReplayEntry {
                    timestamp_ns: 0,
                    value: (),
                },
                ReplayEntry {
                    timestamp_ns: 1,
                    value: (),
                },
            ],
            failed: 0,
        };
        assert!((result.success_rate() - 1.0).abs() < f64::EPSILON);
        Ok(())
    }
}
