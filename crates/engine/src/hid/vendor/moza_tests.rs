//! Tests for Moza protocol handler

use super::moza::{
    ES_BUTTON_COUNT, ES_LED_COUNT, FfbMode, MozaDeviceCategory, MozaEsCompatibility,
    MozaEsJoystickMode, MozaHatDirection, MozaInitState, MozaModel, MozaProtocol, MozaRetryPolicy,
    MozaTopologyHint, es_compatibility, identify_device, input_report, is_wheelbase_product,
    product_ids, report_ids,
};
use super::moza_direct::REPORT_LEN;
use super::{DeviceWriter, FfbConfig, VendorProtocol, get_vendor_protocol};
use crate::input::KsClutchMode;
use std::cell::RefCell;

/// Mock device writer for testing
struct MockDeviceWriter {
    feature_reports: RefCell<Vec<Vec<u8>>>,
    output_reports: RefCell<Vec<Vec<u8>>>,
    fail_on_write: bool,
}

impl MockDeviceWriter {
    fn new() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
            fail_on_write: false,
        }
    }

    fn with_failure() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
            fail_on_write: true,
        }
    }

    fn get_feature_reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }
}

impl DeviceWriter for MockDeviceWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        let len = data.len();
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        let len = data.len();
        self.output_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }
}

#[test]
fn test_moza_protocol_creation() {
    let protocol = MozaProtocol::new(0x0002);
    assert_eq!(protocol.model(), MozaModel::R9);
    assert!(!protocol.is_v2_hardware());

    let protocol_v2 = MozaProtocol::new(0x0012);
    assert_eq!(protocol_v2.model(), MozaModel::R9);
    assert!(protocol_v2.is_v2_hardware());
}

#[test]
fn test_moza_model_from_pid() {
    // V1 PIDs
    assert_eq!(MozaModel::from_pid(0x0005), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(0x0004), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(0x0002), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(0x0006), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(0x0000), MozaModel::R16);
    assert_eq!(MozaModel::from_pid(0x0003), MozaModel::SrpPedals);

    // V2 PIDs
    assert_eq!(MozaModel::from_pid(0x0015), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(0x0014), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(0x0012), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(0x0016), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(0x0010), MozaModel::R16);

    // Unknown
    assert_eq!(MozaModel::from_pid(0xFFFF), MozaModel::Unknown);
}

#[test]
fn test_moza_identity_wheelbase_topology() {
    let identity = identify_device(product_ids::R9_V2);
    assert_eq!(identity.category, MozaDeviceCategory::Wheelbase);
    assert_eq!(
        identity.topology_hint,
        MozaTopologyHint::WheelbaseAggregated
    );
    assert!(identity.supports_ffb);
    assert!(is_wheelbase_product(product_ids::R9_V2));
}

#[test]
fn test_moza_identity_peripherals() {
    let pedals = identify_device(product_ids::SR_P_PEDALS);
    assert_eq!(pedals.category, MozaDeviceCategory::Pedals);
    assert_eq!(pedals.topology_hint, MozaTopologyHint::StandaloneUsb);
    assert!(!pedals.supports_ffb);

    let shifter = identify_device(product_ids::HGP_SHIFTER);
    assert_eq!(shifter.category, MozaDeviceCategory::Shifter);
    assert_eq!(shifter.topology_hint, MozaTopologyHint::StandaloneUsb);
    assert!(!shifter.supports_ffb);

    let unknown = identify_device(0xFEED);
    assert_eq!(unknown.category, MozaDeviceCategory::Unknown);
    assert_eq!(unknown.topology_hint, MozaTopologyHint::Unknown);
    assert!(!unknown.supports_ffb);
    assert!(!is_wheelbase_product(0xFEED));
}

#[test]
fn test_moza_parse_aggregated_pedal_axes_basic() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);

    let report = [
        input_report::REPORT_ID,
        0x00,
        0x80, // steering center
        0x34,
        0x12, // throttle = 0x1234
        0xCD,
        0xAB, // brake = 0xABCD
        0x0F,
        0x0F, // clutch = 0x0F0F
        0xAA,
        0x55, // handbrake = 0x55AA
    ];

    let parsed = protocol
        .parse_aggregated_pedal_axes(&report)
        .ok_or("failed to parse aggregated pedal axes")?;

    assert_eq!(parsed.throttle, 0x1234);
    assert_eq!(parsed.brake, 0xABCD);
    assert_eq!(parsed.clutch, Some(0x0F0F));
    assert_eq!(parsed.handbrake, Some(0x55AA));
    Ok(())
}

#[test]
fn test_moza_parse_aggregated_pedal_axes_missing_optional_axes()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R3_V1);

    // Includes report ID + steering + throttle + brake only.
    let report = [
        input_report::REPORT_ID,
        0xFF,
        0x7F, // steering near center
        0x00,
        0x10, // throttle = 0x1000
        0x00,
        0x20, // brake = 0x2000
    ];

    let parsed = protocol
        .parse_aggregated_pedal_axes(&report)
        .ok_or("failed to parse required throttle/brake axes")?;

    assert_eq!(parsed.throttle, 0x1000);
    assert_eq!(parsed.brake, 0x2000);
    assert_eq!(parsed.clutch, None);
    assert_eq!(parsed.handbrake, None);
    Ok(())
}

#[test]
fn test_moza_parse_aggregated_pedal_axes_rejects_wrong_report_id() {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let report = [
        0x02, // telemetry report, not input report
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let parsed = protocol.parse_aggregated_pedal_axes(&report);
    assert_eq!(parsed, None);
}

#[test]
fn test_moza_parse_input_state_populates_ks_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);

    let report = [
        input_report::REPORT_ID,
        0x00,
        0x80,
        0x34,
        0x12,
        0xCD,
        0xAB,
        0x0F,
        0x0F,
        0xAA,
        0x55,
        0x01,
        0x02,
        0x03,
        0x04,
        0x05,
        0x06,
        0x07,
        0x08,
        0x09,
        0x0A,
        0x0B,
        0x0C,
        0x0D,
        0x0E,
        0x0F,
        0x10,
        0x11,
        0x12,
        0x13,
        0x14,
        0x15,
        0x16,
    ];

    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected wheelbase state parse")?;

    let expected_buttons = [0x01u8, 0x02, 0x03, 0x04];
    assert_eq!(&state.ks_snapshot.buttons[..4], &expected_buttons[..]);
    assert_eq!(state.ks_snapshot.hat, 0x11);
    assert_eq!(state.ks_snapshot.clutch_mode, KsClutchMode::Unknown);
    assert_eq!(state.ks_snapshot.clutch_left, None);
    Ok(())
}

#[test]
fn test_moza_pedal_axis_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V2);

    let report = [
        input_report::REPORT_ID,
        0x00,
        0x80,
        0x00,
        0x00, // throttle = 0x0000
        0xFF,
        0xFF, // brake = 0xFFFF
        0x00,
        0x80, // clutch = 0x8000
    ];

    let normalized = protocol
        .parse_aggregated_pedal_axes(&report)
        .ok_or("failed to parse report for normalization test")?
        .normalize();

    assert_eq!(normalized.throttle, 0.0);
    assert_eq!(normalized.brake, 1.0);

    let clutch = normalized.clutch.ok_or("expected clutch sample")?;
    assert!((clutch - (32768.0 / 65535.0)).abs() < 0.000_01);
    assert_eq!(normalized.handbrake, None);
    Ok(())
}

#[test]
fn test_moza_parse_standalone_hbp_state_with_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);
    let report = [0x11, 0x34, 0x12, 0x80];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected handbrake-only report")?;

    assert_eq!(state.handbrake_u16, 0x1234);
    assert_eq!(state.clutch_u16, 0);
    assert_eq!(state.buttons[0], 0x80);
    Ok(())
}

#[test]
fn test_moza_parse_standalone_srp_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::SR_P_PEDALS);
    let report = [input_report::REPORT_ID, 0x34, 0x12, 0x78, 0x56];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected standalone SR-P pedal report")?;

    assert_eq!(state.throttle_u16, 0x1234);
    assert_eq!(state.brake_u16, 0x5678);
    assert_eq!(state.clutch_u16, 0);
    assert_eq!(state.handbrake_u16, 0);
    Ok(())
}

#[test]
fn test_moza_parse_standalone_hbp_state_without_report_id() -> Result<(), Box<dyn std::error::Error>>
{
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);
    let report = [0xAA, 0x55];
    let state = protocol
        .parse_input_state(&report)
        .ok_or("expected handbrake-only report")?;

    assert_eq!(state.handbrake_u16, 0x55AA);
    assert_eq!(state.clutch_u16, 0);
    Ok(())
}

#[test]
fn test_moza_es_compatibility_matrix() {
    // Vendor-documented R9 split.
    assert_eq!(
        es_compatibility(product_ids::R9_V1),
        MozaEsCompatibility::UnsupportedHardwareRevision
    );
    assert_eq!(
        es_compatibility(product_ids::R9_V2),
        MozaEsCompatibility::Supported
    );

    // Known bundle-compatible base.
    assert_eq!(
        es_compatibility(product_ids::R5_V1),
        MozaEsCompatibility::Supported
    );

    // Not yet capture-validated in this codebase.
    assert_eq!(
        es_compatibility(product_ids::R12_V2),
        MozaEsCompatibility::UnknownWheelbase
    );

    // Not a wheelbase.
    assert_eq!(
        es_compatibility(product_ids::SR_P_PEDALS),
        MozaEsCompatibility::NotWheelbase
    );
}

#[test]
fn test_moza_es_compatibility_protocol_accessor() {
    let v1 = MozaProtocol::new(product_ids::R9_V1);
    assert_eq!(
        v1.es_compatibility(),
        MozaEsCompatibility::UnsupportedHardwareRevision
    );

    let v2 = MozaProtocol::new(product_ids::R9_V2);
    assert_eq!(v2.es_compatibility(), MozaEsCompatibility::Supported);
    assert!(v2.es_compatibility().is_supported());
}

#[test]
fn test_moza_es_compatibility_diagnostic_messages() {
    let incompatible = MozaEsCompatibility::UnsupportedHardwareRevision.diagnostic_message();
    assert!(incompatible.is_some());

    let unknown = MozaEsCompatibility::UnknownWheelbase.diagnostic_message();
    assert!(unknown.is_some());

    let not_wheelbase = MozaEsCompatibility::NotWheelbase.diagnostic_message();
    assert!(not_wheelbase.is_none());
}

#[test]
fn test_moza_es_joystick_mode_from_config() {
    assert_eq!(
        MozaEsJoystickMode::from_config_value(0),
        Some(MozaEsJoystickMode::Buttons)
    );
    assert_eq!(
        MozaEsJoystickMode::from_config_value(1),
        Some(MozaEsJoystickMode::DPad)
    );
    assert_eq!(MozaEsJoystickMode::from_config_value(2), None);
}

#[test]
fn test_moza_hat_direction_parsing() {
    assert_eq!(
        MozaHatDirection::from_hid_hat_value(0),
        Some(MozaHatDirection::Up)
    );
    assert_eq!(
        MozaHatDirection::from_hid_hat_value(4),
        Some(MozaHatDirection::Down)
    );
    assert_eq!(
        MozaHatDirection::from_hid_hat_value(8),
        Some(MozaHatDirection::Center)
    );
    assert_eq!(MozaHatDirection::from_hid_hat_value(9), None);
}

#[test]
fn test_moza_es_surface_constants() {
    assert_eq!(ES_BUTTON_COUNT, 22);
    assert_eq!(ES_LED_COUNT, 10);
}

#[test]
fn test_moza_max_torque() {
    assert!((MozaModel::R3.max_torque_nm() - 3.9).abs() < 0.01);
    assert!((MozaModel::R5.max_torque_nm() - 5.5).abs() < 0.01);
    assert!((MozaModel::R9.max_torque_nm() - 9.0).abs() < 0.01);
    assert!((MozaModel::R12.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((MozaModel::R16.max_torque_nm() - 16.0).abs() < 0.01);
    assert!((MozaModel::R21.max_torque_nm() - 21.0).abs() < 0.01);
    assert!((MozaModel::SrpPedals.max_torque_nm() - 0.0).abs() < 0.01);
    assert!((MozaModel::Unknown.max_torque_nm() - 10.0).abs() < 0.01);
}

#[test]
fn test_moza_encoder_cpr() {
    // V1 devices use 15-bit encoder
    let v1_protocol = MozaProtocol::new(0x0002); // R9 V1
    let v1_config = v1_protocol.get_ffb_config();
    assert_eq!(v1_config.encoder_cpr, 32768);

    // V2 standard devices use 18-bit encoder
    let v2_r9 = MozaProtocol::new(0x0012); // R9 V2
    let v2_config = v2_r9.get_ffb_config();
    assert_eq!(v2_config.encoder_cpr, 262144);

    // V2 R16/R21 use 21-bit encoder
    let v2_r16 = MozaProtocol::new(0x0010); // R16 V2
    let r16_config = v2_r16.get_ffb_config();
    assert_eq!(r16_config.encoder_cpr, 2097152);
}

#[test]
fn test_moza_ffb_config() {
    let protocol = MozaProtocol::new(0x0002); // R9 V1
    let config = protocol.get_ffb_config();

    assert!(config.fix_conditional_direction);
    assert!(config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 9.0).abs() < 0.01);
}

#[test]
fn test_moza_initialize_device() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Standard, true);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 3); // high torque, start reports, ffb mode

    // Check high torque report
    assert_eq!(reports[0][0], super::moza::report_ids::HIGH_TORQUE);
    assert_eq!(reports[0][1], 0x00);
    assert_eq!(reports[0][2], 0x00);
    assert_eq!(reports[0][3], 0x00);

    // Check start reports
    assert_eq!(reports[1][0], super::moza::report_ids::START_REPORTS);
    assert_eq!(reports[1][1], 0x00);
    assert_eq!(reports[1][2], 0x00);
    assert_eq!(reports[1][3], 0x00);

    // Check FFB mode
    assert_eq!(reports[2][0], report_ids::FFB_MODE);
    assert_eq!(reports[2][1], 0x00);
    assert_eq!(reports[2][2], 0x00);
    assert_eq!(reports[2][3], 0x00);

    Ok(())
}

#[test]
fn test_moza_initialize_device_high_torque_off_by_default() -> Result<(), Box<dyn std::error::Error>>
{
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    assert!(
        !protocol.is_high_torque_enabled(),
        "high torque must be off by default"
    );

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert_eq!(
        reports.len(),
        2,
        "only start_reports + ffb_mode sent without high torque"
    );

    // First report must be START_REPORTS, not HIGH_TORQUE
    assert_eq!(reports[0][0], super::moza::report_ids::START_REPORTS);
    assert_eq!(reports[1][0], report_ids::FFB_MODE);

    Ok(())
}

#[test]
fn test_moza_initialize_device_respects_configured_mode() -> Result<(), Box<dyn std::error::Error>>
{
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Direct, true);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[2][0], report_ids::FFB_MODE);
    assert_eq!(reports[2][1], FfbMode::Direct as u8);
    assert_eq!(reports[2][2], 0x00);
    assert_eq!(reports[2][3], 0x00);

    Ok(())
}

#[test]
fn test_moza_initialize_device_idempotent_and_stateful() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002); // R9, high_torque=false by default
    let mut writer = MockDeviceWriter::new();

    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);

    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);
    let sent = writer.get_feature_reports().len();
    assert_eq!(sent, 2); // start_reports + ffb_mode (no high_torque)

    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);
    assert_eq!(writer.get_feature_reports().len(), sent);

    Ok(())
}

#[test]
fn test_moza_pedals_skip_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0003); // SR-P Pedals
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert!(reports.is_empty()); // No reports sent for pedals

    Ok(())
}

#[test]
fn test_moza_peripheral_skip_initialization_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0003); // SR-P Pedals
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert!(protocol.output_report_id().is_none());
    assert!(protocol.output_report_len().is_none());

    Ok(())
}

#[test]
fn test_moza_handbrake_skip_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert!(reports.is_empty());

    Ok(())
}

#[test]
fn test_moza_initialization_records_failure_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002); // R9
    let mut writer = MockDeviceWriter::with_failure();

    let result = protocol.initialize_device(&mut writer);
    assert!(result.is_err());
    assert_eq!(protocol.init_state(), MozaInitState::Failed);

    Ok(())
}

#[test]
fn test_moza_output_report_metadata() {
    let protocol = MozaProtocol::new(0x0004); // R5 V1
    assert_eq!(protocol.output_report_id(), Some(report_ids::DIRECT_TORQUE));
    assert_eq!(protocol.output_report_len(), Some(REPORT_LEN));

    let protocol_peripheral = MozaProtocol::new(0x0003); // SR-P Pedals
    assert!(protocol_peripheral.output_report_id().is_none());
    assert!(protocol_peripheral.output_report_len().is_none());
}

#[test]
fn test_moza_set_rotation_range() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    protocol.set_rotation_range(&mut writer, 900)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], super::moza::report_ids::ROTATION_RANGE);
    assert_eq!(reports[0][1], 0x01); // Set Range command

    // Check degrees (900 in little-endian)
    let degrees_bytes = 900u16.to_le_bytes();
    assert_eq!(reports[0][2], degrees_bytes[0]);
    assert_eq!(reports[0][3], degrees_bytes[1]);

    Ok(())
}

#[test]
fn test_moza_set_ffb_mode() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    protocol.set_ffb_mode(&mut writer, FfbMode::Direct)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], super::moza::report_ids::FFB_MODE);
    assert_eq!(reports[0][1], FfbMode::Direct as u8);
    assert_eq!(reports[0][2], 0x00);
    assert_eq!(reports[0][3], 0x00);

    Ok(())
}

#[test]
fn test_get_vendor_protocol_moza() {
    let protocol = get_vendor_protocol(0x346E, 0x0002);
    assert!(protocol.is_some());

    let proto = protocol.as_ref();
    assert!(proto.is_some());
    let p = proto.map(|p| p.as_ref());
    assert!(p.is_some());
}

#[test]
fn test_get_vendor_protocol_unknown() {
    let protocol = get_vendor_protocol(0x1234, 0x5678);
    assert!(protocol.is_none());
}

#[test]
fn test_ffb_config_default() {
    let config = FfbConfig::default();

    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, None);
    assert!((config.max_torque_nm - 10.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 4096);
}

#[test]
fn test_moza_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    let data = [0x01, 0x02, 0x03];
    protocol.send_feature_report(&mut writer, 0xAB, &data)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0xAB, 0x01, 0x02, 0x03]);

    Ok(())
}

#[test]
fn test_v1_vs_v2_detection() {
    // V1 PIDs (0x000x)
    assert!(!MozaProtocol::new(0x0000).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0002).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0004).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0005).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0006).is_v2_hardware());

    // V2 PIDs (0x001x)
    assert!(MozaProtocol::new(0x0010).is_v2_hardware());
    assert!(MozaProtocol::new(0x0012).is_v2_hardware());
    assert!(MozaProtocol::new(0x0014).is_v2_hardware());
    assert!(MozaProtocol::new(0x0015).is_v2_hardware());
    assert!(MozaProtocol::new(0x0016).is_v2_hardware());
}

// ─── PR3: Retry state machine tests ─────────────────────────────────────────

#[test]
fn test_moza_retry_policy_delay_capped() {
    let policy = MozaRetryPolicy {
        max_retries: 3,
        base_delay_ms: 500,
    };
    assert_eq!(policy.delay_ms_for(0), 500);
    assert_eq!(policy.delay_ms_for(1), 1000);
    assert_eq!(policy.delay_ms_for(2), 2000);
    assert_eq!(policy.delay_ms_for(3), 4000);
    // Capped at 8x: attempts beyond 3 stay at 4000ms
    assert_eq!(policy.delay_ms_for(4), 4000);
    assert_eq!(policy.delay_ms_for(100), 4000);
}

#[test]
fn test_moza_reset_to_uninitialized_clears_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Standard, true);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);

    protocol.reset_to_uninitialized();
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert_eq!(protocol.retry_count(), 0);

    // Can re-initialize after reset
    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::Ready);

    Ok(())
}

#[test]
fn test_moza_retries_bounded_by_max_retries() -> Result<(), Box<dyn std::error::Error>> {
    // With DEFAULT_MAX_RETRIES (3), after 3 failures → PermanentFailure
    let protocol = MozaProtocol::new(0x0002);

    for expected_state in [
        MozaInitState::Failed,           // retry_count = 1
        MozaInitState::Failed,           // retry_count = 2
        MozaInitState::PermanentFailure, // retry_count = 3 >= max_retries
    ] {
        let mut writer = MockDeviceWriter::with_failure();
        let result = protocol.initialize_device(&mut writer);
        assert!(result.is_err());
        assert_eq!(
            protocol.init_state(),
            expected_state,
            "expected {:?} after retry_count={}",
            expected_state,
            protocol.retry_count()
        );
    }

    // Once PermanentFailure, further calls are no-ops
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    assert_eq!(protocol.init_state(), MozaInitState::PermanentFailure);
    assert!(writer.get_feature_reports().is_empty());

    // Reset clears PermanentFailure
    protocol.reset_to_uninitialized();
    assert_eq!(protocol.init_state(), MozaInitState::Uninitialized);
    assert_eq!(protocol.retry_count(), 0);

    Ok(())
}

#[test]
fn test_moza_is_ffb_ready() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Standard, false);
    assert!(!protocol.is_ffb_ready(), "not ready before handshake");

    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    assert!(protocol.is_ffb_ready(), "ready after successful handshake");

    protocol.reset_to_uninitialized();
    assert!(!protocol.is_ffb_ready(), "not ready after reset");

    Ok(())
}

#[test]
fn test_moza_can_retry_state() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    assert!(!protocol.can_retry(), "cannot retry from Uninitialized");

    let mut writer = MockDeviceWriter::with_failure();
    let result = protocol.initialize_device(&mut writer);
    assert!(result.is_err());
    assert_eq!(protocol.init_state(), MozaInitState::Failed);
    assert!(protocol.can_retry(), "can retry after first failure");

    Ok(())
}

// ─── Policy / allowlist tests ────────────────────────────────────────────────

#[test]
fn test_signature_is_trusted_none_crc32_is_not_trusted() {
    // Without OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE set, a None CRC is untrusted.
    assert!(
        !super::moza::signature_is_trusted(None),
        "None CRC32 must not be trusted by default"
    );
}

#[test]
fn test_effective_ffb_mode_direct_without_trust_downgrades() {
    // Direct mode with untrusted signature (None CRC, no env override) → Standard.
    let effective = super::moza::effective_ffb_mode(FfbMode::Direct, None);
    assert_eq!(
        effective,
        FfbMode::Standard,
        "Direct mode must downgrade to Standard when signature is untrusted"
    );
}

#[test]
fn test_effective_ffb_mode_standard_passes_through() {
    // Standard mode is always allowed regardless of trust.
    let effective = super::moza::effective_ffb_mode(FfbMode::Standard, None);
    assert_eq!(effective, FfbMode::Standard);
}

#[test]
fn test_effective_high_torque_opt_in_false_without_env() {
    // Without any env override, high torque opt-in is always false.
    let opt_in = super::moza::effective_high_torque_opt_in(None);
    assert!(!opt_in, "high torque must not opt-in without env var");
}

#[test]
fn test_policy_new_with_config_high_torque_true_sends_three_reports()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Standard, true);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 3, "high_torque=true → 3 reports");
    assert_eq!(reports[0][0], report_ids::HIGH_TORQUE);
    assert_eq!(reports[1][0], report_ids::START_REPORTS);
    assert_eq!(reports[2][0], report_ids::FFB_MODE);
    Ok(())
}

#[test]
fn test_policy_new_with_config_high_torque_false_sends_two_reports()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(0x0002, FfbMode::Standard, false);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 2, "high_torque=false → 2 reports");
    assert_eq!(reports[0][0], report_ids::START_REPORTS);
    assert_eq!(reports[1][0], report_ids::FFB_MODE);
    Ok(())
}
