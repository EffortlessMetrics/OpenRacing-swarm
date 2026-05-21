use racing_wheel_hid_moza_protocol::REPORT_LEN;

pub(crate) fn short_hid_write_error(bytes_written: usize) -> Option<String> {
    if bytes_written == REPORT_LEN {
        None
    } else {
        Some(format!(
            "short_hid_write: expected {REPORT_LEN} bytes, wrote {bytes_written}"
        ))
    }
}

pub(crate) fn short_zero_output_write_error(
    expected_len: usize,
    bytes_written: usize,
) -> Option<String> {
    if bytes_written >= expected_len {
        None
    } else {
        Some(format!(
            "short_hid_write: expected at least {expected_len} bytes, wrote {bytes_written}"
        ))
    }
}
