//! Deep tests for the Moza HID protocol crate — NEW coverage areas.
//!
//! Focuses on areas NOT already covered by existing proptests and unit tests:
//!
//! 1. VendorProtocol `shutdown_device` default no-op behaviour
//! 2. FfbConfig default values and per-model invariants
//! 3. Multi-device concurrent protocol instances
//! 4. MozaEsCompatibility diagnostic_message exhaustive coverage
//! 5. MozaInputState both_clutches_pressed threshold boundary cases
//! 6. DeviceSignature construction with optional fields
//! 7. Known-good Moza HID report golden byte sequences (per model)
//! 8. Proptest: FfbConfig invariants across all device PIDs
//! 9. Proptest: encoder CPR V1-vs-V2 invariants
//! 10. Proptest: MozaHatDirection round-trip for valid range
//! 11. Proptest: MozaEsJoystickMode from_config_value totality
//! 12. Proptest: MozaPedalAxesRaw normalize preserves None-ness
//! 13. Proptest: multi-encoder instances are independent

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::writer::{DeviceWriter, FfbConfig};
use racing_wheel_hid_moza_protocol::{
    DeviceSignature, FfbMode, MOZA_VENDOR_ID, MozaDeviceCategory, MozaDirectTorqueEncoder,
    MozaEsCompatibility, MozaEsJoystickMode, MozaHatDirection, MozaInitState, MozaInputState,
    MozaModel, MozaPedalAxesRaw, MozaProtocol, REPORT_LEN, SignatureVerdict, VendorProtocol,
    es_compatibility, identify_device, input_report, is_wheelbase_product, product_ids, report_ids,
    rim_ids, verify_signature,
};

// ── Mock writers ─────────────────────────────────────────────────────────────

struct RecordingWriter {
    feature_reports: Vec<Vec<u8>>,
    output_reports: Vec<Vec<u8>>,
}

impl RecordingWriter {
    fn new() -> Self {
        Self {
            feature_reports: Vec::new(),
            output_reports: Vec::new(),
        }
    }
}

impl DeviceWriter for RecordingWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        Ok(data.len())
    }
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.push(data.to_vec());
        Ok(data.len())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. VendorProtocol::shutdown_device — default no-op
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn shutdown_device_is_noop_for_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V2,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
    ];
    for pid in pids {
        let protocol = MozaProtocol::new(pid);
        let mut writer = RecordingWriter::new();
        protocol.shutdown_device(&mut writer)?;
        assert!(
            writer.feature_reports.is_empty(),
            "shutdown_device must be no-op for PID 0x{pid:04X}"
        );
        assert!(
            writer.output_reports.is_empty(),
            "shutdown_device must not send output reports for PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn shutdown_device_is_noop_for_peripherals() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::SR_P_PEDALS,
        product_ids::HBP_HANDBRAKE,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
    ] {
        let protocol = MozaProtocol::new(pid);
        let mut writer = RecordingWriter::new();
        protocol.shutdown_device(&mut writer)?;
        assert!(
            writer.feature_reports.is_empty(),
            "peripheral 0x{pid:04X} shutdown must be no-op"
        );
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. FfbConfig default values
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn ffb_config_default_has_sane_values() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = FfbConfig::default();
    assert!(
        !cfg.fix_conditional_direction,
        "default fix_conditional_direction must be false"
    );
    assert!(
        !cfg.uses_vendor_usage_page,
        "default uses_vendor_usage_page must be false"
    );
    assert_eq!(
        cfg.required_b_interval, None,
        "default b_interval must be None"
    );
    assert!(
        (cfg.max_torque_nm - 10.0).abs() < 0.01,
        "default max_torque must be 10.0 Nm"
    );
    assert_eq!(cfg.encoder_cpr, 4096, "default encoder_cpr must be 4096");
    Ok(())
}

#[test]
fn ffb_config_moza_overrides_all_defaults() -> Result<(), Box<dyn std::error::Error>> {
    // Moza wheelbases must override every default to Moza-specific values.
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let cfg = protocol.get_ffb_config();
    let default_cfg = FfbConfig::default();
    assert_ne!(
        cfg.fix_conditional_direction, default_cfg.fix_conditional_direction,
        "Moza must override fix_conditional_direction"
    );
    assert_ne!(
        cfg.uses_vendor_usage_page, default_cfg.uses_vendor_usage_page,
        "Moza must override uses_vendor_usage_page"
    );
    assert_ne!(
        cfg.required_b_interval, default_cfg.required_b_interval,
        "Moza must override required_b_interval"
    );
    assert_ne!(
        cfg.encoder_cpr, default_cfg.encoder_cpr,
        "Moza must override encoder_cpr"
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Multi-device concurrent protocol instances
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_protocol_instances_are_independent() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate wheelbase + pedals + handbrake existing simultaneously.
    let wb = MozaProtocol::new(product_ids::R9_V2);
    let pedals = MozaProtocol::new(product_ids::SR_P_PEDALS);
    let handbrake = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    assert_eq!(wb.model(), MozaModel::R9);
    assert_eq!(pedals.model(), MozaModel::SrpPedals);
    assert_eq!(handbrake.model(), MozaModel::Unknown);

    // Only the wheelbase should accept init.
    let mut wb_writer = RecordingWriter::new();
    let mut ped_writer = RecordingWriter::new();
    let mut hb_writer = RecordingWriter::new();

    wb.initialize_device(&mut wb_writer)?;
    pedals.initialize_device(&mut ped_writer)?;
    handbrake.initialize_device(&mut hb_writer)?;

    assert_eq!(wb.init_state(), MozaInitState::Ready);
    assert!(!wb_writer.feature_reports.is_empty());
    assert!(ped_writer.feature_reports.is_empty());
    assert!(hb_writer.feature_reports.is_empty());
    Ok(())
}

#[test]
fn multiple_wheelbases_init_independently() -> Result<(), Box<dyn std::error::Error>> {
    // Two wheelbases of different models should not interfere.
    let r5 = MozaProtocol::new(product_ids::R5_V1);
    let r12 = MozaProtocol::new(product_ids::R12_V2);

    let mut w5 = RecordingWriter::new();
    let mut w12 = RecordingWriter::new();

    r5.initialize_device(&mut w5)?;
    r12.initialize_device(&mut w12)?;

    assert_eq!(r5.init_state(), MozaInitState::Ready);
    assert_eq!(r12.init_state(), MozaInitState::Ready);

    // R5 config
    let cfg5 = r5.get_ffb_config();
    let cfg12 = r12.get_ffb_config();
    assert!(
        (cfg5.max_torque_nm - 5.5).abs() < 0.01,
        "R5 max torque should be 5.5"
    );
    assert!(
        (cfg12.max_torque_nm - 12.0).abs() < 0.01,
        "R12 max torque should be 12.0"
    );
    // V1 vs V2 CPR difference
    assert_ne!(cfg5.encoder_cpr, cfg12.encoder_cpr);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. MozaEsCompatibility diagnostic_message exhaustive coverage
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn es_compatibility_diagnostic_messages_complete() -> Result<(), Box<dyn std::error::Error>> {
    let supported = MozaEsCompatibility::Supported;
    assert!(supported.diagnostic_message().is_some());
    assert!(supported.is_supported());

    let unsupported = MozaEsCompatibility::UnsupportedHardwareRevision;
    let msg = unsupported.diagnostic_message().ok_or("expected message")?;
    assert!(msg.contains("R9 V1"), "message must mention R9 V1");
    assert!(!unsupported.is_supported());

    let unknown = MozaEsCompatibility::UnknownWheelbase;
    let msg = unknown.diagnostic_message().ok_or("expected message")?;
    assert!(
        msg.contains("capture-validated"),
        "message must mention validation"
    );
    assert!(!unknown.is_supported());

    let not_wb = MozaEsCompatibility::NotWheelbase;
    assert!(not_wb.diagnostic_message().is_none());
    assert!(!not_wb.is_supported());
    Ok(())
}

#[test]
fn es_compatibility_all_wheelbases_have_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let all_wb_pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    for pid in all_wb_pids {
        let compat = es_compatibility(pid);
        assert_ne!(
            compat,
            MozaEsCompatibility::NotWheelbase,
            "wheelbase PID 0x{pid:04X} must not be NotWheelbase"
        );
        // All wheelbase variants must produce a diagnostic message
        assert!(
            compat.diagnostic_message().is_some(),
            "wheelbase PID 0x{pid:04X} must have diagnostic message"
        );
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. MozaInputState both_clutches_pressed threshold boundary cases
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn both_clutches_pressed_exact_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = MozaInputState::empty(0);
    state.clutch_u16 = 30000;
    state.handbrake_u16 = 30000;
    // At exact threshold, both are >= threshold
    assert!(state.both_clutches_pressed(30000));
    // One below threshold
    assert!(!state.both_clutches_pressed(30001));
    Ok(())
}

#[test]
fn both_clutches_pressed_one_axis_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = MozaInputState::empty(0);
    state.clutch_u16 = u16::MAX;
    state.handbrake_u16 = 0;
    assert!(!state.both_clutches_pressed(1));
    Ok(())
}

#[test]
fn both_clutches_pressed_zero_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let state = MozaInputState::empty(0);
    // Both axes are 0, threshold is 0 → both are >= 0
    assert!(state.both_clutches_pressed(0));
    Ok(())
}

#[test]
fn both_clutches_pressed_max_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = MozaInputState::empty(0);
    state.clutch_u16 = u16::MAX;
    state.handbrake_u16 = u16::MAX;
    assert!(state.both_clutches_pressed(u16::MAX));
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. DeviceSignature construction variants
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn device_signature_from_vid_pid_leaves_optionals_none() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, product_ids::R5_V1);
    assert_eq!(sig.vendor_id, MOZA_VENDOR_ID);
    assert_eq!(sig.product_id, product_ids::R5_V1);
    assert_eq!(sig.interface_number, None);
    assert_eq!(sig.descriptor_len, None);
    assert_eq!(sig.descriptor_crc32, None);
    Ok(())
}

#[test]
fn device_signature_full_construction_does_not_affect_verdict()
-> Result<(), Box<dyn std::error::Error>> {
    let minimal = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, product_ids::R12_V2);
    let full = DeviceSignature {
        vendor_id: MOZA_VENDOR_ID,
        product_id: product_ids::R12_V2,
        interface_number: Some(2),
        descriptor_len: Some(1024),
        descriptor_crc32: Some(0xCAFEBABE),
    };
    assert_eq!(verify_signature(&minimal), verify_signature(&full));
    Ok(())
}

#[test]
fn device_signature_zero_vid_pid_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature::from_vid_pid(0x0000, 0x0000);
    assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. Known-good Moza HID report golden byte sequences (per model)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn golden_torque_r3_full_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R3.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(3.9, 0, &mut out);
    assert_eq!(out[0], 0x20);
    assert_eq!(i16::from_le_bytes([out[1], out[2]]), i16::MAX);
    assert_eq!(out[3] & 0x01, 0x01);
    assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    Ok(())
}

#[test]
fn golden_torque_r9_tenth_scale() -> Result<(), Box<dyn std::error::Error>> {
    let max = MozaModel::R9.max_torque_nm(); // 9.0
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.9, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    let expected = (i16::MAX as f32 * 0.1).round() as i16;
    assert!(
        (raw as i32 - expected as i32).abs() <= 1,
        "R9 tenth-scale: raw={raw} expected≈{expected}"
    );
    assert_eq!(out[0], report_ids::DIRECT_TORQUE);
    assert_eq!(out[3] & 0x01, 0x01);
    Ok(())
}

#[test]
fn golden_torque_r12_with_slew_rate_500() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(12.0).with_slew_rate(500);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-6.0, 0, &mut out);
    // Report ID
    assert_eq!(out[0], 0x20);
    // Torque: -6 Nm is half negative → ≈ i16::MIN / 2
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert!(raw < 0, "negative torque must yield negative raw");
    let expected = (i16::MIN as f32 * 0.5).round() as i16;
    assert!(
        (raw as i32 - expected as i32).abs() <= 1,
        "R12 half-neg: raw={raw} expected≈{expected}"
    );
    // Flags: motor(0x01) | slew(0x02) = 0x03
    assert_eq!(out[3] & 0x03, 0x03);
    // Slew rate: 500 LE
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 500);
    // Reserved
    assert_eq!(out[6], 0);
    assert_eq!(out[7], 0);
    Ok(())
}

#[test]
fn golden_torque_r21_full_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R21.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-21.0, 0, &mut out);
    assert_eq!(i16::from_le_bytes([out[1], out[2]]), i16::MIN);
    assert_eq!(out[3] & 0x01, 0x01);
    Ok(())
}

#[test]
fn golden_handshake_r16_direct_high_torque() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R16_R21_V2, FfbMode::Direct, true);
    let mut writer = RecordingWriter::new();
    protocol.initialize_device(&mut writer)?;

    // Expect 3 feature reports: HIGH_TORQUE, START_REPORTS, FFB_MODE
    assert_eq!(writer.feature_reports.len(), 3);
    assert_eq!(writer.feature_reports[0], [0x02, 0x00, 0x00, 0x00]);
    assert_eq!(writer.feature_reports[1], [0x03, 0x00, 0x00, 0x00]);
    assert_eq!(writer.feature_reports[2], [0x11, 0x02, 0x00, 0x00]);
    Ok(())
}

#[test]
fn golden_rotation_range_270_degrees() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V2);
    let mut writer = RecordingWriter::new();
    protocol.set_rotation_range(&mut writer, 270)?;
    let le = 270u16.to_le_bytes();
    assert_eq!(
        writer.feature_reports[0],
        [report_ids::ROTATION_RANGE, 0x01, le[0], le[1]]
    );
    Ok(())
}

#[test]
fn golden_rotation_range_1080_degrees() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V1);
    let mut writer = RecordingWriter::new();
    protocol.set_rotation_range(&mut writer, 1080)?;
    let le = 1080u16.to_le_bytes();
    assert_eq!(
        writer.feature_reports[0],
        [report_ids::ROTATION_RANGE, 0x01, le[0], le[1]]
    );
    Ok(())
}

// ── Known-good wheelbase input report parsing ────────────────────────────────

#[test]
fn golden_wheelbase_input_r5_full_axes() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;
    // Steering: 0x8000 (mid-range)
    report[input_report::STEERING_START] = 0x00;
    report[input_report::STEERING_START + 1] = 0x80;
    // Throttle: full
    report[input_report::THROTTLE_START] = 0xFF;
    report[input_report::THROTTLE_START + 1] = 0xFF;
    // Brake: zero
    report[input_report::BRAKE_START] = 0x00;
    report[input_report::BRAKE_START + 1] = 0x00;
    // Buttons byte 0 = 0xAA
    report[input_report::BUTTONS_START] = 0xAA;
    // Hat = Down (4)
    report[input_report::HAT_START] = 0x04;
    // Funky/rim = GS
    report[input_report::FUNKY_START] = rim_ids::GS_V2;
    // Rotary
    report[input_report::ROTARY_START] = 0x10;
    report[input_report::ROTARY_START + 1] = 0x20;

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected wheelbase parse")?;

    assert_eq!(state.steering_u16, 0x8000);
    assert_eq!(state.throttle_u16, 0xFFFF);
    assert_eq!(state.brake_u16, 0);
    assert_eq!(state.buttons[0], 0xAA);
    assert_eq!(state.hat, 0x04);
    assert_eq!(state.funky, rim_ids::GS_V2);
    assert_eq!(state.ks_snapshot.encoders[0], 0x10);
    assert_eq!(state.ks_snapshot.encoders[1], 0x20);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. Encoder CPR values for all known model+version combos
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn encoder_cpr_v1_all_models_15_bit() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
    ] {
        let cfg = MozaProtocol::new(pid).get_ffb_config();
        assert_eq!(
            cfg.encoder_cpr, 32768,
            "V1 PID 0x{pid:04X} must be 15-bit CPR (32768)"
        );
    }
    Ok(())
}

#[test]
fn encoder_cpr_v2_standard_models_18_bit() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
    ] {
        let cfg = MozaProtocol::new(pid).get_ffb_config();
        assert_eq!(
            cfg.encoder_cpr, 262144,
            "V2 PID 0x{pid:04X} must be 18-bit CPR (262144)"
        );
    }
    Ok(())
}

#[test]
fn encoder_cpr_v2_r16_r21_21_bit() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = MozaProtocol::new(product_ids::R16_R21_V2).get_ffb_config();
    assert_eq!(cfg.encoder_cpr, 2097152, "R16/R21 V2 must be 21-bit CPR");
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. Standalone parsing edge cases for multi-device setups
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn wheelbase_protocol_ignores_standalone_report_format() -> Result<(), Box<dyn std::error::Error>> {
    // A wheelbase protocol instance receiving a non-0x01 report ID should return None.
    let protocol = MozaProtocol::new(product_ids::R5_V2);
    let non_standard_report = [0x02u8, 0xFF, 0xFF, 0x00, 0x80];
    assert!(
        protocol.parse_input_state(&non_standard_report).is_none(),
        "wheelbase must reject non-0x01 report ID"
    );
    Ok(())
}

#[test]
fn shifter_protocol_has_no_parse_path() -> Result<(), Box<dyn std::error::Error>> {
    // Shifters don't have a parse_input_state path.
    let protocol = MozaProtocol::new(product_ids::HGP_SHIFTER);
    let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
    assert!(
        protocol.parse_input_state(&report).is_none(),
        "shifter has no standalone parse path"
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 10. Error recovery: reset after permanent failure allows re-init
// ═════════════════════════════════════════════════════════════════════════════

struct FailNTimesWriter {
    fail_count: usize,
    remaining_failures: usize,
}

impl FailNTimesWriter {
    fn new(n: usize) -> Self {
        Self {
            fail_count: 0,
            remaining_failures: n,
        }
    }
}

impl DeviceWriter for FailNTimesWriter {
    fn write_feature_report(&mut self, _: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.remaining_failures > 0 {
            self.remaining_failures -= 1;
            self.fail_count += 1;
            Err("transient failure".into())
        } else {
            Ok(4)
        }
    }
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }
}

#[test]
fn error_recovery_reset_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R9_V2, FfbMode::Standard);

    // Phase 1: fail once
    let mut failing_writer = FailNTimesWriter::new(100);
    assert!(protocol.initialize_device(&mut failing_writer).is_err());
    assert_eq!(protocol.init_state(), MozaInitState::Failed);
    assert_eq!(protocol.retry_count(), 1);

    // Phase 2: reset
    protocol.reset_to_uninitialized();
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert_eq!(protocol.retry_count(), 0);

    // Phase 3: succeed
    let mut good_writer = RecordingWriter::new();
    protocol.initialize_device(&mut good_writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);
    assert!(protocol.is_ffb_ready());
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 11. MozaPedalAxesRaw normalize with None fields
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pedal_normalize_preserves_none_clutch_and_handbrake() -> Result<(), Box<dyn std::error::Error>> {
    let raw = MozaPedalAxesRaw {
        throttle: 32768,
        brake: 16384,
        clutch: None,
        handbrake: None,
    };
    let norm = raw.normalize();
    assert!(norm.clutch.is_none(), "None clutch must remain None");
    assert!(norm.handbrake.is_none(), "None handbrake must remain None");
    // Throttle and brake still valid
    assert!(norm.throttle > 0.0 && norm.throttle < 1.0);
    assert!(norm.brake > 0.0 && norm.brake < 1.0);
    Ok(())
}

#[test]
fn pedal_normalize_partial_none_fields() -> Result<(), Box<dyn std::error::Error>> {
    let raw = MozaPedalAxesRaw {
        throttle: u16::MAX,
        brake: 0,
        clutch: Some(u16::MAX),
        handbrake: None,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.001);
    assert!(norm.brake.abs() < 0.001);
    assert!((norm.clutch.ok_or("clutch should be Some")? - 1.0).abs() < 0.001);
    assert!(norm.handbrake.is_none());
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 12. MozaEsJoystickMode exhaustive domain coverage
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn es_joystick_mode_all_u8_values() -> Result<(), Box<dyn std::error::Error>> {
    // Only 0 and 1 are valid
    let mut valid_count = 0u32;
    for v in 0u8..=255 {
        match MozaEsJoystickMode::from_config_value(v) {
            Some(MozaEsJoystickMode::Buttons) => {
                assert_eq!(v, 0);
                valid_count += 1;
            }
            Some(MozaEsJoystickMode::DPad) => {
                assert_eq!(v, 1);
                valid_count += 1;
            }
            None => {
                assert!(v >= 2, "values 0-1 must parse, got None for {v}");
            }
        }
    }
    assert_eq!(valid_count, 2, "exactly 2 valid joystick modes");
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 13. MozaInputState default equivalence
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn input_state_default_equals_empty_zero() -> Result<(), Box<dyn std::error::Error>> {
    let default = MozaInputState::default();
    let empty = MozaInputState::empty(0);
    assert_eq!(default, empty);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Property-based tests covering NEW ground
// ═════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(300))]

    // ── FfbConfig invariants across all known wheelbases ─────────────────

    /// All Moza wheelbases must set fix_conditional_direction and uses_vendor_usage_page,
    /// require a 1ms bInterval, and have a positive max_torque.
    #[test]
    fn prop_ffb_config_invariants_for_all_wheelbases(
        pid in prop_oneof![
            Just(product_ids::R3_V1),
            Just(product_ids::R3_V2),
            Just(product_ids::R5_V1),
            Just(product_ids::R5_V2),
            Just(product_ids::R9_V1),
            Just(product_ids::R9_V2),
            Just(product_ids::R12_V1),
            Just(product_ids::R12_V2),
            Just(product_ids::R16_R21_V1),
            Just(product_ids::R16_R21_V2),
        ]
    ) {
        let cfg = MozaProtocol::new(pid).get_ffb_config();
        prop_assert!(cfg.fix_conditional_direction, "PID 0x{:04X} fix_conditional_direction", pid);
        prop_assert!(cfg.uses_vendor_usage_page, "PID 0x{:04X} uses_vendor_usage_page", pid);
        prop_assert_eq!(cfg.required_b_interval, Some(1), "PID 0x{:04X} bInterval", pid);
        prop_assert!(cfg.max_torque_nm > 0.0, "PID 0x{:04X} positive max_torque", pid);
        prop_assert!(cfg.encoder_cpr > 0, "PID 0x{:04X} positive encoder_cpr", pid);
    }

    // ── Encoder CPR V1 vs V2 invariants ─────────────────────────────────

    /// V2 wheelbases always have a higher encoder CPR than their V1 counterparts.
    #[test]
    fn prop_v2_encoder_cpr_ge_v1(
        pair in prop_oneof![
            Just((product_ids::R3_V1, product_ids::R3_V2)),
            Just((product_ids::R5_V1, product_ids::R5_V2)),
            Just((product_ids::R9_V1, product_ids::R9_V2)),
            Just((product_ids::R12_V1, product_ids::R12_V2)),
            Just((product_ids::R16_R21_V1, product_ids::R16_R21_V2)),
        ]
    ) {
        let (v1_pid, v2_pid) = pair;
        let cpr_v1 = MozaProtocol::new(v1_pid).get_ffb_config().encoder_cpr;
        let cpr_v2 = MozaProtocol::new(v2_pid).get_ffb_config().encoder_cpr;
        prop_assert!(
            cpr_v2 > cpr_v1,
            "V2 CPR ({}) must exceed V1 CPR ({}) for PIDs 0x{:04X}/0x{:04X}", cpr_v2, cpr_v1, v1_pid, v2_pid
        );
    }

    // ── MozaHatDirection valid round-trip ────────────────────────────────

    /// All valid hat values (0..=8) must parse to Some and be distinct.
    #[test]
    fn prop_hat_direction_valid_range(value in 0u8..=8u8) {
        let dir = MozaHatDirection::from_hid_hat_value(value);
        prop_assert!(dir.is_some(), "hat value {} must parse", value);
    }

    /// Invalid hat values (9..=255) must parse to None.
    #[test]
    fn prop_hat_direction_invalid_range(value in 9u8..=255u8) {
        prop_assert!(MozaHatDirection::from_hid_hat_value(value).is_none());
    }

    // ── MozaPedalAxesRaw normalize None preservation ────────────────────

    /// If clutch is None in the raw struct, it must be None after normalize.
    #[test]
    fn prop_pedal_normalize_none_clutch_preserved(
        throttle: u16,
        brake: u16,
        handbrake: u16,
    ) {
        let raw = MozaPedalAxesRaw {
            throttle,
            brake,
            clutch: None,
            handbrake: Some(handbrake),
        };
        let norm = raw.normalize();
        prop_assert!(norm.clutch.is_none(), "None clutch must remain None after normalize");
        prop_assert!(norm.handbrake.is_some(), "Some handbrake must remain Some after normalize");
    }

    // ── Multi-encoder independence ──────────────────────────────────────

    /// Two encoders with different max_torque produce different raw values
    /// for the same Nm torque (when torque is within both ranges).
    #[test]
    fn prop_different_max_torque_different_raw(
        max_a in 1.0_f32..=10.0_f32,
        max_b in 11.0_f32..=21.0_f32,
        frac in 0.1_f32..=0.9_f32,
    ) {
        let torque = max_a * frac; // within range of both encoders
        let enc_a = MozaDirectTorqueEncoder::new(max_a);
        let enc_b = MozaDirectTorqueEncoder::new(max_b);
        let mut out_a = [0u8; REPORT_LEN];
        let mut out_b = [0u8; REPORT_LEN];
        enc_a.encode(torque, 0, &mut out_a);
        enc_b.encode(torque, 0, &mut out_b);
        let raw_a = i16::from_le_bytes([out_a[1], out_a[2]]);
        let raw_b = i16::from_le_bytes([out_b[1], out_b[2]]);
        // Same torque Nm but higher max → lower percent → lower raw
        prop_assert!(
            raw_a > raw_b,
            "same Nm with smaller max ({}) must yield higher raw ({}) than larger max ({}): raw_b={}", max_a, raw_a, max_b, raw_b
        );
    }

    // ── Signature verdict consistency with identity category ─────────────

    /// For every Moza VID PID: if category is Wheelbase, verdict must be KnownWheelbase.
    /// If Pedals/Shifter/Handbrake, must be KnownPeripheral.
    #[test]
    fn prop_signature_verdict_matches_category(pid: u16) {
        let identity = identify_device(pid);
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        let verdict = verify_signature(&sig);
        match identity.category {
            MozaDeviceCategory::Wheelbase => {
                prop_assert_eq!(verdict, SignatureVerdict::KnownWheelbase,
                    "wheelbase pid 0x{:04X}", pid);
            }
            MozaDeviceCategory::Pedals
            | MozaDeviceCategory::Shifter
            | MozaDeviceCategory::Handbrake => {
                prop_assert_eq!(verdict, SignatureVerdict::KnownPeripheral,
                    "peripheral pid 0x{:04X}", pid);
            }
            MozaDeviceCategory::Unknown => {
                prop_assert_ne!(verdict, SignatureVerdict::KnownWheelbase,
                    "unknown pid 0x{:04X} must not be KnownWheelbase", pid);
            }
        }
    }

    // ── Protocol model max_torque matches FfbConfig max_torque ───────────

    /// For all known wheelbases, MozaModel::max_torque_nm must equal
    /// the FfbConfig max_torque_nm from the protocol instance.
    #[test]
    fn prop_model_torque_equals_ffb_config_torque(
        pid in prop_oneof![
            Just(product_ids::R3_V1),
            Just(product_ids::R3_V2),
            Just(product_ids::R5_V1),
            Just(product_ids::R5_V2),
            Just(product_ids::R9_V1),
            Just(product_ids::R9_V2),
            Just(product_ids::R12_V1),
            Just(product_ids::R12_V2),
            Just(product_ids::R16_R21_V1),
            Just(product_ids::R16_R21_V2),
        ]
    ) {
        let protocol = MozaProtocol::new(pid);
        let model_nm = protocol.model().max_torque_nm();
        let config_nm = protocol.get_ffb_config().max_torque_nm;
        prop_assert!(
            (model_nm - config_nm).abs() < 0.01,
            "model says {} Nm but FfbConfig says {} Nm for PID 0x{:04X}", model_nm, config_nm, pid
        );
    }

    // ── output_report_id/len consistency with is_wheelbase ──────────────

    /// Wheelbases must have output report id/len; peripherals must not.
    #[test]
    fn prop_output_report_depends_on_wheelbase(pid: u16) {
        let protocol = MozaProtocol::new(pid);
        let is_wb = is_wheelbase_product(pid);
        if is_wb {
            prop_assert_eq!(protocol.output_report_id(), Some(report_ids::DIRECT_TORQUE),
                "wheelbase 0x{:04X} must have output report id", pid);
            prop_assert_eq!(protocol.output_report_len(), Some(REPORT_LEN),
                "wheelbase 0x{:04X} must have output report len", pid);
        } else {
            prop_assert_eq!(protocol.output_report_id(), None,
                "non-wheelbase 0x{:04X} must not have output report id", pid);
            prop_assert_eq!(protocol.output_report_len(), None,
                "non-wheelbase 0x{:04X} must not have output report len", pid);
        }
    }
}
