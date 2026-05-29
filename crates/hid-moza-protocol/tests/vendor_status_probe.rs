use std::collections::BTreeMap;
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::status_probe::{
    MozaReadOnlyStatusProbeError, MozaReadOnlyStatusResponseFrameClass,
    MozaReadOnlyStatusResponseFrameDisposition, READ_ONLY_STATUS_CODEC_STATUS,
    decode_read_only_status_response, demux_read_only_status_response_frame,
    demux_read_only_status_response_stream, diagnose_read_only_status_response_frame,
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

fn checked_in_status_matrix() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../ci/hardware/moza-r5/2026-05-13/vendor-status-mode-matrix.json"
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
fn read_only_status_probe_decodes_response_group_device_transform() -> TestResult {
    let base_gain = read_only_status_commands()
        .find(|command| command.id == "base_gain_get_overall_strength")
        .ok_or_else(|| invalid_data("missing base gain status command"))?;
    let response_frame = hex_to_bytes("7E03A8310203E854")?;
    let decoded = decode_read_only_status_response(base_gain, &response_frame)?;

    assert_eq!(decoded.command.id, "base_gain_get_overall_strength");
    assert_eq!(decoded.group, 0xA8);
    assert_eq!(decoded.device_id, 0x31);
    assert_eq!(decoded.command_id, 0x02);
    assert_eq!(decoded.payload, &[0x03, 0xE8]);

    let diagnosis = diagnose_read_only_status_response_frame(&response_frame);
    assert_eq!(
        diagnosis.classification,
        MozaReadOnlyStatusResponseFrameClass::RegistryStatusResponse
    );
    assert!(diagnosis.registry_command_known);
    assert_eq!(diagnosis.checksum_valid, Some(true));

    Ok(())
}

#[test]
fn read_only_status_probe_demux_matches_response_group_device_transform() -> TestResult {
    let compatibility = read_only_status_commands()
        .find(|command| command.id == "compatibility_get_mode")
        .ok_or_else(|| invalid_data("missing compatibility status command"))?;
    let response_frame = hex_to_bytes("7E029F21170064")?;

    let diagnosis = demux_read_only_status_response_frame(compatibility, &response_frame);
    assert_eq!(
        diagnosis.disposition,
        MozaReadOnlyStatusResponseFrameDisposition::MatchingRegistryStatusResponse
    );
    assert!(diagnosis.matched_expected_command);
    assert!(diagnosis.decode_error.is_none());

    Ok(())
}

#[test]
fn read_only_status_probe_diagnoses_checked_in_debug_log_stream() -> TestResult {
    let matrix = checked_in_status_matrix()?;
    let responses = array_field(&matrix, "responses")?;
    assert_eq!(responses.len(), 9);

    let mut framed_ascii_log_count = 0usize;
    let mut desynchronized_count = 0usize;
    let mut registry_status_count = 0usize;

    for response in responses {
        let frame = hex_to_bytes(str_field(response, "response_frame_hex")?)?;
        let diagnosis = diagnose_read_only_status_response_frame(&frame);
        if diagnosis.classification == MozaReadOnlyStatusResponseFrameClass::FramedAsciiTelemetryLog
        {
            framed_ascii_log_count += 1;
        }
        if diagnosis.classification
            == MozaReadOnlyStatusResponseFrameClass::StreamDesynchronizedOrPartialLogFrame
        {
            desynchronized_count += 1;
        }
        if diagnosis.classification == MozaReadOnlyStatusResponseFrameClass::RegistryStatusResponse
        {
            registry_status_count += 1;
        }

        assert_eq!(diagnosis.group, Some(0x0e));
        assert_eq!(diagnosis.device_id, Some(0x71));
        assert_eq!(diagnosis.command, Some(0x05));
        assert!(!diagnosis.registry_command_known);
    }

    assert_eq!(framed_ascii_log_count, 8);
    assert_eq!(desynchronized_count, 1);
    assert_eq!(registry_status_count, 0);

    Ok(())
}

#[test]
fn read_only_status_probe_diagnoses_fixture_frames_as_registry_status() -> TestResult {
    let codec_fixture = codec_fixture()?;
    let fixtures = fixtures_by_id(&codec_fixture)?;

    for command in read_only_status_commands() {
        let fixture = fixtures
            .get(command.id)
            .ok_or_else(|| invalid_data(format!("missing fixture `{}`", command.id)))?;
        let frame = hex_to_bytes(str_field(fixture, "raw_frame_hex")?)?;
        let diagnosis = diagnose_read_only_status_response_frame(&frame);
        assert_eq!(
            diagnosis.classification,
            MozaReadOnlyStatusResponseFrameClass::RegistryStatusResponse
        );
        assert!(diagnosis.registry_command_known);
        assert_eq!(diagnosis.checksum_valid, Some(true));
    }

    Ok(())
}

#[test]
fn read_only_status_probe_demux_skips_debug_stream_until_matching_response() -> TestResult {
    let matrix = checked_in_status_matrix()?;
    let responses = array_field(&matrix, "responses")?;
    let debug_frame = hex_to_bytes(str_field(
        responses
            .first()
            .ok_or_else(|| invalid_data("missing checked-in debug response"))?,
        "response_frame_hex",
    )?)?;
    let estop = read_only_status_commands()
        .find(|command| command.id == "estop_get_ffb")
        .ok_or_else(|| invalid_data("missing estop status command"))?;
    let matching_frame = encode_read_only_status_query(estop)?;
    let frames = [debug_frame.as_slice(), matching_frame.as_slice()];

    let demux = demux_read_only_status_response_stream(estop, frames);
    assert_eq!(demux.scanned_frame_count, 2);
    assert_eq!(demux.matching_frame_index, Some(1));
    assert_eq!(demux.skipped_diagnostic_frame_count, 1);
    assert_eq!(demux.skipped_malformed_or_desynchronized_frame_count, 0);
    assert_eq!(demux.skipped_non_matching_registry_frame_count, 0);
    assert_eq!(
        demux.frame_diagnoses[0].disposition,
        MozaReadOnlyStatusResponseFrameDisposition::SkippableDiagnosticFrame
    );
    assert_eq!(
        demux.frame_diagnoses[1].disposition,
        MozaReadOnlyStatusResponseFrameDisposition::MatchingRegistryStatusResponse
    );
    assert!(demux.frame_diagnoses[1].matched_expected_command);

    Ok(())
}

#[test]
fn read_only_status_probe_demux_checked_in_matrix_has_no_matching_status_reply() -> TestResult {
    let matrix = checked_in_status_matrix()?;
    let responses = array_field(&matrix, "responses")?;
    let estop = read_only_status_commands()
        .find(|command| command.id == "estop_get_ffb")
        .ok_or_else(|| invalid_data("missing estop status command"))?;
    let frames = responses
        .iter()
        .map(|response| hex_to_bytes(str_field(response, "response_frame_hex")?))
        .collect::<Result<Vec<_>, _>>()?;
    let frame_slices = frames.iter().map(Vec::as_slice).collect::<Vec<_>>();

    let demux = demux_read_only_status_response_stream(estop, frame_slices);
    assert_eq!(demux.scanned_frame_count, 9);
    assert_eq!(demux.matching_frame_index, None);
    assert_eq!(demux.skipped_diagnostic_frame_count, 8);
    assert_eq!(demux.skipped_malformed_or_desynchronized_frame_count, 1);
    assert_eq!(demux.skipped_non_matching_registry_frame_count, 0);
    assert_eq!(demux.skipped_unknown_non_registry_frame_count, 0);

    Ok(())
}

#[test]
fn read_only_status_probe_demux_rejects_mismatched_registry_response() -> TestResult {
    let estop = read_only_status_commands()
        .find(|command| command.id == "estop_get_ffb")
        .ok_or_else(|| invalid_data("missing estop status command"))?;
    let compatibility = read_only_status_commands()
        .find(|command| command.id == "compatibility_get_mode")
        .ok_or_else(|| invalid_data("missing compatibility status command"))?;
    let compatibility_frame = encode_read_only_status_query(compatibility)?;

    let diagnosis = demux_read_only_status_response_frame(estop, &compatibility_frame);
    assert_eq!(
        diagnosis.disposition,
        MozaReadOnlyStatusResponseFrameDisposition::NonMatchingRegistryStatusResponse
    );
    assert!(!diagnosis.matched_expected_command);
    assert!(
        diagnosis
            .decode_error
            .as_deref()
            .unwrap_or_default()
            .contains("status response command mismatch")
    );

    let frames = [compatibility_frame.as_slice()];
    let demux = demux_read_only_status_response_stream(estop, frames);
    assert_eq!(demux.matching_frame_index, None);
    assert_eq!(demux.skipped_non_matching_registry_frame_count, 1);
    assert!(
        demux
            .first_non_matching_error
            .as_deref()
            .unwrap_or_default()
            .contains("status response command mismatch")
    );

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
