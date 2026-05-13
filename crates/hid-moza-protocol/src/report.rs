//! Moza HID report layout constants and zero-copy report views.

#![deny(static_mut_refs)]

pub use racing_wheel_moza_wheelbase_report::{
    RawWheelbaseReport, WheelbaseInputRaw, WheelbasePedalAxesRaw, input_report,
    looks_like_live_r5_v1_extended_report, parse_axis, parse_wheelbase_input_report,
    parse_wheelbase_pedal_axes, parse_wheelbase_report,
};

/// Best-effort layouts for direct USB HBP handbrake reports.
pub mod hbp_report {
    pub use racing_wheel_hbp::{
        RAW_AXIS_START, RAW_BUTTON, WITH_REPORT_ID_AXIS_START, WITH_REPORT_ID_BUTTON,
    };
}

/// Moza HID Report IDs.
///
/// These report IDs are used on the HID interface for device control and FFB.
/// The serial/CDC ACM interface uses a separate framing protocol (see
/// `protocol.rs` module-level docs).
pub mod report_ids {
    /// Device info query
    pub const DEVICE_INFO: u8 = 0x01;
    /// High torque enable
    pub const HIGH_TORQUE: u8 = 0x02;
    /// Start input reports
    pub const START_REPORTS: u8 = 0x03;
    /// Set rotation range
    pub const ROTATION_RANGE: u8 = 0x10;
    /// Set FFB mode
    pub const FFB_MODE: u8 = 0x11;
    /// Direct torque output
    pub const DIRECT_TORQUE: u8 = 0x20;
    /// Device gain
    pub const DEVICE_GAIN: u8 = 0x21;
}
