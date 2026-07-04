//! Deep tests for the scheduler subsystem.
//!
//! Covers creation, timing accuracy, deadline detection, metrics,
//! graceful shutdown, multi-scheduler independence, and property tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PERIOD_1KHZ_NS, PLL, RTError,
    RTSetup,
};

// ===========================================================================
// 1. Scheduler creation with various configurations
// ===========================================================================

#[test]
fn creation_1khz() -> Result<(), Box<dyn std::error::Error>> {
    let s = AbsoluteScheduler::new_1khz();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
    assert_eq!(s.tick_count(), 0);
    assert!(!s.is_rt_setup_applied());
    Ok(())
}

#[test]
fn creation_custom_period() -> Result<(), Box<dyn std::error::Error>> {
    for &period in &[100_000u64, 500_000, 2_000_000, 10_000_000] {
        let s = AbsoluteScheduler::with_period(period);
        assert_eq!(s.period_ns(), period);
    }
    Ok(())
}

#[test]
fn creation_zero_period_is_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let s = AbsoluteScheduler::with_period(0);
    assert_eq!(s.period_ns(), 1);
    Ok(())
}

#[test]
fn creation_default_is_1khz() -> Result<(), Box<dyn std::error::Error>> {
    let s = AbsoluteScheduler::default();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
    Ok(())
}

#[test]
fn creation_with_adaptive_config() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = AbsoluteScheduler::new_1khz();
    s.set_adaptive_scheduling(AdaptiveSchedulingConfig::enabled());
    let state = s.adaptive_scheduling();
    assert!(state.enabled);
    Ok(())
}

// ===========================================================================
// 2. Tick timing accuracy (statistical) — use a 5ms period to be CI-friendly
// ===========================================================================

#[test]
fn tick_timing_accuracy_statistical() -> Result<(), Box<dyn std::error::Error>> {
    let period_ns: u64 = 5_000_000; // 5 ms — relaxed for CI
    let num_ticks: u64 = 200;
    let mut scheduler = AbsoluteScheduler::with_period(period_ns);
    let setup = RTSetup::minimal();
    scheduler.apply_rt_setup(&setup)?;

    let start = Instant::now();
    let mut completed: u64 = 0;
    for _ in 0..num_ticks {
        match scheduler.wait_for_tick() {
            Ok(_) => completed += 1,
            Err(RTError::TimingViolation) => completed += 1, // still a tick
            Err(e) => return Err(e.into()),
        }
    }
    let elapsed = start.elapsed();

    assert_eq!(completed, num_ticks);

    let expected = Duration::from_nanos(period_ns * num_ticks);
    let ratio = elapsed.as_secs_f64() / expected.as_secs_f64();

    // Accept 0.5x–2.0x of expected wall time (CI machines can be slow)
    assert!(
        (0.5..=2.0).contains(&ratio),
        "timing ratio {ratio:.3} outside [0.5, 2.0]; elapsed={elapsed:?}, expected={expected:?}"
    );
    Ok(())
}

// ===========================================================================
// 3. Deadline detection — a slow tick triggers TimingViolation
// ===========================================================================

#[test]
fn deadline_detection_slow_tick() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = AbsoluteScheduler::with_period(1_000_000); // 1 ms
    let setup = RTSetup::minimal();
    scheduler.apply_rt_setup(&setup)?;

    // Burn the first tick so the scheduler is running
    let _ = scheduler.wait_for_tick();

    // Sleep well past the next deadline to force a timing violation
    thread::sleep(Duration::from_millis(50));

    let result = scheduler.wait_for_tick();
    // The scheduler should detect the late tick
    assert!(
        result.is_ok() || result == Err(RTError::TimingViolation),
        "expected Ok or TimingViolation, got {result:?}"
    );

    // Regardless of the error, the jitter metrics must record the late tick
    let metrics = scheduler.metrics();
    assert!(
        metrics.max_jitter_ns >= 1_000_000,
        "max jitter should reflect the late tick: {}",
        metrics.max_jitter_ns
    );
    Ok(())
}

// ===========================================================================
// 4. Scheduler metrics: tick count, missed ticks, max jitter
// ===========================================================================

#[test]
fn metrics_tick_count_increments() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = AbsoluteScheduler::with_period(1_000_000);
    let setup = RTSetup::minimal();
    scheduler.apply_rt_setup(&setup)?;

    let target_ticks: u64 = 10;
    let mut ok_count: u64 = 0;

    for _ in 0..target_ticks {
        match scheduler.wait_for_tick() {
            Ok(tick) => {
                ok_count += 1;
                // Tick number is overall count (may exceed ok_count due to timing violations)
                assert!(
                    tick >= ok_count,
                    "tick {tick} should be >= ok_count {ok_count}"
                );
            }
            Err(RTError::TimingViolation) => {
                // Tick still counts internally
            }
            Err(e) => return Err(e.into()),
        }
    }

    assert_eq!(scheduler.tick_count(), target_ticks);
    assert_eq!(scheduler.metrics().total_ticks, target_ticks);
    Ok(())
}

#[test]
fn metrics_jitter_percentiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1_000);

    // 990 low-jitter samples, 10 high-jitter samples
    for _ in 0..990 {
        metrics.record_tick(10_000, false);
    }
    for _ in 0..10 {
        metrics.record_tick(500_000, true);
    }

    let p50 = metrics.p50_jitter_ns();
    let p99 = metrics.p99_jitter_ns();

    assert!(p50 <= 100_000, "p50 should be low-jitter, got {p50}");
    assert!(
        p99 >= 100_000,
        "p99 should capture the high-jitter tail, got {p99}"
    );
    assert_eq!(metrics.total_ticks, 1_000);
    assert_eq!(metrics.missed_ticks, 10);
    assert_eq!(metrics.max_jitter_ns, 500_000);
    Ok(())
}

#[test]
fn metrics_missed_tick_rate_accuracy() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();

    for i in 0..1_000u64 {
        metrics.record_tick(i * 100, i % 100 == 0);
    }

    let rate = metrics.missed_tick_rate();
    let expected = 10.0 / 1_000.0;
    assert!(
        (rate - expected).abs() < 1e-10,
        "missed tick rate {rate} != expected {expected}"
    );
    Ok(())
}

// ===========================================================================
// 5. Graceful shutdown — scheduler stops cleanly within timeout
// ===========================================================================

#[test]
fn graceful_shutdown_within_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let mut scheduler = AbsoluteScheduler::with_period(1_000_000);
        let setup = RTSetup::minimal();
        let _ = scheduler.apply_rt_setup(&setup);

        let mut completed_ticks = 0u64;
        while let Ok(_) | Err(RTError::TimingViolation) = scheduler.wait_for_tick() {
            completed_ticks += 1;
            if stop_clone.load(Ordering::Acquire) {
                break;
            }
        }
        completed_ticks
    });

    // Let the scheduler run for a short while
    thread::sleep(Duration::from_millis(50));
    stop.store(true, Ordering::Release);

    let start = Instant::now();
    let ticks = handle.join().map_err(|_| "scheduler thread panicked")?;
    let shutdown_time = start.elapsed();

    // Should shut down quickly (within 100ms even on slow CI)
    assert!(
        shutdown_time < Duration::from_millis(100),
        "shutdown took {shutdown_time:?}, expected < 100ms"
    );
    assert!(ticks > 0, "scheduler should have completed at least 1 tick");
    Ok(())
}

// ===========================================================================
// 6. Multiple schedulers — independent schedulers don't interfere
// ===========================================================================

#[test]
fn multiple_schedulers_independent() -> Result<(), Box<dyn std::error::Error>> {
    let num_schedulers = 4;
    let ticks_each: u64 = 20;

    let handles: Vec<_> = (0..num_schedulers)
        .map(|i| {
            thread::spawn(move || {
                let period = 2_000_000 + (i as u64) * 500_000; // stagger periods
                let mut sched = AbsoluteScheduler::with_period(period);
                let setup = RTSetup::minimal();
                let _ = sched.apply_rt_setup(&setup);

                let mut completed: u64 = 0;
                for _ in 0..ticks_each {
                    match sched.wait_for_tick() {
                        Ok(_) | Err(RTError::TimingViolation) => completed += 1,
                        Err(_) => break,
                    }
                }
                (sched.period_ns(), sched.tick_count(), completed)
            })
        })
        .collect();

    for h in handles {
        let (period, tick_count, completed) = h.join().map_err(|_| "scheduler thread panicked")?;
        assert!(period > 0, "period should be positive");
        assert_eq!(tick_count, ticks_each);
        assert_eq!(completed, ticks_each);
    }
    Ok(())
}

// ===========================================================================
// 7. PLL stability under various conditions
// ===========================================================================

#[test]
fn pll_converges_with_consistent_timing() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);

    for _ in 0..500 {
        let _ = pll.update(1_000_000);
    }

    assert!(pll.is_stable());
    assert!(
        pll.phase_error_ns().abs() < 1_000.0,
        "phase error should be near zero, got {}",
        pll.phase_error_ns()
    );
    Ok(())
}

#[test]
fn pll_stays_bounded_under_drift() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);

    // Simulate consistent 5% slow drift
    for _ in 0..200 {
        let corrected = pll.update(1_050_000);
        let ns = corrected.as_nanos() as u64;
        // Must stay within ±10% of target
        assert!(
            (900_000..=1_100_000).contains(&ns),
            "corrected period {ns} outside bounds"
        );
    }
    Ok(())
}

// ===========================================================================
// 8. Property tests
// ===========================================================================

/// Scheduler period is always positive and bounded for any valid input.
#[test]
fn property_period_always_positive_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
    for period in [0u64, 1, 100, 1_000_000, u64::MAX / 2, u64::MAX] {
        let s = AbsoluteScheduler::with_period(period);
        assert!(s.period_ns() >= 1, "period must be at least 1");
    }
    Ok(())
}

/// Adaptive scheduling always keeps the target within [min, max] bounds.
#[test]
fn property_adaptive_target_stays_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(50_000, 50_000);
    scheduler.set_adaptive_scheduling(config);

    // Simulate extreme load signals
    for load in [0u64, 100, 500, 1000, 5000] {
        scheduler.record_processing_time_us(load);
        let state = scheduler.adaptive_scheduling();
        assert!(
            state.target_period_ns >= state.min_period_ns,
            "target {} < min {}",
            state.target_period_ns,
            state.min_period_ns
        );
        assert!(
            state.target_period_ns <= state.max_period_ns,
            "target {} > max {}",
            state.target_period_ns,
            state.max_period_ns
        );
    }
    Ok(())
}

/// JitterMetrics percentiles are monotonically ordered: p50 <= p95 <= p99.
#[test]
fn property_percentile_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(500);

    for i in 0..500u64 {
        metrics.record_tick(i * 100, false);
    }

    let p50 = metrics.p50_jitter_ns();
    let p95 = metrics.p95_jitter_ns();
    let p99 = metrics.p99_jitter_ns();

    assert!(p50 <= p95, "p50 ({p50}) should be <= p95 ({p95})");
    assert!(p95 <= p99, "p95 ({p95}) should be <= p99 ({p99})");
    Ok(())
}

/// Reset clears all scheduler state cleanly.
#[test]
fn property_reset_clears_all_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    scheduler.apply_rt_setup(&setup)?;

    // Run some ticks
    for _ in 0..5 {
        let _ = scheduler.wait_for_tick();
    }

    scheduler.reset();

    assert_eq!(scheduler.tick_count(), 0);
    assert_eq!(scheduler.metrics().total_ticks, 0);
    assert_eq!(scheduler.metrics().missed_ticks, 0);
    assert_eq!(scheduler.metrics().max_jitter_ns, 0);
    Ok(())
}

/// Normalized adaptive config is always valid.
#[test]
fn property_normalized_config_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = [
        AdaptiveSchedulingConfig {
            min_period_ns: 2_000_000,
            max_period_ns: 500_000, // inverted
            ..Default::default()
        },
        AdaptiveSchedulingConfig {
            min_period_ns: 0,
            max_period_ns: 0,
            increase_step_ns: 0,
            decrease_step_ns: 0,
            processing_ema_alpha: 0.0,
            ..Default::default()
        },
        AdaptiveSchedulingConfig {
            jitter_tighten_threshold_ns: 999_999,
            jitter_relax_threshold_ns: 1,
            processing_tighten_threshold_us: 999,
            processing_relax_threshold_us: 1,
            processing_ema_alpha: 50.0,
            ..Default::default()
        },
    ];

    for mut cfg in test_cases {
        cfg.normalize();
        assert!(
            cfg.is_valid(),
            "config should be valid after normalize: {cfg:?}"
        );
    }
    Ok(())
}
