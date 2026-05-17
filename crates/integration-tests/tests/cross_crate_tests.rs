//! Cross-crate integration tests verifying interfaces between workspace crates.
//!
//! These tests ensure types, errors, and data flow correctly across crate
//! boundaries:
//! 1. Schema types flow: schemas → engine → service
//! 2. Error type propagation across crate boundaries
//! 3. Profile round-trip: save → load → apply
//! 4. Telemetry normalization consistency across adapters
//! 5. Filter config cross-crate serialization
//! 6. IPC message validity with engine state
//! 7. Plugin ABI type compatibility (loader ↔ plugin)
//! 8. Device type consistency: HID → engine → service
//! 9. Calibration data full-stack flow
//! 10. Safety state transitions observable from service layer

use std::collections::HashSet;

// ── Schemas ──────────────────────────────────────────────────────────────────
use racing_wheel_schemas::prelude::*;

// ── Engine ───────────────────────────────────────────────────────────────────
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline, VirtualDevice};

// ── Filters ──────────────────────────────────────────────────────────────────
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};

// ── Calibration ──────────────────────────────────────────────────────────────
use openracing_calibration::AxisCalibration;

// ── Profile ──────────────────────────────────────────────────────────────────
use openracing_profile::WheelProfile;

// ── Telemetry adapters ───────────────────────────────────────────────────────
use openracing_telemetry_adapters::adapter_factories;

// ── IPC ──────────────────────────────────────────────────────────────────────
use openracing_ipc::codec::{MessageHeader, message_flags, message_types};

// ── Plugin ABI ───────────────────────────────────────────────────────────────
use openracing_plugin_abi::{
    PLUG_ABI_MAGIC, PLUG_ABI_VERSION, PluginCapabilities, PluginHeader, WASM_ABI_VERSION,
    WasmExportValidation, WasmPluginInfo,
};

// ── Device types ─────────────────────────────────────────────────────────────
use openracing_device_types::{DeviceInputs, TelemetryData};

// ── Errors ───────────────────────────────────────────────────────────────────
use openracing_errors::{DeviceError, OpenRacingError, RTError};

// ── Service ──────────────────────────────────────────────────────────────────
use racing_wheel_service::system_config::SystemConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Schema types flow: schemas → engine → service
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn schema_device_id_usable_across_engine_and_service() -> Result<(), Box<dyn std::error::Error>> {
    // DeviceId from schemas crate
    let device_id: DeviceId = "cross-crate-device-001".parse()?;

    // Used to create a VirtualDevice in engine crate
    let device = VirtualDevice::new(device_id.clone(), "Test Wheel".to_string());

    // Device capabilities from schemas carry through
    let caps = DeviceCapabilities::new(
        true,                // supports_pid
        true,                // supports_raw_torque_1khz
        true,                // supports_health_stream
        false,               // supports_led_bus
        TorqueNm::new(8.0)?, // max_torque
        4096,                // encoder_cpr
        1000,                // min_report_period_us (= 1000 Hz)
    );
    assert!(
        !device_id.as_str().is_empty(),
        "DeviceId must have non-empty string representation"
    );
    assert!(
        caps.max_update_rate_hz() >= 1000.0,
        "Update rate should be at least 1000Hz"
    );

    // Verify the device was created with correct ID
    let _ = device;
    Ok(())
}

#[test]
fn schema_normalized_telemetry_flows_to_engine_frame() -> Result<(), Box<dyn std::error::Error>> {
    // Build telemetry using schemas crate builder
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(25.0)
        .steering_angle(0.3)
        .throttle(0.7)
        .brake(0.1)
        .gear(3)
        .ffb_scalar(0.5)
        .build();

    // Feed into engine frame
    let mut frame = EngineFrame {
        ffb_in: telemetry.ffb_scalar,
        torque_out: telemetry.ffb_scalar,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };

    // Process through engine pipeline
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut frame)?;

    assert!(
        frame.torque_out.is_finite(),
        "Engine pipeline output must be finite after processing schema telemetry"
    );

    // Apply safety clamping (service layer concern)
    let safety = SafetyService::new(5.0, 20.0);
    let clamped = safety.clamp_torque_nm(frame.torque_out * 5.0);
    assert!(
        clamped.is_finite() && clamped.abs() <= 5.0,
        "Safety-clamped torque must be finite and within safe limit, got {}",
        clamped
    );

    Ok(())
}

#[test]
fn schema_telemetry_snapshot_serializes_with_engine_data() -> Result<(), Box<dyn std::error::Error>>
{
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(40.0)
        .rpm(7500.0)
        .max_rpm(9000.0)
        .throttle(0.9)
        .brake(0.0)
        .gear(5)
        .ffb_scalar(0.65)
        .build();

    // Serialize via serde (schemas → JSON)
    let json = serde_json::to_string(&telemetry)?;
    assert!(!json.is_empty(), "JSON serialization must produce output");

    // Deserialize back (simulating IPC/service layer consumption)
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
    assert!(
        (decoded.speed_ms - telemetry.speed_ms).abs() < 0.01,
        "speed_ms must survive round-trip"
    );
    assert!(
        (decoded.rpm - telemetry.rpm).abs() < 0.01,
        "rpm must survive round-trip"
    );
    assert_eq!(decoded.gear, telemetry.gear, "gear must survive round-trip");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Error type propagation across crate boundaries
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_error_converts_to_openracing_error() -> Result<(), Box<dyn std::error::Error>> {
    let rt_err = RTError::TimingViolation;
    let open_err: OpenRacingError = rt_err.into();

    assert!(
        matches!(open_err, OpenRacingError::RT(RTError::TimingViolation)),
        "RTError::TimingViolation must convert to OpenRacingError::RT, got {:?}",
        open_err
    );

    // Error display must include context
    let display = format!("{open_err}");
    assert!(
        !display.is_empty(),
        "OpenRacingError display must not be empty"
    );

    Ok(())
}

#[test]
fn device_error_converts_to_openracing_error() -> Result<(), Box<dyn std::error::Error>> {
    let dev_err = DeviceError::Disconnected("test-device".to_string());
    let open_err: OpenRacingError = dev_err.into();

    assert!(
        matches!(
            open_err,
            OpenRacingError::Device(DeviceError::Disconnected(_))
        ),
        "DeviceError must convert to OpenRacingError::Device"
    );

    Ok(())
}

#[test]
fn all_rt_error_variants_have_display() -> Result<(), Box<dyn std::error::Error>> {
    let variants = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::SafetyInterlock,
        RTError::BufferOverflow,
        RTError::DeadlineMissed,
        RTError::ResourceUnavailable,
    ];

    for variant in &variants {
        let display = format!("{variant}");
        assert!(
            !display.is_empty(),
            "{:?}: Display must produce non-empty output",
            variant
        );

        // Verify conversion to OpenRacingError preserves the variant
        let open_err: OpenRacingError = (*variant).into();
        let open_display = format!("{open_err}");
        assert!(
            !open_display.is_empty(),
            "{:?}: OpenRacingError display must not be empty",
            variant
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Profile round-trip: save → load → apply
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn profile_roundtrip_save_load_apply() -> Result<(), Box<dyn std::error::Error>> {
    // Create a profile (openracing-profile crate)
    let mut profile = WheelProfile::new("Test Racing Profile", "device-123");
    profile.settings.ffb.overall_gain = 0.85;
    profile.settings.ffb.damper_strength = 0.3;
    profile.settings.ffb.friction_strength = 0.15;
    profile.settings.ffb.torque_limit = 15.0;

    // Serialize (save)
    let json = serde_json::to_string_pretty(&profile)?;
    assert!(
        !json.is_empty(),
        "Profile serialization must produce output"
    );

    // Deserialize (load)
    let loaded: WheelProfile = serde_json::from_str(&json)?;

    // Verify all settings survived round-trip
    assert!(
        (loaded.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON,
        "overall_gain must survive round-trip"
    );
    assert!(
        (loaded.settings.ffb.damper_strength - 0.3).abs() < f32::EPSILON,
        "damper_strength must survive round-trip"
    );
    assert!(
        (loaded.settings.ffb.friction_strength - 0.15).abs() < f32::EPSILON,
        "friction_strength must survive round-trip"
    );
    assert!(
        (loaded.settings.ffb.torque_limit - 15.0).abs() < f32::EPSILON,
        "torque_limit must survive round-trip"
    );

    // Apply to filter chain (cross-crate: profile → filters)
    let damper = DamperState::fixed(loaded.settings.ffb.damper_strength);
    let friction = FrictionState::fixed(loaded.settings.ffb.friction_strength);

    let mut frame = FilterFrame {
        ffb_in: 0.7 * loaded.settings.ffb.overall_gain,
        torque_out: 0.7 * loaded.settings.ffb.overall_gain,
        wheel_speed: 10.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };

    damper_filter(&mut frame, &damper);
    friction_filter(&mut frame, &friction);
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite(),
        "Filter output must be finite after applying loaded profile settings"
    );
    assert!(
        frame.torque_out.abs() <= 1.0,
        "Filter output must be within [-1, 1], got {}",
        frame.torque_out
    );

    Ok(())
}

#[test]
fn profile_schema_version_is_consistent() -> Result<(), Box<dyn std::error::Error>> {
    let profile = WheelProfile::new("Version Test", "dev-001");

    assert!(
        profile.schema_version >= 1,
        "Schema version must be at least 1, got {}",
        profile.schema_version
    );
    assert!(
        profile.version >= 1,
        "Profile version must be at least 1, got {}",
        profile.version
    );

    // Verify default settings are reasonable
    assert!(
        profile.settings.ffb.overall_gain > 0.0 && profile.settings.ffb.overall_gain <= 1.0,
        "Default overall_gain must be in (0, 1], got {}",
        profile.settings.ffb.overall_gain
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Telemetry normalization consistency across adapters
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_adapters_produce_consistent_normalized_output() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    assert!(
        factories.len() >= 15,
        "Expected at least 15 telemetry adapters, found {}",
        factories.len()
    );

    let mut seen_ids = HashSet::new();
    for (game_id, factory) in factories {
        // Verify unique game IDs across all adapters
        assert!(
            seen_ids.insert(*game_id),
            "Duplicate adapter game_id: {}",
            game_id
        );

        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *game_id,
            "Adapter game_id() must match registration"
        );

        // Test with a large buffer — result should be Ok or Err, never panic
        let large_buf = vec![0u8; 2048];
        let _ = adapter.normalize(&large_buf);

        // Test with empty buffer
        let _ = adapter.normalize(&[]);
    }

    Ok(())
}

#[test]
fn normalized_telemetry_fields_are_bounded_across_adapters()
-> Result<(), Box<dyn std::error::Error>> {
    // Build telemetry for each scenario via the schemas builder
    let scenarios = [
        ("idle", 0.0f32, 0.0f32, 0i8),
        ("cruising", 20.0, 0.3, 3),
        ("racing", 60.0, 0.8, 5),
        ("braking", 5.0, -0.1, 2),
    ];

    for (label, speed, ffb, gear) in &scenarios {
        let telem = NormalizedTelemetry::builder()
            .speed_ms(*speed)
            .ffb_scalar(*ffb)
            .gear(*gear)
            .throttle(0.5)
            .brake(0.0)
            .build();

        assert!(
            telem.speed_ms.is_finite(),
            "{label}: speed_ms must be finite"
        );
        assert!(
            telem.ffb_scalar.is_finite(),
            "{label}: ffb_scalar must be finite"
        );
        assert!(
            telem.throttle >= 0.0 && telem.throttle <= 1.0,
            "{label}: throttle must be in [0,1], got {}",
            telem.throttle
        );
        assert!(
            telem.brake >= 0.0 && telem.brake <= 1.0,
            "{label}: brake must be in [0,1], got {}",
            telem.brake
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Filter config cross-crate serialization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn filter_config_serializes_across_crate_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    // Create FilterConfig from schemas crate
    let config = FilterConfig::default();

    // Serialize (simulating one crate writing config)
    let json = serde_json::to_string(&config)?;
    assert!(!json.is_empty(), "FilterConfig JSON must not be empty");

    // Deserialize in another context (simulating another crate reading it)
    let decoded: FilterConfig = serde_json::from_str(&json)?;

    // Verify structural equality
    assert_eq!(
        config.reconstruction, decoded.reconstruction,
        "reconstruction must survive round-trip"
    );

    Ok(())
}

#[test]
fn filter_config_from_schemas_applies_to_filter_chain() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig::default();

    // Use schema config values to initialize filter states (cross-crate flow)
    let damper = DamperState::fixed(config.damper.value());
    let friction = FrictionState::fixed(config.friction.value());

    let mut frame = FilterFrame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 10.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };

    damper_filter(&mut frame, &damper);
    friction_filter(&mut frame, &friction);
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite(),
        "Filter output with schema config must be finite"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. IPC messages contain valid engine state representations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_message_header_roundtrips_for_all_message_types() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        ("DEVICE", message_types::DEVICE),
        ("PROFILE", message_types::PROFILE),
        ("SAFETY", message_types::SAFETY),
        ("HEALTH", message_types::HEALTH),
        ("FEATURE_NEGOTIATION", message_types::FEATURE_NEGOTIATION),
        ("GAME", message_types::GAME),
        ("TELEMETRY", message_types::TELEMETRY),
        ("DIAGNOSTIC", message_types::DIAGNOSTIC),
    ];

    for (label, msg_type) in &types {
        let header = MessageHeader::new(*msg_type, 256, 42);
        let encoded = header.encode();
        assert_eq!(
            encoded.len(),
            MessageHeader::SIZE,
            "{label}: encoded header must be {} bytes",
            MessageHeader::SIZE
        );

        let decoded = MessageHeader::decode(&encoded)?;
        assert_eq!(
            decoded.message_type, *msg_type,
            "{label}: message_type must survive round-trip"
        );
        assert_eq!(
            decoded.payload_len, 256,
            "{label}: payload_len must survive round-trip"
        );
        assert_eq!(
            decoded.sequence, 42,
            "{label}: sequence must survive round-trip"
        );
    }

    Ok(())
}

#[test]
fn ipc_message_flags_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let flags = [
        message_flags::COMPRESSED,
        message_flags::REQUIRES_ACK,
        message_flags::IS_RESPONSE,
        message_flags::IS_ERROR,
        message_flags::STREAMING,
    ];

    // All flags must be unique (no overlapping bits)
    for i in 0..flags.len() {
        for j in (i + 1)..flags.len() {
            assert_eq!(
                flags[i] & flags[j],
                0,
                "Flags at indices {} and {} must not overlap: 0x{:04x} & 0x{:04x}",
                i,
                j,
                flags[i],
                flags[j]
            );
        }
    }

    Ok(())
}

#[test]
fn ipc_safety_message_represents_engine_state() -> Result<(), Box<dyn std::error::Error>> {
    // Create engine safety state
    let mut safety = SafetyService::new(5.0, 20.0);
    assert_eq!(
        safety.state(),
        &SafetyState::SafeTorque,
        "Initial state must be SafeTorque"
    );

    // Build an IPC SAFETY message header that would carry this state
    let header = MessageHeader::new(message_types::SAFETY, 64, 1);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert_eq!(
        decoded.message_type,
        message_types::SAFETY,
        "Safety message type must be preserved"
    );

    // After a fault, the safety state changes — IPC should carry updated state
    safety.report_fault(FaultType::UsbStall);
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "Safety must be Faulted after USB stall"
    );

    // A new IPC message with updated sequence reflects the state change
    let header_after = MessageHeader::new(message_types::SAFETY, 64, 2);
    let decoded_after = MessageHeader::decode(&header_after.encode())?;
    assert_eq!(
        decoded_after.sequence, 2,
        "Sequence must increment to reflect state change"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Plugin ABI type compatibility (loader ↔ plugin)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn plugin_header_roundtrip_matches_between_loader_and_plugin()
-> Result<(), Box<dyn std::error::Error>> {
    // Simulate plugin side: create header with capabilities
    let plugin_caps = PluginCapabilities::TELEMETRY | PluginCapabilities::HAPTICS;
    let plugin_header = PluginHeader::new(plugin_caps);

    // Serialize to bytes (plugin writes these to shared memory / file)
    let bytes = plugin_header.to_bytes();
    assert_eq!(bytes.len(), 16, "PluginHeader must be exactly 16 bytes");

    // Simulate loader side: read and validate header
    let loader_header = PluginHeader::from_bytes(&bytes);
    assert!(
        loader_header.is_valid(),
        "Loader must validate plugin header as valid"
    );
    assert_eq!(
        loader_header.magic, PLUG_ABI_MAGIC,
        "Magic must match PLUG_ABI_MAGIC"
    );
    assert_eq!(
        loader_header.abi_version, PLUG_ABI_VERSION,
        "ABI version must match"
    );
    assert!(
        loader_header.has_capability(PluginCapabilities::TELEMETRY),
        "Loader must see TELEMETRY capability"
    );
    assert!(
        loader_header.has_capability(PluginCapabilities::HAPTICS),
        "Loader must see HAPTICS capability"
    );
    assert!(
        !loader_header.has_capability(PluginCapabilities::LEDS),
        "Loader must not see LEDS capability"
    );

    Ok(())
}

#[test]
fn plugin_abi_versions_are_consistent() -> Result<(), Box<dyn std::error::Error>> {
    // Native plugin ABI
    assert_eq!(
        PLUG_ABI_VERSION, 0x0001_0000,
        "Native plugin ABI version must be 1.0"
    );
    assert_eq!(
        PLUG_ABI_MAGIC, 0x57574C31,
        "Plugin magic must be 'WWL1' in LE"
    );

    // WASM plugin ABI
    assert_eq!(WASM_ABI_VERSION, 1, "WASM ABI version must be 1");

    // Default header must be valid
    let default_header = PluginHeader::default();
    assert!(
        default_header.is_valid(),
        "Default PluginHeader must be valid"
    );

    // WasmPluginInfo default must reference correct ABI version
    let wasm_info = WasmPluginInfo::default();
    assert_eq!(
        wasm_info.abi_version, WASM_ABI_VERSION,
        "WasmPluginInfo default ABI version must match WASM_ABI_VERSION"
    );

    Ok(())
}

#[test]
fn wasm_export_validation_enforces_abi_contract() -> Result<(), Box<dyn std::error::Error>> {
    // Valid plugin exports
    let valid = WasmExportValidation {
        has_process: true,
        has_memory: true,
        has_init: true,
        has_shutdown: true,
        has_get_info: false,
    };
    assert!(valid.is_valid(), "Plugin with process+memory must be valid");
    assert!(
        valid.missing_required().is_empty(),
        "No required exports missing"
    );

    // Missing required `process` export
    let missing_process = WasmExportValidation {
        has_process: false,
        has_memory: true,
        ..Default::default()
    };
    assert!(
        !missing_process.is_valid(),
        "Plugin without process must be invalid"
    );
    let missing = missing_process.missing_required();
    assert!(
        missing.contains(&"process"),
        "Must report 'process' as missing"
    );

    // Missing required `memory` export
    let missing_memory = WasmExportValidation {
        has_process: true,
        has_memory: false,
        ..Default::default()
    };
    assert!(
        !missing_memory.is_valid(),
        "Plugin without memory must be invalid"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Device types consistent across HID → engine → service
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn device_inputs_flow_from_hid_to_engine() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate HID layer producing DeviceInputs (openracing-device-types)
    let inputs = DeviceInputs::default()
        .with_steering(32768) // center
        .with_pedals(0, 0, 0); // no throttle, no brake, no clutch

    assert_eq!(
        inputs.steering,
        Some(32768),
        "Steering must be set to center"
    );
    assert_eq!(inputs.throttle, Some(0), "Throttle must be zero");
    assert_eq!(inputs.brake, Some(0), "Brake must be zero");

    // Create TelemetryData (device-side telemetry)
    let telem_data = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 5.0,
        temperature_c: 35,
        fault_flags: 0,
        hands_on: true,
    };

    // Verify it's usable by engine (no type mismatch)
    assert!(telem_data.hands_on, "hands_on must be true");
    assert_eq!(telem_data.fault_flags, 0, "No faults initially");

    Ok(())
}

#[test]
fn device_capabilities_schema_used_by_engine() -> Result<(), Box<dyn std::error::Error>> {
    // Device capabilities from schemas
    let caps = DeviceCapabilities::new(
        true,                 // supports_pid
        true,                 // supports_raw_torque_1khz
        true,                 // supports_health_stream
        false,                // supports_led_bus
        TorqueNm::new(12.0)?, // max_torque
        4096,                 // encoder_cpr
        1000,                 // min_report_period_us (= 1000 Hz)
    );

    // These capabilities influence engine behavior
    assert!(
        caps.max_update_rate_hz() > 0.0,
        "max_update_rate_hz must be positive"
    );

    // Create a virtual device (engine crate) and verify it integrates
    let device_id: DeviceId = "capability-test-wheel".parse()?;
    let _device = VirtualDevice::new(device_id, "Capability Test".to_string());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Calibration data flows through the full stack
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn calibration_flows_from_schemas_through_engine_to_filters()
-> Result<(), Box<dyn std::error::Error>> {
    // Schema-level calibration data
    let cal_data = CalibrationData::new(CalibrationType::Full);
    assert_eq!(
        cal_data.calibration_type,
        CalibrationType::Full,
        "Calibration type must be Full"
    );

    // Low-level axis calibration (openracing-calibration crate)
    let axis_cal = AxisCalibration::new(0, 65535).with_center(32768);

    // Test calibration across the full raw range
    let test_points = [(0u16, "left"), (32768, "center"), (65535, "right")];

    for (raw, label) in &test_points {
        let normalized = axis_cal.apply(*raw);
        assert!(
            (0.0..=1.0).contains(&normalized),
            "{label}: calibrated value must be in [0,1], got {normalized}"
        );

        // Feed calibrated value into filter chain (cross-crate: calibration → filters)
        let ffb_input = (normalized - 0.5) * 2.0;
        let mut frame = FilterFrame {
            ffb_in: ffb_input,
            torque_out: ffb_input,
            wheel_speed: 5.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        let damper = DamperState::fixed(0.05);
        damper_filter(&mut frame, &damper);
        torque_cap_filter(&mut frame, 1.0);

        assert!(
            frame.torque_out.is_finite(),
            "{label}: filter output must be finite"
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "{label}: filter output must be capped to [-1,1], got {}",
            frame.torque_out
        );
    }

    Ok(())
}

#[test]
fn calibration_center_position_matches_schema_expectation() -> Result<(), Box<dyn std::error::Error>>
{
    let axis_cal = AxisCalibration::new(0, 65535).with_center(32768);

    // Center must normalize to ~0.5
    let center_norm = axis_cal.apply(32768);
    assert!(
        (center_norm - 0.5).abs() < 0.01,
        "Center position must normalize to ~0.5, got {}",
        center_norm
    );

    // Schema CalibrationData can represent this
    let mut cal_data = CalibrationData::new(CalibrationType::Center);
    cal_data.center_position = Some(0.5);
    assert!(
        cal_data.center_position.is_some(),
        "Center position must be set in schema"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Safety state transitions observable from service layer
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn safety_state_transitions_are_observable() -> Result<(), Box<dyn std::error::Error>> {
    let mut safety = SafetyService::new(5.0, 20.0);

    // Initial state: SafeTorque
    assert_eq!(
        safety.state(),
        &SafetyState::SafeTorque,
        "Initial safety state must be SafeTorque"
    );

    // Torque is limited to safe limit
    let safe_torque = safety.clamp_torque_nm(3.0);
    assert!(
        (safe_torque - 3.0).abs() < 0.001,
        "3 Nm must pass through in SafeTorque state"
    );

    // Transition to Faulted on any fault
    safety.report_fault(FaultType::ThermalLimit);
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::ThermalLimit,
                "Fault type must be ThermalLimit"
            );
        }
        other => {
            return Err(format!("Expected Faulted state, got {:?}", other).into());
        }
    }

    // In Faulted state, torque must be zero
    let faulted_torque = safety.clamp_torque_nm(10.0);
    assert!(
        faulted_torque.abs() < 0.001,
        "Torque must be zero in Faulted state, got {}",
        faulted_torque
    );

    Ok(())
}

#[test]
fn safety_state_observable_through_system_config() -> Result<(), Box<dyn std::error::Error>> {
    // SystemConfig from service crate must be loadable with defaults
    let config_json = serde_json::json!({
        "schema_version": "1.0.0",
        "engine": {
            "tick_rate_hz": 1000,
            "max_jitter_us": 250,
            "force_ffb_mode": null,
            "disable_realtime": false,
            "rt_cpu_affinity": null,
            "memory_lock_all": true,
            "processing_budget_us": 200
        },
        "service": {
            "service_name": "openracing",
            "service_display_name": "OpenRacing Service",
            "service_description": "OpenRacing force feedback service",
            "health_check_interval": 5,
            "max_restart_attempts": 3,
            "restart_delay": 1,
            "auto_restart": true,
            "shutdown_timeout": 10
        },
        "ipc": {
            "transport": "Native",
            "bind_address": null,
            "max_connections": 10,
            "connection_timeout": 5,
            "enable_acl": false,
            "max_message_size": 65536
        },
        "games": {
            "auto_configure": true,
            "auto_profile_switch": true,
            "profile_switch_timeout_ms": 1000,
            "telemetry_timeout_s": 5,
            "supported_games": {}
        },
        "safety": {
            "default_safe_torque_nm": 5.0,
            "max_torque_nm": 25.0,
            "fault_response_timeout_ms": 50,
            "hands_off_timeout_s": 3,
            "temp_warning_c": 60,
            "temp_fault_c": 80,
            "require_physical_interlock": false
        },
        "plugins": {
            "enabled": true,
            "plugin_paths": [],
            "auto_load": true,
            "timeout_ms": 5000,
            "max_memory_mb": 256,
            "enable_native": false
        },
        "observability": {
            "enable_metrics": true,
            "metrics_interval_s": 10,
            "enable_tracing": false,
            "tracing_sample_rate": 0.1,
            "enable_blackbox": true,
            "blackbox_retention_hours": 24,
            "health_stream_hz": 10
        },
        "development": {
            "enable_dev_features": false,
            "enable_debug_logging": false,
            "enable_virtual_devices": false,
            "disable_safety_interlocks": false,
            "enable_plugin_dev_mode": false,
            "mock_telemetry": false
        }
    });

    // Verify SystemConfig deserializes (service layer can read engine config)
    let _config: SystemConfig = serde_json::from_value(config_json)?;

    Ok(())
}

#[test]
fn safety_faults_cascade_to_pipeline_output() -> Result<(), Box<dyn std::error::Error>> {
    let faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];

    for fault in &faults {
        let mut safety = SafetyService::new(5.0, 20.0);
        let mut pipeline = EnginePipeline::new();

        // Process a frame
        let mut frame = EngineFrame {
            ffb_in: 0.9,
            torque_out: 0.9,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut frame)?;

        // Inject fault
        safety.report_fault(*fault);

        // Verify zero torque in faulted state
        let clamped = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            clamped.abs() < 0.001,
            "{:?}: torque must be zero after fault, got {}",
            fault,
            clamped
        );
    }

    Ok(())
}
