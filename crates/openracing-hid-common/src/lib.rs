//! Common HID utilities for racing wheel protocol implementations
//!
//! This crate provides common utilities shared across different HID protocol
//! implementations for racing wheel hardware.

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod device_info;
pub mod hid_traits;
pub mod math;
pub mod passive_input;
pub mod report_parser;

pub use device_info::*;
pub use hid_traits::*;
pub use report_parser::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HidCommonError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Failed to open device: {0}")]
    OpenError(String),

    #[error("Failed to read from device: {0}")]
    ReadError(String),

    #[error("Failed to write to device: {0}")]
    WriteError(String),

    #[error("Invalid report format: {0}")]
    InvalidReport(String),

    #[error("Device disconnected")]
    Disconnected,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type HidCommonResult<T> = Result<T, HidCommonError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_types() {
        let err = HidCommonError::DeviceNotFound("test".to_string());
        assert_eq!(format!("{}", err), "Device not found: test");

        let err = HidCommonError::Disconnected;
        assert_eq!(format!("{}", err), "Device disconnected");
    }
}
