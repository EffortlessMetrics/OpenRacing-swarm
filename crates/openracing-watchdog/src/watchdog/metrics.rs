//! Derived plugin performance metric views.

use std::collections::HashMap;

use super::WatchdogSystem;

impl WatchdogSystem {
    /// Get plugin performance metrics.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn get_plugin_performance_metrics(&self) -> HashMap<String, HashMap<String, f64>> {
        let stats = self.plugin_stats.read();
        stats
            .iter()
            .map(|(plugin_id, plugin_stats)| {
                let mut metrics = HashMap::new();
                metrics.insert(
                    "total_executions".to_string(),
                    #[allow(clippy::cast_precision_loss)]
                    {
                        plugin_stats.total_executions as f64
                    },
                );
                metrics.insert(
                    "average_execution_time_us".to_string(),
                    plugin_stats.average_execution_time_us(),
                );
                metrics.insert(
                    "timeout_rate_percent".to_string(),
                    plugin_stats.timeout_rate(),
                );
                metrics.insert(
                    "quarantine_count".to_string(),
                    f64::from(plugin_stats.quarantine_count),
                );
                metrics.insert(
                    "consecutive_timeouts".to_string(),
                    f64::from(plugin_stats.consecutive_timeouts),
                );

                if let Some(remaining) = plugin_stats.quarantine_remaining() {
                    #[allow(clippy::cast_precision_loss)]
                    let remaining_ms = remaining.as_millis() as f64;
                    metrics.insert("quarantine_remaining_ms".to_string(), remaining_ms);
                }

                (plugin_id.clone(), metrics)
            })
            .collect()
    }
}
