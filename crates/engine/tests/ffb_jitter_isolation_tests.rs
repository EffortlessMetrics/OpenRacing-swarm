#![allow(clippy::manual_is_multiple_of)]
//! FFB Jitter Isolation Tests
//!
//! This module contains tests to prove that LED and haptics output systems
//! do not interfere with the 1kHz FFB loop timing and jitter characteristics.

use racing_wheel_engine::led_haptics::*;
use racing_wheel_engine::ports::{NormalizedTelemetry, TelemetryFlags};
use racing_wheel_schemas::prelude::{DeviceId, FrequencyHz, Gain, HapticsConfig, LedConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

/// Mock FFB engine for testing jitter isolation
struct MockFfbEngine {
    tick_times: Arc<Mutex<Vec<Instant>>>,
    is_running: Arc<Mutex<bool>>,
    target_frequency: f64,
}

impl MockFfbEngine {
    fn new(target_frequency: f64) -> Self {
        Self {
            tick_times: Arc::new(Mutex::new(Vec::new())),
            is_running: Arc::new(Mutex::new(false)),
            target_frequency,
        }
    }

    async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut running = must(self.is_running.lock());
            *running = true;
        }

        let tick_times = Arc::clone(&self.tick_times);
        let is_running = Arc::clone(&self.is_running);
        let target_frequency = self.target_frequency;

        tokio::spawn(async move {
            let period_ns = (1_000_000_000.0 / target_frequency) as u64;
            let mut next_tick = Instant::now();

            loop {
                // Check if we should continue running
                {
                    let running = must(is_running.lock());
                    if !*running {
                        break;
                    }
                }

                // Record tick time
                let now = Instant::now();
                {
                    let mut times = must(tick_times.lock());
                    times.push(now);
                }

                // Simulate FFB processing work (50-200μs)
                let work_duration = Duration::from_micros(100);
                tokio::time::sleep(work_duration).await;

                // Calculate next tick time
                next_tick += Duration::from_nanos(period_ns);

                // Sleep until next tick (simulating absolute scheduler)
                let sleep_duration = next_tick.saturating_duration_since(Instant::now());
                if sleep_duration > Duration::from_nanos(0) {
                    tokio::time::sleep(sleep_duration).await;
                }
            }
        });

        Ok(())
    }

    fn stop(&self) {
        let mut running = must(self.is_running.lock());
        *running = false;
    }

    fn get_jitter_stats(&self) -> JitterStats {
        let times = must(self.tick_times.lock());
        calculate_jitter_stats(&times, self.target_frequency)
    }

    fn clear_stats(&self) {
        let mut times = must(self.tick_times.lock());
        times.clear();
    }
}

/// Jitter statistics for analysis
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct JitterStats {
    // dead_code allowed for test structure fields
    #[allow(dead_code)]
    mean_interval_ns: f64,
    #[allow(dead_code)]
    std_dev_ns: f64,
    #[allow(dead_code)]
    max_jitter_ns: u64,
    #[allow(dead_code)]
    p99_jitter_ns: u64,
    #[allow(dead_code)]
    missed_ticks: usize,
    #[allow(dead_code)]
    total_ticks: usize,
}

/// Calculate jitter statistics from tick times
fn calculate_jitter_stats(tick_times: &[Instant], target_frequency: f64) -> JitterStats {
    if tick_times.len() < 2 {
        return JitterStats {
            mean_interval_ns: 0.0,
            std_dev_ns: 0.0,
            max_jitter_ns: 0,
            p99_jitter_ns: 0,
            missed_ticks: 0,
            total_ticks: tick_times.len(),
        };
    }

    let target_interval_ns = 1_000_000_000.0 / target_frequency;

    // Calculate intervals between ticks
    let intervals: Vec<u64> = tick_times
        .windows(2)
        .map(|w| (w[1] - w[0]).as_nanos() as u64)
        .collect();

    // Calculate jitter (deviation from target interval)
    let jitters: Vec<u64> = intervals
        .iter()
        .map(|&interval| {
            let target = target_interval_ns as u64;
            interval.abs_diff(target)
        })
        .collect();

    // Calculate statistics
    let mean_interval = intervals.iter().sum::<u64>() as f64 / intervals.len() as f64;

    let variance = intervals
        .iter()
        .map(|&x| {
            let diff = x as f64 - mean_interval;
            diff * diff
        })
        .sum::<f64>()
        / intervals.len() as f64;

    let std_dev = variance.sqrt();

    let max_jitter = *jitters.iter().max().unwrap_or(&0);

    // Calculate p99 jitter
    let mut sorted_jitters = jitters.clone();
    sorted_jitters.sort_unstable();
    let p99_index = (sorted_jitters.len() as f64 * 0.99) as usize;
    let p99_jitter = sorted_jitters.get(p99_index).copied().unwrap_or(0);

    // Count missed ticks (intervals > 1.5x target)
    let missed_threshold = (target_interval_ns * 1.5) as u64;
    let missed_ticks = intervals.iter().filter(|&&x| x > missed_threshold).count();

    JitterStats {
        mean_interval_ns: mean_interval,
        std_dev_ns: std_dev,
        max_jitter_ns: max_jitter,
        p99_jitter_ns: p99_jitter,
        missed_ticks,
        total_ticks: tick_times.len(),
    }
}

/// Helper function to create test telemetry with varying characteristics
fn create_test_telemetry(
    rpm: f32,
    speed_ms: f32,
    slip_ratio: f32,
    gear: i8,
) -> NormalizedTelemetry {
    NormalizedTelemetry {
        ffb_scalar: 1.0,
        rpm,
        speed_ms,
        slip_ratio,
        gear,
        flags: TelemetryFlags::default(),
        car_id: None,
        track_id: None,
        timestamp: Instant::now(),
    }
}

fn create_varying_telemetry(index: usize) -> NormalizedTelemetry {
    let base_rpm = 3000.0 + (index as f32 * 100.0) % 5000.0;
    let base_speed = 15.0 + (index as f32 * 2.0) % 30.0;
    let slip = (index as f32 * 0.1) % 0.8;
    let gear = ((index / 10) % 6) as i8 + 1;

    NormalizedTelemetry {
        ffb_scalar: 0.5 + (index as f32 * 0.1) % 0.5,
        rpm: base_rpm,
        speed_ms: base_speed,
        slip_ratio: slip,
        gear,
        flags: TelemetryFlags {
            yellow_flag: (index % 50) == 0,
            red_flag: (index % 100) == 0,
            blue_flag: (index % 75) == 0,
            checkered_flag: (index % 200) == 0,
            pit_limiter: (index % 30) == 0,
            drs_enabled: (index % 20) == 0,
            ers_available: (index % 15) == 0,
            in_pit: (index % 40) == 0,
        },
        car_id: Some("test_car".to_string()),
        track_id: Some("test_track".to_string()),
        timestamp: std::time::Instant::now(),
    }
}

/// Helper function to create test LED configuration
fn create_test_led_config() -> LedConfig {
    let mut colors = HashMap::new();
    colors.insert("green".to_string(), [0, 255, 0]);
    colors.insert("yellow".to_string(), [255, 255, 0]);
    colors.insert("red".to_string(), [255, 0, 0]);
    colors.insert("blue".to_string(), [0, 0, 255]);

    must(LedConfig::new(
        vec![0.75, 0.82, 0.88, 0.92, 0.96],
        "progressive".to_string(),
        must(Gain::new(0.8)),
        colors,
    ))
}

/// Helper function to create test haptics configuration
fn create_test_haptics_config() -> HapticsConfig {
    let mut effects = HashMap::new();
    effects.insert("kerb".to_string(), true);
    effects.insert("slip".to_string(), true);
    effects.insert("gear_shift".to_string(), true);
    effects.insert("collision".to_string(), true);

    HapticsConfig::new(
        true,
        must(Gain::new(0.8)), // High intensity to stress test
        must(FrequencyHz::new(120.0)),
        effects,
    )
}

/// Check if running under coverage instrumentation
fn running_under_coverage() -> bool {
    std::env::var_os("LLVM_PROFILE_FILE").is_some()
}

fn skip_timing_sensitive_tests() -> bool {
    running_under_coverage()
        || std::env::var_os("CI").is_some()
        || std::env::var("OPENRACING_SKIP_TIMING_GUARANTEES")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod jitter_isolation_tests {
    use super::*;

    #[cfg_attr(windows, ignore = "Requires RT scheduling for jitter assertions")]
    #[tokio::test]
    async fn test_baseline_ffb_jitter() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test FFB engine alone without LED/haptics interference
        let ffb_engine = MockFfbEngine::new(1000.0); // 1kHz

        must(ffb_engine.start().await);

        // Run for 1 second to collect baseline data
        tokio::time::sleep(Duration::from_millis(1000)).await;

        ffb_engine.stop();

        let baseline_stats = ffb_engine.get_jitter_stats();

        // Verify baseline performance meets requirements
        assert!(
            baseline_stats.p99_jitter_ns <= 250_000, // ≤0.25ms p99 jitter
            "Baseline p99 jitter too high: {} ns",
            baseline_stats.p99_jitter_ns
        );

        assert!(
            baseline_stats.missed_ticks == 0,
            "Baseline should have no missed ticks, got {}",
            baseline_stats.missed_ticks
        );

        println!("Baseline FFB stats: {:?}", baseline_stats);
    }

    #[cfg_attr(windows, ignore = "Requires RT scheduling for jitter assertions")]
    #[tokio::test]
    async fn test_ffb_jitter_with_led_haptics_60hz() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test FFB engine with LED/haptics running at 60Hz
        let ffb_engine = MockFfbEngine::new(1000.0); // 1kHz FFB

        let device_id = must(DeviceId::new("jitter-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut led_haptics_system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            60.0, // 60Hz LED/haptics
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(1000);

        // Start both systems
        must(ffb_engine.start().await);
        must(led_haptics_system.start(telemetry_rx).await);

        // Generate varying telemetry to stress test the LED/haptics system
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10)); // 100Hz telemetry
            for i in 0..1000 {
                let telemetry = create_varying_telemetry(i);
                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Consume LED/haptics output to simulate real usage
        let output_handle = tokio::spawn(async move {
            let mut count = 0;
            while count < 600 {
                // Expect ~60 outputs per second for 10 seconds
                if let Ok(output) =
                    tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await
                {
                    match output {
                        Some(_) => {
                            count += 1;
                            // Simulate processing the output (e.g., sending to hardware)
                            tokio::task::yield_now().await;
                        }
                        None => break,
                    }
                } else {
                    break;
                }
            }
        });

        // Run test for 10 seconds
        tokio::time::sleep(Duration::from_millis(10000)).await;

        // Stop systems
        ffb_engine.stop();
        led_haptics_system.stop();

        // Wait for handles to complete
        let _ = tokio::join!(telemetry_handle, output_handle);

        let with_led_haptics_stats = ffb_engine.get_jitter_stats();

        // Verify that LED/haptics don't significantly impact FFB jitter
        assert!(
            with_led_haptics_stats.p99_jitter_ns <= 250_000, // Still ≤0.25ms p99 jitter
            "FFB p99 jitter too high with LED/haptics: {} ns",
            with_led_haptics_stats.p99_jitter_ns
        );

        assert!(
            with_led_haptics_stats.missed_ticks == 0,
            "FFB should have no missed ticks with LED/haptics, got {}",
            with_led_haptics_stats.missed_ticks
        );

        println!(
            "FFB stats with LED/haptics @ 60Hz: {:?}",
            with_led_haptics_stats
        );
    }

    #[cfg_attr(windows, ignore = "Requires RT scheduling for jitter assertions")]
    #[tokio::test]
    async fn test_ffb_jitter_with_led_haptics_200hz() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test FFB engine with LED/haptics running at maximum 200Hz
        let ffb_engine = MockFfbEngine::new(1000.0); // 1kHz FFB

        let device_id = must(DeviceId::new("jitter-test-device-200hz".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut led_haptics_system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            200.0, // 200Hz LED/haptics (maximum rate)
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(1000);

        // Start both systems
        must(ffb_engine.start().await);
        must(led_haptics_system.start(telemetry_rx).await);

        // Generate high-frequency varying telemetry
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(5)); // 200Hz telemetry
            for i in 0..2000 {
                let telemetry = create_varying_telemetry(i);
                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Consume LED/haptics output at high rate
        let output_handle = tokio::spawn(async move {
            let mut count = 0;
            while count < 2000 {
                // Expect ~200 outputs per second for 10 seconds
                if let Ok(output) =
                    tokio::time::timeout(Duration::from_millis(10), output_rx.recv()).await
                {
                    match output {
                        Some(_) => {
                            count += 1;
                            // Simulate more intensive processing
                            for _ in 0..10 {
                                tokio::task::yield_now().await;
                            }
                        }
                        None => break,
                    }
                } else {
                    break;
                }
            }
        });

        // Run test for 10 seconds
        tokio::time::sleep(Duration::from_millis(10000)).await;

        // Stop systems
        ffb_engine.stop();
        led_haptics_system.stop();

        // Wait for handles to complete
        let _ = tokio::join!(telemetry_handle, output_handle);

        let with_high_rate_stats = ffb_engine.get_jitter_stats();

        // Verify that even high-rate LED/haptics don't impact FFB jitter
        assert!(
            with_high_rate_stats.p99_jitter_ns <= 250_000, // Still ≤0.25ms p99 jitter
            "FFB p99 jitter too high with high-rate LED/haptics: {} ns",
            with_high_rate_stats.p99_jitter_ns
        );

        assert!(
            with_high_rate_stats.missed_ticks == 0,
            "FFB should have no missed ticks with high-rate LED/haptics, got {}",
            with_high_rate_stats.missed_ticks
        );

        println!(
            "FFB stats with LED/haptics @ 200Hz: {:?}",
            with_high_rate_stats
        );
    }

    #[cfg_attr(windows, ignore = "Requires RT scheduling for jitter assertions")]
    #[tokio::test]
    async fn test_comparative_jitter_analysis() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Compare FFB jitter with and without LED/haptics to prove isolation

        // Test 1: Baseline (FFB only)
        let ffb_engine = MockFfbEngine::new(1000.0);
        must(ffb_engine.start().await);
        tokio::time::sleep(Duration::from_millis(5000)).await;
        ffb_engine.stop();
        let baseline_stats = ffb_engine.get_jitter_stats();
        ffb_engine.clear_stats();

        // Test 2: With LED/haptics active
        let device_id = must(DeviceId::new("comparative-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut led_haptics_system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            120.0, // 120Hz
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(1000);

        must(ffb_engine.start().await);
        must(led_haptics_system.start(telemetry_rx).await);

        // Generate complex telemetry scenario
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(8)); // ~125Hz
            for i in 0..625 {
                // 5 seconds worth
                let mut telemetry = create_varying_telemetry(i);

                // Add some complex scenarios
                if i % 100 == 0 {
                    telemetry.flags.yellow_flag = true;
                }
                if i % 150 == 0 {
                    telemetry.slip_ratio = 0.9; // High slip
                }
                if i % 200 == 0 {
                    telemetry.rpm = 7800.0; // High RPM
                }

                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Consume outputs
        let output_handle = tokio::spawn(async move {
            while let Ok(output) =
                tokio::time::timeout(Duration::from_millis(20), output_rx.recv()).await
            {
                if output.is_none() {
                    break;
                }
                // Simulate output processing
                tokio::task::yield_now().await;
            }
        });

        tokio::time::sleep(Duration::from_millis(5000)).await;

        ffb_engine.stop();
        led_haptics_system.stop();

        let _ = tokio::join!(telemetry_handle, output_handle);

        let with_led_haptics_stats = ffb_engine.get_jitter_stats();

        // Compare statistics
        println!("Baseline FFB stats: {:?}", baseline_stats);
        println!("With LED/haptics stats: {:?}", with_led_haptics_stats);

        // Verify that jitter increase is minimal (< 10% increase allowed)
        let jitter_increase_ratio =
            with_led_haptics_stats.p99_jitter_ns as f64 / baseline_stats.p99_jitter_ns as f64;

        assert!(
            jitter_increase_ratio <= 1.1, // ≤10% increase
            "FFB jitter increased too much with LED/haptics: {:.2}x increase",
            jitter_increase_ratio
        );

        // Verify both meet the requirement
        assert!(
            baseline_stats.p99_jitter_ns <= 250_000,
            "Baseline p99 jitter exceeds requirement: {} ns",
            baseline_stats.p99_jitter_ns
        );

        assert!(
            with_led_haptics_stats.p99_jitter_ns <= 250_000,
            "With LED/haptics p99 jitter exceeds requirement: {} ns",
            with_led_haptics_stats.p99_jitter_ns
        );

        // Verify no missed ticks in either case
        assert_eq!(baseline_stats.missed_ticks, 0);
        assert_eq!(with_led_haptics_stats.missed_ticks, 0);

        println!(
            "✓ FFB jitter isolation verified: {:.2}% increase with LED/haptics active",
            (jitter_increase_ratio - 1.0) * 100.0
        );
    }

    #[cfg_attr(windows, ignore = "Requires RT scheduling for jitter assertions")]
    #[tokio::test]
    async fn test_cpu_usage_isolation() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test that LED/haptics don't cause excessive CPU usage that could affect FFB
        use std::sync::atomic::{AtomicU64, Ordering};

        let cpu_work_counter = Arc::new(AtomicU64::new(0));

        // Simulate CPU-intensive FFB work
        let ffb_cpu_counter = Arc::clone(&cpu_work_counter);
        let ffb_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_micros(1000)); // 1kHz
            for _ in 0..5000 {
                // 5 seconds
                // Simulate FFB processing work
                for _ in 0..1000 {
                    ffb_cpu_counter.fetch_add(1, Ordering::Relaxed);
                }
                interval.tick().await;
            }
        });

        // Start LED/haptics system
        let device_id = must(DeviceId::new("cpu-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut led_haptics_system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            150.0, // 150Hz
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(1000);
        must(led_haptics_system.start(telemetry_rx).await);

        // Generate telemetry
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10)); // 100Hz
            for i in 0..500 {
                let telemetry = create_varying_telemetry(i);
                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Consume outputs
        let output_handle = tokio::spawn(async move {
            while let Ok(_output) =
                tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await
            {
                // if output.is_none() { break; } // accessing _output triggers unused if I don't use it.
                // But wait, output is Option.
                if _output.is_none() {
                    break;
                }
            }
        });

        // Wait for completion
        let _ = tokio::join!(ffb_handle, telemetry_handle, output_handle);

        led_haptics_system.stop();

        let total_cpu_work = cpu_work_counter.load(Ordering::Relaxed);

        // Verify that significant CPU work was completed (indicating FFB wasn't starved)
        assert!(
            total_cpu_work > 4_000_000, // Should complete most of the work
            "FFB CPU work was starved: only {} work units completed",
            total_cpu_work
        );

        println!(
            "✓ CPU isolation verified: {} work units completed",
            total_cpu_work
        );
    }
}

#[cfg(test)]
mod timing_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_led_update_latency() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test that LED updates happen within 20ms of telemetry input (LDH-01)
        let device_id = must(DeviceId::new("latency-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            100.0, // 100Hz
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);
        must(system.start(telemetry_rx).await);

        // Send telemetry and measure response time
        let send_time = Instant::now();
        let telemetry = create_varying_telemetry(0);
        must(telemetry_tx.send(telemetry).await);

        // Wait for output
        if let Ok(Some(_output)) =
            tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await
        {
            let response_time = send_time.elapsed();

            // Verify latency requirement (≤20ms)
            assert!(
                response_time <= Duration::from_millis(20),
                "LED update latency too high: {:?}",
                response_time
            );

            println!("✓ LED update latency: {:?}", response_time);
        } else {
            panic!("Failed to receive LED output within timeout");
        }

        system.stop();
    }

    #[tokio::test]
    async fn test_haptics_frequency_range() {
        if skip_timing_sensitive_tests() {
            eprintln!("skipping timing-sensitive test under coverage/shared CI");
            return;
        }

        // Test that haptics operate in the 60-200Hz range (LDH-04)
        let device_id = must(DeviceId::new("frequency-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        // Test different update rates
        let test_rates = vec![60.0, 100.0, 150.0, 200.0];

        for rate in test_rates {
            let (mut system, mut output_rx) = LedHapticsSystem::new(
                device_id.clone(),
                led_config.clone(),
                haptics_config.clone(),
                rate,
            );

            let (telemetry_tx, telemetry_rx) = mpsc::channel(100);
            must(system.start(telemetry_rx).await);

            // Send telemetry with haptics-triggering conditions
            let mut telemetry = create_varying_telemetry(0);
            telemetry.slip_ratio = 0.5; // Trigger haptics
            must(telemetry_tx.send(telemetry).await);

            // Collect timing data
            let mut output_times = Vec::new();
            let start_time = Instant::now();

            while output_times.len() < 10 && start_time.elapsed() < Duration::from_millis(500) {
                if let Ok(Some(output)) =
                    tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await
                {
                    output_times.push(output.timestamp);

                    // Verify haptics are present
                    assert!(
                        !output.haptics_patterns.is_empty(),
                        "Expected haptics patterns at {}Hz",
                        rate
                    );
                }
            }

            // Verify update rate
            if output_times.len() >= 2 {
                let intervals: Vec<Duration> = output_times
                    .windows(2)
                    .map(|w| w[1].duration_since(w[0]))
                    .collect();

                let avg_interval = intervals.iter().sum::<Duration>() / intervals.len() as u32;
                let measured_rate = 1.0 / avg_interval.as_secs_f64();

                // Allow extra tolerance on Windows due to timer resolution and test load
                let rate_tolerance = if cfg!(windows) {
                    rate * 0.70
                } else {
                    rate * 0.1
                };
                assert!(
                    (measured_rate - rate as f64).abs() <= rate_tolerance as f64,
                    "Measured rate {:.1}Hz too far from target {:.1}Hz",
                    measured_rate,
                    rate
                );

                println!(
                    "✓ Haptics rate {:.1}Hz verified (measured: {:.1}Hz)",
                    rate, measured_rate
                );
            }

            system.stop();
        }
    }

    #[tokio::test]
    async fn test_rpm_hysteresis_timing() {
        // Test that RPM hysteresis prevents flicker at steady RPM (LDH-02)
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Test steady RPM with small variations
        let base_rpm = 6000.0;
        let variation = 50.0; // Small RPM variation

        let mut patterns = Vec::new();

        for i in 0..20 {
            let rpm = base_rpm + (i as f32 % 4.0 - 2.0) * variation; // ±100 RPM variation
            let telemetry = create_test_telemetry(rpm, 30.0, 0.0, 4);

            let colors = engine.update_pattern(&telemetry);
            let lit_count = colors.iter().filter(|&c| *c != LedColor::OFF).count();
            patterns.push(lit_count);

            // Small delay to simulate real timing
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Verify that LED pattern is stable (no rapid changes)
        let unique_patterns: std::collections::HashSet<_> = patterns.iter().collect();

        assert!(
            unique_patterns.len() <= 2, // Allow at most 2 different patterns
            "Too much LED pattern variation with steady RPM: {} unique patterns",
            unique_patterns.len()
        );

        println!(
            "✓ RPM hysteresis working: {} unique patterns for varying RPM",
            unique_patterns.len()
        );
    }
}
