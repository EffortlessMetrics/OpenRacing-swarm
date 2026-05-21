//! Telemetry rate limiting utilities.
//!
//! Extracted from service telemetry runtime to keep rate control as a small,
//! reusable and independently versioned crate.

#![deny(static_mut_refs)]

use std::time::{Duration, Instant};

/// Rate limiter to protect RT-adjacent paths from telemetry parsing bursts.
pub struct RateLimiter {
    max_rate_hz: u32,
    min_interval: Duration,
    last_processed: Option<Instant>,
    dropped_count: u64,
    processed_count: u64,
}

impl RateLimiter {
    /// Create a new rate limiter with maximum rate in Hz.
    pub fn new(max_rate_hz: u32) -> Self {
        let divisor = max_rate_hz.max(1) as u64;
        let min_interval = Duration::from_nanos(1_000_000_000 / divisor);

        Self {
            max_rate_hz,
            min_interval,
            last_processed: None,
            dropped_count: 0,
            processed_count: 0,
        }
    }

    /// Returns true if processing should proceed at this instant.
    pub fn should_process(&mut self) -> bool {
        let now = Instant::now();

        if let Some(last) = self.last_processed {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_interval {
                self.dropped_count += 1;
                return false;
            }
        }

        self.last_processed = Some(now);
        self.processed_count += 1;
        true
    }

    /// Async variant that waits until a processing slot is available.
    pub async fn wait_for_slot(&mut self) {
        let now = Instant::now();

        if let Some(last) = self.last_processed {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_interval {
                let wait_time = self.min_interval - elapsed;
                tokio::time::sleep(wait_time).await;
            }
        }

        self.last_processed = Some(Instant::now());
        self.processed_count += 1;
    }

    /// Number of frames dropped for rate limiting.
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    /// Number of frames processed.
    pub fn processed_count(&self) -> u64 {
        self.processed_count
    }

    /// Current drop rate in percent.
    pub fn drop_rate_percent(&self) -> f32 {
        let total = self.dropped_count + self.processed_count;
        if total == 0 {
            0.0
        } else {
            (self.dropped_count as f32 / total as f32) * 100.0
        }
    }

    /// Reset collected statistics.
    pub fn reset_stats(&mut self) {
        self.dropped_count = 0;
        self.processed_count = 0;
    }

    /// Current max configured rate.
    pub fn max_rate_hz(&self) -> u32 {
        self.max_rate_hz
    }

    /// Update the max configured rate.
    pub fn set_max_rate_hz(&mut self, max_rate_hz: u32) {
        let effective = max_rate_hz.max(1);
        self.max_rate_hz = effective;
        self.min_interval = Duration::from_nanos(1_000_000_000 / effective as u64);
    }
}

/// Rate limiter statistics for monitoring.
#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub max_rate_hz: u32,
    pub processed_count: u64,
    pub dropped_count: u64,
    pub drop_rate_percent: f32,
}

impl From<&RateLimiter> for RateLimiterStats {
    fn from(limiter: &RateLimiter) -> Self {
        Self {
            max_rate_hz: limiter.max_rate_hz,
            processed_count: limiter.processed_count,
            dropped_count: limiter.dropped_count,
            drop_rate_percent: limiter.drop_rate_percent(),
        }
    }
}

/// Adaptive limiter that adjusts based on observed CPU usage.
pub struct AdaptiveRateLimiter {
    base_limiter: RateLimiter,
    initial_rate_hz: u32,
    target_cpu_percent: f32,
    current_cpu_percent: f32,
    adjustment_factor: f32,
}

impl AdaptiveRateLimiter {
    /// Create a new adaptive limiter.
    pub fn new(initial_rate_hz: u32, target_cpu_percent: f32) -> Self {
        Self {
            base_limiter: RateLimiter::new(initial_rate_hz),
            initial_rate_hz,
            target_cpu_percent,
            current_cpu_percent: 0.0,
            adjustment_factor: 1.0,
        }
    }

    /// Update the observed CPU usage and rebalance limiter behavior.
    pub fn update_cpu_usage(&mut self, cpu_percent: f32) {
        self.current_cpu_percent = cpu_percent;

        if cpu_percent > self.target_cpu_percent {
            self.adjustment_factor *= 0.95;
        } else if cpu_percent < self.target_cpu_percent * 0.8 {
            self.adjustment_factor *= 1.05;
        }

        self.adjustment_factor = self.adjustment_factor.clamp(0.1, 2.0);

        let adjusted_rate = (self.initial_rate_hz as f32 * self.adjustment_factor) as u32;
        self.base_limiter.set_max_rate_hz(adjusted_rate.max(1));
    }

    /// Returns true if processing should proceed.
    pub fn should_process(&mut self) -> bool {
        self.base_limiter.should_process()
    }

    /// Async variant that waits for the next processing slot.
    pub async fn wait_for_slot(&mut self) {
        self.base_limiter.wait_for_slot().await;
    }

    /// Snapshot limiter stats.
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats::from(&self.base_limiter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(1000);
        assert_eq!(limiter.max_rate_hz(), 1000);
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[test]
    fn test_rate_limiting() {
        let mut limiter = RateLimiter::new(10);

        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert_eq!(limiter.processed_count(), 1);
        assert_eq!(limiter.dropped_count(), 1);
    }

    #[test]
    fn test_drop_rate_calculation() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert!((limiter.drop_rate_percent() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_stats_reset() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        limiter.reset_stats();
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_async_rate_limiting() {
        let mut limiter = RateLimiter::new(100);

        limiter.wait_for_slot().await;
        let first = std::time::Instant::now();

        limiter.wait_for_slot().await;
        let second = std::time::Instant::now();

        assert!(second.duration_since(first) >= Duration::from_millis(8));
        assert_eq!(limiter.processed_count(), 2);
    }

    #[test]
    fn test_adaptive_rate_limiter() {
        let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
        adaptive.update_cpu_usage(80.0);
        let high = adaptive.stats();
        adaptive.update_cpu_usage(20.0);
        let low = adaptive.stats();
        assert!(low.max_rate_hz >= high.max_rate_hz);
    }

    #[test]
    fn test_rate_limiter_stats() {
        let mut limiter = RateLimiter::new(100);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert!(!limiter.should_process());

        let stats = RateLimiterStats::from(&limiter);
        assert_eq!(stats.max_rate_hz, 100);
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.dropped_count, 2);
        assert!(stats.drop_rate_percent > 0.0);
    }

    #[test]
    fn zero_capacity_is_safe() {
        // new(0): divisor is clamped to 1, so min_interval = 1s; stored rate may be 0
        let mut limiter = RateLimiter::new(0);
        // First call is always accepted regardless of stored rate
        assert!(limiter.should_process());
        // Second immediate call must be rejected (min_interval = 1s)
        assert!(!limiter.should_process());
    }

    #[test]
    fn set_max_rate_hz_zero_is_clamped_to_one() {
        let mut limiter = RateLimiter::new(100);
        limiter.set_max_rate_hz(0);
        assert_eq!(limiter.max_rate_hz(), 1);
    }

    #[test]
    fn drop_rate_percent_is_zero_when_no_events() {
        let limiter = RateLimiter::new(100);
        assert_eq!(limiter.drop_rate_percent(), 0.0);
    }

    #[test]
    fn drop_rate_percent_is_bounded() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        // Drop many calls
        for _ in 0..99 {
            limiter.should_process();
        }
        let rate = limiter.drop_rate_percent();
        assert!((0.0..=100.0).contains(&rate));
    }

    #[test]
    fn adaptive_limiter_clamps_adjustment_factor() {
        let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
        // Drive factor down toward minimum (0.1) by many high-cpu updates
        for _ in 0..200 {
            adaptive.update_cpu_usage(100.0);
        }
        let stats = adaptive.stats();
        assert!(stats.max_rate_hz >= 1);
        // Drive factor up toward maximum (2.0) by many low-cpu updates
        for _ in 0..200 {
            adaptive.update_cpu_usage(0.0);
        }
        let stats_high = adaptive.stats();
        assert!(stats_high.max_rate_hz >= stats.max_rate_hz);
    }

    #[test]
    fn first_call_always_accepted_for_positive_rate() {
        for rate in [1u32, 10, 100, 1000, u16::MAX as u32, u32::MAX] {
            let mut limiter = RateLimiter::new(rate);
            assert!(
                limiter.should_process(),
                "first call should be accepted for rate {rate}"
            );
        }
    }

    #[test]
    fn second_immediate_call_rejected_for_positive_rate() {
        for rate in [1u32, 10, 100, 10_000] {
            let mut limiter = RateLimiter::new(rate);
            let _ = limiter.should_process();
            assert!(
                !limiter.should_process(),
                "second immediate call should be rejected for rate {rate}"
            );
        }
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        #[test]
        fn prop_first_call_always_accepted(rate in 1u32..=1_000_000u32) {
            let mut limiter = RateLimiter::new(rate);
            prop_assert!(limiter.should_process());
            prop_assert_eq!(limiter.processed_count(), 1);
            prop_assert_eq!(limiter.dropped_count(), 0);
        }

        // Keep this in the same range as the integration property so the
        // interval stays well above two back-to-back Instant::now() calls on
        // hosted CI. Boundary tests cover near-zero high-rate behavior.
        #[test]
        fn prop_second_immediate_call_rejected(rate in 1u32..=10_000u32) {
            let mut limiter = RateLimiter::new(rate);
            let _ = limiter.should_process();
            prop_assert!(!limiter.should_process());
        }

        #[test]
        fn prop_set_max_rate_hz_roundtrips(rate in 1u32..=100_000u32) {
            let mut limiter = RateLimiter::new(1);
            limiter.set_max_rate_hz(rate);
            prop_assert_eq!(limiter.max_rate_hz(), rate);
        }

        #[test]
        fn prop_drop_rate_bounded(rate in 1u32..=100_000u32, burst in 1u32..=500u32) {
            let mut limiter = RateLimiter::new(rate);
            for _ in 0..burst {
                let _ = limiter.should_process();
            }
            let pct = limiter.drop_rate_percent();
            prop_assert!((0.0..=100.0).contains(&pct),
                "drop_rate_percent={pct} out of range for rate={rate}, burst={burst}");
        }

        #[test]
        fn prop_processed_plus_dropped_equals_total(rate in 1u32..=10_000u32, burst in 1u32..=200u32) {
            let mut limiter = RateLimiter::new(rate);
            for _ in 0..burst {
                let _ = limiter.should_process();
            }
            prop_assert_eq!(
                limiter.processed_count() + limiter.dropped_count(),
                u64::from(burst),
                "processed + dropped must equal total calls"
            );
        }

        #[test]
        fn prop_reset_stats_zeroes_counters(rate in 1u32..=10_000u32, burst in 1u32..=100u32) {
            let mut limiter = RateLimiter::new(rate);
            for _ in 0..burst {
                let _ = limiter.should_process();
            }
            limiter.reset_stats();
            prop_assert_eq!(limiter.processed_count(), 0);
            prop_assert_eq!(limiter.dropped_count(), 0);
            prop_assert_eq!(limiter.drop_rate_percent(), 0.0);
        }

        #[test]
        fn prop_set_max_rate_hz_zero_clamps(rate in 0u32..=1u32) {
            let mut limiter = RateLimiter::new(100);
            limiter.set_max_rate_hz(rate);
            prop_assert!(limiter.max_rate_hz() >= 1,
                "set_max_rate_hz({rate}) must clamp to >=1");
        }

        #[test]
        fn prop_stats_snapshot_consistent(rate in 1u32..=10_000u32, burst in 1u32..=200u32) {
            let mut limiter = RateLimiter::new(rate);
            for _ in 0..burst {
                let _ = limiter.should_process();
            }
            let stats = RateLimiterStats::from(&limiter);
            prop_assert_eq!(stats.max_rate_hz, limiter.max_rate_hz());
            prop_assert_eq!(stats.processed_count, limiter.processed_count());
            prop_assert_eq!(stats.dropped_count, limiter.dropped_count());
            let expected_pct = limiter.drop_rate_percent();
            prop_assert!((stats.drop_rate_percent - expected_pct).abs() < f32::EPSILON);
        }

        #[test]
        fn prop_adaptive_rate_never_zero(
            initial_rate in 1u32..=10_000u32,
            target_cpu in 1.0f32..=99.0f32,
            observed_cpu in 0.0f32..=100.0f32,
            iterations in 1u32..=100u32,
        ) {
            let mut adaptive = AdaptiveRateLimiter::new(initial_rate, target_cpu);
            for _ in 0..iterations {
                adaptive.update_cpu_usage(observed_cpu);
            }
            let stats = adaptive.stats();
            prop_assert!(stats.max_rate_hz >= 1,
                "adaptive rate must never drop to 0");
        }
    }

    // -----------------------------------------------------------------------
    // Reconfiguration mid-stream
    // -----------------------------------------------------------------------

    #[test]
    fn reconfigure_preserves_timing_state() {
        let mut limiter = RateLimiter::new(100);
        assert!(limiter.should_process());
        // Reconfigure to a slower rate
        limiter.set_max_rate_hz(10);
        // Immediate call should be rejected (old timestamp still recent)
        assert!(!limiter.should_process());
        assert_eq!(limiter.max_rate_hz(), 10);
    }

    #[test]
    fn reconfigure_faster_allows_sooner() {
        let mut limiter = RateLimiter::new(1); // 1 Hz = 1s interval
        assert!(limiter.should_process());
        // Switch to max rate — near-zero interval
        limiter.set_max_rate_hz(u32::MAX);
        assert!(limiter.should_process());
    }

    #[test]
    fn multiple_reconfigurations_stable() {
        let mut limiter = RateLimiter::new(100);
        for rate in [1, 1000, 50, u32::MAX, 1, 60] {
            limiter.set_max_rate_hz(rate);
            let effective = limiter.max_rate_hz();
            assert!(
                effective >= 1,
                "rate must be >= 1 after set_max_rate_hz({rate})"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Boundary values
    // -----------------------------------------------------------------------

    #[test]
    fn boundary_rate_one_hz() {
        let mut limiter = RateLimiter::new(1);
        assert!(limiter.should_process());
        // At 1 Hz, min_interval = 1s so immediate call must be rejected
        assert!(!limiter.should_process());
        assert_eq!(limiter.processed_count(), 1);
        assert_eq!(limiter.dropped_count(), 1);
    }

    #[test]
    fn boundary_rate_u32_max() {
        let mut limiter = RateLimiter::new(u32::MAX);
        assert!(limiter.should_process());
        // Near-zero interval so second call should also pass
        assert!(limiter.should_process());
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[test]
    fn boundary_set_max_rate_hz_to_u32_max() {
        let mut limiter = RateLimiter::new(1);
        limiter.set_max_rate_hz(u32::MAX);
        // The stored value is u32::MAX (since max(1) = u32::MAX)
        assert_eq!(limiter.max_rate_hz(), u32::MAX);
    }

    #[test]
    fn boundary_zero_rate_constructor_first_accepted() {
        let mut limiter = RateLimiter::new(0);
        assert!(limiter.should_process());
        assert_eq!(limiter.processed_count(), 1);
    }

    #[test]
    fn large_burst_counters_do_not_overflow() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        for _ in 0..10_000 {
            let _ = limiter.should_process();
        }
        assert_eq!(limiter.processed_count() + limiter.dropped_count(), 10_001);
    }

    // -----------------------------------------------------------------------
    // Reset and statistics edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn reset_stats_then_continue_counting() {
        let mut limiter = RateLimiter::new(60);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        limiter.reset_stats();

        // After reset, counters are zero but timing state is preserved
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);

        // Next immediate call should still be rate-limited
        assert!(!limiter.should_process());
        assert_eq!(limiter.dropped_count(), 1);
    }

    #[test]
    fn drop_rate_percent_all_processed() {
        let mut limiter = RateLimiter::new(u32::MAX);
        // At near-zero interval, many calls should be processed
        for _ in 0..10 {
            let _ = limiter.should_process();
        }
        // All should be processed with near-zero interval
        assert_eq!(limiter.dropped_count(), 0);
        assert_eq!(limiter.drop_rate_percent(), 0.0);
    }

    #[test]
    fn stats_snapshot_after_reset_is_clean() {
        let mut limiter = RateLimiter::new(100);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        limiter.reset_stats();

        let stats = RateLimiterStats::from(&limiter);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.dropped_count, 0);
        assert_eq!(stats.drop_rate_percent, 0.0);
        assert_eq!(stats.max_rate_hz, 100);
    }

    // -----------------------------------------------------------------------
    // Adaptive rate limiter: deeper coverage
    // -----------------------------------------------------------------------

    #[test]
    fn adaptive_stable_cpu_preserves_rate() {
        let mut adaptive = AdaptiveRateLimiter::new(500, 50.0);
        // CPU between target * 0.8 = 40% and target = 50% → no adjustment
        for _ in 0..50 {
            adaptive.update_cpu_usage(45.0);
        }
        assert_eq!(adaptive.stats().max_rate_hz, 500);
    }

    #[test]
    fn adaptive_rapid_oscillation_converges() {
        let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
        for i in 0..100 {
            let cpu = if i % 2 == 0 { 80.0 } else { 20.0 };
            adaptive.update_cpu_usage(cpu);
        }
        let stats = adaptive.stats();
        // Should still be within clamped bounds
        assert!(stats.max_rate_hz >= 1);
        assert!(stats.max_rate_hz <= 2000);
    }

    #[test]
    fn adaptive_stats_reflect_base_limiter() {
        let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
        assert!(adaptive.should_process());
        assert!(!adaptive.should_process());
        let stats = adaptive.stats();
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.dropped_count, 1);
    }

    #[test]
    fn adaptive_zero_cpu_increases_rate() {
        let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
        let initial = adaptive.stats().max_rate_hz;
        for _ in 0..50 {
            adaptive.update_cpu_usage(0.0);
        }
        assert!(adaptive.stats().max_rate_hz > initial);
    }

    #[test]
    fn adaptive_max_cpu_decreases_rate() {
        let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
        let initial = adaptive.stats().max_rate_hz;
        for _ in 0..50 {
            adaptive.update_cpu_usage(100.0);
        }
        assert!(adaptive.stats().max_rate_hz < initial);
    }

    // -----------------------------------------------------------------------
    // Timer precision edge cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn async_wait_first_slot_is_immediate() {
        let mut limiter = RateLimiter::new(60);
        let start = Instant::now();
        limiter.wait_for_slot().await;
        let elapsed = start.elapsed();
        // First slot should be nearly immediate (no prior timestamp)
        assert!(
            elapsed < Duration::from_millis(5),
            "first slot should be immediate, took {elapsed:?}"
        );
        assert_eq!(limiter.processed_count(), 1);
    }

    #[tokio::test]
    async fn async_wait_at_1hz_respects_interval() {
        let mut limiter = RateLimiter::new(1);
        limiter.wait_for_slot().await;
        let start = Instant::now();
        limiter.wait_for_slot().await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(950),
            "1Hz should wait ~1s, took {elapsed:?}"
        );
    }
}
