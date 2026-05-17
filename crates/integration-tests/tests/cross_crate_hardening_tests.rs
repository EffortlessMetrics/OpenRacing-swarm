//! Cross-crate integration tests for RC hardening.
//!
//! These tests verify interactions **between** workspace crates rather than
//! testing individual crate behaviour:
//!
//! 1. Full telemetry pipeline: raw bytes → adapter → normalized data → validation
//! 2. Multi-vendor device initialization patterns
//! 3. Game support matrix loading and validation
//! 4. Plugin system integration (manifest + capabilities + quarantine)
//! 5. Service configuration merging/validation

use std::collections::HashSet;

use uuid::Uuid;

// ── Telemetry adapters + schemas ─────────────────────────────────────────────
use openracing_telemetry_adapters::{MockAdapter, TelemetryAdapter, adapter_factories};
use openracing_telemetry_config::{load_default_matrix, matrix_game_id_set};
use racing_wheel_schemas::prelude::*;

// ── Engine types ─────────────────────────────────────────────────────────────
use racing_wheel_engine::safety::{FaultType, SafetyService};
use racing_wheel_engine::{
    CapabilityNegotiator, Frame as EngineFrame, ModeSelectionPolicy, Pipeline as EnginePipeline,
    VirtualDevice, VirtualHidPort,
};

// ── Filter types ─────────────────────────────────────────────────────────────
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};

// ── Plugin types ─────────────────────────────────────────────────────────────
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use racing_wheel_plugins::registry::{PluginMetadata, VersionCompatibility, check_compatibility};
use racing_wheel_plugins::{CapabilityChecker, PluginClass};

// ── Service types ────────────────────────────────────────────────────────────
use racing_wheel_service::system_config::SystemConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Full telemetry pipeline: raw bytes → adapter → normalized → filter → safety
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a minimal Forza Sled packet (232 bytes) with known field values.
fn build_forza_sled_packet(vel_x: f32, vel_z: f32, rpm: f32, max_rpm: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 232];
    buf[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    buf[8..12].copy_from_slice(&max_rpm.to_le_bytes());
    buf[16..20].copy_from_slice(&rpm.to_le_bytes());
    buf[32..36].copy_from_slice(&vel_x.to_le_bytes());
    buf[40..44].copy_from_slice(&vel_z.to_le_bytes());
    buf
}

/// Build a minimal LFS OutGauge packet (96 bytes).
fn build_lfs_outgauge_packet(speed: f32, rpm: f32, gear: u8, throttle: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    buf[10] = gear;
    buf[12..16].copy_from_slice(&speed.to_le_bytes());
    buf[16..20].copy_from_slice(&rpm.to_le_bytes());
    buf[48..52].copy_from_slice(&throttle.to_le_bytes());
    buf
}

#[test]
fn full_pipeline_forza_raw_bytes_to_safety_clamped_output() -> Result<(), Box<dyn std::error::Error>>
{
    // Step 1: raw bytes → adapter (telemetry-adapters crate)
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == "forza_motorsport")
        .ok_or("forza_motorsport adapter not found")?;
    let adapter = factory();

    let packet = build_forza_sled_packet(30.0, 40.0, 7200.0, 9000.0);
    let telemetry = adapter.normalize(&packet)?;

    // Step 2: verify normalized telemetry (schemas crate)
    let expected_speed = (30.0f32.powi(2) + 40.0f32.powi(2)).sqrt();
    assert!(
        (telemetry.speed_ms - expected_speed).abs() < 1.0,
        "Forza speed should be ~{expected_speed}, got {}",
        telemetry.speed_ms
    );
    assert!(
        (telemetry.rpm - 7200.0).abs() < 1.0,
        "Forza RPM should be ~7200, got {}",
        telemetry.rpm
    );

    // Step 3: feed into filter pipeline (openracing-filters crate)
    let mut frame = FilterFrame {
        ffb_in: telemetry.ffb_scalar,
        torque_out: telemetry.ffb_scalar,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let damper = DamperState::fixed(0.02);
    let friction = FrictionState::fixed(0.01);
    damper_filter(&mut frame, &damper);
    friction_filter(&mut frame, &friction);
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite(),
        "Filter output must be finite, got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out.abs() <= 1.0,
        "Filter output must be in [-1, 1], got {}",
        frame.torque_out
    );

    // Step 4: pass through engine pipeline + safety (engine crate)
    let mut engine_frame = EngineFrame {
        ffb_in: frame.torque_out,
        torque_out: frame.torque_out,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut engine_frame)?;

    let safety = SafetyService::new(5.0, 20.0);
    let final_nm = safety.clamp_torque_nm(engine_frame.torque_out * 5.0);
    assert!(
        final_nm.abs() <= 5.0,
        "Safety-clamped torque must be ≤ safe limit, got {}",
        final_nm
    );

    Ok(())
}

#[test]
fn full_pipeline_lfs_raw_bytes_to_safety_clamped_output() -> Result<(), Box<dyn std::error::Error>>
{
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == "live_for_speed")
        .ok_or("live_for_speed adapter not found")?;
    let adapter = factory();

    let packet = build_lfs_outgauge_packet(42.0, 6800.0, 4, 0.9);
    let telemetry = adapter.normalize(&packet)?;

    assert!(
        (telemetry.speed_ms - 42.0).abs() < 0.5,
        "LFS speed should be ~42.0, got {}",
        telemetry.speed_ms
    );

    // Feed through filters
    let mut frame = FilterFrame {
        ffb_in: telemetry.ffb_scalar,
        torque_out: telemetry.ffb_scalar,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut frame, &DamperState::fixed(0.01));
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "Filter output out of range: {}",
        frame.torque_out
    );

    // Safety clamp in faulted state should zero output
    let mut safety = SafetyService::new(5.0, 20.0);
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(frame.torque_out * 5.0);
    assert!(
        clamped.abs() < 0.001,
        "Faulted safety must zero torque, got {}",
        clamped
    );

    Ok(())
}

#[test]
fn mock_adapter_output_integrates_with_filter_and_engine_pipeline()
-> Result<(), Box<dyn std::error::Error>> {
    let adapter = MockAdapter::new("test_pipeline".to_string());
    let telemetry = adapter.normalize(&[])?;

    // MockAdapter produces rpm=5000
    assert!(
        (telemetry.rpm - 5000.0).abs() < 1.0,
        "MockAdapter RPM should be 5000, got {}",
        telemetry.rpm
    );

    // Feed through filter + engine pipeline
    let mut frame = FilterFrame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut frame, &DamperState::fixed(0.01));
    friction_filter(&mut frame, &FrictionState::fixed(0.01));
    torque_cap_filter(&mut frame, 1.0);

    let mut engine_frame = EngineFrame {
        ffb_in: frame.torque_out,
        torque_out: frame.torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut engine_frame)?;

    assert!(
        engine_frame.torque_out.is_finite(),
        "Engine output must be finite"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multi-vendor device initialization patterns
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_vendor_virtual_devices_enumerate_correctly() -> Result<(), Box<dyn std::error::Error>>
{
    use racing_wheel_engine::ports::HidPort;

    let vendors = [
        ("vdev-moza-001", "Moza R16"),
        ("vdev-fanatec-001", "Fanatec DD Pro"),
        ("vdev-logitech-001", "Logitech G Pro"),
        ("vdev-simagic-001", "Simagic Alpha Mini"),
    ];

    let mut port = VirtualHidPort::new();
    for (id_str, name) in &vendors {
        let id: DeviceId = id_str.parse()?;
        let device = VirtualDevice::new(id, name.to_string());
        port.add_device(device)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    assert_eq!(
        devices.len(),
        vendors.len(),
        "Expected {} devices, found {}",
        vendors.len(),
        devices.len()
    );

    // Verify all devices are connected and have 1kHz capability
    for info in &devices {
        assert!(info.is_connected, "Device {} should be connected", info.id);
        assert!(
            info.capabilities.supports_raw_torque_1khz,
            "Device {} should support 1kHz raw torque",
            info.id
        );
    }

    // Verify unique IDs
    let ids: HashSet<&DeviceId> = devices.iter().map(|d| &d.id).collect();
    assert_eq!(ids.len(), vendors.len(), "All device IDs must be unique");

    Ok(())
}

#[test]
fn capability_negotiation_selects_correct_ffb_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Build device capabilities matching a high-end direct-drive wheel
    let caps = DeviceCapabilities::new(
        false,                // supports_pid
        true,                 // supports_raw_torque_1khz
        true,                 // supports_health_stream
        true,                 // supports_led_bus
        TorqueNm::new(25.0)?, // max torque
        10000,                // encoder_cpr
        1000,                 // min_report_period_us
    );

    // Negotiate without game compatibility info → should pick RawTorque
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);
    assert_eq!(
        result.mode,
        racing_wheel_engine::FFBMode::RawTorque,
        "DD wheel without game info should negotiate RawTorque, got {:?}",
        result.mode
    );
    assert!(
        result.update_rate_hz >= 999.0,
        "RawTorque should run at ~1kHz, got {}",
        result.update_rate_hz
    );

    // With game compatibility → should respect game preference
    let game_compat = racing_wheel_engine::GameCompatibility {
        game_id: "test_game".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: racing_wheel_engine::FFBMode::RawTorque,
    };
    let result2 = CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game_compat));
    assert_eq!(
        result2.mode,
        racing_wheel_engine::FFBMode::RawTorque,
        "DD wheel + robust FFB game should use RawTorque"
    );

    // PID-only device should not get RawTorque
    let pid_caps = DeviceCapabilities::new(
        true,                // supports_pid
        false,               // no raw torque
        false,               // no health stream
        false,               // no LED bus
        TorqueNm::new(3.0)?, // lower torque
        1024,                // lower encoder CPR
        16666,               // ~60Hz report period
    );
    let result3 = CapabilityNegotiator::negotiate_capabilities(&pid_caps, None);
    assert_eq!(
        result3.mode,
        racing_wheel_engine::FFBMode::PidPassthrough,
        "PID-only device should negotiate PidPassthrough, got {:?}",
        result3.mode
    );

    // ModeSelectionPolicy compatibility check
    assert!(
        ModeSelectionPolicy::is_mode_compatible(racing_wheel_engine::FFBMode::RawTorque, &caps),
        "RawTorque must be compatible with DD device"
    );
    assert!(
        !ModeSelectionPolicy::is_mode_compatible(
            racing_wheel_engine::FFBMode::RawTorque,
            &pid_caps
        ),
        "RawTorque must not be compatible with PID-only device"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Game support matrix loading and validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn game_support_matrix_loads_and_contains_required_games() -> Result<(), Box<dyn std::error::Error>>
{
    let matrix = load_default_matrix()?;

    // Matrix must have a reasonable number of games
    assert!(
        matrix.games.len() >= 20,
        "Expected ≥20 games in matrix, found {}",
        matrix.games.len()
    );

    // Key titles must be present
    let required_games = [
        "acc",
        "iracing",
        "forza_motorsport",
        "rfactor2",
        "live_for_speed",
        "beamng_drive",
        "eawrc",
    ];
    for game in &required_games {
        assert!(
            matrix.has_game_id(game),
            "Game support matrix must include '{game}'"
        );
    }

    Ok(())
}

#[test]
fn game_support_matrix_entries_have_valid_telemetry_config()
-> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;

    for (game_id, support) in &matrix.games {
        // Every game must have a name
        assert!(
            !support.name.is_empty(),
            "Game '{game_id}' must have a non-empty name"
        );

        // Every game must have at least one version
        assert!(
            !support.versions.is_empty(),
            "Game '{game_id}' must have at least one version entry"
        );

        // Telemetry method must be non-empty
        assert!(
            !support.telemetry.method.is_empty(),
            "Game '{game_id}' must have a telemetry method"
        );

        // For games that have telemetry, update rate must be reasonable (1..=1000 Hz)
        if support.telemetry.method != "none" {
            assert!(
                support.telemetry.update_rate_hz >= 1 && support.telemetry.update_rate_hz <= 1000,
                "Game '{game_id}' has suspicious update rate: {}",
                support.telemetry.update_rate_hz
            );
        }

        // Config writer must be non-empty
        assert!(
            !support.config_writer.is_empty(),
            "Game '{game_id}' must specify a config_writer"
        );
    }

    Ok(())
}

#[test]
fn adapter_registry_aligns_with_game_support_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let config_ids = matrix_game_id_set()?;
    let support_ids = racing_wheel_telemetry_support::matrix_game_id_set()?;
    let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

    // Every adapter should appear in both YAML matrices
    let mut missing_config = Vec::new();
    let mut missing_support = Vec::new();
    for id in &adapter_ids {
        if !config_ids.contains(*id) {
            missing_config.push(*id);
        }
        if !support_ids.contains(*id) {
            missing_support.push(*id);
        }
    }

    assert!(
        missing_config.is_empty(),
        "Adapters missing from telemetry-config YAML: {missing_config:?}"
    );
    assert!(
        missing_support.is_empty(),
        "Adapters missing from telemetry-support YAML: {missing_support:?}"
    );

    Ok(())
}

#[test]
fn game_support_matrix_stable_games_have_adapter() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;
    let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

    let stable = matrix.stable_games();
    assert!(
        !stable.is_empty(),
        "Matrix should have at least one stable game"
    );

    let mut missing = Vec::new();
    for game_id in &stable {
        if !adapter_ids.contains(&**game_id) {
            missing.push(&**game_id);
        }
    }

    assert!(
        missing.is_empty(),
        "Stable games without adapters: {missing:?}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Plugin system integration
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a valid test manifest for a WASM (Safe) telemetry-processing plugin.
fn make_safe_plugin_manifest(name: &str, capabilities: Vec<Capability>) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Test plugin: {name}"),
        author: "Integration Test".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities,
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: 4000,
            max_memory_bytes: 8 * 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

/// Build a valid test manifest for a native (Fast) DSP plugin.
fn make_fast_plugin_manifest(name: &str) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Fast test plugin: {name}"),
        author: "Integration Test".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Fast,
        capabilities: vec![Capability::ReadTelemetry, Capability::ProcessDsp],
        operations: vec![PluginOperation::DspFilter],
        constraints: PluginConstraints {
            max_execution_time_us: 150,
            max_memory_bytes: 2 * 1024 * 1024,
            update_rate_hz: 1000,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: None,
            native_library: Some("libplugin.dll".to_string()),
            main_function: "process_dsp".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

#[test]
fn safe_plugin_manifest_validates_with_correct_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let validator = ManifestValidator::default();
    let manifest = make_safe_plugin_manifest(
        "telemetry-overlay",
        vec![Capability::ReadTelemetry, Capability::ModifyTelemetry],
    );

    validator
        .validate(&manifest)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(())
}

#[test]
fn fast_plugin_manifest_validates_with_dsp_capability() -> Result<(), Box<dyn std::error::Error>> {
    let validator = ManifestValidator::default();
    let manifest = make_fast_plugin_manifest("rt-dsp-filter");

    validator
        .validate(&manifest)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(())
}

#[test]
fn safe_plugin_rejects_dsp_capability() -> Result<(), Box<dyn std::error::Error>> {
    let validator = ManifestValidator::default();
    // ProcessDsp is not allowed for Safe plugins
    let manifest = make_safe_plugin_manifest(
        "bad-safe-plugin",
        vec![Capability::ReadTelemetry, Capability::ProcessDsp],
    );

    let result = validator.validate(&manifest);
    assert!(
        result.is_err(),
        "Safe plugin with ProcessDsp capability must fail validation"
    );

    Ok(())
}

#[test]
fn plugin_manifest_rejects_excessive_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let validator = ManifestValidator::default();
    let mut manifest = make_safe_plugin_manifest("greedy-plugin", vec![Capability::ReadTelemetry]);
    // Exceed Safe plugin execution time limit (max 5000μs)
    manifest.constraints.max_execution_time_us = 10_000;

    let result = validator.validate(&manifest);
    assert!(
        result.is_err(),
        "Plugin exceeding execution time budget must fail validation"
    );

    Ok(())
}

#[test]
fn capability_checker_enforces_cross_crate_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    // A plugin with only ReadTelemetry should not be able to modify or do DSP
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(
        checker.check_telemetry_read().is_ok(),
        "Read should be allowed"
    );
    assert!(
        checker.check_telemetry_modify().is_err(),
        "Modify should be denied"
    );
    assert!(
        checker.check_dsp_processing().is_err(),
        "DSP should be denied"
    );
    assert!(
        checker.check_led_control().is_err(),
        "LED control should be denied"
    );

    // A plugin with full capabilities
    let full_checker = CapabilityChecker::new(vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::ProcessDsp,
    ]);

    assert!(full_checker.check_telemetry_read().is_ok());
    assert!(full_checker.check_telemetry_modify().is_ok());
    assert!(full_checker.check_led_control().is_ok());
    assert!(full_checker.check_dsp_processing().is_ok());

    Ok(())
}

#[test]
fn quarantine_manager_quarantines_after_repeated_crashes() -> Result<(), Box<dyn std::error::Error>>
{
    let policy = QuarantinePolicy {
        max_crashes: 3,
        max_budget_violations: 10,
        violation_window_minutes: 60,
        quarantine_duration_minutes: 60,
        max_escalation_levels: 5,
    };

    let mut manager = QuarantineManager::new(policy);
    let plugin_id = Uuid::new_v4();

    // Record crashes up to threshold
    for i in 0..3 {
        manager.record_violation(plugin_id, ViolationType::Crash, format!("crash #{}", i + 1))?;
    }

    // After 3 crashes the plugin should be quarantined
    assert!(
        manager.is_quarantined(plugin_id),
        "Plugin must be quarantined after 3 crashes"
    );

    // Verify quarantine state
    let state = manager.get_quarantine_state(plugin_id);
    assert!(state.is_some(), "Quarantine state must exist");
    let state = state.ok_or("missing quarantine state")?;
    assert_eq!(state.total_crashes, 3);
    assert!(state.quarantine_start.is_some());
    assert!(state.quarantine_end.is_some());

    Ok(())
}

#[test]
fn quarantine_release_allows_plugin_to_run_again() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let plugin_id = Uuid::new_v4();

    // Manually quarantine
    manager.manual_quarantine(plugin_id, 60)?;
    assert!(
        manager.is_quarantined(plugin_id),
        "Plugin should be quarantined"
    );

    // Release
    manager.release_from_quarantine(plugin_id)?;
    assert!(
        !manager.is_quarantined(plugin_id),
        "Plugin should not be quarantined after release"
    );

    Ok(())
}

#[test]
fn plugin_version_compatibility_across_registry() -> Result<(), Box<dyn std::error::Error>> {
    // Same major, higher minor → compatible
    let req = semver::Version::new(1, 0, 0);
    let avail = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&req, &avail),
        VersionCompatibility::Compatible
    );

    // Different major → incompatible
    let avail_v2 = semver::Version::new(2, 0, 0);
    assert_eq!(
        check_compatibility(&req, &avail_v2),
        VersionCompatibility::Incompatible
    );

    // 0.x requires exact minor match
    let req_0 = semver::Version::new(0, 3, 0);
    let avail_0_ok = semver::Version::new(0, 3, 5);
    let avail_0_bad = semver::Version::new(0, 4, 0);
    assert_eq!(
        check_compatibility(&req_0, &avail_0_ok),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(&req_0, &avail_0_bad),
        VersionCompatibility::Incompatible
    );

    Ok(())
}

#[test]
fn plugin_metadata_validation_rejects_empty_fields() -> Result<(), Box<dyn std::error::Error>> {
    let meta = PluginMetadata::new(
        "",
        semver::Version::new(1, 0, 0),
        "Author",
        "Description",
        "MIT",
    );
    let result = meta.validate();
    assert!(result.is_err(), "Empty plugin name must fail validation");

    let meta2 = PluginMetadata::new(
        "ValidName",
        semver::Version::new(1, 0, 0),
        "",
        "Description",
        "MIT",
    );
    let result2 = meta2.validate();
    assert!(result2.is_err(), "Empty author must fail validation");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Service configuration merging/validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn default_system_config_validates_successfully() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    config.validate()?;
    Ok(())
}

#[test]
fn system_config_rejects_zero_tick_rate() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = SystemConfig::default();
    config.engine.tick_rate_hz = 0;

    let result = config.validate();
    assert!(result.is_err(), "Zero tick rate must fail validation");

    Ok(())
}

#[test]
fn system_config_rejects_excessive_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = SystemConfig::default();
    config.engine.max_jitter_us = 5000;

    let result = config.validate();
    assert!(result.is_err(), "Jitter > 1000μs must fail validation");

    Ok(())
}

#[test]
fn system_config_rejects_invalid_safety_torque() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = SystemConfig::default();
    // safe torque > max torque
    config.safety.default_safe_torque_nm = 30.0;
    config.safety.max_torque_nm = 25.0;

    let result = config.validate();
    assert!(
        result.is_err(),
        "Safe torque > max torque must fail validation"
    );

    Ok(())
}

#[test]
fn system_config_rejects_invalid_tracing_sample_rate() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = SystemConfig::default();
    config.observability.tracing_sample_rate = 2.0;

    let result = config.validate();
    assert!(result.is_err(), "Sample rate > 1.0 must fail validation");

    Ok(())
}

#[test]
fn system_config_serialization_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&config)?;

    // Deserialize back
    let restored: SystemConfig = serde_json::from_str(&json)?;

    // Validate the restored config
    restored.validate()?;

    // Spot-check key fields survived the round trip
    assert_eq!(config.schema_version, restored.schema_version);
    assert_eq!(config.engine.tick_rate_hz, restored.engine.tick_rate_hz);
    assert_eq!(config.safety.max_torque_nm, restored.safety.max_torque_nm);
    assert_eq!(config.service.service_name, restored.service.service_name);

    Ok(())
}

#[test]
fn system_config_default_game_entries_match_adapter_registry()
-> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

    // Every game in the default SystemConfig should have a corresponding adapter
    for game_id in config.games.supported_games.keys() {
        assert!(
            adapter_ids.contains(&**game_id),
            "SystemConfig game '{game_id}' has no matching adapter in the registry"
        );
    }

    Ok(())
}

#[test]
fn system_config_safety_limits_align_with_engine_safety_service()
-> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();

    // Create a SafetyService from the config values
    let safety = SafetyService::new(
        config.safety.default_safe_torque_nm,
        config.safety.max_torque_nm,
    );

    // In SafeTorque state, torque at the limit should pass through
    let clamped = safety.clamp_torque_nm(config.safety.default_safe_torque_nm);
    assert!(
        (clamped - config.safety.default_safe_torque_nm).abs() < 0.01,
        "Torque at safe limit should pass through, got {}",
        clamped
    );

    // Torque above the safe limit should be clamped
    let over_limit = safety.clamp_torque_nm(config.safety.default_safe_torque_nm + 10.0);
    assert!(
        over_limit <= config.safety.default_safe_torque_nm + 0.01,
        "Torque above safe limit should be clamped, got {}",
        over_limit
    );

    Ok(())
}

#[test]
fn system_config_migration_from_v0_updates_schema_version() -> Result<(), Box<dyn std::error::Error>>
{
    let mut config = SystemConfig {
        schema_version: "wheel.config/0".to_string(),
        ..SystemConfig::default()
    };

    let migrated = config.migrate()?;
    assert!(migrated, "Migration from v0 should report changes");
    assert_eq!(
        config.schema_version, "wheel.config/1",
        "Schema version must be updated to v1"
    );

    // Migrated config should still validate
    config.validate()?;

    Ok(())
}
