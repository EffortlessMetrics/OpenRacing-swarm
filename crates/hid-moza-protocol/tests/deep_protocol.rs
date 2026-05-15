//! Deep protocol tests for the Moza HID protocol crate.
//!
//! Covers:
//! 1. Direct torque encoding/decoding round-trip tests
//! 2. Handshake frame construction and state machine tests
//! 3. Report parsing tests for all known Moza report types
//! 4. Motor enable/disable bit patterns
//! 5. Protocol version handling (V1 vs V2)
//! 6. Property tests for torque value encoding

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    DEFAULT_MAX_RETRIES, DeviceWriter, FfbMode, MozaDirectTorqueEncoder, MozaInitState, MozaModel,
    MozaProtocol, MozaRetryPolicy, REPORT_LEN, TorqueEncoder, TorqueQ8_8, VendorProtocol,
    input_report, parse_wheelbase_input_report, parse_wheelbase_report, product_ids, report_ids,
    rim_ids,
};

// ── Mock writers ─────────────────────────────────────────────────────────────

struct RecordingWriter {
    feature_reports: Vec<Vec<u8>>,
}

impl RecordingWriter {
    fn new() -> Self {
        Self {
            feature_reports: Vec::new(),
        }
    }
}

impl DeviceWriter for RecordingWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        Ok(data.len())
    }
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }
}

struct AlwaysFailWriter;

impl DeviceWriter for AlwaysFailWriter {
    fn write_feature_report(&mut self, _: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Err("simulated write failure".into())
    }
    fn write_output_report(&mut self, _: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Err("simulated write failure".into())
    }
}

struct FailOnFeatureReportWriter {
    fail_report_id: u8,
    feature_reports: Vec<Vec<u8>>,
}

impl FailOnFeatureReportWriter {
    fn new(fail_report_id: u8) -> Self {
        Self {
            fail_report_id,
            feature_reports: Vec::new(),
        }
    }
}

impl DeviceWriter for FailOnFeatureReportWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        if data.first().copied() == Some(self.fail_report_id) {
            Err("simulated feature write failure".into())
        } else {
            Ok(data.len())
        }
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Decode raw i16 from encoded torque bytes back to Nm.
fn decode_torque_nm(out: &[u8; REPORT_LEN], max_torque_nm: f32) -> f32 {
    let raw = i16::from_le_bytes([out[1], out[2]]);
    if raw >= 0 {
        raw as f32 / i16::MAX as f32 * max_torque_nm
    } else {
        raw as f32 / (-(i16::MIN as f32)) * max_torque_nm
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. Direct torque encoding/decoding round-trip tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn round_trip_r5_half_scale() -> Result<(), Box<dyn std::error::Error>> {
    let max = MozaModel::R5.max_torque_nm();
    let torque_nm = max * 0.5;
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(torque_nm, 0, &mut out);

    let decoded = decode_torque_nm(&out, max);
    let tolerance = max / i16::MAX as f32 + 1e-4;
    assert!(
        (torque_nm - decoded).abs() <= tolerance,
        "R5 half-scale: encoded {torque_nm} decoded {decoded}"
    );
    Ok(())
}

#[test]
fn round_trip_r9_negative_quarter_scale() -> Result<(), Box<dyn std::error::Error>> {
    let max = MozaModel::R9.max_torque_nm();
    let torque_nm = -max * 0.25;
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(torque_nm, 0, &mut out);

    let decoded = decode_torque_nm(&out, max);
    let tolerance = max / i16::MAX as f32 + 1e-4;
    assert!(
        (torque_nm - decoded).abs() <= tolerance,
        "R9 -quarter: encoded {torque_nm} decoded {decoded}"
    );
    Ok(())
}

#[test]
fn round_trip_full_scale_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for model in [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ] {
        let max = model.max_torque_nm();
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];

        // Positive full scale → i16::MAX
        enc.encode(max, 0, &mut out);
        let raw_pos = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw_pos, i16::MAX, "{model:?} +full must saturate");

        // Negative full scale → i16::MIN
        enc.encode(-max, 0, &mut out);
        let raw_neg = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw_neg, i16::MIN, "{model:?} -full must saturate");

        // Both decode back within 0.01 Nm
        let dec_pos = decode_torque_nm(
            &{
                let mut o = [0u8; REPORT_LEN];
                enc.encode(max, 0, &mut o);
                o
            },
            max,
        );
        assert!(
            (dec_pos - max).abs() < 0.01,
            "{model:?} +full decode: {dec_pos}"
        );
    }
    Ok(())
}

#[test]
fn round_trip_zero_torque_all_models() -> Result<(), Box<dyn std::error::Error>> {
    for model in [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ] {
        let enc = MozaDirectTorqueEncoder::new(model.max_torque_nm());
        let mut out = [0u8; REPORT_LEN];
        enc.encode(0.0, 0, &mut out);

        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw, 0, "zero torque → raw=0 for {model:?}");
        assert_eq!(decode_torque_nm(&out, model.max_torque_nm()), 0.0);
    }
    Ok(())
}

#[test]
fn round_trip_q8_8_path() -> Result<(), Box<dyn std::error::Error>> {
    let max = MozaModel::R5.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    // 2.75 Nm in Q8.8 = 2.75 * 256 = 704
    let q8: TorqueQ8_8 = 704;
    let mut out = [0u8; REPORT_LEN];
    TorqueEncoder::encode(&enc, q8, 0, 0, &mut out);

    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert!(
        raw > 0,
        "Q8.8=704 (2.75Nm) must yield positive raw, got {raw}"
    );
    assert_eq!(out[0], report_ids::DIRECT_TORQUE);

    // Negative Q8.8
    let q8_neg: TorqueQ8_8 = -1152; // -4.5 Nm
    TorqueEncoder::encode(&enc, q8_neg, 0, 0, &mut out);
    let raw_neg = i16::from_le_bytes([out[1], out[2]]);
    assert!(
        raw_neg < 0,
        "Q8.8=-1152 (-4.5Nm) must yield negative raw, got {raw_neg}"
    );
    Ok(())
}

#[test]
fn q8_8_clamp_values_are_symmetric() -> Result<(), Box<dyn std::error::Error>> {
    for model in [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ] {
        let enc = MozaDirectTorqueEncoder::new(model.max_torque_nm());
        let cmax = TorqueEncoder::clamp_max(&enc);
        let cmin = TorqueEncoder::clamp_min(&enc);

        assert!(cmax > 0, "{model:?} clamp_max must be positive");
        assert!(cmin < 0, "{model:?} clamp_min must be negative");
        assert_eq!(cmin, -cmax, "{model:?} clamps must be symmetric");

        let expected = (model.max_torque_nm() * 256.0).round() as TorqueQ8_8;
        assert_eq!(cmax, expected, "{model:?} clamp_max = max_nm * 256");
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Handshake frame construction and state machine tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn handshake_initial_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert!(!protocol.is_ffb_ready());
    assert_eq!(protocol.retry_count(), 0);
    assert!(!protocol.can_retry());
    Ok(())
}

#[test]
fn handshake_success_transitions_to_ready() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Standard);
    let mut writer = RecordingWriter::new();

    protocol.initialize_device(&mut writer)?;

    assert_eq!(protocol.init_state(), MozaInitState::Ready);
    assert!(protocol.is_ffb_ready());
    assert_eq!(protocol.retry_count(), 0);
    Ok(())
}

#[test]
fn handshake_standard_mode_sends_two_reports() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R9_V2, FfbMode::Standard, false);
    let mut writer = RecordingWriter::new();

    protocol.initialize_device(&mut writer)?;

    // Without high torque: start_reports + ffb_mode
    assert_eq!(writer.feature_reports.len(), 2);
    assert_eq!(writer.feature_reports[0][0], report_ids::START_REPORTS);
    assert_eq!(writer.feature_reports[1][0], report_ids::FFB_MODE);
    assert_eq!(writer.feature_reports[1][1], FfbMode::Standard as u8);
    Ok(())
}

#[test]
fn handshake_r5_v1_standard_mode_skips_start_input_feature_report()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R5_V1, FfbMode::Standard, false);
    let mut writer = RecordingWriter::new();

    protocol.initialize_device(&mut writer)?;

    assert_eq!(writer.feature_reports.len(), 1);
    assert_eq!(writer.feature_reports[0][0], report_ids::FFB_MODE);
    assert_eq!(writer.feature_reports[0][1], FfbMode::Standard as u8);
    Ok(())
}

#[test]
fn handshake_direct_mode_with_high_torque_sends_three_reports()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R12_V1, FfbMode::Direct, true);
    let mut writer = RecordingWriter::new();

    protocol.initialize_device(&mut writer)?;

    assert_eq!(writer.feature_reports.len(), 3);
    // high_torque → start_reports → ffb_mode
    assert_eq!(writer.feature_reports[0][0], report_ids::HIGH_TORQUE);
    assert_eq!(writer.feature_reports[1][0], report_ids::START_REPORTS);
    assert_eq!(writer.feature_reports[2][0], report_ids::FFB_MODE);
    assert_eq!(writer.feature_reports[2][1], FfbMode::Direct as u8);
    Ok(())
}

#[test]
fn handshake_failure_transitions_to_failed() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R5_V2, FfbMode::Standard);
    let mut writer = AlwaysFailWriter;

    assert!(protocol.initialize_device(&mut writer).is_err());

    assert_eq!(protocol.init_state(), MozaInitState::Failed);
    assert!(!protocol.is_ffb_ready());
    assert_eq!(protocol.retry_count(), 1);
    assert!(protocol.can_retry());
    Ok(())
}

#[test]
fn handshake_exhausts_retries_to_permanent_failure() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R9_V1, FfbMode::Standard);
    let mut writer = AlwaysFailWriter;

    for attempt in 1..=DEFAULT_MAX_RETRIES {
        assert!(protocol.initialize_device(&mut writer).is_err());

        if attempt < DEFAULT_MAX_RETRIES {
            assert_eq!(
                protocol.init_state(),
                MozaInitState::Failed,
                "attempt {attempt}"
            );
            assert!(
                protocol.can_retry(),
                "attempt {attempt}: should be retryable"
            );
        } else {
            assert_eq!(
                protocol.init_state(),
                MozaInitState::PermanentFailure,
                "attempt {attempt}: expected PermanentFailure"
            );
            assert!(!protocol.can_retry());
        }
    }
    assert_eq!(protocol.retry_count(), DEFAULT_MAX_RETRIES);
    Ok(())
}

#[test]
fn handshake_permanent_failure_blocks_further_init() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Standard);
    let mut fail_writer = AlwaysFailWriter;

    for _ in 0..DEFAULT_MAX_RETRIES {
        assert!(protocol.initialize_device(&mut fail_writer).is_err());
    }
    assert_eq!(protocol.init_state(), MozaInitState::PermanentFailure);

    // Working writer after permanent failure → no-op
    let mut good_writer = RecordingWriter::new();
    protocol.initialize_device(&mut good_writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::PermanentFailure);
    assert!(
        good_writer.feature_reports.is_empty(),
        "no reports after permanent failure"
    );
    Ok(())
}

#[test]
fn handshake_reset_clears_state_and_allows_reinit() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R9_V2, FfbMode::Standard);
    let mut fail_writer = AlwaysFailWriter;

    assert!(protocol.initialize_device(&mut fail_writer).is_err());
    assert_eq!(protocol.init_state(), MozaInitState::Failed);
    assert_eq!(protocol.retry_count(), 1);

    protocol.reset_to_uninitialized();
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert_eq!(protocol.retry_count(), 0);

    let mut good_writer = RecordingWriter::new();
    protocol.initialize_device(&mut good_writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);
    Ok(())
}

#[test]
fn handshake_start_failure_does_not_set_ffb_mode() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R9_V2, FfbMode::Standard, false);
    let mut writer = FailOnFeatureReportWriter::new(report_ids::START_REPORTS);

    let result = protocol.initialize_device(&mut writer);

    assert!(result.is_err());
    assert_eq!(protocol.init_state(), MozaInitState::Failed);
    assert_eq!(writer.feature_reports.len(), 1);
    assert_eq!(writer.feature_reports[0][0], report_ids::START_REPORTS);
    assert!(
        !writer
            .feature_reports
            .iter()
            .any(|report| report.first().copied() == Some(report_ids::FFB_MODE)),
        "FFB mode report must not be sent after start-input failure"
    );
    Ok(())
}

#[test]
fn handshake_ready_blocks_reinitialization() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R5_V2, FfbMode::Standard);
    let mut writer = RecordingWriter::new();

    protocol.initialize_device(&mut writer)?;
    let first_count = writer.feature_reports.len();

    protocol.initialize_device(&mut writer)?;
    assert_eq!(
        writer.feature_reports.len(),
        first_count,
        "re-init in Ready state must be a no-op"
    );
    Ok(())
}

#[test]
fn handshake_non_wheelbase_skips_init() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::SR_P_PEDALS,
        product_ids::HBP_HANDBRAKE,
        product_ids::HGP_SHIFTER,
    ] {
        let protocol = MozaProtocol::new(pid);
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        assert!(
            writer.feature_reports.is_empty(),
            "peripheral 0x{pid:04X} must not send feature reports"
        );
    }
    Ok(())
}

#[test]
fn retry_policy_default_values() -> Result<(), Box<dyn std::error::Error>> {
    let policy = MozaRetryPolicy::default();
    assert_eq!(policy.max_retries, DEFAULT_MAX_RETRIES);
    assert_eq!(policy.base_delay_ms, 500);
    Ok(())
}

#[test]
fn retry_policy_exponential_backoff() -> Result<(), Box<dyn std::error::Error>> {
    let policy = MozaRetryPolicy {
        max_retries: 10,
        base_delay_ms: 100,
    };
    assert_eq!(policy.delay_ms_for(0), 100);
    assert_eq!(policy.delay_ms_for(1), 200);
    assert_eq!(policy.delay_ms_for(2), 400);
    assert_eq!(policy.delay_ms_for(3), 800);
    // Capped at shift=3
    assert_eq!(policy.delay_ms_for(4), 800);
    assert_eq!(policy.delay_ms_for(255), 800);
    Ok(())
}

#[test]
fn retry_policy_delay_saturates() -> Result<(), Box<dyn std::error::Error>> {
    let policy = MozaRetryPolicy {
        max_retries: 10,
        base_delay_ms: u32::MAX / 2,
    };
    assert_eq!(policy.delay_ms_for(3), u32::MAX);
    Ok(())
}

#[test]
fn init_state_to_u8_all_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let states = [
        MozaInitState::Uninitialized,
        MozaInitState::Initializing,
        MozaInitState::Ready,
        MozaInitState::Failed,
        MozaInitState::PermanentFailure,
    ];
    let values: Vec<u8> = states.iter().map(|s| s.to_u8()).collect();
    for i in 0..values.len() {
        for j in (i + 1)..values.len() {
            assert_ne!(
                values[i], values[j],
                "{:?} and {:?} share u8 value {}",
                states[i], states[j], values[i]
            );
        }
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Report parsing tests for all known Moza report types
// ═════════════════════════════════════════════════════════════════════════════

// ── Report ID constants (golden values) ──────────────────────────────────────

#[test]
fn report_ids_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::DEVICE_INFO, 0x01);
    assert_eq!(report_ids::HIGH_TORQUE, 0x02);
    assert_eq!(report_ids::START_REPORTS, 0x03);
    assert_eq!(report_ids::ROTATION_RANGE, 0x10);
    assert_eq!(report_ids::FFB_MODE, 0x11);
    assert_eq!(report_ids::DIRECT_TORQUE, 0x20);
    assert_eq!(report_ids::DEVICE_GAIN, 0x21);
    Ok(())
}

#[test]
fn ffb_mode_discriminant_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FfbMode::Off as u8, 0xFF);
    assert_eq!(FfbMode::Standard as u8, 0x00);
    assert_eq!(FfbMode::Direct as u8, 0x02);
    Ok(())
}

// ── Feature report byte construction ─────────────────────────────────────────

#[test]
fn high_torque_report_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R9_V2, FfbMode::Direct, true);
    let mut writer = RecordingWriter::new();
    protocol.enable_high_torque(&mut writer)?;
    assert_eq!(writer.feature_reports[0], [0x02, 0x00, 0x00, 0x00]);
    Ok(())
}

#[test]
fn start_reports_command_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let mut writer = RecordingWriter::new();
    protocol.start_input_reports(&mut writer)?;
    assert_eq!(writer.feature_reports[0], [0x03, 0x00, 0x00, 0x00]);
    Ok(())
}

#[test]
fn ffb_mode_report_bytes_all_modes() -> Result<(), Box<dyn std::error::Error>> {
    for (mode, expected_byte) in [
        (FfbMode::Standard, 0x00u8),
        (FfbMode::Direct, 0x02u8),
        (FfbMode::Off, 0xFFu8),
    ] {
        let protocol = MozaProtocol::new(product_ids::R9_V2);
        let mut writer = RecordingWriter::new();
        protocol.set_ffb_mode(&mut writer, mode)?;
        assert_eq!(
            writer.feature_reports[0],
            [report_ids::FFB_MODE, expected_byte, 0x00, 0x00],
            "FFB mode {mode:?}"
        );
    }
    Ok(())
}

#[test]
fn rotation_range_report_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R16_R21_V2);
    let mut writer = RecordingWriter::new();
    protocol.set_rotation_range(&mut writer, 900)?;
    let range_bytes = 900u16.to_le_bytes();
    assert_eq!(
        writer.feature_reports[0],
        [
            report_ids::ROTATION_RANGE,
            0x01,
            range_bytes[0],
            range_bytes[1]
        ]
    );
    Ok(())
}

#[test]
fn rotation_range_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V2);
    for degrees in [0u16, 270, 360, 540, 900, 1080, u16::MAX] {
        let mut writer = RecordingWriter::new();
        protocol.set_rotation_range(&mut writer, degrees)?;
        let le = degrees.to_le_bytes();
        assert_eq!(writer.feature_reports[0][2], le[0], "degrees={degrees}");
        assert_eq!(writer.feature_reports[0][3], le[1], "degrees={degrees}");
    }
    Ok(())
}

#[test]
fn send_feature_report_oversized_payload_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let mut writer = RecordingWriter::new();
    let big_data = [0u8; 64]; // 64 payload + 1 report_id = 65 > 64 limit
    let result = protocol.send_feature_report(&mut writer, 0x01, &big_data);
    assert!(result.is_err(), "oversized payload must be rejected");
    Ok(())
}

#[test]
fn send_feature_report_max_valid_size() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let mut writer = RecordingWriter::new();
    let data = [0xABu8; 63]; // 63 payload + 1 report_id = 64 (exactly at limit)
    protocol.send_feature_report(&mut writer, 0x42, &data)?;
    assert_eq!(writer.feature_reports[0][0], 0x42);
    assert_eq!(&writer.feature_reports[0][1..], &data[..]);
    Ok(())
}

// ── Wheelbase input report parsing ───────────────────────────────────────────

#[test]
fn parse_wheelbase_report_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
    let report = [0x02u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert!(parse_wheelbase_report(&report).is_none());
    Ok(())
}

#[test]
fn parse_wheelbase_full_input_with_all_axes() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..][..2].copy_from_slice(&0xBEEFu16.to_le_bytes());
    report[input_report::THROTTLE_START..][..2].copy_from_slice(&0xCAFEu16.to_le_bytes());
    report[input_report::BRAKE_START..][..2].copy_from_slice(&0xDEADu16.to_le_bytes());
    report[input_report::CLUTCH_START..][..2].copy_from_slice(&0xFACEu16.to_le_bytes());
    report[input_report::HANDBRAKE_START..][..2].copy_from_slice(&0x1234u16.to_le_bytes());
    report[input_report::HAT_START] = 0x04;
    report[input_report::FUNKY_START] = rim_ids::ES;
    report[input_report::ROTARY_START] = 0x19;
    report[input_report::ROTARY_START + 1] = 0x64;

    let parsed =
        parse_wheelbase_input_report(&report).ok_or("expected full wheelbase input parse")?;

    assert_eq!(parsed.steering, 0xBEEF);
    assert_eq!(parsed.pedals.throttle, 0xCAFE);
    assert_eq!(parsed.pedals.brake, 0xDEAD);
    assert_eq!(parsed.pedals.clutch, Some(0xFACE));
    assert_eq!(parsed.pedals.handbrake, Some(0x1234));
    assert_eq!(parsed.hat, 0x04);
    assert_eq!(parsed.funky, rim_ids::ES);
    assert_eq!(parsed.rotary, [0x19, 0x64]);
    Ok(())
}

#[test]
fn parse_input_state_wheelbase_with_es_rim() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..][..2].copy_from_slice(&0x8000u16.to_le_bytes());
    report[input_report::THROTTLE_START..][..2].copy_from_slice(&0x4000u16.to_le_bytes());
    report[input_report::BRAKE_START..][..2].copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::FUNKY_START] = rim_ids::ES;
    report[input_report::ROTARY_START] = 0x55;
    report[input_report::ROTARY_START + 1] = 0xAA;

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected wheelbase parse")?;

    assert_eq!(state.steering_u16, 0x8000);
    assert_eq!(state.throttle_u16, 0x4000);
    assert_eq!(state.brake_u16, 0x2000);
    assert_eq!(state.funky, rim_ids::ES);
    assert_eq!(state.ks_snapshot.encoders[0], 0x55);
    assert_eq!(state.ks_snapshot.encoders[1], i16::from(0xAAu8));
    Ok(())
}

#[test]
fn parse_pedal_axes_aggregated_from_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V2);
    let mut report = [0u8; input_report::HANDBRAKE_START + 2];
    report[0] = input_report::REPORT_ID;
    report[input_report::THROTTLE_START..][..2].copy_from_slice(&0x1234u16.to_le_bytes());
    report[input_report::BRAKE_START..][..2].copy_from_slice(&0x5678u16.to_le_bytes());
    report[input_report::CLUTCH_START..][..2].copy_from_slice(&0x9ABCu16.to_le_bytes());
    report[input_report::HANDBRAKE_START..][..2].copy_from_slice(&0xCDEFu16.to_le_bytes());

    let axes = protocol
        .parse_aggregated_pedal_axes(&report)
        .ok_or("expected pedal axes parse")?;

    assert_eq!(axes.throttle, 0x1234);
    assert_eq!(axes.brake, 0x5678);
    assert_eq!(axes.clutch, Some(0x9ABC));
    assert_eq!(axes.handbrake, Some(0xCDEF));
    Ok(())
}

#[test]
fn parse_input_state_standalone_srp_pedals() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::SR_P_PEDALS);
    let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected SR-P parse")?;
    assert_eq!(state.throttle_u16, 0xFFFF);
    assert_eq!(state.brake_u16, 0x8000);
    assert_eq!(state.steering_u16, 0);
    Ok(())
}

#[test]
fn parse_input_state_standalone_hbp_handbrake() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);
    let report = [0x01u8, 0xAB, 0xCD, 0x01];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected HBP parse")?;
    assert_eq!(state.handbrake_u16, 0xCDAB);
    assert_eq!(state.buttons[0], 0x01);
    Ok(())
}

#[test]
fn parse_input_state_rejects_empty_report() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    assert!(protocol.parse_input_state(&[]).is_none());
    Ok(())
}

#[test]
fn parse_input_state_rejects_short_wheelbase_report() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let report = [input_report::REPORT_ID, 0x00, 0x80];
    assert!(protocol.parse_input_state(&report).is_none());
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Motor enable/disable bit patterns
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn motor_enable_bit_set_for_nonzero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(10.0);
    let mut out = [0u8; REPORT_LEN];
    for torque_nm in [0.1_f32, 1.0, 5.0, 10.0, -0.1, -1.0, -5.0, -10.0] {
        enc.encode(torque_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_ne!(raw, 0, "torque {torque_nm} must yield non-zero raw");
        assert_eq!(
            out[3] & 0x01,
            0x01,
            "motor-enable bit must be set for torque={torque_nm}"
        );
    }
    Ok(())
}

#[test]
fn motor_enable_bit_clear_for_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(10.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.0, 0, &mut out);
    assert_eq!(out[3] & 0x01, 0x00);

    // Also via encode_zero
    let mut out2 = [0xFFu8; REPORT_LEN];
    enc.encode_zero(&mut out2);
    assert_eq!(out2[3] & 0x01, 0x00);

    // Also via TorqueEncoder::encode_zero
    let mut out3 = [0xFFu8; REPORT_LEN];
    TorqueEncoder::encode_zero(&enc, &mut out3);
    assert_eq!(out3[3] & 0x01, 0x00);
    Ok(())
}

#[test]
fn slew_rate_flag_and_payload_independent_of_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0).with_slew_rate(750);
    for torque in [0.0_f32, 4.5, -4.5, 9.0, -9.0] {
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        assert_eq!(
            out[3] & 0x02,
            0x02,
            "slew flag must be set for torque={torque}"
        );
        assert_eq!(
            u16::from_le_bytes([out[4], out[5]]),
            750,
            "slew payload must be 750 for torque={torque}"
        );
    }
    Ok(())
}

#[test]
fn combined_flags_motor_enable_and_slew_rate() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5).with_slew_rate(300);
    let mut out = [0u8; REPORT_LEN];

    // Non-zero torque + slew: flags = 0x03 (motor=0x01 | slew=0x02)
    enc.encode(2.75, 0, &mut out);
    assert_eq!(out[3] & 0x03, 0x03, "both motor and slew bits set");

    // Zero torque + slew: flags = 0x02 (slew only, motor disabled)
    enc.encode(0.0, 0, &mut out);
    assert_eq!(out[3] & 0x01, 0x00, "motor disabled for zero torque");
    assert_eq!(out[3] & 0x02, 0x02, "slew flag still set for zero torque");
    Ok(())
}

#[test]
fn torque_encoder_flags_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    // Q8.8 = 256 → 1.0 Nm (non-zero → motor enable bit set)
    TorqueEncoder::encode(&enc, 256, 0, 0xF0, &mut out);

    // Extra flags 0xF0 should be OR'd with motor enable 0x01
    assert_eq!(out[3] & 0xF0, 0xF0, "extra flags preserved");
    assert_eq!(out[3] & 0x01, 0x01, "motor enable also set");
    assert_eq!(out[3], 0xF1);
    Ok(())
}

#[test]
fn encode_zero_clears_entire_output() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0xFFu8; REPORT_LEN];
    enc.encode_zero(&mut out);

    assert_eq!(out[0], report_ids::DIRECT_TORQUE);
    assert_eq!(&out[1..], &[0u8; REPORT_LEN - 1]);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. Protocol version handling (V1 vs V2)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn v1_pids_detected_as_v1() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
    ] {
        let protocol = MozaProtocol::new(pid);
        assert!(!protocol.is_v2_hardware(), "PID 0x{pid:04X} must be V1");
    }
    Ok(())
}

#[test]
fn v2_pids_detected_as_v2() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
        product_ids::R16_R21_V2,
    ] {
        let protocol = MozaProtocol::new(pid);
        assert!(protocol.is_v2_hardware(), "PID 0x{pid:04X} must be V2");
    }
    Ok(())
}

#[test]
fn v1_encoder_cpr_is_15_bit() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::R5_V1, product_ids::R9_V1, product_ids::R12_V1] {
        let protocol = MozaProtocol::new(pid);
        let config = protocol.get_ffb_config();
        assert_eq!(config.encoder_cpr, 32768, "V1 PID 0x{pid:04X}: 15-bit CPR");
    }
    Ok(())
}

#[test]
fn v2_standard_encoder_cpr_is_18_bit() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
    ] {
        let protocol = MozaProtocol::new(pid);
        let config = protocol.get_ffb_config();
        assert_eq!(config.encoder_cpr, 262144, "V2 PID 0x{pid:04X}: 18-bit CPR");
    }
    Ok(())
}

#[test]
fn v2_r16_r21_encoder_cpr_is_21_bit() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R16_R21_V2);
    let config = protocol.get_ffb_config();
    assert_eq!(config.encoder_cpr, 2097152, "R16/R21 V2: 21-bit CPR");
    Ok(())
}

#[test]
fn ffb_config_max_torque_matches_model() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, f32)] = &[
        (product_ids::R3_V1, 3.9),
        (product_ids::R5_V2, 5.5),
        (product_ids::R9_V1, 9.0),
        (product_ids::R12_V2, 12.0),
        (product_ids::R16_R21_V1, 16.0),
    ];
    for &(pid, expected_nm) in cases {
        let protocol = MozaProtocol::new(pid);
        let config = protocol.get_ffb_config();
        assert!(
            (config.max_torque_nm - expected_nm).abs() < 0.01,
            "PID 0x{pid:04X}: max_torque={} expected {expected_nm}",
            config.max_torque_nm
        );
    }
    Ok(())
}

#[test]
fn ffb_config_common_fields() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::R5_V1, product_ids::R9_V2, product_ids::R12_V1] {
        let protocol = MozaProtocol::new(pid);
        let config = protocol.get_ffb_config();
        assert!(
            config.fix_conditional_direction,
            "Moza requires fix_conditional_direction"
        );
        assert!(config.uses_vendor_usage_page, "Moza uses vendor usage page");
        assert_eq!(config.required_b_interval, Some(1), "1ms interval for 1kHz");
    }
    Ok(())
}

#[test]
fn output_report_id_for_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::R5_V1, product_ids::R9_V2, product_ids::R12_V1] {
        let protocol = MozaProtocol::new(pid);
        assert_eq!(
            protocol.output_report_id(),
            Some(report_ids::DIRECT_TORQUE),
            "PID 0x{pid:04X}"
        );
        assert_eq!(
            protocol.output_report_len(),
            Some(REPORT_LEN),
            "PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn output_report_id_none_for_peripherals() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::SR_P_PEDALS,
        product_ids::HBP_HANDBRAKE,
        product_ids::HGP_SHIFTER,
    ] {
        let protocol = MozaProtocol::new(pid);
        assert_eq!(protocol.output_report_id(), None, "PID 0x{pid:04X}");
        assert_eq!(protocol.output_report_len(), None, "PID 0x{pid:04X}");
    }
    Ok(())
}

#[test]
fn protocol_model_and_product_id() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, MozaModel)] = &[
        (product_ids::R3_V1, MozaModel::R3),
        (product_ids::R5_V2, MozaModel::R5),
        (product_ids::R9_V1, MozaModel::R9),
        (product_ids::R12_V2, MozaModel::R12),
        (product_ids::R16_R21_V1, MozaModel::R16),
    ];
    for &(pid, expected_model) in cases {
        let protocol = MozaProtocol::new(pid);
        assert_eq!(protocol.product_id(), pid);
        assert_eq!(protocol.model(), expected_model);
    }
    Ok(())
}

#[test]
fn protocol_ffb_mode_reflects_construction() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [FfbMode::Off, FfbMode::Standard, FfbMode::Direct] {
        let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R9_V2, mode);
        assert_eq!(protocol.ffb_mode(), mode);
    }
    Ok(())
}

#[test]
fn protocol_high_torque_reflects_construction() -> Result<(), Box<dyn std::error::Error>> {
    let ht_on = MozaProtocol::new_with_config(product_ids::R5_V1, FfbMode::Direct, true);
    assert!(ht_on.is_high_torque_enabled());

    let ht_off = MozaProtocol::new_with_config(product_ids::R5_V1, FfbMode::Direct, false);
    assert!(!ht_off.is_high_torque_enabled());
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. Property tests for torque value encoding
// ═════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Encoding is idempotent: encoding the same value twice yields identical bytes.
    #[test]
    fn prop_encoding_idempotent(
        max in 0.1_f32..=21.0_f32,
        torque in -21.0_f32..=21.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out1 = [0u8; REPORT_LEN];
        let mut out2 = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out1);
        enc.encode(torque, 0, &mut out2);
        prop_assert_eq!(out1, out2, "encoding must be deterministic for torque={}", torque);
    }

    /// Slew-rate bytes 4-5 are independent of the torque value.
    #[test]
    fn prop_slew_rate_independent_of_torque(
        max in 0.1_f32..=21.0_f32,
        torque_a in -21.0_f32..=21.0_f32,
        torque_b in -21.0_f32..=21.0_f32,
        slew_rate: u16,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max).with_slew_rate(slew_rate);
        let mut out_a = [0u8; REPORT_LEN];
        let mut out_b = [0u8; REPORT_LEN];
        enc.encode(torque_a, 0, &mut out_a);
        enc.encode(torque_b, 0, &mut out_b);
        prop_assert_eq!(
            &out_a[4..6], &out_b[4..6],
            "slew bytes must be independent of torque"
        );
    }

    /// Q8.8 encode round-trip: the decoded Nm value has the same sign as the input Q8.8.
    #[test]
    fn prop_q8_8_sign_preserved(
        max in 0.5_f32..=21.0_f32,
        torque: i16,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        TorqueEncoder::encode(&enc, torque, 0, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        // Small Q8.8 values near zero may quantize to 0
        if torque > 10 {
            prop_assert!(raw >= 0, "positive Q8.8={torque} must yield non-negative raw={raw}");
        } else if torque < -10 {
            prop_assert!(raw <= 0, "negative Q8.8={torque} must yield non-positive raw={raw}");
        }
    }

    /// Q8.8 encode: clamp_min/clamp_max values produce full-scale raw output.
    #[test]
    fn prop_q8_8_clamp_bounds_produce_full_scale(max in 0.5_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];

        let cmax = TorqueEncoder::clamp_max(&enc);
        TorqueEncoder::encode(&enc, cmax, 0, 0, &mut out);
        let raw_pos = i16::from_le_bytes([out[1], out[2]]);
        // At full scale, raw should be near i16::MAX (within quantization tolerance)
        prop_assert!(
            raw_pos > i16::MAX / 2,
            "clamp_max={cmax} must produce near-full-scale raw={raw_pos}"
        );

        let cmin = TorqueEncoder::clamp_min(&enc);
        TorqueEncoder::encode(&enc, cmin, 0, 0, &mut out);
        let raw_neg = i16::from_le_bytes([out[1], out[2]]);
        prop_assert!(
            raw_neg < i16::MIN / 2,
            "clamp_min={cmin} must produce near-negative-full-scale raw={raw_neg}"
        );
    }

    /// Encoding round-trip: encode Nm → extract raw → decode back → compare.
    #[test]
    fn prop_nm_round_trip_within_tolerance(
        max in 0.1_f32..=21.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max * frac;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque_nm, 0, &mut out);

        let raw = i16::from_le_bytes([out[1], out[2]]);
        let decoded = if raw >= 0 {
            raw as f32 / i16::MAX as f32 * max
        } else {
            raw as f32 / (-(i16::MIN as f32)) * max
        };
        let tolerance = max / i16::MAX as f32 + 1e-4;
        prop_assert!(
            (torque_nm - decoded).abs() <= tolerance,
            "round-trip: {torque_nm} → raw={raw} → {decoded} (tolerance={tolerance})"
        );
    }

    /// Retry policy delay is always at least base_delay_ms.
    #[test]
    fn prop_retry_delay_at_least_base(
        base_ms in 1u32..=10000u32,
        attempt in 0u8..=10u8,
    ) {
        let policy = MozaRetryPolicy { max_retries: 20, base_delay_ms: base_ms };
        let delay = policy.delay_ms_for(attempt);
        prop_assert!(delay >= base_ms, "delay {delay} < base {base_ms} for attempt {attempt}");
    }
}
