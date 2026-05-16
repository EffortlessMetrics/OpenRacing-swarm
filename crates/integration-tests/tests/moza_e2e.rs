//! BDD end-to-end tests for the Moza protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_moza_protocol::{FfbMode, MozaInitState, product_ids, report_ids};
use racing_wheel_integration_tests::moza_virtual::MozaScenario;

// ─── Scenario 1: wheelbase handshake ─────────────────────────────────────────

#[test]
fn scenario_wheelbase_only_handshake_with_high_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R5 V1 wheelbase with high_torque enabled
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R5_V1, FfbMode::Standard, true);

    // When: initialized
    s.initialize()?;

    // Then: state is Ready
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    // Then: exact R5 V1 handshake sequence [HIGH_TORQUE, FFB_MODE].
    // R5 V1 does not expose START_REPORTS as a valid feature report.
    let reports = s.device.feature_reports();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0][0], report_ids::HIGH_TORQUE);
    assert_eq!(reports[1][0], report_ids::FFB_MODE);
    assert_eq!(reports[1][1], FfbMode::Standard as u8);
    assert!(
        !s.device.sent_feature_report_id(report_ids::START_REPORTS),
        "R5 V1 must not send START_REPORTS as a feature report"
    );

    Ok(())
}

// ─── Scenario 2: high torque not sent by default ─────────────────────────────

#[test]
fn scenario_high_torque_not_sent_by_default() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R9 V2, default config (high_torque_enabled = false)
    let mut s = MozaScenario::wheelbase(product_ids::R9_V2);
    assert!(!s.protocol.is_high_torque_enabled());

    // When: initialized
    s.initialize()?;

    // Then: HIGH_TORQUE report is NOT in the sequence
    assert!(!s.device.sent_feature_report_id(report_ids::HIGH_TORQUE));

    // Then: START_REPORTS and FFB_MODE are still sent
    assert!(s.device.sent_feature_report_id(report_ids::START_REPORTS));
    assert!(s.device.sent_feature_report_id(report_ids::FFB_MODE));

    // Then: only 2 reports total
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

// ─── Scenario 3: high torque sent when explicitly enabled ────────────────────

#[test]
fn scenario_high_torque_sent_when_explicitly_enabled() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R9 V2, high_torque_enabled = true
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R9_V2, FfbMode::Standard, true);
    assert!(s.protocol.is_high_torque_enabled());

    // When: initialized
    s.initialize()?;

    // Then: HIGH_TORQUE IS first in the sequence
    let reports = s.device.feature_reports();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0][0], report_ids::HIGH_TORQUE);

    Ok(())
}

// ─── Scenario 4: FFB not ready before handshake ──────────────────────────────

#[test]
fn scenario_ffb_not_ready_before_handshake() {
    // Given: wheelbase, not yet initialized
    let s = MozaScenario::wheelbase(product_ids::R5_V2);

    // Then: is_ffb_ready returns false
    assert!(!s.protocol.is_ffb_ready());
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);
}

// ─── Scenario 5: FFB becomes ready after handshake ───────────────────────────

#[test]
fn scenario_ffb_ready_after_successful_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R5 V1 wheelbase
    let mut s = MozaScenario::wheelbase(product_ids::R5_V1);
    assert!(!s.protocol.is_ffb_ready());

    // When: initialized
    s.initialize()?;

    // Then: is_ffb_ready returns true
    assert!(s.protocol.is_ffb_ready());

    Ok(())
}

// ─── Scenario 6: handshake retry on transient IO failure ─────────────────────

#[test]
fn scenario_handshake_retry_on_transient_io_failure() -> Result<(), Box<dyn std::error::Error>> {
    // Given: device with IO failures (simulated)
    let mut s = MozaScenario::wheelbase_failing(product_ids::R5_V1);

    // When: first attempt fails
    assert!(
        s.initialize().is_err(),
        "transient write failure must surface while leaving retry state"
    );
    assert_eq!(s.protocol.init_state(), MozaInitState::Failed);
    assert!(
        s.protocol.can_retry(),
        "should be able to retry after first failure"
    );

    // Given: device recovers
    s.device.reconnect();

    // When: retry succeeds
    s.initialize()?;

    // Then: state is Ready
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);
    assert!(s.protocol.is_ffb_ready());

    Ok(())
}

// ─── Scenario 7: retries bounded by max_retries ──────────────────────────────

#[test]
fn scenario_retries_bounded_no_deadlock() -> Result<(), Box<dyn std::error::Error>> {
    // Given: always-failing device
    let mut s = MozaScenario::wheelbase_failing(product_ids::R5_V1);

    // When: exhausting retries (DEFAULT_MAX_RETRIES = 3)
    for _ in 0..3 {
        assert!(
            s.initialize().is_err(),
            "failing virtual device must surface initialization errors"
        );
    }

    // Then: state is PermanentFailure (not deadlocked)
    assert_eq!(s.protocol.init_state(), MozaInitState::PermanentFailure);
    assert!(!s.protocol.can_retry());

    // Then: further calls are no-ops (no new reports)
    let report_count_before = s.device.feature_reports().len();
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), report_count_before);

    Ok(())
}

// ─── Scenario 8: disconnect resets handshake ─────────────────────────────────

#[test]
fn scenario_disconnect_resets_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Ready device
    let mut s = MozaScenario::wheelbase(product_ids::R5_V1);
    s.initialize()?;
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    // When: disconnect
    s.device.disconnect();
    s.protocol.reset_to_uninitialized();

    // Then: state is Uninitialized
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);
    assert!(!s.protocol.is_ffb_ready());

    // Then: reconnect + re-initialize succeeds
    s.device.reconnect();
    s.initialize()?;
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    Ok(())
}

// ─── Scenario 9: peripheral skips initialization ─────────────────────────────

#[test]
fn scenario_peripheral_device_skips_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: HBP handbrake (peripheral)
    let mut s = MozaScenario::wheelbase(product_ids::HBP_HANDBRAKE);

    // When: initialize called
    s.initialize()?;

    // Then: no feature reports sent
    assert!(
        s.device.feature_reports().is_empty(),
        "peripheral should not receive handshake"
    );

    // Then: state stays Uninitialized (peripheral not tracked)
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);

    Ok(())
}

// ─── Scenario 10: direct torque FFB mode ─────────────────────────────────────

#[test]
fn scenario_direct_torque_ffb_mode_sets_mode_byte() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R12 V2 in direct torque mode
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R12_V2, FfbMode::Direct, false);

    // When: initialized
    s.initialize()?;

    // Then: FFB_MODE report contains Direct mode byte (0x02)
    let ffb_reports = s.device.feature_reports_with_id(report_ids::FFB_MODE);
    assert_eq!(ffb_reports.len(), 1);
    assert_eq!(ffb_reports[0][1], FfbMode::Direct as u8);

    Ok(())
}

// ─── Scenario 11: golden R5 V1 input parse ───────────────────────────────────

#[test]
fn scenario_r5_v1_golden_input_report_parse() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, input_report};

    // Given: R5 V1 protocol
    let protocol = MozaProtocol::new(product_ids::R5_V1);

    // When: a golden 0x01 report with known byte values
    // steering = 0x8000 (center), throttle = 0xFFFF (full), brake = 0x0000 (off)
    let report = [
        input_report::REPORT_ID,
        0x00,
        0x80, // steering = 0x8000 (center)
        0xFF,
        0xFF, // throttle = 0xFFFF (full)
        0x00,
        0x00, // brake = 0x0000 (off)
    ];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected successful parse of golden R5 V1 report")?;

    // Then: pedal axes decode to expected raw values
    let pedals = protocol
        .parse_aggregated_pedal_axes(&report)
        .ok_or("expected aggregated pedal parse")?;
    assert_eq!(pedals.throttle, 0xFFFF, "full throttle should be 0xFFFF");
    assert_eq!(pedals.brake, 0x0000, "zero brake should be 0x0000");

    // Then: normalized pedals hit the expected extremes
    let normalized = pedals.normalize();
    assert!(
        (normalized.throttle - 1.0).abs() < 0.0001,
        "full throttle normalizes to 1.0, got {}",
        normalized.throttle
    );
    assert!(
        normalized.brake.abs() < 0.0001,
        "zero brake normalizes to 0.0, got {}",
        normalized.brake
    );

    // Then: state contains correct raw throttle/brake
    assert_eq!(state.throttle_u16, 0xFFFF);
    assert_eq!(state.brake_u16, 0x0000);

    Ok(())
}

// ─── Scenario 12: SR-P standalone golden parse ───────────────────────────────

#[test]
fn scenario_srp_standalone_golden_parse() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: SR-P standalone protocol
    let protocol = MozaProtocol::new(product_ids::SR_P_PEDALS);

    // When: a golden SR-P report [report_id, thr_lo, thr_hi, brk_lo, brk_hi]
    // Note: SR-P standalone reports use 0x01 as the report ID byte.
    let report = [0x01u8, 0x00, 0x40, 0xFF, 0xFF];

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected SR-P parse")?;

    // Then: throttle at 0x4000 (~25%), brake at 0xFFFF (full)
    assert_eq!(state.throttle_u16, 0x4000);
    assert_eq!(state.brake_u16, 0xFFFF);

    Ok(())
}

// ─── Scenario 13: HBP standalone golden parse ────────────────────────────────

#[test]
fn scenario_hbp_standalone_golden_parse() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: HBP standalone protocol
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    // When: a 2-byte raw report (no report ID prefix) with half-pull value
    let report = [0x00, 0x80]; // 0x8000 (half-pull)
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected HBP parse")?;

    // Then: handbrake at expected raw value
    assert_eq!(
        state.handbrake_u16, 0x8000,
        "half-pull should decode to 0x8000"
    );

    Ok(())
}

// ─── Scenario 14: rotation range command encoding ────────────────────────────

#[test]
fn scenario_rotation_range_command_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{MOZA_VENDOR_ID, MozaProtocol};
    use racing_wheel_integration_tests::moza_virtual::VirtualMozaDevice;

    // Given: R5 V1 wheelbase
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let mut device = VirtualMozaDevice::new(MOZA_VENDOR_ID, product_ids::R5_V1);

    // When: set_rotation_range is called with 900 degrees
    protocol.set_rotation_range(&mut device, 900)?;

    // Then: exactly one feature report was sent
    let reports = device.feature_reports();
    assert_eq!(reports.len(), 1, "exactly one feature report expected");

    // Then: report starts with ROTATION_RANGE report ID
    assert_eq!(reports[0][0], report_ids::ROTATION_RANGE);

    // Then: second byte is the Set command (0x01)
    assert_eq!(reports[0][1], 0x01, "second byte must be Set Range command");

    // Then: bytes 2-3 contain 900 in little-endian
    let degrees = u16::from_le_bytes([reports[0][2], reports[0][3]]);
    assert_eq!(degrees, 900, "rotation range must be 900 degrees");

    Ok(())
}

// ─── Scenario 15: V1 vs V2 encoder CPR verification ─────────────────────────

#[test]
fn scenario_v1_vs_v2_encoder_cpr_differs() {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, VendorProtocol};

    // Given: R5 V1 and R5 V2 protocols
    let v1 = MozaProtocol::new(product_ids::R5_V1);
    let v2 = MozaProtocol::new(product_ids::R5_V2);

    // Then: V1 uses 15-bit encoder (CPR = 32768)
    let v1_config = v1.get_ffb_config();
    assert_eq!(
        v1_config.encoder_cpr, 32768,
        "R5 V1 must use 15-bit encoder (CPR=32768)"
    );

    // Then: V2 uses 18-bit encoder (CPR = 262144)
    let v2_config = v2.get_ffb_config();
    assert_eq!(
        v2_config.encoder_cpr, 262144,
        "R5 V2 must use 18-bit encoder (CPR=262144)"
    );

    // Then: Both share the same max torque (5.5 Nm for R5)
    assert!(
        (v1_config.max_torque_nm - 5.5).abs() < 0.01,
        "R5 V1 max torque must be 5.5 Nm"
    );
    assert!(
        (v2_config.max_torque_nm - 5.5).abs() < 0.01,
        "R5 V2 max torque must be 5.5 Nm"
    );
}

// ─── Scenario 16: KS-attached wheelbase button and hat parsing ───────────────

#[test]
fn scenario_ks_attached_wheelbase_buttons_and_hat() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, input_report, rim_ids};

    // Given: R9 V2 with KS wheel attached
    let protocol = MozaProtocol::new(product_ids::R9_V2);

    // When: report with specific button and hat bytes
    // Layout: [report_id, steer_lo, steer_hi, thr_lo, thr_hi, brk_lo, brk_hi,
    //          clch_lo, clch_hi, hb_lo, hb_hi,
    //          btn[0..16], hat, funky, rot[0..1], rot[1..2]]
    let mut report = [0u8; 31];
    report[0] = input_report::REPORT_ID;
    report[1] = 0x00;
    report[2] = 0x80; // steering center
    report[3] = 0x00;
    report[4] = 0x00; // throttle = 0
    report[5] = 0x00;
    report[6] = 0x00; // brake = 0
    // button bytes [11..27]: set button[0] = 0b00000011 (buttons 0 & 1 pressed)
    report[11] = 0x03;
    // hat byte at [27]
    report[27] = 0x04; // Down (per Moza hat encoding)
    // funky byte at [28] identifies KS rim attachment
    report[input_report::FUNKY_START] = rim_ids::KS;

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected R9 V2 parse")?;

    // Then: button byte 0 contains our set bits
    assert_eq!(
        state.ks_snapshot.buttons[0], 0x03,
        "first button byte must be 0x03"
    );

    // Then: hat byte matches
    assert_eq!(
        state.ks_snapshot.hat, 0x04,
        "hat direction byte must be 0x04 (Down)"
    );

    Ok(())
}

// ─── Scenario 17: HBP button-mode byte lands in buttons[0] ──────────────────

#[test]
fn scenario_hbp_button_mode_byte_in_buttons() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: HBP standalone protocol
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    // When: report-id-prefixed layout with button byte set (button pressed)
    // Layout: [report_id=0x01, axis_lo, axis_hi, button_byte]
    let report = [0x01u8, 0x00, 0x80, 0x01]; // 0x8000 axis, button=0x01

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected HBP parse with button byte")?;

    // Then: handbrake axis at expected raw value (0x8000)
    assert_eq!(
        state.handbrake_u16, 0x8000,
        "handbrake axis should be 0x8000"
    );

    // Then: button byte propagated to buttons[0]
    assert_eq!(
        state.buttons[0], 0x01,
        "button byte must land in buttons[0]"
    );

    Ok(())
}

// ─── Scenario 17.1: HBP button released clears buttons[0] ────────────────────

#[test]
fn scenario_hbp_button_released_has_zero_button_byte() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: HBP standalone protocol
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    // When: raw two-byte layout (no button byte, handbrake fully released)
    let report = [0x00u8, 0x00]; // 0x0000 = fully released

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected HBP raw two-byte parse")?;

    // Then: handbrake axis is zero (released)
    assert_eq!(state.handbrake_u16, 0x0000, "released HBP should be 0x0000");

    // Then: buttons[0] is clear (no button byte in this layout)
    assert_eq!(
        state.buttons[0], 0x00,
        "buttons[0] must be clear without button byte"
    );

    Ok(())
}

// ─── Scenario 17.2: HBP raw-with-button layout full-pull ─────────────────────

#[test]
fn scenario_hbp_raw_with_button_full_pull() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: HBP standalone protocol
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    // When: raw layout with button: [axis_lo, axis_hi, button_byte]
    let report = [0xFF, 0xFF, 0x01u8]; // 0xFFFF = full pull, button pressed

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected HBP raw-with-button parse")?;

    // Then: handbrake at maximum raw value
    assert_eq!(
        state.handbrake_u16, 0xFFFF,
        "full-pull should decode to 0xFFFF"
    );

    // Then: button byte present
    assert_eq!(state.buttons[0], 0x01, "button byte must be 0x01");

    Ok(())
}

// ─── Scenario 17.3: HBP wrong product ID returns None ────────────────────────

#[test]
fn scenario_hbp_wrong_product_id_returns_none() {
    use racing_wheel_hid_moza_protocol::MozaProtocol;

    // Given: protocol initialized with a wheelbase PID (not HBP)
    let protocol = MozaProtocol::new(product_ids::R5_V1);

    // When: same bytes that would parse as HBP are fed to a wheelbase protocol
    // (short report, no report ID 0x01)
    let report = [0xAAu8, 0x55];

    // Then: does not parse as HBP (would parse as wheelbase if it were a valid wheelbase report,
    // but this is too short so it returns None)
    let state = protocol.parse_input_state(&report);
    assert!(
        state.is_none(),
        "wheelbase protocol must not misparse short reports as HBP"
    );
}

// ─── Scenario 18: KS rim detection via funky byte ────────────────────────────

#[test]
fn scenario_ks_rim_detected_via_funky_byte() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{KsClutchMode, MozaProtocol, input_report, rim_ids};

    // Given: R9 V2 wheelbase protocol
    let protocol = MozaProtocol::new(product_ids::R9_V2);

    // When: report with funky byte = KS rim ID (0x05) and buttons set
    let mut report = [0u8; 31];
    report[0] = input_report::REPORT_ID;
    report[2] = 0x80; // steering center
    report[input_report::FUNKY_START] = rim_ids::KS; // KS rim marker
    report[input_report::BUTTONS_START] = 0b0000_1010; // buttons 1 and 3

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected R9 V2 + KS parse")?;

    // Then: button byte correctly parsed via KS map
    assert_eq!(
        state.ks_snapshot.buttons[0], 0b0000_1010,
        "buttons[0] must reflect report byte"
    );

    // Then: clutch mode is Unknown (no clutch axes defined in default map)
    assert_eq!(
        state.ks_snapshot.clutch_mode,
        KsClutchMode::Unknown,
        "clutch mode must be Unknown without a capture-validated map"
    );

    Ok(())
}

// ─── Scenario 18.1: non-KS rim uses fallback snapshot ────────────────────────

#[test]
fn scenario_non_ks_rim_uses_fallback_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{
        KsClutchMode, KsJoystickMode, KsRotaryMode, MozaProtocol, input_report, rim_ids,
    };

    // Given: R5 V2 wheelbase, no KS rim attached
    let protocol = MozaProtocol::new(product_ids::R5_V2);

    // When: report with funky byte = CS V2 rim (0x01)
    let mut report = [0u8; 31];
    report[0] = input_report::REPORT_ID;
    report[2] = 0x80;
    report[input_report::FUNKY_START] = rim_ids::CS_V2;
    report[input_report::BUTTONS_START] = 0xAB;
    report[input_report::HAT_START] = 0x02;
    report[input_report::ROTARY_START] = 0x10;
    report[input_report::ROTARY_START + 1] = 0x20;

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected R5 V2 parse")?;

    // Then: fallback snapshot is used (common controls)
    assert_eq!(
        state.ks_snapshot.buttons[0], 0xAB,
        "buttons must come from the common fallback snapshot"
    );
    assert_eq!(state.ks_snapshot.hat, 0x02, "hat must be from report byte");

    // Then: fallback provides rotary bytes in encoders[0..1]
    assert_eq!(state.ks_snapshot.encoders[0], 0x10, "rotary 0 via fallback");
    assert_eq!(state.ks_snapshot.encoders[1], 0x20, "rotary 1 via fallback");

    // Then: higher encoders are zero (not in fallback map)
    assert!(
        state.ks_snapshot.encoders[2..].iter().all(|&e| e == 0),
        "higher encoder slots must be zero in fallback path"
    );

    // Then: modes all Unknown in fallback
    assert_eq!(state.ks_snapshot.clutch_mode, KsClutchMode::Unknown);
    assert_eq!(state.ks_snapshot.rotary_mode, KsRotaryMode::Unknown);
    assert_eq!(state.ks_snapshot.joystick_mode, KsJoystickMode::Unknown);

    Ok(())
}

// ─── Scenario 18.2: KS rim rotary bytes preserved through parse ──────────────

#[test]
fn scenario_ks_rim_rotary_bytes_preserved() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, input_report, rim_ids};

    // Given: R9 V2 wheelbase with KS rim
    let protocol = MozaProtocol::new(product_ids::R9_V2);

    // When: report with KS rim and specific rotary bytes
    let mut report = [0u8; 31];
    report[0] = input_report::REPORT_ID;
    report[2] = 0x80;
    report[input_report::FUNKY_START] = rim_ids::KS;
    report[input_report::ROTARY_START] = 0x42; // rotary 0
    report[input_report::ROTARY_START + 1] = 0x7E; // rotary 1

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected KS parse")?;

    // Then: rotary bytes end up in encoders[0] and encoders[1]
    // (base wheelbase rotary bytes are authoritative even with KS rim)
    assert_eq!(
        state.ks_snapshot.encoders[0], 0x42,
        "rotary 0 must be preserved in encoder[0]"
    );
    assert_eq!(
        state.ks_snapshot.encoders[1], 0x7E,
        "rotary 1 must be preserved in encoder[1]"
    );

    Ok(())
}

// ─── Scenario 18.3: KS rim hat byte round-trip ────────────────────────────────

#[test]
fn scenario_ks_hat_byte_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, input_report, rim_ids};

    // Given: R12 V2 wheelbase with KS rim
    let protocol = MozaProtocol::new(product_ids::R12_V2);

    // Hat encoding (Moza): 0=center, 1=up, 2=right, 3=down, 4=left
    for (hat_value, description) in [
        (0x00u8, "center"),
        (0x01, "up"),
        (0x03, "down"),
        (0x04, "left"),
    ] {
        let mut report = [0u8; 31];
        report[0] = input_report::REPORT_ID;
        report[2] = 0x80;
        report[input_report::FUNKY_START] = rim_ids::KS;
        report[input_report::HAT_START] = hat_value;

        let state = protocol.parse_input_state(&report).ok_or(format!(
            "expected parse for hat={hat_value} ({description})"
        ))?;

        assert_eq!(
            state.ks_snapshot.hat, hat_value,
            "hat value {description} ({hat_value}) must round-trip correctly"
        );
    }

    Ok(())
}

// ─── Scenario 18.4: non-wheelbase product has empty KS snapshot ───────────────

#[test]
fn scenario_non_wheelbase_has_empty_ks_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::{
        KS_ENCODER_COUNT, KsClutchMode, KsJoystickMode, KsRotaryMode, MozaProtocol, input_report,
    };

    // Given: HBP handbrake (non-wheelbase peripheral)
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);

    // When: a valid 2-byte raw HBP report
    let report = [0xFFu8, 0xFF];
    let state = protocol.parse_input_state(&report);

    // Then: parses successfully
    assert!(state.is_some(), "HBP report must parse");
    let state = state.ok_or("expected parse result")?;

    // Then: KS snapshot is default-empty (HBP is not a wheelbase)
    assert_eq!(state.ks_snapshot.clutch_mode, KsClutchMode::Unknown);
    assert_eq!(state.ks_snapshot.rotary_mode, KsRotaryMode::Unknown);
    assert_eq!(state.ks_snapshot.joystick_mode, KsJoystickMode::Unknown);
    assert!(state.ks_snapshot.clutch_combined.is_none());
    assert_eq!(state.ks_snapshot.encoders, [0i16; KS_ENCODER_COUNT]);

    // Also: there's no REPORT_ID prefix for HBP raw layout, so the input_report constant
    // is not mentioned here, but we confirm non-wheelbase handling is separate.
    let _ = input_report::REPORT_ID; // just ensure it's reachable

    Ok(())
}

// ─── Scenario 19: double-disconnect does not panic ────────────────────────────

#[test]
fn scenario_double_disconnect_is_safe() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized wheelbase
    let mut s = MozaScenario::wheelbase(product_ids::R5_V1);
    s.initialize()?;

    // When: disconnect twice in a row
    s.device.disconnect();
    s.protocol.reset_to_uninitialized();
    // A second disconnect/reset should be idempotent
    s.device.disconnect();
    s.protocol.reset_to_uninitialized();

    // Then: state is Uninitialized, no panic
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);

    Ok(())
}

// ─── Scenario 18: FFB off mode encoding ──────────────────────────────────────

#[test]
fn scenario_ffb_off_mode_encoding() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R5 V1 with FFB mode = Off
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R5_V1, FfbMode::Off, false);

    // When: initialized
    s.initialize()?;

    // Then: FFB_MODE report contains Off byte (0xFF)
    let ffb_reports = s.device.feature_reports_with_id(report_ids::FFB_MODE);
    assert_eq!(ffb_reports.len(), 1);
    assert_eq!(
        ffb_reports[0][1],
        FfbMode::Off as u8,
        "Off mode byte must be 0xFF"
    );

    Ok(())
}

// ─── Scenario 19: R9 V2 full handshake report order ──────────────────────────

#[test]
fn scenario_r9_v2_full_handshake_report_order() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R9 V2 with high torque enabled
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R9_V2, FfbMode::Standard, true);

    // When: initialized
    s.initialize()?;

    let reports = s.device.feature_reports();
    assert_eq!(
        reports.len(),
        3,
        "R9 V2 high_torque=true requires 3 reports"
    );

    // Then: report 0 = [HIGH_TORQUE, 0, 0, 0]
    assert_eq!(reports[0], vec![report_ids::HIGH_TORQUE, 0x00, 0x00, 0x00]);

    // Then: report 1 = [START_REPORTS, 0, 0, 0]
    assert_eq!(
        reports[1],
        vec![report_ids::START_REPORTS, 0x00, 0x00, 0x00]
    );

    // Then: report 2 = [FFB_MODE, Standard byte, 0, 0]
    assert_eq!(
        reports[2],
        vec![report_ids::FFB_MODE, FfbMode::Standard as u8, 0x00, 0x00]
    );

    Ok(())
}
