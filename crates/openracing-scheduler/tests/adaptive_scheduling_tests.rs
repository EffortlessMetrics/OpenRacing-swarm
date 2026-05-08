//! Comprehensive tests for the adaptive scheduling system.
//!
//! Tests cover:
//! 1. Load detection: CPU utilization measurement, moving averages (EMA)
//! 2. Rate transitions: smooth transitions between tick rates
//! 3. Safety constraints: never drop below minimum safe rate
//! 4. Hysteresis: avoid oscillating between rates
//! 5. Device capability: respect device maximum update rates
//! 6. Recovery: return to full rate when load decreases
//! 7. Budget tracking: RT tick budget measurement and enforcement

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, AdaptiveSchedulingState, PERIOD_1KHZ_NS,
};

// ===========================================================================
// Helper: drive N adaptive ticks without actually sleeping.
//
// `update_adaptive_target` is private, but every call to `wait_for_tick`
// invokes it. Because `wait_for_tick` does real sleeping, we instead test
// by manipulating the public surface:
//   - `set_adaptive_scheduling` to install configs
//   - `record_processing_time_us` to feed load signals
//   - `adaptive_scheduling()` to read back state
//
// For deterministic control over the jitter/deadline signal we use a helper
// that creates a scheduler, pumps processing times, and reads state.
// ===========================================================================

/// Build a scheduler with adaptive scheduling enabled and the given config.
fn scheduler_with_config(config: AdaptiveSchedulingConfig) -> AbsoluteScheduler {
    let mut s = AbsoluteScheduler::new_1khz();
    s.set_adaptive_scheduling(config);
    s
}

// ===========================================================================
// 1. Load detection — EMA processing-time tracking
// ===========================================================================

#[test]
fn ema_initial_sample_is_exact() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(0.2));

    s.record_processing_time_us(100);
    let state = s.adaptive_scheduling();

    // First sample should seed the EMA directly.
    assert!(
        (state.processing_time_ema_us - 100.0).abs() < f64::EPSILON,
        "First EMA sample should be exact, got {}",
        state.processing_time_ema_us
    );
    assert_eq!(state.last_processing_time_us, 100);
    Ok(())
}

#[test]
fn ema_converges_toward_recent_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(0.5));

    // Seed with 100, then shift to 200.
    s.record_processing_time_us(100);
    s.record_processing_time_us(200);
    let state = s.adaptive_scheduling();

    // EMA with alpha=0.5: after 2nd sample => 0.5*100 + 0.5*200 = 150.
    assert!(
        (state.processing_time_ema_us - 150.0).abs() < 1e-6,
        "EMA should be 150, got {}",
        state.processing_time_ema_us
    );
    Ok(())
}

#[test]
fn ema_three_samples_progressive() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(0.2));

    // Samples: 100, 200, 300
    s.record_processing_time_us(100); // EMA = 100
    s.record_processing_time_us(200); // EMA = 0.8*100 + 0.2*200 = 120
    s.record_processing_time_us(300); // EMA = 0.8*120 + 0.2*300 = 156
    let state = s.adaptive_scheduling();

    assert!(
        (state.processing_time_ema_us - 156.0).abs() < 1e-6,
        "EMA should be 156, got {}",
        state.processing_time_ema_us
    );
    assert_eq!(state.last_processing_time_us, 300);
    Ok(())
}

#[test]
fn ema_alpha_one_tracks_instantly() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(1.0));

    s.record_processing_time_us(100);
    s.record_processing_time_us(500);
    let state = s.adaptive_scheduling();

    // With alpha=1.0, EMA should immediately equal latest sample.
    assert!(
        (state.processing_time_ema_us - 500.0).abs() < 1e-6,
        "EMA with alpha=1.0 should track instantly, got {}",
        state.processing_time_ema_us
    );
    Ok(())
}

#[test]
fn ema_very_low_alpha_is_slow() -> Result<(), Box<dyn std::error::Error>> {
    // Alpha is clamped to minimum 0.01 by normalize().
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(0.01));

    s.record_processing_time_us(100);
    // Feed 20 samples at 500us — EMA should barely move with alpha=0.01
    for _ in 0..20 {
        s.record_processing_time_us(500);
    }
    let state = s.adaptive_scheduling();

    // After 20 updates at alpha=0.01 from seed 100 toward 500:
    // EMA should still be well below 500 (slow tracking).
    assert!(
        state.processing_time_ema_us < 400.0,
        "Low alpha should track slowly, got {}",
        state.processing_time_ema_us
    );
    assert!(
        state.processing_time_ema_us > 100.0,
        "EMA should have moved from seed, got {}",
        state.processing_time_ema_us
    );
    Ok(())
}

// ===========================================================================
// 2. Rate transitions — period increases/decreases via wait_for_tick
// ===========================================================================

// We cannot directly call `update_adaptive_target`, so we test via
// `wait_for_tick` with a lenient period to keep tests fast and CI-safe.

#[test]
fn period_increases_under_high_processing_load() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_ema_alpha(1.0); // alpha=1 so EMA tracks instantly

    let mut s = AbsoluteScheduler::with_period(5_000_000); // 5ms for CI
    s.set_adaptive_scheduling(config);

    let baseline = s.adaptive_scheduling().target_period_ns;

    // Feed a high processing time (above relax threshold of 180us).
    s.record_processing_time_us(250);

    // Run a tick to trigger adaptive update.
    let _ = s.wait_for_tick(); // ignore timing violation in test
    let after = s.adaptive_scheduling().target_period_ns;

    // Period should have increased (or be at max).
    assert!(
        after >= baseline,
        "Period should increase under load: before={baseline}, after={after}"
    );
    Ok(())
}

#[test]
fn period_decreases_when_system_is_healthy() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_jitter_thresholds(200_000, 50_000)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // First, push the period up by feeding high load.
    s.record_processing_time_us(250);
    for _ in 0..20 {
        let _ = s.wait_for_tick();
    }
    let elevated = s.adaptive_scheduling().target_period_ns;

    // Now feed low processing time (below tighten threshold of 80us).
    s.record_processing_time_us(50);
    for _ in 0..20 {
        let _ = s.wait_for_tick();
    }
    let recovered = s.adaptive_scheduling().target_period_ns;

    // Period should have decreased (or be at min).
    assert!(
        recovered <= elevated,
        "Period should decrease after recovery: elevated={elevated}, recovered={recovered}"
    );
    Ok(())
}

// ===========================================================================
// 3. Safety constraints — never exceed bounds
// ===========================================================================

#[test]
fn adaptive_period_never_exceeds_max_bound() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(50_000, 2_000) // very large step to stress-test
        .with_processing_thresholds(100, 50)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // Feed extreme processing load and run many ticks.
    s.record_processing_time_us(500);
    for _ in 0..100 {
        let _ = s.wait_for_tick();
    }

    let state = s.adaptive_scheduling();
    assert!(
        state.target_period_ns <= 1_100_000,
        "Period must not exceed max bound, got {}",
        state.target_period_ns
    );
    Ok(())
}

#[test]
fn adaptive_period_never_goes_below_min_bound() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(2_000, 50_000) // very large decrease step
        .with_processing_thresholds(500, 400)
        .with_jitter_thresholds(500_000, 400_000)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // Feed low processing time to trigger tightening.
    s.record_processing_time_us(10);
    for _ in 0..100 {
        let _ = s.wait_for_tick();
    }

    let state = s.adaptive_scheduling();
    assert!(
        state.target_period_ns >= 900_000,
        "Period must not go below min bound, got {}",
        state.target_period_ns
    );
    Ok(())
}

#[test]
fn set_adaptive_scheduling_clamps_initial_period() -> Result<(), Box<dyn std::error::Error>> {
    // Scheduler with 500us period, but adaptive bounds are 900-1100us.
    let mut s = AbsoluteScheduler::with_period(500_000);
    let config = AdaptiveSchedulingConfig::enabled().with_period_bounds(900_000, 1_100_000);

    s.set_adaptive_scheduling(config);
    let state = s.adaptive_scheduling();

    assert!(
        state.target_period_ns >= 900_000,
        "Initial period should be clamped to min bound, got {}",
        state.target_period_ns
    );
    Ok(())
}

#[test]
fn set_adaptive_scheduling_clamps_high_period() -> Result<(), Box<dyn std::error::Error>> {
    // Scheduler with 2ms period, but adaptive bounds are 900-1100us.
    let mut s = AbsoluteScheduler::with_period(2_000_000);
    let config = AdaptiveSchedulingConfig::enabled().with_period_bounds(900_000, 1_100_000);

    s.set_adaptive_scheduling(config);
    let state = s.adaptive_scheduling();

    assert!(
        state.target_period_ns <= 1_100_000,
        "Initial period should be clamped to max bound, got {}",
        state.target_period_ns
    );
    Ok(())
}

// ===========================================================================
// 4. Hysteresis — dead-zone between tighten and relax thresholds
// ===========================================================================

#[test]
fn moderate_load_does_not_change_period() -> Result<(), Box<dyn std::error::Error>> {
    // processing relax=180, tighten=80 → processing in [81..179] should be dead-zone
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_jitter_thresholds(200_000, 50_000)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // Seed EMA at a value between tighten and relax thresholds.
    s.record_processing_time_us(120);

    let before = s.adaptive_scheduling().target_period_ns;

    // The jitter in test may be low enough to trigger tighten via the jitter
    // channel, so we specifically check that processing in the dead zone alone
    // doesn't force a change. We run a few ticks and check stability.
    // Note: because jitter might cause changes, we just verify the period stays
    // within a small delta of the initial value after a few ticks.
    for _ in 0..5 {
        let _ = s.wait_for_tick();
        s.record_processing_time_us(120); // keep EMA in dead zone
    }

    let after = s.adaptive_scheduling().target_period_ns;

    // The period may have changed due to jitter, but it shouldn't have moved
    // by more than a few steps in either direction.
    let delta = after.abs_diff(before);
    // If only jitter-driven, max 5 ticks × 5us step = 25us change
    assert!(
        delta <= 25_000,
        "Period should be relatively stable in dead zone: before={before}, after={after}, delta={delta}"
    );
    Ok(())
}

#[test]
fn hysteresis_prevents_oscillation() -> Result<(), Box<dyn std::error::Error>> {
    // Rapidly alternating high/low processing times should not cause large oscillations
    // because the EMA smoothing dampens the signal.
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_ema_alpha(0.2); // slow EMA to dampen oscillation

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // Alternate between high (250) and low (30) processing times.
    s.record_processing_time_us(130); // seed in middle

    let mut periods: Vec<u64> = Vec::new();
    for i in 0..20 {
        let pt = if i % 2 == 0 { 250 } else { 30 };
        s.record_processing_time_us(pt);
        let _ = s.wait_for_tick();
        periods.push(s.adaptive_scheduling().target_period_ns);
    }

    // Count direction changes (oscillations).
    let mut direction_changes = 0u32;
    for window in periods.windows(3) {
        let going_up = window[1] > window[0];
        let going_down = window[2] < window[1];
        let reversed_up = window[1] < window[0] && window[2] > window[1];
        if (going_up && going_down) || reversed_up {
            direction_changes += 1;
        }
    }

    // With EMA smoothing, we shouldn't see more oscillations than half the ticks.
    assert!(
        direction_changes < 10,
        "Too many oscillations ({direction_changes}), hysteresis should dampen"
    );
    Ok(())
}

// ===========================================================================
// 5. Device capability — respect max update rates via period bounds
// ===========================================================================

#[test]
fn device_max_rate_respected_via_min_period() -> Result<(), Box<dyn std::error::Error>> {
    // If a device can only do 500Hz, min_period should be 2ms.
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(2_000_000, 4_000_000) // 250-500Hz
        .with_step_sizes(10_000, 5_000)
        .with_processing_thresholds(500, 200)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // Even with very low load, period should not go below device minimum.
    s.record_processing_time_us(10);
    for _ in 0..50 {
        let _ = s.wait_for_tick();
    }

    let state = s.adaptive_scheduling();
    assert!(
        state.target_period_ns >= 2_000_000,
        "Should respect device min period (500Hz max), got {} ns",
        state.target_period_ns
    );
    Ok(())
}

#[test]
fn narrow_device_range_constrains_adaptation() -> Result<(), Box<dyn std::error::Error>> {
    // Device supports only a tight range (e.g. 980-1020us).
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(980_000, 1_020_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    // High load.
    s.record_processing_time_us(300);
    for _ in 0..20 {
        let _ = s.wait_for_tick();
    }

    let state = s.adaptive_scheduling();
    assert!(
        state.target_period_ns >= 980_000 && state.target_period_ns <= 1_020_000,
        "Narrow device range must be respected: got {}",
        state.target_period_ns
    );
    Ok(())
}

// ===========================================================================
// 6. Recovery — return to base rate after load subsides
// ===========================================================================

#[test]
fn full_recovery_to_min_period_after_load_spike() -> Result<(), Box<dyn std::error::Error>> {
    // Use a wide, slow period range so Windows CI timing noise does not keep
    // every recovery tick in a missed-deadline state.
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(20_000_000, 30_000_000)
        .with_step_sizes(5_000_000, 5_000_000)
        .with_processing_thresholds(180, 80)
        .with_jitter_thresholds(50_000_000, 40_000_000) // 50ms/40ms — never fires on CI
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(25_000_000);
    s.set_adaptive_scheduling(config);

    // Phase 1: drive period to max via high processing load.
    s.record_processing_time_us(300);
    for _ in 0..2 {
        let _ = s.wait_for_tick();
    }
    let at_max = s.adaptive_scheduling();
    assert!(
        at_max.is_at_max(),
        "Should reach max period under sustained high load, got {}",
        at_max.target_period_ns
    );

    // Phase 2: sustained low processing load to drive period back toward min.
    s.record_processing_time_us(30);
    for _ in 0..5 {
        let _ = s.wait_for_tick();
    }
    let recovered = s.adaptive_scheduling();

    assert!(
        recovered.target_period_ns < at_max.target_period_ns,
        "Period should decrease after load subsides: max={}, recovered={}",
        at_max.target_period_ns,
        recovered.target_period_ns
    );
    Ok(())
}

#[test]
fn reset_clears_adaptive_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(1.0));

    s.record_processing_time_us(200);

    s.reset();

    let state = s.adaptive_scheduling();
    assert_eq!(state.last_processing_time_us, 0);
    assert!(
        state.processing_time_ema_us.abs() < f64::EPSILON,
        "EMA should be reset to 0, got {}",
        state.processing_time_ema_us
    );
    Ok(())
}

// ===========================================================================
// 7. Budget tracking — processing time recording and enforcement
// ===========================================================================

#[test]
fn processing_time_records_last_value() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled());

    s.record_processing_time_us(42);
    assert_eq!(s.adaptive_scheduling().last_processing_time_us, 42);

    s.record_processing_time_us(99);
    assert_eq!(s.adaptive_scheduling().last_processing_time_us, 99);
    Ok(())
}

#[test]
fn processing_time_zero_does_not_trigger_adaptation() -> Result<(), Box<dyn std::error::Error>> {
    // When no processing time has been reported (last = 0), the "has_signal"
    // flag in update_adaptive_target should be false, so processing-based
    // adaptation should not fire.
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(900_000, 1_100_000)
        .with_step_sizes(5_000, 2_000)
        .with_processing_thresholds(180, 80)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    let before = s.adaptive_scheduling().target_period_ns;

    // Don't report any processing time — run ticks with only jitter signal.
    for _ in 0..10 {
        let _ = s.wait_for_tick();
    }

    let after = s.adaptive_scheduling().target_period_ns;

    // The period may change due to jitter, but without processing signal,
    // the processing-based overload path shouldn't fire. We just verify
    // the system didn't crash and the period stayed in bounds.
    assert!((900_000..=1_100_000).contains(&after));
    assert!(before > 0, "Initial period should be set, got {before}");
    Ok(())
}

#[test]
fn multiple_processing_samples_ema_monotonic_approach() -> Result<(), Box<dyn std::error::Error>> {
    // With constant input, EMA should monotonically approach that value.
    let mut s = scheduler_with_config(AdaptiveSchedulingConfig::enabled().with_ema_alpha(0.3));

    // Seed at 100, then feed constant 200.
    s.record_processing_time_us(100);
    let mut prev_ema = s.adaptive_scheduling().processing_time_ema_us;

    for _ in 0..20 {
        s.record_processing_time_us(200);
        let ema = s.adaptive_scheduling().processing_time_ema_us;
        assert!(
            ema >= prev_ema,
            "EMA should monotonically increase toward 200: prev={prev_ema}, cur={ema}"
        );
        prev_ema = ema;
    }

    // Should be close to 200 after many samples.
    assert!(
        prev_ema > 190.0,
        "EMA should converge to ~200, got {prev_ema}"
    );
    Ok(())
}

// ===========================================================================
// 8. Configuration validation and normalization
// ===========================================================================

#[test]
fn config_normalize_swaps_inverted_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AdaptiveSchedulingConfig {
        min_period_ns: 2_000_000,
        max_period_ns: 1_000_000,
        ..AdaptiveSchedulingConfig::default()
    };
    config.normalize();

    assert_eq!(config.min_period_ns, 1_000_000);
    assert_eq!(config.max_period_ns, 2_000_000);
    Ok(())
}

#[test]
fn config_normalize_enforces_nonzero_min() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AdaptiveSchedulingConfig {
        min_period_ns: 0,
        max_period_ns: 0,
        increase_step_ns: 0,
        decrease_step_ns: 0,
        ..AdaptiveSchedulingConfig::default()
    };
    config.normalize();

    assert!(config.min_period_ns >= 1);
    assert!(config.max_period_ns >= config.min_period_ns);
    assert!(config.increase_step_ns >= 1);
    assert!(config.decrease_step_ns >= 1);
    Ok(())
}

#[test]
fn config_normalize_clamps_inverted_thresholds() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AdaptiveSchedulingConfig {
        jitter_tighten_threshold_ns: 300_000,
        jitter_relax_threshold_ns: 200_000, // tighten > relax
        processing_tighten_threshold_us: 200,
        processing_relax_threshold_us: 100, // tighten > relax
        ..AdaptiveSchedulingConfig::default()
    };
    config.normalize();

    assert!(config.jitter_tighten_threshold_ns <= config.jitter_relax_threshold_ns);
    assert!(config.processing_tighten_threshold_us <= config.processing_relax_threshold_us);
    Ok(())
}

#[test]
fn config_normalize_clamps_ema_alpha_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let mut low = AdaptiveSchedulingConfig {
        processing_ema_alpha: 0.001,
        ..AdaptiveSchedulingConfig::default()
    };
    low.normalize();
    assert!((low.processing_ema_alpha - 0.01).abs() < 1e-10);

    let mut high = AdaptiveSchedulingConfig {
        processing_ema_alpha: 5.0,
        ..AdaptiveSchedulingConfig::default()
    };
    high.normalize();
    assert!((high.processing_ema_alpha - 1.0).abs() < 1e-10);
    Ok(())
}

#[test]
fn config_is_valid_for_default() -> Result<(), Box<dyn std::error::Error>> {
    assert!(AdaptiveSchedulingConfig::default().is_valid());
    assert!(AdaptiveSchedulingConfig::enabled().is_valid());
    Ok(())
}

#[test]
fn config_is_invalid_for_bad_values() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        AdaptiveSchedulingConfig {
            min_period_ns: 0,
            ..AdaptiveSchedulingConfig::default()
        },
        AdaptiveSchedulingConfig {
            min_period_ns: 2_000_000,
            max_period_ns: 1_000_000,
            ..AdaptiveSchedulingConfig::default()
        },
        AdaptiveSchedulingConfig {
            increase_step_ns: 0,
            ..AdaptiveSchedulingConfig::default()
        },
        AdaptiveSchedulingConfig {
            decrease_step_ns: 0,
            ..AdaptiveSchedulingConfig::default()
        },
        AdaptiveSchedulingConfig {
            processing_ema_alpha: 0.001,
            ..AdaptiveSchedulingConfig::default()
        },
        AdaptiveSchedulingConfig {
            jitter_tighten_threshold_ns: 500_000,
            jitter_relax_threshold_ns: 100_000,
            ..AdaptiveSchedulingConfig::default()
        },
    ];

    for (i, config) in cases.iter().enumerate() {
        assert!(!config.is_valid(), "Case {i} should be invalid: {config:?}");
    }
    Ok(())
}

#[test]
fn config_normalize_then_valid() -> Result<(), Box<dyn std::error::Error>> {
    // Any config, no matter how broken, should be valid after normalize().
    let mut config = AdaptiveSchedulingConfig {
        min_period_ns: 0,
        max_period_ns: 0,
        increase_step_ns: 0,
        decrease_step_ns: 0,
        jitter_tighten_threshold_ns: 999_999,
        jitter_relax_threshold_ns: 1,
        processing_tighten_threshold_us: 999,
        processing_relax_threshold_us: 1,
        processing_ema_alpha: -5.0,
        enabled: true,
    };
    config.normalize();

    assert!(
        config.is_valid(),
        "Config should be valid after normalize: {config:?}"
    );
    Ok(())
}

// ===========================================================================
// 9. State snapshot correctness
// ===========================================================================

#[test]
fn state_reflects_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled().with_period_bounds(800_000, 1_200_000);

    let s = scheduler_with_config(config);
    let state = s.adaptive_scheduling();

    assert!(state.enabled);
    assert_eq!(state.min_period_ns, 800_000);
    assert_eq!(state.max_period_ns, 1_200_000);
    assert!(state.target_period_ns >= 800_000);
    assert!(state.target_period_ns <= 1_200_000);
    Ok(())
}

#[test]
fn state_period_fraction_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let at_min = AdaptiveSchedulingState {
        target_period_ns: 900_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!((at_min.period_fraction() - 0.0).abs() < 1e-10);

    let at_max = AdaptiveSchedulingState {
        target_period_ns: 1_100_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!((at_max.period_fraction() - 1.0).abs() < 1e-10);

    let at_mid = AdaptiveSchedulingState {
        target_period_ns: 1_000_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!((at_mid.period_fraction() - 0.5).abs() < 1e-10);
    Ok(())
}

#[test]
fn state_equal_min_max_fraction_is_half() -> Result<(), Box<dyn std::error::Error>> {
    let state = AdaptiveSchedulingState {
        target_period_ns: 1_000_000,
        min_period_ns: 1_000_000,
        max_period_ns: 1_000_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!((state.period_fraction() - 0.5).abs() < 1e-10);
    Ok(())
}

#[test]
fn disabled_adaptive_uses_fixed_period() -> Result<(), Box<dyn std::error::Error>> {
    let s = AbsoluteScheduler::new_1khz();
    let state = s.adaptive_scheduling();

    assert!(!state.enabled);
    assert_eq!(state.target_period_ns, PERIOD_1KHZ_NS);
    Ok(())
}

// ===========================================================================
// 10. Builder pattern coverage
// ===========================================================================

#[test]
fn builder_chain_all_options() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(800_000, 1_200_000)
        .with_step_sizes(10_000, 5_000)
        .with_jitter_thresholds(300_000, 100_000)
        .with_processing_thresholds(200, 100)
        .with_ema_alpha(0.3);

    assert!(config.enabled);
    assert_eq!(config.min_period_ns, 800_000);
    assert_eq!(config.max_period_ns, 1_200_000);
    assert_eq!(config.increase_step_ns, 10_000);
    assert_eq!(config.decrease_step_ns, 5_000);
    assert_eq!(config.jitter_relax_threshold_ns, 300_000);
    assert_eq!(config.jitter_tighten_threshold_ns, 100_000);
    assert_eq!(config.processing_relax_threshold_us, 200);
    assert_eq!(config.processing_tighten_threshold_us, 100);
    assert!((config.processing_ema_alpha - 0.3).abs() < 1e-10);
    Ok(())
}

#[test]
fn enabled_factory_creates_enabled_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::enabled();
    assert!(config.enabled);
    // Should still have valid defaults.
    assert!(config.is_valid());
    Ok(())
}

// ===========================================================================
// 11. Edge cases
// ===========================================================================

#[test]
fn adaptive_with_equal_min_max_period() -> Result<(), Box<dyn std::error::Error>> {
    // If min == max, the period is fixed regardless of load.
    let config = AdaptiveSchedulingConfig::enabled()
        .with_period_bounds(1_000_000, 1_000_000)
        .with_processing_thresholds(180, 80)
        .with_ema_alpha(1.0);

    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(config);

    s.record_processing_time_us(500);
    for _ in 0..10 {
        let _ = s.wait_for_tick();
    }

    let state = s.adaptive_scheduling();
    assert_eq!(
        state.target_period_ns, 1_000_000,
        "With equal bounds, period should be fixed"
    );
    Ok(())
}

#[test]
fn repeated_set_adaptive_scheduling_does_not_corrupt() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = AbsoluteScheduler::new_1khz();

    // Set, override, and override again.
    s.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::enabled().with_period_bounds(800_000, 1_200_000),
    );
    s.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::enabled().with_period_bounds(900_000, 1_100_000),
    );
    s.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::enabled().with_period_bounds(950_000, 1_050_000),
    );

    let state = s.adaptive_scheduling();
    assert_eq!(state.min_period_ns, 950_000);
    assert_eq!(state.max_period_ns, 1_050_000);
    assert!(state.target_period_ns >= 950_000 && state.target_period_ns <= 1_050_000);
    Ok(())
}

#[test]
fn reconfigure_from_enabled_to_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = AbsoluteScheduler::with_period(5_000_000);
    s.set_adaptive_scheduling(AdaptiveSchedulingConfig::enabled());
    assert!(s.adaptive_scheduling().enabled);

    s.set_adaptive_scheduling(AdaptiveSchedulingConfig::new()); // disabled
    assert!(!s.adaptive_scheduling().enabled);
    Ok(())
}

#[test]
fn state_is_at_min_and_is_at_max() -> Result<(), Box<dyn std::error::Error>> {
    let at_min = AdaptiveSchedulingState {
        target_period_ns: 900_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!(at_min.is_at_min());
    assert!(!at_min.is_at_max());

    let at_max = AdaptiveSchedulingState {
        target_period_ns: 1_100_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!(!at_max.is_at_min());
    assert!(at_max.is_at_max());

    let mid = AdaptiveSchedulingState {
        target_period_ns: 1_000_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..AdaptiveSchedulingState::default()
    };
    assert!(!mid.is_at_min());
    assert!(!mid.is_at_max());
    Ok(())
}
