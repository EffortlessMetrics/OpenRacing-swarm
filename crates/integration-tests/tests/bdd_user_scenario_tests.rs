//! BDD-style acceptance tests for key user-facing scenarios.
//!
//! Each test follows the **Given / When / Then** pattern and uses mocks/stubs
//! for hardware — no real USB devices or running game processes are required.
//!
//! # Scenarios
//!
//! 1. Logitech G29 connected → service start → FFB active
//! 2. Forza running with telemetry → packet arrives → speed/RPM available
//! 3. Safety interlock triggered → fault persists → torque output zero
//! 4. WASM plugin loaded → exceeds resource limits → quarantined
//! 5. Multiple devices connected → game starts → correct device auto-selected

use std::time::{Duration, Instant};

use openracing_telemetry_adapters::{ForzaAdapter, TelemetryAdapter};
use racing_wheel_engine::policies::SafetyPolicy;
use racing_wheel_engine::protocol::fault_flags;
use racing_wheel_engine::safety::{FaultType, SafetyState};
use racing_wheel_engine::{CapabilityNegotiator, FFBMode, GameCompatibility, ModeSelectionPolicy};
use racing_wheel_hid_logitech_protocol::product_ids as logitech_product_ids;
use racing_wheel_integration_tests::logitech_virtual::LogitechScenario;
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 1: Logitech G29 connected → service start → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Logitech G29 is connected (mocked HID device)
/// When   the service starts and initialises the device
/// Then   force feedback should be active
/// And    the device negotiates PID pass-through mode
/// ```
#[test]
fn test_given_logitech_g29_connected_when_service_starts_then_ffb_is_active()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a Logitech G29 is connected (virtual device)
    let mut scenario = LogitechScenario::wheel(logitech_product_ids::G29_PS);

    // When: the service starts and initialises the device
    scenario.initialize()?;

    // Then: FFB should be active — the protocol sent feature reports to the device
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "G29 initialisation must send feature reports to activate FFB"
    );

    // And: the device negotiates PID pass-through mode (G29 is PID-only)
    let g29_caps = DeviceCapabilities::new(
        true,  // supports_pid
        false, // supports_raw_torque_1khz (G29 is PID-only)
        false, // supports_health_stream
        false, // supports_led_bus
        TorqueNm::new(2.8)?,
        4096,
        2000,
    );
    let mode = ModeSelectionPolicy::select_mode(&g29_caps, None);
    assert_eq!(
        mode,
        FFBMode::PidPassthrough,
        "G29 must negotiate PID pass-through mode for FFB"
    );

    // And: the device reports FFB support via its capabilities
    assert!(
        g29_caps.supports_ffb(),
        "G29 must report force feedback support"
    );

    Ok(())
}

/// ```text
/// Given  a Logitech G29 with a capability report
/// When   the engine parses the capabilities
/// Then   the negotiation result confirms FFB is available at the expected rate
/// ```
#[test]
fn test_given_g29_capabilities_when_negotiated_then_ffb_available_at_expected_rate()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a Logitech G29 capabilities report
    let g29_caps =
        DeviceCapabilities::new(true, false, false, false, TorqueNm::new(2.8)?, 4096, 2000);
    let report = CapabilityNegotiator::create_capabilities_report(&g29_caps);

    // When: the engine parses the capability report
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    // Then: the negotiation confirms FFB via PID at the device's max update rate
    let negotiation = CapabilityNegotiator::negotiate_capabilities(&parsed, None);
    assert_eq!(
        negotiation.mode,
        FFBMode::PidPassthrough,
        "G29 negotiation must select PID pass-through"
    );
    assert!(
        negotiation.update_rate_hz > 0.0,
        "negotiated update rate must be positive"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 2: Forza telemetry → packet arrives → speed/RPM available
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a minimal 232-byte Forza Sled telemetry packet.
///
/// Sled offsets:
///   0  – is_race_on (i32)
///   8  – engine_max_rpm (f32)
///  16  – current_rpm (f32)
///  32  – vel_x (f32), 36 – vel_y (f32), 40 – vel_z (f32)
fn make_forza_sled_packet(rpm: f32, vel_x: f32, vel_y: f32, vel_z: f32) -> Vec<u8> {
    let mut data = vec![0u8; 232];
    // is_race_on = 1 (racing)
    data[0..4].copy_from_slice(&1i32.to_le_bytes());
    // engine_max_rpm
    data[8..12].copy_from_slice(&8000.0f32.to_le_bytes());
    // current_rpm
    data[16..20].copy_from_slice(&rpm.to_le_bytes());
    // velocity components (m/s)
    data[32..36].copy_from_slice(&vel_x.to_le_bytes());
    data[36..40].copy_from_slice(&vel_y.to_le_bytes());
    data[40..44].copy_from_slice(&vel_z.to_le_bytes());
    data
}

/// ```text
/// Given  Forza is running and telemetry is enabled
/// When   a Sled telemetry packet arrives with RPM=5000, speed≈44.7 m/s
/// Then   the adapter parses speed and RPM into the normalized format
/// And    both values are within physically valid ranges
/// ```
#[test]
fn test_given_forza_running_when_packet_arrives_then_speed_and_rpm_available()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: Forza telemetry adapter is active
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.game_id(), "forza_motorsport");

    // And: a Sled packet with RPM = 5000, velocity = (30, 5, 33) m/s
    //      speed magnitude = sqrt(30² + 5² + 33²) ≈ 44.94 m/s
    let packet = make_forza_sled_packet(5000.0, 30.0, 5.0, 33.0);

    // When: the telemetry packet is normalized
    let telemetry = adapter.normalize(&packet)?;

    // Then: RPM is available and correct
    assert!(
        (telemetry.rpm - 5000.0).abs() < 1.0,
        "RPM must be ~5000, got {}",
        telemetry.rpm
    );

    // And: speed is available and derived from the velocity vector
    let expected_speed = (30.0f32.powi(2) + 5.0f32.powi(2) + 33.0f32.powi(2)).sqrt();
    assert!(
        (telemetry.speed_ms - expected_speed).abs() < 0.5,
        "speed_ms must be ~{expected_speed:.1}, got {}",
        telemetry.speed_ms
    );

    // And: values are within physically valid ranges
    assert!(
        telemetry.rpm >= 0.0 && telemetry.rpm <= 20000.0,
        "RPM must be in [0, 20000], got {}",
        telemetry.rpm
    );
    assert!(
        telemetry.speed_ms >= 0.0 && telemetry.speed_ms <= 500.0,
        "speed_ms must be in [0, 500], got {}",
        telemetry.speed_ms
    );

    Ok(())
}

/// ```text
/// Given  Forza adapter is running
/// When   an empty or truncated packet arrives
/// Then   normalize returns an error (no panic or undefined behaviour)
/// ```
#[test]
fn test_given_forza_running_when_truncated_packet_arrives_then_error_returned()
-> Result<(), Box<dyn std::error::Error>> {
    let adapter = ForzaAdapter::new();

    // When/Then: empty packet returns error
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must be rejected"
    );

    // When/Then: too-short packet returns error
    assert!(
        adapter.normalize(&[0u8; 100]).is_err(),
        "100-byte packet is shorter than Sled (232) and must be rejected"
    );

    Ok(())
}

/// ```text
/// Given  Forza is running but the race is paused (is_race_on == 0)
/// When   a Sled packet arrives
/// Then   speed and RPM are zero (idle state)
/// ```
#[test]
fn test_given_forza_race_not_active_when_packet_arrives_then_values_are_zero()
-> Result<(), Box<dyn std::error::Error>> {
    let adapter = ForzaAdapter::new();
    // Packet with is_race_on = 0 (paused/menu)
    let data = vec![0u8; 232];
    let telemetry = adapter.normalize(&data)?;

    assert!(
        telemetry.speed_ms.abs() < f32::EPSILON,
        "speed must be zero when race is not active"
    );
    assert!(
        telemetry.rpm.abs() < f32::EPSILON,
        "RPM must be zero when race is not active"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 3: Safety interlock triggered → fault persists → torque zero
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a safety interlock is triggered (USB communication fault)
/// When   the fault persists (state remains Faulted)
/// Then   torque output is zero and any requested torque is clamped to zero
/// ```
#[test]
fn test_given_safety_interlock_triggered_when_fault_persists_then_torque_is_zero()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: engine running normally
    let mut safety = racing_wheel_engine::safety::SafetyService::new(5.0, 25.0);
    assert_eq!(*safety.state(), SafetyState::SafeTorque);
    assert!(
        safety.max_torque_nm() > 0.0,
        "torque must be non-zero before fault"
    );

    // When: a USB communication fault is triggered
    safety.report_fault(FaultType::UsbStall);

    // Then: state transitions to Faulted
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "state must be Faulted after USB stall, got {:?}",
        safety.state()
    );

    // And: torque output is zero
    assert!(
        safety.max_torque_nm() == 0.0,
        "max torque must be 0 Nm in Faulted state, got {}",
        safety.max_torque_nm()
    );

    // And: the fault persists — requesting any torque value always yields zero
    for requested in [0.0, 1.0, 5.0, 25.0, -10.0] {
        let clamped = safety.clamp_torque_nm(requested);
        assert!(
            clamped == 0.0,
            "clamped torque must be 0 Nm while fault persists, got {clamped} for request {requested}"
        );
    }

    // And: the safety policy also confirms shutdown for USB fault flags
    let policy = SafetyPolicy::new()?;
    assert!(
        policy.requires_immediate_shutdown(fault_flags::USB_FAULT),
        "USB fault flag must trigger immediate shutdown per safety policy"
    );

    Ok(())
}

/// ```text
/// Given  the motor is running at safe torque
/// When   a thermal fault is detected and persists
/// Then   the motor shutdown completes within 50ms
/// And    torque output remains zero while faulted
/// ```
#[test]
fn test_given_motor_running_when_thermal_fault_persists_then_shutdown_within_50ms()
-> Result<(), Box<dyn std::error::Error>> {
    let mut safety = racing_wheel_engine::safety::SafetyService::new(5.0, 25.0);

    // When: thermal fault is detected
    let fault_start = Instant::now();
    safety.report_fault(FaultType::ThermalLimit);
    let fault_elapsed = fault_start.elapsed();

    // Then: shutdown completes well within 50ms
    assert!(
        fault_elapsed < Duration::from_millis(50),
        "fault handling must complete in <50ms (actual: {fault_elapsed:?})"
    );

    // And: torque remains zero while fault persists
    assert!(
        safety.max_torque_nm() == 0.0,
        "torque must be 0 Nm while thermal fault persists"
    );

    // And: the wire-level safety policy also triggers for thermal faults
    let policy = SafetyPolicy::new()?;
    assert!(
        policy.requires_immediate_shutdown(fault_flags::THERMAL_FAULT),
        "thermal fault flag must trigger immediate shutdown"
    );

    Ok(())
}

/// ```text
/// Given  the engine is running normally
/// When   every critical fault type is raised one by one
/// Then   each fault results in torque output going to zero
/// ```
#[test]
fn test_given_engine_running_when_any_critical_fault_then_torque_zero()
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

        // When: the critical fault is raised
        safety.report_fault(*fault_type);

        // Then: torque is zero
        assert!(
            safety.max_torque_nm() == 0.0,
            "{label}: torque must be 0 Nm after fault"
        );

        // And: the safety policy confirms the fault flag requires shutdown
        assert!(
            policy.requires_immediate_shutdown(flag),
            "{label}: fault flag 0x{flag:02X} must require shutdown"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 4: WASM plugin → exceeds resource limits → quarantined
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a WASM plugin is loaded and managed by the quarantine system
/// When   it exceeds resource limits repeatedly (budget violations)
/// Then   it is quarantined and further execution is blocked
/// ```
#[test]
fn test_given_wasm_plugin_loaded_when_exceeds_resource_limits_then_quarantined()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a quarantine manager with strict policy (quarantine after 3 budget violations)
    let policy = QuarantinePolicy {
        max_crashes: 3,
        max_budget_violations: 3,
        violation_window_minutes: 60,
        quarantine_duration_minutes: 60,
        max_escalation_levels: 5,
    };
    let mut manager = QuarantineManager::new(policy);
    let plugin_id = uuid::Uuid::new_v4();

    // And: the plugin is not quarantined initially
    assert!(
        !manager.is_quarantined(plugin_id),
        "plugin must not be quarantined before any violations"
    );

    // When: the plugin exceeds resource limits 3 times (budget violations)
    for i in 1..=3 {
        manager.record_violation(
            plugin_id,
            ViolationType::BudgetViolation,
            format!("exceeded fuel limit: call {i}"),
        )?;
    }

    // Then: the plugin is quarantined
    assert!(
        manager.is_quarantined(plugin_id),
        "plugin must be quarantined after 3 budget violations"
    );

    // And: the quarantine state records the violations
    let state = manager
        .get_quarantine_state(plugin_id)
        .ok_or("quarantine state must exist for the plugin")?;
    assert!(
        state.is_quarantined,
        "quarantine state must reflect active quarantine"
    );
    assert_eq!(
        state.total_budget_violations, 3,
        "must record exactly 3 budget violations"
    );

    Ok(())
}

/// ```text
/// Given  a WASM plugin that repeatedly crashes
/// When   the crash count exceeds the policy threshold
/// Then   it is quarantined
/// And    the quarantine can be released manually
/// ```
#[test]
fn test_given_wasm_plugin_crashes_when_threshold_exceeded_then_quarantined_and_releasable()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = QuarantinePolicy {
        max_crashes: 2,
        max_budget_violations: 10,
        violation_window_minutes: 60,
        quarantine_duration_minutes: 30,
        max_escalation_levels: 3,
    };
    let mut manager = QuarantineManager::new(policy);
    let plugin_id = uuid::Uuid::new_v4();

    // When: plugin crashes twice
    for _ in 0..2 {
        manager.record_violation(
            plugin_id,
            ViolationType::Crash,
            "WASM trap: unreachable instruction".to_string(),
        )?;
    }

    // Then: plugin is quarantined
    assert!(
        manager.is_quarantined(plugin_id),
        "plugin must be quarantined after 2 crashes"
    );

    // And: the quarantine can be released manually
    manager.release_from_quarantine(plugin_id)?;
    assert!(
        !manager.is_quarantined(plugin_id),
        "plugin must not be quarantined after manual release"
    );

    Ok(())
}

/// ```text
/// Given  a WASM plugin with a single budget violation
/// When   the violation count is below the quarantine threshold
/// Then   the plugin remains active (not quarantined)
/// ```
#[test]
fn test_given_wasm_plugin_when_below_violation_threshold_then_not_quarantined()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = QuarantinePolicy::default(); // max_budget_violations = 10
    let mut manager = QuarantineManager::new(policy);
    let plugin_id = uuid::Uuid::new_v4();

    // When: a single budget violation occurs
    manager.record_violation(
        plugin_id,
        ViolationType::BudgetViolation,
        "exceeded fuel limit once".to_string(),
    )?;

    // Then: the plugin is NOT quarantined (threshold not reached)
    assert!(
        !manager.is_quarantined(plugin_id),
        "plugin must not be quarantined after a single violation"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 5: Multiple devices → game starts → correct device auto-selected
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  multiple devices are connected (G29, Fanatec CSL DD, Simucube 2 Pro)
/// When   a game starts with robust FFB support (e.g. ACC)
/// Then   each device auto-selects the correct FFB mode
/// And    the highest-capability device negotiates raw torque
/// ```
#[test]
fn test_given_multiple_devices_when_game_starts_then_correct_device_auto_selected()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: three devices of increasing capability
    let g29 = DeviceCapabilities::new(true, false, false, false, TorqueNm::new(2.8)?, 4096, 2000);
    let csl_dd = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000);
    let sc2_pro = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500);

    // And: a game with robust FFB (ACC)
    let acc = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // When: each device negotiates FFB mode for ACC
    let g29_mode = ModeSelectionPolicy::select_mode(&g29, Some(&acc));
    let csl_dd_mode = ModeSelectionPolicy::select_mode(&csl_dd, Some(&acc));
    let sc2_pro_mode = ModeSelectionPolicy::select_mode(&sc2_pro, Some(&acc));

    // Then: the G29 falls back to PID (no raw torque support)
    assert_eq!(
        g29_mode,
        FFBMode::PidPassthrough,
        "G29 must use PID pass-through (no raw torque support)"
    );

    // And: the CSL DD and SC2 Pro auto-select raw torque
    assert_eq!(
        csl_dd_mode,
        FFBMode::RawTorque,
        "CSL DD must auto-select raw torque for ACC"
    );
    assert_eq!(
        sc2_pro_mode,
        FFBMode::RawTorque,
        "SC2 Pro must auto-select raw torque for ACC"
    );

    Ok(())
}

/// ```text
/// Given  multiple devices connected
/// When   a game starts that only supports telemetry synthesis (arcade port)
/// Then   all devices fall back to TelemetrySynth mode
/// ```
#[test]
fn test_given_multiple_devices_when_arcade_game_starts_then_telemetry_synth_selected()
-> Result<(), Box<dyn std::error::Error>> {
    let csl_dd = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000);
    let sc2_pro = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500);

    // Given: an arcade racing port with no robust FFB
    let arcade = GameCompatibility {
        game_id: "arcade_racer".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };

    // When: devices negotiate FFB mode for the arcade game
    let csl_mode = ModeSelectionPolicy::select_mode(&csl_dd, Some(&arcade));
    let sc2_mode = ModeSelectionPolicy::select_mode(&sc2_pro, Some(&arcade));

    // Then: both devices fall back to TelemetrySynth
    assert_eq!(
        csl_mode,
        FFBMode::TelemetrySynth,
        "CSL DD must use TelemetrySynth for an arcade port"
    );
    assert_eq!(
        sc2_mode,
        FFBMode::TelemetrySynth,
        "SC2 Pro must use TelemetrySynth for an arcade port"
    );

    Ok(())
}

/// ```text
/// Given  multiple devices with different capabilities
/// When   the engine enumerates all devices via capability reports
/// Then   each device's capabilities are correctly round-tripped
/// And    the correct FFB mode is negotiated per device
/// ```
#[test]
fn test_given_multiple_devices_when_enumerated_then_capabilities_round_trip_correctly()
-> Result<(), Box<dyn std::error::Error>> {
    let devices: &[(&str, DeviceCapabilities, FFBMode)] = &[
        (
            "Logitech G29",
            DeviceCapabilities::new(true, false, false, false, TorqueNm::new(2.8)?, 4096, 2000),
            FFBMode::PidPassthrough,
        ),
        (
            "Fanatec CSL DD",
            DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000),
            FFBMode::RawTorque,
        ),
        (
            "Simucube 2 Pro",
            DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500),
            FFBMode::RawTorque,
        ),
    ];

    for (label, caps, expected_mode) in devices {
        // Capability report round-trip
        let report = CapabilityNegotiator::create_capabilities_report(caps);
        let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        assert!(
            (parsed.max_torque.value() - caps.max_torque.value()).abs() < 0.1,
            "{label}: max torque round-trip failed"
        );

        // And: correct mode negotiated
        let mode = ModeSelectionPolicy::select_mode(&parsed, None);
        assert_eq!(
            mode, *expected_mode,
            "{label}: expected {expected_mode:?}, got {mode:?}"
        );
    }

    Ok(())
}
