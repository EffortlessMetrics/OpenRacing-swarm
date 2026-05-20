use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::fake_transport::{
    FAKE_TRANSPORT_CODEC_STATUS, MozaFakeSerialTransport, MozaFakeSerialTransportError,
};
use racing_wheel_hid_moza_protocol::serial::frame::MozaSerialFrameError;
use racing_wheel_hid_moza_protocol::serial::vendor_authority::{
    MozaRiskClass, MozaSerialCodecStatus,
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

fn transport_fixture() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../fixtures/moza/r5/vendor-fake-serial-transport.json"
    ))
}

fn transport_schema() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../schemas/moza-vendor-fake-serial-transport.schema.json"
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

fn string_set<'a>(value: &'a Value, field: &str) -> Result<BTreeSet<&'a str>, io::Error> {
    let mut values = BTreeSet::new();
    for item in array_field(value, field)? {
        let item = item
            .as_str()
            .ok_or_else(|| invalid_data(format!("field `{field}` must contain strings")))?;
        values.insert(item);
    }
    Ok(values)
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
fn fake_transport_fixture_is_non_claiming() -> TestResult {
    let fixture = transport_fixture()?;

    assert_eq!(
        str_field(&fixture, "claim_scope")?,
        "software_fake_transport_only"
    );
    assert_eq!(str_field(&fixture, "transport_kind")?, "fake_only");
    assert_eq!(str_field(&fixture, "codec_status")?, "round_trip_verified");
    assert!(bool_field(&fixture, "fake_transport_verified")?);
    assert!(!bool_field(&fixture, "native_control_evidence")?);
    assert!(!bool_field(&fixture, "hardware_output_authorized")?);
    assert!(!bool_field(&fixture, "native_visible_ready")?);
    assert!(!bool_field(&fixture, "opened_serial_device")?);
    assert!(!bool_field(&fixture, "sent_read_only_query_commands")?);
    assert!(!bool_field(&fixture, "sent_output_writes")?);
    assert!(!bool_field(&fixture, "sent_configuration_writes")?);
    assert!(!bool_field(&fixture, "sent_firmware_or_dfu_commands")?);
    assert!(!bool_field(&fixture, "real_hardware_validated")?);
    assert_eq!(
        FAKE_TRANSPORT_CODEC_STATUS,
        MozaSerialCodecStatus::RoundTripVerified
    );
    assert!(!FAKE_TRANSPORT_CODEC_STATUS.allows_hardware_writes());

    Ok(())
}

#[test]
fn schema_requires_fake_transport_safety_gates() -> TestResult {
    let schema = transport_schema()?;
    let required = array_field(&schema, "required")?;

    for field in [
        "claim_scope",
        "native_control_evidence",
        "hardware_output_authorized",
        "native_visible_ready",
        "transport_kind",
        "fake_transport_verified",
        "opened_serial_device",
        "sent_read_only_query_commands",
        "sent_output_writes",
        "sent_configuration_writes",
        "sent_firmware_or_dfu_commands",
        "real_hardware_validated",
        "accepted_fixture_ids",
        "blocked_fixture_ids",
    ] {
        assert!(
            required.iter().any(|entry| entry.as_str() == Some(field)),
            "fake transport schema must require `{field}`"
        );
    }

    Ok(())
}

#[test]
fn fake_transport_accepts_only_read_only_status_fixtures() -> TestResult {
    let codec_fixture = codec_fixture()?;
    let transport_fixture = transport_fixture()?;
    let fixtures = fixtures_by_id(&codec_fixture)?;
    let accepted = string_set(&transport_fixture, "accepted_fixture_ids")?;
    let blocked = string_set(&transport_fixture, "blocked_fixture_ids")?;
    let mut transport = MozaFakeSerialTransport::new();

    for fixture_id in &accepted {
        let fixture = fixtures
            .get(fixture_id)
            .ok_or_else(|| invalid_data(format!("missing accepted fixture `{fixture_id}`")))?;
        let frame = hex_to_bytes(str_field(fixture, "raw_frame_hex")?)?;
        let exchange = transport.submit_read_only_fixture_frame(&frame)?;
        assert_eq!(exchange.command_id, *fixture_id);
        assert_eq!(exchange.risk_class, MozaRiskClass::VendorStatus);
        assert_eq!(exchange.synthetic_response_payload, vec![0]);
    }

    for fixture_id in &blocked {
        let fixture = fixtures
            .get(fixture_id)
            .ok_or_else(|| invalid_data(format!("missing blocked fixture `{fixture_id}`")))?;
        let frame = hex_to_bytes(str_field(fixture, "raw_frame_hex")?)?;
        assert!(
            matches!(
                transport.submit_read_only_fixture_frame(&frame),
                Err(MozaFakeSerialTransportError::AuthorizationRequired { .. })
            ),
            "write-like fixture `{fixture_id}` must be rejected by the fake transport"
        );
    }

    assert_eq!(transport.exchanges().len(), accepted.len());

    Ok(())
}

#[test]
fn fake_transport_rejects_malformed_and_unknown_frames() -> TestResult {
    let mut transport = MozaFakeSerialTransport::new();
    let mut bad_checksum = hex_to_bytes("7E01281302C9")?;
    let checksum_index = bad_checksum.len() - 1;
    bad_checksum[checksum_index] ^= 1;
    assert!(matches!(
        transport.submit_read_only_fixture_frame(&bad_checksum),
        Err(MozaFakeSerialTransportError::Frame(
            MozaSerialFrameError::ChecksumMismatch { .. }
        ))
    ));

    let unknown = hex_to_bytes("7E01FF13019F")?;
    assert!(matches!(
        transport.submit_read_only_fixture_frame(&unknown),
        Err(MozaFakeSerialTransportError::Frame(
            MozaSerialFrameError::UnknownCommand {
                group: 0xff,
                command: 1
            }
        ))
    ));

    assert!(transport.exchanges().is_empty());

    Ok(())
}
