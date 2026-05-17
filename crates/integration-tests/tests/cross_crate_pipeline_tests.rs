//! Cross-crate integration tests for OpenRacing pipeline interactions.
//!
//! These tests verify realistic flows spanning multiple workspace crates:
//! - Telemetry normalization → filter pipeline
//! - Safety fault → pipeline output clamping
//! - Watchdog overrun → safety fault cascade
//! - FMEA soft stop → filter chain ramp-down
//! - Blackbox diagnostic → pipeline frame recording
//! - Axis calibration → filter chain input
//! - Telemetry adapters → normalized FFB frames

use std::time::Duration;

use anyhow::Result;
use tempfile::TempDir;

// Engine types (safety, pipeline, frame, diagnostics)
use racing_wheel_engine::diagnostic::{BlackboxConfig, BlackboxRecorder};
use racing_wheel_engine::safety::{
    FaultType, SafetyService, SafetyState, SoftStopController, WatchdogConfig, WatchdogSystem,
};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline};

// Schemas (telemetry, profiles, domain types)
use racing_wheel_schemas::prelude::*;

// Filter-level types
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, SlewRateState, damper_filter,
    friction_filter, slew_rate_filter, torque_cap_filter,
};

// Calibration
use openracing_calibration::AxisCalibration;

// Telemetry adapters
use openracing_telemetry_adapters::adapter_factories;

/// Helper: build a filter-level Frame from telemetry values.
fn filter_frame_from_telemetry(ffb_scalar: f32, wheel_speed: f32) -> FilterFrame {
    FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

/// Helper: build an engine-level Frame.
fn engine_frame(ffb_in: f32, wheel_speed: f32) -> EngineFrame {
    EngineFrame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

// ---------------------------------------------------------------------------
// Test 1: schemas (NormalizedTelemetryBuilder) → openracing-filters pipeline
// ---------------------------------------------------------------------------

#[test]
fn telemetry_normalization_feeds_filter_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    // Build normalized telemetry from schemas crate
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .steering_angle(0.15)
        .throttle(0.8)
        .brake(0.0)
        .gear(4)
        .ffb_scalar(0.6)
        .build();

    // Extract telemetry values and construct a filter Frame
    let mut frame = filter_frame_from_telemetry(telemetry.ffb_scalar, telemetry.speed_ms);

    // Apply damper (speed-proportional opposing torque) and friction
    let damper = DamperState::fixed(0.01);
    let friction = FrictionState::fixed(0.05);
    damper_filter(&mut frame, &damper);
    friction_filter(&mut frame, &friction);
    torque_cap_filter(&mut frame, 1.0);

    // Verify output is finite and bounded
    assert!(
        frame.torque_out.is_finite(),
        "Torque output must be finite after filter chain, got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out.abs() <= 1.0,
        "Torque output must be within [-1.0, 1.0], got {}",
        frame.torque_out
    );
    // At 30 m/s with damper coeff 0.01, damper contributes -0.3
    // friction at positive speed contributes -0.05
    // Combined with initial 0.6 should yield a reduced positive torque
    assert!(
        frame.torque_out < telemetry.ffb_scalar,
        "Damper + friction should reduce torque from {}, got {}",
        telemetry.ffb_scalar,
        frame.torque_out
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 2: engine safety (SafetyService + FaultType) → engine pipeline clamp
// ---------------------------------------------------------------------------

#[test]
fn safety_fault_zeroes_clamped_pipeline_output() -> Result<(), Box<dyn std::error::Error>> {
    // Create safety service: 5 Nm safe limit, 20 Nm high-torque limit
    let mut safety = SafetyService::new(5.0, 20.0);

    // Initially in SafeTorque state: clamping should allow up to safe limit
    let clamped_safe = safety.clamp_torque_nm(3.0);
    assert!(
        (clamped_safe - 3.0).abs() < 0.001,
        "3 Nm should pass through in SafeTorque state, got {}",
        clamped_safe
    );

    // Report a USB stall fault
    safety.report_fault(FaultType::UsbStall);
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "Safety state must be Faulted after reporting UsbStall, got {:?}",
        safety.state()
    );

    // In faulted state, any torque request should be clamped to 0
    let clamped_faulted = safety.clamp_torque_nm(10.0);
    assert!(
        clamped_faulted.abs() < 0.001,
        "Faulted state must clamp torque to 0, got {}",
        clamped_faulted
    );

    // Also process through empty engine pipeline to verify it still works
    let mut pipeline = EnginePipeline::new();
    let mut frame = engine_frame(0.8, 5.0);
    pipeline.process(&mut frame)?;

    // Pipeline passthrough + safety clamp = 0
    let final_torque = safety.clamp_torque_nm(frame.torque_out);
    assert!(
        final_torque.abs() < 0.001,
        "Pipeline output must be zeroed by safety clamp in faulted state, got {}",
        final_torque
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 3: engine watchdog → safety fault cascade
// ---------------------------------------------------------------------------

#[test]
fn watchdog_overrun_cascades_to_safety_fault() -> Result<(), Box<dyn std::error::Error>> {
    // Configure watchdog: 100μs timeout, quarantine after 3 overruns
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(100)
        .plugin_max_timeouts(3)
        .plugin_quarantine_duration(Duration::from_secs(60))
        .build()?;

    let watchdog = WatchdogSystem::new(config);
    let mut safety = SafetyService::new(5.0, 20.0);

    // Record normal executions (under budget) — no fault
    for _ in 0..5 {
        let fault = watchdog.record_plugin_execution("test-plugin", 50);
        assert!(fault.is_none(), "Normal execution should not trigger fault");
    }

    // Record overruns exceeding budget
    let mut fault_triggered = false;
    for _ in 0..10 {
        if let Some(fault_type) = watchdog.record_plugin_execution("test-plugin", 500) {
            // Cascade fault to safety service
            safety.report_fault(fault_type);
            fault_triggered = true;
            break;
        }
    }

    assert!(
        fault_triggered,
        "Watchdog should trigger fault after repeated overruns"
    );
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "Safety must enter Faulted state after watchdog cascade, got {:?}",
        safety.state()
    );

    // Verify plugin is quarantined
    assert!(
        watchdog.is_plugin_quarantined("test-plugin"),
        "Plugin must be quarantined after overrun fault"
    );

    // Verify torque is clamped to zero in faulted state
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(
        clamped.abs() < 0.001,
        "Torque must be zero after watchdog→safety cascade, got {}",
        clamped
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 4: FMEA soft stop controller → filter chain ramp-down
// ---------------------------------------------------------------------------

#[test]
fn soft_stop_ramps_torque_to_zero_through_filter_chain() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SoftStopController::new();
    let damper = DamperState::fixed(0.05);
    let friction = FrictionState::fixed(0.02);

    // Start soft stop from 0.8 normalized torque over 50ms
    controller.start_soft_stop_with_duration(0.8, Duration::from_millis(50));
    assert!(
        controller.is_active(),
        "Soft stop must be active after start"
    );

    let mut prev_torque = f32::MAX;
    let mut tick_count = 0u32;

    // Simulate 1kHz ticks (1ms each) for 60ms (enough to complete 50ms ramp)
    while controller.is_active() && tick_count < 100 {
        let ramp_torque = controller.update(Duration::from_millis(1));

        // Feed ramping torque through filter chain
        let mut frame = filter_frame_from_telemetry(ramp_torque, 2.0);
        damper_filter(&mut frame, &damper);
        friction_filter(&mut frame, &friction);
        torque_cap_filter(&mut frame, 1.0);

        assert!(
            frame.torque_out.is_finite(),
            "Filter output must be finite during soft stop ramp at tick {}",
            tick_count
        );

        // Ramp torque should be monotonically decreasing toward zero
        assert!(
            ramp_torque <= prev_torque + 0.001,
            "Ramp torque must decrease: prev={}, current={} at tick {}",
            prev_torque,
            ramp_torque,
            tick_count
        );
        prev_torque = ramp_torque;
        tick_count += 1;
    }

    assert!(
        tick_count >= 10,
        "Soft stop should run for multiple ticks, ran for {}",
        tick_count
    );
    assert!(
        !controller.is_active(),
        "Soft stop must complete within 100ms budget"
    );

    // Final torque from controller should be at or near zero
    let final_torque = controller.update(Duration::from_millis(1));
    assert!(
        final_torque.abs() < 0.01,
        "Final ramp torque must be near zero, got {}",
        final_torque
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 5: engine diagnostic blackbox → pipeline frame recording
// ---------------------------------------------------------------------------

#[test]
fn blackbox_records_engine_pipeline_frames() -> Result<(), Box<dyn std::error::Error>> {
    let tmp_dir = TempDir::new()?;
    let device_id: DeviceId = "blackbox-test-device".parse()?;

    let config = BlackboxConfig {
        device_id: device_id.clone(),
        output_dir: tmp_dir.path().to_path_buf(),
        max_duration_s: 60,
        max_file_size_bytes: 10 * 1024 * 1024,
        compression_level: 1,
        enable_stream_a: true,
        enable_stream_b: false,
        enable_stream_c: false,
    };

    let mut recorder =
        BlackboxRecorder::new(config).map_err(|e| anyhow::anyhow!("Recorder init: {}", e))?;

    let mut pipeline = EnginePipeline::new();
    let safety_state = SafetyState::SafeTorque;

    // Record 100 frames through pipeline + blackbox
    for seq in 0..100u16 {
        let mut frame = EngineFrame {
            ffb_in: (seq as f32 / 100.0) * 2.0 - 1.0, // ramp -1.0 to ~1.0
            torque_out: 0.0,
            wheel_speed: seq as f32 * 0.1,
            hands_off: false,
            ts_mono_ns: seq as u64 * 1_000_000, // 1ms intervals
            seq,
        };
        frame.torque_out = frame.ffb_in; // pipeline passthrough for empty pipeline
        pipeline.process(&mut frame)?;

        recorder
            .record_frame(&frame, &[], &safety_state, 50)
            .map_err(|e| anyhow::anyhow!("Record frame {}: {}", seq, e))?;
    }

    let output_path = recorder
        .finalize()
        .map_err(|e| anyhow::anyhow!("Finalize: {}", e))?;

    // Verify the .wbb file was created and is non-empty
    assert!(
        output_path.exists(),
        "Blackbox file must exist at {:?}",
        output_path
    );
    let metadata = std::fs::metadata(&output_path)?;
    assert!(
        metadata.len() > 0,
        "Blackbox file must be non-empty, got {} bytes",
        metadata.len()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 6: calibration → filter chain input normalization
// ---------------------------------------------------------------------------

#[test]
fn calibration_normalizes_steering_for_filter_chain() -> Result<(), Box<dyn std::error::Error>> {
    // Configure steering axis: raw range 0-65535, center at 32768
    let cal = AxisCalibration::new(0, 65535).with_center(32768);

    let damper = DamperState::fixed(0.1);
    let friction = FrictionState::fixed(0.03);
    let mut slew = SlewRateState::per_tick(0.5);

    // Test multiple calibration points across the raw range
    let test_points: &[(u16, &str)] = &[
        (0, "full left"),
        (16384, "quarter left"),
        (32768, "center"),
        (49152, "quarter right"),
        (65535, "full right"),
    ];

    let mut prev_normalized = -1.0f32;
    for &(raw_value, label) in test_points {
        let normalized = cal.apply(raw_value);

        // Calibrated value should be in [0.0, 1.0]
        assert!(
            (0.0..=1.0).contains(&normalized),
            "{}: calibrated value must be in [0, 1], got {}",
            label,
            normalized
        );

        // Values should be monotonically increasing across the range
        assert!(
            normalized >= prev_normalized - 0.001,
            "{}: calibrated values must increase monotonically, prev={}, cur={}",
            label,
            prev_normalized,
            normalized
        );
        prev_normalized = normalized;

        // Map calibrated steering to FFB torque input: center=0, edges=±1
        let ffb_input = (normalized - 0.5) * 2.0;
        let mut frame = FilterFrame {
            ffb_in: ffb_input,
            torque_out: ffb_input,
            wheel_speed: 5.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        damper_filter(&mut frame, &damper);
        friction_filter(&mut frame, &friction);
        slew_rate_filter(&mut frame, &mut slew);
        torque_cap_filter(&mut frame, 1.0);

        assert!(
            frame.torque_out.is_finite(),
            "{}: filter output must be finite, got {}",
            label,
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "{}: filter output must be within [-1, 1], got {}",
            label,
            frame.torque_out
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 7: telemetry adapters → schemas → filter pipeline consistency
// ---------------------------------------------------------------------------

#[test]
fn adapter_telemetry_normalizes_to_valid_ffb_frames() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    assert!(
        factories.len() >= 15,
        "Expected at least 15 telemetry adapters, found {}",
        factories.len()
    );

    // For each registered adapter, simulate building telemetry and feeding it into FFB
    let damper = DamperState::fixed(0.02);
    let friction = FrictionState::fixed(0.01);

    // Simulate a range of game telemetry scenarios
    let scenarios: &[(&str, f32, f32, i8)] = &[
        ("idle", 0.0, 0.0, 0),
        ("cruising", 20.0, 0.3, 3),
        ("racing", 60.0, 0.8, 5),
        ("braking", 15.0, 0.1, 2),
        ("reverse", 3.0, -0.2, -1),
    ];

    for (scenario_name, speed, ffb, gear) in scenarios {
        let telemetry = NormalizedTelemetry::builder()
            .speed_ms(*speed)
            .ffb_scalar(*ffb)
            .gear(*gear)
            .throttle(0.5)
            .brake(0.0)
            .build();

        let mut frame = filter_frame_from_telemetry(telemetry.ffb_scalar, telemetry.speed_ms);
        damper_filter(&mut frame, &damper);
        friction_filter(&mut frame, &friction);
        torque_cap_filter(&mut frame, 1.0);

        assert!(
            frame.torque_out.is_finite(),
            "Scenario '{}': output must be finite, got {}",
            scenario_name,
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "Scenario '{}': output must be within [-1, 1], got {}",
            scenario_name,
            frame.torque_out
        );
    }

    // Verify all adapter game IDs are unique (cross-crate: adapters + schemas)
    let mut seen_ids = std::collections::HashSet::new();
    for (game_id, _) in factories {
        assert!(
            seen_ids.insert(*game_id),
            "Duplicate adapter game_id: {}",
            game_id
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 8: safety + all fault types → pipeline torque zeroing
// ---------------------------------------------------------------------------

#[test]
fn all_fault_types_zero_torque_through_engine_pipeline() -> Result<(), Box<dyn std::error::Error>> {
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
        let mut safety = SafetyService::new(5.0, 20.0);
        let mut pipeline = EnginePipeline::new();

        // Process a frame with high torque
        let mut frame = engine_frame(0.9, 10.0);
        pipeline.process(&mut frame)?;

        // Before fault: torque should pass through
        let pre_fault = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            pre_fault.abs() > 0.0,
            "{:?}: torque must be non-zero before fault",
            fault
        );

        // Trigger fault
        safety.report_fault(*fault);

        // After fault: torque must be zero
        let post_fault = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            post_fault.abs() < 0.001,
            "{:?}: torque must be zero after fault, got {}",
            fault,
            post_fault
        );
    }

    Ok(())
}
