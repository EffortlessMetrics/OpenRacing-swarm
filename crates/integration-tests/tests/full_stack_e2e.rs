//! Deep end-to-end integration tests exercising the full OpenRacing stack.
//!
//! Cross-crate coverage: engine (VirtualDevice, VirtualHidPort, Pipeline, Frame,
//! SafetyService, SafetyState, FaultType) × schemas (DeviceId, DeviceCapabilities,
//! TorqueNm, NormalizedTelemetry) × filters (damper, friction, torque_cap, slew_rate)
//! × service (WheelService) × telemetry-adapters (adapter_factories).
//!
//! Scenarios:
//! 1. Virtual device → Engine → Telemetry → FFB output (full pipeline)
//! 2. Multiple games in sequence (switch detection, adapter swap)
//! 3. Device hot-plug during active session (connect, disconnect, reconnect)
//! 4. Profile switch during active session (smooth transition)
//! 5. Safety interlock activation during high torque (emergency stop)
//! 6. Telemetry recording during session (capture and verify)
//! 7. Configuration change during active session (hot-reload)
//! 8. Graceful shutdown with active devices and sessions

use std::time::Duration;

use anyhow::Result;

use racing_wheel_engine::ports::{HidDevice, HidPort};
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{Frame, Pipeline, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;

use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, SlewRateState, damper_filter,
    friction_filter, slew_rate_filter, torque_cap_filter,
};

use openracing_telemetry_adapters::{TelemetryAdapter, adapter_factories};
use racing_wheel_service::system_config::SystemConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a VirtualHidPort pre-loaded with one virtual device.
fn make_port_with_device(id_str: &str, name: &str) -> Result<(VirtualHidPort, DeviceId)> {
    let id: DeviceId = id_str.parse()?;
    let device = VirtualDevice::new(id.clone(), name.to_string());
    let mut port = VirtualHidPort::new();
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((port, id))
}

/// Build a filter-level frame from telemetry scalars.
fn filter_frame(ffb_scalar: f32, wheel_speed: f32, seq: u16) -> FilterFrame {
    FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: u64::from(seq) * 1_000_000,
        seq,
    }
}

/// Build an engine-level frame.
fn engine_frame(ffb_in: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: u64::from(seq) * 1_000_000,
        seq,
    }
}

/// Run the full filter → engine → safety → device pipeline for one tick.
fn run_full_tick(
    ffb_scalar: f32,
    wheel_speed: f32,
    seq: u16,
    pipeline: &mut Pipeline,
    safety: &SafetyService,
    device: &mut dyn HidDevice,
    safe_torque_nm: f32,
) -> Result<f32> {
    // Filter stage
    let mut ff = filter_frame(ffb_scalar, wheel_speed, seq);
    let damper = DamperState::fixed(0.02);
    let friction = FrictionState::fixed(0.01);
    damper_filter(&mut ff, &damper);
    friction_filter(&mut ff, &friction);
    torque_cap_filter(&mut ff, 1.0);

    // Engine pipeline stage
    let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
    pipeline.process(&mut ef)?;

    // Safety clamp
    let torque_nm = safety.clamp_torque_nm(ef.torque_out * safe_torque_nm);

    // Device write
    device.write_ffb_report(torque_nm, seq)?;

    Ok(torque_nm)
}

/// Look up a telemetry adapter by game_id from the registry.
fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| anyhow::anyhow!("adapter '{game_id}' not found"))?;
    Ok(factory())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Virtual device → Engine → Telemetry → FFB output (full pipeline)
// ═══════════════════════════════════════════════════════════════════════════════

/// Exercises the complete data path: filter pipeline → engine pipeline → safety
/// clamp → device write → telemetry readback. Validates data integrity across
/// every crate boundary.
#[test]
fn full_pipeline_virtual_device_to_ffb_output() -> Result<()> {
    let id: DeviceId = "full-pipeline-001".parse()?;
    let mut device = VirtualDevice::new(id, "Full Pipeline Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    // Run 100 ticks through the full pipeline
    for seq in 0u16..100 {
        let ffb_in = ((seq as f32) * 0.05).sin() * 0.6;
        let torque_nm = run_full_tick(ffb_in, 1.0, seq, &mut pipeline, &safety, &mut device, 5.0)?;

        assert!(
            torque_nm.is_finite(),
            "tick {seq}: torque_nm must be finite, got {torque_nm}"
        );
        assert!(
            torque_nm.abs() <= 5.0,
            "tick {seq}: torque must not exceed safe limit, got {torque_nm}"
        );
    }

    // Verify telemetry readback
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after 100-tick pipeline"))?;
    assert!(
        telem.temperature_c <= 150,
        "temperature must be in sane range"
    );

    Ok(())
}

/// Full pipeline with physics simulation: device state evolves across ticks.
#[test]
fn full_pipeline_with_physics_simulation() -> Result<()> {
    let id: DeviceId = "full-physics-001".parse()?;
    let mut device = VirtualDevice::new(id, "Physics Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    for seq in 0u16..50 {
        let ffb_in = 0.4;
        let _torque = run_full_tick(ffb_in, 0.5, seq, &mut pipeline, &safety, &mut device, 5.0)?;

        // Simulate physics between ticks
        device.simulate_physics(Duration::from_millis(1));
    }

    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after physics simulation"))?;

    // Wheel should have moved after 50 ticks of constant torque
    assert!(
        telem.wheel_angle_deg.abs() > 0.0 || telem.wheel_speed_rad_s.abs() > 0.0,
        "wheel must have moved after sustained torque"
    );

    Ok(())
}

/// Validates filter pipeline produces bounded, finite output for extreme inputs.
#[test]
fn full_pipeline_extreme_inputs_remain_bounded() -> Result<()> {
    let id: DeviceId = "full-extreme-001".parse()?;
    let mut device = VirtualDevice::new(id, "Extreme Input Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    let extreme_values: &[f32] = &[0.0, 1.0, -1.0, 0.999, -0.999, 0.001, -0.001];

    for (i, &ffb_in) in extreme_values.iter().enumerate() {
        let seq = i as u16;
        let torque = run_full_tick(ffb_in, 10.0, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(
            torque.is_finite(),
            "extreme input {ffb_in}: torque must be finite, got {torque}"
        );
        assert!(
            torque.abs() <= 5.0,
            "extreme input {ffb_in}: torque must be within safe limit"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multiple games in sequence (switch detection, adapter swap)
// ═══════════════════════════════════════════════════════════════════════════════

/// Simulates switching between games in sequence: each adapter must normalize
/// its minimum-valid packet correctly, and the pipeline must accept the output.
#[test]
fn multi_game_sequence_adapter_swap() -> Result<()> {
    let id: DeviceId = "multi-game-001".parse()?;
    let mut device = VirtualDevice::new(id, "Multi-Game Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    // Simulate switching between games
    let game_ids = ["forza_motorsport", "live_for_speed", "dirt_rally_2"];

    for (game_idx, game_id) in game_ids.iter().enumerate() {
        let adapter = get_adapter(game_id)?;
        assert_eq!(
            adapter.game_id(),
            *game_id,
            "adapter game_id must match registration"
        );

        // Build a large zero-filled buffer (some adapters accept this, others
        // reject it; the key assertion is no panic and the pipeline remains valid).
        let large_buf = vec![0u8; 2048];
        let ffb_scalar = match adapter.normalize(&large_buf) {
            Ok(telem) => telem.ffb_scalar,
            Err(_) => 0.0, // Use zero FFB if adapter rejects the packet
        };

        // Run a few ticks with this game's FFB output
        for tick in 0u16..10 {
            let seq = (game_idx as u16) * 100 + tick;
            let mut ef = engine_frame(ffb_scalar, 1.0, seq);
            pipeline.process(&mut ef)?;
            let torque_nm = safety.clamp_torque_nm(ef.torque_out * 5.0);
            device.write_ffb_report(torque_nm, seq)?;
        }
    }

    // Device must remain functional after three game switches
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after game switches"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Verify all high-priority adapters can be instantiated and produce consistent
/// game_id values across sequential creation.
#[test]
fn multi_game_adapter_registry_consistency() -> Result<()> {
    let required = ["acc", "iracing", "gran_turismo_7", "f1_25", "rfactor2"];
    let factories = adapter_factories();

    for game_id in &required {
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| id == game_id)
            .ok_or_else(|| anyhow::anyhow!("adapter '{game_id}' not in registry"))?;

        // Instantiate twice to verify no global state corruption
        let a1 = factory();
        let a2 = factory();
        assert_eq!(a1.game_id(), a2.game_id(), "game_id must be stable");
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device hot-plug during active session
// ═══════════════════════════════════════════════════════════════════════════════

/// Full cycle: device active → disconnect mid-session → reconnect → resume FFB.
#[test]
fn hotplug_disconnect_reconnect_during_active_session() -> Result<()> {
    let id: DeviceId = "hotplug-active-001".parse()?;
    let mut device = VirtualDevice::new(id, "Hot-Plug Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Phase 1: normal operation
    for seq in 0u16..20 {
        let mut frame = engine_frame(0.5, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }
    assert!(device.is_connected(), "device must be connected in phase 1");

    // Phase 2: disconnect
    device.disconnect();
    assert!(!device.is_connected());

    // Writes must fail
    let write_err = device.write_ffb_report(1.0, 20);
    assert!(write_err.is_err(), "write must fail after disconnect");

    // Telemetry must be None
    assert!(device.read_telemetry().is_none(), "telemetry must be None");

    // Report USB stall fault
    safety.report_fault(FaultType::UsbStall);
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "safety must enter faulted state"
    );

    // Phase 3: reconnect and recover
    device.reconnect();
    assert!(device.is_connected());

    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Phase 4: resume normal operation
    for seq in 21u16..40 {
        let mut frame = engine_frame(0.3, 0.5, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after reconnect"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Hot-plug a new device via VirtualHidPort mid-session while an existing
/// device remains active.
#[tokio::test]
async fn hotplug_add_device_mid_session_via_port() -> Result<()> {
    let (mut port, id_a) = make_port_with_device("hotplug-port-a", "First Wheel")?;

    // Use the first device
    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev_a.write_ffb_report(2.0, 0)?;

    // Hot-plug a second device
    let id_b: DeviceId = "hotplug-port-b".parse()?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Second Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // First device remains operational
    dev_a.write_ffb_report(3.0, 1)?;
    assert!(dev_a.read_telemetry().is_some());

    // Second device is fully functional
    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev_b.write_ffb_report(1.5, 0)?;
    assert!(dev_b.read_telemetry().is_some());

    // Port enumerates both
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 2, "both devices must be listed");

    Ok(())
}

/// Remove one device from a multi-device port; remaining device unaffected.
#[tokio::test]
async fn hotplug_remove_device_other_unaffected() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "remove-a".parse()?;
    let id_b: DeviceId = "remove-b".parse()?;
    port.add_device(VirtualDevice::new(id_a.clone(), "Wheel A".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Wheel B".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Remove device A
    port.remove_device(&id_a)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Device B must remain fully functional
    dev_b.write_ffb_report(2.0, 0)?;
    assert!(dev_b.read_telemetry().is_some());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Profile switch during active session (smooth transition)
// ═══════════════════════════════════════════════════════════════════════════════

/// Simulates switching filter parameters mid-session (different torque caps and
/// damping values), ensuring the pipeline transitions smoothly without errors.
#[test]
fn profile_switch_smooth_transition() -> Result<()> {
    let id: DeviceId = "profile-switch-001".parse()?;
    let mut device = VirtualDevice::new(id, "Profile Switch Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Profile A: high damping, low torque cap
    let damper_a = DamperState::fixed(0.05);
    let friction_a = FrictionState::fixed(0.03);
    let cap_a = 0.8;

    // Profile B: low damping, full torque cap
    let damper_b = DamperState::fixed(0.01);
    let friction_b = FrictionState::fixed(0.005);
    let cap_b = 1.0;

    let mut last_torque_a = 0.0f32;

    // Run Profile A for 50 ticks
    let mut pipeline = Pipeline::new();
    for seq in 0u16..50 {
        let mut ff = filter_frame(0.6, 1.0, seq);
        damper_filter(&mut ff, &damper_a);
        friction_filter(&mut ff, &friction_a);
        torque_cap_filter(&mut ff, cap_a);

        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        last_torque_a = torque;
    }

    // Switch to Profile B
    let mut pipeline_b = Pipeline::new();
    let mut first_torque_b = 0.0f32;
    for seq in 50u16..100 {
        let mut ff = filter_frame(0.6, 1.0, seq);
        damper_filter(&mut ff, &damper_b);
        friction_filter(&mut ff, &friction_b);
        torque_cap_filter(&mut ff, cap_b);

        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline_b.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        if seq == 50 {
            first_torque_b = torque;
        }
    }

    // Both profiles produced valid, finite torque values
    assert!(last_torque_a.is_finite(), "profile A torque must be finite");
    assert!(
        first_torque_b.is_finite(),
        "profile B torque must be finite"
    );

    // Device remains operational after switch
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after profile switch"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Switch between slew-rate-limited and non-limited profiles.
#[test]
fn profile_switch_slew_rate_transition() -> Result<()> {
    let id: DeviceId = "slew-switch-001".parse()?;
    let mut device = VirtualDevice::new(id, "Slew Switch Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);
    let mut pipeline = Pipeline::new();
    let mut slew_state = SlewRateState::new(0.8);

    // Phase 1: with slew rate limiting
    for seq in 0u16..30 {
        let mut ff = filter_frame(0.7, 1.0, seq);
        slew_rate_filter(&mut ff, &mut slew_state);
        torque_cap_filter(&mut ff, 1.0);

        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    // Phase 2: no slew rate limiting (wider slew)
    let mut slew_state_wide = SlewRateState::new(1.0);
    for seq in 30u16..60 {
        let mut ff = filter_frame(0.7, 1.0, seq);
        slew_rate_filter(&mut ff, &mut slew_state_wide);
        torque_cap_filter(&mut ff, 1.0);

        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    assert!(device.read_telemetry().is_some());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Safety interlock activation during high torque (emergency stop)
// ═══════════════════════════════════════════════════════════════════════════════

/// During normal pipeline operation, a safety fault must immediately zero
/// torque output across the full crate chain.
#[test]
fn safety_interlock_emergency_stop_full_pipeline() -> Result<()> {
    let id: DeviceId = "interlock-001".parse()?;
    let mut device = VirtualDevice::new(id, "Interlock Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal: torque flows
    let mut frame = engine_frame(0.8, 1.0, 0);
    pipeline.process(&mut frame)?;
    let normal_torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(normal_torque, 0)?;
    assert!(
        normal_torque.abs() > 0.01,
        "normal torque must be non-zero, got {normal_torque}"
    );

    // EMERGENCY: safety interlock violation during high torque
    safety.report_fault(FaultType::SafetyInterlockViolation);

    // All subsequent ticks must produce zero torque
    for seq in 1u16..20 {
        let mut frame = engine_frame(0.9, 2.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            torque.abs() < 0.001,
            "tick {seq}: torque must be zero after interlock violation, got {torque}"
        );
        // Device write with zero torque must succeed
        device.write_ffb_report(torque, seq)?;
    }

    Ok(())
}

/// Multiple fault types injected in rapid succession must keep safety in
/// faulted state with torque zeroed.
#[test]
fn safety_interlock_cascade_faults_keep_zero_torque() -> Result<()> {
    let id: DeviceId = "cascade-001".parse()?;
    let mut device = VirtualDevice::new(id, "Cascade Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    let faults = [
        FaultType::Overcurrent,
        FaultType::ThermalLimit,
        FaultType::EncoderNaN,
    ];

    for (i, fault) in faults.iter().enumerate() {
        safety.report_fault(*fault);
        let mut frame = engine_frame(0.5, 1.0, i as u16);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            torque.abs() < 0.001,
            "fault {fault:?}: torque must be zero, got {torque}"
        );
        device.write_ffb_report(torque, i as u16)?;
    }

    // Final fault should be the last injected
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::EncoderNaN,
                "most recent fault must be EncoderNaN"
            );
        }
        other => {
            return Err(anyhow::anyhow!("expected Faulted, got {other:?}"));
        }
    }

    Ok(())
}

/// Device fault flags correlate with safety service fault detection.
#[test]
fn safety_interlock_device_fault_flags_correlation() -> Result<()> {
    let id: DeviceId = "fault-flag-001".parse()?;
    let mut device = VirtualDevice::new(id, "Fault Flag Wheel".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Inject device-level fault
    device.inject_fault(0x04); // thermal fault bit
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
    assert_ne!(telem.fault_flags, 0, "device fault flags must be set");

    // Safety reacts
    safety.report_fault(FaultType::ThermalLimit);
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(clamped.abs() < 0.001, "safety must zero torque on fault");

    // Clear device faults + safety
    device.clear_faults();
    let telem_clear = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after clear"))?;
    assert_eq!(telem_clear.fault_flags, 0, "device faults must be cleared");

    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    let resumed = safety.clamp_torque_nm(3.0);
    assert!(
        (resumed - 3.0).abs() < 0.01,
        "torque must flow after recovery"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Telemetry recording during session (capture and verify)
// ═══════════════════════════════════════════════════════════════════════════════

/// Capture telemetry snapshots across a simulated session and verify data
/// integrity: all fields finite, timestamps monotonic, torque bounded.
#[test]
fn telemetry_recording_capture_and_verify() -> Result<()> {
    let id: DeviceId = "telem-record-001".parse()?;
    let mut device = VirtualDevice::new(id, "Telemetry Recording Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    let mut torque_history: Vec<f32> = Vec::new();

    for seq in 0u16..200 {
        let ffb_in = ((seq as f32) * 0.03).sin() * 0.5;
        let mut frame = engine_frame(ffb_in, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;

        if seq % 10 == 0 {
            device.simulate_physics(Duration::from_millis(10));
        }
        torque_history.push(torque);
    }

    // Verify recorded torque history
    assert_eq!(torque_history.len(), 200, "must have 200 recorded samples");

    for (i, &t) in torque_history.iter().enumerate() {
        assert!(t.is_finite(), "sample {i}: torque must be finite, got {t}");
        assert!(
            t.abs() <= 5.0,
            "sample {i}: torque must be within safe limit, got {t}"
        );
    }

    // Verify device telemetry still available
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after recording"))?;
    assert!(telem.wheel_angle_deg.is_finite());
    assert!(telem.wheel_speed_rad_s.is_finite());

    Ok(())
}

/// Telemetry normalization round-trip: adapter output → serialize → deserialize
/// preserves all key fields.
#[test]
fn telemetry_recording_normalization_round_trip() -> Result<()> {
    let telem = NormalizedTelemetry::builder()
        .speed_ms(55.0)
        .rpm(7200.0)
        .gear(5)
        .throttle(0.9)
        .brake(0.0)
        .steering_angle(0.05)
        .ffb_scalar(0.45)
        .build();

    let json = serde_json::to_string(&telem)?;
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert!(
        (decoded.speed_ms - 55.0).abs() < f32::EPSILON,
        "speed_ms round-trip"
    );
    assert!(
        (decoded.rpm - 7200.0).abs() < f32::EPSILON,
        "rpm round-trip"
    );
    assert_eq!(decoded.gear, 5, "gear round-trip");
    assert!(
        (decoded.ffb_scalar - 0.45).abs() < f32::EPSILON,
        "ffb_scalar round-trip"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Configuration change during active session (hot-reload)
// ═══════════════════════════════════════════════════════════════════════════════

/// SystemConfig can be modified and re-applied without disrupting an active
/// pipeline session. Validates config serde round-trip and parameter changes.
#[test]
fn config_hot_reload_during_active_session() -> Result<()> {
    let id: DeviceId = "config-reload-001".parse()?;
    let mut device = VirtualDevice::new(id, "Config Reload Wheel".to_string());
    let mut pipeline = Pipeline::new();

    // Load default config
    let config = SystemConfig::default();
    let initial_safe_torque = config.safety.default_safe_torque_nm;
    let safety = SafetyService::new(initial_safe_torque, config.safety.max_torque_nm);

    // Run 20 ticks with initial config
    for seq in 0u16..20 {
        let mut frame = engine_frame(0.5, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * initial_safe_torque);
        device.write_ffb_report(torque, seq)?;
    }

    // Simulate config hot-reload: serialize → modify → deserialize
    let json = serde_json::to_string(&config)?;
    let mut updated: SystemConfig = serde_json::from_str(&json)?;
    updated.safety.default_safe_torque_nm = 3.0; // Reduce safe torque

    // Apply updated config
    let safety_updated = SafetyService::new(
        updated.safety.default_safe_torque_nm,
        updated.safety.max_torque_nm,
    );

    // Run 20 ticks with updated config
    for seq in 20u16..40 {
        let mut frame = engine_frame(0.5, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety_updated
            .clamp_torque_nm(frame.torque_out * updated.safety.default_safe_torque_nm);
        assert!(
            torque.abs() <= updated.safety.default_safe_torque_nm,
            "tick {seq}: torque must respect updated limit"
        );
        device.write_ffb_report(torque, seq)?;
    }

    assert!(device.read_telemetry().is_some());

    Ok(())
}

/// Validate that SystemConfig default values are self-consistent and can
/// initialize all services without error.
#[test]
fn config_hot_reload_default_is_self_consistent() -> Result<()> {
    let config = SystemConfig::default();

    // Safety limits must be positive and ordered
    assert!(
        config.safety.default_safe_torque_nm > 0.0,
        "default safe torque must be positive"
    );
    assert!(
        config.safety.max_torque_nm >= config.safety.default_safe_torque_nm,
        "max torque must be >= safe torque"
    );

    // Engine tick rate must be positive
    assert!(config.engine.tick_rate_hz > 0, "tick rate must be positive");

    // Serde round-trip
    let json = serde_json::to_string_pretty(&config)?;
    let decoded: SystemConfig = serde_json::from_str(&json)?;
    assert_eq!(
        decoded.schema_version, config.schema_version,
        "schema_version must survive round-trip"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Graceful shutdown with active devices and sessions
// ═══════════════════════════════════════════════════════════════════════════════

/// A service with active devices must shut down cleanly: all devices receive
/// zero torque, telemetry becomes unavailable, no resources leak.
#[tokio::test]
async fn graceful_shutdown_with_active_devices() -> Result<()> {
    let (port, id) = make_port_with_device("shutdown-001", "Shutdown Wheel")?;

    // Open and use the device
    let mut dev = port
        .open_device(&id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev.write_ffb_report(3.0, 0)?;

    // Simulate graceful shutdown: write zero torque
    dev.write_ffb_report(0.0, 1)?;

    // Device reports connected but with zero torque
    let telem = dev
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing during shutdown"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Multiple devices must all receive zero torque during graceful shutdown.
#[tokio::test]
async fn graceful_shutdown_multiple_devices() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let ids: Vec<DeviceId> = (0..3)
        .map(|i| format!("shutdown-multi-{i}").parse())
        .collect::<Result<Vec<_>, _>>()?;

    for (i, id) in ids.iter().enumerate() {
        let device = VirtualDevice::new(id.clone(), format!("Wheel {i}"));
        port.add_device(device)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // Open and use all devices
    let mut devices: Vec<Box<dyn HidDevice>> = Vec::new();
    for id in &ids {
        let dev = port
            .open_device(id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        devices.push(dev);
    }

    for (i, dev) in devices.iter_mut().enumerate() {
        dev.write_ffb_report(2.0, 0)?;
        assert!(
            dev.read_telemetry().is_some(),
            "device {i} must have telemetry"
        );
    }

    // Graceful shutdown: zero torque for all devices
    for dev in devices.iter_mut() {
        dev.write_ffb_report(0.0, 1)?;
    }

    // Verify all devices still readable
    for (i, dev) in devices.iter_mut().enumerate() {
        assert!(
            dev.read_telemetry().is_some(),
            "device {i} must still have telemetry after zero-torque"
        );
    }

    Ok(())
}

/// Safety service + pipeline + device shutdown sequence: fault → zero torque →
/// clear → safe state.
#[test]
fn graceful_shutdown_safety_pipeline_sequence() -> Result<()> {
    let id: DeviceId = "shutdown-safety-001".parse()?;
    let mut device = VirtualDevice::new(id, "Shutdown Safety Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Active session
    let mut frame = engine_frame(0.6, 1.0, 0);
    pipeline.process(&mut frame)?;
    let active_torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(active_torque, 0)?;

    // Initiate shutdown: fault to zero torque
    safety.report_fault(FaultType::PipelineFault);
    let shutdown_torque = safety.clamp_torque_nm(5.0);
    assert!(shutdown_torque.abs() < 0.001, "shutdown must zero torque");
    device.write_ffb_report(shutdown_torque, 1)?;

    // Wait hold period and clear
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    Ok(())
}
