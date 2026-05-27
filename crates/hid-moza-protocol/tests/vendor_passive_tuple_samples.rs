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

fn payload_shape_for_tuple<'a>(
    shapes: &'a [Value],
    tuple_id: &str,
) -> Result<&'a Value, io::Error> {
    shapes
        .iter()
        .find(|shape| str_field(shape, "tuple_id").is_ok_and(|id| id == tuple_id))
        .ok_or_else(|| invalid_data(format!("missing payload shape for tuple `{tuple_id}`")))
}

fn semantic_hypothesis_for_tuple<'a>(
    hypotheses: &'a [Value],
    tuple_id: &str,
) -> Result<&'a Value, io::Error> {
    hypotheses
        .iter()
        .find(|hypothesis| str_field(hypothesis, "tuple_id").is_ok_and(|id| id == tuple_id))
        .ok_or_else(|| {
            invalid_data(format!(
                "missing semantic hypothesis for tuple `{tuple_id}`"
            ))
        })
}

fn semantic_correlation_target_for_hypothesis<'a>(
    targets: &'a [Value],
    semantic_hypothesis: &str,
) -> Result<&'a Value, io::Error> {
    targets
        .iter()
        .find(|target| {
            str_field(target, "semantic_hypothesis").is_ok_and(|id| id == semantic_hypothesis)
        })
        .ok_or_else(|| {
            invalid_data(format!(
                "missing semantic correlation target for `{semantic_hypothesis}`"
            ))
        })
}

fn string_array_at<'a>(value: &'a Value, pointer: &str) -> Result<Vec<&'a str>, io::Error> {
    array_at(value, pointer)?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .ok_or_else(|| invalid_data(format!("non-string entry at `{pointer}`")))
        })
        .collect()
}

fn packet_pattern_for_sequence<'a>(
    patterns: &'a [Value],
    expected_sequence: &[&str],
) -> Result<&'a Value, io::Error> {
    for pattern in patterns {
        if string_array_at(pattern, "/tuple_sequence")? == expected_sequence {
            return Ok(pattern);
        }
    }

    Err(invalid_data(format!(
        "missing packet pattern `{}`",
        expected_sequence.join(" -> ")
    )))
}

fn repeated_motif_for_sequence<'a>(
    motifs: &'a [Value],
    expected_sequence: &[&str],
) -> Result<&'a Value, io::Error> {
    for motif in motifs {
        if string_array_at(motif, "/tuple_sequence")? == expected_sequence {
            return Ok(motif);
        }
    }

    Err(invalid_data(format!(
        "missing repeated motif `{}`",
        expected_sequence.join(" -> ")
    )))
}

fn packet_group_for<'a>(
    groups: &'a [Value],
    scenario: &str,
    packet_ordinal: usize,
) -> Result<&'a Value, io::Error> {
    groups
        .iter()
        .find(|group| {
            str_field(group, "scenario").is_ok_and(|value| value == scenario)
                && usize_field(group, "packet_ordinal").is_ok_and(|value| value == packet_ordinal)
        })
        .ok_or_else(|| {
            invalid_data(format!(
                "missing packet group `{scenario}`/{packet_ordinal}"
            ))
        })
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
fn passive_decode_candidate_samples_preserve_non_sendable_semantic_hypotheses() -> TestResult {
    let review = protocol_evidence_review()?;
    let summary = value_at(
        &review,
        "/passive_tuple_registry_coverage/decode_candidate_semantic_hypothesis_summary",
    )?;

    assert_eq!(
        str_field(summary, "claim_scope")?,
        "no_output_passive_tuple_semantic_hypothesis_review"
    );
    assert_eq!(
        str_field(summary, "sample_scope")?,
        "highest_frequency_unknown_commanded_tuples"
    );
    assert_eq!(usize_field(summary, "tuple_count")?, 5);
    assert_eq!(usize_field(summary, "hypothesis_count")?, 5);
    assert!(bool_field(summary, "all_hypotheses_unknown_commanded")?);
    assert!(bool_field(summary, "all_hypotheses_non_sendable")?);
    assert!(!bool_field(summary, "semantic_decode_claim")?);
    assert!(!bool_field(summary, "registry_promotion_claim")?);
    assert!(!bool_field(summary, "hardware_output_authorized")?);
    assert!(!bool_field(summary, "native_control_evidence")?);
    assert!(!bool_field(summary, "output_sendability_claim")?);
    assert!(!bool_field(
        summary,
        "protocol_evidence_sufficient_for_output_plan"
    )?);

    let hypotheses = array_at(summary, "/tuple_hypotheses")?;
    let keepalive = semantic_hypothesis_for_tuple(hypotheses, "0x5A/0x1B/0x00")?;
    assert_eq!(
        str_field(keepalive, "observed_pattern_hint")?,
        "repeated_high_frequency_0x1b_pair"
    );
    assert_eq!(
        str_field(keepalive, "semantic_hypothesis")?,
        "session_or_status_keepalive_candidate"
    );
    assert_eq!(str_field(keepalive, "confidence")?, "low_pattern_only");
    assert!(!bool_field(keepalive, "semantic_decode_claim")?);
    assert!(!bool_field(keepalive, "registry_promotion_claim")?);
    assert!(!bool_field(keepalive, "hardware_output_authorized")?);
    assert!(!bool_field(keepalive, "output_sendability_claim")?);

    let triad = semantic_hypothesis_for_tuple(hypotheses, "0x25/0x19/0x02")?;
    assert_eq!(
        str_field(triad, "observed_pattern_hint")?,
        "repeated_zero_payload_0x19_triad"
    );
    assert_eq!(
        str_field(triad, "semantic_hypothesis")?,
        "base_status_or_mode_poll_candidate"
    );
    assert_eq!(str_field(triad, "confidence")?, "low_pattern_only");
    assert!(!bool_field(triad, "semantic_decode_claim")?);
    assert!(!bool_field(triad, "registry_promotion_claim")?);
    assert!(!bool_field(triad, "hardware_output_authorized")?);
    assert!(!bool_field(triad, "output_sendability_claim")?);

    Ok(())
}

#[test]
fn passive_decode_candidate_samples_preserve_non_sendable_correlation_plan() -> TestResult {
    let review = protocol_evidence_review()?;
    let plan = value_at(
        &review,
        "/passive_tuple_registry_coverage/decode_candidate_semantic_correlation_plan",
    )?;

    assert_eq!(
        str_field(plan, "claim_scope")?,
        "no_output_passive_tuple_semantic_correlation_plan"
    );
    assert_eq!(
        str_field(plan, "source_hypothesis_scope")?,
        "no_output_passive_tuple_semantic_hypothesis_review"
    );
    assert_eq!(usize_field(plan, "hypothesis_count")?, 5);
    assert_eq!(usize_field(plan, "correlation_target_count")?, 2);
    assert!(bool_field(plan, "all_targets_non_sendable")?);
    assert!(!bool_field(plan, "semantic_decode_claim")?);
    assert!(!bool_field(plan, "registry_promotion_claim")?);
    assert!(!bool_field(plan, "hardware_output_authorized")?);
    assert!(!bool_field(plan, "native_control_evidence")?);
    assert!(!bool_field(plan, "output_sendability_claim")?);
    assert!(!bool_field(
        plan,
        "protocol_evidence_sufficient_for_output_plan"
    )?);

    let targets = array_at(plan, "/targets")?;
    let keepalive = semantic_correlation_target_for_hypothesis(
        targets,
        "session_or_status_keepalive_candidate",
    )?;
    assert_eq!(
        string_array_at(keepalive, "/tuple_ids")?,
        ["0x5A/0x1B/0x00", "0x5D/0x1B/0x01"]
    );
    assert_eq!(
        string_array_at(keepalive, "/observed_completed_scenarios")?,
        [
            "pit-house-open-idle",
            "pit-house-full-controls",
            "pit-house-setting-change",
        ]
    );
    assert_eq!(
        string_array_at(keepalive, "/missing_correlation_scenarios")?,
        [
            "simhub-open-idle",
            "simhub-output-session",
            "simulator-session-start-stop"
        ]
    );
    assert_eq!(
        str_field(keepalive, "next_capture_priority")?,
        "simhub-open-idle"
    );
    assert!(!bool_field(keepalive, "output_sendability_claim")?);

    let triad =
        semantic_correlation_target_for_hypothesis(targets, "base_status_or_mode_poll_candidate")?;
    assert_eq!(
        string_array_at(triad, "/tuple_ids")?,
        ["0x25/0x19/0x01", "0x25/0x19/0x02", "0x25/0x19/0x03"]
    );
    assert_eq!(
        str_field(triad, "next_capture_priority")?,
        "simhub-open-idle"
    );
    assert!(!bool_field(triad, "semantic_decode_claim")?);
    assert!(!bool_field(triad, "registry_promotion_claim")?);
    assert!(!bool_field(triad, "hardware_output_authorized")?);
    assert!(!bool_field(triad, "output_sendability_claim")?);

    assert!(
        string_array_at(plan, "/required_artifacts")?
            .iter()
            .any(|artifact| artifact.ends_with("simhub-open-idle/sniff-summary.json"))
    );
    assert!(
        string_array_at(plan, "/required_artifacts")?
            .iter()
            .all(|artifact| !artifact.contains("pit-house-setting-change"))
    );

    Ok(())
}

#[test]
fn passive_decode_candidate_samples_preserve_packet_group_hints() -> TestResult {
    let review = protocol_evidence_review()?;
    let summary = value_at(
        &review,
        "/passive_tuple_registry_coverage/decode_candidate_packet_group_summary",
    )?;

    assert_eq!(
        str_field(summary, "claim_scope")?,
        "no_output_passive_tuple_packet_group_review"
    );
    assert_eq!(
        str_field(summary, "sample_scope")?,
        "highest_frequency_unknown_commanded_tuples"
    );
    assert_eq!(usize_field(summary, "packet_group_count")?, 15);
    assert_eq!(usize_field(summary, "sample_count")?, 45);
    assert_eq!(usize_field(summary, "unique_packet_pattern_count")?, 3);
    assert_eq!(usize_field(summary, "repeated_contiguous_motif_count")?, 7);
    assert!(bool_field(summary, "all_packet_groups_checksum_valid")?);
    assert!(bool_field(summary, "all_packet_groups_unknown_commanded")?);
    assert!(bool_field(summary, "all_packet_groups_non_sendable")?);
    assert!(!bool_field(summary, "hardware_output_authorized")?);
    assert!(!bool_field(summary, "native_control_evidence")?);
    assert!(!bool_field(summary, "output_sendability_claim")?);
    assert!(!bool_field(
        summary,
        "protocol_evidence_sufficient_for_output_plan"
    )?);

    let groups = array_at(summary, "/packet_groups")?;
    let combined = packet_group_for(groups, "pit-house-full-controls", 3)?;
    assert_eq!(usize_field(combined, "frame_ordinal_min")?, 1);
    assert_eq!(usize_field(combined, "frame_ordinal_max")?, 5);
    assert_eq!(
        string_array_at(combined, "/tuple_sequence")?,
        Vec::from([
            "0x5A/0x1B/0x00",
            "0x5D/0x1B/0x01",
            "0x25/0x19/0x02",
            "0x25/0x19/0x03",
            "0x25/0x19/0x01",
        ])
    );
    assert_eq!(usize_field(combined, "sample_count")?, 5);
    assert!(bool_field(combined, "all_samples_checksum_valid")?);
    assert!(bool_field(combined, "all_samples_unknown_commanded")?);
    assert!(!bool_field(combined, "hardware_output_authorized")?);
    assert!(!bool_field(combined, "output_sendability_claim")?);

    let setting_change_combined = packet_group_for(groups, "pit-house-setting-change", 3)?;
    assert_eq!(
        usize_field(setting_change_combined, "frame_ordinal_min")?,
        1
    );
    assert_eq!(
        usize_field(setting_change_combined, "frame_ordinal_max")?,
        5
    );
    assert_eq!(
        string_array_at(setting_change_combined, "/tuple_sequence")?,
        Vec::from([
            "0x5A/0x1B/0x00",
            "0x5D/0x1B/0x01",
            "0x25/0x19/0x02",
            "0x25/0x19/0x03",
            "0x25/0x19/0x01",
        ])
    );
    assert_eq!(usize_field(setting_change_combined, "sample_count")?, 5);
    assert!(bool_field(
        setting_change_combined,
        "all_samples_checksum_valid"
    )?);
    assert!(bool_field(
        setting_change_combined,
        "all_samples_unknown_commanded"
    )?);
    assert!(!bool_field(
        setting_change_combined,
        "hardware_output_authorized"
    )?);
    assert!(!bool_field(
        setting_change_combined,
        "output_sendability_claim"
    )?);

    let patterns = array_at(summary, "/packet_patterns")?;
    let pair = packet_pattern_for_sequence(patterns, &["0x5A/0x1B/0x00", "0x5D/0x1B/0x01"])?;
    assert_eq!(usize_field(pair, "observed_packet_count")?, 6);
    assert_eq!(usize_field(pair, "sample_count")?, 12);
    assert_eq!(usize_field(pair, "scenario_count")?, 3);
    assert!(!bool_field(pair, "hardware_output_authorized")?);
    assert!(!bool_field(pair, "output_sendability_claim")?);

    let triad = packet_pattern_for_sequence(
        patterns,
        &["0x25/0x19/0x02", "0x25/0x19/0x03", "0x25/0x19/0x01"],
    )?;
    assert_eq!(usize_field(triad, "observed_packet_count")?, 6);
    assert_eq!(usize_field(triad, "sample_count")?, 18);
    assert_eq!(usize_field(triad, "scenario_count")?, 3);
    assert!(!bool_field(triad, "hardware_output_authorized")?);
    assert!(!bool_field(triad, "output_sendability_claim")?);

    let combined_pattern = packet_pattern_for_sequence(
        patterns,
        &[
            "0x5A/0x1B/0x00",
            "0x5D/0x1B/0x01",
            "0x25/0x19/0x02",
            "0x25/0x19/0x03",
            "0x25/0x19/0x01",
        ],
    )?;
    assert_eq!(usize_field(combined_pattern, "observed_packet_count")?, 3);
    assert_eq!(usize_field(combined_pattern, "sample_count")?, 15);
    assert_eq!(usize_field(combined_pattern, "scenario_count")?, 2);
    assert!(!bool_field(combined_pattern, "hardware_output_authorized")?);
    assert!(!bool_field(combined_pattern, "output_sendability_claim")?);

    let motifs = array_at(summary, "/repeated_contiguous_motifs")?;
    let repeated_pair = repeated_motif_for_sequence(motifs, &["0x5A/0x1B/0x00", "0x5D/0x1B/0x01"])?;
    assert_eq!(usize_field(repeated_pair, "motif_len")?, 2);
    assert_eq!(usize_field(repeated_pair, "observed_count")?, 9);
    assert_eq!(usize_field(repeated_pair, "scenario_count")?, 3);
    assert!(!bool_field(repeated_pair, "hardware_output_authorized")?);
    assert!(!bool_field(repeated_pair, "output_sendability_claim")?);

    let repeated_triad = repeated_motif_for_sequence(
        motifs,
        &["0x25/0x19/0x02", "0x25/0x19/0x03", "0x25/0x19/0x01"],
    )?;
    assert_eq!(usize_field(repeated_triad, "motif_len")?, 3);
    assert_eq!(usize_field(repeated_triad, "observed_count")?, 9);
    assert_eq!(usize_field(repeated_triad, "scenario_count")?, 3);
    assert!(!bool_field(repeated_triad, "hardware_output_authorized")?);
    assert!(!bool_field(repeated_triad, "output_sendability_claim")?);

    Ok(())
}

#[test]
fn passive_decode_candidate_samples_preserve_payload_shape_hints() -> TestResult {
    let review = protocol_evidence_review()?;
    let summary = value_at(
        &review,
        "/passive_tuple_registry_coverage/decode_candidate_payload_shape_summary",
    )?;

    assert_eq!(
        str_field(summary, "claim_scope")?,
        "no_output_passive_tuple_payload_shape_review"
    );
    assert_eq!(
        str_field(summary, "sample_scope")?,
        "highest_frequency_unknown_commanded_tuples"
    );
    assert_eq!(usize_field(summary, "tuple_count")?, 5);
    assert_eq!(usize_field(summary, "sample_count")?, 45);
    assert_eq!(usize_field(summary, "unique_payload_shape_count")?, 5);
    assert!(bool_field(summary, "all_samples_checksum_valid")?);
    assert!(bool_field(summary, "all_samples_unknown_commanded")?);
    assert!(bool_field(
        summary,
        "all_sample_payloads_empty_or_zero_filled"
    )?);
    assert!(!bool_field(summary, "hardware_output_authorized")?);
    assert!(!bool_field(summary, "native_control_evidence")?);
    assert!(!bool_field(summary, "output_sendability_claim")?);
    assert!(!bool_field(
        summary,
        "protocol_evidence_sufficient_for_output_plan"
    )?);

    let shapes = array_at(summary, "/tuple_payload_shapes")?;
    let empty_status = payload_shape_for_tuple(shapes, "0x5A/0x1B/0x00")?;
    assert_eq!(usize_field(empty_status, "sample_count")?, 9);
    assert_eq!(usize_field(empty_status, "payload_len_min")?, 0);
    assert_eq!(usize_field(empty_status, "payload_len_max")?, 0);
    assert_eq!(
        array_at(empty_status, "/unique_payload_hex_values")?
            .first()
            .and_then(Value::as_str),
        Some("")
    );
    assert_eq!(
        array_at(empty_status, "/payload_kinds")?
            .first()
            .and_then(Value::as_str),
        Some("empty")
    );
    assert!(!bool_field(empty_status, "hardware_output_authorized")?);
    assert!(!bool_field(empty_status, "output_sendability_claim")?);

    for tuple_id in [
        "0x5D/0x1B/0x01",
        "0x25/0x19/0x01",
        "0x25/0x19/0x02",
        "0x25/0x19/0x03",
    ] {
        let zero_status = payload_shape_for_tuple(shapes, tuple_id)?;
        assert_eq!(usize_field(zero_status, "sample_count")?, 9);
        assert_eq!(usize_field(zero_status, "payload_len_min")?, 2);
        assert_eq!(usize_field(zero_status, "payload_len_max")?, 2);
        assert_eq!(
            array_at(zero_status, "/unique_payload_hex_values")?
                .first()
                .and_then(Value::as_str),
            Some("0000")
        );
        assert_eq!(
            array_at(zero_status, "/payload_kinds")?
                .first()
                .and_then(Value::as_str),
            Some("zero_filled")
        );
        assert!(bool_field(
            zero_status,
            "all_sample_payloads_empty_or_zero_filled"
        )?);
        assert!(!bool_field(zero_status, "hardware_output_authorized")?);
        assert!(!bool_field(zero_status, "output_sendability_claim")?);
    }

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
    assert_eq!(usize_field(coverage, "decode_candidate_sample_count")?, 45);
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

    assert_eq!(decoded_sample_count, 45);
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
    assert_eq!(paired_1b_samples, 9);

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
    assert_eq!(ordered_19_triads, 9);

    Ok(())
}
