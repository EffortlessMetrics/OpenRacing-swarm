//! Snapshot tests for telemetry adapter normalization.
//!
//! These tests encode "known good" behavior for protocol parsing.
//! Run `cargo insta review` to accept new snapshots.

use openracing_telemetry_adapters::{ACCAdapter, CustomUdpSpec, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const FIXTURE_REALTIME_CAR_UPDATE_CAR_7: &[u8] =
    include_bytes!("../../service/tests/fixtures/acc/realtime_car_update_car_7.bin");
const FIXTURE_REGISTRATION_RESULT_SUCCESS: &[u8] =
    include_bytes!("../../service/tests/fixtures/acc/registration_result_success.bin");

/// Snapshot the normalized telemetry produced from a known realtime car update fixture.
#[test]
fn acc_realtime_car_update_normalized_snapshot() -> TestResult {
    let adapter = ACCAdapter::new();
    let normalized = adapter.normalize(FIXTURE_REALTIME_CAR_UPDATE_CAR_7)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

/// Registration packets are control messages and must not produce telemetry.
#[test]
fn acc_registration_result_is_not_telemetry() -> TestResult {
    let adapter = ACCAdapter::new();
    let result = adapter.normalize(FIXTURE_REGISTRATION_RESULT_SUCCESS);
    assert!(
        result.is_err(),
        "registration result should not parse as telemetry"
    );
    Ok(())
}

/// Snapshot the canonical channel names for the Codemasters mode-0 field spec.
#[test]
fn codemasters_mode0_field_names_snapshot() -> TestResult {
    let spec = CustomUdpSpec::from_mode(0);
    let names: Vec<&str> = spec.fields.iter().map(|f| f.channel.as_str()).collect();
    insta::assert_yaml_snapshot!(names);
    Ok(())
}

/// Snapshot the decoded values from a deterministic mode-0 packet.
#[test]
fn codemasters_mode0_decode_snapshot() -> TestResult {
    let spec = CustomUdpSpec::from_mode(0);

    let mut packet = Vec::new();
    packet.extend_from_slice(&45.0f32.to_le_bytes()); // speed
    packet.extend_from_slice(&8500.0f32.to_le_bytes()); // engine_rate
    packet.extend_from_slice(&5i32.to_le_bytes()); // gear
    packet.extend_from_slice(&0.3f32.to_le_bytes()); // steering_input
    packet.extend_from_slice(&0.8f32.to_le_bytes()); // throttle_input
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // brake_input
    packet.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input

    let decoded = spec.decode(&packet)?;
    let mut sorted_values: Vec<(String, f32)> = decoded.values.into_iter().collect();
    sorted_values.sort_by(|(a, _), (b, _)| a.cmp(b));
    insta::assert_yaml_snapshot!(sorted_values);
    Ok(())
}
