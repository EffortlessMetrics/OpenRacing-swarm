//! Plugin execution monitoring and quarantine operations.

use std::time::Duration;

use crate::FaultType;
use crate::error::{WatchdogError, WatchdogResult};
use crate::quarantine::QuarantineReason;
use crate::stats::PluginStats;

use super::WatchdogSystem;

impl WatchdogSystem {
    /// Record plugin execution.
    ///
    /// Returns `Some(FaultType)` if the plugin was quarantined due to consecutive timeouts.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe. The internal write lock is held for a minimal duration.
    /// Note: First execution for a plugin ID will allocate a new entry.
    pub fn record_plugin_execution(
        &self,
        plugin_id: &str,
        execution_time_us: u64,
    ) -> Option<FaultType> {
        let should_quarantine = {
            let mut stats = self.plugin_stats.write();
            let plugin_stats = stats.entry(plugin_id.to_string()).or_default();

            if execution_time_us > self.config.plugin_timeout_us {
                plugin_stats.record_timeout(execution_time_us);

                let quarantine_enabled = *self.quarantine_policy_enabled.read();
                quarantine_enabled
                    && plugin_stats.consecutive_timeouts >= self.config.plugin_max_timeouts
            } else {
                plugin_stats.record_success(execution_time_us);
                false
            }
        };

        if should_quarantine {
            self.quarantine_plugin(plugin_id);
            return Some(FaultType::PluginOverrun);
        }

        None
    }

    /// Quarantine a plugin.
    fn quarantine_plugin(&self, plugin_id: &str) {
        let reason = {
            let mut stats = self.plugin_stats.write();
            let plugin_stats = stats.entry(plugin_id.to_string()).or_default();
            plugin_stats.consecutive_timeouts = 0;

            let mut manager = self.quarantine_manager.write();
            manager.quarantine(
                plugin_id,
                Some(self.config.plugin_quarantine_duration),
                QuarantineReason::ConsecutiveTimeouts,
                plugin_stats,
            );
            QuarantineReason::ConsecutiveTimeouts
        };

        tracing::warn!(
            plugin_id = plugin_id,
            reason = ?reason,
            "Plugin quarantined due to consecutive timeouts"
        );

        let callbacks = self.fault_callbacks.read();
        for callback in callbacks.iter() {
            callback(FaultType::PluginOverrun, plugin_id);
        }
    }

    /// Check if a plugin is quarantined.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    #[must_use]
    pub fn is_plugin_quarantined(&self, plugin_id: &str) -> bool {
        let stats = self.plugin_stats.read();
        stats
            .get(plugin_id)
            .is_some_and(PluginStats::is_quarantined)
    }

    /// Get plugin statistics.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe for read access.
    #[must_use]
    pub fn get_plugin_stats(&self, plugin_id: &str) -> Option<PluginStats> {
        let stats = self.plugin_stats.read();
        stats.get(plugin_id).cloned()
    }

    /// Get all plugin statistics.
    #[must_use]
    pub fn get_all_plugin_stats(&self) -> std::collections::HashMap<String, PluginStats> {
        let stats = self.plugin_stats.read();
        stats.clone()
    }

    /// Release a plugin from quarantine.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found or not quarantined.
    pub fn release_plugin_quarantine(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        let plugin_stats = stats
            .get_mut(plugin_id)
            .ok_or_else(|| WatchdogError::plugin_not_found(plugin_id))?;

        if !plugin_stats.is_quarantined() {
            return Err(WatchdogError::not_quarantined(plugin_id));
        }

        let mut manager = self.quarantine_manager.write();
        manager.release(plugin_id, plugin_stats)?;

        tracing::info!(plugin_id = plugin_id, "Plugin released from quarantine");
        Ok(())
    }

    /// Get all quarantined plugins with remaining duration.
    #[must_use]
    pub fn get_quarantined_plugins(&self) -> Vec<(String, Duration)> {
        let manager = self.quarantine_manager.read();
        manager.get_quarantined()
    }

    /// Check if any plugins are currently quarantined.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    #[must_use]
    pub fn has_any_quarantined_plugins(&self) -> bool {
        let manager = self.quarantine_manager.read();
        !manager.is_empty()
    }

    /// Reset plugin statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn reset_plugin_stats(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        let plugin_stats = stats
            .get_mut(plugin_id)
            .ok_or_else(|| WatchdogError::plugin_not_found(plugin_id))?;
        plugin_stats.reset();
        Ok(())
    }

    /// Reset all plugin statistics.
    pub fn reset_all_plugin_stats(&self) {
        let mut stats = self.plugin_stats.write();
        stats.clear();
    }

    /// Register a plugin for monitoring.
    pub fn register_plugin(&self, plugin_id: &str) {
        let mut stats = self.plugin_stats.write();
        stats.entry(plugin_id.to_string()).or_default();
    }

    /// Unregister a plugin from monitoring.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn unregister_plugin(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        if stats.remove(plugin_id).is_some() {
            Ok(())
        } else {
            Err(WatchdogError::plugin_not_found(plugin_id))
        }
    }

    /// Get the number of registered plugins.
    #[must_use]
    pub fn plugin_count(&self) -> usize {
        self.plugin_stats.read().len()
    }
}
