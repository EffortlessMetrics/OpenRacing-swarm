use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::frame::{
    MozaSerialFrameError, decode_fixture_frame, decode_observed_frame_shape,
};
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn protocol_evidence_review() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json"
    ))
}

fn value_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value, io::Error> {
    value
        .pointer(pointer)
        .ok_or_else(|| invalid_data(format!("missing JSON pointer `{pointer}`")))
}

fn array_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a Vec<Value>, io::Error> {
    value_at(value, pointer)?
        .as_array()
        .ok_or_else(|| invalid_data(format!("JSON pointer `{pointer}` is not an array")))
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

fn usize_field(value: &Value, field: &str) -> Result<usize, io::Error> {
    let number = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_data(format!("missing integer field `{field}`")))?;
    usize::try_from(number)
        .map_err(|_| invalid_data(format!("field `{field}` is outside usize range")))
}

fn hex_to_u8(hex: &str) -> Result<u8, io::Error> {
    let trimmed = if let Some(stripped) = hex.strip_prefix("0x") {
        stripped
    } else if let Some(stripped) = hex.strip_prefix("0X") {
        stripped
    } else {
        hex
    };
    u8::from_str_radix(trimmed, 16).map_err(|_| invalid_data(format!("invalid hex byte `{hex}`")))
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

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("")
}

fn decode_candidate_sample_fixtures(review: &Value) -> Result<&Vec<Value>, io::Error> {
    array_at(
        review,
        "/passive_tuple_registry_coverage/decode_candidate_sample_fixtures",
    )
}

fn sample_frames_for_tuple<'a>(
    fixtures: &'a [Value],
    tuple_id: &str,
) -> Result<&'a Vec<Value>, io::Error> {
    for fixture in fixtures {
        if str_field(fixture, "tuple_id")? == tuple_id {
            return fixture
                .get("sample_frames")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    invalid_data(format!("tuple `{tuple_id}` is missing sample_frames"))
                });
        }
    }

    Err(invalid_data(format!(
        "missing decode candidate fixture for tuple `{tuple_id}`"
    )))
}

fn find_sample_frame<'a>(
    fixtures: &'a [Value],
    tuple_id: &str,
    scenario: &str,
    packet_ordinal: usize,
    frame_ordinal_in_packet: usize,
) -> Result<&'a Value, io::Error> {
    for sample in sample_frames_for_tuple(fixtures, tuple_id)? {
        if str_field(sample, "scenario")? == scenario
            && usize_field(sample, "packet_ordinal")? == packet_ordinal
            && usize_field(sample, "frame_ordinal_in_packet")? == frame_ordinal_in_packet
        {
            return Ok(sample);
        }
    }

    Err(invalid_data(format!(
        "missing sample `{tuple_id}` in `{scenario}` packet {packet_ordinal} frame {frame_ordinal_in_packet}"
    )))
}

fn assert_unknown_sample_remains_non_sendable(sample: &Value) -> TestResult {
    assert!(bool_field(sample, "checksum_valid")?);
    assert!(!bool_field(sample, "hardware_output_authorized")?);
    assert!(!bool_field(sample, "output_sendability_claim")?);

    let frame = hex_to_bytes(str_field(sample, "frame_hex")?)?;
    let observed = decode_observed_frame_shape(&frame)?;
    assert!(observed.command.is_none());
    assert!(matches!(
        decode_fixture_frame(&frame),
        Err(MozaSerialFrameError::UnknownCommand { group, command })
            if group == observed.group && command == observed.command_id
    ));

    Ok(())
}

#[test]
fn passive_decode_candidate_samples_are_non_claiming() -> TestResult {
    let review = protocol_evidence_review()?;
    let coverage = value_at(&review, "/passive_tuple_registry_coverage")?;

    assert_eq!(
        str_field(coverage, "decode_candidate_sample_scope")?,
        "highest_frequency_unknown_commanded_tuples"
    );
    assert_eq!(usize_field(coverage, "decode_candidate_sample_count")?, 30);
    assert_eq!(
        str_field(coverage, "unknown_tuple_risk_class")?,
        "unknown_do_not_send"
    );
    assert!(!bool_field(coverage, "hardware_output_authorized")?);
    assert!(!bool_field(coverage, "output_sendability_claim")?);

    let tuple_ids = array_at(coverage, "/decode_candidate_sample_tuple_ids")?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .ok_or_else(|| invalid_data("sample tuple id must be a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        tuple_ids,
        Vec::from([
            "0x5A/0x1B/0x00",
            "0x5D/0x1B/0x01",
            "0x25/0x19/0x01",
            "0x25/0x19/0x02",
            "0x25/0x19/0x03",
        ])
    );

    for fixture in array_at(coverage, "/decode_candidate_sample_fixtures")? {
        assert_eq!(str_field(fixture, "registry_status")?, "unknown_commanded");
        assert!(!bool_field(fixture, "hardware_output_authorized")?);
        assert!(!bool_field(fixture, "output_sendability_claim")?);
    }

    Ok(())
}

#[test]
fn observed_decoder_accepts_sample_shape_without_promoting_unknown_tuples() -> TestResult {
    let review = protocol_evidence_review()?;
    let fixtures = decode_candidate_sample_fixtures(&review)?;

    let mut decoded_sample_count = 0usize;
    for fixture in fixtures {
        let fixture_tuple_id = str_field(fixture, "tuple_id")?;
        let sample_frames = fixture
            .get("sample_frames")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_data("decode candidate fixture is missing sample_frames"))?;
        assert_eq!(usize_field(fixture, "sample_count")?, sample_frames.len());

        for sample in sample_frames {
            assert_eq!(str_field(sample, "tuple_id")?, fixture_tuple_id);
            assert!(str_field(sample, "scenario")?.starts_with("pit-house-"));
            assert!(bool_field(sample, "checksum_valid")?);
            assert!(!bool_field(sample, "hardware_output_authorized")?);
            assert!(!bool_field(sample, "output_sendability_claim")?);

            let frame = hex_to_bytes(str_field(sample, "frame_hex")?)?;
            let observed = decode_observed_frame_shape(&frame)?;

            assert_eq!(observed.group, hex_to_u8(str_field(sample, "group")?)?);
            assert_eq!(
                observed.device_id,
                hex_to_u8(str_field(sample, "device_id")?)?
            );
            assert_eq!(
                observed.command_id,
                hex_to_u8(str_field(sample, "command")?)?
            );
            assert_eq!(observed.payload.len(), usize_field(sample, "payload_len")?);
            assert_eq!(
                bytes_to_hex(observed.payload),
                str_field(sample, "payload_hex")?
            );
            assert_eq!(
                observed.checksum,
                hex_to_u8(str_field(sample, "checksum_hex")?)?
            );
            assert!(
                observed.command.is_none(),
                "passive sample `{fixture_tuple_id}` must remain unknown to the semantic registry"
            );

            assert!(matches!(
                decode_fixture_frame(&frame),
                Err(MozaSerialFrameError::UnknownCommand { group, command })
                    if group == observed.group && command == observed.command_id
            ));
            decoded_sample_count = decoded_sample_count.saturating_add(1);
        }
    }

    assert_eq!(decoded_sample_count, 30);
    Ok(())
}

#[test]
fn passive_decode_candidate_samples_preserve_repeated_packet_order_hints() -> TestResult {
    let review = protocol_evidence_review()?;
    let fixtures = decode_candidate_sample_fixtures(&review)?;

    let mut paired_1b_samples = 0usize;
    for first in sample_frames_for_tuple(fixtures, "0x5A/0x1B/0x00")? {
        let scenario = str_field(first, "scenario")?;
        let packet_ordinal = usize_field(first, "packet_ordinal")?;
        let first_frame_ordinal = usize_field(first, "frame_ordinal_in_packet")?;
        let second = find_sample_frame(
            fixtures,
            "0x5D/0x1B/0x01",
            scenario,
            packet_ordinal,
            first_frame_ordinal.saturating_add(1),
        )?;

        assert_unknown_sample_remains_non_sendable(first)?;
        assert_unknown_sample_remains_non_sendable(second)?;
        paired_1b_samples = paired_1b_samples.saturating_add(1);
    }

    assert_eq!(
        paired_1b_samples,
        sample_frames_for_tuple(fixtures, "0x5D/0x1B/0x01")?.len()
    );
    assert_eq!(paired_1b_samples, 6);

    let mut ordered_19_triads = 0usize;
    for first in sample_frames_for_tuple(fixtures, "0x25/0x19/0x02")? {
        let scenario = str_field(first, "scenario")?;
        let packet_ordinal = usize_field(first, "packet_ordinal")?;
        let first_frame_ordinal = usize_field(first, "frame_ordinal_in_packet")?;
        let second = find_sample_frame(
            fixtures,
            "0x25/0x19/0x03",
            scenario,
            packet_ordinal,
            first_frame_ordinal.saturating_add(1),
        )?;
        let third = find_sample_frame(
            fixtures,
            "0x25/0x19/0x01",
            scenario,
            packet_ordinal,
            first_frame_ordinal.saturating_add(2),
        )?;

        assert_unknown_sample_remains_non_sendable(first)?;
        assert_unknown_sample_remains_non_sendable(second)?;
        assert_unknown_sample_remains_non_sendable(third)?;
        ordered_19_triads = ordered_19_triads.saturating_add(1);
    }

    assert_eq!(
        ordered_19_triads,
        sample_frames_for_tuple(fixtures, "0x25/0x19/0x03")?.len()
    );
    assert_eq!(
        ordered_19_triads,
        sample_frames_for_tuple(fixtures, "0x25/0x19/0x01")?.len()
    );
    assert_eq!(ordered_19_triads, 6);

    Ok(())
}
