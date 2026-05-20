use std::collections::BTreeMap;
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::status_probe::{
    MozaReadOnlyStatusProbeError, READ_ONLY_STATUS_CODEC_STATUS, decode_read_only_status_response,
    encode_read_only_status_query, read_only_status_commands,
};
use racing_wheel_hid_moza_protocol::serial::vendor_authority::{
    MozaRiskClass, MozaSerialCodecStatus, REQUIRED_VENDOR_COMMANDS,
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
fn read_only_status_probe_encodes_only_vendor_status_commands() -> TestResult {
    assert_eq!(
        READ_ONLY_STATUS_CODEC_STATUS,
        MozaSerialCodecStatus::RoundTripVerified
    );
    assert!(!READ_ONLY_STATUS_CODEC_STATUS.allows_hardware_writes());

    let codec_fixture = codec_fixture()?;
    let fixtures = fixtures_by_id(&codec_fixture)?;
    let mut encoded_count = 0;

    for command in REQUIRED_VENDOR_COMMANDS {
        match encode_read_only_status_query(command) {
            Ok(frame) => {
                encoded_count += 1;
                assert_eq!(command.risk_class, MozaRiskClass::VendorStatus);
                assert!(command.read_only_status_probe_allowed);
                let fixture = fixtures
                    .get(command.id)
                    .ok_or_else(|| invalid_data(format!("missing fixture `{}`", command.id)))?;
                assert_eq!(frame, hex_to_bytes(str_field(fixture, "raw_frame_hex")?)?);
            }
            Err(MozaReadOnlyStatusProbeError::NotReadOnlyStatusProbeAllowed {
                command_id,
                risk_class,
            }) => {
                assert_eq!(command_id, command.id);
                assert_ne!(risk_class, MozaRiskClass::VendorStatus);
                assert!(!command.read_only_status_probe_allowed);
            }
            Err(error) => return Err(Box::new(error)),
        }
    }

    assert_eq!(encoded_count, read_only_status_commands().count());
    assert_eq!(encoded_count, 9);

    Ok(())
}

#[test]
fn read_only_status_probe_decodes_matching_responses() -> TestResult {
    let codec_fixture = codec_fixture()?;
    let fixtures = fixtures_by_id(&codec_fixture)?;

    for command in read_only_status_commands() {
        let fixture = fixtures
            .get(command.id)
            .ok_or_else(|| invalid_data(format!("missing fixture `{}`", command.id)))?;
        let frame = hex_to_bytes(str_field(fixture, "raw_frame_hex")?)?;
        let decoded = decode_read_only_status_response(command, &frame)?;

        assert_eq!(decoded.command.id, command.id);
        assert_eq!(decoded.device_id, command.device_id);
        assert!(decoded.payload.is_empty());
    }

    Ok(())
}

#[test]
fn read_only_status_probe_rejects_mismatched_response_command() -> TestResult {
    let estop = read_only_status_commands()
        .find(|command| command.id == "estop_get_ffb")
        .ok_or_else(|| invalid_data("missing estop status command"))?;
    let compatibility = read_only_status_commands()
        .find(|command| command.id == "compatibility_get_mode")
        .ok_or_else(|| invalid_data("missing compatibility status command"))?;
    let compatibility_frame = encode_read_only_status_query(compatibility)?;

    assert!(matches!(
        decode_read_only_status_response(estop, &compatibility_frame),
        Err(MozaReadOnlyStatusProbeError::ResponseCommandMismatch {
            expected_command_id: "estop_get_ffb",
            actual_command_id: "compatibility_get_mode"
        })
    ));

    Ok(())
}
