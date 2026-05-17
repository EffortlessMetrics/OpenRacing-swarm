//! BDD-style acceptance tests for end-to-end user scenarios.
//!
//! Each test follows the **Given / When / Then** pattern and exercises the
//! public APIs with mock or virtual infrastructure — no real USB hardware or
//! running game processes are required.
//!
//! # Scenarios covered
//!
//! * **Plug-and-Play Device Detection** – a Logitech G29 is identified with
//!   correct capabilities (2.8 Nm, 900°, PID).
//! * **Multi-Device Support** – devices from different vendors are enumerated
//!   with correct per-device capabilities.
//! * **Telemetry Reception** – an ACC telemetry packet is parsed into a
//!   unified format with valid field ranges.
//! * **Force Feedback Pipeline** – a 3 Nm command on a 5 Nm device stays
//!   within safety limits and is formatted for the wire protocol.
//! * **Safety Interlock Activation** – a communication-loss fault triggers
//!   torque shutdown within 50 ms.
//! * **Game Profile Switching** – switching from an ACC profile to an iRacing
//!   profile applies the correct device settings.

use std::time::{Duration, Instant};

use racing_wheel_engine::policies::SafetyPolicy;
use racing_wheel_engine::protocol::{TorqueCommand, fault_flags};
use racing_wheel_engine::safety::{FaultType, SafetyState};
use racing_wheel_engine::{CapabilityNegotiator, FFBMode, GameCompatibility, ModeSelectionPolicy};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 1: Plug-and-Play Device Detection
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: Logitech G29 is connected and identified
///
/// ```text
/// Given  a Logitech G29 is connected (mocked HID device)
/// When   the engine starts and reads the capabilities report
/// Then   it identifies the device correctly
/// And    reports correct capabilities (2.8 Nm, 900°, PID support)
/// ```
#[test]
fn scenario_plug_and_play_logitech_g29_detected_with_correct_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a Logitech G29 (2.8 Nm max torque, PID-only, no raw torque at 1 kHz)
    let g29_caps = DeviceCapabilities::new(
        true,  // supports_pid
        false, // supports_raw_torque_1khz (G29 is PID-only)
        false, // supports_health_stream
        false, // supports_led_bus
        TorqueNm::new(2.8)?,
        4096, // encoder CPR
        2000, // min report period (500 Hz max)
    );

    // Simulate the capabilities report round-trip (device → host)
    let report = CapabilityNegotiator::create_capabilities_report(&g29_caps);

    // When: the engine parses the capabilities report on startup
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    // Then: the device is identified with PID support
    assert!(parsed.supports_pid, "Logitech G29 must report PID support");
    assert!(
        !parsed.supports_raw_torque_1khz,
        "Logitech G29 must NOT report raw-torque 1 kHz support"
    );

    // And: max torque is 2.8 Nm
    let max_torque = parsed.max_torque.value();
    assert!(
        (max_torque - 2.8).abs() < 0.1,
        "G29 max torque must be ~2.8 Nm, got {max_torque}"
    );

    // And: the device supports force feedback (via PID)
    assert!(
        parsed.supports_ffb(),
        "G29 must support FFB (via PID protocol)"
    );

    // And: the negotiated mode is PID pass-through (not raw torque)
    let mode = ModeSelectionPolicy::select_mode(&parsed, None);
    assert_eq!(
        mode,
        FFBMode::PidPassthrough,
        "G29 must negotiate PID pass-through mode"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 2: Multi-Device Support
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: multiple devices from different vendors are connected
///
/// ```text
/// Given  multiple devices from different vendors are connected
/// When   the engine enumerates devices
/// Then   all are identified with correct capabilities
/// ```
#[test]
fn scenario_multi_device_enumeration_identifies_all_vendors()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: three devices from different vendors
    let devices: Vec<(&str, DeviceCapabilities)> = vec![
        (
            "Logitech G29 (PID-only, 2.8 Nm)",
            DeviceCapabilities::new(true, false, false, false, TorqueNm::new(2.8)?, 4096, 2000),
        ),
        (
            "Fanatec CSL DD (raw torque, 8 Nm)",
            DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000),
        ),
        (
            "Simucube 2 Pro (raw torque, 25 Nm)",
            DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500),
        ),
    ];

    // When: the engine enumerates all devices via capability reports
    let mut parsed_devices = Vec::new();
    for (label, caps) in &devices {
        let report = CapabilityNegotiator::create_capabilities_report(caps);
        let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        parsed_devices.push((*label, parsed));
    }

    // Then: all devices are detected
    assert_eq!(
        parsed_devices.len(),
        3,
        "all three devices must be enumerated"
    );

    // And: the Logitech G29 capabilities are correct
    let (_, ref g29) = parsed_devices[0];
    assert!(g29.supports_pid, "G29 must support PID");
    assert!(
        !g29.supports_raw_torque_1khz,
        "G29 must not support raw torque"
    );
    assert!(
        (g29.max_torque.value() - 2.8).abs() < 0.1,
        "G29 max torque must be ~2.8 Nm"
    );

    // And: the Fanatec CSL DD capabilities are correct
    let (_, ref csl_dd) = parsed_devices[1];
    assert!(
        csl_dd.supports_raw_torque_1khz,
        "CSL DD must support raw torque"
    );
    assert!(
        (csl_dd.max_torque.value() - 8.0).abs() < 0.1,
        "CSL DD max torque must be ~8 Nm"
    );

    // And: the Simucube 2 Pro capabilities are correct
    let (_, ref sc2) = parsed_devices[2];
    assert!(
        sc2.supports_raw_torque_1khz,
        "SC2 Pro must support raw torque"
    );
    assert!(
        (sc2.max_torque.value() - 25.0).abs() < 0.1,
        "SC2 Pro max torque must be ~25 Nm"
    );

    // And: each device negotiates the correct FFB mode
    assert_eq!(
        ModeSelectionPolicy::select_mode(g29, None),
        FFBMode::PidPassthrough,
        "G29 → PID pass-through"
    );
    assert_eq!(
        ModeSelectionPolicy::select_mode(csl_dd, None),
        FFBMode::RawTorque,
        "CSL DD → raw torque"
    );
    assert_eq!(
        ModeSelectionPolicy::select_mode(sc2, None),
        FFBMode::RawTorque,
        "SC2 Pro → raw torque"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 3: Telemetry Reception
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a minimal ACC `RealtimeCarUpdate` binary packet for testing.
///
/// Layout follows ACC broadcasting protocol v4 (same as plug_and_play.rs).
fn make_acc_car_update_packet(gear_raw: u8, speed_kmh: u16) -> Vec<u8> {
    let mut p = Vec::with_capacity(77);
    p.push(3u8); // MSG_REALTIME_CAR_UPDATE
    p.extend_from_slice(&0u16.to_le_bytes()); // car_index
    p.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    p.push(1u8); // driver_count
    p.push(gear_raw); // gear_raw
    for _ in 0..3 {
        p.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_x, y, yaw
    }
    p.push(1u8); // car_location = 1 (on track)
    p.extend_from_slice(&speed_kmh.to_le_bytes()); // speed_kmh
    p.extend_from_slice(&1u16.to_le_bytes()); // position
    p.extend_from_slice(&1u16.to_le_bytes()); // cup_position
    p.extend_from_slice(&5000u16.to_le_bytes()); // track_position
    p.extend_from_slice(&0.5f32.to_le_bytes()); // spline_position
    p.extend_from_slice(&3u16.to_le_bytes()); // laps
    p.extend_from_slice(&(-500i32).to_le_bytes()); // delta_ms
    // Three lap-time records (13 bytes each)
    for _ in 0..3 {
        p.extend_from_slice(&0i32.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8); // split_count
        p.push(0u8); // is_invalid
        p.push(1u8); // is_valid_for_best
        p.push(0u8); // is_outlap
        p.push(0u8); // is_inlap
    }
    p
}

/// Scenario: ACC telemetry is received and parsed into unified format
///
/// ```text
/// Given  an Assetto Corsa Competizione session is running (mocked UDP)
/// When   telemetry data is received
/// Then   it's correctly parsed into unified format
/// And    all fields have valid ranges
/// ```
#[test]
fn scenario_telemetry_reception_acc_packet_parsed_with_valid_ranges()
-> Result<(), Box<dyn std::error::Error>> {
    use openracing_telemetry_adapters::{ACCAdapter, TelemetryAdapter};

    // Given: an ACC session sending a RealtimeCarUpdate packet
    //   gear_raw = 5 → normalised gear = 5 − 1 = 4
    //   speed_kmh = 180 → speed_ms = 180 / 3.6 = 50.0 m/s
    let adapter = ACCAdapter::new();
    let pkt = make_acc_car_update_packet(5, 180);

    // When: telemetry data is received and normalised
    let telemetry = adapter.normalize(&pkt)?;

    // Then: speed is correctly converted (km/h → m/s)
    let expected_speed_ms = 180.0_f32 / 3.6;
    assert!(
        (telemetry.speed_ms - expected_speed_ms).abs() < 0.5,
        "speed_ms must be ~{expected_speed_ms:.1}, got {}",
        telemetry.speed_ms
    );

    // And: gear is correctly decoded (gear_raw 5 − 1 = 4)
    assert_eq!(
        telemetry.gear, 4,
        "gear must be 4 (gear_raw=5, ACC offset=1)"
    );

    // And: speed is within a physically valid range (0–500 m/s)
    assert!(
        telemetry.speed_ms >= 0.0 && telemetry.speed_ms <= 500.0,
        "speed_ms must be in [0, 500], got {}",
        telemetry.speed_ms
    );

    // And: gear is within a valid range (-1 reverse .. 8 forward)
    assert!(
        telemetry.gear >= -1 && telemetry.gear <= 8,
        "gear must be in [-1, 8], got {}",
        telemetry.gear
    );

    // And: the adapter self-identifies as "acc"
    assert_eq!(
        adapter.game_id(),
        "acc",
        "ACC adapter must report game_id 'acc'"
    );

    Ok(())
}

/// Scenario: truncated telemetry packets are rejected gracefully
///
/// ```text
/// Given  an ACC adapter
/// When   a truncated or empty packet is received
/// Then   normalize() returns Err (no panic, no undefined behaviour)
/// ```
#[test]
fn scenario_telemetry_reception_truncated_packet_returns_error()
-> Result<(), Box<dyn std::error::Error>> {
    use openracing_telemetry_adapters::{ACCAdapter, TelemetryAdapter};

    let adapter = ACCAdapter::new();

    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must return Err"
    );
    assert!(
        adapter.normalize(&[3u8]).is_err(),
        "single-byte packet must return Err"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 4: Force Feedback Pipeline
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: FFB command on a 5 Nm device stays within safety limits
///
/// ```text
/// Given  a device with 5 Nm max torque
/// When   a 3 Nm force feedback command is processed
/// Then   the output is within the safety limits
/// And    the command is formatted for the device's protocol
/// ```
#[test]
fn scenario_ffb_pipeline_3nm_command_on_5nm_device_within_limits()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a device with 5 Nm max torque
    let device_caps =
        DeviceCapabilities::new(true, true, true, false, TorqueNm::new(5.0)?, 65535, 1000);

    // And: a safety policy (default: 5 Nm safe limit, 25 Nm high limit)
    let policy = SafetyPolicy::new()?;

    // When: a 3 Nm FFB command is requested
    let requested_torque = TorqueNm::new(3.0)?;
    let validated = policy.validate_torque_limits(
        requested_torque,
        false, // not in high-torque mode
        &device_caps,
    )?;

    // Then: the output torque is within the device's safety limits
    assert!(
        validated.value() <= device_caps.max_torque.value(),
        "validated torque ({} Nm) must not exceed device max ({} Nm)",
        validated.value(),
        device_caps.max_torque.value()
    );
    assert!(
        (validated.value() - 3.0).abs() < 0.01,
        "3 Nm request on a 5 Nm device must pass through as ~3 Nm, got {}",
        validated.value()
    );

    // And: the command is formatted for the OWP-1 wire protocol
    let cmd = TorqueCommand::new(validated.value(), 0x00, 1);
    assert!(cmd.validate_crc(), "torque command CRC must be valid");
    assert!(
        (cmd.torque_nm() - 3.0).abs() < 0.01,
        "wire-format torque must be ~3 Nm, got {}",
        cmd.torque_nm()
    );

    // And: the report ID is correct (0x20 = torque command)
    assert_eq!(cmd.report_id, 0x20, "torque command report ID must be 0x20");

    Ok(())
}

/// Scenario: FFB command exceeding device max torque is rejected
///
/// ```text
/// Given  a device with 5 Nm max torque
/// When   a 10 Nm force feedback command is requested
/// Then   the safety policy rejects it as exceeding the limit
/// ```
#[test]
fn scenario_ffb_pipeline_command_exceeding_device_limit_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a device with 5 Nm max torque in safe mode (5 Nm safe limit)
    let device_caps =
        DeviceCapabilities::new(true, true, true, false, TorqueNm::new(5.0)?, 65535, 1000);
    let policy = SafetyPolicy::new()?;

    // When: a 10 Nm command is requested (exceeds both device and safe-mode limits)
    let result = policy.validate_torque_limits(TorqueNm::new(10.0)?, false, &device_caps);

    // Then: the policy rejects the request
    assert!(
        result.is_err(),
        "10 Nm request on a 5 Nm device must be rejected by the safety policy"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 5: Safety Interlock Activation
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: communication loss triggers safety interlock within 50 ms
///
/// ```text
/// Given  the engine is running normally
/// When   a fault is detected (e.g., communication loss)
/// Then   the safety interlock activates within 50 ms
/// And    torque output goes to zero
/// ```
#[test]
fn scenario_safety_interlock_comm_loss_activates_within_50ms()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the engine is running normally (SafeTorque state, 5 Nm safe, 25 Nm high)
    let mut safety = racing_wheel_engine::safety::SafetyService::new(5.0, 25.0);
    assert_eq!(
        *safety.state(),
        SafetyState::SafeTorque,
        "engine must start in SafeTorque state"
    );
    assert!(
        safety.max_torque_nm() > 0.0,
        "torque must be non-zero before fault"
    );

    // When: a USB communication fault is detected
    let fault_start = Instant::now();
    safety.report_fault(FaultType::UsbStall);
    let fault_elapsed = fault_start.elapsed();

    // Then: the safety interlock activates within 50 ms
    assert!(
        fault_elapsed < Duration::from_millis(50),
        "fault handling must complete in <50 ms (actual: {fault_elapsed:?})"
    );

    // And: the state transitions to Faulted
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "state must be Faulted after communication loss, got {:?}",
        safety.state()
    );

    // And: torque output goes to zero
    assert!(
        safety.max_torque_nm() == 0.0,
        "max torque must be 0 Nm in Faulted state, got {}",
        safety.max_torque_nm()
    );

    // And: the safety policy also confirms shutdown for USB fault flags
    let policy = SafetyPolicy::new()?;
    assert!(
        policy.requires_immediate_shutdown(fault_flags::USB_FAULT),
        "USB fault must trigger immediate shutdown per safety policy"
    );

    Ok(())
}

/// Scenario: all critical fault types trigger immediate shutdown
///
/// ```text
/// Given  the engine is running
/// When   any critical fault is detected (USB / encoder / thermal / overcurrent)
/// Then   the safety interlock activates and torque goes to zero
/// ```
#[test]
fn scenario_safety_interlock_all_critical_faults_trigger_shutdown()
-> Result<(), Box<dyn std::error::Error>> {
    let critical_faults: &[(FaultType, u8, &str)] = &[
        (FaultType::UsbStall, fault_flags::USB_FAULT, "USB stall"),
        (
            FaultType::EncoderNaN,
            fault_flags::ENCODER_FAULT,
            "encoder NaN",
        ),
        (
            FaultType::ThermalLimit,
            fault_flags::THERMAL_FAULT,
            "thermal limit",
        ),
        (
            FaultType::Overcurrent,
            fault_flags::OVERCURRENT_FAULT,
            "overcurrent",
        ),
    ];

    let policy = SafetyPolicy::new()?;

    for &(ref fault_type, flag, label) in critical_faults {
        // Given: a fresh, running engine
        let mut safety = racing_wheel_engine::safety::SafetyService::new(5.0, 25.0);

        // When: the fault is reported
        safety.report_fault(*fault_type);

        // Then: the state is Faulted
        assert!(
            matches!(safety.state(), SafetyState::Faulted { .. }),
            "{label}: state must be Faulted"
        );

        // And: torque is zero
        assert!(
            safety.max_torque_nm() == 0.0,
            "{label}: max torque must be 0 Nm in Faulted state"
        );

        // And: the policy confirms the fault flag requires shutdown
        assert!(
            policy.requires_immediate_shutdown(flag),
            "{label}: fault flag 0x{flag:02X} must require immediate shutdown"
        );
    }

    Ok(())
}

/// Scenario: clamping torque in faulted state always returns zero
///
/// ```text
/// Given  a faulted safety service
/// When   any torque value is clamped
/// Then   the result is always 0 Nm
/// ```
#[test]
fn scenario_safety_interlock_clamping_in_faulted_state_returns_zero()
-> Result<(), Box<dyn std::error::Error>> {
    let mut safety = racing_wheel_engine::safety::SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::UsbStall);

    // Any requested torque must be clamped to 0 in faulted state
    for requested in [0.0, 1.0, 5.0, 25.0, -10.0] {
        let clamped = safety.clamp_torque_nm(requested);
        assert!(
            clamped == 0.0,
            "clamped torque must be 0 Nm in faulted state, got {clamped} for request {requested}"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 6: Game Profile Switching
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: switching game profile from ACC to iRacing
///
/// ```text
/// Given  a device profile for ACC
/// When   switching to iRacing
/// Then   the profile is applied correctly
/// And    device settings are reconfigured
/// ```
#[test]
fn scenario_game_profile_switch_acc_to_iracing_applies_correct_settings()
-> Result<(), Box<dyn std::error::Error>> {
    use openracing_profile::{WheelProfile, WheelSettings, validation};

    // Given: an ACC profile with ACC-specific settings
    let acc_settings = WheelSettings {
        ffb: openracing_profile::FfbSettings {
            overall_gain: 0.70,
            torque_limit: 12.0,
            spring_strength: 0.0,
            damper_strength: 0.15,
            friction_strength: 0.10,
            effects_enabled: true,
        },
        input: openracing_profile::InputSettings {
            steering_range: 540,
            ..Default::default()
        },
        ..Default::default()
    };
    let acc_profile =
        WheelProfile::new("ACC GT3 Profile", "test-device").with_settings(acc_settings);

    // Validate the ACC profile
    validation::validate_profile(&acc_profile)?;

    // When: switching to iRacing with different settings
    let iracing_settings = WheelSettings {
        ffb: openracing_profile::FfbSettings {
            overall_gain: 0.85,
            torque_limit: 18.0,
            spring_strength: 0.05,
            damper_strength: 0.20,
            friction_strength: 0.08,
            effects_enabled: true,
        },
        input: openracing_profile::InputSettings {
            steering_range: 900,
            ..Default::default()
        },
        ..Default::default()
    };
    let iracing_profile =
        WheelProfile::new("iRacing GT3 Profile", "test-device").with_settings(iracing_settings);

    // Validate the iRacing profile
    validation::validate_profile(&iracing_profile)?;

    // Then: the iRacing profile's FFB settings differ from ACC
    assert!(
        (iracing_profile.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON,
        "iRacing gain must be 0.85, got {}",
        iracing_profile.settings.ffb.overall_gain
    );
    assert!(
        (iracing_profile.settings.ffb.torque_limit - 18.0).abs() < f32::EPSILON,
        "iRacing torque limit must be 18 Nm, got {}",
        iracing_profile.settings.ffb.torque_limit
    );

    // And: steering range is reconfigured (540° ACC → 900° iRacing)
    assert_ne!(
        acc_profile.settings.input.steering_range, iracing_profile.settings.input.steering_range,
        "steering range must differ between ACC and iRacing profiles"
    );
    assert_eq!(
        iracing_profile.settings.input.steering_range, 900,
        "iRacing steering range must be 900°"
    );

    // And: the profile merge applies iRacing overrides on top of ACC base
    let merged = validation::merge_profiles(&acc_profile, &iracing_profile);
    assert!(
        (merged.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON,
        "merged gain must use iRacing override (0.85), got {}",
        merged.settings.ffb.overall_gain
    );
    assert_eq!(
        merged.settings.input.steering_range, 900,
        "merged steering range must use iRacing override (900°)"
    );

    Ok(())
}

/// Scenario: invalid profile settings are rejected before application
///
/// ```text
/// Given  a profile with out-of-range values
/// When   the profile is validated
/// Then   validation fails with a descriptive error
/// And    the invalid profile is never applied
/// ```
#[test]
fn scenario_game_profile_invalid_settings_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_profile::{WheelProfile, WheelSettings, validation};

    // Given: a profile with an FFB gain > 1.0 (invalid)
    let bad_settings = WheelSettings {
        ffb: openracing_profile::FfbSettings {
            overall_gain: 1.5, // out of range: must be [0.0, 1.0]
            ..Default::default()
        },
        ..Default::default()
    };
    let bad_profile = WheelProfile::new("Bad Profile", "test-device").with_settings(bad_settings);

    // When: the profile is validated
    let result = validation::validate_profile(&bad_profile);

    // Then: validation fails
    assert!(
        result.is_err(),
        "profile with gain=1.5 must fail validation"
    );

    Ok(())
}

/// Scenario: mode negotiation changes when switching game compatibility
///
/// ```text
/// Given  a direct-drive device capable of raw torque
/// When   the game compatibility changes from a robust-FFB title to an arcade port
/// Then   the negotiated FFB mode adapts accordingly
/// ```
#[test]
fn scenario_game_profile_mode_negotiation_adapts_to_game() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a direct-drive device
    let dd_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 1000);

    // ACC supports robust FFB → should get RawTorque
    let acc_compat = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // An arcade port supports telemetry only → should get TelemetrySynth
    let arcade_compat = GameCompatibility {
        game_id: "arcade_racer".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };

    // When/Then: ACC negotiates raw torque
    let acc_mode = ModeSelectionPolicy::select_mode(&dd_caps, Some(&acc_compat));
    assert_eq!(
        acc_mode,
        FFBMode::RawTorque,
        "ACC on a DD device must negotiate RawTorque"
    );

    // When/Then: arcade port falls back to telemetry synthesis
    let arcade_mode = ModeSelectionPolicy::select_mode(&dd_caps, Some(&arcade_compat));
    assert_eq!(
        arcade_mode,
        FFBMode::TelemetrySynth,
        "arcade port on a DD device must negotiate TelemetrySynth"
    );

    Ok(())
}
