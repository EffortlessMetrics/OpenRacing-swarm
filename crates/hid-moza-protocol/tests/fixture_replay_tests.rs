//! Replay promoted Moza capture fixtures through the parser.
//!
//! `wheelctl moza promote-fixtures` writes sanitized JSON files in this shape.
//! This test makes those files part of the normal parser regression suite once
//! real hardware captures are promoted into `crates/hid-moza-protocol/fixtures`.

use racing_wheel_hid_moza_protocol::{FfbMode, MozaProtocol};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[test]
fn promoted_capture_fixtures_replay_through_moza_parser() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut fixtures = Vec::new();
    collect_fixture_paths(&root, &mut fixtures)?;

    if fixtures.is_empty() {
        return Err(invalid_data(format!(
            "expected at least one Moza parser fixture under {}",
            root.display()
        )));
    }

    for fixture in fixtures {
        replay_fixture(&fixture)?;
    }

    Ok(())
}

fn collect_fixture_paths(root: &Path, fixtures: &mut Vec<PathBuf>) -> TestResult {
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_fixture_paths(&path, fixtures)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            fixtures.push(path);
        }
    }

    fixtures.sort();
    Ok(())
}

fn replay_fixture(path: &Path) -> TestResult {
    let text = fs::read_to_string(path)?;
    let fixture: Value = serde_json::from_str(&text)?;

    if require_u64(&fixture, "schema_version", path)? != 1 {
        return_invalid(path, "schema_version must be 1")?;
    }
    if !require_bool(&fixture, "no_ffb_writes", path)? {
        return_invalid(path, "fixture must record no_ffb_writes=true")?;
    }

    let reports = require_array(&fixture, "reports", path)?;
    let included_reports = require_u64(&fixture, "included_reports", path)? as usize;
    if included_reports != reports.len() {
        return_invalid(
            path,
            format!(
                "included_reports={included_reports} does not match reports.len()={}",
                reports.len()
            ),
        )?;
    }

    let total_reports = require_u64(&fixture, "total_reports", path)? as usize;
    if total_reports < included_reports {
        return_invalid(
            path,
            format!(
                "total_reports={total_reports} is less than included_reports={included_reports}"
            ),
        )?;
    }

    for report in reports {
        replay_report(path, report)?;
    }

    Ok(())
}

fn replay_report(path: &Path, report: &Value) -> TestResult {
    let pid = parse_hex_u16(require_str(report, "product_id", path)?)?;
    let data_hex = require_str(report, "data_hex", path)?;
    let data = decode_hex(data_hex)?;

    let declared_len = require_u64(report, "report_len", path)? as usize;
    if declared_len != data.len() {
        return_invalid(
            path,
            format!(
                "report_len={declared_len} does not match decoded len={}",
                data.len()
            ),
        )?;
    }

    let report_id = require_str(report, "report_id", path)?;
    let first_byte = data
        .first()
        .copied()
        .ok_or_else(|| invalid_data(format!("{}: empty report data", path.display())))?;
    if report_id != hex_u8(first_byte) {
        return_invalid(
            path,
            format!(
                "report_id={report_id} does not match first data byte {}",
                hex_u8(first_byte)
            ),
        )?;
    }

    let protocol = MozaProtocol::new_with_config(pid, FfbMode::Off, false);
    let parsed = protocol.parse_input_state(&data).ok_or_else(|| {
        invalid_data(format!(
            "{}: parser rejected promoted fixture report for PID 0x{pid:04X}",
            path.display()
        ))
    })?;
    let expected = require_object(report, "parsed", path)?;

    compare_u16(path, expected, "steering_u16", parsed.steering_u16)?;
    compare_u16(path, expected, "throttle_u16", parsed.throttle_u16)?;
    compare_u16(path, expected, "brake_u16", parsed.brake_u16)?;
    compare_u16(path, expected, "clutch_u16", parsed.clutch_u16)?;
    compare_u16(path, expected, "handbrake_u16", parsed.handbrake_u16)?;
    compare_u8(path, expected, "hat", parsed.hat)?;
    compare_u8(path, expected, "funky", parsed.funky)?;
    compare_u32(path, expected, "tick", parsed.tick)?;
    compare_buttons(path, expected, &parsed.buttons)?;
    compare_rotary(path, expected, &parsed.rotary)?;

    Ok(())
}

fn compare_u16(path: &Path, value: &Value, key: &str, actual: u16) -> TestResult {
    let expected = require_u64(value, key, path)?;
    if expected != u64::from(actual) {
        return_invalid(
            path,
            format!("{key} expected {expected}, parser produced {actual}"),
        )?;
    }
    Ok(())
}

fn compare_u8(path: &Path, value: &Value, key: &str, actual: u8) -> TestResult {
    let expected = require_u64(value, key, path)?;
    if expected != u64::from(actual) {
        return_invalid(
            path,
            format!("{key} expected {expected}, parser produced {actual}"),
        )?;
    }
    Ok(())
}

fn compare_u32(path: &Path, value: &Value, key: &str, actual: u32) -> TestResult {
    let expected = require_u64(value, key, path)?;
    if expected != u64::from(actual) {
        return_invalid(
            path,
            format!("{key} expected {expected}, parser produced {actual}"),
        )?;
    }
    Ok(())
}

fn compare_buttons(path: &Path, value: &Value, actual: &[u8; 16]) -> TestResult {
    let expected = require_array(value, "buttons_hex", path)?;
    if expected.len() != actual.len() {
        return_invalid(
            path,
            format!(
                "buttons_hex len={} does not match parser button len={}",
                expected.len(),
                actual.len()
            ),
        )?;
    }

    for (index, expected_value) in expected.iter().enumerate() {
        let expected_hex = expected_value.as_str().ok_or_else(|| {
            invalid_data(format!(
                "{}: buttons_hex[{index}] must be a string",
                path.display()
            ))
        })?;
        let actual_hex = hex_u8(actual[index]);
        if expected_hex != actual_hex {
            return_invalid(
                path,
                format!(
                    "buttons_hex[{index}] expected {expected_hex}, parser produced {actual_hex}"
                ),
            )?;
        }
    }

    Ok(())
}

fn compare_rotary(path: &Path, value: &Value, actual: &[u8; 2]) -> TestResult {
    let expected = require_array(value, "rotary", path)?;
    if expected.len() != actual.len() {
        return_invalid(
            path,
            format!(
                "rotary len={} does not match parser rotary len={}",
                expected.len(),
                actual.len()
            ),
        )?;
    }

    for (index, expected_value) in expected.iter().enumerate() {
        let expected_u64 = expected_value.as_u64().ok_or_else(|| {
            invalid_data(format!(
                "{}: rotary[{index}] must be a number",
                path.display()
            ))
        })?;
        if expected_u64 != u64::from(actual[index]) {
            return_invalid(
                path,
                format!(
                    "rotary[{index}] expected {expected_u64}, parser produced {}",
                    actual[index]
                ),
            )?;
        }
    }

    Ok(())
}

fn require_object<'a>(value: &'a Value, key: &str, path: &Path) -> TestResult<&'a Value> {
    let child = value
        .get(key)
        .ok_or_else(|| invalid_data(format!("{}: missing required field {key}", path.display())))?;
    if !child.is_object() {
        return_invalid(path, format!("{key} must be an object"))?;
    }
    Ok(child)
}

fn require_array<'a>(value: &'a Value, key: &str, path: &Path) -> TestResult<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data(format!("{}: {key} must be an array", path.display())))
}

fn require_str<'a>(value: &'a Value, key: &str, path: &Path) -> TestResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_data(format!("{}: {key} must be a string", path.display())))
}

fn require_u64(value: &Value, key: &str, path: &Path) -> TestResult<u64> {
    value.get(key).and_then(Value::as_u64).ok_or_else(|| {
        invalid_data(format!(
            "{}: {key} must be an unsigned integer",
            path.display()
        ))
    })
}

fn require_bool(value: &Value, key: &str, path: &Path) -> TestResult<bool> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_data(format!("{}: {key} must be a boolean", path.display())))
}

fn parse_hex_u16(value: &str) -> TestResult<u16> {
    let raw = value
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    u16::from_str_radix(raw, 16)
        .map_err(|error| invalid_data(format!("invalid u16 hex value {value}: {error}")))
}

fn decode_hex(value: &str) -> TestResult<Vec<u8>> {
    let raw = value.trim();
    if !raw.len().is_multiple_of(2) {
        return Err(invalid_data(format!(
            "hex payload has odd number of chars: {raw}"
        )));
    }

    let mut out = Vec::with_capacity(raw.len() / 2);
    for pair in raw.as_bytes().chunks(2) {
        let pair = std::str::from_utf8(pair)?;
        let byte = u8::from_str_radix(pair, 16)
            .map_err(|error| invalid_data(format!("invalid hex byte {pair}: {error}")))?;
        out.push(byte);
    }
    Ok(out)
}

fn hex_u8(value: u8) -> String {
    format!("0x{value:02X}")
}

fn return_invalid<T>(path: &Path, message: impl Into<String>) -> TestResult<T> {
    Err(invalid_data(format!(
        "{}: {}",
        path.display(),
        message.into()
    )))
}

fn invalid_data(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}
