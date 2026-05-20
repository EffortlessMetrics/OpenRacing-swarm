use std::collections::BTreeMap;
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::frame::{
    MESSAGE_START, MozaSerialFrameError, SERIAL_FIXTURE_CODEC_STATUS, decode_fixture_frame,
};
use racing_wheel_hid_moza_protocol::serial::vendor_authority::{
    MozaSerialCodecStatus, REQUIRED_VENDOR_COMMANDS,
};
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn codec_fixture() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../fixtures/moza/r5/vendor-serial-codec-fixtures.json"
    ))
}

fn codec_schema() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../schemas/moza-vendor-serial-codec-fixtures.schema.json"
    ))
}

fn array_field<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>, io::Error> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data(format!("missing array field `{field}`")))
}

fn str_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, io::Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_data(format!("missing string field `{field}`")))
}

fn bool_field(value: &Value, field: &str) -> Result<bool, io::Error> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_data(format!("missing bool field `{field}`")))
}

fn u8_field(value: &Value, field: &str) -> Result<u8, io::Error> {
    let number = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_data(format!("missing integer field `{field}`")))?;
    u8::try_from(number).map_err(|_| invalid_data(format!("field `{field}` is outside u8 range")))
}

fn fixtures_by_id(codec_fixture: &Value) -> Result<BTreeMap<&str, &Value>, io::Error> {
    let mut fixtures = BTreeMap::new();
    for fixture in array_field(codec_fixture, "fixtures")? {
        let id = str_field(fixture, "id")?;
        if fixtures.insert(id, fixture).is_some() {
            return Err(invalid_data(format!("duplicate codec fixture `{id}`")));
        }
    }
    Ok(fixtures)
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, io::Error> {
    if !hex.len().is_multiple_of(2) {
        return Err(invalid_data("hex fixture length must be even"));
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for offset in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[offset..offset + 2], 16)
            .map_err(|_| invalid_data(format!("invalid hex byte at offset {offset}")))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

#[test]
fn codec_fixture_is_non_claiming_and_decode_only() -> TestResult {
    let fixture = codec_fixture()?;

    assert_eq!(
        str_field(&fixture, "claim_scope")?,
        "software_fixture_decode_only"
    );
    assert_eq!(str_field(&fixture, "codec_status")?, "fixture_decode_only");
    assert!(!bool_field(&fixture, "native_control_evidence")?);
    assert!(!bool_field(&fixture, "hardware_output_authorized")?);
    assert!(!bool_field(&fixture, "native_visible_ready")?);
    assert!(!bool_field(&fixture, "sent_read_only_query_commands")?);
    assert!(!bool_field(&fixture, "sent_output_writes")?);
    assert!(!bool_field(&fixture, "sent_configuration_writes")?);
    assert!(!bool_field(&fixture, "sent_firmware_or_dfu_commands")?);
    assert_eq!(
        SERIAL_FIXTURE_CODEC_STATUS,
        MozaSerialCodecStatus::FixtureDecodeOnly
    );
    assert!(!SERIAL_FIXTURE_CODEC_STATUS.allows_hardware_writes());

    Ok(())
}

#[test]
fn schema_requires_decode_only_safety_gates() -> TestResult {
    let schema = codec_schema()?;
    let required = array_field(&schema, "required")?;

    for field in [
        "claim_scope",
        "native_control_evidence",
        "hardware_output_authorized",
        "native_visible_ready",
        "codec_status",
        "sent_read_only_query_commands",
        "sent_output_writes",
        "sent_configuration_writes",
        "sent_firmware_or_dfu_commands",
        "fixtures",
    ] {
        assert!(
            required.iter().any(|entry| entry.as_str() == Some(field)),
            "codec fixture schema must require `{field}`"
        );
    }

    Ok(())
}

#[test]
fn codec_fixtures_decode_to_required_registry_commands() -> TestResult {
    let fixture = codec_fixture()?;
    let fixtures = fixtures_by_id(&fixture)?;
    assert_eq!(fixtures.len(), REQUIRED_VENDOR_COMMANDS.len());

    for expected in REQUIRED_VENDOR_COMMANDS {
        let fixture_entry = fixtures
            .get(expected.id)
            .ok_or_else(|| invalid_data(format!("missing codec fixture `{}`", expected.id)))?;
        let frame = hex_to_bytes(str_field(fixture_entry, "raw_frame_hex")?)?;
        let decoded = decode_fixture_frame(&frame)?;

        assert_eq!(frame.first().copied(), Some(MESSAGE_START));
        assert_eq!(decoded.group, expected.group);
        assert_eq!(decoded.command_id, expected.command);
        assert_eq!(decoded.command.id, expected.id);
        assert_eq!(decoded.command.family, expected.family);
        assert_eq!(decoded.command.risk_class, expected.risk_class);
        assert_eq!(decoded.device_id, u8_field(fixture_entry, "device_id")?);
        assert_eq!(decoded.payload.len(), 0);
        assert_eq!(u8_field(fixture_entry, "payload_len")?, 0);
        assert!(bool_field(fixture_entry, "checksum_valid")?);
        assert_eq!(
            str_field(fixture_entry, "risk_class")?,
            expected.risk_class.as_registry_str()
        );
        assert!(!bool_field(fixture_entry, "hardware_output_authorized")?);
        assert!(!bool_field(fixture_entry, "sendable_on_hardware")?);
    }

    Ok(())
}

#[test]
fn decoder_rejects_drifted_or_unknown_fixture_bytes() -> TestResult {
    let valid = hex_to_bytes("7E01281302C9")?;

    let mut bad_start = valid.clone();
    bad_start[0] = 0;
    assert!(matches!(
        decode_fixture_frame(&bad_start),
        Err(MozaSerialFrameError::BadStart { actual: 0 })
    ));

    let mut bad_length = valid.clone();
    bad_length[1] = 2;
    assert!(matches!(
        decode_fixture_frame(&bad_length),
        Err(MozaSerialFrameError::LengthMismatch {
            declared_len: 2,
            expected_len: 7,
            actual_len: 6
        })
    ));

    let mut bad_checksum = valid.clone();
    let checksum_index = bad_checksum.len() - 1;
    bad_checksum[checksum_index] ^= 1;
    assert!(matches!(
        decode_fixture_frame(&bad_checksum),
        Err(MozaSerialFrameError::ChecksumMismatch { .. })
    ));

    let unknown = hex_to_bytes("7E01FF13019F")?;
    assert!(matches!(
        decode_fixture_frame(&unknown),
        Err(MozaSerialFrameError::UnknownCommand {
            group: 0xff,
            command: 1
        })
    ));

    Ok(())
}
