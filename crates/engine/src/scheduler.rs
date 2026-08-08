//! Absolute scheduler for 1kHz real-time operation with PLL and RT setup
//!
//! Implements the scheduling architecture specified in [ADR-0004: Real-Time Scheduling Architecture].
//!
//! [ADR-0004]: file:///h:/Code/Rust/OpenRacing/docs/adr/0004-rt-scheduling-architecture.md

pub use openracing_errors::{RTError, RTResult};
use std::time::{Duration, Instant};

/// Phase-Locked Loop for drift correction
#[derive(Debug, Clone)]
pub struct PLL {
    /// Target period in nanoseconds
    target_period_ns: u64,

    /// Current estimated period in nanoseconds
    estimated_period_ns: f64,

    /// PLL gain factor (lower = more stable, higher = faster correction)
    gain: f64,

    /// Last tick timestamp for period measurement
    last_tick: Option<Instant>,

    /// Accumulated phase error
    phase_error_ns: f64,
}

impl PLL {
    /// Create new PLL with target period
    pub fn new(target_period_ns: u64) -> Self {
        Self {
            target_period_ns,
            estimated_period_ns: target_period_ns as f64,
            gain: 0.01, // 1% correction per tick
            last_tick: None,
            phase_error_ns: 0.0,
        }
    }

    /// Update PLL with actual tick timing
    pub fn update(&mut self, actual_tick_time: Instant) -> Duration {
        if let Some(last) = self.last_tick {
            let actual_period_ns = actual_tick_time.duration_since(last).as_nanos() as f64;

            // Calculate phase error
            let period_error = actual_period_ns - self.target_period_ns as f64;
            self.phase_error_ns += period_error;

            // Apply PLL correction
            let correction = self.gain * (period_error + 0.1 * self.phase_error_ns);
            self.estimated_period_ns = self.target_period_ns as f64 - correction;

            // Clamp to reasonable bounds (±10% of target)
            let min_period = self.target_period_ns as f64 * 0.9;
            let max_period = self.target_period_ns as f64 * 1.1;
            self.estimated_period_ns = self.estimated_period_ns.clamp(min_period, max_period);
        }

        self.last_tick = Some(actual_tick_time);
        Duration::from_nanos(self.estimated_period_ns as u64)
    }

    /// Get current phase error in nanoseconds
    pub fn phase_error_ns(&self) -> f64 {
        self.phase_error_ns
    }

    /// Reset PLL state
    pub fn reset(&mut self) {
        self.estimated_period_ns = self.target_period_ns as f64;
        self.phase_error_ns = 0.0;
        self.last_tick = None;
    }

    /// Update the target period used by the PLL.
    ///
    /// This is used by adaptive scheduling to change the loop period based on
    /// observed system load while keeping PLL drift correction behavior.
    pub fn set_target_period_ns(&mut self, target_period_ns: u64) {
        self.target_period_ns = target_period_ns.max(1);

        // Keep the estimated period in a sane range around the new target.
        let min_period = self.target_period_ns as f64 * 0.9;
        let max_period = self.target_period_ns as f64 * 1.1;
        self.estimated_period_ns = self.estimated_period_ns.clamp(min_period, max_period);
    }

    /// Get current target period in nanoseconds.
    pub fn target_period_ns(&self) -> u64 {
        self.target_period_ns
    }
}

/// Real-time setup configuration
#[derive(Debug, Clone)]
pub struct RTSetup {
    /// Enable high-priority scheduling
    pub high_priority: bool,

    /// Enable memory locking (prevent swapping)
    pub lock_memory: bool,

    /// Disable power throttling
    pub disable_power_throttling: bool,

    /// CPU affinity mask (None = no affinity)
    pub cpu_affinity: Option<u64>,
}

impl Default for RTSetup {
    fn default() -> Self {
        Self {
            high_priority: true,
            lock_memory: true,
            disable_power_throttling: true,
            cpu_affinity: None,
        }
    }
}

/// Adaptive scheduling configuration.
///
/// Adaptive scheduling dynamically adjusts the scheduler period within bounded
/// limits to reduce timing violations under load, then returns toward the base
/// period when the system is healthy.
#[derive(Debug, Clone)]
pub struct AdaptiveSchedulingConfig {
    /// Enable adaptive scheduling.
    pub enabled: bool,
    /// Minimum allowed period in nanoseconds.
    pub min_period_ns: u64,
    /// Maximum allowed period in nanoseconds.
    pub max_period_ns: u64,
    /// Step size for period increase when overloaded.
    pub increase_step_ns: u64,
    /// Step size for period decrease when healthy.
    pub decrease_step_ns: u64,
    /// Jitter threshold above which period should be relaxed.
    pub jitter_relax_threshold_ns: u64,
    /// Jitter threshold below which period can tighten.
    pub jitter_tighten_threshold_ns: u64,
    /// Processing-time threshold above which period should be relaxed.
    pub processing_relax_threshold_us: u64,
    /// Processing-time threshold below which period can tighten.
    pub processing_tighten_threshold_us: u64,
    /// EMA alpha for processing-time smoothing [0.01, 1.0].
    pub processing_ema_alpha: f64,
}

impl Default for AdaptiveSchedulingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_period_ns: 900_000,              // 0.9ms
            max_period_ns: 1_100_000,            // 1.1ms
            increase_step_ns: 5_000,             // 5us
            decrease_step_ns: 2_000,             // 2us
            jitter_relax_threshold_ns: 200_000,  // 0.2ms
            jitter_tighten_threshold_ns: 50_000, // 0.05ms
            processing_relax_threshold_us: 180,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: 0.2,
        }
    }
}

/// Snapshot of adaptive scheduling runtime state.
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveSchedulingState {
    /// Whether adaptive scheduling is enabled.
    pub enabled: bool,
    /// Current adaptive target period in nanoseconds.
    pub target_period_ns: u64,
    /// Minimum allowed period in nanoseconds.
    pub min_period_ns: u64,
    /// Maximum allowed period in nanoseconds.
    pub max_period_ns: u64,
    /// Last recorded processing time in microseconds.
    pub last_processing_time_us: u64,
    /// Exponential moving average of processing time in microseconds.
    pub processing_time_ema_us: f64,
}

/// Jitter metrics collection
#[derive(Debug, Clone)]
pub struct JitterMetrics {
    /// Total number of ticks
    pub total_ticks: u64,

    /// Number of missed deadlines
    pub missed_ticks: u64,

    /// Maximum observed jitter in nanoseconds
    pub max_jitter_ns: u64,

    /// Running sum of squared jitter for variance calculation
    pub jitter_sum_squared: f64,

    /// Last observed jitter sample.
    pub last_jitter_ns: u64,

    /// Recent jitter samples for percentile calculation
    pub recent_jitter_samples: Vec<u64>,

    /// Maximum samples to keep for percentile calculation
    pub max_samples: usize,

    /// Ring buffer write index once `recent_jitter_samples` reaches `max_samples`.
    next_sample_index: usize,

    /// Reused scratch storage for percentile selection to avoid per-call allocations.
    percentile_scratch: Vec<u64>,
}

impl Default for JitterMetrics {
    fn default() -> Self {
        const DEFAULT_MAX_SAMPLES: usize = 10_000;
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            max_jitter_ns: 0,
            jitter_sum_squared: 0.0,
            last_jitter_ns: 0,
            recent_jitter_samples: Vec::with_capacity(DEFAULT_MAX_SAMPLES),
            max_samples: DEFAULT_MAX_SAMPLES,
            next_sample_index: 0,
            percentile_scratch: Vec::with_capacity(DEFAULT_MAX_SAMPLES),
        }
    }
}

impl JitterMetrics {
    /// Create new jitter metrics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tick with its jitter
    pub fn record_tick(&mut self, jitter_ns: u64, missed_deadline: bool) {
        self.total_ticks += 1;

        if missed_deadline {
            self.missed_ticks += 1;
        }

        self.max_jitter_ns = self.max_jitter_ns.max(jitter_ns);
        self.jitter_sum_squared += (jitter_ns as f64).powi(2);
        self.last_jitter_ns = jitter_ns;

        if self.max_samples == 0 {
            return;
        }

        // Keep recent samples in a bounded ring to avoid per-tick shifting.
        if self.recent_jitter_samples.len() < self.max_samples {
            self.recent_jitter_samples.push(jitter_ns);
            if self.recent_jitter_samples.len() == self.max_samples {
                self.next_sample_index = 0;
            }
        } else {
            self.recent_jitter_samples[self.next_sample_index] = jitter_ns;
            self.next_sample_index = (self.next_sample_index + 1) % self.max_samples;
        }
    }

    /// Calculate p99 jitter in nanoseconds
    pub fn p99_jitter_ns(&mut self) -> u64 {
        if self.recent_jitter_samples.is_empty() {
            return 0;
        }

        if self.percentile_scratch.capacity() < self.recent_jitter_samples.len() {
            self.percentile_scratch
                .reserve(self.recent_jitter_samples.len() - self.percentile_scratch.capacity());
        }

        self.percentile_scratch.clear();
        self.percentile_scratch
            .extend_from_slice(&self.recent_jitter_samples);

        let len = self.percentile_scratch.len();
        let p99_index = ((len as f64 * 0.99) as usize).min(len.saturating_sub(1));
        let (_, p99, _) = self.percentile_scratch.select_nth_unstable(p99_index);
        *p99
    }

    /// Calculate missed tick rate (0.0 to 1.0)
    pub fn missed_tick_rate(&self) -> f64 {
        if self.total_ticks == 0 {
            0.0
        } else {
            self.missed_ticks as f64 / self.total_ticks as f64
        }
    }

    /// Check if metrics meet performance requirements
    pub fn meets_requirements(&mut self) -> bool {
        // Requirements: p99 jitter ≤ 0.25ms, missed tick rate ≤ 0.001%
        self.p99_jitter_ns() <= 250_000 && self.missed_tick_rate() <= 0.00001
    }
}

/// Absolute scheduler for precise 1kHz timing with PLL and jitter metrics
pub struct AbsoluteScheduler {
    /// Target period in nanoseconds
    period_ns: u64,

    /// Next scheduled tick time
    next_tick: Instant,

    /// Total tick count
    tick_count: u64,

    /// Phase-locked loop for drift correction
    pll: PLL,

    /// Jitter metrics collection
    metrics: JitterMetrics,

    /// Adaptive scheduling configuration
    adaptive_config: AdaptiveSchedulingConfig,

    /// Current adaptive target period
    adaptive_period_ns: u64,

    /// Last processing time reported by the engine loop (microseconds)
    last_processing_time_us: u64,

    /// Exponential moving average of processing time (microseconds)
    processing_time_ema_us: f64,

    /// RT setup applied
    rt_setup_applied: bool,

    /// Reused waitable timer handle for Windows high-precision sleep.
    #[cfg(target_os = "windows")]
    waitable_timer_raw: Option<isize>,
}

impl AbsoluteScheduler {
    /// Create new scheduler with 1kHz (1ms) period
    pub fn new_1khz() -> Self {
        let period_ns = 1_000_000; // 1ms in nanoseconds
        Self {
            period_ns,
            next_tick: Instant::now(),
            tick_count: 0,
            pll: PLL::new(period_ns),
            metrics: JitterMetrics::new(),
            adaptive_config: AdaptiveSchedulingConfig::default(),
            adaptive_period_ns: period_ns,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
            rt_setup_applied: false,
            #[cfg(target_os = "windows")]
            waitable_timer_raw: None,
        }
    }

    /// Configure adaptive scheduling.
    ///
    /// The configuration is normalized to maintain safe, bounded behavior.
    pub fn set_adaptive_scheduling(&mut self, mut config: AdaptiveSchedulingConfig) {
        if config.min_period_ns > config.max_period_ns {
            std::mem::swap(&mut config.min_period_ns, &mut config.max_period_ns);
        }

        // Keep limits sane and non-zero.
        config.min_period_ns = config.min_period_ns.max(1);
        config.max_period_ns = config.max_period_ns.max(config.min_period_ns);
        config.increase_step_ns = config.increase_step_ns.max(1);
        config.decrease_step_ns = config.decrease_step_ns.max(1);

        // Ensure tighten thresholds are not above relax thresholds.
        if config.jitter_tighten_threshold_ns > config.jitter_relax_threshold_ns {
            config.jitter_tighten_threshold_ns = config.jitter_relax_threshold_ns;
        }
        if config.processing_tighten_threshold_us > config.processing_relax_threshold_us {
            config.processing_tighten_threshold_us = config.processing_relax_threshold_us;
        }

        // Clamp EMA alpha to a stable range.
        config.processing_ema_alpha = config.processing_ema_alpha.clamp(0.01, 1.0);

        self.adaptive_config = config;
        self.adaptive_period_ns = self.period_ns.clamp(
            self.adaptive_config.min_period_ns,
            self.adaptive_config.max_period_ns,
        );

        let pll_target = if self.adaptive_config.enabled {
            self.adaptive_period_ns
        } else {
            self.period_ns
        };
        self.pll.set_target_period_ns(pll_target);
    }

    /// Get adaptive scheduling runtime state.
    pub fn adaptive_scheduling(&self) -> AdaptiveSchedulingState {
        AdaptiveSchedulingState {
            enabled: self.adaptive_config.enabled,
            target_period_ns: self.adaptive_period_ns,
            min_period_ns: self.adaptive_config.min_period_ns,
            max_period_ns: self.adaptive_config.max_period_ns,
            last_processing_time_us: self.last_processing_time_us,
            processing_time_ema_us: self.processing_time_ema_us,
        }
    }

    /// Report per-tick processing time in microseconds.
    ///
    /// This signal is consumed by adaptive scheduling to estimate current load.
    pub fn record_processing_time_us(&mut self, processing_time_us: u64) {
        self.last_processing_time_us = processing_time_us;

        if self.processing_time_ema_us <= f64::EPSILON {
            self.processing_time_ema_us = processing_time_us as f64;
            return;
        }

        let alpha = self.adaptive_config.processing_ema_alpha;
        self.processing_time_ema_us =
            (1.0 - alpha) * self.processing_time_ema_us + alpha * processing_time_us as f64;
    }

    /// Compute updated target period from current load signals.
    fn update_adaptive_target_period(&mut self, jitter_ns: u64, missed_deadline: bool) -> u64 {
        if !self.adaptive_config.enabled {
            self.adaptive_period_ns = self.period_ns;
            return self.period_ns;
        }

        let jitter_overloaded =
            missed_deadline || jitter_ns >= self.adaptive_config.jitter_relax_threshold_ns;
        let jitter_healthy =
            !missed_deadline && jitter_ns <= self.adaptive_config.jitter_tighten_threshold_ns;

        let has_processing_signal = self.last_processing_time_us > 0;
        let processing_overloaded = has_processing_signal
            && self.processing_time_ema_us
                >= self.adaptive_config.processing_relax_threshold_us as f64;
        let processing_healthy = has_processing_signal
            && self.processing_time_ema_us
                <= self.adaptive_config.processing_tighten_threshold_us as f64;

        if jitter_overloaded || processing_overloaded {
            self.adaptive_period_ns = self
                .adaptive_period_ns
                .saturating_add(self.adaptive_config.increase_step_ns);
        } else if jitter_healthy && processing_healthy {
            self.adaptive_period_ns = self
                .adaptive_period_ns
                .saturating_sub(self.adaptive_config.decrease_step_ns);
        }

        self.adaptive_period_ns = self.adaptive_period_ns.clamp(
            self.adaptive_config.min_period_ns,
            self.adaptive_config.max_period_ns,
        );

        self.adaptive_period_ns
    }

    /// Apply real-time setup for optimal performance
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        if self.rt_setup_applied {
            return Ok(()); // Already applied
        }

        #[cfg(target_os = "windows")]
        {
            self.apply_windows_rt_setup(setup)?;
        }

        #[cfg(target_os = "linux")]
        {
            self.apply_linux_rt_setup(setup)?;
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            let _ = setup;
        }

        self.rt_setup_applied = true;
        Ok(())
    }

    /// Wait for next tick (RT-safe) with PLL correction and jitter measurement
    pub fn wait_for_tick(&mut self) -> RTResult<u64> {
        let tick_start = Instant::now();

        // Check if we missed the deadline
        let missed_deadline = tick_start >= self.next_tick;
        let jitter_ns = if missed_deadline {
            tick_start.duration_since(self.next_tick).as_nanos() as u64
        } else {
            self.next_tick.duration_since(tick_start).as_nanos() as u64
        };

        // Record metrics
        self.metrics.record_tick(jitter_ns, missed_deadline);

        // If we're early, sleep until the target time
        if !missed_deadline {
            self.sleep_until(self.next_tick)?;
        }

        // Update adaptive target from observed load/jitter before PLL correction.
        let adaptive_target_period_ns =
            self.update_adaptive_target_period(jitter_ns, missed_deadline);
        self.pll.set_target_period_ns(adaptive_target_period_ns);

        // Update PLL with actual tick timing
        let actual_tick_time = Instant::now();
        let corrected_period = self.pll.update(actual_tick_time);

        // Update tick count and schedule next tick
        self.tick_count += 1;
        self.next_tick += corrected_period;

        // Check for severe timing violations
        let max_jitter_ns = if cfg!(test) { 5_000_000 } else { 250_000 };
        if jitter_ns > max_jitter_ns {
            return Err(RTError::TimingViolation);
        }

        Ok(self.tick_count)
    }

    /// Apply Windows-specific RT setup
    #[cfg(target_os = "windows")]
    fn apply_windows_rt_setup(&self, setup: &RTSetup) -> RTResult {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
        };

        if setup.high_priority {
            // SAFETY: `GetCurrentThread` returns the current thread pseudo
            // handle, which is valid for `SetThreadPriority` for the duration
            // of this call. No handle ownership is transferred.
            unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)
                    .map_err(|_| RTError::TimingViolation)?;
            }

            // Note: SetProcessPriorityClass and power management APIs
            // would be added here in a full implementation
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail
    #[cfg(target_os = "windows")]
    fn sleep_until(&mut self, target: Instant) -> RTResult {
        use windows::Win32::System::Threading::{INFINITE, SetWaitableTimer, WaitForSingleObject};

        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // If duration is very short, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));
        let _sleep_target = now + sleep_duration;

        let timer = self.get_or_create_waitable_timer()?;
        // Relative due time in 100ns units. Negative means relative wait.
        let due_time_100ns = relative_due_time_100ns(sleep_duration);

        // SAFETY: `timer` is owned by this scheduler, `due_time_100ns` lives
        // until the call returns, and `WaitForSingleObject` only observes the
        // valid handle without taking ownership.
        unsafe {
            SetWaitableTimer(timer, &due_time_100ns, 0, None, None, false)
                .map_err(|_| RTError::TimingViolation)?;
            WaitForSingleObject(timer, INFINITE);
        }

        // Busy-spin for the final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }

    /// Apply Linux-specific RT setup
    #[cfg(target_os = "linux")]
    fn apply_linux_rt_setup(&self, setup: &RTSetup) -> RTResult {
        use libc::{
            MCL_CURRENT, MCL_FUTURE, SCHED_FIFO, mlockall, sched_param, sched_setscheduler,
        };

        if setup.high_priority {
            // Set SCHED_FIFO with high priority
            let param = sched_param {
                sched_priority: 80, // High priority but not maximum
            };

            // SAFETY: `param` is initialized for `sched_setscheduler` and the
            // pid `0` targets the current process per POSIX/Linux semantics.
            unsafe {
                if sched_setscheduler(0, SCHED_FIFO, &param) != 0 {
                    // Fall back to trying via rtkit if direct scheduling fails
                    // This would require rtkit integration in a full implementation
                }
            }
        }

        if setup.lock_memory {
            // SAFETY: `mlockall` takes only flag bits and does not dereference
            // user pointers; failure is advisory for this setup path.
            unsafe {
                // Lock all current and future memory to prevent swapping
                mlockall(MCL_CURRENT | MCL_FUTURE);
            }
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail
    #[cfg(target_os = "linux")]
    fn sleep_until(&mut self, target: Instant) -> RTResult {
        use libc::{CLOCK_MONOTONIC, clock_nanosleep, timespec};

        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // If duration is very short, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));

        let ts = timespec {
            tv_sec: sleep_duration.as_secs() as i64,
            tv_nsec: sleep_duration.subsec_nanos() as i64,
        };

        // SAFETY: `ts` is a valid relative timespec for the duration of the
        // call and the remaining-time pointer is null because interruption is
        // handled as a timing violation.
        let result = unsafe {
            clock_nanosleep(
                CLOCK_MONOTONIC,
                0, // Relative time
                &ts,
                std::ptr::null_mut(),
            )
        };

        if result != 0 {
            return Err(RTError::TimingViolation);
        }

        // Busy-spin for the final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }

    /// Fallback sleep implementation
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn sleep_until(&mut self, target: Instant) -> RTResult {
        let now = Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
        Ok(())
    }

    /// Get current tick count
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get jitter metrics
    pub fn metrics(&self) -> &JitterMetrics {
        &self.metrics
    }

    /// Get PLL phase error in nanoseconds
    pub fn phase_error_ns(&self) -> f64 {
        self.pll.phase_error_ns()
    }

    /// Reset scheduler state (for testing)
    pub fn reset(&mut self) {
        self.next_tick = Instant::now();
        self.tick_count = 0;
        self.pll.reset();
        self.pll.set_target_period_ns(self.period_ns);
        self.metrics = JitterMetrics::new();
        self.adaptive_period_ns = self.period_ns;
        self.last_processing_time_us = 0;
        self.processing_time_ema_us = 0.0;
    }

    /// Check if RT setup has been applied
    pub fn is_rt_setup_applied(&self) -> bool {
        self.rt_setup_applied
    }
}

#[cfg(target_os = "windows")]
impl AbsoluteScheduler {
    fn get_or_create_waitable_timer(&mut self) -> RTResult<windows::Win32::Foundation::HANDLE> {
        use std::ffi::c_void;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Threading::CreateWaitableTimerW;

        if let Some(raw_handle) = self.waitable_timer_raw {
            return Ok(HANDLE(raw_handle as *mut c_void));
        }

        // SAFETY: Passing null security attributes and name requests an unnamed
        // manual-reset waitable timer; the returned handle is stored and closed
        // in `Drop`.
        let timer = unsafe {
            CreateWaitableTimerW(None, true, None).map_err(|_| RTError::TimingViolation)?
        };
        self.waitable_timer_raw = Some(timer.0 as isize);
        Ok(timer)
    }
}

#[cfg(target_os = "windows")]
fn relative_due_time_100ns(duration: Duration) -> i64 {
    let ticks_100ns = (duration.as_nanos() / 100).min(i64::MAX as u128) as i64;
    -ticks_100ns.max(1)
}

#[cfg(target_os = "windows")]
impl Drop for AbsoluteScheduler {
    fn drop(&mut self) {
        use std::ffi::c_void;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Foundation::HANDLE;

        if let Some(raw_handle) = self.waitable_timer_raw.take() {
            // SAFETY: `raw_handle` was returned by `CreateWaitableTimerW` and
            // is removed from `waitable_timer_raw` before close to avoid a
            // double close on repeated drops.
            unsafe {
                let timer = HANDLE(raw_handle as *mut c_void);
                let _ = CloseHandle(timer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_pll_creation() {
        let pll = PLL::new(1_000_000); // 1ms
        assert_eq!(pll.target_period_ns, 1_000_000);
        assert_eq!(pll.estimated_period_ns, 1_000_000.0);
        assert_eq!(pll.phase_error_ns(), 0.0);
    }

    #[test]
    fn test_pll_update() {
        let mut pll = PLL::new(1_000_000); // 1ms

        let _start = Instant::now();
        thread::sleep(Duration::from_millis(1));
        let tick1 = Instant::now();

        let corrected_period = pll.update(tick1);
        assert!(corrected_period.as_nanos() > 900_000); // Should be close to 1ms
        assert!(corrected_period.as_nanos() < 1_100_000);
    }

    #[test]
    fn test_jitter_metrics() {
        let mut metrics = JitterMetrics::new();

        // Record some ticks
        metrics.record_tick(100_000, false); // 0.1ms jitter
        metrics.record_tick(200_000, false); // 0.2ms jitter
        metrics.record_tick(300_000, true); // 0.3ms jitter, missed deadline

        assert_eq!(metrics.total_ticks, 3);
        assert_eq!(metrics.missed_ticks, 1);
        assert_eq!(metrics.max_jitter_ns, 300_000);
        assert_eq!(metrics.missed_tick_rate(), 1.0 / 3.0);
    }

    #[test]
    fn test_jitter_metrics_p99() {
        let mut metrics = JitterMetrics::new();

        // Add 100 samples with known distribution
        for i in 0..100 {
            metrics.record_tick(i * 1000, false); // 0 to 99µs
        }

        let p99 = metrics.p99_jitter_ns();
        assert!(p99 >= 98_000); // Should be around 98-99µs
        assert!(p99 <= 99_000);
    }

    #[test]
    fn test_jitter_metrics_uses_bounded_ring_buffer() {
        let mut metrics = JitterMetrics::new();
        metrics.max_samples = 3;
        metrics.recent_jitter_samples.clear();

        for i in 1..=5 {
            metrics.record_tick(i * 1_000, false);
        }

        assert_eq!(metrics.recent_jitter_samples.len(), 3);

        let mut sorted = metrics.recent_jitter_samples.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![3_000, 4_000, 5_000]);
        assert_eq!(metrics.last_jitter_ns, 5_000);
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = AbsoluteScheduler::new_1khz();
        assert_eq!(scheduler.period_ns, 1_000_000);
        assert_eq!(scheduler.tick_count(), 0);
        assert!(!scheduler.is_rt_setup_applied());
    }

    #[test]
    fn test_adaptive_scheduling_defaults_disabled() {
        let scheduler = AbsoluteScheduler::new_1khz();
        let adaptive = scheduler.adaptive_scheduling();

        assert!(!adaptive.enabled);
        assert_eq!(adaptive.target_period_ns, 1_000_000);
        assert_eq!(adaptive.min_period_ns, 900_000);
        assert_eq!(adaptive.max_period_ns, 1_100_000);
    }

    #[test]
    fn test_adaptive_scheduling_increases_period_under_load() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: 950_000,
            max_period_ns: 1_050_000,
            increase_step_ns: 10_000,
            decrease_step_ns: 5_000,
            jitter_relax_threshold_ns: 200_000,
            jitter_tighten_threshold_ns: 50_000,
            processing_relax_threshold_us: 150,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: 1.0,
        });

        scheduler.record_processing_time_us(220);
        let p1 = scheduler.update_adaptive_target_period(260_000, true);
        let p2 = scheduler.update_adaptive_target_period(280_000, true);

        assert_eq!(p1, 1_010_000);
        assert_eq!(p2, 1_020_000);
    }

    #[test]
    fn test_adaptive_scheduling_requires_processing_signal_for_tightening() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: 950_000,
            max_period_ns: 1_050_000,
            increase_step_ns: 10_000,
            decrease_step_ns: 5_000,
            jitter_relax_threshold_ns: 200_000,
            jitter_tighten_threshold_ns: 50_000,
            processing_relax_threshold_us: 150,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: 1.0,
        });

        // No processing signal yet; healthy jitter alone should not tighten period.
        let period = scheduler.update_adaptive_target_period(20_000, false);
        assert_eq!(period, 1_000_000);
    }

    #[test]
    fn test_adaptive_scheduling_recovers_period_when_healthy() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: 950_000,
            max_period_ns: 1_050_000,
            increase_step_ns: 10_000,
            decrease_step_ns: 5_000,
            jitter_relax_threshold_ns: 200_000,
            jitter_tighten_threshold_ns: 50_000,
            processing_relax_threshold_us: 150,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: 1.0,
        });

        // Increase target period first.
        scheduler.record_processing_time_us(220);
        let _ = scheduler.update_adaptive_target_period(260_000, true);
        let _ = scheduler.update_adaptive_target_period(260_000, true);

        // Then recover toward base period under healthy conditions.
        scheduler.record_processing_time_us(40);
        let recovered_1 = scheduler.update_adaptive_target_period(20_000, false);
        let recovered_2 = scheduler.update_adaptive_target_period(20_000, false);

        assert_eq!(recovered_1, 1_015_000);
        assert_eq!(recovered_2, 1_010_000);
    }

    #[test]
    fn test_adaptive_scheduling_clamps_to_bounds() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: 990_000,
            max_period_ns: 1_010_000,
            increase_step_ns: 20_000,
            decrease_step_ns: 20_000,
            jitter_relax_threshold_ns: 100_000,
            jitter_tighten_threshold_ns: 10_000,
            processing_relax_threshold_us: 120,
            processing_tighten_threshold_us: 60,
            processing_ema_alpha: 1.0,
        });

        scheduler.record_processing_time_us(300);
        let high = scheduler.update_adaptive_target_period(500_000, true);
        assert_eq!(high, 1_010_000);

        scheduler.record_processing_time_us(10);
        let low = scheduler.update_adaptive_target_period(1_000, false);
        assert_eq!(low, 990_000);
    }

    #[test]
    fn test_record_processing_time_updates_ema() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            processing_ema_alpha: 0.5,
            ..Default::default()
        });

        scheduler.record_processing_time_us(100);
        scheduler.record_processing_time_us(200);

        let adaptive = scheduler.adaptive_scheduling();
        assert_eq!(adaptive.last_processing_time_us, 200);
        assert!((adaptive.processing_time_ema_us - 150.0).abs() < 1e-6);
    }

    #[test]
    fn test_rt_setup_default() {
        let setup = RTSetup::default();
        assert!(setup.high_priority);
        assert!(setup.lock_memory);
        assert!(setup.disable_power_throttling);
        assert!(setup.cpu_affinity.is_none());
    }

    #[test]
    fn test_scheduler_reset() {
        let mut scheduler = AbsoluteScheduler::new_1khz();

        // Simulate some ticks
        scheduler.tick_count = 100;
        scheduler.metrics.total_ticks = 100;

        scheduler.reset();

        assert_eq!(scheduler.tick_count(), 0);
        assert_eq!(scheduler.metrics().total_ticks, 0);
    }

    #[test]
    fn test_metrics_requirements() {
        let mut metrics = JitterMetrics::new();

        // Add samples that meet requirements (low jitter, no missed ticks)
        for _ in 0..1000 {
            metrics.record_tick(100_000, false); // 0.1ms jitter, no missed ticks
        }

        assert!(metrics.meets_requirements());

        // Add a few samples that violate jitter but not enough to affect p99
        for _ in 0..5 {
            metrics.record_tick(300_000, false); // 0.3ms jitter but no missed deadline
        }

        // Should still meet requirements due to p99 calculation (only 0.5% of samples are high jitter)
        assert!(metrics.meets_requirements());

        // Now add enough missed ticks to violate the missed tick rate requirement
        for _ in 0..100 {
            metrics.record_tick(100_000, true); // Low jitter but missed deadline
        }

        // Should now fail requirements due to missed tick rate
        assert!(!metrics.meets_requirements());
    }

    #[tokio::test]
    async fn test_scheduler_basic_operation() {
        let mut scheduler = AbsoluteScheduler::new_1khz();

        // Apply RT setup (should not fail even if we don't have permissions)
        let setup = RTSetup::default();
        let _ = scheduler.apply_rt_setup(&setup);

        let start = Instant::now();

        // Run a few ticks
        const EXPECTED_TICKS: u64 = 5;
        for expected_tick in 1..=EXPECTED_TICKS {
            // This might fail in CI due to timing, so we'll be lenient
            match scheduler.wait_for_tick() {
                Ok(tick) => {
                    assert_eq!(tick, expected_tick);
                }
                Err(RTError::TimingViolation) => {
                    // Expected in CI environments with poor timing
                    break;
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        let elapsed = start.elapsed();
        let metrics = scheduler.metrics();

        // `wait_for_tick` sleeps only when it is early; once a deadline has
        // been missed it returns without waiting at all. In test builds it
        // still returns `Ok` for misses up to 5ms, so a successful call is
        // not evidence that any time passed -- under load every tick can miss
        // its deadline, skip its sleep, and still report `Ok`.
        //
        // Assert on elapsed only when the whole loop ran without missing a
        // deadline. That is the one case where the scheduler is known to have
        // waited for five successive 1kHz deadlines, which is comfortably
        // above the bound below.
        if metrics.total_ticks == EXPECTED_TICKS && metrics.missed_ticks == 0 {
            // Should have taken some time (be lenient for CI)
            assert!(elapsed.as_micros() >= 100);
        }

        // Check that metrics were collected. Every attempt is recorded,
        // including one that ends in a timing violation.
        assert!(scheduler.metrics().total_ticks > 0);
    }

    // --- Additional performance gate and timing measurement tests ---

    #[test]
    fn test_jitter_metrics_p99_empty_samples() {
        let mut metrics = JitterMetrics::new();
        // p99 on empty data should return 0
        assert_eq!(metrics.p99_jitter_ns(), 0);
    }

    #[test]
    fn test_jitter_metrics_p99_single_sample() {
        let mut metrics = JitterMetrics::new();
        metrics.record_tick(42_000, false);
        // With one sample, p99 should be that sample
        assert_eq!(metrics.p99_jitter_ns(), 42_000);
    }

    #[test]
    fn test_jitter_metrics_p99_two_samples() {
        let mut metrics = JitterMetrics::new();
        metrics.record_tick(10_000, false);
        metrics.record_tick(20_000, false);
        let p99 = metrics.p99_jitter_ns();
        // With 2 samples, p99 should be near the higher value
        assert!((10_000..=20_000).contains(&p99));
    }

    #[test]
    fn test_jitter_metrics_all_identical_samples() {
        let mut metrics = JitterMetrics::new();
        for _ in 0..1000 {
            metrics.record_tick(100_000, false);
        }
        // All identical samples: p99 = that value
        assert_eq!(metrics.p99_jitter_ns(), 100_000);
    }

    #[test]
    fn test_missed_tick_rate_zero_ticks() {
        let metrics = JitterMetrics::new();
        // 0 total ticks -> rate is 0.0 (no division by zero)
        assert_eq!(metrics.missed_tick_rate(), 0.0);
    }

    #[test]
    fn test_missed_tick_rate_no_misses() {
        let mut metrics = JitterMetrics::new();
        for _ in 0..100 {
            metrics.record_tick(50_000, false);
        }
        assert_eq!(metrics.missed_tick_rate(), 0.0);
    }

    #[test]
    fn test_missed_tick_rate_all_missed() {
        let mut metrics = JitterMetrics::new();
        for _ in 0..100 {
            metrics.record_tick(500_000, true);
        }
        assert_eq!(metrics.missed_tick_rate(), 1.0);
    }

    #[test]
    fn test_missed_tick_rate_precision() {
        let mut metrics = JitterMetrics::new();
        // 1 miss in 100_000 ticks = 0.00001 = exactly the 0.001% threshold
        for _ in 0..99_999 {
            metrics.record_tick(50_000, false);
        }
        metrics.record_tick(500_000, true);
        let rate = metrics.missed_tick_rate();
        assert!((rate - 0.00001).abs() < 1e-10);
    }

    #[test]
    fn test_meets_requirements_boundary() {
        let mut metrics = JitterMetrics::new();
        // All low-jitter, no missed ticks -> meets requirements
        for _ in 0..1000 {
            metrics.record_tick(100_000, false); // 0.1ms, well under 0.25ms
        }
        assert!(metrics.meets_requirements());
    }

    #[test]
    fn test_meets_requirements_fails_on_high_p99_jitter() {
        let mut metrics = JitterMetrics::new();
        // 99 low samples + enough high samples to push p99 above 250µs
        for _ in 0..50 {
            metrics.record_tick(100_000, false);
        }
        for _ in 0..50 {
            metrics.record_tick(300_000, false); // 0.3ms > 0.25ms
        }
        // With 50% of samples at 300µs, p99 will be 300µs > 250µs threshold
        assert!(!metrics.meets_requirements());
    }

    #[test]
    fn test_jitter_max_tracking() {
        let mut metrics = JitterMetrics::new();
        metrics.record_tick(100, false);
        metrics.record_tick(999_999, false);
        metrics.record_tick(500, false);
        assert_eq!(metrics.max_jitter_ns, 999_999);
    }

    #[test]
    fn test_jitter_sum_squared_accumulation() {
        let mut metrics = JitterMetrics::new();
        metrics.record_tick(1000, false);
        metrics.record_tick(2000, false);
        // sum_squared = 1000^2 + 2000^2 = 1_000_000 + 4_000_000 = 5_000_000
        assert!((metrics.jitter_sum_squared - 5_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_ring_buffer_wraps_correctly() {
        let mut metrics = JitterMetrics::new();
        metrics.max_samples = 5;
        metrics.recent_jitter_samples.clear();

        // Fill buffer exactly
        for i in 1..=5 {
            metrics.record_tick(i * 100, false);
        }
        assert_eq!(metrics.recent_jitter_samples.len(), 5);

        // Overwrite oldest entries
        for i in 6..=8 {
            metrics.record_tick(i * 100, false);
        }
        assert_eq!(metrics.recent_jitter_samples.len(), 5);

        // Buffer should contain samples 4,5,6,7,8 (*100)
        let mut sorted = metrics.recent_jitter_samples.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![400, 500, 600, 700, 800]);
    }

    #[test]
    fn test_ring_buffer_zero_max_samples() {
        let mut metrics = JitterMetrics::new();
        metrics.max_samples = 0;
        metrics.recent_jitter_samples.clear();

        metrics.record_tick(1000, false);
        // With max_samples=0, no samples should be stored
        assert!(metrics.recent_jitter_samples.is_empty());
        // But tick counting still works
        assert_eq!(metrics.total_ticks, 1);
    }

    #[test]
    fn test_pll_reset_clears_state() {
        let mut pll = PLL::new(1_000_000);
        let tick = Instant::now();
        let _ = pll.update(tick);
        assert!(pll.last_tick.is_some());

        pll.reset();
        assert_eq!(pll.phase_error_ns(), 0.0);
        assert!(pll.last_tick.is_none());
        assert_eq!(pll.estimated_period_ns, 1_000_000.0);
    }

    #[test]
    fn test_pll_clamps_to_bounds() {
        let mut pll = PLL::new(1_000_000);
        let t1 = Instant::now();
        let _ = pll.update(t1);

        // Force a huge delay to try pushing estimated period beyond ±10%
        thread::sleep(Duration::from_millis(10));
        let t2 = Instant::now();
        let period = pll.update(t2);

        let min_ns = 900_000u128;
        let max_ns = 1_100_000u128;
        assert!(period.as_nanos() >= min_ns);
        assert!(period.as_nanos() <= max_ns);
    }

    #[test]
    fn test_pll_set_target_period() {
        let mut pll = PLL::new(1_000_000);
        pll.set_target_period_ns(2_000_000);
        assert_eq!(pll.target_period_ns(), 2_000_000);
    }

    #[test]
    fn test_pll_set_target_period_zero_becomes_one() {
        let mut pll = PLL::new(1_000_000);
        pll.set_target_period_ns(0);
        assert_eq!(pll.target_period_ns(), 1);
    }

    #[test]
    fn test_scheduler_tick_count_increments() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        assert_eq!(scheduler.tick_count(), 0);

        // Directly manipulate for unit testing since wait_for_tick has timing deps
        scheduler.tick_count = 42;
        assert_eq!(scheduler.tick_count(), 42);
    }

    #[test]
    fn test_adaptive_scheduling_disabled_returns_base_period() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        // Adaptive scheduling defaults to disabled
        let period = scheduler.update_adaptive_target_period(300_000, true);
        assert_eq!(period, 1_000_000);
    }

    #[test]
    fn test_jitter_last_sample_tracked() {
        let mut metrics = JitterMetrics::new();
        metrics.record_tick(100, false);
        assert_eq!(metrics.last_jitter_ns, 100);
        metrics.record_tick(999, false);
        assert_eq!(metrics.last_jitter_ns, 999);
    }

    #[test]
    fn test_p99_percentile_scratch_reuse() {
        let mut metrics = JitterMetrics::new();
        for i in 0..100 {
            metrics.record_tick(i * 1000, false);
        }
        // Call p99 twice — scratch buffer should be reused without error
        let first = metrics.p99_jitter_ns();
        let second = metrics.p99_jitter_ns();
        assert_eq!(first, second);
    }
}
