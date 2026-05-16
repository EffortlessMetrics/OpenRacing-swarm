//! System component heartbeat and health-check operations.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::FaultType;
use crate::health::{HealthCheck, HealthStatus, SystemComponent};

use super::WatchdogSystem;

impl WatchdogSystem {
    /// Record system component heartbeat.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe.
    pub fn heartbeat(&self, component: SystemComponent) {
        let mut checks = self.health_checks.write();
        if let Some(health_check) = checks.get_mut(&component) {
            health_check.heartbeat();
        }
    }

    /// Report system component failure.
    pub fn report_component_failure(&self, component: SystemComponent, error: Option<String>) {
        let status = {
            let mut checks = self.health_checks.write();
            if let Some(health_check) = checks.get_mut(&component) {
                health_check.report_failure(error);
                health_check.status
            } else {
                return;
            }
        };

        if status == HealthStatus::Faulted {
            let fault_type = match component {
                SystemComponent::RtThread | SystemComponent::TelemetryAdapter => {
                    FaultType::TimingViolation
                }
                SystemComponent::HidCommunication | SystemComponent::DeviceManager => {
                    FaultType::UsbStall
                }
                SystemComponent::PluginHost => FaultType::PluginOverrun,
                SystemComponent::SafetySystem => FaultType::SafetyInterlockViolation,
            };

            let callbacks = self.fault_callbacks.read();
            for callback in callbacks.iter() {
                callback(fault_type, &format!("{component:?}"));
            }
        }
    }

    /// Add metric to a system component.
    pub fn add_component_metric(&self, component: SystemComponent, name: String, value: f64) {
        let mut checks = self.health_checks.write();
        if let Some(health_check) = checks.get_mut(&component) {
            health_check.add_metric(name, value);
        }
    }

    /// Get component health status.
    #[must_use]
    pub fn get_component_health(&self, component: SystemComponent) -> Option<HealthCheck> {
        let checks = self.health_checks.read();
        checks.get(&component).cloned()
    }

    /// Get all component health statuses.
    #[must_use]
    pub fn get_all_component_health(&self) -> HashMap<SystemComponent, HealthCheck> {
        let checks = self.health_checks.read();
        checks.clone()
    }

    /// Perform periodic health checks.
    ///
    /// Returns a list of detected faults.
    pub fn perform_health_checks(&self) -> Vec<FaultType> {
        let now = Instant::now();
        {
            let last = self.last_health_check.read();
            if now.duration_since(*last) < self.config.health_check_interval {
                return Vec::new();
            }
        }

        {
            let mut last = self.last_health_check.write();
            *last = now;
        }

        let mut faults = Vec::new();

        {
            let mut checks = self.health_checks.write();

            if let Some(health_check) = checks.get_mut(&SystemComponent::RtThread)
                && health_check
                    .check_timeout(Duration::from_millis(self.config.rt_thread_timeout_ms))
            {
                faults.push(FaultType::TimingViolation);
            }

            if let Some(health_check) = checks.get_mut(&SystemComponent::HidCommunication)
                && health_check.check_timeout(Duration::from_millis(self.config.hid_timeout_ms))
            {
                faults.push(FaultType::UsbStall);
            }

            if let Some(health_check) = checks.get_mut(&SystemComponent::TelemetryAdapter) {
                health_check.check_timeout(Duration::from_millis(self.config.telemetry_timeout_ms));
            }
        }

        {
            let mut stats = self.plugin_stats.write();
            let mut manager = self.quarantine_manager.write();
            manager.cleanup_expired_with_stats(&mut stats);
        }

        faults
    }

    /// Get system health summary.
    #[must_use]
    pub fn get_health_summary(&self) -> HashMap<SystemComponent, HealthStatus> {
        let checks = self.health_checks.read();
        checks
            .iter()
            .map(|(component, health_check)| (*component, health_check.status))
            .collect()
    }

    /// Check if any component is faulted.
    #[must_use]
    pub fn has_faulted_components(&self) -> bool {
        let checks = self.health_checks.read();
        checks
            .values()
            .any(|health_check| health_check.status == HealthStatus::Faulted)
    }

    /// Check if any components are registered.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    #[must_use]
    pub fn has_registered_components(&self) -> bool {
        let checks = self.health_checks.read();
        !checks.is_empty()
    }

    /// Get component uptime (time since last heartbeat).
    #[must_use]
    pub fn get_component_uptime(&self, component: SystemComponent) -> Option<Duration> {
        let checks = self.health_checks.read();
        checks
            .get(&component)
            .and_then(|health_check| health_check.last_heartbeat)
            .map(|last_heartbeat| last_heartbeat.elapsed())
    }
}
