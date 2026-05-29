use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::fake_transport::{
    FAKE_TRANSPORT_CODEC_STATUS, MozaFakeSerialTransport, MozaFakeSerialTransportError,
};
use racing_wheel_hid_moza_protocol::serial::frame::{MozaSerialFrameError, serial_checksum};
use racing_wheel_hid_moza_protocol::serial::vendor_authority::{
    FORBIDDEN_VENDOR_CLASSES, MozaRiskClass, MozaSerialCodecStatus,
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

fn protocol_evidence_review() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json"
    ))
}

fn payload_rerun_endpoint_candidates() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../ci/hardware/moza-r5/2026-05-13/vendor-status-endpoint-candidates-from-payload-rerun.json"
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

fn usize_field(value: &Value, field: &str) -> Result<usize, io::Error> {
    let number = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_data(format!("missing integer field `{field}`")))?;
    usize::try_from(number)
        .map_err(|_| invalid_data(format!("field `{field}` is outside usize range")))
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

fn tuple_bytes(tuple_id: &str) -> Result<(u8, u8, u8), io::Error> {
    let mut parts = tuple_id.split('/');
    let group = parse_hex_byte(
        parts
            .next()
            .ok_or_else(|| invalid_data(format!("tuple `{tuple_id}` missing group")))?,
    )?;
    let device_id = parse_hex_byte(
        parts
            .next()
            .ok_or_else(|| invalid_data(format!("tuple `{tuple_id}` missing device id")))?,
    )?;
    let command = parse_hex_byte(
        parts
            .next()
            .ok_or_else(|| invalid_data(format!("tuple `{tuple_id}` missing command")))?,
    )?;
    if parts.next().is_some() {
        return Err(invalid_data(format!("tuple `{tuple_id}` has extra parts")));
    }
    Ok((group, device_id, command))
}

fn parse_hex_byte(value: &str) -> Result<u8, io::Error> {
    let trimmed = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or_else(|| value.trim());
    u8::from_str_radix(trimmed, 16)
        .map_err(|_| invalid_data(format!("invalid tuple byte `{value}`")))
}

fn representative_frame_hex(tuple_id: &str, payload_len: usize) -> Result<String, io::Error> {
    let (group, device_id, command) = tuple_bytes(tuple_id)?;
    let declared_len = payload_len
        .checked_add(1)
        .ok_or_else(|| invalid_data("representative payload length overflow"))?;
    let declared_len = u8::try_from(declared_len)
        .map_err(|_| invalid_data("representative payload length does not fit in u8"))?;
    let mut frame = vec![0x7e, declared_len, group, device_id, command];
    frame.resize(frame.len() + payload_len, 0);
    let checksum = serial_checksum(&frame);
    frame.push(checksum);
    Ok(frame
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>())
}

fn mode_enable_candidates(review: &Value) -> Result<&Vec<Value>, io::Error> {
    array_at(
        review,
        "/passive_tuple_registry_coverage/decode_candidate_mode_enable_review/candidates",
    )
}

fn command_id_0x07_analogs(endpoint_review: &Value) -> Result<&Vec<Value>, io::Error> {
    array_field(endpoint_review, "passive_command_id_0x07_analogs")
}

fn candidates_by_id(candidates: &[Value]) -> Result<BTreeMap<&str, &Value>, io::Error> {
    let mut by_id = BTreeMap::new();
    for candidate in candidates {
        let candidate_id = str_field(candidate, "candidate_id")?;
        if by_id.insert(candidate_id, candidate).is_some() {
            return Err(invalid_data(format!(
                "duplicate candidate id `{candidate_id}`"
            )));
        }
    }
    Ok(by_id)
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
    assert!(!bool_field(&fixture, "smoke_ready")?);
    assert!(!bool_field(&fixture, "release_ready")?);
    assert!(!bool_field(&fixture, "registry_promotion_claim")?);
    assert!(!bool_field(&fixture, "output_sendability_claim")?);
    assert!(!bool_field(&fixture, "opened_serial_device")?);
    assert!(!bool_field(&fixture, "sent_read_only_query_commands")?);
    assert!(!bool_field(&fixture, "sent_output_writes")?);
    assert!(!bool_field(&fixture, "sent_configuration_writes")?);
    assert!(!bool_field(&fixture, "sent_firmware_or_dfu_commands")?);
    assert!(!bool_field(&fixture, "high_torque_enabled")?);
    assert!(!bool_field(&fixture, "real_hardware_validated")?);
    assert_eq!(
        usize_field(&fixture, "mode_enable_candidate_group_count")?,
        2
    );
    assert_eq!(
        usize_field(&fixture, "mode_enable_candidate_frame_count")?,
        5
    );
    assert_eq!(
        usize_field(&fixture, "mode_enable_candidate_send_path_rejected_count")?,
        5
    );
    assert!(bool_field(
        &fixture,
        "mode_enable_candidates_unknown_do_not_send"
    )?);
    assert_eq!(
        usize_field(&fixture, "authority_status_endpoint_candidate_group_count")?,
        2
    );
    assert_eq!(
        usize_field(&fixture, "authority_status_endpoint_candidate_frame_count")?,
        5
    );
    assert_eq!(
        usize_field(
            &fixture,
            "authority_status_endpoint_candidate_send_path_rejected_count"
        )?,
        5
    );
    assert!(bool_field(
        &fixture,
        "authority_status_endpoint_candidates_unknown_do_not_send"
    )?);
    assert!(!bool_field(
        &fixture,
        "authority_status_endpoint_candidates_match_payload_status"
    )?);
    assert!(!bool_field(&fixture, "corrected_read_only_probe_ready")?);
    assert_eq!(
        usize_field(&fixture, "authority_command_id_0x07_analog_count")?,
        5
    );
    assert_eq!(
        usize_field(&fixture, "authority_command_id_0x07_analog_frame_count")?,
        5
    );
    assert_eq!(
        usize_field(
            &fixture,
            "authority_command_id_0x07_analog_send_path_rejected_count"
        )?,
        5
    );
    assert!(bool_field(
        &fixture,
        "authority_command_id_0x07_analogs_unknown_do_not_send"
    )?);
    assert!(!bool_field(
        &fixture,
        "authority_command_id_0x07_analogs_match_payload_status"
    )?);
    assert!(!bool_field(
        &fixture,
        "authority_command_id_0x07_analogs_read_only_probe_allowed"
    )?);
    assert!(!bool_field(
        &fixture,
        "authority_command_id_0x07_analogs_sendable"
    )?);
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
        "smoke_ready",
        "release_ready",
        "registry_promotion_claim",
        "output_sendability_claim",
        "transport_kind",
        "fake_transport_verified",
        "opened_serial_device",
        "sent_read_only_query_commands",
        "sent_output_writes",
        "sent_configuration_writes",
        "sent_firmware_or_dfu_commands",
        "high_torque_enabled",
        "real_hardware_validated",
        "accepted_fixture_ids",
        "blocked_fixture_ids",
        "mode_enable_candidate_group_count",
        "mode_enable_candidate_frame_count",
        "mode_enable_candidate_send_path_rejected_count",
        "mode_enable_candidates_unknown_do_not_send",
        "authority_status_endpoint_candidate_group_count",
        "authority_status_endpoint_candidate_frame_count",
        "authority_status_endpoint_candidate_send_path_rejected_count",
        "authority_status_endpoint_candidates_unknown_do_not_send",
        "authority_status_endpoint_candidates_match_payload_status",
        "corrected_read_only_probe_ready",
        "authority_command_id_0x07_analog_fixture_source",
        "authority_command_id_0x07_analog_count",
        "authority_command_id_0x07_analog_frame_count",
        "authority_command_id_0x07_analog_send_path_rejected_count",
        "authority_command_id_0x07_analogs_unknown_do_not_send",
        "authority_command_id_0x07_analogs_match_payload_status",
        "authority_command_id_0x07_analogs_read_only_probe_allowed",
        "authority_command_id_0x07_analogs_sendable",
        "authority_command_id_0x07_analog_observations",
        "authority_status_endpoint_candidate_observations",
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

#[test]
fn fake_transport_observes_mode_enable_candidates_without_sendability() -> TestResult {
    let review = protocol_evidence_review()?;
    let candidates = mode_enable_candidates(&review)?;
    let mut transport = MozaFakeSerialTransport::new();
    let mut observed_frame_count = 0usize;

    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        assert_eq!(str_field(candidate, "risk_class")?, "unknown_do_not_send");
        assert!(!bool_field(candidate, "semantic_decode_claim")?);
        assert!(!bool_field(candidate, "registry_promotion_claim")?);
        assert!(!bool_field(candidate, "hardware_output_authorized")?);
        assert!(!bool_field(candidate, "native_control_evidence")?);
        assert!(!bool_field(candidate, "output_sendability_claim")?);

        let candidate_id = str_field(candidate, "candidate_id")?;
        let semantics = array_field(candidate, "candidate_semantics")?
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .ok_or_else(|| invalid_data("candidate semantic must be a string"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let frame_hexes = array_field(candidate, "representative_frame_hexes")?;

        for frame_hex in frame_hexes {
            let frame_hex = frame_hex
                .as_str()
                .ok_or_else(|| invalid_data("representative frame must be a string"))?;
            let frame = hex_to_bytes(frame_hex)?;
            let observation = transport.observe_mode_enable_candidate_fixture_frame(&frame)?;

            assert_eq!(observation.candidate_id, candidate_id);
            assert_eq!(observation.semantic_hypothesis, candidate_id);
            assert_eq!(observation.risk_class, MozaRiskClass::UnknownDoNotSend);
            assert!(!observation.semantic_decode_claim);
            assert!(!observation.registry_promotion_claim);
            assert!(!observation.hardware_output_authorized);
            assert!(!observation.native_control_evidence);
            assert!(!observation.output_sendability_claim);

            for semantic in &semantics {
                assert!(
                    observation.candidate_semantics.contains(semantic),
                    "candidate `{candidate_id}` missing semantic question `{semantic}`"
                );
            }
            observed_frame_count = observed_frame_count.saturating_add(1);
        }
    }

    assert_eq!(observed_frame_count, 5);
    assert_eq!(
        transport.mode_enable_candidate_observations().len(),
        observed_frame_count
    );
    assert!(
        transport.exchanges().is_empty(),
        "observed mode/enable candidates must not create command exchanges"
    );

    Ok(())
}

#[test]
fn mode_enable_candidates_cannot_use_command_send_path() -> TestResult {
    let review = protocol_evidence_review()?;
    let candidates = mode_enable_candidates(&review)?;
    let mut transport = MozaFakeSerialTransport::new();
    let mut rejected_frame_count = 0usize;

    for candidate in candidates {
        for frame_hex in array_field(candidate, "representative_frame_hexes")? {
            let frame_hex = frame_hex
                .as_str()
                .ok_or_else(|| invalid_data("representative frame must be a string"))?;
            let frame = hex_to_bytes(frame_hex)?;
            assert!(
                matches!(
                    transport.submit_read_only_fixture_frame(&frame),
                    Err(MozaFakeSerialTransportError::Frame(
                        MozaSerialFrameError::UnknownCommand { .. }
                    ))
                ),
                "mode/enable candidate frame `{frame_hex}` must not enter the command send path"
            );
            rejected_frame_count = rejected_frame_count.saturating_add(1);
        }
    }

    assert_eq!(rejected_frame_count, 5);
    assert!(transport.exchanges().is_empty());

    Ok(())
}

#[test]
fn authority_status_endpoint_candidates_remain_fake_only_and_non_sendable() -> TestResult {
    let endpoint_review = payload_rerun_endpoint_candidates()?;
    let protocol_review = protocol_evidence_review()?;
    let mode_enable_by_id = candidates_by_id(mode_enable_candidates(&protocol_review)?)?;
    let endpoint_candidates = array_field(&endpoint_review, "passive_tuple_candidates")?;
    let mut transport = MozaFakeSerialTransport::new();
    let mut observed_frame_count = 0usize;
    let mut send_path_rejected_count = 0usize;

    assert_eq!(
        str_field(&endpoint_review, "source_diagnosis_classification")?,
        "authority_status_endpoint_specific_debug_telemetry_without_payload"
    );
    assert!(!bool_field(
        &endpoint_review,
        "corrected_read_only_probe_ready"
    )?);
    assert!(!bool_field(&endpoint_review, "output_sendability_claim")?);
    assert!(!bool_field(&endpoint_review, "registry_promotion_claim")?);
    assert!(!bool_field(&endpoint_review, "semantic_decode_claim")?);
    assert_eq!(endpoint_candidates.len(), 2);

    for endpoint_candidate in endpoint_candidates {
        assert_eq!(
            str_field(endpoint_candidate, "risk_class")?,
            "unknown_do_not_send"
        );
        assert!(!bool_field(endpoint_candidate, "read_only_probe_allowed")?);
        assert!(!bool_field(endpoint_candidate, "output_sendability_claim")?);
        assert!(!bool_field(endpoint_candidate, "registry_promotion_claim")?);
        assert!(!bool_field(endpoint_candidate, "semantic_decode_claim")?);

        let passive_hypothesis = str_field(endpoint_candidate, "passive_hypothesis")?;
        let mode_enable_candidate = mode_enable_by_id.get(passive_hypothesis).ok_or_else(|| {
            invalid_data(format!(
                "endpoint candidate references missing passive hypothesis `{passive_hypothesis}`"
            ))
        })?;
        let endpoint_tuple_count = array_field(endpoint_candidate, "tuple_ids")?.len();
        assert_eq!(
            endpoint_tuple_count,
            array_field(mode_enable_candidate, "tuple_ids")?.len()
        );
        let expected_semantics = array_field(endpoint_candidate, "question_scope")?
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .ok_or_else(|| invalid_data("endpoint question scope must contain strings"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        for frame_hex in array_field(mode_enable_candidate, "representative_frame_hexes")? {
            let frame_hex = frame_hex
                .as_str()
                .ok_or_else(|| invalid_data("representative frame must be a string"))?;
            let frame = hex_to_bytes(frame_hex)?;
            let observation =
                transport.observe_authority_status_endpoint_candidate_fixture_frame(&frame)?;

            assert_eq!(observation.candidate_id, passive_hypothesis);
            assert_eq!(observation.semantic_hypothesis, passive_hypothesis);
            assert_eq!(observation.risk_class, MozaRiskClass::UnknownDoNotSend);
            assert!(!observation.matches_payload_authority_status_response);
            assert!(!observation.corrected_read_only_probe_ready);
            assert!(!observation.semantic_decode_claim);
            assert!(!observation.registry_promotion_claim);
            assert!(!observation.hardware_output_authorized);
            assert!(!observation.native_control_evidence);
            assert!(!observation.output_sendability_claim);
            for semantic in &expected_semantics {
                assert!(
                    observation.candidate_semantics.contains(semantic),
                    "endpoint candidate `{passive_hypothesis}` missing semantic question `{semantic}`"
                );
            }
            observed_frame_count = observed_frame_count.saturating_add(1);

            assert!(
                matches!(
                    transport.submit_read_only_fixture_frame(&frame),
                    Err(MozaFakeSerialTransportError::Frame(
                        MozaSerialFrameError::UnknownCommand { .. }
                    ))
                ),
                "authority-status endpoint candidate frame `{frame_hex}` must not enter the command send path"
            );
            send_path_rejected_count = send_path_rejected_count.saturating_add(1);
        }
    }

    assert_eq!(observed_frame_count, 5);
    assert_eq!(send_path_rejected_count, 5);
    assert_eq!(
        transport
            .authority_status_endpoint_candidate_observations()
            .len(),
        observed_frame_count
    );
    assert!(transport.exchanges().is_empty());

    Ok(())
}

#[test]
fn authority_command_id_0x07_analogs_remain_fake_only_and_non_sendable() -> TestResult {
    let endpoint_review = payload_rerun_endpoint_candidates()?;
    let analogs = command_id_0x07_analogs(&endpoint_review)?;
    let mut transport = MozaFakeSerialTransport::new();
    let mut observed_frame_count = 0usize;
    let mut send_path_rejected_count = 0usize;

    assert_eq!(analogs.len(), 5);
    assert!(!bool_field(
        &endpoint_review,
        "passive_command_id_0x07_analogs_match_payload_status"
    )?);
    assert!(!bool_field(
        &endpoint_review,
        "passive_command_id_0x07_analogs_read_only_probe_allowed"
    )?);
    assert!(!bool_field(
        &endpoint_review,
        "passive_command_id_0x07_analogs_sendable"
    )?);

    for analog in analogs {
        let tuple_id = str_field(analog, "tuple_id")?;
        assert_eq!(str_field(analog, "risk_class")?, "unknown_do_not_send");
        assert_eq!(str_field(analog, "registry_status")?, "unknown_commanded");
        assert!(bool_field(analog, "command_id_matches_expected_source")?);
        assert!(!bool_field(
            analog,
            "response_shape_matches_expected_authority_status"
        )?);
        assert!(!bool_field(analog, "read_only_probe_allowed")?);
        assert!(!bool_field(analog, "output_sendability_claim")?);
        assert!(!bool_field(analog, "registry_promotion_claim")?);
        assert!(!bool_field(analog, "semantic_decode_claim")?);

        let payload_len = usize_field(analog, "payload_len_min")?;
        assert_eq!(payload_len, usize_field(analog, "payload_len_max")?);
        let frame_hex = representative_frame_hex(tuple_id, payload_len)?;
        let frame = hex_to_bytes(&frame_hex)?;
        let observation =
            transport.observe_authority_command_id_0x07_analog_fixture_frame(&frame)?;

        assert_eq!(observation.candidate_id, "authority_command_id_0x07_analog");
        assert_eq!(observation.tuple_id, tuple_id);
        assert_eq!(observation.payload_len, payload_len);
        assert_eq!(observation.risk_class, MozaRiskClass::UnknownDoNotSend);
        assert!(!observation.matches_payload_authority_status_response);
        assert!(!observation.corrected_read_only_probe_ready);
        assert!(!observation.read_only_probe_allowed);
        assert!(!observation.semantic_decode_claim);
        assert!(!observation.registry_promotion_claim);
        assert!(!observation.hardware_output_authorized);
        assert!(!observation.native_control_evidence);
        assert!(!observation.output_sendability_claim);
        for semantic in [
            "authority_state_status_endpoint_question",
            "payload_bearing_read_only_status_question",
            "command_id_0x07_analog",
        ] {
            assert!(
                observation.candidate_semantics.contains(&semantic),
                "authority command-id analog `{tuple_id}` missing semantic question `{semantic}`"
            );
        }
        observed_frame_count = observed_frame_count.saturating_add(1);

        assert!(
            matches!(
                transport.submit_read_only_fixture_frame(&frame),
                Err(MozaFakeSerialTransportError::Frame(
                    MozaSerialFrameError::UnknownCommand { .. }
                ))
            ),
            "authority command-id analog frame `{frame_hex}` must not enter the command send path"
        );
        send_path_rejected_count = send_path_rejected_count.saturating_add(1);
    }

    assert_eq!(observed_frame_count, 5);
    assert_eq!(send_path_rejected_count, 5);
    assert_eq!(
        transport
            .authority_command_id_0x07_analog_observations()
            .len(),
        observed_frame_count
    );
    assert!(transport.exchanges().is_empty());

    Ok(())
}

#[test]
fn fake_transport_keeps_forbidden_classes_out_of_send_paths() -> TestResult {
    assert!(!MozaRiskClass::UnknownDoNotSend.is_encodable());
    assert!(!MozaRiskClass::UnknownDoNotSend.can_send_without_exact_authorization());
    assert!(!MozaRiskClass::UnknownDoNotSend.requires_exact_authorization());
    assert!(!MozaRiskClass::FirmwareOrDfuForbidden.is_encodable());
    assert!(!MozaRiskClass::FirmwareOrDfuForbidden.can_send_without_exact_authorization());
    assert!(!MozaRiskClass::FirmwareOrDfuForbidden.requires_exact_authorization());

    for (class_id, risk_class) in FORBIDDEN_VENDOR_CLASSES {
        assert!(
            !risk_class.can_send_without_exact_authorization(),
            "forbidden class `{class_id}` must not be sendable without review"
        );
    }

    let fixture = transport_fixture()?;
    assert!(!bool_field(&fixture, "high_torque_enabled")?);
    assert!(!bool_field(&fixture, "sent_firmware_or_dfu_commands")?);
    assert!(
        array_field(&fixture, "blocked_actions")?
            .iter()
            .any(|action| action.as_str() == Some("high torque"))
    );
    assert!(
        array_field(&fixture, "blocked_actions")?
            .iter()
            .any(|action| action.as_str() == Some("firmware or DFU command"))
    );
    assert!(
        array_field(&fixture, "blocked_actions")?
            .iter()
            .any(|action| action.as_str() == Some("unknown host-to-device command"))
    );

    Ok(())
}
