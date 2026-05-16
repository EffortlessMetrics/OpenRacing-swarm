//! Core watchdog system for monitoring plugins and system components.
//!
//! This module provides the main `WatchdogSystem` struct that coordinates
//! plugin execution monitoring, component health checks, and quarantine management.

mod callbacks;
mod components;
mod config;
mod metrics;
mod plugins;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::FaultType;
use crate::health::{HealthCheck, SystemComponent};
use crate::quarantine::QuarantineManager;
use crate::stats::PluginStats;

pub use config::{WatchdogConfig, WatchdogConfigBuilder};

/// Callback function type for fault notifications.
pub type FaultCallback = Box<dyn Fn(FaultType, &str) + Send + Sync>;

/// Type alias for the list of registered fault callbacks.
type FaultCallbacks = Vec<Arc<dyn Fn(FaultType, &str) + Send + Sync>>;

/// Watchdog system for monitoring plugins and system components.
///
/// This struct provides comprehensive monitoring capabilities:
/// - Plugin execution timing and timeout detection
/// - Component health tracking via heartbeats
/// - Automatic quarantine of misbehaving plugins
/// - Fault notification callbacks
///
/// # Thread Safety
///
/// The `WatchdogSystem` uses internal synchronization via `RwLock` for
/// thread-safe access to statistics and health status.
///
/// # RT Safety
///
/// The following methods are RT-safe (no allocations after initialization):
/// - `record_plugin_execution()`
/// - `heartbeat()`
/// - `is_plugin_quarantined()`
/// - `get_plugin_stats()` (read-only)
pub struct WatchdogSystem {
    config: WatchdogConfig,
    plugin_stats: RwLock<HashMap<String, PluginStats>>,
    health_checks: RwLock<HashMap<SystemComponent, HealthCheck>>,
    quarantine_manager: RwLock<QuarantineManager>,
    last_health_check: RwLock<Instant>,
    quarantine_policy_enabled: RwLock<bool>,
    fault_callbacks: RwLock<FaultCallbacks>,
}

impl WatchdogSystem {
    /// Create a new watchdog system with the given configuration.
    #[must_use]
    pub fn new(config: WatchdogConfig) -> Self {
        let mut health_checks = HashMap::new();
        for component in SystemComponent::all() {
            health_checks.insert(component, HealthCheck::new(component));
        }

        let quarantine_duration = config.plugin_quarantine_duration;

        Self {
            config,
            plugin_stats: RwLock::new(HashMap::new()),
            health_checks: RwLock::new(health_checks),
            quarantine_manager: RwLock::new(QuarantineManager::with_default_duration(
                quarantine_duration,
            )),
            last_health_check: RwLock::new(Instant::now()),
            quarantine_policy_enabled: RwLock::new(true),
            fault_callbacks: RwLock::new(Vec::new()),
        }
    }
}

impl Default for WatchdogSystem {
    fn default() -> Self {
        Self::new(WatchdogConfig::default())
    }
}

impl std::fmt::Debug for WatchdogSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchdogSystem")
            .field("config", &self.config)
            .field("plugin_count", &self.plugin_stats.read().len())
            .field(
                "quarantine_policy_enabled",
                &*self.quarantine_policy_enabled.read(),
            )
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::HealthStatus;

    #[test]
    fn test_plugin_execution_tracking() {
        let watchdog = WatchdogSystem::default();

        let fault = watchdog.record_plugin_execution("test_plugin", 50);
        assert!(fault.is_none());

        let stats = watchdog.get_plugin_stats("test_plugin");
        assert!(stats.is_some());
        if let Some(stats) = stats {
            assert_eq!(stats.total_executions, 1);
            assert_eq!(stats.last_execution_time_us, 50);
            assert_eq!(stats.timeout_count, 0);
        }
    }

    #[test]
    fn test_plugin_timeout_detection() {
        let watchdog = WatchdogSystem::default();

        let fault = watchdog.record_plugin_execution("test_plugin", 150);
        assert!(fault.is_none()); // First timeout, not quarantined yet

        let stats = watchdog.get_plugin_stats("test_plugin");
        assert!(stats.is_some());
        if let Some(stats) = stats {
            assert_eq!(stats.timeout_count, 1);
            assert_eq!(stats.consecutive_timeouts, 1);
        }
    }

    #[test]
    fn test_plugin_quarantine() {
        let watchdog = WatchdogSystem::default();

        for i in 0..5 {
            let fault = watchdog.record_plugin_execution("test_plugin", 150);
            if i == 4 {
                assert_eq!(fault, Some(FaultType::PluginOverrun));
            }
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));

        let quarantined = watchdog.get_quarantined_plugins();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].0, "test_plugin");
    }

    #[test]
    fn test_plugin_quarantine_release() {
        let watchdog = WatchdogSystem::default();

        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));

        let result = watchdog.release_plugin_quarantine("test_plugin");
        assert!(result.is_ok());
        assert!(!watchdog.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_system_component_health() {
        let watchdog = WatchdogSystem::default();

        let health = watchdog.get_component_health(SystemComponent::RtThread);
        assert!(health.is_some());
        if let Some(health) = health {
            assert_eq!(health.status, HealthStatus::Unknown);
        }

        watchdog.heartbeat(SystemComponent::RtThread);
        let health = watchdog.get_component_health(SystemComponent::RtThread);
        assert!(health.is_some());
        if let Some(health) = health {
            assert_eq!(health.status, HealthStatus::Healthy);
        }

        watchdog
            .report_component_failure(SystemComponent::RtThread, Some("Test error".to_string()));
        let health = watchdog.get_component_health(SystemComponent::RtThread);
        assert!(health.is_some());
        if let Some(health) = health {
            assert_eq!(health.consecutive_failures, 1);
            assert_eq!(health.last_error, Some("Test error".to_string()));
        }
    }

    #[test]
    fn test_health_summary() {
        let watchdog = WatchdogSystem::default();

        watchdog.heartbeat(SystemComponent::RtThread);
        watchdog.report_component_failure(SystemComponent::HidCommunication, None);

        let summary = watchdog.get_health_summary();
        assert_eq!(summary[&SystemComponent::RtThread], HealthStatus::Healthy);
        assert_eq!(
            summary[&SystemComponent::HidCommunication],
            HealthStatus::Healthy
        ); // Only 1 failure

        assert!(!watchdog.has_faulted_components());
    }

    #[test]
    fn test_fault_callback() {
        let watchdog = WatchdogSystem::default();

        watchdog.add_fault_callback(|_fault_type, _component| {
            // Callback received
        });

        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_performance_metrics() {
        let watchdog = WatchdogSystem::default();

        watchdog.record_plugin_execution("plugin1", 50);
        watchdog.record_plugin_execution("plugin1", 150);
        watchdog.record_plugin_execution("plugin2", 75);

        let metrics = watchdog.get_plugin_performance_metrics();

        assert_eq!(metrics.len(), 2);
        assert!(metrics.contains_key("plugin1"));
        assert!(metrics.contains_key("plugin2"));

        let plugin1_metrics = &metrics["plugin1"];
        assert!((plugin1_metrics["total_executions"] - 2.0).abs() < f64::EPSILON);
        assert!((plugin1_metrics["timeout_rate_percent"] - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quarantine_policy_toggle() {
        let watchdog = WatchdogSystem::default();

        watchdog.set_quarantine_policy_enabled(false);

        for _ in 0..10 {
            let fault = watchdog.record_plugin_execution("test_plugin", 150);
            assert!(fault.is_none());
        }

        assert!(!watchdog.is_plugin_quarantined("test_plugin"));

        watchdog.set_quarantine_policy_enabled(true);

        for i in 0..5 {
            let fault = watchdog.record_plugin_execution("test_plugin2", 150);
            if i == 4 {
                assert_eq!(fault, Some(FaultType::PluginOverrun));
            }
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin2"));
    }

    #[test]
    fn test_config_builder() {
        let result = WatchdogConfig::builder()
            .plugin_timeout_us(200)
            .plugin_max_timeouts(3)
            .plugin_quarantine_duration(Duration::from_mins(10))
            .rt_thread_timeout_ms(20)
            .build();

        assert!(result.is_ok());
        if let Ok(config) = result {
            assert_eq!(config.plugin_timeout_us, 200);
            assert_eq!(config.plugin_max_timeouts, 3);
            assert_eq!(config.plugin_quarantine_duration, Duration::from_mins(10));
            assert_eq!(config.rt_thread_timeout_ms, 20);
        }
    }

    #[test]
    fn test_config_validation() {
        let config = WatchdogConfig {
            plugin_timeout_us: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = WatchdogConfig {
            plugin_max_timeouts: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_plugin_registration() {
        let watchdog = WatchdogSystem::default();

        assert_eq!(watchdog.plugin_count(), 0);

        watchdog.register_plugin("plugin_a");
        watchdog.register_plugin("plugin_b");
        assert_eq!(watchdog.plugin_count(), 2);

        let result = watchdog.unregister_plugin("plugin_a");
        assert!(result.is_ok());
        assert_eq!(watchdog.plugin_count(), 1);

        let result = watchdog.unregister_plugin("unknown");
        assert!(result.is_err());
    }
}
