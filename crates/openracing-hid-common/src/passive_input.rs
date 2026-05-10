//! Passive HID input report decoding for fixture replay and capture validation.
//!
//! This module intentionally does not open HID devices or write reports. It
//! decodes already-captured input reports from a declarative layout so hardware
//! lanes can validate passive axis, button, and hat behavior without depending
//! on a device-family parser.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PassiveInputError {
    #[error("axis id must not be empty")]
    EmptyAxisId,

    #[error("button id must not be empty")]
    EmptyButtonId,

    #[error("hat id must not be empty")]
    EmptyHatId,

    #[error("axis `{axis_id}` has invalid logical range {logical_min}..={logical_max}")]
    InvalidAxisRange {
        axis_id: String,
        logical_min: i32,
        logical_max: i32,
    },

    #[error("button `{button_id}` has invalid bit index {bit_index}; expected 0..=7")]
    InvalidButtonBitIndex { button_id: String, bit_index: u8 },

    #[error("hat `{hat_id}` has invalid bit range offset={bit_offset} width={bit_width}")]
    InvalidHatBitRange {
        hat_id: String,
        bit_offset: u8,
        bit_width: u8,
    },

    #[error("expected report id 0x{expected:02X}, but report was empty")]
    MissingReportId { expected: u8 },

    #[error("expected report id 0x{expected:02X}, got 0x{actual:02X}")]
    UnexpectedReportId { expected: u8, actual: u8 },

    #[error("report too short for `{field_id}`: need {needed} bytes, got {actual}")]
    ReportTooShort {
        field_id: String,
        needed: usize,
        actual: usize,
    },

    #[error("field `{field_id}` offset overflows report length calculation")]
    FieldOffsetOverflow { field_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveInputLayout {
    pub report_id: Option<u8>,
    pub axes: Vec<PassiveAxisField>,
    pub buttons: Vec<PassiveButtonField>,
    pub hat: Option<PassiveHatField>,
}

impl PassiveInputLayout {
    #[must_use]
    pub fn new(report_id: Option<u8>) -> Self {
        Self {
            report_id,
            axes: Vec::new(),
            buttons: Vec::new(),
            hat: None,
        }
    }

    #[must_use]
    pub fn with_axis(mut self, axis: PassiveAxisField) -> Self {
        self.axes.push(axis);
        self
    }

    #[must_use]
    pub fn with_button(mut self, button: PassiveButtonField) -> Self {
        self.buttons.push(button);
        self
    }

    #[must_use]
    pub fn with_hat(mut self, hat: PassiveHatField) -> Self {
        self.hat = Some(hat);
        self
    }

    pub fn validate(&self) -> Result<(), PassiveInputError> {
        for axis in &self.axes {
            axis.validate()?;
        }

        for button in &self.buttons {
            button.validate()?;
        }

        if let Some(hat) = &self.hat {
            hat.validate()?;
        }

        Ok(())
    }

    #[must_use]
    pub fn minimum_report_len(&self) -> usize {
        let report_id_len = usize::from(self.report_id.is_some());
        let axis_len = self
            .axes
            .iter()
            .map(|axis| axis.offset.saturating_add(axis.width.byte_len()))
            .max()
            .unwrap_or(0);
        let button_len = self
            .buttons
            .iter()
            .map(|button| button.byte_offset.saturating_add(1))
            .max()
            .unwrap_or(0);
        let hat_len = self
            .hat
            .as_ref()
            .map_or(0, |hat| hat.byte_offset.saturating_add(1));

        report_id_len.max(axis_len).max(button_len).max(hat_len)
    }

    pub fn parse_report(&self, report: &[u8]) -> Result<PassiveInputSnapshot, PassiveInputError> {
        self.validate()?;
        self.validate_report_id(report)?;

        let mut axes = Vec::with_capacity(self.axes.len());
        for axis in &self.axes {
            axes.push(axis.parse(report)?);
        }

        let mut buttons = Vec::with_capacity(self.buttons.len());
        for button in &self.buttons {
            buttons.push(button.parse(report)?);
        }

        let hat = self.hat.as_ref().map(|hat| hat.parse(report)).transpose()?;

        Ok(PassiveInputSnapshot {
            report_id: self.report_id,
            axes,
            buttons,
            hat,
        })
    }

    fn validate_report_id(&self, report: &[u8]) -> Result<(), PassiveInputError> {
        let Some(expected) = self.report_id else {
            return Ok(());
        };

        let Some(actual) = report.first().copied() else {
            return Err(PassiveInputError::MissingReportId { expected });
        };

        if actual == expected {
            Ok(())
        } else {
            Err(PassiveInputError::UnexpectedReportId { expected, actual })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAxisField {
    pub axis_id: String,
    pub offset: usize,
    pub width: PassiveAxisWidth,
    pub endian: PassiveEndian,
    pub logical_min: i32,
    pub logical_max: i32,
}

impl PassiveAxisField {
    #[must_use]
    pub fn unsigned_8(axis_id: impl Into<String>, offset: usize) -> Self {
        Self {
            axis_id: axis_id.into(),
            offset,
            width: PassiveAxisWidth::Unsigned8,
            endian: PassiveEndian::Little,
            logical_min: 0,
            logical_max: u8::MAX.into(),
        }
    }

    #[must_use]
    pub fn signed_8(axis_id: impl Into<String>, offset: usize) -> Self {
        Self {
            axis_id: axis_id.into(),
            offset,
            width: PassiveAxisWidth::Signed8,
            endian: PassiveEndian::Little,
            logical_min: i8::MIN.into(),
            logical_max: i8::MAX.into(),
        }
    }

    #[must_use]
    pub fn unsigned_16(axis_id: impl Into<String>, offset: usize, endian: PassiveEndian) -> Self {
        Self {
            axis_id: axis_id.into(),
            offset,
            width: PassiveAxisWidth::Unsigned16,
            endian,
            logical_min: 0,
            logical_max: u16::MAX.into(),
        }
    }

    #[must_use]
    pub fn signed_16(axis_id: impl Into<String>, offset: usize, endian: PassiveEndian) -> Self {
        Self {
            axis_id: axis_id.into(),
            offset,
            width: PassiveAxisWidth::Signed16,
            endian,
            logical_min: i16::MIN.into(),
            logical_max: i16::MAX.into(),
        }
    }

    #[must_use]
    pub fn with_logical_range(mut self, logical_min: i32, logical_max: i32) -> Self {
        self.logical_min = logical_min;
        self.logical_max = logical_max;
        self
    }

    fn validate(&self) -> Result<(), PassiveInputError> {
        if self.axis_id.is_empty() {
            return Err(PassiveInputError::EmptyAxisId);
        }

        if self.logical_min >= self.logical_max {
            return Err(PassiveInputError::InvalidAxisRange {
                axis_id: self.axis_id.clone(),
                logical_min: self.logical_min,
                logical_max: self.logical_max,
            });
        }

        Ok(())
    }

    fn parse(&self, report: &[u8]) -> Result<PassiveAxisValue, PassiveInputError> {
        ensure_len(
            report,
            checked_needed_len(self.offset, self.width.byte_len(), &self.axis_id)?,
            &self.axis_id,
        )?;
        let raw = match self.width {
            PassiveAxisWidth::Unsigned8 => i32::from(report[self.offset]),
            PassiveAxisWidth::Signed8 => i32::from(i8::from_ne_bytes([report[self.offset]])),
            PassiveAxisWidth::Unsigned16 => i32::from(read_u16(report, self.offset, self.endian)),
            PassiveAxisWidth::Signed16 => i32::from(read_i16(report, self.offset, self.endian)),
        };

        Ok(PassiveAxisValue {
            axis_id: self.axis_id.clone(),
            raw,
            normalized: normalize_axis(raw, self.logical_min, self.logical_max),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassiveAxisWidth {
    Unsigned8,
    Signed8,
    Unsigned16,
    Signed16,
}

impl PassiveAxisWidth {
    #[must_use]
    pub fn byte_len(self) -> usize {
        match self {
            Self::Unsigned8 | Self::Signed8 => 1,
            Self::Unsigned16 | Self::Signed16 => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassiveEndian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveButtonField {
    pub button_id: String,
    pub byte_offset: usize,
    pub bit_index: u8,
}

impl PassiveButtonField {
    #[must_use]
    pub fn new(button_id: impl Into<String>, byte_offset: usize, bit_index: u8) -> Self {
        Self {
            button_id: button_id.into(),
            byte_offset,
            bit_index,
        }
    }

    fn validate(&self) -> Result<(), PassiveInputError> {
        if self.button_id.is_empty() {
            return Err(PassiveInputError::EmptyButtonId);
        }

        if self.bit_index > 7 {
            return Err(PassiveInputError::InvalidButtonBitIndex {
                button_id: self.button_id.clone(),
                bit_index: self.bit_index,
            });
        }

        Ok(())
    }

    fn parse(&self, report: &[u8]) -> Result<PassiveButtonValue, PassiveInputError> {
        ensure_len(
            report,
            checked_needed_len(self.byte_offset, 1, &self.button_id)?,
            &self.button_id,
        )?;
        let mask = 1_u8 << self.bit_index;

        Ok(PassiveButtonValue {
            button_id: self.button_id.clone(),
            pressed: report[self.byte_offset] & mask != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveHatField {
    pub hat_id: String,
    pub byte_offset: usize,
    pub bit_offset: u8,
    pub bit_width: u8,
    pub neutral: u8,
}

impl PassiveHatField {
    #[must_use]
    pub fn nibble(hat_id: impl Into<String>, byte_offset: usize, bit_offset: u8) -> Self {
        Self {
            hat_id: hat_id.into(),
            byte_offset,
            bit_offset,
            bit_width: 4,
            neutral: 0x0F,
        }
    }

    #[must_use]
    pub fn byte(hat_id: impl Into<String>, byte_offset: usize) -> Self {
        Self {
            hat_id: hat_id.into(),
            byte_offset,
            bit_offset: 0,
            bit_width: 8,
            neutral: 0x08,
        }
    }

    #[must_use]
    pub fn with_neutral(mut self, neutral: u8) -> Self {
        self.neutral = neutral;
        self
    }

    fn validate(&self) -> Result<(), PassiveInputError> {
        if self.hat_id.is_empty() {
            return Err(PassiveInputError::EmptyHatId);
        }

        let valid_width = (1..=8).contains(&self.bit_width);
        let valid_range = u16::from(self.bit_offset) + u16::from(self.bit_width) <= 8;
        if !valid_width || !valid_range {
            return Err(PassiveInputError::InvalidHatBitRange {
                hat_id: self.hat_id.clone(),
                bit_offset: self.bit_offset,
                bit_width: self.bit_width,
            });
        }

        Ok(())
    }

    fn parse(&self, report: &[u8]) -> Result<PassiveHatValue, PassiveInputError> {
        ensure_len(
            report,
            checked_needed_len(self.byte_offset, 1, &self.hat_id)?,
            &self.hat_id,
        )?;
        let mask = if self.bit_width == 8 {
            u8::MAX
        } else {
            (1_u8 << self.bit_width) - 1
        };
        let raw = (report[self.byte_offset] >> self.bit_offset) & mask;

        Ok(PassiveHatValue {
            hat_id: self.hat_id.clone(),
            raw,
            neutral: raw == self.neutral,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveInputSnapshot {
    pub report_id: Option<u8>,
    pub axes: Vec<PassiveAxisValue>,
    pub buttons: Vec<PassiveButtonValue>,
    pub hat: Option<PassiveHatValue>,
}

impl PassiveInputSnapshot {
    #[must_use]
    pub fn axis(&self, axis_id: &str) -> Option<&PassiveAxisValue> {
        self.axes.iter().find(|axis| axis.axis_id == axis_id)
    }

    #[must_use]
    pub fn button(&self, button_id: &str) -> Option<&PassiveButtonValue> {
        self.buttons
            .iter()
            .find(|button| button.button_id == button_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveAxisValue {
    pub axis_id: String,
    pub raw: i32,
    pub normalized: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveButtonValue {
    pub button_id: String,
    pub pressed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveHatValue {
    pub hat_id: String,
    pub raw: u8,
    pub neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveInputRangeTracker {
    axes: Vec<PassiveAxisRange>,
    buttons: Vec<PassiveButtonRange>,
    hat: Option<PassiveHatRange>,
}

impl PassiveInputRangeTracker {
    #[must_use]
    pub fn from_layout(layout: &PassiveInputLayout) -> Self {
        Self {
            axes: layout
                .axes
                .iter()
                .map(|axis| PassiveAxisRange::new(axis.axis_id.clone()))
                .collect(),
            buttons: layout
                .buttons
                .iter()
                .map(|button| PassiveButtonRange::new(button.button_id.clone()))
                .collect(),
            hat: layout
                .hat
                .as_ref()
                .map(|hat| PassiveHatRange::new(hat.hat_id.clone(), hat.neutral)),
        }
    }

    pub fn observe(&mut self, snapshot: &PassiveInputSnapshot) {
        for axis_value in &snapshot.axes {
            if let Some(axis_range) = self
                .axes
                .iter_mut()
                .find(|axis_range| axis_range.axis_id == axis_value.axis_id)
            {
                axis_range.observe(axis_value.raw);
            }
        }

        for button_value in &snapshot.buttons {
            if let Some(button_range) = self
                .buttons
                .iter_mut()
                .find(|button_range| button_range.button_id == button_value.button_id)
            {
                button_range.observe(button_value.pressed);
            }
        }

        if let (Some(tracked_hat), Some(hat_value)) = (&mut self.hat, &snapshot.hat) {
            tracked_hat.observe(hat_value.raw);
        }
    }

    #[must_use]
    pub fn axes(&self) -> &[PassiveAxisRange] {
        &self.axes
    }

    #[must_use]
    pub fn buttons(&self) -> &[PassiveButtonRange] {
        &self.buttons
    }

    #[must_use]
    pub fn hat(&self) -> Option<&PassiveHatRange> {
        self.hat.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAxisRange {
    pub axis_id: String,
    pub min_raw: Option<i32>,
    pub max_raw: Option<i32>,
}

impl PassiveAxisRange {
    #[must_use]
    pub fn new(axis_id: String) -> Self {
        Self {
            axis_id,
            min_raw: None,
            max_raw: None,
        }
    }

    fn observe(&mut self, raw: i32) {
        self.min_raw = Some(self.min_raw.map_or(raw, |current| current.min(raw)));
        self.max_raw = Some(self.max_raw.map_or(raw, |current| current.max(raw)));
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        matches!((self.min_raw, self.max_raw), (Some(min), Some(max)) if min != max)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveButtonRange {
    pub button_id: String,
    pub saw_released: bool,
    pub saw_pressed: bool,
}

impl PassiveButtonRange {
    #[must_use]
    pub fn new(button_id: String) -> Self {
        Self {
            button_id,
            saw_released: false,
            saw_pressed: false,
        }
    }

    fn observe(&mut self, pressed: bool) {
        if pressed {
            self.saw_pressed = true;
        } else {
            self.saw_released = true;
        }
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.saw_released && self.saw_pressed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveHatRange {
    pub hat_id: String,
    pub neutral: u8,
    pub saw_neutral: bool,
    pub values: Vec<u8>,
}

impl PassiveHatRange {
    #[must_use]
    pub fn new(hat_id: String, neutral: u8) -> Self {
        Self {
            hat_id,
            neutral,
            saw_neutral: false,
            values: Vec::new(),
        }
    }

    fn observe(&mut self, raw: u8) {
        if raw == self.neutral {
            self.saw_neutral = true;
        }

        if !self.values.contains(&raw) {
            self.values.push(raw);
        }
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.values.len() > 1
    }
}

fn checked_needed_len(
    offset: usize,
    width: usize,
    field_id: &str,
) -> Result<usize, PassiveInputError> {
    offset
        .checked_add(width)
        .ok_or_else(|| PassiveInputError::FieldOffsetOverflow {
            field_id: field_id.to_string(),
        })
}

fn ensure_len(report: &[u8], needed: usize, field_id: &str) -> Result<(), PassiveInputError> {
    if report.len() >= needed {
        Ok(())
    } else {
        Err(PassiveInputError::ReportTooShort {
            field_id: field_id.to_string(),
            needed,
            actual: report.len(),
        })
    }
}

fn read_u16(report: &[u8], offset: usize, endian: PassiveEndian) -> u16 {
    let bytes = [report[offset], report[offset + 1]];
    match endian {
        PassiveEndian::Little => u16::from_le_bytes(bytes),
        PassiveEndian::Big => u16::from_be_bytes(bytes),
    }
}

fn read_i16(report: &[u8], offset: usize, endian: PassiveEndian) -> i16 {
    let bytes = [report[offset], report[offset + 1]];
    match endian {
        PassiveEndian::Little => i16::from_le_bytes(bytes),
        PassiveEndian::Big => i16::from_be_bytes(bytes),
    }
}

fn normalize_axis(raw: i32, logical_min: i32, logical_max: i32) -> f32 {
    let span = (i64::from(logical_max) - i64::from(logical_min)) as f32;
    let value = (i64::from(raw) - i64::from(logical_min)) as f32 / span;
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        PassiveAxisField, PassiveAxisWidth, PassiveButtonField, PassiveEndian, PassiveHatField,
        PassiveInputError, PassiveInputLayout, PassiveInputRangeTracker,
    };

    #[test]
    fn parses_axes_buttons_and_hat() -> Result<(), Box<dyn std::error::Error>> {
        let layout = PassiveInputLayout::new(Some(0x01))
            .with_axis(PassiveAxisField::unsigned_16(
                "wheel",
                1,
                PassiveEndian::Little,
            ))
            .with_axis(PassiveAxisField::unsigned_8("throttle", 3))
            .with_button(PassiveButtonField::new("shift_up", 4, 2))
            .with_hat(PassiveHatField::nibble("hat", 5, 0));

        let snapshot = layout.parse_report(&[0x01, 0x34, 0x12, 0x80, 0b0000_0100, 0x03])?;

        let Some(wheel) = snapshot.axis("wheel") else {
            return Err("missing wheel axis".into());
        };
        assert_eq!(wheel.raw, 0x1234);
        assert!((wheel.normalized - 0.071_107_04).abs() < 0.000_01);

        let Some(throttle) = snapshot.axis("throttle") else {
            return Err("missing throttle axis".into());
        };
        assert_eq!(throttle.raw, 0x80);
        assert!((throttle.normalized - 0.501_960_8).abs() < 0.000_01);

        let Some(button) = snapshot.button("shift_up") else {
            return Err("missing shift_up button".into());
        };
        assert!(button.pressed);

        let Some(hat) = snapshot.hat else {
            return Err("missing hat".into());
        };
        assert_eq!(hat.raw, 0x03);
        assert!(!hat.neutral);

        Ok(())
    }

    #[test]
    fn rejects_wrong_report_id() {
        let layout = PassiveInputLayout::new(Some(0x01));

        let err = layout.parse_report(&[0x02]);

        assert_eq!(
            err,
            Err(PassiveInputError::UnexpectedReportId {
                expected: 0x01,
                actual: 0x02,
            })
        );
    }

    #[test]
    fn rejects_short_report() {
        let layout = PassiveInputLayout::new(Some(0x01)).with_axis(PassiveAxisField::signed_16(
            "wheel",
            1,
            PassiveEndian::Little,
        ));

        let err = layout.parse_report(&[0x01, 0x00]);

        assert_eq!(
            err,
            Err(PassiveInputError::ReportTooShort {
                field_id: "wheel".to_string(),
                needed: 3,
                actual: 2,
            })
        );
    }

    #[test]
    fn parses_signed_axes_and_clamps_normalization() -> Result<(), Box<dyn std::error::Error>> {
        let layout = PassiveInputLayout::new(None)
            .with_axis(PassiveAxisField::signed_8("x", 0))
            .with_axis(
                PassiveAxisField::signed_16("y", 1, PassiveEndian::Big)
                    .with_logical_range(-100, 100),
            );

        let snapshot = layout.parse_report(&[0x80, 0x7F, 0xFF])?;

        let Some(x) = snapshot.axis("x") else {
            return Err("missing x axis".into());
        };
        assert_eq!(x.raw, -128);
        assert_eq!(x.normalized, 0.0);

        let Some(y) = snapshot.axis("y") else {
            return Err("missing y axis".into());
        };
        assert_eq!(y.raw, i32::from(i16::MAX));
        assert_eq!(y.normalized, 1.0);

        Ok(())
    }

    #[test]
    fn rejects_invalid_layouts() {
        let empty_axis =
            PassiveInputLayout::new(None).with_axis(PassiveAxisField::unsigned_8("", 0));
        assert_eq!(empty_axis.validate(), Err(PassiveInputError::EmptyAxisId));

        let bad_range = PassiveInputLayout::new(None)
            .with_axis(PassiveAxisField::unsigned_8("x", 0).with_logical_range(10, 10));
        assert_eq!(
            bad_range.validate(),
            Err(PassiveInputError::InvalidAxisRange {
                axis_id: "x".to_string(),
                logical_min: 10,
                logical_max: 10,
            })
        );

        let bad_button =
            PassiveInputLayout::new(None).with_button(PassiveButtonField::new("a", 0, 8));
        assert_eq!(
            bad_button.validate(),
            Err(PassiveInputError::InvalidButtonBitIndex {
                button_id: "a".to_string(),
                bit_index: 8,
            })
        );

        let bad_hat = PassiveInputLayout::new(None).with_hat(PassiveHatField {
            hat_id: "hat".to_string(),
            byte_offset: 0,
            bit_offset: 6,
            bit_width: 4,
            neutral: 0x0F,
        });
        assert_eq!(
            bad_hat.validate(),
            Err(PassiveInputError::InvalidHatBitRange {
                hat_id: "hat".to_string(),
                bit_offset: 6,
                bit_width: 4,
            })
        );
    }

    #[test]
    fn tracks_axis_button_and_hat_ranges() -> Result<(), Box<dyn std::error::Error>> {
        let layout = PassiveInputLayout::new(Some(0x01))
            .with_axis(PassiveAxisField::unsigned_8("pedal", 1))
            .with_button(PassiveButtonField::new("button_a", 2, 0))
            .with_hat(PassiveHatField::nibble("hat", 3, 0));
        let mut tracker = PassiveInputRangeTracker::from_layout(&layout);

        for report in [[0x01, 0x00, 0x00, 0x0F], [0x01, 0xFF, 0x01, 0x02]] {
            let snapshot = layout.parse_report(&report)?;
            tracker.observe(&snapshot);
        }

        let Some(axis_range) = tracker.axes().first() else {
            return Err("missing axis range".into());
        };
        assert_eq!(axis_range.min_raw, Some(0));
        assert_eq!(axis_range.max_raw, Some(255));
        assert!(axis_range.changed());

        let Some(button_range) = tracker.buttons().first() else {
            return Err("missing button range".into());
        };
        assert!(button_range.changed());

        let Some(hat) = tracker.hat() else {
            return Err("missing hat range".into());
        };
        assert_eq!(hat.neutral, 0x0F);
        assert!(hat.changed());
        assert!(hat.saw_neutral);
        assert_eq!(hat.values, vec![0x0F, 0x02]);

        Ok(())
    }

    #[test]
    fn serializes_layout_and_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let layout = PassiveInputLayout::new(Some(0x01))
            .with_axis(PassiveAxisField {
                axis_id: "x".to_string(),
                offset: 1,
                width: PassiveAxisWidth::Unsigned8,
                endian: PassiveEndian::Little,
                logical_min: 0,
                logical_max: 100,
            })
            .with_button(PassiveButtonField::new("a", 2, 1))
            .with_hat(PassiveHatField::byte("hat", 3));
        let serialized = serde_json::to_string(&layout)?;
        let round_tripped: PassiveInputLayout = serde_json::from_str(&serialized)?;
        assert_eq!(round_tripped, layout);

        let snapshot = layout.parse_report(&[0x01, 50, 0b0000_0010, 0x08])?;
        let serialized_snapshot = serde_json::to_string(&snapshot)?;
        let round_tripped_snapshot: super::PassiveInputSnapshot =
            serde_json::from_str(&serialized_snapshot)?;
        assert_eq!(round_tripped_snapshot, snapshot);

        Ok(())
    }
}
