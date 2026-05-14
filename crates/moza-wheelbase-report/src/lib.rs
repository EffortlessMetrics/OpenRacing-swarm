//! Moza wheelbase aggregated input report parsing primitives.
//!
//! This crate is intentionally small and I/O-free so protocol crates can
//! consume capture-validated parsing logic without pulling runtime concerns.

#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

/// [ADR-0007]: Multi-Vendor HID Protocol Architecture
/// This crate follows the "SRP Microcrate" pattern for vendor-specific HID protocols.
/// Report ID and byte offsets for wheelbase-aggregated input reports.
pub mod input_report {
    /// HID report ID for the aggregated input report
    pub const REPORT_ID: u8 = 0x01;
    /// Byte offset where the 16-bit LE steering axis begins
    pub const STEERING_START: usize = 1;
    /// Byte offset where the 16-bit LE throttle axis begins
    pub const THROTTLE_START: usize = 3;
    /// Byte offset where the 16-bit LE brake axis begins
    pub const BRAKE_START: usize = 5;
    /// Byte offset where the 16-bit LE clutch axis begins
    pub const CLUTCH_START: usize = 7;
    /// Byte offset where the 16-bit LE handbrake axis begins
    pub const HANDBRAKE_START: usize = 9;
    /// Byte offset where the button bitmask begins
    pub const BUTTONS_START: usize = 11;
    /// Number of bytes in the button bitmask (128 buttons)
    pub const BUTTONS_LEN: usize = 16;
    /// Byte offset of the hat switch / D-pad value
    pub const HAT_START: usize = BUTTONS_START + BUTTONS_LEN;
    /// Byte offset of the funky-switch / rim-identifier byte
    pub const FUNKY_START: usize = HAT_START + 1;
    /// Byte offset where rotary encoder bytes begin
    pub const ROTARY_START: usize = FUNKY_START + 1;
    /// Number of rotary encoder bytes
    pub const ROTARY_LEN: usize = 2;

    /// Observed report length for the live R5 V1 wheelbase + KS aggregated path.
    ///
    /// This extended path keeps steering at byte 1, but carries additional
    /// axis-like values before the packed button/control surface.
    pub const R5_V1_EXTENDED_REPORT_LEN: usize = 42;
    /// Byte offset where the live R5 V1 extended axis block exposes the first
    /// moving pedal-like slot observed during SR-P-through-wheelbase capture.
    pub const R5_V1_EXTENDED_AXIS0_START: usize = 11;
    /// Byte offset for the second observed live R5 V1 extended axis slot.
    pub const R5_V1_EXTENDED_AXIS1_START: usize = 13;
    /// Byte offset for the third observed live R5 V1 extended axis slot.
    pub const R5_V1_EXTENDED_AXIS2_START: usize = 15;
    /// Byte offset for an axis-like KS control observed in live R5 V1 + KS captures.
    pub const R5_V1_EXTENDED_KS_AXIS0_START: usize = 3;
    /// Byte offset where isolated live R5 V1 through-hub throttle captures moved.
    pub const R5_V1_EXTENDED_THROTTLE_START: usize = 5;
    /// Byte offset where the live R5 V1 + KS packed control bytes begin.
    pub const R5_V1_EXTENDED_BUTTONS_START: usize = 17;
    /// Byte offset for the live R5 V1 + KS direction byte observed during KS capture.
    pub const R5_V1_EXTENDED_HAT_START: usize = 28;
    /// Byte offset for the first auxiliary live R5 V1 hub signal observed in
    /// isolated through-wheelbase control captures.
    ///
    /// This is intentionally not assigned a pedal/rim semantic label yet. It is
    /// useful as generic passive evidence until isolated captures and descriptor
    /// evidence prove a stable control role.
    pub const R5_V1_EXTENDED_AUX0_START: usize = 34;
    /// Byte offset for the second auxiliary live R5 V1 hub signal observed in
    /// isolated through-wheelbase control captures.
    pub const R5_V1_EXTENDED_AUX1_START: usize = 36;
}

/// Minimum bytes required for a valid wheelbase report containing steering,
/// throttle, and brake axes.
pub const MIN_REPORT_LEN: usize = input_report::BRAKE_START + 2;

/// Lightweight parsed view over a wheelbase-style input report.
#[derive(Debug, Clone, Copy)]
pub struct RawWheelbaseReport<'a> {
    report: &'a [u8],
}

impl<'a> RawWheelbaseReport<'a> {
    /// Construct a borrowed report view without validation.
    ///
    /// Prefer [`parse_wheelbase_report`] when report ID/length validation is required.
    pub fn new(report: &'a [u8]) -> Self {
        Self { report }
    }

    /// Returns the HID report ID (first byte), or `0` if the slice is empty.
    pub fn report_id(&self) -> u8 {
        self.report.first().copied().unwrap_or(0)
    }

    /// Returns the raw byte slice backing this report.
    pub fn report_bytes(&self) -> &'a [u8] {
        self.report
    }

    /// Returns a single byte at `offset`, or `None` if out of range.
    pub fn byte(&self, offset: usize) -> Option<u8> {
        self.report.get(offset).copied()
    }

    /// Reads a little-endian `u16` axis value starting at `start`.
    ///
    /// Returns `None` if there are fewer than 2 bytes at that offset.
    pub fn axis_u16_le(&self, start: usize) -> Option<u16> {
        parse_axis(self.report, start)
    }

    /// Reads a little-endian `u16` axis value, returning `0` when bytes are missing.
    pub fn axis_u16_or_zero(&self, start: usize) -> u16 {
        self.axis_u16_le(start).unwrap_or(0)
    }
}

/// Raw wheelbase pedal samples from an aggregated report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelbasePedalAxesRaw {
    /// Raw throttle axis value (0–65535)
    pub throttle: u16,
    /// Raw brake axis value (0–65535)
    pub brake: u16,
    /// Raw clutch axis value, if reported by hardware
    pub clutch: Option<u16>,
    /// Raw handbrake axis value, if reported by hardware
    pub handbrake: Option<u16>,
}

/// Raw wheelbase input sample extracted from a single report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelbaseInputRaw {
    /// Raw steering axis value (0–65535, center ≈ 32768)
    pub steering: u16,
    /// Parsed pedal axis snapshot
    pub pedals: WheelbasePedalAxesRaw,
    /// Button bitmask bytes (up to 128 buttons, 1 bit each)
    pub buttons: [u8; input_report::BUTTONS_LEN],
    /// Hat switch / D-pad position (vendor-specific encoding)
    pub hat: u8,
    /// Vendor-specific byte immediately after `hat`.
    ///
    /// OpenRacing currently treats this as an opaque discriminator; some firmwares
    /// appear to use it as a rim identifier and it is used to gate rim-specific parsing.
    pub funky: u8,
    /// Rotary encoder raw bytes
    pub rotary: [u8; input_report::ROTARY_LEN],
}

/// Parse a little-endian `u16` axis from `report` at `start`.
///
/// NOTE: This helper is intentionally duplicated in other tiny protocol microcrates
/// (e.g. `racing-wheel-hbp`) to keep them dependency-minimal. Keep implementations in sync.
pub fn parse_axis(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

/// Returns true for the live 42-byte R5 V1 aggregated layout observed during
/// passive KS/SR-P hardware captures.
///
/// Older synthetic fixtures sometimes use padded 64-byte buffers with legacy
/// offsets, so report length alone is intentionally not enough to select this
/// layout. The live layout consistently carries `0x08` in the first packed
/// control byte while preserving additional control bits in the same byte.
pub fn looks_like_live_r5_v1_extended_report(report: &[u8]) -> bool {
    report.len() == input_report::R5_V1_EXTENDED_REPORT_LEN
        && report.first().copied() == Some(input_report::REPORT_ID)
        && report
            .get(input_report::R5_V1_EXTENDED_BUTTONS_START)
            .copied()
            .is_some_and(|value| value & 0x08 == 0x08)
}

#[derive(Debug, Clone, Copy)]
struct WheelbaseInputLayout {
    throttle_start: Option<usize>,
    brake_start: Option<usize>,
    clutch_start: Option<usize>,
    handbrake_start: Option<usize>,
    buttons_start: usize,
    hat_start: usize,
    funky_start: Option<usize>,
    rotary_start: Option<usize>,
}

impl WheelbaseInputLayout {
    const LEGACY: Self = Self {
        throttle_start: Some(input_report::THROTTLE_START),
        brake_start: Some(input_report::BRAKE_START),
        clutch_start: Some(input_report::CLUTCH_START),
        handbrake_start: Some(input_report::HANDBRAKE_START),
        buttons_start: input_report::BUTTONS_START,
        hat_start: input_report::HAT_START,
        funky_start: Some(input_report::FUNKY_START),
        rotary_start: Some(input_report::ROTARY_START),
    };

    const R5_V1_EXTENDED: Self = Self {
        throttle_start: Some(input_report::R5_V1_EXTENDED_THROTTLE_START),
        brake_start: None,
        clutch_start: None,
        handbrake_start: None,
        buttons_start: input_report::R5_V1_EXTENDED_BUTTONS_START,
        hat_start: input_report::R5_V1_EXTENDED_HAT_START,
        funky_start: None,
        rotary_start: None,
    };
}

fn wheelbase_input_layout(report: &RawWheelbaseReport<'_>) -> WheelbaseInputLayout {
    if looks_like_live_r5_v1_extended_report(report.report_bytes()) {
        WheelbaseInputLayout::R5_V1_EXTENDED
    } else {
        WheelbaseInputLayout::LEGACY
    }
}

fn parse_wheelbase_pedal_axes_from_report(
    report: &RawWheelbaseReport<'_>,
) -> Option<WheelbasePedalAxesRaw> {
    let layout = wheelbase_input_layout(report);
    let throttle = layout
        .throttle_start
        .and_then(|start| report.axis_u16_le(start))
        .unwrap_or(0);
    let brake = layout
        .brake_start
        .and_then(|start| report.axis_u16_le(start))
        .unwrap_or(0);
    let clutch = layout
        .clutch_start
        .and_then(|start| report.axis_u16_le(start));
    let handbrake = layout
        .handbrake_start
        .and_then(|start| report.axis_u16_le(start));

    Some(WheelbasePedalAxesRaw {
        throttle,
        brake,
        clutch,
        handbrake,
    })
}

/// Parse a wheelbase input report into a lightweight borrowed view.
///
/// Returns `None` unless:
/// - report ID is `input_report::REPORT_ID`
/// - report length is at least `MIN_REPORT_LEN`
pub fn parse_wheelbase_report(report: &[u8]) -> Option<RawWheelbaseReport<'_>> {
    if report.first().copied() != Some(input_report::REPORT_ID) {
        return None;
    }
    if report.len() < MIN_REPORT_LEN {
        return None;
    }
    Some(RawWheelbaseReport::new(report))
}

/// Parse wheelbase-aggregated pedal axes.
pub fn parse_wheelbase_pedal_axes(report: &[u8]) -> Option<WheelbasePedalAxesRaw> {
    let report = parse_wheelbase_report(report)?;
    parse_wheelbase_pedal_axes_from_report(&report)
}

/// Parse a full wheelbase-aggregated input report.
///
/// Optional controls (clutch, handbrake, buttons, hat, funky, rotary) are
/// zero-filled when their bytes are absent.
pub fn parse_wheelbase_input_report(report: &[u8]) -> Option<WheelbaseInputRaw> {
    let report = parse_wheelbase_report(report)?;
    let layout = wheelbase_input_layout(&report);
    let steering = report.axis_u16_le(input_report::STEERING_START)?;
    let pedals = parse_wheelbase_pedal_axes_from_report(&report)?;

    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    let bytes = report.report_bytes();
    if bytes.len() > layout.buttons_start {
        let end = bytes
            .len()
            .min(layout.buttons_start + input_report::BUTTONS_LEN);
        let count = end - layout.buttons_start;
        buttons[..count].copy_from_slice(&bytes[layout.buttons_start..end]);
    }

    let hat = report.byte(layout.hat_start).unwrap_or(0);
    let funky = layout
        .funky_start
        .and_then(|start| report.byte(start))
        .unwrap_or(0);

    let mut rotary = [0u8; input_report::ROTARY_LEN];
    if let Some(rotary_start) = layout.rotary_start
        && bytes.len() > rotary_start
    {
        let end = bytes.len().min(rotary_start + input_report::ROTARY_LEN);
        let count = end - rotary_start;
        rotary[..count].copy_from_slice(&bytes[rotary_start..end]);
    }

    Some(WheelbaseInputRaw {
        steering,
        pedals,
        buttons,
        hat,
        funky,
        rotary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wheelbase_report_rejects_non_input_id() {
        let report = [0x02u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(parse_wheelbase_report(&report).map(|r| r.report_id()), None);
    }

    #[test]
    fn parse_wheelbase_report_rejects_short_input() {
        let report = [input_report::REPORT_ID, 0x00, 0x80, 0x01, 0x00, 0x02];
        assert_eq!(parse_wheelbase_report(&report).map(|r| r.report_id()), None);
    }

    #[test]
    fn parse_wheelbase_pedal_axes_reads_optional_axes() -> Result<(), Box<dyn std::error::Error>> {
        let report = [
            input_report::REPORT_ID,
            0x00,
            0x80,
            0x34,
            0x12,
            0x78,
            0x56,
            0xBC,
            0x9A,
            0xEF,
            0xCD,
        ];

        let parsed =
            parse_wheelbase_pedal_axes(&report).ok_or("expected wheelbase pedal axis parse")?;

        assert_eq!(parsed.throttle, 0x1234);
        assert_eq!(parsed.brake, 0x5678);
        assert_eq!(parsed.clutch, Some(0x9ABC));
        assert_eq!(parsed.handbrake, Some(0xCDEF));
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_zero_fills_missing_controls() -> Result<(), Box<dyn std::error::Error>>
    {
        let report = [input_report::REPORT_ID, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        let parsed = parse_wheelbase_input_report(&report)
            .ok_or("expected wheelbase input parse for required fields")?;

        assert_eq!(parsed.steering, 0x2211);
        assert_eq!(parsed.pedals.throttle, 0x4433);
        assert_eq!(parsed.pedals.brake, 0x6655);
        assert_eq!(parsed.pedals.clutch, None);
        assert_eq!(parsed.pedals.handbrake, None);
        assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
        assert_eq!(parsed.hat, 0);
        assert_eq!(parsed.funky, 0);
        assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_preserves_partial_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0u8; input_report::BUTTONS_START + 3];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x2211u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x4433u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x6655u16.to_le_bytes());
        report[input_report::BUTTONS_START] = 0xA1;
        report[input_report::BUTTONS_START + 1] = 0xB2;
        report[input_report::BUTTONS_START + 2] = 0xC3;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected partial wheelbase parse")?;

        assert_eq!(parsed.buttons[0], 0xA1);
        assert_eq!(parsed.buttons[1], 0xB2);
        assert_eq!(parsed.buttons[2], 0xC3);
        assert_eq!(parsed.buttons[3..], [0u8; input_report::BUTTONS_LEN - 3]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_reads_full_length_controls() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x2211u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x4433u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x6655u16.to_le_bytes());
        report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
            .copy_from_slice(&0x8877u16.to_le_bytes());
        report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
            .copy_from_slice(&0xAA99u16.to_le_bytes());

        let mut expected_buttons = [0u8; input_report::BUTTONS_LEN];
        for (i, button) in expected_buttons.iter_mut().enumerate() {
            *button = i as u8;
            report[input_report::BUTTONS_START + i] = *button;
        }

        report[input_report::HAT_START] = 0x04;
        report[input_report::FUNKY_START] = 0x05;
        report[input_report::ROTARY_START] = 0x19;
        report[input_report::ROTARY_START + 1] = 0x64;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected full-length wheelbase parse")?;

        assert_eq!(parsed.steering, 0x2211);
        assert_eq!(parsed.pedals.throttle, 0x4433);
        assert_eq!(parsed.pedals.brake, 0x6655);
        assert_eq!(parsed.pedals.clutch, Some(0x8877));
        assert_eq!(parsed.pedals.handbrake, Some(0xAA99));
        assert_eq!(parsed.buttons, expected_buttons);
        assert_eq!(parsed.hat, 0x04);
        assert_eq!(parsed.funky, 0x05);
        assert_eq!(parsed.rotary, [0x19, 0x64]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_reads_live_r5_v1_extended_controls()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0u8; input_report::R5_V1_EXTENDED_REPORT_LEN];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x7A37u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_KS_AXIS0_START
            ..input_report::R5_V1_EXTENDED_KS_AXIS0_START + 2]
            .copy_from_slice(&0x1234u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_THROTTLE_START
            ..input_report::R5_V1_EXTENDED_THROTTLE_START + 2]
            .copy_from_slice(&0x3456u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS0_START
            ..input_report::R5_V1_EXTENDED_AXIS0_START + 2]
            .copy_from_slice(&0x5678u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS1_START
            ..input_report::R5_V1_EXTENDED_AXIS1_START + 2]
            .copy_from_slice(&0x9ABCu16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS2_START
            ..input_report::R5_V1_EXTENDED_AXIS2_START + 2]
            .copy_from_slice(&0xDEF0u16.to_le_bytes());
        report
            [input_report::R5_V1_EXTENDED_AUX0_START..input_report::R5_V1_EXTENDED_AUX0_START + 2]
            .copy_from_slice(&0x2468u16.to_le_bytes());
        report
            [input_report::R5_V1_EXTENDED_AUX1_START..input_report::R5_V1_EXTENDED_AUX1_START + 2]
            .copy_from_slice(&0x1357u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_BUTTONS_START] = 0x08;
        report[input_report::R5_V1_EXTENDED_BUTTONS_START + 1] = 0x04;
        report[input_report::R5_V1_EXTENDED_BUTTONS_START + 10] = 0x80;
        report[input_report::R5_V1_EXTENDED_HAT_START] = 0x03;

        let parsed = parse_wheelbase_input_report(&report)
            .ok_or("expected live R5 V1 extended input parse")?;

        assert_eq!(parsed.steering, 0x7A37);
        assert_eq!(
            parse_axis(&report, input_report::R5_V1_EXTENDED_AXIS0_START),
            Some(0x5678)
        );
        assert_eq!(
            parse_axis(&report, input_report::R5_V1_EXTENDED_AXIS1_START),
            Some(0x9ABC)
        );
        assert_eq!(
            parse_axis(&report, input_report::R5_V1_EXTENDED_AXIS2_START),
            Some(0xDEF0)
        );
        assert_eq!(
            parse_axis(&report, input_report::R5_V1_EXTENDED_AUX0_START),
            Some(0x2468)
        );
        assert_eq!(
            parse_axis(&report, input_report::R5_V1_EXTENDED_AUX1_START),
            Some(0x1357)
        );
        assert_eq!(parsed.pedals.throttle, 0x3456);
        assert_eq!(parsed.pedals.brake, 0);
        assert_eq!(parsed.pedals.clutch, None);
        assert_eq!(parsed.pedals.handbrake, None);
        assert_eq!(parsed.buttons[0], 0x08);
        assert_eq!(parsed.buttons[1], 0x04);
        assert_eq!(parsed.buttons[10], 0x80);
        assert_eq!(parsed.hat, 0x03);
        assert_eq!(parsed.funky, 0x00);
        assert_eq!(parsed.rotary, [0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn padded_legacy_report_does_not_select_live_r5_v1_extended_layout()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0u8; 64];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x8000u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x1234u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x5678u16.to_le_bytes());

        let parsed = parse_wheelbase_input_report(&report)
            .ok_or("expected padded legacy report to parse")?;

        assert!(!looks_like_live_r5_v1_extended_report(&report));
        assert_eq!(parsed.pedals.throttle, 0x1234);
        assert_eq!(parsed.pedals.brake, 0x5678);
        Ok(())
    }

    #[test]
    fn parse_axis_returns_none_when_exactly_at_boundary() {
        // A 1-byte slice can't hold a u16 starting at offset 0
        let report = [0x01u8];
        assert_eq!(parse_axis(&report, 0), None);
    }

    #[test]
    fn parse_axis_returns_none_for_empty_slice() {
        assert_eq!(parse_axis(&[], 0), None);
    }

    #[test]
    fn parse_axis_boundary_values() {
        let min_report = [input_report::REPORT_ID, 0x00, 0x00];
        assert_eq!(parse_axis(&min_report, 1), Some(0u16));

        let max_report = [input_report::REPORT_ID, 0xFF, 0xFF];
        assert_eq!(parse_axis(&max_report, 1), Some(u16::MAX));
    }

    #[test]
    fn parse_wheelbase_report_accepts_minimal_valid_report() {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = input_report::REPORT_ID;
        let parsed = parse_wheelbase_report(&report);
        assert!(parsed.is_some());
        assert_eq!(parsed.map(|r| r.report_id()), Some(input_report::REPORT_ID));
    }

    #[test]
    fn axis_u16_or_zero_returns_zero_on_missing_bytes() {
        let report = [input_report::REPORT_ID, 0xAB];
        let view = RawWheelbaseReport::new(&report);
        // offset 5 is beyond the 2-byte slice
        assert_eq!(view.axis_u16_or_zero(5), 0);
    }

    #[test]
    fn raw_report_byte_accessor() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x01, 0xAA, 0xBB, 0xCC];
        let view = RawWheelbaseReport::new(&data);
        assert_eq!(view.byte(0), Some(0x01));
        assert_eq!(view.byte(1), Some(0xAA));
        assert_eq!(view.byte(4), None);
        Ok(())
    }

    #[test]
    fn raw_report_report_bytes_returns_full_slice() {
        let data = [0x01, 0x02, 0x03];
        let view = RawWheelbaseReport::new(&data);
        assert_eq!(view.report_bytes(), &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn raw_report_id_defaults_to_zero_on_empty() {
        let view = RawWheelbaseReport::new(&[]);
        assert_eq!(view.report_id(), 0);
    }

    #[test]
    fn parse_wheelbase_report_rejects_empty_input() {
        assert!(parse_wheelbase_report(&[]).is_none());
    }

    #[test]
    fn parse_wheelbase_pedal_axes_returns_none_for_wrong_id() {
        let report = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(parse_wheelbase_pedal_axes(&report).is_none());
    }

    #[test]
    fn wheelbase_pedal_axes_raw_eq() {
        let a = WheelbasePedalAxesRaw {
            throttle: 100,
            brake: 200,
            clutch: Some(300),
            handbrake: None,
        };
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn wheelbase_input_raw_eq() {
        let a = WheelbaseInputRaw {
            steering: 0x1234,
            pedals: WheelbasePedalAxesRaw {
                throttle: 100,
                brake: 200,
                clutch: None,
                handbrake: None,
            },
            buttons: [0u8; input_report::BUTTONS_LEN],
            hat: 0,
            funky: 0,
            rotary: [0u8; input_report::ROTARY_LEN],
        };
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn input_report_constants_are_consistent() {
        // Use const assertions for compile-time-known values
        const _: () = assert!(input_report::STEERING_START < input_report::THROTTLE_START);
        const _: () = assert!(input_report::THROTTLE_START < input_report::BRAKE_START);
        const _: () = assert!(input_report::BRAKE_START < input_report::CLUTCH_START);
        const _: () = assert!(input_report::CLUTCH_START < input_report::HANDBRAKE_START);
        const _: () = assert!(input_report::HANDBRAKE_START < input_report::BUTTONS_START);
        assert_eq!(
            input_report::HAT_START,
            input_report::BUTTONS_START + input_report::BUTTONS_LEN
        );
        assert_eq!(input_report::FUNKY_START, input_report::HAT_START + 1);
        assert_eq!(input_report::ROTARY_START, input_report::FUNKY_START + 1);
    }

    #[test]
    fn min_report_len_matches_brake_end() {
        assert_eq!(MIN_REPORT_LEN, input_report::BRAKE_START + 2);
    }

    // --- Round-trip encoding tests ---

    #[test]
    fn full_report_encoding_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let steering: u16 = 0xBEEF;
        let throttle: u16 = 0xCAFE;
        let brake: u16 = 0xDEAD;
        let clutch: u16 = 0xFACE;
        let handbrake: u16 = 0x1234;

        let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&steering.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&throttle.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&brake.to_le_bytes());
        report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
            .copy_from_slice(&clutch.to_le_bytes());
        report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
            .copy_from_slice(&handbrake.to_le_bytes());
        report[input_report::HAT_START] = 0x07;
        report[input_report::FUNKY_START] = 0x0A;
        report[input_report::ROTARY_START] = 0x55;
        report[input_report::ROTARY_START + 1] = 0xAA;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected full round-trip parse")?;
        assert_eq!(parsed.steering, steering);
        assert_eq!(parsed.pedals.throttle, throttle);
        assert_eq!(parsed.pedals.brake, brake);
        assert_eq!(parsed.pedals.clutch, Some(clutch));
        assert_eq!(parsed.pedals.handbrake, Some(handbrake));
        assert_eq!(parsed.hat, 0x07);
        assert_eq!(parsed.funky, 0x0A);
        assert_eq!(parsed.rotary, [0x55, 0xAA]);
        Ok(())
    }

    #[test]
    fn pedal_axes_encoding_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let throttle: u16 = 0x1111;
        let brake: u16 = 0x2222;
        let clutch: u16 = 0x3333;
        let handbrake: u16 = 0x4444;

        let mut report = [0u8; input_report::HANDBRAKE_START + 2];
        report[0] = input_report::REPORT_ID;
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&throttle.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&brake.to_le_bytes());
        report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
            .copy_from_slice(&clutch.to_le_bytes());
        report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
            .copy_from_slice(&handbrake.to_le_bytes());

        let parsed =
            parse_wheelbase_pedal_axes(&report).ok_or("expected pedal axes round-trip parse")?;
        assert_eq!(parsed.throttle, throttle);
        assert_eq!(parsed.brake, brake);
        assert_eq!(parsed.clutch, Some(clutch));
        assert_eq!(parsed.handbrake, Some(handbrake));
        Ok(())
    }

    // --- Boundary value tests ---

    #[test]
    fn parse_axis_at_usize_max_offset_returns_none() {
        let report = [0x00, 0x00];
        assert_eq!(parse_axis(&report, usize::MAX), None);
    }

    #[test]
    fn parse_wheelbase_report_one_byte_short_of_min_rejected() {
        let report = [0u8; MIN_REPORT_LEN - 1];
        // First byte is 0x00, not REPORT_ID, but even with correct ID it's too short
        let mut report_with_id = vec![0u8; MIN_REPORT_LEN - 1];
        report_with_id[0] = input_report::REPORT_ID;
        assert!(parse_wheelbase_report(&report).is_none());
        assert!(parse_wheelbase_report(&report_with_id).is_none());
    }

    #[test]
    fn parse_wheelbase_report_id_zero_rejected() {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x00;
        assert!(parse_wheelbase_report(&report).is_none());
    }

    #[test]
    fn parse_wheelbase_all_ff_axes() -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0xFFu8; MIN_REPORT_LEN];
        report[0] = input_report::REPORT_ID;
        let parsed =
            parse_wheelbase_pedal_axes(&report).ok_or("expected parse for 0xFF-filled axes")?;
        assert_eq!(parsed.throttle, u16::MAX);
        assert_eq!(parsed.brake, u16::MAX);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_all_zero_axes() -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0x00u8; MIN_REPORT_LEN];
        report[0] = input_report::REPORT_ID;
        let parsed =
            parse_wheelbase_pedal_axes(&report).ok_or("expected parse for zero-filled axes")?;
        assert_eq!(parsed.throttle, 0);
        assert_eq!(parsed.brake, 0);
        Ok(())
    }

    // --- Field extraction: partial optional axes ---

    #[test]
    fn parse_wheelbase_pedal_axes_clutch_present_handbrake_absent()
    -> Result<(), Box<dyn std::error::Error>> {
        // Report long enough for clutch (offset 7..9) but not handbrake (offset 9..11)
        let mut report = [0u8; input_report::HANDBRAKE_START];
        report[0] = input_report::REPORT_ID;
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x1111u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x2222u16.to_le_bytes());
        report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
            .copy_from_slice(&0x3333u16.to_le_bytes());

        let parsed = parse_wheelbase_pedal_axes(&report)
            .ok_or("expected parse with clutch but no handbrake")?;
        assert_eq!(parsed.throttle, 0x1111);
        assert_eq!(parsed.brake, 0x2222);
        assert_eq!(parsed.clutch, Some(0x3333));
        assert_eq!(parsed.handbrake, None);
        Ok(())
    }

    // --- Status byte interpretation: hat/funky/rotary edge cases ---

    #[test]
    fn parse_wheelbase_input_hat_funky_without_rotary() -> Result<(), Box<dyn std::error::Error>> {
        // Report long enough for hat and funky but not rotary
        let mut report = [0u8; input_report::ROTARY_START];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x1000u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x2000u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x3000u16.to_le_bytes());
        report[input_report::HAT_START] = 0x05;
        report[input_report::FUNKY_START] = 0x0B;

        let parsed = parse_wheelbase_input_report(&report)
            .ok_or("expected parse with hat/funky but no rotary")?;
        assert_eq!(parsed.hat, 0x05);
        assert_eq!(parsed.funky, 0x0B);
        assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_partial_rotary() -> Result<(), Box<dyn std::error::Error>> {
        // Report has one rotary byte but not the second
        let mut report = [0u8; input_report::ROTARY_START + 1];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x1000u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x2000u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x3000u16.to_le_bytes());
        report[input_report::ROTARY_START] = 0x77;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected parse with partial rotary")?;
        assert_eq!(parsed.rotary, [0x77, 0x00]);
        Ok(())
    }

    #[test]
    fn raw_report_axis_u16_le_at_various_offsets() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x01, 0xAA, 0xBB, 0xCC, 0xDD];
        let view = RawWheelbaseReport::new(&data);
        assert_eq!(view.axis_u16_le(0), Some(0xAA01));
        assert_eq!(view.axis_u16_le(1), Some(0xBBAA));
        assert_eq!(view.axis_u16_le(3), Some(0xDDCC));
        assert_eq!(view.axis_u16_le(4), None);
        Ok(())
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_parse_axis_round_trips_any_le_u16(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let expected = u16::from_le_bytes([lo, hi]);
            let buf = [lo, hi];
            prop_assert_eq!(parse_axis(&buf, 0), Some(expected));
        }

        #[test]
        fn prop_parse_axis_offset_oob_returns_none(
            len in 0usize..=8usize,
            start in 0usize..=8usize,
        ) {
            let buf = vec![0u8; len];
            if start + 2 > len {
                prop_assert_eq!(parse_axis(&buf, start), None);
            }
        }

        #[test]
        fn prop_full_report_steering_round_trips(
            steering_lo in 0u8..=255u8,
            steering_hi in 0u8..=255u8,
        ) {
            let steering = u16::from_le_bytes([steering_lo, steering_hi]);
            let mut report = [0u8; MIN_REPORT_LEN + 4];
            report[0] = input_report::REPORT_ID;
            report[input_report::STEERING_START] = steering_lo;
            report[input_report::STEERING_START + 1] = steering_hi;

            if let Some(parsed) = parse_wheelbase_input_report(&report) {
                prop_assert_eq!(parsed.steering, steering);
            }
        }

        #[test]
        fn prop_pedal_axes_throttle_round_trips(
            throttle_lo in 0u8..=255u8,
            throttle_hi in 0u8..=255u8,
        ) {
            let throttle = u16::from_le_bytes([throttle_lo, throttle_hi]);
            let mut report = [0u8; MIN_REPORT_LEN + 4];
            report[0] = input_report::REPORT_ID;
            report[input_report::THROTTLE_START] = throttle_lo;
            report[input_report::THROTTLE_START + 1] = throttle_hi;

            if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
                prop_assert_eq!(parsed.throttle, throttle);
            }
        }

        #[test]
        fn prop_pedal_axes_brake_round_trips(
            brake_lo in 0u8..=255u8,
            brake_hi in 0u8..=255u8,
        ) {
            let brake = u16::from_le_bytes([brake_lo, brake_hi]);
            let mut report = [0u8; MIN_REPORT_LEN + 4];
            report[0] = input_report::REPORT_ID;
            report[input_report::BRAKE_START] = brake_lo;
            report[input_report::BRAKE_START + 1] = brake_hi;

            if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
                prop_assert_eq!(parsed.brake, brake);
            }
        }

        #[test]
        fn prop_wrong_report_id_always_rejected(id in 2u8..=255u8) {
            let mut report = [0u8; MIN_REPORT_LEN + 4];
            report[0] = id;
            prop_assert!(parse_wheelbase_report(&report).is_none());
        }

        #[test]
        fn prop_axis_u16_or_zero_matches_option(
            lo in 0u8..=255u8,
            hi in 0u8..=255u8,
        ) {
            let data = [0x01, lo, hi];
            let view = RawWheelbaseReport::new(&data);
            let opt = view.axis_u16_le(1);
            let or_zero = view.axis_u16_or_zero(1);
            prop_assert_eq!(opt.unwrap_or(0), or_zero);
        }

        #[test]
        fn prop_clutch_round_trips(
            clutch_lo in 0u8..=255u8,
            clutch_hi in 0u8..=255u8,
        ) {
            let clutch = u16::from_le_bytes([clutch_lo, clutch_hi]);
            let mut report = [0u8; input_report::HANDBRAKE_START];
            report[0] = input_report::REPORT_ID;
            report[input_report::CLUTCH_START] = clutch_lo;
            report[input_report::CLUTCH_START + 1] = clutch_hi;

            if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
                prop_assert_eq!(parsed.clutch, Some(clutch));
            }
        }

        #[test]
        fn prop_handbrake_round_trips(
            hb_lo in 0u8..=255u8,
            hb_hi in 0u8..=255u8,
        ) {
            let handbrake = u16::from_le_bytes([hb_lo, hb_hi]);
            let mut report = [0u8; input_report::HANDBRAKE_START + 2];
            report[0] = input_report::REPORT_ID;
            report[input_report::HANDBRAKE_START] = hb_lo;
            report[input_report::HANDBRAKE_START + 1] = hb_hi;

            if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
                prop_assert_eq!(parsed.handbrake, Some(handbrake));
            }
        }

        #[test]
        fn prop_full_report_all_axes_preserved(
            steer in 0u16..=65535u16,
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
        ) {
            let mut report = [0u8; MIN_REPORT_LEN + 4];
            report[0] = input_report::REPORT_ID;
            report[input_report::STEERING_START..input_report::STEERING_START + 2]
                .copy_from_slice(&steer.to_le_bytes());
            report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
                .copy_from_slice(&throttle.to_le_bytes());
            report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
                .copy_from_slice(&brake.to_le_bytes());

            if let Some(parsed) = parse_wheelbase_input_report(&report) {
                prop_assert_eq!(parsed.steering, steer);
                prop_assert_eq!(parsed.pedals.throttle, throttle);
                prop_assert_eq!(parsed.pedals.brake, brake);
            }
        }
    }
}
