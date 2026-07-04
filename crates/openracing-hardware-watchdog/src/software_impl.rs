//! Software watchdog implementation.
//!
//! This module provides `SoftwareWatchdog`, a software-based implementation
//! of the `HardwareWatchdog` trait for testing and hardware-free environments.

use crate::config::WatchdogConfig;
use crate::error::{HardwareWatchdogError, HardwareWatchdogResult};
use crate::state::{WatchdogMetrics, WatchdogState, WatchdogStatus};
use crate::watchdog::HardwareWatchdog;
use portable_atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Software-based hardware watchdog implementation.
///
/// This implementation provides a software watchdog that can be used
/// when hardware watchdog is not available, or for testing purposes.
///
/// # Real-Time Safety
///
/// All methods are RT-safe:
/// - No heap allocations after initialization
/// - No blocking operations
/// - All state transitions are atomic
///
/// # WCET Bounds
///
/// - `feed()`: < 100ns
/// - `is_armed()`: < 50ns
/// - `has_timed_out()`: < 100ns
/// - `arm()`: < 500ns
/// - `disarm()`: < 500ns
/// - `trigger_safe_state()`: < 200ns
/// - `status()`: < 50ns
///
/// # Example
///
/// ```rust
/// use openracing_hardware_watchdog::{SoftwareWatchdog, WatchdogConfig, HardwareWatchdog};
///
/// let config = WatchdogConfig::new(100).expect("Valid config");
/// let mut watchdog = SoftwareWatchdog::new(config);
///
/// assert!(watchdog.arm().is_ok());
/// assert!(watchdog.feed().is_ok());
/// assert!(watchdog.is_armed());
/// assert!(!watchdog.has_timed_out());
/// ```
#[derive(Debug)]
pub struct SoftwareWatchdog {
    /// Watchdog configuration.
    config: WatchdogConfig,
    /// Watchdog state machine.
    state: WatchdogState,
    /// Last feed timestamp in microseconds.
    last_feed_us: AtomicU64,
    /// Time source start point (for elapsed time calculation).
    start_time_us: AtomicU64,
    /// Safe state triggered flag.
    safe_state_triggered: AtomicBool,
    /// Metrics snapshot counters.
    metrics: AtomicWatchdogMetrics,
}

impl SoftwareWatchdog {
    /// Create a new software watchdog with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Watchdog configuration.
    #[must_use]
    pub fn new(config: WatchdogConfig) -> Self {
        Self {
            config,
            state: WatchdogState::new(),
            last_feed_us: AtomicU64::new(0),
            start_time_us: AtomicU64::new(0),
            safe_state_triggered: AtomicBool::new(false),
            metrics: AtomicWatchdogMetrics::new(),
        }
    }

    /// Create a new software watchdog with a timeout in milliseconds.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Watchdog timeout in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the timeout is outside the valid range.
    pub fn with_timeout(timeout_ms: u32) -> HardwareWatchdogResult<Self> {
        let config = WatchdogConfig::new(timeout_ms)?;
        Ok(Self::new(config))
    }

    /// Create a new software watchdog with default 100ms timeout.
    #[must_use]
    pub fn with_default_timeout() -> Self {
        Self::new(WatchdogConfig::default())
    }

    /// Get elapsed time in microseconds since start.
    ///
    /// In `no_std` environments without a time source, this returns 0.
    /// Use `set_elapsed_us()` to provide time from an external source.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn elapsed_us(&self) -> u64 {
        #[cfg(feature = "std")]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
                let micros = d.as_micros();
                if micros > u128::from(u64::MAX) {
                    u64::MAX
                } else {
                    micros as u64
                }
            })
        }
        #[cfg(not(feature = "std"))]
        {
            0
        }
    }

    /// Set the elapsed time from an external source.
    ///
    /// Use this in `no_std` environments to provide time from an external
    /// source (e.g., a hardware timer).
    pub fn set_elapsed_us(&self, elapsed_us: u64) {
        self.start_time_us.store(elapsed_us, Ordering::Release);
    }

    /// Get the current timestamp for feeding.
    #[must_use]
    fn current_timestamp_us(&self) -> u64 {
        self.elapsed_us()
    }

    /// Check if the watchdog has timed out based on elapsed time.
    fn check_timeout(&self) -> bool {
        let status = self.state.status();
        if status != WatchdogStatus::Armed {
            return false;
        }

        let last_feed = self.last_feed_us.load(Ordering::Acquire);
        if last_feed == 0 {
            return false;
        }

        let current = self.current_timestamp_us();
        let elapsed = current.saturating_sub(last_feed);
        let timeout_us = self.config.timeout_us();

        elapsed > timeout_us
    }

    /// Manually trigger a timeout for testing purposes.
    ///
    /// This method forces the watchdog into the timed out state,
    /// which is useful for testing timeout handling logic.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 200ns
    ///
    /// # Errors
    ///
    /// Returns an error if the watchdog is not in the Armed state.
    pub fn trigger_timeout(&self) -> HardwareWatchdogResult<()> {
        self.state.timeout()?;
        self.metrics.record_timeout();
        Ok(())
    }
}

#[derive(Debug)]
struct AtomicWatchdogMetrics {
    feed_count: AtomicU64,
    arm_count: AtomicU64,
    timeout_count: AtomicU64,
    safe_state_count: AtomicU64,
    consecutive_failures: AtomicU32,
    max_feed_interval_us: AtomicU64,
    last_feed_timestamp_us: AtomicU64,
}

impl AtomicWatchdogMetrics {
    fn new() -> Self {
        Self {
            feed_count: AtomicU64::new(0),
            arm_count: AtomicU64::new(0),
            timeout_count: AtomicU64::new(0),
            safe_state_count: AtomicU64::new(0),
            consecutive_failures: AtomicU32::new(0),
            max_feed_interval_us: AtomicU64::new(0),
            last_feed_timestamp_us: AtomicU64::new(0),
        }
    }

    fn record_feed(&self, timestamp_us: u64) {
        let previous = self
            .last_feed_timestamp_us
            .swap(timestamp_us, Ordering::AcqRel);
        if previous > 0 {
            self.record_max_feed_interval(timestamp_us.saturating_sub(previous));
        }
        self.feed_count.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Release);
    }

    fn record_max_feed_interval(&self, interval_us: u64) {
        let mut current = self.max_feed_interval_us.load(Ordering::Acquire);
        while interval_us > current {
            match self.max_feed_interval_us.compare_exchange_weak(
                current,
                interval_us,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(observed) => current = observed,
            }
        }
    }

    fn record_arm(&self) {
        self.arm_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_timeout(&self) {
        self.timeout_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_safe_state(&self) {
        self.safe_state_count.fetch_add(1, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.feed_count.store(0, Ordering::Release);
        self.arm_count.store(0, Ordering::Release);
        self.timeout_count.store(0, Ordering::Release);
        self.safe_state_count.store(0, Ordering::Release);
        self.consecutive_failures.store(0, Ordering::Release);
        self.max_feed_interval_us.store(0, Ordering::Release);
        self.last_feed_timestamp_us.store(0, Ordering::Release);
    }

    fn snapshot(&self) -> WatchdogMetrics {
        WatchdogMetrics {
            feed_count: self.feed_count.load(Ordering::Acquire),
            arm_count: self.arm_count.load(Ordering::Acquire),
            timeout_count: self.timeout_count.load(Ordering::Acquire),
            safe_state_count: self.safe_state_count.load(Ordering::Acquire),
            consecutive_failures: self.consecutive_failures.load(Ordering::Acquire),
            max_feed_interval_us: self.max_feed_interval_us.load(Ordering::Acquire),
            last_feed_timestamp_us: self.last_feed_timestamp_us.load(Ordering::Acquire),
        }
    }
}

impl HardwareWatchdog for SoftwareWatchdog {
    fn feed(&mut self) -> HardwareWatchdogResult<()> {
        let status = self.state.status();

        match status {
            WatchdogStatus::Armed => {
                let timestamp = self.current_timestamp_us();
                self.last_feed_us.store(timestamp, Ordering::Release);
                self.state.feed()?;
                self.metrics.record_feed(timestamp);
                Ok(())
            }
            WatchdogStatus::TimedOut => Err(HardwareWatchdogError::TimedOut),
            WatchdogStatus::Disarmed => Err(HardwareWatchdogError::NotArmed),
            WatchdogStatus::SafeState => Err(HardwareWatchdogError::SafeStateAlreadyTriggered),
        }
    }

    fn timeout_ms(&self) -> u32 {
        self.config.timeout_ms
    }

    fn is_armed(&self) -> bool {
        self.state.status() == WatchdogStatus::Armed
    }

    fn arm(&mut self) -> HardwareWatchdogResult<()> {
        self.state.arm()?;
        self.start_time_us
            .store(self.current_timestamp_us(), Ordering::Release);
        self.last_feed_us
            .store(self.current_timestamp_us(), Ordering::Release);
        self.metrics.record_arm();
        Ok(())
    }

    fn disarm(&mut self) -> HardwareWatchdogResult<()> {
        self.state.disarm()
    }

    fn trigger_safe_state(&mut self) -> HardwareWatchdogResult<()> {
        self.state.trigger_safe_state()?;
        self.safe_state_triggered.store(true, Ordering::Release);
        self.metrics.record_safe_state();
        Ok(())
    }

    fn has_timed_out(&self) -> bool {
        if self.state.status() == WatchdogStatus::TimedOut {
            return true;
        }

        if self.check_timeout() {
            let _ = self.state.timeout();
            self.metrics.record_timeout();
            return true;
        }

        false
    }

    fn is_safe_state_triggered(&self) -> bool {
        self.safe_state_triggered.load(Ordering::Acquire)
    }

    fn status(&self) -> WatchdogStatus {
        self.state.status()
    }

    fn time_since_last_feed_us(&self) -> Option<u64> {
        let last_feed = self.last_feed_us.load(Ordering::Acquire);
        if last_feed == 0 {
            return None;
        }
        let current = self.current_timestamp_us();
        Some(current.saturating_sub(last_feed))
    }

    fn reset(&mut self) {
        self.state.reset();
        self.last_feed_us.store(0, Ordering::Release);
        self.start_time_us.store(0, Ordering::Release);
        self.safe_state_triggered.store(false, Ordering::Release);
        self.metrics.reset();
    }

    fn config(&self) -> &WatchdogConfig {
        &self.config
    }

    fn metrics(&self) -> WatchdogMetrics {
        self.metrics.snapshot()
    }
}

impl Default for SoftwareWatchdog {
    fn default() -> Self {
        Self::with_default_timeout()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_software_watchdog_creation() {
        let watchdog = SoftwareWatchdog::with_default_timeout();

        assert_eq!(watchdog.timeout_ms(), 100);
        assert!(!watchdog.is_armed());
        assert!(!watchdog.has_timed_out());
    }

    #[test]
    fn test_software_watchdog_default() {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert_eq!(watchdog.timeout_ms(), 100);
    }

    #[test]
    fn test_software_watchdog_is_send_sync_from_atomic_fields() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SoftwareWatchdog>();
    }

    #[test]
    fn test_atomic_metrics_tracks_feed_snapshot() {
        let metrics = AtomicWatchdogMetrics::new();

        metrics.record_feed(1_000);
        metrics.record_feed(1_500);
        metrics.record_feed(2_250);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.feed_count, 3);
        assert_eq!(snapshot.max_feed_interval_us, 750);
        assert_eq!(snapshot.last_feed_timestamp_us, 2_250);
    }

    #[test]
    fn test_atomic_metrics_reset_clears_snapshot() {
        let metrics = AtomicWatchdogMetrics::new();

        metrics.record_arm();
        metrics.record_feed(1_000);
        metrics.record_timeout();
        metrics.record_safe_state();
        metrics.reset();

        assert_eq!(metrics.snapshot(), WatchdogMetrics::new());
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_atomic_metrics_shared_updates_are_counted() {
        use std::sync::Arc;
        use std::thread;
        use std::vec::Vec;

        const WORKERS: u64 = 4;
        const UPDATES_PER_WORKER: u64 = 32;

        let metrics = Arc::new(AtomicWatchdogMetrics::new());
        let mut handles = Vec::new();
        for worker in 0..WORKERS {
            let metrics = Arc::clone(&metrics);
            handles.push(thread::spawn(move || {
                metrics.record_arm();
                for update in 0..UPDATES_PER_WORKER {
                    metrics.record_feed(worker * UPDATES_PER_WORKER + update + 1);
                    metrics.record_timeout();
                    metrics.record_safe_state();
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "metrics worker should finish");
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.arm_count, WORKERS);
        assert_eq!(snapshot.feed_count, WORKERS * UPDATES_PER_WORKER);
        assert_eq!(snapshot.timeout_count, WORKERS * UPDATES_PER_WORKER);
        assert_eq!(snapshot.safe_state_count, WORKERS * UPDATES_PER_WORKER);
    }

    #[test]
    fn test_arm_disarm() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(!watchdog.is_armed());

        assert!(watchdog.arm().is_ok());
        assert!(watchdog.is_armed());

        let result = watchdog.arm();
        assert!(result.is_err());

        assert!(watchdog.disarm().is_ok());
        assert!(!watchdog.is_armed());

        let result = watchdog.disarm();
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_when_disarmed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        let result = watchdog.feed();
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_when_armed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.arm().is_ok());
        let result = watchdog.feed();
        assert!(result.is_ok());

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 1);
    }

    #[test]
    fn test_trigger_safe_state() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.trigger_safe_state().is_ok());
        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);

        let result = watchdog.trigger_safe_state();
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.arm().is_ok());
        assert!(watchdog.feed().is_ok());

        watchdog.reset();

        assert!(!watchdog.is_armed());
        assert!(!watchdog.is_safe_state_triggered());
    }

    #[test]
    fn test_status() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);

        assert!(watchdog.arm().is_ok());
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);
    }

    #[test]
    fn test_is_healthy() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.is_healthy());

        assert!(watchdog.arm().is_ok());
        assert!(watchdog.is_healthy());
    }
}
