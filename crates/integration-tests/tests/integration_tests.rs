//! Main integration test runner
//!
//! This module contains the primary integration tests that can be run with `cargo test`

use anyhow::Result;
use racing_wheel_integration_tests::*;
use std::time::Duration;

fn performance_gate_timeout(gate_duration: Duration) -> Duration {
    let margin = if gates::ci_gates_enabled() {
        Duration::from_secs(15)
    } else {
        Duration::from_secs(20)
    };

    gate_duration + margin
}

fn jitter_test_timeout() -> Duration {
    performance_gate_timeout(gates::ffb_jitter_measurement_duration())
}

fn jitter_p99_limit_ms() -> f64 {
    gates::ffb_jitter_p99_limit_ms()
}

fn hid_latency_test_timeout() -> Duration {
    performance_gate_timeout(gates::hid_latency_measurement_duration())
}

fn zero_missed_ticks_test_timeout() -> Duration {
    performance_gate_timeout(gates::zero_missed_ticks_measurement_duration())
}

fn skip_shared_ci_timing_guarantees() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var("OPENRACING_SKIP_TIMING_GUARANTEES")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
}

fn acceptance_subset_timeout() -> Duration {
    if gates::ci_gates_enabled() {
        // 180s gives headroom for slow CI runners while still detecting
        // genuine hangs. The previous 60/90s limits caused spurious timeouts
        // when sysinfo::System::new_all() scanned every host process and
        // service initialization was slower under CI resource contention.
        Duration::from_secs(180)
    } else {
        Duration::from_secs(180)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_user_journey_uj01_first_run() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = user_journeys::test_uj01_first_run().await?;

        if !result.passed {
            anyhow::bail!("UJ-01 test failed: {:?}", result.errors);
        }

        // Performance gate check is only meaningful with RT scheduling
        #[cfg(not(target_os = "windows"))]
        if !gates::ci_gates_enabled() && !result.metrics.meets_performance_gates() {
            anyhow::bail!("UJ-01 performance gates not met");
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_user_journey_uj01_first_run exceeded 30 second limit");
            eprintln!("Diagnostic: User journey UJ-01 (first run) did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Service initialization is blocked");
            eprintln!("  - Device enumeration is hanging");
            eprintln!("  - Resource contention in the test harness");
            anyhow::bail!("test_user_journey_uj01_first_run timed out after 30 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_user_journey_uj02_profile_switching() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = user_journeys::test_uj02_profile_switching().await?;

        if !result.passed {
            anyhow::bail!("UJ-02 test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_user_journey_uj02_profile_switching exceeded 30 second limit");
            eprintln!(
                "Diagnostic: User journey UJ-02 (profile switching) did not complete in time."
            );
            eprintln!("This may indicate:");
            eprintln!("  - Profile loading is blocked");
            eprintln!("  - Profile application is hanging");
            eprintln!("  - IPC communication timeout");
            anyhow::bail!("test_user_journey_uj02_profile_switching timed out after 30 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "Fault response timing requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "Fault response timing requires RT scheduling on macOS"
)]
async fn test_user_journey_uj03_fault_recovery() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive UJ-03 fault recovery gate under shared CI");
        return Ok(());
    }

    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = user_journeys::test_uj03_fault_recovery().await?;

        if !result.passed {
            anyhow::bail!("UJ-03 test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_user_journey_uj03_fault_recovery exceeded 30 second limit");
            eprintln!("Diagnostic: User journey UJ-03 (fault recovery) did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Fault injection is blocked");
            eprintln!("  - Recovery mechanism is hanging");
            eprintln!("  - Safety system deadlock");
            anyhow::bail!("test_user_journey_uj03_fault_recovery timed out after 30 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_user_journey_uj04_debug_workflow() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = user_journeys::test_uj04_debug_workflow_ci().await?;

        if !result.passed {
            anyhow::bail!("UJ-04 test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_user_journey_uj04_debug_workflow exceeded 30 second limit");
            eprintln!("Diagnostic: User journey UJ-04 (debug workflow) did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Diagnostic collection is blocked");
            eprintln!("  - Black box recording is hanging");
            eprintln!("  - Support bundle generation timeout");
            anyhow::bail!("test_user_journey_uj04_debug_workflow timed out after 30 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "FFB jitter gate requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "FFB jitter gate requires RT scheduling on macOS"
)]
async fn test_performance_gates_ffb_jitter() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive FFB jitter gate under shared CI");
        return Ok(());
    }

    init_test_environment()?;
    let timeout_limit = jitter_test_timeout();

    // Keep wrapper timeout aligned with the gate measurement duration.
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = gates::test_ffb_jitter_gate().await?;
        let jitter_limit = jitter_p99_limit_ms();

        if !result.passed {
            anyhow::bail!("FFB jitter gate failed: {:?}", result.errors);
        }

        if result.metrics.jitter_p99_ms > jitter_limit {
            anyhow::bail!(
                "FFB jitter p99 {}ms exceeds limit {}ms",
                result.metrics.jitter_p99_ms,
                jitter_limit
            );
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(timeout_limit, test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!(
                "TIMEOUT: test_performance_gates_ffb_jitter exceeded {:?} limit",
                timeout_limit
            );
            eprintln!("Diagnostic: FFB jitter gate test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - RT loop is blocked or deadlocked");
            eprintln!("  - Metrics collection is hanging");
            eprintln!("  - System under heavy load");
            anyhow::bail!(
                "test_performance_gates_ffb_jitter timed out after {:?}",
                timeout_limit
            )
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "HID latency gate requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "HID latency gate requires RT scheduling on macOS"
)]
async fn test_performance_gates_hid_latency() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive HID latency gate under shared CI");
        return Ok(());
    }

    init_test_environment()?;
    let timeout_limit = hid_latency_test_timeout();

    // Keep timeout aligned with the gate duration plus CI-safe margin.
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = gates::test_hid_latency_gate().await?;
        let hid_latency_limit = gates::hid_latency_p99_limit_us();

        if !result.passed {
            anyhow::bail!("HID latency gate failed: {:?}", result.errors);
        }

        if result.metrics.hid_latency_p99_us > hid_latency_limit {
            anyhow::bail!(
                "HID latency p99 {}us exceeds limit {}us",
                result.metrics.hid_latency_p99_us,
                hid_latency_limit
            );
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(timeout_limit, test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!(
                "TIMEOUT: test_performance_gates_hid_latency exceeded {:?} limit",
                timeout_limit
            );
            eprintln!("Diagnostic: HID latency gate test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - HID communication is blocked");
            eprintln!("  - Device I/O is hanging");
            eprintln!("  - USB subsystem issues");
            anyhow::bail!(
                "test_performance_gates_hid_latency timed out after {:?}",
                timeout_limit
            )
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "Zero missed ticks gate requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "Zero missed ticks gate requires RT scheduling on macOS"
)]
async fn test_performance_gates_zero_missed_ticks() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive zero missed ticks gate under shared CI");
        return Ok(());
    }

    init_test_environment()?;
    let timeout_limit = zero_missed_ticks_test_timeout();

    // Keep timeout aligned with the gate duration plus CI-safe margin.
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = gates::test_zero_missed_ticks_gate().await?;
        let allowed_missed_ticks = gates::allowed_zero_missed_ticks(result.metrics.total_ticks);

        if !result.passed {
            anyhow::bail!("Zero missed ticks gate failed: {:?}", result.errors);
        }

        if result.metrics.missed_ticks > allowed_missed_ticks {
            anyhow::bail!(
                "Missed {} ticks, expected <= {}",
                result.metrics.missed_ticks,
                allowed_missed_ticks
            );
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(timeout_limit, test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!(
                "TIMEOUT: test_performance_gates_zero_missed_ticks exceeded {:?} limit",
                timeout_limit
            );
            eprintln!("Diagnostic: Zero missed ticks gate test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - RT loop is blocked or deadlocked");
            eprintln!("  - Tick counting mechanism is hanging");
            eprintln!("  - System scheduling issues");
            anyhow::bail!(
                "test_performance_gates_zero_missed_ticks timed out after {:?}",
                timeout_limit
            )
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_hotplug_stress_basic() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = stress::test_hotplug_stress_ci().await?;

        if !result.passed {
            anyhow::bail!("Hot-plug stress test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            // Diagnostic output on timeout (Requirement 2.5)
            eprintln!("TIMEOUT: test_hotplug_stress_basic exceeded 30 second limit");
            eprintln!("Diagnostic: Hot-plug stress test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Device enumeration is blocked or slow");
            eprintln!("  - Event handling is deadlocked");
            eprintln!("  - Resource contention in the test harness");
            anyhow::bail!("test_hotplug_stress_basic timed out after 30 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "Fault injection stress test requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "Fault injection stress test requires RT scheduling on macOS"
)]
async fn test_fault_injection_stress() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive fault injection stress gate under shared CI");
        return Ok(());
    }

    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 60 seconds
    // (longer timeout for stress test)
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = stress::test_fault_injection_stress().await?;

        if !result.passed {
            anyhow::bail!("Fault injection stress test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(60), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_fault_injection_stress exceeded 60 second limit");
            eprintln!("Diagnostic: Fault injection stress test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Fault injection mechanism is blocked");
            eprintln!("  - Recovery is not completing");
            eprintln!("  - Safety system deadlock");
            anyhow::bail!("test_fault_injection_stress timed out after 60 seconds")
        }
    }
}

#[tokio::test]
#[ignore = "Long-running soak test, run explicitly with --include-ignored"]
async fn test_ci_soak_test() -> Result<()> {
    init_test_environment()?;

    let result = soak::run_ci_soak_test().await?;

    if !result.passed {
        panic!("CI soak test failed: {:?}", result.errors);
    }

    assert_eq!(result.metrics.missed_ticks, 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "Acceptance tests require RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "Acceptance tests require RT scheduling on macOS"
)]
async fn test_acceptance_tests_subset() -> Result<()> {
    init_test_environment()?;
    let timeout_limit = acceptance_subset_timeout();

    // Acceptance subset includes multiple flows; use CI-aware timeout budget.
    // Requirements: 2.1, 2.5
    let test_future = async {
        // Run CI-focused acceptance tests only.
        let results = acceptance::run_ci_acceptance_tests().await?;

        let failed_tests: Vec<_> = results
            .iter()
            .filter(|(_, result)| !result.passed)
            .collect();

        if !failed_tests.is_empty() {
            anyhow::bail!("Acceptance tests failed: {:?}", failed_tests);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(timeout_limit, test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!(
                "TIMEOUT: test_acceptance_tests_subset exceeded {:?} limit",
                timeout_limit
            );
            eprintln!("Diagnostic: Acceptance test subset did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - One or more acceptance tests are hanging");
            eprintln!("  - Service initialization is blocked");
            eprintln!("  - Resource contention in test harness");
            anyhow::bail!(
                "test_acceptance_tests_subset timed out after {:?}",
                timeout_limit
            )
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(
    target_os = "windows",
    ignore = "Performance benchmark suite requires RT scheduling on Windows"
)]
#[cfg_attr(
    target_os = "macos",
    ignore = "Performance benchmark suite requires RT scheduling on macOS"
)]
async fn test_performance_benchmark_suite() -> Result<()> {
    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping RT-scheduling-sensitive performance benchmark suite under shared CI");
        return Ok(());
    }

    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 60 seconds
    // (longer timeout for benchmark suite)
    // Requirements: 2.1, 2.5
    let test_future = async {
        let results = performance::run_performance_benchmark_suite().await?;

        let failed_benchmarks: Vec<_> = results.iter().filter(|result| !result.passed).collect();

        if !failed_benchmarks.is_empty() {
            anyhow::bail!("Performance benchmarks failed: {:?}", failed_benchmarks);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(60), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_performance_benchmark_suite exceeded 60 second limit");
            eprintln!("Diagnostic: Performance benchmark suite did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - One or more benchmarks are hanging");
            eprintln!("  - RT loop is blocked");
            eprintln!("  - System under heavy load");
            anyhow::bail!("test_performance_benchmark_suite timed out after 60 seconds")
        }
    }
}

// Test fixtures validation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_device_fixtures() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 10 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let fixtures = fixtures::get_device_fixtures();

        if fixtures.is_empty() {
            anyhow::bail!("No device fixtures found");
        }

        for fixture in fixtures {
            // Validate fixture data
            if fixture.name.is_empty() {
                anyhow::bail!("Fixture has empty name");
            }
            if fixture.capabilities.max_torque.value() <= 0.0 {
                anyhow::bail!("Fixture {} has invalid max_torque", fixture.name);
            }
            if fixture.capabilities.encoder_cpr == 0 {
                anyhow::bail!("Fixture {} has invalid encoder_cpr", fixture.name);
            }
            if fixture.telemetry_data.samples.is_empty() {
                anyhow::bail!("Fixture {} has no telemetry samples", fixture.name);
            }
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(10), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_device_fixtures exceeded 10 second limit");
            eprintln!("Diagnostic: Device fixtures test did not complete in time.");
            anyhow::bail!("test_device_fixtures timed out after 10 seconds")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_profile_fixtures() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 10 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let fixtures = fixtures::get_profile_fixtures();

        if fixtures.is_empty() {
            anyhow::bail!("No profile fixtures found");
        }

        for fixture in fixtures {
            // Validate fixture structure
            if fixture.name.is_empty() {
                anyhow::bail!("Fixture has empty name");
            }
            if fixture.json_content.is_empty() {
                anyhow::bail!("Fixture {} has empty json_content", fixture.name);
            }

            // Try to parse JSON
            let parse_result = serde_json::from_str::<serde_json::Value>(&fixture.json_content);

            if fixture.expected_valid && parse_result.is_err() {
                anyhow::bail!(
                    "Valid fixture {} should parse: {:?}",
                    fixture.name,
                    parse_result.err()
                );
            }
            // Note: Invalid fixtures might still parse as JSON but fail schema validation
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(10), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_profile_fixtures exceeded 10 second limit");
            eprintln!("Diagnostic: Profile fixtures test did not complete in time.");
            anyhow::bail!("test_profile_fixtures timed out after 10 seconds")
        }
    }
}

// Integration test configuration validation
#[test]
fn test_performance_thresholds() {
    // Validate that our performance thresholds are reasonable (compile-time checks)
    const _: () = {
        assert!(MAX_JITTER_P99_MS > 0.0);
        assert!(MAX_JITTER_P99_MS <= 1.0); // Should be sub-millisecond
        assert!(MAX_HID_LATENCY_P99_US > 0.0);
        assert!(MAX_HID_LATENCY_P99_US <= 1000.0); // Should be sub-millisecond
    };

    assert_eq!(FFB_FREQUENCY_HZ, 1000); // 1kHz requirement
}

#[test]
fn test_soak_test_duration() {
    // Validate soak test duration is 48 hours
    assert_eq!(SOAK_TEST_DURATION, Duration::from_secs(48 * 60 * 60));
}

// Helper function to run a quick smoke test
async fn run_smoke_test() -> Result<TestResult> {
    let config = TestConfig {
        duration: Duration::from_secs(5),
        virtual_device: true,
        enable_tracing: false,
        enable_metrics: false,
        ..Default::default()
    };

    let mut harness = common::TestHarness::new(config).await?;
    let start_time = std::time::Instant::now();

    harness.start_service().await?;

    // Basic functionality check
    tokio::time::sleep(Duration::from_secs(3)).await;

    let metrics = harness.collect_metrics().await;
    harness.shutdown().await?;

    Ok(TestResult {
        passed: true,
        duration: start_time.elapsed(),
        metrics,
        errors: vec![],
        requirement_coverage: vec!["SMOKE".to_string()],
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_smoke_test() -> Result<()> {
    init_test_environment()?;

    // Wrap test body with timeout to ensure test completes within 30 seconds
    // Requirements: 2.1, 2.5
    let test_future = async {
        let result = run_smoke_test().await?;

        if !result.passed {
            anyhow::bail!("Smoke test failed: {:?}", result.errors);
        }

        Ok::<(), anyhow::Error>(())
    };

    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            eprintln!("TIMEOUT: test_smoke_test exceeded 30 second limit");
            eprintln!("Diagnostic: Smoke test did not complete in time.");
            eprintln!("This may indicate:");
            eprintln!("  - Service startup is blocked");
            eprintln!("  - Basic functionality check is hanging");
            eprintln!("  - Service shutdown is not completing");
            anyhow::bail!("test_smoke_test timed out after 30 seconds")
        }
    }
}
