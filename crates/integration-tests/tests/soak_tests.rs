//! Soak tests for long-running stability verification.
//!
//! These tests exercise the system under sustained load to verify stability,
//! memory behavior, and correctness over thousands of iterations. Iteration
//! counts are kept reasonable (10 000–50 000) so tests complete in seconds
//! while still exposing drift, accumulation, and leak-class defects.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use openracing_filters::Frame;
use openracing_fmea::{FaultType, FmeaMatrix, FmeaSystem, SoftStopController};
use openracing_pipeline::Pipeline;
use openracing_profile::{WheelProfile, WheelSettings};
use openracing_telemetry_recorder::{TelemetryPlayer, TestFixtureGenerator, TestScenario};
use openracing_watchdog::{SystemComponent, WatchdogConfig, WatchdogSystem};
use racing_wheel_engine::JitterMetrics;
use racing_wheel_engine::safety::{
    SafetyInterlockState, SafetyInterlockSystem, SafetyService, SoftwareWatchdog,
};
use racing_wheel_schemas::prelude::NormalizedTelemetry;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn make_interlock(max_torque_nm: f32, watchdog_timeout_ms: u32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(watchdog_timeout_ms));
    SafetyInterlockSystem::new(watchdog, max_torque_nm)
}

const ALL_FAULTS: [FaultType; 9] = [
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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Engine processing 10 000 consecutive frames without error
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_engine_10k_consecutive_frames_no_error() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;
    let mut pipeline = Pipeline::new();

    for tick in 0u64..10_000 {
        let input = (tick as f32 * 0.05).sin() * 0.8;
        let mut frame = Frame {
            ffb_in: input,
            torque_out: input,
            wheel_speed: (tick as f32 * 0.01).cos() * 5.0,
            hands_off: false,
            ts_mono_ns: tick * 1_000_000,
            seq: (tick & 0xFFFF) as u16,
        };

        pipeline.process(&mut frame)?;

        let result = interlock.process_tick(frame.torque_out * 25.0);
        assert_eq!(
            result.state,
            SafetyInterlockState::Normal,
            "tick {tick}: unexpected state {:?}",
            result.state
        );
        assert!(
            result.torque_command.abs() <= 25.0 + f32::EPSILON,
            "tick {tick}: torque {:.4} exceeds limit",
            result.torque_command
        );
        assert!(
            !result.fault_occurred,
            "tick {tick}: unexpected fault {:?}",
            result.fault_type
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Pipeline processing with rapid config changes
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_pipeline_rapid_config_changes() -> Result<()> {
    let mut pipeline = Pipeline::new();

    for tick in 0u64..10_000 {
        // Swap pipeline every 100 ticks to simulate rapid config changes
        if tick % 100 == 0 {
            pipeline = Pipeline::with_hash(tick);
        }

        let input = (tick as f32 * 0.02).sin() * 0.9;
        let mut frame = Frame {
            ffb_in: input,
            torque_out: input,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: tick * 1_000_000,
            seq: (tick & 0xFFFF) as u16,
        };

        pipeline.process(&mut frame)?;

        // Empty pipeline passthrough: output must equal input
        assert!(
            (frame.torque_out - input).abs() < f32::EPSILON,
            "tick {tick}: passthrough violated after config swap, in={input} out={}",
            frame.torque_out
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Telemetry recording for extended periods (many frames)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_telemetry_recording_extended_session() -> Result<()> {
    let temp_dir = tempfile::TempDir::new()?;
    let output_path = temp_dir.path().join("soak_recording.json");
    let mut recorder = openracing_telemetry_recorder::TelemetryRecorder::new(output_path)?;

    recorder.start_recording("soak_test_game".to_string());
    assert!(recorder.is_recording());

    for i in 0u64..10_000 {
        let progress = i as f32 / 10_000.0;
        let telemetry = NormalizedTelemetry::builder()
            .ffb_scalar((progress * 8.0 * std::f32::consts::PI).sin() * 0.8)
            .rpm(4000.0 + progress * 4000.0)
            .speed_ms(30.0 + progress * 50.0)
            .slip_ratio(0.1)
            .gear(3)
            .build();

        let frame = racing_wheel_schemas::telemetry::TelemetryFrame::new(
            telemetry,
            i * 16_666_667, // ~60 fps timestamps
            i,
            64,
        );
        recorder.record_frame(frame);
    }

    assert_eq!(recorder.frame_count(), 10_000);

    let recording = recorder.stop_recording(Some("Soak test recording".to_string()))?;
    assert_eq!(recording.frames.len(), 10_000);
    assert_eq!(recording.metadata.game_id, "soak_test_game");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Memory stability — no growth over many iterations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_memory_stability_no_growth_in_pipeline() -> Result<()> {
    let mut pipeline = Pipeline::new();
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    // Under clean operation, the fault log should remain empty.
    let mut max_fault_log_len = 0usize;

    for tick in 0u64..20_000 {
        let input = (tick as f32 * 0.01).sin() * 0.7;
        let mut frame = Frame {
            ffb_in: input,
            torque_out: input,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: tick * 1_000_000,
            seq: (tick & 0xFFFF) as u16,
        };

        pipeline.process(&mut frame)?;
        let result = interlock.process_tick(frame.torque_out * 25.0);
        assert!(!result.fault_occurred, "tick {tick}: unexpected fault");

        if tick % 2_000 == 0 {
            let log_len = interlock.fault_log().len();
            if log_len > max_fault_log_len {
                max_fault_log_len = log_len;
            }
        }
    }

    assert_eq!(max_fault_log_len, 0, "fault log grew during clean soak");

    // Inject faults and verify log stays bounded (max 1000 entries)
    for i in 0..300 {
        interlock.report_fault(ALL_FAULTS[i % ALL_FAULTS.len()]);
        let _ = interlock.process_tick(5.0);
    }

    let final_log_len = interlock.fault_log().len();
    assert!(
        final_log_len <= 1000,
        "fault log unbounded: {final_log_len} entries"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Scheduler stability over 1 000+ ticks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_jitter_metrics_stability_over_many_ticks() -> Result<()> {
    let mut metrics = JitterMetrics::new();

    for tick in 0u64..10_000 {
        // Simulate jitter with occasional spikes
        let jitter_ns = if tick % 500 == 0 { 100_000 } else { 10_000 };
        let missed = jitter_ns > 250_000;
        metrics.record_tick(jitter_ns, missed);
    }

    assert_eq!(metrics.total_ticks, 10_000);
    assert_eq!(metrics.missed_tick_rate(), 0.0);

    // p99 should be reasonable — all values well below 250 µs
    let p99 = metrics.p99_jitter_ns();
    assert!(p99 <= 250_000, "p99 jitter {p99}ns exceeds 250µs");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Concurrent operations under sustained load
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_concurrent_pipeline_and_safety_sustained() -> Result<(), BoxErr> {
    const NUM_THREADS: usize = 4;
    const ITERATIONS: usize = 5_000;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let error_count = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let barrier = Arc::clone(&barrier);
            let errors = Arc::clone(&error_count);
            thread::spawn(move || -> Result<(), BoxErr> {
                barrier.wait();
                let mut pipeline = Pipeline::new();
                let mut interlock = make_interlock(25.0, 500);
                interlock.arm().map_err(|e| format!("arm: {e}"))?;

                for i in 0..ITERATIONS {
                    let input = ((tid * ITERATIONS + i) as f32 * 0.03).sin() * 0.8;
                    let mut frame = Frame {
                        ffb_in: input,
                        torque_out: input,
                        wheel_speed: 0.0,
                        hands_off: false,
                        ts_mono_ns: i as u64 * 1_000_000,
                        seq: (i & 0xFFFF) as u16,
                    };

                    if pipeline.process(&mut frame).is_err() {
                        errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }

                    let result = interlock.process_tick(frame.torque_out * 25.0);
                    if result.fault_occurred {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert_eq!(
        error_count.load(Ordering::SeqCst),
        0,
        "unexpected errors during concurrent soak"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Safety service torque clamping sustained soak
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_safety_service_torque_clamping_20k() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    for tick in 0u64..20_000 {
        let raw = (tick as f32 * 0.03).sin() * 100.0;
        let clamped = safety.clamp_torque_nm(raw);
        assert!(
            clamped.abs() <= 5.0 + f32::EPSILON,
            "tick {tick}: clamped {clamped} exceeds safe torque 5.0"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. FMEA system detection soak — no false positives under clean input
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_fmea_no_false_positives() -> Result<()> {
    let mut fmea = FmeaSystem::new();

    for tick in 0u64..10_000 {
        // Clean USB input: 0 failures, no timeout
        assert!(
            fmea.detect_usb_fault(0, None).is_none(),
            "tick {tick}: false USB fault"
        );

        // Clean encoder input: valid float
        let val = (tick as f32 * 0.01).sin() * 100.0;
        assert!(
            fmea.detect_encoder_fault(val).is_none(),
            "tick {tick}: false encoder fault for val={val}"
        );

        // Clean thermal input: well below limit
        assert!(
            fmea.detect_thermal_fault(40.0, false).is_none(),
            "tick {tick}: false thermal fault"
        );

        // Clean timing: 50µs jitter
        assert!(
            fmea.detect_timing_violation(50).is_none(),
            "tick {tick}: false timing violation"
        );
    }

    assert!(!fmea.has_active_fault());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Watchdog heartbeat soak — components stay healthy
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_watchdog_heartbeat_sustained() -> Result<(), BoxErr> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(500)
        .plugin_max_timeouts(10)
        .plugin_quarantine_duration(Duration::from_secs(30))
        .rt_thread_timeout_ms(100)
        .hid_timeout_ms(100)
        .build()
        .map_err(|e| format!("config: {e}"))?;

    let watchdog = WatchdogSystem::new(config);

    let components = [
        SystemComponent::RtThread,
        SystemComponent::HidCommunication,
        SystemComponent::TelemetryAdapter,
        SystemComponent::PluginHost,
        SystemComponent::SafetySystem,
    ];

    for tick in 0u64..5_000 {
        for component in &components {
            watchdog.heartbeat(*component);
        }

        // Record plugin execution well within budget
        let exec_time = 100 + (tick % 50);
        let fault = watchdog.record_plugin_execution("soak_plugin", exec_time);
        assert!(
            fault.is_none(),
            "tick {tick}: unexpected plugin fault for exec_time={exec_time}µs"
        );
    }

    assert!(!watchdog.is_plugin_quarantined("soak_plugin"));

    let stats = watchdog.get_plugin_stats("soak_plugin");
    assert!(stats.is_some(), "plugin stats missing");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Telemetry playback soak — full fixture playback
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_telemetry_playback_full_fixture() -> Result<()> {
    let recording =
        TestFixtureGenerator::generate_racing_session("soak_playback".to_string(), 60.0, 60.0);
    assert_eq!(recording.frames.len(), 3600);

    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    player.set_playback_speed(10.0); // 10x speed

    let mut frames_consumed = 0usize;
    let start = Instant::now();
    // Process all frames by polling (up to 10 seconds wall time)
    while !player.is_finished() && start.elapsed() < Duration::from_secs(10) {
        if player.get_next_frame().is_some() {
            frames_consumed += 1;
        }
        std::thread::sleep(Duration::from_micros(100));
    }

    assert!(
        frames_consumed > 0,
        "no frames consumed during playback soak"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Soft-stop controller soak — repeated activations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_soft_stop_repeated_activations() -> Result<()> {
    let mut controller = SoftStopController::new();

    for cycle in 0u32..500 {
        controller.start_soft_stop_with_duration(20.0, Duration::from_millis(100));
        assert!(
            controller.is_active(),
            "cycle {cycle}: controller not active after start"
        );

        // Step through the ramp
        for step in 0..20 {
            let torque = controller.update(Duration::from_millis(5));
            assert!(
                torque.is_finite(),
                "cycle {cycle} step {step}: non-finite torque {torque}"
            );
        }

        controller.reset();
        assert!(
            !controller.is_active(),
            "cycle {cycle}: controller still active after reset"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Multi-device pipeline soak — 4 independent pipelines for 10 000 ticks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_multi_device_pipeline_10k() -> Result<()> {
    let device_count = 4;
    let mut devices: Vec<(SafetyInterlockSystem, Pipeline)> = (0..device_count)
        .map(|_| (make_interlock(25.0, 500), Pipeline::new()))
        .collect();

    for (interlock, _) in &mut devices {
        interlock.arm()?;
    }

    for tick in 0u64..10_000 {
        for (dev_idx, (interlock, pipeline)) in devices.iter_mut().enumerate() {
            let input = ((tick as f32 + dev_idx as f32 * 0.25) * 0.1).sin() * 18.0;
            let mut frame = Frame {
                ffb_in: input / 25.0,
                torque_out: input / 25.0,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: tick * 1_000_000,
                seq: (tick & 0xFFFF) as u16,
            };

            pipeline.process(&mut frame)?;
            let result = interlock.process_tick(frame.torque_out * 25.0);

            assert_eq!(
                result.state,
                SafetyInterlockState::Normal,
                "dev {dev_idx} tick {tick}: unexpected state {:?}",
                result.state
            );
            assert!(
                result.torque_command.abs() <= 25.0 + f32::EPSILON,
                "dev {dev_idx} tick {tick}: torque {} exceeds limit",
                result.torque_command
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Profile switching soak — thousands of switches with no corruption
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_profile_switching_many_iterations() -> Result<()> {
    let _initial = WheelProfile::new("default", "dev-0").with_settings(WheelSettings::default());
    let mut active;

    for i in 0u32..10_000 {
        let name = format!("profile-{i}");
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = (i as f32 / 10_000.0).clamp(0.0, 1.0);
        settings.ffb.damper_strength = (i as f32 * 0.001) % 1.0;

        active = WheelProfile::new(&name, "dev-0").with_settings(settings);

        assert_eq!(active.name, name, "iteration {i}: name mismatch");
        assert!(
            active.settings.ffb.overall_gain >= 0.0 && active.settings.ffb.overall_gain <= 1.0,
            "iteration {i}: gain out of range: {}",
            active.settings.ffb.overall_gain
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Interlock arm/disarm/reset soak — 1 000 cycles
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_interlock_arm_disarm_reset_cycles() -> Result<()> {
    for cycle in 0u32..1_000 {
        let mut interlock = make_interlock(25.0, 500);
        interlock.arm()?;

        // Run a burst of ticks
        for tick in 0..100 {
            let result = interlock.process_tick(10.0);
            assert!(
                !result.fault_occurred,
                "cycle {cycle} tick {tick}: unexpected fault"
            );
        }

        interlock.disarm()?;
        interlock.reset()?;

        assert_eq!(
            interlock.state(),
            &SafetyInterlockState::Normal,
            "cycle {cycle}: state not Normal after reset"
        );
        assert!(
            !interlock.is_watchdog_armed(),
            "cycle {cycle}: watchdog still armed"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. FMEA matrix soak — repeated lookups over many iterations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_fmea_matrix_repeated_lookups() -> Result<()> {
    let matrix = FmeaMatrix::with_defaults();

    for tick in 0u64..10_000 {
        let fault = ALL_FAULTS[(tick as usize) % ALL_FAULTS.len()];
        let entry = matrix.get(fault);
        assert!(
            entry.is_some(),
            "tick {tick}: missing FMEA entry for {fault:?}"
        );

        if let Some(e) = entry {
            assert_eq!(e.fault_type, fault);
            assert!(e.max_response_time_ms > 0);
        }
    }

    assert_eq!(matrix.len(), ALL_FAULTS.len());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. JitterMetrics reset soak — repeated collect/reset cycles
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_jitter_metrics_reset_cycles() -> Result<()> {
    for cycle in 0u32..500 {
        let mut metrics = JitterMetrics::new();

        for tick in 0u32..1_000 {
            let jitter_ns = ((tick as u64 * 7 + cycle as u64 * 13) % 200_000) + 1_000;
            metrics.record_tick(jitter_ns, false);
        }

        assert_eq!(
            metrics.total_ticks, 1_000,
            "cycle {cycle}: total_ticks mismatch"
        );

        let p99 = metrics.p99_jitter_ns();
        assert!(p99 > 0, "cycle {cycle}: p99 should be positive");

        // Verify fresh metrics start clean
        let fresh = JitterMetrics::new();
        assert_eq!(
            fresh.total_ticks, 0,
            "cycle {cycle}: fresh metrics not zeroed"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Telemetry scenario soak — all scenarios back-to-back
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_telemetry_all_scenarios_sequential() -> Result<()> {
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];

    for round in 0u32..100 {
        for (idx, &scenario) in scenarios.iter().enumerate() {
            let recording = TestFixtureGenerator::generate_test_scenario(scenario, 2.0, 60.0);

            assert_eq!(
                recording.frames.len(),
                120,
                "round {round} scenario {idx}: unexpected frame count"
            );

            for frame in &recording.frames {
                assert!(
                    frame.data.rpm >= 0.0,
                    "round {round} scenario {idx}: negative RPM"
                );
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 18. Concurrent profile reads under sustained write load
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_concurrent_profile_reads_under_writes() -> Result<(), BoxErr> {
    const NUM_THREADS: usize = 8;
    const ITERATIONS: usize = 2_000;

    let profile = Arc::new(RwLock::new(
        WheelProfile::new("default", "dev-0").with_settings(WheelSettings::default()),
    ));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let profile = Arc::clone(&profile);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                barrier.wait();
                for i in 0..ITERATIONS {
                    if tid % 3 == 0 {
                        let name = format!("soak-profile-{tid}-{i}");
                        let mut settings = WheelSettings::default();
                        settings.ffb.overall_gain = (i as f32 / ITERATIONS as f32).clamp(0.0, 1.0);
                        let new_prof = WheelProfile::new(&name, "dev-0").with_settings(settings);
                        let mut p = profile.write().map_err(|e| format!("write: {e}"))?;
                        *p = new_prof;
                    } else {
                        let p = profile.read().map_err(|e| format!("read: {e}"))?;
                        assert!(
                            p.settings.ffb.overall_gain >= 0.0,
                            "gain must be non-negative"
                        );
                        let _name = &p.name;
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    Ok(())
}
