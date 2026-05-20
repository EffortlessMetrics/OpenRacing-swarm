use anyhow::Error;
use serde_json::Value;

use crate::error::CliError;

pub(super) fn short_hid_write_error(report_len: usize, bytes_written: usize) -> Option<String> {
    if bytes_written == report_len {
        None
    } else {
        Some(format!(
            "short_hid_write: expected {report_len} bytes, wrote {bytes_written}"
        ))
    }
}

pub(super) fn short_zero_output_write_error(
    expected_len: usize,
    bytes_written: usize,
) -> Option<String> {
    if bytes_written >= expected_len {
        None
    } else {
        Some(format!(
            "short_hid_write: expected {expected_len} bytes, wrote {bytes_written}"
        ))
    }
}

pub(super) fn no_out_of_scope_device_commands(receipt: &Value) -> bool {
    receipt
        .get("no_serial_config_commands")
        .and_then(Value::as_bool)
        == Some(true)
        && receipt
            .get("no_firmware_or_dfu_commands")
            .and_then(Value::as_bool)
            == Some(true)
}

pub(super) fn receipt_failure(message: impl Into<String>) -> Error {
    CliError::ReceiptFailure(message.into()).into()
}
