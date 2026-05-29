use std::collections::BTreeSet;
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::response_semantics::{
    MozaPassiveResponsePayloadClass, MozaPassiveResponseSemanticError,
    SESSION_AUTHORITY_PAIR_RESPONSE_GROUP_ID, STATUS_MODE_TRIAD_RESPONSE_GROUP_ID,
    decode_passive_response_semantic_fixture,
};
use racing_wheel_hid_moza_protocol::serial::vendor_authority::MozaRiskClass;
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

fn response_source_correlation() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../ci/hardware/moza-r5/2026-05-13/vendor-status-response-source-correlation.json"
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

fn correlated_response_tuple_ids(correlation: &Value) -> Result<BTreeSet<String>, io::Error> {
    let mut tuple_ids = BTreeSet::new();
    for group in array_field(correlation, "response_correlation_groups")? {
        if !bool_field(group, "sample_scoped_response_correlation_found")? {
            continue;
        }
        for tuple_id in array_field(group, "matched_expected_response_tuple_ids")? {
            let tuple_id = tuple_id
                .as_str()
                .ok_or_else(|| invalid_data("matched response tuple id must be a string"))?;
            tuple_ids.insert(tuple_id.to_string());
        }
    }
    Ok(tuple_ids)
}

fn correlated_response_samples<'a>(
    review: &'a Value,
    tuple_ids: &BTreeSet<String>,
) -> Result<Vec<&'a Value>, io::Error> {
    let scenarios = array_at(review, "/sniff_evidence/scenarios")?;
    let mut samples = Vec::new();
    for scenario in scenarios {
        for sample in array_field(scenario, "device_to_host_serial_frame_tuple_samples")? {
            let tuple_id = str_field(sample, "tuple_id")?;
            if tuple_ids.contains(tuple_id) {
                samples.push(sample);
            }
        }
    }
    Ok(samples)
}

#[test]
fn correlated_passive_response_samples_have_fixture_decoder_coverage() -> TestResult {
    let correlation = response_source_correlation()?;
    let review = protocol_evidence_review()?;
    let tuple_ids = correlated_response_tuple_ids(&correlation)?;
    let samples = correlated_response_samples(&review, &tuple_ids)?;
    let mut observed_groups = BTreeSet::new();
    let mut observed_tuples = BTreeSet::new();

    assert_eq!(tuple_ids.len(), 5);
    assert_eq!(samples.len(), 11);
    assert!(!bool_field(&correlation, "live_read_only_probe_allowed")?);
    assert!(!bool_field(&correlation, "authorization_plan_allowed")?);
    assert!(!bool_field(&correlation, "motion_attempt_allowed")?);

    for sample in samples {
        let frame = hex_to_bytes(str_field(sample, "frame_hex")?)?;
        let observation = decode_passive_response_semantic_fixture(&frame)?;

        assert!(tuple_ids.contains(&observation.tuple_id));
        assert_eq!(observation.risk_class, MozaRiskClass::UnknownDoNotSend);
        assert!(observation.fixture_decoder_coverage);
        assert_eq!(
            observation.payload_class,
            MozaPassiveResponsePayloadClass::ZeroFilled
        );
        assert!(!observation.payload_variation_observed);
        assert!(!observation.semantic_decode_claim);
        assert!(!observation.registry_promotion_claim);
        assert!(!observation.read_only_probe_allowed);
        assert!(!observation.corrected_read_only_probe_ready);
        assert!(!observation.hardware_output_authorized);
        assert!(!observation.native_control_evidence);
        assert!(!observation.output_sendability_claim);

        observed_groups.insert(observation.group_id);
        observed_tuples.insert(observation.tuple_id);
    }

    assert!(observed_groups.contains(STATUS_MODE_TRIAD_RESPONSE_GROUP_ID));
    assert!(observed_groups.contains(SESSION_AUTHORITY_PAIR_RESPONSE_GROUP_ID));
    assert_eq!(observed_tuples, tuple_ids);

    Ok(())
}

#[test]
fn registry_authority_response_tuples_remain_absent_from_passive_samples() -> TestResult {
    let correlation = response_source_correlation()?;

    assert!(!bool_field(
        &correlation,
        "registry_authority_response_tuple_found"
    )?);
    assert!(!bool_field(
        &correlation,
        "payload_bearing_authority_state_source_found"
    )?);
    assert!(!bool_field(
        &correlation,
        "reviewed_equivalent_status_source_found"
    )?);
    assert!(!bool_field(
        &correlation,
        "corrected_read_only_probe_ready"
    )?);

    for tuple_id in ["0xA1/0x21/0x07", "0xC6/0xC1/0x01"] {
        let frame = match tuple_id {
            "0xA1/0x21/0x07" => "7E02A121070157",
            "0xC6/0xC1/0x01" => "7E02C6C1010116",
            _ => return Err(Box::new(invalid_data("unexpected tuple id"))),
        };
        let frame = hex_to_bytes(frame)?;
        assert!(matches!(
            decode_passive_response_semantic_fixture(&frame),
            Err(MozaPassiveResponseSemanticError::UnreviewedResponseTuple { .. })
        ));
    }

    Ok(())
}
