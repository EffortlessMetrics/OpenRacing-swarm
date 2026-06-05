//! Linux-specific platform implementation.

use crate::error::{RTError, RTResult};
use crate::rt_setup::RTSetup;
use core::time::Duration;
use libc::{
    CLOCK_MONOTONIC, MCL_CURRENT, MCL_FUTURE, SCHED_FIFO, clock_nanosleep, mlockall, sched_param,
    sched_setscheduler, time_t, timespec,
};
use std::time::Instant;

const RT_THREAD_PRIORITY: i32 = 80;

/// Linux-specific sleep implementation.
pub struct PlatformSleep;

impl PlatformSleep {
    /// Create new platform sleep instance.
    pub fn new() -> Self {
        Self
    }

    /// Apply Linux-specific RT setup.
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        apply_linux_rt_setup(setup);
        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail.
    ///
    /// Uses clock_nanosleep for the bulk of the sleep, then busy-spins
    /// for the final ~80 microseconds to achieve precise timing.
    pub fn sleep_until(&mut self, target: Instant) -> RTResult {
        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // For very short durations, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));
        let ts = duration_to_timespec(sleep_duration);
        sleep_relative_with_clock(&ts)?;

        // Busy-spin for final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }
}

fn apply_linux_rt_setup(setup: &RTSetup) {
    let param = sched_param {
        sched_priority: RT_THREAD_PRIORITY,
    };

    // SAFETY: `sched_setscheduler` receives pid 0 to target the current
    // process/thread context and a pointer to the initialized `sched_param`
    // that lives for the duration of the call. `mlockall` receives only valid
    // libc flag constants. Both calls are best-effort RT setup; failures are
    // intentionally non-fatal because ordinary users may lack CAP_SYS_NICE or
    // mlock permissions.
    unsafe {
        if setup.high_priority {
            let _ = sched_setscheduler(0, SCHED_FIFO, &param);
        }

        if setup.lock_memory {
            let _ = mlockall(MCL_CURRENT | MCL_FUTURE);
        }
    }
}

fn duration_to_timespec(duration: Duration) -> timespec {
    timespec {
        tv_sec: saturating_time_t_secs(duration.as_secs()),
        tv_nsec: duration.subsec_nanos().into(),
    }
}

fn saturating_time_t_secs(seconds: u64) -> time_t {
    match time_t::try_from(seconds) {
        Ok(value) => value,
        Err(_) => time_t::MAX,
    }
}

fn valid_timespec(ts: &timespec) -> bool {
    ts.tv_sec >= 0 && ts.tv_nsec >= 0 && ts.tv_nsec < 1_000_000_000
}

fn sleep_relative_with_clock(ts: &timespec) -> RTResult {
    if !valid_timespec(ts) {
        return Err(RTError::TimingViolation);
    }

    // SAFETY: `ts` is validated as a relative `timespec` and remains valid for
    // the duration of the call. A null remainder pointer is permitted by
    // `clock_nanosleep` when the caller does not need interrupted-sleep state.
    let result = unsafe { clock_nanosleep(CLOCK_MONOTONIC, 0, ts, std::ptr::null_mut()) };

    if result != 0 {
        return Err(RTError::TimingViolation);
    }

    Ok(())
}

impl Default for PlatformSleep {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_to_timespec_sets_valid_fields() {
        let ts = duration_to_timespec(Duration::new(2, 123_456_789));

        assert_eq!(ts.tv_sec, 2);
        assert_eq!(ts.tv_nsec, 123_456_789);
        assert!(valid_timespec(&ts));
    }

    #[test]
    fn duration_to_timespec_saturates_seconds() {
        let ts = duration_to_timespec(Duration::from_secs(u64::MAX));

        assert_eq!(ts.tv_sec, time_t::MAX);
        assert_eq!(ts.tv_nsec, 0);
        assert!(valid_timespec(&ts));
    }

    #[test]
    fn valid_timespec_rejects_invalid_nanoseconds() {
        let ts = timespec {
            tv_sec: 0,
            tv_nsec: 1_000_000_000,
        };

        assert!(!valid_timespec(&ts));
    }
}
