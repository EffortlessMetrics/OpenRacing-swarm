//! Stress tests for system resilience under extreme conditions.
//!
//! These tests push the system beyond normal operating parameters to verify
//! correct error handling, bounded resource usage, and recovery behavior.
//! Iteration counts are kept reasonable so tests complete in seconds while
//! still exercising boundary and overload code paths.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;

use openracing_filters::Frame;
use openracing_fmea::{FaultType, FmeaSystem, SoftStopController};
use openracing_pipeline::Pipeline;
use openracing_profile::{WheelProfile, WheelSettings};
use openracing_telemetry_recorder::TestFixtureGenerator;
use openracing_telemetry_streams::TelemetryBuffer;
use openracing_watchdog::{SystemComponent, WatchdogConfig, WatchdogSystem};
use racing_wheel_engine::JitterMetrics;
use racing_wheel_engine::safety::{
    SafetyInterlockState, SafetyInterlockSystem, SafetyService, SoftwareWatchdog,
};

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
// 1. Maximum concurrent device connections
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_max_concurrent_device_connections() -> Result<()> {
    let device_count = 16;
    let mut devices: Vec<(SafetyInterlockSystem, Pipeline)> = (0..device_count)
        .map(|_| (make_interlock(25.0, 500), Pipeline::new()))
        .collect();

    for (interlock, _) in &mut devices {
        interlock.arm()?;
    }

    // Run all devices simultaneously for 1000 ticks
    for tick in 0u64..1_000 {
        for (dev_idx, (interlock, pipeline)) in devices.iter_mut().enumerate() {
            let input = ((tick as f32 + dev_idx as f32 * 0.5) * 0.1).sin() * 0.7;
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
// 2. Rapid connect/disconnect cycling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_rapid_connect_disconnect_500_cycles() -> Result<()> {
    for cycle in 0u32..500 {
        let mut interlock = make_interlock(25.0, 500);
        interlock.arm()?;

        // Short burst
        for tick in 0..20 {
            let result = interlock.process_tick(10.0);
            assert!(
                !result.fault_occurred,
                "cycle {cycle} tick {tick}: fault during normal operation"
            );
        }

        interlock.disarm()?;
        interlock.reset()?;

        assert_eq!(
            interlock.state(),
            &SafetyInterlockState::Normal,
            "cycle {cycle}: not Normal after reset"
        );
        assert!(
            !interlock.is_watchdog_armed(),
            "cycle {cycle}: watchdog still armed"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Maximum telemetry data rate
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_max_telemetry_data_rate() -> Result<()> {
    let buffer = TelemetryBuffer::<u64>::new(4096);

    // Push data at maximum rate
    for i in 0u64..50_000 {
        buffer.push(i);
    }

    // Buffer should be bounded
    assert!(
        buffer.len() <= 4096,
        "buffer unbounded: len={}",
        buffer.len()
    );

    // Latest value should be from the end of the sequence
    let latest = buffer.latest();
    assert!(latest.is_some(), "buffer lost all data");
    if let Some(val) = latest {
        assert!(
            val >= 50_000 - 4096,
            "latest value {val} too old — data not advancing"
        );
    }

    // Drain completely
    let mut drained = 0usize;
    while buffer.pop().is_some() {
        drained += 1;
    }
    assert!(drained <= 4096, "drained more than capacity: {drained}");
    assert!(buffer.is_empty());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Large profile storage (hundreds of profiles)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_large_profile_storage() -> Result<()> {
    let mut profiles: Vec<WheelProfile> = Vec::with_capacity(500);

    for i in 0u32..500 {
        let name = format!("stress-profile-{i:04}");
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = (i as f32 / 500.0).clamp(0.0, 1.0);
        settings.ffb.torque_limit = 5.0 + (i as f32 % 20.0);
        settings.ffb.damper_strength = (i as f32 * 0.002) % 1.0;
        settings.input.steering_range = 180 + (i as u16 % 720);

        let profile = WheelProfile::new(&name, format!("dev-{}", i % 10)).with_settings(settings);
        profiles.push(profile);
    }

    assert_eq!(profiles.len(), 500);

    // Verify each profile retained its identity
    for (i, profile) in profiles.iter().enumerate() {
        let expected_name = format!("stress-profile-{i:04}");
        assert_eq!(profile.name, expected_name, "profile {i}: name mismatch");
        assert!(
            profile.settings.ffb.overall_gain >= 0.0 && profile.settings.ffb.overall_gain <= 1.0,
            "profile {i}: gain out of range"
        );
    }

    // Serialization round-trip for all profiles
    for (i, profile) in profiles.iter().enumerate() {
        let json = serde_json::to_string(profile)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(
            restored.name, profile.name,
            "profile {i}: round-trip name mismatch"
        );
        assert!(
            (restored.settings.ffb.overall_gain - profile.settings.ffb.overall_gain).abs()
                < f32::EPSILON,
            "profile {i}: round-trip gain mismatch"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Error recovery under continuous error injection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_error_recovery_continuous_fault_injection() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    let mut successful_recoveries = 0u32;
    let mut total_injections = 0u32;

    for cycle in 0u32..200 {
        let fault = ALL_FAULTS[cycle as usize % ALL_FAULTS.len()];
        interlock.report_fault(fault);
        total_injections += 1;

        // Process ticks in faulted state — torque must be limited
        for tick in 0..10 {
            let result = interlock.process_tick(20.0);
            assert!(
                result.torque_command.abs() <= 25.0 + f32::EPSILON,
                "cycle {cycle} tick {tick}: torque {} exceeds max in faulted state",
                result.torque_command
            );
        }

        // Wait the minimum fault duration then attempt recovery
        std::thread::sleep(Duration::from_millis(110));
        match interlock.clear_fault() {
            Ok(()) => {
                successful_recoveries += 1;
                assert_eq!(
                    interlock.state(),
                    &SafetyInterlockState::Normal,
                    "cycle {cycle}: not Normal after clear_fault"
                );
            }
            Err(msg) => {
                assert!(
                    msg.contains("100ms") || msg.contains("No fault"),
                    "cycle {cycle}: unexpected error: {msg}"
                );
            }
        }
    }

    assert!(
        successful_recoveries > 0,
        "no successful recoveries across {total_injections} injections"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Concurrent telemetry producers/consumers under stress
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_concurrent_telemetry_producers_consumers() -> Result<(), BoxErr> {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const ITEMS_PER_PRODUCER: usize = 5_000;

    let buffer = Arc::new(TelemetryBuffer::<u64>::new(1024));
    let produced = Arc::new(AtomicU64::new(0));
    let consumed = Arc::new(AtomicU64::new(0));
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(PRODUCERS + CONSUMERS));

    let mut handles: Vec<thread::JoinHandle<Result<(), BoxErr>>> = Vec::new();

    for pid in 0..PRODUCERS {
        let buf = Arc::clone(&buffer);
        let bar = Arc::clone(&barrier);
        let prod = Arc::clone(&produced);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            for seq in 0..ITEMS_PER_PRODUCER {
                let value = (pid as u64) * 100_000 + seq as u64;
                buf.push(value);
                prod.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    for _ in 0..CONSUMERS {
        let buf = Arc::clone(&buffer);
        let bar = Arc::clone(&barrier);
        let cons = Arc::clone(&consumed);
        let d = Arc::clone(&done);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            loop {
                if let Some(_val) = buf.pop() {
                    cons.fetch_add(1, Ordering::Relaxed);
                } else if d.load(Ordering::Acquire) {
                    while buf.pop().is_some() {
                        cons.fetch_add(1, Ordering::Relaxed);
                    }
                    break;
                }
                thread::yield_now();
            }
            Ok(())
        }));
    }

    // Wait for producers first
    for h in handles.drain(..PRODUCERS) {
        h.join().map_err(|_| "producer panicked")??;
    }
    done.store(true, Ordering::Release);

    for h in handles {
        h.join().map_err(|_| "consumer panicked")??;
    }

    let total_produced = produced.load(Ordering::SeqCst);
    let total_consumed = consumed.load(Ordering::SeqCst);
    assert_eq!(
        total_produced,
        (PRODUCERS * ITEMS_PER_PRODUCER) as u64,
        "produced count mismatch"
    );
    // consumed ≤ produced due to ring buffer overflow
    assert!(
        total_consumed <= total_produced,
        "consumed {total_consumed} > produced {total_produced}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Watchdog plugin quarantine stress
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_watchdog_plugin_quarantine() -> Result<(), BoxErr> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(200)
        .plugin_max_timeouts(5)
        .plugin_quarantine_duration(Duration::from_millis(100))
        .rt_thread_timeout_ms(100)
        .hid_timeout_ms(100)
        .build()
        .map_err(|e| format!("config: {e}"))?;

    let watchdog = WatchdogSystem::new(config);

    // Drive a plugin past its timeout threshold to trigger quarantine
    for _ in 0..20 {
        // Exceed the 200µs budget
        let _fault = watchdog.record_plugin_execution("stress_plugin", 500);
    }

    assert!(
        watchdog.is_plugin_quarantined("stress_plugin"),
        "plugin should be quarantined after exceeding timeout count"
    );

    // Release quarantine (immediately, while still active)
    let release = watchdog.release_plugin_quarantine("stress_plugin");
    assert!(release.is_ok(), "quarantine release failed");

    // Plugin should work again
    let fault = watchdog.record_plugin_execution("stress_plugin", 50);
    assert!(fault.is_none(), "unexpected fault after quarantine release");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Pipeline process with extreme input values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_pipeline_extreme_input_values() -> Result<()> {
    let mut pipeline = Pipeline::new();

    let extreme_values: &[f32] = &[
        0.0,
        -0.0,
        1.0,
        -1.0,
        f32::MIN,
        f32::MAX,
        f32::EPSILON,
        -f32::EPSILON,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.999_999_9,
        -0.999_999_9,
    ];

    for (i, &value) in extreme_values.iter().enumerate() {
        let mut frame = Frame {
            ffb_in: value,
            torque_out: value,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: i as u64 * 1_000_000,
            seq: i as u16,
        };

        // Empty pipeline is passthrough — must not panic
        let result = pipeline.process(&mut frame);
        assert!(
            result.is_ok(),
            "pipeline failed on extreme value {value}: {:?}",
            result.err()
        );
    }

    // Repeat many times to check for state accumulation issues
    for tick in 0u64..10_000 {
        let value = extreme_values[(tick as usize) % extreme_values.len()];
        let mut frame = Frame {
            ffb_in: value,
            torque_out: value,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: tick * 1_000_000,
            seq: (tick & 0xFFFF) as u16,
        };
        let _ = pipeline.process(&mut frame);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. FMEA fault cycling stress — all fault types in rapid succession
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_fmea_all_fault_types_rapid_cycling() -> Result<()> {
    let mut fmea = FmeaSystem::new();

    for cycle in 0u32..1_000 {
        let fault = ALL_FAULTS[cycle as usize % ALL_FAULTS.len()];

        // Drive detection functions
        match fault {
            FaultType::UsbStall => {
                let _ = fmea.detect_usb_fault(100, Some(Duration::from_millis(10)));
            }
            FaultType::EncoderNaN => {
                let _ = fmea.detect_encoder_fault(f32::NAN);
            }
            FaultType::ThermalLimit => {
                let _ = fmea.detect_thermal_fault(200.0, false);
            }
            FaultType::PluginOverrun => {
                let _ = fmea.detect_plugin_overrun("stress_plugin", 1_000_000);
            }
            FaultType::TimingViolation => {
                let _ = fmea.detect_timing_violation(1_000_000);
            }
            _ => {
                // For faults without direct detection methods, use handle_fault
                let _ = fmea.handle_fault(fault, 10.0);
            }
        }

        // Clear after each cycle to avoid sticky state blocking further detection
        let _ = fmea.clear_fault();
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Safety interlock stress — faulted-state always zero torque
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_faulted_state_zero_torque_all_inputs() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::Overcurrent);

    for tick in 0u64..10_000 {
        // Try every possible input pattern
        let raw = match tick % 4 {
            0 => 100.0,
            1 => -100.0,
            2 => f32::MAX,
            _ => f32::NAN,
        };
        let clamped = safety.clamp_torque_nm(raw);
        assert!(
            clamped.abs() < f32::EPSILON,
            "tick {tick}: faulted state torque {clamped} != 0.0 for input {raw}"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Concurrent FMEA access stress
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_concurrent_fmea_access() -> Result<(), BoxErr> {
    const NUM_THREADS: usize = 8;
    const ITERATIONS: usize = 1_000;

    let fmea = Arc::new(Mutex::new(FmeaSystem::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let faults_detected = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let fmea = Arc::clone(&fmea);
            let bar = Arc::clone(&barrier);
            let faults = Arc::clone(&faults_detected);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    let mut sys = fmea.lock().map_err(|e| format!("lock: {e}"))?;
                    match tid % 4 {
                        0 => {
                            if sys
                                .detect_usb_fault(i as u32 % 10, Some(Duration::from_millis(100)))
                                .is_some()
                            {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        1 => {
                            let val = if i % 100 == 0 { f32::NAN } else { i as f32 };
                            if sys.detect_encoder_fault(val).is_some() {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        2 => {
                            let jitter = (i as u64 % 500) + 1;
                            if sys.detect_timing_violation(jitter).is_some() {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        _ => {
                            let temp = 40.0 + (i as f32 * 0.1);
                            if sys.detect_thermal_fault(temp, false).is_some() {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    // No assertions on fault count — just that we didn't crash
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Emergency stop stress — repeated e-stops
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_repeated_emergency_stops() -> Result<()> {
    for cycle in 0u32..500 {
        let mut interlock = make_interlock(25.0, 500);
        interlock.arm()?;

        // Run a few normal ticks
        for _ in 0..10 {
            let _ = interlock.process_tick(15.0);
        }

        // Emergency stop
        let estop = interlock.emergency_stop();
        assert!(
            estop.torque_command.abs() < f32::EPSILON,
            "cycle {cycle}: e-stop torque {} != 0.0",
            estop.torque_command
        );
        assert!(
            matches!(estop.state, SafetyInterlockState::EmergencyStop { .. }),
            "cycle {cycle}: not in EmergencyStop state"
        );

        // All subsequent ticks must be zero
        for tick in 0..20 {
            let result = interlock.process_tick(25.0);
            assert!(
                result.torque_command.abs() < f32::EPSILON,
                "cycle {cycle} tick {tick}: non-zero torque after e-stop: {}",
                result.torque_command
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Watchdog multi-component stress
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_watchdog_multi_component_health_checks() -> Result<(), BoxErr> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(500)
        .plugin_max_timeouts(10)
        .plugin_quarantine_duration(Duration::from_secs(30))
        .rt_thread_timeout_ms(500)
        .hid_timeout_ms(500)
        .build()
        .map_err(|e| format!("config: {e}"))?;

    let watchdog = WatchdogSystem::new(config);

    let components = [
        SystemComponent::RtThread,
        SystemComponent::HidCommunication,
        SystemComponent::TelemetryAdapter,
        SystemComponent::PluginHost,
        SystemComponent::SafetySystem,
        SystemComponent::DeviceManager,
    ];

    // Register many plugins
    for pid in 0..20 {
        let plugin_id = format!("stress_plugin_{pid}");
        let exec_time = 50 + (pid * 10);
        let _ = watchdog.record_plugin_execution(&plugin_id, exec_time);
    }

    // Rapid heartbeat cycles
    for _ in 0u32..5_000 {
        for component in &components {
            watchdog.heartbeat(*component);
        }

        // Run health checks periodically
        let faults = watchdog.perform_health_checks();
        // Faults are acceptable; we just verify no panic
        let _ = faults;
    }

    // Verify all components have health status
    let health = watchdog.get_all_component_health();
    assert!(!health.is_empty(), "no component health data after stress");

    assert!(!watchdog.has_faulted_components());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. JitterMetrics stress — high-jitter scenario
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_jitter_metrics_high_jitter_scenario() -> Result<()> {
    let mut metrics = JitterMetrics::new();

    for tick in 0u64..10_000 {
        // 5% of ticks have extreme jitter (> 250µs threshold)
        let jitter_ns = if tick % 20 == 0 {
            500_000 // 500µs — missed deadline
        } else {
            50_000 // 50µs — normal
        };
        let missed = jitter_ns > 250_000;
        metrics.record_tick(jitter_ns, missed);
    }

    assert_eq!(metrics.total_ticks, 10_000);

    let miss_rate = metrics.missed_tick_rate();
    // Expected ~5% miss rate
    assert!(
        (miss_rate - 0.05).abs() < 0.01,
        "miss rate {miss_rate} not close to 5%"
    );

    // p99 should reflect the spikes
    let p99 = metrics.p99_jitter_ns();
    assert!(
        p99 > 100_000,
        "p99 {p99}ns suspiciously low for high-jitter scenario"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Soft-stop stress — rapid start/reset cycling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_soft_stop_rapid_cycling() -> Result<()> {
    let mut controller = SoftStopController::new();

    for cycle in 0u32..2_000 {
        let start_torque = (cycle as f32 * 0.1) % 25.0;
        controller.start_soft_stop_with_duration(start_torque, Duration::from_millis(50));

        // Single update step
        let torque = controller.update(Duration::from_millis(1));
        assert!(
            torque.is_finite(),
            "cycle {cycle}: non-finite torque {torque}"
        );
        assert!(
            torque.abs() <= start_torque + f32::EPSILON,
            "cycle {cycle}: torque {torque} exceeds start {start_torque}"
        );

        controller.reset();
        assert!(
            !controller.is_active(),
            "cycle {cycle}: still active after reset"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Telemetry fixture stress — generate and verify many recordings
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_telemetry_fixture_generation() -> Result<()> {
    for i in 0u32..200 {
        let duration = 1.0 + (i as f32 % 5.0);
        let fps = 30.0 + (i as f32 % 30.0);
        let game_id = format!("stress_game_{i}");

        let recording =
            TestFixtureGenerator::generate_racing_session(game_id.clone(), duration, fps);

        let expected_frames = (duration * fps) as usize;
        assert_eq!(
            recording.frames.len(),
            expected_frames,
            "iteration {i}: frame count mismatch for duration={duration} fps={fps}"
        );
        assert_eq!(recording.metadata.game_id, game_id);

        for frame in &recording.frames {
            assert!(frame.data.rpm >= 0.0, "iteration {i}: negative RPM");
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Profile migration stress — round-trip hundreds of profiles
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_profile_migration_round_trip() -> Result<()> {
    for i in 0u32..500 {
        let mut profile = WheelProfile::new(format!("migrate-{i}"), format!("dev-{}", i % 8));

        // Simulate old schema
        profile.schema_version = 0;

        let migrated = openracing_profile::migrate_profile(&mut profile)?;
        assert!(migrated, "profile {i}: expected migration from v0");
        assert_eq!(
            profile.schema_version,
            openracing_profile::CURRENT_SCHEMA_VERSION,
            "profile {i}: schema version mismatch"
        );

        // Migrate again — should be no-op
        let again = openracing_profile::migrate_profile(&mut profile)?;
        assert!(!again, "profile {i}: second migration should be no-op");

        // Serialization round-trip
        let json = serde_json::to_string(&profile)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, profile.name);
        assert_eq!(restored.schema_version, profile.schema_version);
    }

    Ok(())
}
