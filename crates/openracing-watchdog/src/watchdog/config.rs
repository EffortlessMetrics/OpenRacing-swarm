//! Watchdog configuration and builder types.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::{WatchdogError, WatchdogResult};

/// Watchdog configuration for different components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// Plugin execution timeout per tick (microseconds).
    pub plugin_timeout_us: u64,
    /// Maximum consecutive plugin timeouts before quarantine.
    pub plugin_max_timeouts: u32,
    /// Plugin quarantine duration.
    pub plugin_quarantine_duration: Duration,
    /// RT thread heartbeat timeout (milliseconds).
    pub rt_thread_timeout_ms: u64,
    /// HID communication timeout (milliseconds).
    pub hid_timeout_ms: u64,
    /// Telemetry timeout (milliseconds).
    pub telemetry_timeout_ms: u64,
    /// System health check interval.
    pub health_check_interval: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            plugin_timeout_us: 100,
            plugin_max_timeouts: 5,
            plugin_quarantine_duration: Duration::from_mins(5),
            rt_thread_timeout_ms: 10,
            hid_timeout_ms: 50,
            telemetry_timeout_ms: 1000,
            health_check_interval: Duration::from_millis(100),
        }
    }
}

impl WatchdogConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration values are invalid.
    pub fn validate(&self) -> WatchdogResult<()> {
        if self.plugin_timeout_us == 0 {
            return Err(WatchdogError::invalid_configuration(
                "plugin_timeout_us must be greater than 0",
            ));
        }
        if self.plugin_max_timeouts == 0 {
            return Err(WatchdogError::invalid_configuration(
                "plugin_max_timeouts must be greater than 0",
            ));
        }
        if self.plugin_quarantine_duration.is_zero() {
            return Err(WatchdogError::invalid_configuration(
                "plugin_quarantine_duration must be greater than 0",
            ));
        }
        if self.rt_thread_timeout_ms == 0 {
            return Err(WatchdogError::invalid_configuration(
                "rt_thread_timeout_ms must be greater than 0",
            ));
        }
        Ok(())
    }

    /// Create a configuration builder.
    #[must_use]
    pub fn builder() -> WatchdogConfigBuilder {
        WatchdogConfigBuilder::default()
    }
}

/// Builder for [`WatchdogConfig`].
#[derive(Debug, Default)]
pub struct WatchdogConfigBuilder {
    config: WatchdogConfig,
}

impl WatchdogConfigBuilder {
    /// Set plugin timeout in microseconds.
    #[must_use]
    pub fn plugin_timeout_us(mut self, us: u64) -> Self {
        self.config.plugin_timeout_us = us;
        self
    }

    /// Set maximum consecutive timeouts before quarantine.
    #[must_use]
    pub fn plugin_max_timeouts(mut self, count: u32) -> Self {
        self.config.plugin_max_timeouts = count;
        self
    }

    /// Set quarantine duration.
    #[must_use]
    pub fn plugin_quarantine_duration(mut self, duration: Duration) -> Self {
        self.config.plugin_quarantine_duration = duration;
        self
    }

    /// Set RT thread timeout in milliseconds.
    #[must_use]
    pub fn rt_thread_timeout_ms(mut self, ms: u64) -> Self {
        self.config.rt_thread_timeout_ms = ms;
        self
    }

    /// Set HID timeout in milliseconds.
    #[must_use]
    pub fn hid_timeout_ms(mut self, ms: u64) -> Self {
        self.config.hid_timeout_ms = ms;
        self
    }

    /// Set telemetry timeout in milliseconds.
    #[must_use]
    pub fn telemetry_timeout_ms(mut self, ms: u64) -> Self {
        self.config.telemetry_timeout_ms = ms;
        self
    }

    /// Set health check interval.
    #[must_use]
    pub fn health_check_interval(mut self, interval: Duration) -> Self {
        self.config.health_check_interval = interval;
        self
    }

    /// Build the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn build(self) -> WatchdogResult<WatchdogConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}
