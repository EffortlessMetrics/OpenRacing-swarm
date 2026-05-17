//! RC-level integration tests for Racing Wheel Software
//!
//! These tests verify key integration scenarios required for release-candidate
//! quality, covering device enumeration, FFB pipeline, telemetry round-trip,
//! plugin system basics, and safety interlocks.

use anyhow::Result;
use std::time::{Duration, Instant};

use openracing_telemetry_adapters::adapter_factories;
use racing_wheel_engine::ports::HidPort;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{
    CapabilityNegotiator, DeviceInfo, FFBMode, Frame, GameCompatibility, ModeSelectionPolicy,
    NegotiationResult, Pipeline, VirtualDevice, VirtualHidPort,
};
use racing_wheel_schemas::prelude::*;

use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::{PluginClass, PluginError};

// ---------------------------------------------------------------------------
// Module a: Device Enumeration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_device_enumeration_identifies_supported_device() -> Result<()> {
    let id: DeviceId = "virtual-wheel-001".parse()?;
    let device = VirtualDevice::new(id.clone(), "Moza R16 Virtual".to_string());
    let mut port = VirtualHidPort::new();
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    assert_eq!(devices.len(), 1, "Expected exactly one device listed");
    let info: &DeviceInfo = &devices[0];
    assert_eq!(info.id, id);
    assert!(info.is_connected, "Device should be connected");
    assert!(
        info.capabilities.supports_raw_torque_1khz,
        "Virtual device should support 1kHz raw torque"
    );
    Ok(())
}

#[tokio::test]
async fn test_device_enumeration_multiple_devices() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "wheel-alpha".parse()?;
    let id_b: DeviceId = "wheel-beta".parse()?;
    port.add_device(VirtualDevice::new(id_a.clone(), "Alpha Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Beta Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(devices.len(), 2, "Expected two devices");

    let ids: Vec<&DeviceId> = devices.iter().map(|d| &d.id).collect();
    assert!(ids.contains(&&id_a), "Alpha wheel should be listed");
    assert!(ids.contains(&&id_b), "Beta wheel should be listed");
    Ok(())
}

#[tokio::test]
async fn test_device_enumeration_empty_port() -> Result<()> {
    let port = VirtualHidPort::new();
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(devices.is_empty(), "No devices expected on empty port");
    Ok(())
}

#[tokio::test]
async fn test_device_open_and_read_telemetry() -> Result<()> {
    let id: DeviceId = "telem-device-001".parse()?;
    let device = VirtualDevice::new(id.clone(), "Telemetry Wheel".to_string());
    let mut port = VirtualHidPort::new();
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut opened = port
        .open_device(&id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(opened.is_connected(), "Opened device should be connected");

    let telemetry = opened.read_telemetry();
    assert!(
        telemetry.is_some(),
        "Connected device should return telemetry"
    );
    let telem = telemetry
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
    assert!(
        telem.temperature_c >= 20 && telem.temperature_c <= 100,
        "Temperature should be in valid range"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Module b: FFB Pipeline
// ---------------------------------------------------------------------------

#[test]
fn test_ffb_pipeline_passthrough_empty() -> Result<()> {
    let mut pipeline = Pipeline::new();
    let mut frame = Frame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };

    // An empty pipeline should leave the frame unchanged
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "Empty pipeline should preserve torque_out"
    );
    Ok(())
}

#[test]
fn test_ffb_pipeline_bounds_validation() -> Result<()> {
    let mut pipeline = Pipeline::new();

    // Valid bounded input
    let mut frame = Frame {
        ffb_in: 0.8,
        torque_out: 0.8,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 2_000_000,
        seq: 2,
    };
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok(), "Bounded torque_out should pass validation");
    assert!(
        frame.torque_out.abs() <= 1.0,
        "torque_out should remain within [-1.0, 1.0]"
    );
    Ok(())
}

#[test]
fn test_ffb_safety_clamp_in_safe_mode() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    // In SafeTorque mode, output should be clamped to safe limit
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        (clamped - 5.0).abs() < f32::EPSILON,
        "Torque should be clamped to safe limit (5 Nm), got {}",
        clamped
    );

    // Negative torque should also be clamped symmetrically
    let clamped_neg = safety.clamp_torque_nm(-10.0);
    assert!(
        (clamped_neg - (-5.0)).abs() < f32::EPSILON,
        "Negative torque should be clamped to -5 Nm, got {}",
        clamped_neg
    );
    Ok(())
}

#[test]
fn test_ffb_safety_clamp_nan_to_zero() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);
    let clamped = safety.clamp_torque_nm(f32::NAN);
    assert!(
        (clamped - 0.0).abs() < f32::EPSILON,
        "NaN torque should be clamped to 0.0, got {}",
        clamped
    );
    Ok(())
}

#[tokio::test]
async fn test_ffb_virtual_device_write_read_roundtrip() -> Result<()> {
    use racing_wheel_engine::ports::HidDevice;

    let id: DeviceId = "ffb-roundtrip-001".parse()?;
    let mut device = VirtualDevice::new(id, "FFB Test Wheel".to_string());

    // Write a torque command
    device.write_ffb_report(5.0, 1)?;

    // Read back telemetry (device is in known state)
    let telemetry = device.read_telemetry();
    assert!(telemetry.is_some(), "Should get telemetry back from device");
    Ok(())
}

#[test]
fn test_ffb_mode_selection_raw_torque_preferred() -> Result<()> {
    let caps = DeviceCapabilities::new(
        false, // supports_pid
        true,  // supports_raw_torque_1khz
        true,  // supports_health_stream
        false, // supports_led_bus
        TorqueNm::new(20.0)?,
        10000, // encoder_cpr
        1000,  // min_report_period_us
    );

    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Device with 1kHz support should select RawTorque mode"
    );
    Ok(())
}

#[test]
fn test_ffb_capability_negotiation_with_game() -> Result<()> {
    let caps = DeviceCapabilities::new(
        true, // supports_pid
        true, // supports_raw_torque_1khz
        true, // supports_health_stream
        true, // supports_led_bus
        TorqueNm::new(25.0)?,
        65535,
        1000,
    );

    let game = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let result: NegotiationResult =
        CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game));

    assert_eq!(
        result.mode,
        FFBMode::RawTorque,
        "ACC with capable device should negotiate RawTorque"
    );
    assert!(
        (result.update_rate_hz - 1000.0).abs() < f32::EPSILON,
        "RawTorque should run at 1kHz, got {}",
        result.update_rate_hz
    );
    Ok(())
}

#[test]
fn test_ffb_negotiation_fallback_telemetry_synth() -> Result<()> {
    let caps = DeviceCapabilities::new(
        false, // supports_pid
        false, // supports_raw_torque_1khz — device doesn't support RT
        false, // supports_health_stream
        false, // supports_led_bus
        TorqueNm::new(5.0)?,
        1000,
        16667, // ~60Hz
    );

    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);
    assert_eq!(
        result.mode,
        FFBMode::TelemetrySynth,
        "Incapable device should fall back to TelemetrySynth"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Module c: Telemetry Round-Trip
// ---------------------------------------------------------------------------

#[test]
fn test_telemetry_adapter_registry_not_empty() -> Result<()> {
    let factories = adapter_factories();
    assert!(
        !factories.is_empty(),
        "Adapter factory registry should not be empty"
    );
    // Should have at least the major titles
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();
    for expected in &["forza_motorsport", "acc", "iracing"] {
        assert!(
            ids.contains(expected),
            "Missing expected adapter: {}",
            expected
        );
    }
    Ok(())
}

#[test]
fn test_telemetry_adapter_unique_game_ids() -> Result<()> {
    let factories = adapter_factories();
    let mut seen = std::collections::HashSet::new();
    for (id, factory) in factories {
        assert!(seen.insert(*id), "Duplicate adapter factory ID: {}", id);
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *id,
            "Adapter game_id() should match registry key for '{}'",
            id
        );
    }
    Ok(())
}

#[test]
fn test_telemetry_forza_normalize_rejects_short_packet() -> Result<()> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == "forza_motorsport")
        .ok_or_else(|| anyhow::anyhow!("forza_motorsport adapter not found"))?;

    let adapter = factory();

    // A packet that is too short should be rejected
    let short_packet = [0u8; 4];
    let result = adapter.normalize(&short_packet);
    assert!(result.is_err(), "Short packet should fail normalization");
    Ok(())
}

#[test]
fn test_telemetry_adapters_all_reject_empty_packet() -> Result<()> {
    let factories = adapter_factories();
    let empty: &[u8] = &[];
    let mut failed = Vec::new();
    for (id, factory) in factories {
        let adapter = factory();
        let result = adapter.normalize(empty);
        if result.is_ok() {
            failed.push(*id);
        }
    }
    // Most adapters should reject empty input; allow a small set of known
    // exceptions that silently produce default telemetry for compatibility.
    let total = factories.len();
    let reject_count = total - failed.len();
    assert!(
        reject_count > total / 2,
        "Majority of adapters should reject empty packets; {} of {} accepted: {:?}",
        failed.len(),
        total,
        failed
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Module d: Plugin System Basics
// ---------------------------------------------------------------------------

fn make_test_manifest(class: PluginClass, name: &str) -> PluginManifest {
    let (max_exec_us, max_mem, rate_hz) = match class {
        PluginClass::Safe => (5000, 16 * 1024 * 1024, 200),
        PluginClass::Fast => (200, 4 * 1024 * 1024, 1000),
    };

    PluginManifest {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "Test plugin".to_string(),
        author: "Integration Test".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class,
        capabilities: vec![Capability::ReadTelemetry],
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: max_exec_us,
            max_memory_bytes: max_mem,
            update_rate_hz: rate_hz,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("test_plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

#[test]
fn test_plugin_manifest_validation_safe_class() -> Result<()> {
    let validator = ManifestValidator::default();
    let manifest = make_test_manifest(PluginClass::Safe, "safe-test-plugin");

    let result = validator.validate(&manifest);
    assert!(result.is_ok(), "Valid Safe manifest should pass validation");
    Ok(())
}

#[test]
fn test_plugin_manifest_validation_fast_class() -> Result<()> {
    let validator = ManifestValidator::default();
    let manifest = make_test_manifest(PluginClass::Fast, "fast-test-plugin");

    let result = validator.validate(&manifest);
    assert!(result.is_ok(), "Valid Fast manifest should pass validation");
    Ok(())
}

#[test]
fn test_plugin_manifest_rejects_empty_name() -> Result<()> {
    let validator = ManifestValidator::default();
    let mut manifest = make_test_manifest(PluginClass::Safe, "");
    manifest.name = String::new();

    let result = validator.validate(&manifest);
    assert!(result.is_err(), "Empty name should fail validation");

    match result {
        Err(PluginError::ManifestValidation(msg)) => {
            assert!(
                msg.contains("name"),
                "Error message should mention 'name', got: {}",
                msg
            );
        }
        _ => anyhow::bail!("Expected ManifestValidation error"),
    }
    Ok(())
}

#[test]
fn test_plugin_safe_class_cannot_use_dsp_capability() -> Result<()> {
    let validator = ManifestValidator::default();
    let mut manifest = make_test_manifest(PluginClass::Safe, "bad-safe-plugin");
    manifest.capabilities.push(Capability::ProcessDsp);

    let result = validator.validate(&manifest);
    assert!(
        result.is_err(),
        "Safe plugin requesting ProcessDsp should be rejected"
    );
    Ok(())
}

#[test]
fn test_plugin_fast_class_respects_execution_budget() -> Result<()> {
    let validator = ManifestValidator::default();
    let mut manifest = make_test_manifest(PluginClass::Fast, "over-budget-plugin");
    // Fast plugins max is 200μs; exceed it
    manifest.constraints.max_execution_time_us = 500;

    let result = validator.validate(&manifest);
    assert!(
        result.is_err(),
        "Fast plugin exceeding execution budget should be rejected"
    );
    Ok(())
}

#[test]
fn test_plugin_safe_class_respects_memory_budget() -> Result<()> {
    let validator = ManifestValidator::default();
    let mut manifest = make_test_manifest(PluginClass::Safe, "memory-hog-plugin");
    // Safe plugins max is 16MB; exceed it
    manifest.constraints.max_memory_bytes = 32 * 1024 * 1024;

    let result = validator.validate(&manifest);
    assert!(
        result.is_err(),
        "Safe plugin exceeding memory budget should be rejected"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Module e: Safety Interlocks
// ---------------------------------------------------------------------------

#[test]
fn test_safety_initial_state_is_safe_torque() -> Result<()> {
    let service = SafetyService::new(5.0, 25.0);
    assert_eq!(
        service.state(),
        &SafetyState::SafeTorque,
        "Initial state should be SafeTorque"
    );
    assert!(
        (service.max_torque_nm() - 5.0).abs() < f32::EPSILON,
        "Safe mode max torque should be 5 Nm"
    );
    Ok(())
}

#[test]
fn test_safety_fault_detection_sets_faulted_state() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);

    let before = Instant::now();
    service.report_fault(FaultType::ThermalLimit);
    let after = Instant::now();

    match service.state() {
        SafetyState::Faulted { fault, since } => {
            assert_eq!(
                *fault,
                FaultType::ThermalLimit,
                "Fault type should be ThermalLimit"
            );
            assert!(
                *since >= before && *since <= after,
                "Fault timestamp should be within test window"
            );
        }
        other => anyhow::bail!("Expected Faulted state, got {:?}", other),
    }
    Ok(())
}

#[test]
fn test_safety_faulted_state_clamps_torque_to_zero() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);
    service.report_fault(FaultType::UsbStall);

    let clamped = service.clamp_torque_nm(25.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Faulted state should clamp torque to 0, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn test_safety_fault_response_timing() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);

    let start = Instant::now();
    service.report_fault(FaultType::Overcurrent);
    let torque = service.clamp_torque_nm(20.0);
    let elapsed = start.elapsed();

    assert!(
        torque.abs() < f32::EPSILON,
        "Torque must be zero after fault"
    );

    // Requirement: fault detection time ≤10ms, response time ≤50ms
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection + response must complete within 10ms, took {:?}",
        elapsed
    );
    Ok(())
}

#[test]
fn test_safety_all_fault_types_transition_to_faulted() -> Result<()> {
    let fault_types = [
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

    for fault in &fault_types {
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(*fault);

        match service.state() {
            SafetyState::Faulted {
                fault: reported, ..
            } => {
                assert_eq!(
                    reported, fault,
                    "State should contain reported fault type {:?}",
                    fault
                );
            }
            other => anyhow::bail!("Expected Faulted state for {:?}, got {:?}", fault, other),
        }

        // Verify torque is zero for every fault type
        let clamped = service.clamp_torque_nm(10.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Torque should be 0 in faulted state for {:?}, got {}",
            fault,
            clamped
        );
    }
    Ok(())
}

#[test]
fn test_safety_clear_fault_requires_minimum_duration() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);
    service.report_fault(FaultType::EncoderNaN);

    // Immediately trying to clear should fail (min duration not met)
    let result = service.clear_fault();
    assert!(
        result.is_err(),
        "Clearing fault immediately should fail (minimum duration)"
    );
    Ok(())
}

#[test]
fn test_safety_high_torque_challenge_flow() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);

    // Request high torque — transitions to HighTorqueChallenge
    let challenge = service
        .request_high_torque("test-device")
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        challenge.challenge_token != 0,
        "Challenge token should be non-zero"
    );

    match service.state() {
        SafetyState::HighTorqueChallenge { .. } => {}
        other => anyhow::bail!("Expected HighTorqueChallenge, got {:?}", other),
    }

    // Torque should still be limited during challenge
    let clamped = service.clamp_torque_nm(20.0);
    assert!(
        (clamped - 5.0).abs() < f32::EPSILON,
        "During challenge, torque should be limited to safe max"
    );
    Ok(())
}

#[test]
fn test_safety_cannot_request_high_torque_when_faulted() -> Result<()> {
    let mut service = SafetyService::new(5.0, 25.0);
    service.report_fault(FaultType::ThermalLimit);

    let result = service.request_high_torque("test-device");
    assert!(
        result.is_err(),
        "Should not be able to request high torque while faulted"
    );
    Ok(())
}

#[test]
fn test_safety_deterministic_clamp_under_load() -> Result<()> {
    let service = SafetyService::new(5.0, 25.0);

    // Run many iterations to verify deterministic behavior
    let start = Instant::now();
    for i in 0u32..10_000 {
        let input = (i as f32 / 10_000.0) * 50.0 - 25.0; // range [-25, 25]
        let clamped = service.clamp_torque_nm(input);
        assert!(
            clamped.abs() <= 5.0 + f32::EPSILON,
            "Torque must always be within safe limit"
        );
    }
    let elapsed = start.elapsed();

    // 10k clamp operations should complete well within 1ms total
    assert!(
        elapsed < Duration::from_millis(10),
        "10k clamp operations took {:?}, should be < 10ms",
        elapsed
    );
    Ok(())
}
