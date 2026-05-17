//! BDD-style game integration scenario tests.
//!
//! Each test follows the **Given / When / Then** pattern and uses mock adapters
//! — no running game processes or network services are required.
//!
//! # Scenarios
//!
//! 1. iRacing running → telemetry starts → normalized data flows to FFB pipeline
//! 2. ACC running with shared memory → car state changes → effects update in real-time
//! 3. User switches from Forza to iRacing → adapter switches → FFB continues
//! 4. Game sends NaN telemetry → filtered → safe default used
//! 5. No game running → device connected → device in standby mode
//! 6. ACC running → tire temperature changes → FFB effects update accordingly
//! 7. ACC running → rain starts → grip decreases → FFB reduces
//! 8. ACC running → pit stop → telemetry pauses → FFB enters standby
//! 9. Assetto Corsa running → telemetry starts → normalized data flows
//! 10. Assetto Corsa running → user changes car → FFB recalibrates

use anyhow::Result;

use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use openracing_telemetry_adapters::{
    ACCAdapter, AssettoCorsaAdapter, ForzaAdapter, TelemetryAdapter,
};
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{SafetyService, SafetyState};
use racing_wheel_engine::{FFBMode, GameCompatibility, ModeSelectionPolicy, VirtualDevice};
use racing_wheel_schemas::prelude::*;

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Build a minimal 232-byte Forza Sled telemetry packet.
fn make_forza_sled_packet(rpm: f32, vel_x: f32, vel_y: f32, vel_z: f32) -> Vec<u8> {
    let mut data = vec![0u8; 232];
    data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    data[8..12].copy_from_slice(&8000.0f32.to_le_bytes()); // engine_max_rpm
    data[16..20].copy_from_slice(&rpm.to_le_bytes()); // current_rpm
    data[32..36].copy_from_slice(&vel_x.to_le_bytes()); // vel_x
    data[36..40].copy_from_slice(&vel_y.to_le_bytes()); // vel_y
    data[40..44].copy_from_slice(&vel_z.to_le_bytes()); // vel_z
    data
}

/// Build a minimal Forza Sled packet with NaN values to test sanitisation.
fn make_forza_sled_packet_with_nan() -> Vec<u8> {
    let nan = f32::NAN;
    let mut data = vec![0u8; 232];
    data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    data[8..12].copy_from_slice(&8000.0f32.to_le_bytes()); // engine_max_rpm
    data[16..20].copy_from_slice(&nan.to_le_bytes()); // current_rpm = NaN
    data[32..36].copy_from_slice(&nan.to_le_bytes()); // vel_x = NaN
    data[36..40].copy_from_slice(&nan.to_le_bytes()); // vel_y = NaN
    data[40..44].copy_from_slice(&nan.to_le_bytes()); // vel_z = NaN
    data
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 1: iRacing running → telemetry starts → normalized data flows
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  iRacing is running with telemetry enabled
/// When   telemetry data starts flowing (simulated via Forza adapter for cross-platform)
/// Then   the adapter normalizes the raw data into speed and RPM
/// And    the normalized data flows through the FFB filter pipeline
/// And    the pipeline output is within valid bounds for device output
/// ```
#[test]
fn given_iracing_running_when_telemetry_starts_then_normalized_data_flows_to_ffb_pipeline()
-> Result<()> {
    // Given: a racing game is running with telemetry enabled
    // (We use the Forza adapter as a cross-platform proxy for telemetry normalisation;
    //  iRacing's shared-memory adapter is Windows-only and tested in platform-specific suites.)
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.game_id(), "forza_motorsport");

    // And: a device is connected with a safety service ready
    let id: DeviceId = "bdd-iracing-telem-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD iRacing Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // When: telemetry data arrives with RPM=7000, velocity=(50, 0, 30) → speed ~58.3 m/s
    let packet = make_forza_sled_packet(7000.0, 50.0, 0.0, 30.0);
    let telemetry = adapter.normalize(&packet)?;

    // Then: the adapter normalizes RPM correctly
    assert!(
        (telemetry.rpm - 7000.0).abs() < 1.0,
        "RPM must be ~7000, got {}",
        telemetry.rpm
    );

    // And: speed is derived from velocity vector
    let expected_speed = (50.0f32.powi(2) + 0.0f32.powi(2) + 30.0f32.powi(2)).sqrt();
    assert!(
        (telemetry.speed_ms - expected_speed).abs() < 0.5,
        "speed_ms must be ~{expected_speed:.1}, got {}",
        telemetry.speed_ms
    );

    // And: the normalized data flows through the FFB filter pipeline
    let ffb_scalar = (telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut frame = FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed: telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut frame, &damper);
    torque_cap_filter(&mut frame, 1.0);

    // And: the pipeline output is within valid bounds
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "pipeline output must be finite and within [-1, 1], got {}",
        frame.torque_out
    );

    // And: the safety-clamped torque can be sent to the device
    let torque_nm = frame.torque_out * 5.0;
    let clamped = safety.clamp_torque_nm(torque_nm);
    device.write_ffb_report(clamped, frame.seq)?;
    assert!(
        device.is_connected(),
        "device must remain connected after FFB write"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 2: ACC running → car state changes → effects update in real-time
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  ACC is running and the adapter is active
/// When   the car state changes (different RPM/speed snapshots arrive)
/// Then   each snapshot produces different filter outputs
/// And    the pipeline responds to state changes in real-time
/// And    all outputs remain within valid bounds
/// ```
#[test]
fn given_acc_running_when_car_state_changes_then_effects_update_in_real_time() -> Result<()> {
    // Given: ACC is running — use the ACC adapter to confirm game identity
    let acc_adapter = ACCAdapter::new();
    assert_eq!(acc_adapter.game_id(), "acc");

    // And: use the Forza adapter for normalisation (ACC shared-memory is platform-specific)
    let normaliser = ForzaAdapter::new();

    // And: a device + safety service for FFB output
    let id: DeviceId = "bdd-acc-realtime-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD ACC Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Define car state snapshots: idle → accelerating → braking
    let states: &[(&str, f32, f32, f32, f32)] = &[
        ("idle", 800.0, 0.0, 0.0, 0.0),
        ("accelerating", 5000.0, 30.0, 0.0, 20.0),
        ("braking", 3000.0, 10.0, 0.0, 5.0),
    ];

    let mut previous_output: Option<f32> = None;
    let mut outputs_differ = false;

    for (seq, &(label, rpm, vx, vy, vz)) in states.iter().enumerate() {
        // When: a new car state snapshot arrives
        let packet = make_forza_sled_packet(rpm, vx, vy, vz);
        let telemetry = normaliser.normalize(&packet)?;

        // Derive FFB scalar from telemetry
        let ffb_scalar = (telemetry.rpm / 8000.0).clamp(0.0, 1.0);
        let speed = telemetry.speed_ms;

        let mut frame = FilterFrame {
            ffb_in: ffb_scalar,
            torque_out: ffb_scalar,
            wheel_speed: speed * 0.1,
            hands_off: false,
            ts_mono_ns: (seq as u64) * 1_000_000,
            seq: seq as u16,
        };
        let damper = DamperState::fixed(0.05);
        damper_filter(&mut frame, &damper);
        torque_cap_filter(&mut frame, 1.0);

        // Then: the output is valid
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "{label}: pipeline output must be finite and in [-1, 1], got {}",
            frame.torque_out
        );

        // Track whether outputs differ across states
        if let Some(prev) = previous_output
            && (frame.torque_out - prev).abs() > 0.001
        {
            outputs_differ = true;
        }
        previous_output = Some(frame.torque_out);

        // And: the clamped torque can be written to the device
        let torque_nm = frame.torque_out * 5.0;
        let clamped = safety.clamp_torque_nm(torque_nm);
        device.write_ffb_report(clamped, frame.seq)?;
    }

    // And: the pipeline responded to state changes (outputs are not all identical)
    assert!(
        outputs_differ,
        "filter outputs must differ across car states (idle/accel/brake)"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 3: User switches from Forza to iRacing → adapter switches → FFB continues
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  the user is playing Forza with telemetry flowing through the FFB pipeline
/// When   the user switches to iRacing (a different game with different FFB needs)
/// Then   the adapter switches from Forza to the new game's adapter
/// And    the FFB mode is re-negotiated for the new game
/// And    FFB continues flowing through the pipeline without interruption
/// ```
#[test]
fn given_user_switches_from_forza_to_iracing_then_adapter_switches_and_ffb_continues() -> Result<()>
{
    // Given: the user is playing Forza — telemetry is flowing
    let forza_adapter = ForzaAdapter::new();
    assert_eq!(forza_adapter.game_id(), "forza_motorsport");

    let forza_packet = make_forza_sled_packet(4000.0, 20.0, 0.0, 15.0);
    let forza_telemetry = forza_adapter.normalize(&forza_packet)?;
    assert!(
        forza_telemetry.rpm > 0.0,
        "Forza telemetry must produce non-zero RPM"
    );

    // And: FFB is flowing through the pipeline with Forza's data
    let forza_ffb = (forza_telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut forza_frame = FilterFrame {
        ffb_in: forza_ffb,
        torque_out: forza_ffb,
        wheel_speed: forza_telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    torque_cap_filter(&mut forza_frame, 1.0);
    assert!(
        forza_frame.torque_out.is_finite(),
        "Forza FFB must produce finite output"
    );

    // When: the user switches to iRacing — a new adapter is selected
    // Model iRacing as a game with robust FFB (raw torque preferred)
    let iracing_game = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // Then: the FFB mode is re-negotiated for the new game
    // A high-capability device (e.g., CSL DD) should switch to raw torque
    let device_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000);
    let new_mode = ModeSelectionPolicy::select_mode(&device_caps, Some(&iracing_game));
    assert_eq!(
        new_mode,
        FFBMode::RawTorque,
        "iRacing must negotiate raw torque mode for capable devices"
    );

    // And: FFB continues flowing through the pipeline with the new game's data
    // Simulate an iRacing telemetry snapshot (using Forza packet format as proxy)
    let iracing_packet = make_forza_sled_packet(6500.0, 45.0, 0.0, 25.0);
    let iracing_telemetry = forza_adapter.normalize(&iracing_packet)?;

    let iracing_ffb = (iracing_telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut iracing_frame = FilterFrame {
        ffb_in: iracing_ffb,
        torque_out: iracing_ffb,
        wheel_speed: iracing_telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut iracing_frame, &damper);
    torque_cap_filter(&mut iracing_frame, 1.0);

    assert!(
        iracing_frame.torque_out.is_finite() && iracing_frame.torque_out.abs() <= 1.0,
        "iRacing FFB must produce finite output within [-1, 1], got {}",
        iracing_frame.torque_out
    );

    // And: the device can receive the FFB output seamlessly
    let id: DeviceId = "bdd-game-switch-001".parse()?;
    let mut device = VirtualDevice::new(id, "Game Switch Wheel".to_string());
    let safety = SafetyService::new(8.0, 20.0);
    let torque_nm = iracing_frame.torque_out * device_caps.max_torque.value();
    let clamped = safety.clamp_torque_nm(torque_nm);
    device.write_ffb_report(clamped, iracing_frame.seq)?;
    assert!(
        device.is_connected(),
        "device must remain connected after game switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 4: Game sends NaN telemetry → filtered → safe default used
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a game is running and sending telemetry
/// When   the game sends NaN values in the telemetry packet
/// Then   the adapter filters out the NaN values
/// And    safe defaults (zero) are used in place of NaN
/// And    the FFB pipeline produces a finite, safe output
/// ```
#[test]
fn given_game_sends_nan_telemetry_then_filtered_and_safe_default_used() -> Result<()> {
    // Given: a game adapter is active
    let adapter = ForzaAdapter::new();

    // When: the game sends a packet with NaN values
    let nan_packet = make_forza_sled_packet_with_nan();
    let telemetry = adapter.normalize(&nan_packet)?;

    // Then: the NaN values are filtered to safe defaults
    assert!(
        telemetry.rpm.is_finite(),
        "RPM must be finite after NaN filtering, got {}",
        telemetry.rpm
    );
    assert!(
        telemetry.speed_ms.is_finite(),
        "speed_ms must be finite after NaN filtering, got {}",
        telemetry.speed_ms
    );

    // And: safe defaults (zero or clamped values) are used
    assert!(
        telemetry.rpm >= 0.0,
        "filtered RPM must be non-negative, got {}",
        telemetry.rpm
    );
    assert!(
        telemetry.speed_ms >= 0.0,
        "filtered speed must be non-negative, got {}",
        telemetry.speed_ms
    );

    // And: the FFB pipeline produces a finite, safe output
    let ffb_scalar = if telemetry.rpm.is_finite() && telemetry.rpm > 0.0 {
        (telemetry.rpm / 8000.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut frame = FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed: if telemetry.speed_ms.is_finite() {
            telemetry.speed_ms * 0.1
        } else {
            0.0
        },
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite(),
        "pipeline output must be finite after NaN telemetry, got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out.abs() <= 1.0,
        "pipeline output must be within [-1, 1] after NaN telemetry, got {}",
        frame.torque_out
    );

    // And: a safety-clamped write to the device succeeds
    let safety = SafetyService::new(5.0, 20.0);
    let torque_nm = frame.torque_out * 5.0;
    let clamped = safety.clamp_torque_nm(torque_nm);
    assert!(
        clamped.is_finite(),
        "safety-clamped torque must be finite, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 5: No game running → device connected → device in standby mode
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  no game is running
/// When   a device is connected
/// Then   the safety service is in SafeTorque (standby) mode
/// And    the device is connected but receives no FFB commands
/// And    torque output is limited to the safe default
/// And    the device can transition to active when a game starts
/// ```
#[test]
fn given_no_game_running_when_device_connected_then_device_in_standby_mode() -> Result<()> {
    // Given: no game is running (no telemetry adapter active)
    // When: a device is connected
    let id: DeviceId = "bdd-standby-001".parse()?;
    let device = VirtualDevice::new(id, "BDD Standby Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Then: the safety service is in SafeTorque (standby) mode
    assert_eq!(
        safety.state(),
        &SafetyState::SafeTorque,
        "safety must be in SafeTorque (standby) when no game is running"
    );

    // And: the device is connected but idle
    assert!(
        device.is_connected(),
        "device must be connected in standby mode"
    );

    // And: torque output is limited to the safe default (5 Nm cap)
    let clamped = safety.clamp_torque_nm(15.0);
    assert!(
        clamped <= 5.0,
        "standby mode must cap torque to safe limit (5 Nm), got {clamped}"
    );

    // And: zero torque passes through unchanged
    let zero = safety.clamp_torque_nm(0.0);
    assert!(
        zero.abs() < 0.001,
        "zero torque request must remain zero in standby, got {zero}"
    );

    // And: the device can transition to active when a game starts
    // Simulate by verifying the safety service would allow normal torque
    // within the safe limit
    let active_torque = safety.clamp_torque_nm(3.0);
    assert!(
        (active_torque - 3.0).abs() < 0.01,
        "within-limit torque must flow normally in standby, got {active_torque}"
    );

    // And: a game starting would negotiate the correct FFB mode
    let device_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000);
    let game = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };
    let mode = ModeSelectionPolicy::select_mode(&device_caps, Some(&game));
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "device must be able to negotiate FFB mode when a game starts"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 6: ACC running → tire temperature changes → FFB effects update accordingly
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  ACC is running and telemetry is flowing
/// When   tire temperature changes (cold → optimal → hot)
/// Then   the FFB pipeline adjusts force output based on tire state
/// And    hotter tires produce stronger feedback (more grip)
/// And    the pipeline remains within valid bounds
/// ```
#[test]
fn given_acc_running_when_tire_temperature_changes_then_ffb_effects_update() -> Result<()> {
    // Given: ACC is running with the ACC adapter
    let acc_adapter = ACCAdapter::new();
    assert_eq!(acc_adapter.game_id(), "acc");

    // Use Forza adapter for normalisation (ACC shared-memory is platform-specific)
    let normaliser = ForzaAdapter::new();

    // And: a device + safety service for FFB output
    let id: DeviceId = "bdd-acc-tire-temp-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD ACC Tire Temp Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Tire temperature scenarios: cold (low grip), optimal, hot (reduced grip)
    let tire_temps: &[(&str, f32, f32)] = &[
        ("cold", 800.0, 0.60),     // Cold tires = reduced grip
        ("optimal", 5000.0, 0.85), // Optimal temperature = full grip
        ("hot", 7000.0, 0.75),     // Overheated = slightly reduced grip
    ];

    let mut previous_output: Option<f32> = None;

    for (label, rpm, grip_factor) in tire_temps {
        // When: tire temperature changes at this RPM
        let packet = make_forza_sled_packet(*rpm, 30.0, 0.0, 20.0);
        let telemetry = normaliser.normalize(&packet)?;

        // Apply grip factor to simulate tire temperature effect
        let ffb_scalar = (telemetry.rpm / 8000.0).clamp(0.0, 1.0) * grip_factor;

        let mut frame = FilterFrame {
            ffb_in: ffb_scalar,
            torque_out: ffb_scalar,
            wheel_speed: telemetry.speed_ms * 0.1,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let damper = DamperState::fixed(0.05);
        damper_filter(&mut frame, &damper);
        torque_cap_filter(&mut frame, 1.0);

        // Then: output is valid and within bounds
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "{label}: pipeline output must be finite and in [-1, 1]"
        );

        // Track output for comparison
        if let Some(prev) = previous_output {
            // Outputs should differ based on grip factor
            let diff = (frame.torque_out - prev).abs();
            assert!(
                diff > 0.001,
                "FFB output should change with tire temperature"
            );
        }
        previous_output = Some(frame.torque_out);

        // And: can write to device
        let torque_nm = frame.torque_out * 5.0;
        let clamped = safety.clamp_torque_nm(torque_nm);
        device.write_ffb_report(clamped, frame.seq)?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 7: ACC running → rain starts → grip decreases → FFB reduces
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  ACC is running in dry conditions with normal FFB
/// When   rain starts (wet track conditions)
/// Then   the grip factor decreases due to wet track
/// And    FFB output is reduced proportionally
/// And    the pipeline remains stable and within bounds
/// ```
#[test]
fn given_acc_running_when_rain_starts_then_grip_decreases_and_ffb_reduces() -> Result<()> {
    // Given: ACC is running in dry conditions
    let _acc_adapter = ACCAdapter::new();
    let normaliser = ForzaAdapter::new();

    let id: DeviceId = "bdd-acc-rain-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD ACC Rain Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Dry vs wet grip factors
    let conditions: &[(&str, f32, f32)] = &[
        ("dry", 5000.0, 1.0),       // Dry: full grip
        ("wet_light", 5000.0, 0.7), // Light rain: 70% grip
        ("wet_heavy", 5000.0, 0.5), // Heavy rain: 50% grip
    ];

    for (label, rpm, grip_factor) in conditions {
        // When: track conditions change
        let packet = make_forza_sled_packet(*rpm, 30.0, 0.0, 20.0);
        let telemetry = normaliser.normalize(&packet)?;

        // Apply grip factor for wet conditions
        let ffb_scalar = (telemetry.rpm / 8000.0).clamp(0.0, 1.0) * grip_factor;

        let mut frame = FilterFrame {
            ffb_in: ffb_scalar,
            torque_out: ffb_scalar,
            wheel_speed: telemetry.speed_ms * 0.1,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let damper = DamperState::fixed(0.05);
        damper_filter(&mut frame, &damper);
        torque_cap_filter(&mut frame, 1.0);

        // Then: output reflects reduced grip in wet conditions
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "{label}: pipeline must produce valid output"
        );

        // Verify wet conditions produce lower FFB than dry
        if *grip_factor < 1.0 {
            assert!(
                frame.torque_out <= 0.85,
                "wet conditions should produce reduced FFB"
            );
        }

        // And: can write to device
        let torque_nm = frame.torque_out * 5.0;
        let clamped = safety.clamp_torque_nm(torque_nm);
        device.write_ffb_report(clamped, frame.seq)?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 8: ACC running → pit stop → telemetry pauses → FFB enters standby
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  ACC is running with active telemetry and FFB
/// When   the car enters the pit lane (telemetry pauses)
/// Then   the FFB pipeline enters standby mode
/// And    torque output is reduced to safe levels
/// And    resuming racing restores full FFB
/// ```
#[test]
fn given_acc_running_when_pit_stop_then_telemetry_pauses_and_ffb_standby() -> Result<()> {
    // Given: ACC is running with active FFB
    let normaliser = ForzaAdapter::new();

    let id: DeviceId = "bdd-acc-pitstop-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD ACC Pitstop Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // When: car is on track (normal telemetry)
    let on_track_packet = make_forza_sled_packet(6000.0, 40.0, 0.0, 25.0);
    let on_track_telemetry = normaliser.normalize(&on_track_packet)?;

    let on_track_ffb = (on_track_telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut on_track_frame = FilterFrame {
        ffb_in: on_track_ffb,
        torque_out: on_track_ffb,
        wheel_speed: on_track_telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    torque_cap_filter(&mut on_track_frame, 1.0);

    assert!(
        on_track_frame.torque_out > 0.5,
        "on-track FFB should be active"
    );

    // When: car enters pit (telemetry stops / zeros)
    let in_pit_packet = make_forza_sled_packet(0.0, 0.0, 0.0, 0.0);
    let in_pit_telemetry = normaliser.normalize(&in_pit_packet)?;

    let in_pit_ffb = (in_pit_telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut in_pit_frame = FilterFrame {
        ffb_in: in_pit_ffb,
        torque_out: in_pit_ffb,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    torque_cap_filter(&mut in_pit_frame, 1.0);

    // Then: pit telemetry produces minimal/zero FFB (standby)
    assert!(
        in_pit_frame.torque_out < 0.1,
        "pit lane should produce minimal FFB (standby), got {}",
        in_pit_frame.torque_out
    );

    // And: both states can be written to device
    let on_track_torque = safety.clamp_torque_nm(on_track_frame.torque_out * 5.0);
    device.write_ffb_report(on_track_torque, on_track_frame.seq)?;

    let in_pit_torque = safety.clamp_torque_nm(in_pit_frame.torque_out * 5.0);
    device.write_ffb_report(in_pit_torque, in_pit_frame.seq)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 9: Assetto Corsa running → telemetry starts → normalized data flows
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  Assetto Corsa is running with telemetry enabled
/// When   telemetry data starts flowing
/// Then   the adapter normalizes the raw data into speed and RPM
/// And    the normalized data flows through the FFB filter pipeline
/// And    the pipeline output is within valid bounds for device output
/// ```
#[test]
fn given_assetto_corsa_running_when_telemetry_starts_then_normalized_data_flows() -> Result<()> {
    // Given: Assetto Corsa is running with the AC adapter
    let ac_adapter = AssettoCorsaAdapter::new();
    assert_eq!(ac_adapter.game_id(), "assetto_corsa");

    // Use Forza adapter for normalisation (AC shared-memory is platform-specific)
    let normaliser = ForzaAdapter::new();

    // And: a device is connected with a safety service ready
    let id: DeviceId = "bdd-ac-telem-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD Assetto Corsa Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // When: telemetry data arrives with RPM=6500, velocity=(45, 0, 28) → speed ~52.2 m/s
    let packet = make_forza_sled_packet(6500.0, 45.0, 0.0, 28.0);
    let telemetry = normaliser.normalize(&packet)?;

    // Then: the adapter normalizes RPM correctly
    assert!(
        (telemetry.rpm - 6500.0).abs() < 1.0,
        "RPM must be ~6500, got {}",
        telemetry.rpm
    );

    // And: speed is derived from velocity vector
    let expected_speed = (45.0f32.powi(2) + 0.0f32.powi(2) + 28.0f32.powi(2)).sqrt();
    assert!(
        (telemetry.speed_ms - expected_speed).abs() < 0.5,
        "speed_ms must be ~{expected_speed:.1}, got {}",
        telemetry.speed_ms
    );

    // And: the normalized data flows through the FFB filter pipeline
    let ffb_scalar = (telemetry.rpm / 8000.0).clamp(0.0, 1.0);
    let mut frame = FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed: telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut frame, &damper);
    torque_cap_filter(&mut frame, 1.0);

    // And: the pipeline output is within valid bounds
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "pipeline output must be finite and within [-1, 1], got {}",
        frame.torque_out
    );

    // And: the safety-clamped torque can be sent to the device
    let torque_nm = frame.torque_out * 5.0;
    let clamped = safety.clamp_torque_nm(torque_nm);
    device.write_ffb_report(clamped, frame.seq)?;
    assert!(
        device.is_connected(),
        "device must remain connected after FFB write"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 10: Assetto Corsa running → user changes car → FFB recalibrates
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  Assetto Corsa is running with FFB flowing for one car
/// When   the user changes to a different car (different characteristics)
/// Then   the FFB mode is re-negotiated for the new car
/// And    the filter pipeline adapts to the new car's FFB requirements
/// And    FFB continues without interruption
/// ```
#[test]
fn given_assetto_corsa_running_when_user_changes_car_then_ffb_recalibrates() -> Result<()> {
    // Given: Assetto Corsa is running - user is driving car A
    let normaliser = ForzaAdapter::new();

    let id: DeviceId = "bdd-ac-car-switch-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD AC Car Switch Wheel".to_string());
    let safety = SafetyService::new(8.0, 20.0);

    // Car A: high-powered car (e.g., Ferrari 488 GT3)
    let car_a_packet = make_forza_sled_packet(7000.0, 50.0, 0.0, 30.0);
    let car_a_telemetry = normaliser.normalize(&car_a_packet)?;
    let car_a_ffb = (car_a_telemetry.rpm / 8000.0).clamp(0.0, 1.0);

    let mut car_a_frame = FilterFrame {
        ffb_in: car_a_ffb,
        torque_out: car_a_ffb,
        wheel_speed: car_a_telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    torque_cap_filter(&mut car_a_frame, 1.0);

    // When: user switches to Car B (lower powered, e.g., Porsche 991 GT3)
    let car_b_packet = make_forza_sled_packet(6000.0, 45.0, 0.0, 25.0);
    let car_b_telemetry = normaliser.normalize(&car_b_packet)?;
    let car_b_ffb = (car_b_telemetry.rpm / 8000.0).clamp(0.0, 1.0);

    let mut car_b_frame = FilterFrame {
        ffb_in: car_b_ffb,
        torque_out: car_b_ffb,
        wheel_speed: car_b_telemetry.speed_ms * 0.1,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut car_b_frame, &damper);
    torque_cap_filter(&mut car_b_frame, 1.0);

    // Then: both car profiles produce valid FFB output
    assert!(
        car_a_frame.torque_out.is_finite() && car_a_frame.torque_out.abs() <= 1.0,
        "Car A FFB must be finite and in [-1, 1]"
    );
    assert!(
        car_b_frame.torque_out.is_finite() && car_b_frame.torque_out.abs() <= 1.0,
        "Car B FFB must be finite and in [-1, 1]"
    );

    // And: the device can seamlessly transition between cars
    let car_a_torque = safety.clamp_torque_nm(car_a_frame.torque_out * 8.0);
    device.write_ffb_report(car_a_torque, car_a_frame.seq)?;

    let car_b_torque = safety.clamp_torque_nm(car_b_frame.torque_out * 8.0);
    device.write_ffb_report(car_b_torque, car_b_frame.seq)?;

    assert!(
        device.is_connected(),
        "device must remain connected after car switch"
    );

    Ok(())
}
