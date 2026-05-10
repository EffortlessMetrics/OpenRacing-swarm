//! Software-only HID report replay for validation planning.
//!
//! The virtual backend records raw input/output reports and injected faults
//! without opening HID devices. Receipts from this module are always marked as
//! virtual evidence and cannot be used as real hardware validation.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::EvidenceSource;

/// USB/HID identity for a virtual replay device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualHidIdentity {
    vendor_id: u16,
    product_id: u16,
    device_key: String,
    manufacturer: Option<String>,
    product_name: Option<String>,
    serial_number_present: bool,
    interface_number: Option<u8>,
    usage_page: Option<u16>,
    usage: Option<u16>,
}

impl VirtualHidIdentity {
    /// Create a virtual device identity with a stable non-empty device key.
    pub fn new(
        vendor_id: u16,
        product_id: u16,
        device_key: impl Into<String>,
    ) -> Result<Self, VirtualHidError> {
        let device_key = non_empty_string("device_key", device_key)?;
        Ok(Self {
            vendor_id,
            product_id,
            device_key,
            manufacturer: None,
            product_name: None,
            serial_number_present: false,
            interface_number: None,
            usage_page: None,
            usage: None,
        })
    }

    #[must_use]
    pub const fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    #[must_use]
    pub const fn product_id(&self) -> u16 {
        self.product_id
    }

    #[must_use]
    pub fn device_key(&self) -> &str {
        &self.device_key
    }

    #[must_use]
    pub fn manufacturer(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }

    #[must_use]
    pub fn product_name(&self) -> Option<&str> {
        self.product_name.as_deref()
    }

    #[must_use]
    pub const fn serial_number_present(&self) -> bool {
        self.serial_number_present
    }

    #[must_use]
    pub const fn interface_number(&self) -> Option<u8> {
        self.interface_number
    }

    #[must_use]
    pub const fn usage_page(&self) -> Option<u16> {
        self.usage_page
    }

    #[must_use]
    pub const fn usage(&self) -> Option<u16> {
        self.usage
    }

    /// Attach a manufacturer string.
    pub fn with_manufacturer(
        mut self,
        manufacturer: impl Into<String>,
    ) -> Result<Self, VirtualHidError> {
        self.manufacturer = Some(non_empty_string("manufacturer", manufacturer)?);
        Ok(self)
    }

    /// Attach a product name string.
    pub fn with_product_name(
        mut self,
        product_name: impl Into<String>,
    ) -> Result<Self, VirtualHidError> {
        self.product_name = Some(non_empty_string("product_name", product_name)?);
        Ok(self)
    }

    /// Record whether a serial number was present without storing the value.
    #[must_use]
    pub const fn with_serial_number_present(mut self, present: bool) -> Self {
        self.serial_number_present = present;
        self
    }

    /// Attach HID interface metadata.
    #[must_use]
    pub const fn with_interface(mut self, interface_number: u8) -> Self {
        self.interface_number = Some(interface_number);
        self
    }

    /// Attach HID usage metadata.
    #[must_use]
    pub const fn with_usage(mut self, usage_page: u16, usage: u16) -> Self {
        self.usage_page = Some(usage_page);
        self.usage = Some(usage);
        self
    }

    fn with_product_id(mut self, product_id: u16) -> Self {
        self.product_id = product_id;
        self
    }
}

/// Descriptor metadata used by virtual replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualHidDescriptor {
    report_descriptor_crc32: String,
    input_report_lengths: Vec<usize>,
    output_report_ids: Vec<u8>,
    feature_report_ids: Vec<u8>,
}

impl VirtualHidDescriptor {
    /// Create descriptor metadata with a required non-empty CRC string.
    pub fn new(report_descriptor_crc32: impl Into<String>) -> Result<Self, VirtualHidError> {
        Ok(Self {
            report_descriptor_crc32: non_empty_string(
                "report_descriptor_crc32",
                report_descriptor_crc32,
            )?,
            input_report_lengths: Vec::new(),
            output_report_ids: Vec::new(),
            feature_report_ids: Vec::new(),
        })
    }

    #[must_use]
    pub fn report_descriptor_crc32(&self) -> &str {
        &self.report_descriptor_crc32
    }

    #[must_use]
    pub fn input_report_lengths(&self) -> &[usize] {
        &self.input_report_lengths
    }

    #[must_use]
    pub fn output_report_ids(&self) -> &[u8] {
        &self.output_report_ids
    }

    #[must_use]
    pub fn feature_report_ids(&self) -> &[u8] {
        &self.feature_report_ids
    }

    /// Replace expected input report lengths.
    pub fn with_input_report_lengths(
        mut self,
        lengths: impl IntoIterator<Item = usize>,
    ) -> Result<Self, VirtualHidError> {
        self.input_report_lengths = collect_non_zero_lengths("input_report_lengths", lengths)?;
        Ok(self)
    }

    /// Replace output report IDs.
    #[must_use]
    pub fn with_output_report_ids(mut self, ids: impl IntoIterator<Item = u8>) -> Self {
        self.output_report_ids = ids.into_iter().collect();
        self
    }

    /// Replace feature report IDs.
    #[must_use]
    pub fn with_feature_report_ids(mut self, ids: impl IntoIterator<Item = u8>) -> Self {
        self.feature_report_ids = ids.into_iter().collect();
        self
    }

    fn with_report_descriptor_crc32(mut self, crc32: String) -> Self {
        self.report_descriptor_crc32 = crc32;
        self
    }

    fn min_input_report_length(&self) -> Option<usize> {
        self.input_report_lengths.iter().copied().min()
    }
}

/// A raw input report queued for virtual replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualInputReport {
    timestamp_us: u64,
    bytes: Vec<u8>,
}

impl VirtualInputReport {
    /// Create a virtual input report. The bytes include the report ID when the
    /// device protocol uses report IDs.
    pub fn new(timestamp_us: u64, bytes: impl Into<Vec<u8>>) -> Result<Self, VirtualHidError> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(VirtualHidError::EmptyReport { field: "bytes" });
        }
        Ok(Self {
            timestamp_us,
            bytes,
        })
    }

    #[must_use]
    pub const fn timestamp_us(&self) -> u64 {
        self.timestamp_us
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn report_id(&self) -> u8 {
        match self.bytes.first() {
            Some(report_id) => *report_id,
            None => 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn is_short_for(&self, descriptor: &VirtualHidDescriptor) -> bool {
        descriptor
            .min_input_report_length()
            .is_some_and(|min_len| self.len() < min_len)
    }
}

/// Kind of report written into the virtual output log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualOutputKind {
    Output,
    Feature,
}

/// Result recorded for each attempted virtual write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum VirtualWriteResult {
    Written { bytes_written: usize },
    Disconnected,
    WatchdogExpired { timeout_ms: u64 },
}

/// A virtual read record with parser-relevant fault classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualInputLogEntry {
    event_index: u64,
    report: VirtualInputReport,
    short_report: bool,
    duplicate_timestamp: bool,
}

impl VirtualInputLogEntry {
    #[must_use]
    pub const fn event_index(&self) -> u64 {
        self.event_index
    }

    #[must_use]
    pub const fn timestamp_us(&self) -> u64 {
        self.report.timestamp_us()
    }

    #[must_use]
    pub fn report(&self) -> &VirtualInputReport {
        &self.report
    }

    #[must_use]
    pub const fn short_report(&self) -> bool {
        self.short_report
    }

    #[must_use]
    pub const fn duplicate_timestamp(&self) -> bool {
        self.duplicate_timestamp
    }
}

/// A virtual output/feature report write record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualOutputLogEntry {
    event_index: u64,
    timestamp_us: u64,
    kind: VirtualOutputKind,
    bytes: Vec<u8>,
    result: VirtualWriteResult,
}

impl VirtualOutputLogEntry {
    #[must_use]
    pub const fn event_index(&self) -> u64 {
        self.event_index
    }

    #[must_use]
    pub const fn timestamp_us(&self) -> u64 {
        self.timestamp_us
    }

    #[must_use]
    pub const fn kind(&self) -> VirtualOutputKind {
        self.kind
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn result(&self) -> &VirtualWriteResult {
        &self.result
    }
}

/// Fault/event kinds that a virtual replay can inject or classify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualHidFaultKind {
    Disconnect,
    StaleDescriptor,
    WrongProductId,
    ShortInputReport,
    DuplicateTimestamp,
    WatchdogExpired,
}

/// A virtual fault/event recorded in receipt order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualHidFaultEvent {
    event_index: u64,
    kind: VirtualHidFaultKind,
    details: String,
}

impl VirtualHidFaultEvent {
    #[must_use]
    pub const fn event_index(&self) -> u64 {
        self.event_index
    }

    #[must_use]
    pub const fn kind(&self) -> VirtualHidFaultKind {
        self.kind
    }

    #[must_use]
    pub fn details(&self) -> &str {
        &self.details
    }
}

/// Serializable receipt for a virtual HID replay session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualHidReplayReceipt {
    hardware_source: EvidenceSource,
    real_hardware_validated: bool,
    device: VirtualHidIdentity,
    reported_device: VirtualHidIdentity,
    descriptor: VirtualHidDescriptor,
    reported_descriptor: VirtualHidDescriptor,
    input_log: Vec<VirtualInputLogEntry>,
    output_log: Vec<VirtualOutputLogEntry>,
    fault_events: Vec<VirtualHidFaultEvent>,
}

impl VirtualHidReplayReceipt {
    #[must_use]
    pub const fn hardware_source(&self) -> EvidenceSource {
        self.hardware_source
    }

    #[must_use]
    pub const fn real_hardware_validated(&self) -> bool {
        self.real_hardware_validated
    }

    #[must_use]
    pub fn device(&self) -> &VirtualHidIdentity {
        &self.device
    }

    #[must_use]
    pub fn reported_device(&self) -> &VirtualHidIdentity {
        &self.reported_device
    }

    #[must_use]
    pub fn descriptor(&self) -> &VirtualHidDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub fn reported_descriptor(&self) -> &VirtualHidDescriptor {
        &self.reported_descriptor
    }

    #[must_use]
    pub fn input_log(&self) -> &[VirtualInputLogEntry] {
        &self.input_log
    }

    #[must_use]
    pub fn output_log(&self) -> &[VirtualOutputLogEntry] {
        &self.output_log
    }

    #[must_use]
    pub fn fault_events(&self) -> &[VirtualHidFaultEvent] {
        &self.fault_events
    }
}

/// Software-only HID replay backend for validation tests.
#[derive(Debug, Clone)]
pub struct VirtualHidReplay {
    device: VirtualHidIdentity,
    reported_device: VirtualHidIdentity,
    descriptor: VirtualHidDescriptor,
    reported_descriptor: VirtualHidDescriptor,
    input_queue: VecDeque<VirtualInputReport>,
    input_log: Vec<VirtualInputLogEntry>,
    output_log: Vec<VirtualOutputLogEntry>,
    fault_events: Vec<VirtualHidFaultEvent>,
    connected: bool,
    event_index: u64,
    timestamp_us: u64,
    watchdog_expired: Option<u64>,
}

impl VirtualHidReplay {
    /// Create a virtual HID replay backend from identity and descriptor data.
    #[must_use]
    pub fn new(device: VirtualHidIdentity, descriptor: VirtualHidDescriptor) -> Self {
        Self {
            reported_device: device.clone(),
            reported_descriptor: descriptor.clone(),
            device,
            descriptor,
            input_queue: VecDeque::new(),
            input_log: Vec::new(),
            output_log: Vec::new(),
            fault_events: Vec::new(),
            connected: true,
            event_index: 0,
            timestamp_us: 0,
            watchdog_expired: None,
        }
    }

    #[must_use]
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    #[must_use]
    pub fn device(&self) -> &VirtualHidIdentity {
        &self.device
    }

    #[must_use]
    pub fn reported_device(&self) -> &VirtualHidIdentity {
        &self.reported_device
    }

    #[must_use]
    pub fn descriptor(&self) -> &VirtualHidDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub fn reported_descriptor(&self) -> &VirtualHidDescriptor {
        &self.reported_descriptor
    }

    #[must_use]
    pub fn input_log(&self) -> &[VirtualInputLogEntry] {
        &self.input_log
    }

    #[must_use]
    pub fn output_log(&self) -> &[VirtualOutputLogEntry] {
        &self.output_log
    }

    #[must_use]
    pub fn fault_events(&self) -> &[VirtualHidFaultEvent] {
        &self.fault_events
    }

    /// Set the virtual timestamp used for subsequent write records.
    pub fn set_timestamp_us(&mut self, timestamp_us: u64) {
        self.timestamp_us = timestamp_us;
    }

    /// Queue an input report for later replay.
    pub fn queue_input_report(&mut self, report: VirtualInputReport) {
        self.input_queue.push_back(report);
    }

    /// Queue multiple input reports in order.
    pub fn queue_input_reports(&mut self, reports: impl IntoIterator<Item = VirtualInputReport>) {
        self.input_queue.extend(reports);
    }

    /// Read the next queued input report and classify short/duplicate timing.
    pub fn read_input_report(&mut self) -> Result<VirtualInputReport, VirtualHidError> {
        if !self.connected {
            return Err(VirtualHidError::Disconnected);
        }

        let report = self
            .input_queue
            .pop_front()
            .ok_or(VirtualHidError::NoInputReport)?;
        let event_index = self.next_event_index();
        let short_report = report.is_short_for(&self.reported_descriptor);
        let duplicate_timestamp = self
            .input_log
            .last()
            .is_some_and(|entry| entry.timestamp_us() == report.timestamp_us());

        if short_report {
            self.record_fault(
                event_index,
                VirtualHidFaultKind::ShortInputReport,
                format!(
                    "input report length {} is shorter than expected {:?}",
                    report.len(),
                    self.reported_descriptor.input_report_lengths()
                ),
            );
        }
        if duplicate_timestamp {
            self.record_fault(
                event_index,
                VirtualHidFaultKind::DuplicateTimestamp,
                format!("duplicate timestamp_us {}", report.timestamp_us()),
            );
        }

        self.input_log.push(VirtualInputLogEntry {
            event_index,
            report: report.clone(),
            short_report,
            duplicate_timestamp,
        });
        Ok(report)
    }

    /// Record an output report write attempt.
    pub fn write_output_report(&mut self, bytes: &[u8]) -> Result<usize, VirtualHidError> {
        self.write_report(VirtualOutputKind::Output, bytes)
    }

    /// Record a feature report write attempt.
    pub fn write_feature_report(&mut self, bytes: &[u8]) -> Result<usize, VirtualHidError> {
        self.write_report(VirtualOutputKind::Feature, bytes)
    }

    /// Simulate a disconnect before subsequent reads/writes.
    pub fn disconnect(&mut self) {
        if self.connected {
            self.connected = false;
            let event_index = self.next_event_index();
            self.record_fault(
                event_index,
                VirtualHidFaultKind::Disconnect,
                "virtual device disconnected",
            );
        }
    }

    /// Simulate reconnecting the virtual device.
    pub fn reconnect(&mut self) {
        self.connected = true;
    }

    /// Simulate descriptor drift seen by higher layers.
    pub fn inject_stale_descriptor(
        &mut self,
        report_descriptor_crc32: impl Into<String>,
    ) -> Result<(), VirtualHidError> {
        let crc32 = non_empty_string("report_descriptor_crc32", report_descriptor_crc32)?;
        self.reported_descriptor = self
            .reported_descriptor
            .clone()
            .with_report_descriptor_crc32(crc32.clone());
        let event_index = self.next_event_index();
        self.record_fault(
            event_index,
            VirtualHidFaultKind::StaleDescriptor,
            format!("reported descriptor crc32 changed to {crc32}"),
        );
        Ok(())
    }

    /// Simulate an unexpected product ID being reported.
    pub fn inject_wrong_product_id(&mut self, product_id: u16) {
        self.reported_device = self.reported_device.clone().with_product_id(product_id);
        let event_index = self.next_event_index();
        self.record_fault(
            event_index,
            VirtualHidFaultKind::WrongProductId,
            format!("reported product_id changed to 0x{product_id:04X}"),
        );
    }

    /// Simulate watchdog expiry before subsequent writes.
    pub fn expire_watchdog(&mut self, timeout_ms: u64) {
        self.watchdog_expired = Some(timeout_ms);
        let event_index = self.next_event_index();
        self.record_fault(
            event_index,
            VirtualHidFaultKind::WatchdogExpired,
            format!("virtual watchdog expired after {timeout_ms} ms"),
        );
    }

    /// Build a virtual receipt. This receipt is deliberately never real hardware
    /// evidence.
    #[must_use]
    pub fn receipt(&self) -> VirtualHidReplayReceipt {
        VirtualHidReplayReceipt {
            hardware_source: EvidenceSource::Virtual,
            real_hardware_validated: false,
            device: self.device.clone(),
            reported_device: self.reported_device.clone(),
            descriptor: self.descriptor.clone(),
            reported_descriptor: self.reported_descriptor.clone(),
            input_log: self.input_log.clone(),
            output_log: self.output_log.clone(),
            fault_events: self.fault_events.clone(),
        }
    }

    fn write_report(
        &mut self,
        kind: VirtualOutputKind,
        bytes: &[u8],
    ) -> Result<usize, VirtualHidError> {
        if bytes.is_empty() {
            return Err(VirtualHidError::EmptyReport { field: "bytes" });
        }

        let event_index = self.next_event_index();
        let result = if !self.connected {
            VirtualWriteResult::Disconnected
        } else if let Some(timeout_ms) = self.watchdog_expired {
            VirtualWriteResult::WatchdogExpired { timeout_ms }
        } else {
            VirtualWriteResult::Written {
                bytes_written: bytes.len(),
            }
        };

        self.output_log.push(VirtualOutputLogEntry {
            event_index,
            timestamp_us: self.timestamp_us,
            kind,
            bytes: bytes.to_vec(),
            result: result.clone(),
        });

        match result {
            VirtualWriteResult::Written { bytes_written } => Ok(bytes_written),
            VirtualWriteResult::Disconnected => Err(VirtualHidError::Disconnected),
            VirtualWriteResult::WatchdogExpired { timeout_ms } => {
                Err(VirtualHidError::WatchdogExpired { timeout_ms })
            }
        }
    }

    fn next_event_index(&mut self) -> u64 {
        self.event_index = self.event_index.saturating_add(1);
        self.event_index
    }

    fn record_fault(
        &mut self,
        event_index: u64,
        kind: VirtualHidFaultKind,
        details: impl Into<String>,
    ) {
        self.fault_events.push(VirtualHidFaultEvent {
            event_index,
            kind,
            details: details.into(),
        });
    }
}

/// Errors from virtual HID replay operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VirtualHidError {
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} must not contain zero report lengths")]
    ZeroReportLength { field: &'static str },
    #[error("{field} must not be an empty report")]
    EmptyReport { field: &'static str },
    #[error("virtual HID device is disconnected")]
    Disconnected,
    #[error("no queued virtual input report")]
    NoInputReport,
    #[error("virtual watchdog expired after {timeout_ms} ms")]
    WatchdogExpired { timeout_ms: u64 },
}

fn non_empty_string(
    field: &'static str,
    value: impl Into<String>,
) -> Result<String, VirtualHidError> {
    let value = value.into();
    if value.trim().is_empty() {
        Err(VirtualHidError::EmptyField { field })
    } else {
        Ok(value)
    }
}

fn collect_non_zero_lengths(
    field: &'static str,
    lengths: impl IntoIterator<Item = usize>,
) -> Result<Vec<usize>, VirtualHidError> {
    let lengths = lengths.into_iter().collect::<Vec<_>>();
    if lengths.contains(&0) {
        Err(VirtualHidError::ZeroReportLength { field })
    } else {
        Ok(lengths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> Result<VirtualHidIdentity, VirtualHidError> {
        VirtualHidIdentity::new(0x346E, 0x0014, "virtual-moza-r5")?
            .with_manufacturer("Virtual Moza")?
            .with_product_name("Virtual R5")
            .map(|identity| {
                identity
                    .with_serial_number_present(false)
                    .with_interface(0)
                    .with_usage(0x0001, 0x0004)
            })
    }

    fn sample_descriptor() -> Result<VirtualHidDescriptor, VirtualHidError> {
        let descriptor = VirtualHidDescriptor::new("0x12345678")?
            .with_input_report_lengths([7, 31])?
            .with_output_report_ids([0x20])
            .with_feature_report_ids([0x03, 0x11]);
        Ok(descriptor)
    }

    #[test]
    fn receipt_is_always_virtual_and_not_real_hardware() -> Result<(), Box<dyn std::error::Error>> {
        let replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        let receipt = replay.receipt();

        assert_eq!(receipt.hardware_source(), EvidenceSource::Virtual);
        assert!(!receipt.real_hardware_validated());

        let json = serde_json::to_string(&receipt)?;
        assert!(json.contains("\"hardware_source\":\"virtual\""));
        assert!(json.contains("\"real_hardware_validated\":false"));
        Ok(())
    }

    #[test]
    fn input_reports_replay_in_fifo_order() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        replay.queue_input_report(VirtualInputReport::new(1_000, [0x01, 0x10, 0x20, 0x30])?);
        replay.queue_input_report(VirtualInputReport::new(2_000, [0x02, 0x40, 0x50, 0x60])?);

        let first = replay.read_input_report()?;
        let second = replay.read_input_report()?;

        assert_eq!(first.report_id(), 0x01);
        assert_eq!(second.report_id(), 0x02);
        assert_eq!(replay.input_log().len(), 2);
        assert_eq!(replay.input_log()[0].event_index(), 1);
        assert_eq!(replay.input_log()[1].event_index(), 2);
        Ok(())
    }

    #[test]
    fn output_and_feature_writes_are_logged_without_hid_access() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        replay.set_timestamp_us(10_000);

        assert_eq!(replay.write_output_report(&[0x20, 0x00, 0x00])?, 3);
        assert_eq!(replay.write_feature_report(&[0x11, 0x01])?, 2);

        let log = replay.output_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].kind(), VirtualOutputKind::Output);
        assert_eq!(log[0].timestamp_us(), 10_000);
        assert_eq!(
            log[0].result(),
            &VirtualWriteResult::Written { bytes_written: 3 }
        );
        assert_eq!(log[1].kind(), VirtualOutputKind::Feature);
        Ok(())
    }

    #[test]
    fn disconnect_blocks_later_reads_and_writes() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        replay.queue_input_report(VirtualInputReport::new(1_000, [0x01, 0x10])?);
        replay.disconnect();

        assert_eq!(
            replay.read_input_report(),
            Err(VirtualHidError::Disconnected)
        );
        assert_eq!(
            replay.write_output_report(&[0x20, 0x00]),
            Err(VirtualHidError::Disconnected)
        );
        assert_eq!(replay.output_log().len(), 1);
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::Disconnect)
        );
        Ok(())
    }

    #[test]
    fn stale_descriptor_and_wrong_pid_are_recorded() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);

        replay.inject_stale_descriptor("0x87654321")?;
        replay.inject_wrong_product_id(0x0004);

        assert_eq!(
            replay.reported_descriptor().report_descriptor_crc32(),
            "0x87654321"
        );
        assert_eq!(replay.descriptor().report_descriptor_crc32(), "0x12345678");
        assert_eq!(replay.reported_device().product_id(), 0x0004);
        assert_eq!(replay.device().product_id(), 0x0014);
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::StaleDescriptor)
        );
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::WrongProductId)
        );
        Ok(())
    }

    #[test]
    fn short_reports_and_duplicate_timestamps_are_classified() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        replay.queue_input_reports([
            VirtualInputReport::new(1_000, [0x01, 0x02, 0x03])?,
            VirtualInputReport::new(1_000, [0x01, 0x02, 0x03, 0x04])?,
        ]);

        replay.read_input_report()?;
        replay.read_input_report()?;

        let input_log = replay.input_log();
        assert_eq!(input_log.len(), 2);
        assert!(input_log[0].short_report());
        assert!(!input_log[0].duplicate_timestamp());
        assert!(input_log[1].short_report());
        assert!(input_log[1].duplicate_timestamp());
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::ShortInputReport)
        );
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::DuplicateTimestamp)
        );
        Ok(())
    }

    #[test]
    fn watchdog_expiry_blocks_later_writes_and_logs_attempt() -> Result<(), VirtualHidError> {
        let mut replay = VirtualHidReplay::new(sample_identity()?, sample_descriptor()?);
        replay.expire_watchdog(100);

        assert_eq!(
            replay.write_output_report(&[0x20, 0x00, 0x00]),
            Err(VirtualHidError::WatchdogExpired { timeout_ms: 100 })
        );

        let entry = replay
            .output_log()
            .first()
            .ok_or(VirtualHidError::NoInputReport)?;
        assert_eq!(
            entry.result(),
            &VirtualWriteResult::WatchdogExpired { timeout_ms: 100 }
        );
        assert!(
            replay
                .fault_events()
                .iter()
                .any(|fault| fault.kind() == VirtualHidFaultKind::WatchdogExpired)
        );
        Ok(())
    }

    #[test]
    fn constructors_reject_empty_identity_descriptor_and_report_data() {
        assert_eq!(
            VirtualHidIdentity::new(1, 2, " "),
            Err(VirtualHidError::EmptyField {
                field: "device_key"
            })
        );
        assert_eq!(
            VirtualHidDescriptor::new(""),
            Err(VirtualHidError::EmptyField {
                field: "report_descriptor_crc32"
            })
        );
        assert_eq!(
            VirtualInputReport::new(0, []),
            Err(VirtualHidError::EmptyReport { field: "bytes" })
        );
    }

    #[test]
    fn descriptor_rejects_zero_input_report_lengths() -> Result<(), VirtualHidError> {
        let result = VirtualHidDescriptor::new("0x12345678")?.with_input_report_lengths([0, 7]);
        assert_eq!(
            result,
            Err(VirtualHidError::ZeroReportLength {
                field: "input_report_lengths"
            })
        );
        Ok(())
    }
}
