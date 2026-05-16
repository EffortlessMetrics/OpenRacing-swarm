//! Fault callback registration and policy controls.

use crate::FaultType;

use super::WatchdogSystem;

impl WatchdogSystem {
    /// Add a fault callback.
    pub fn add_fault_callback<F>(&self, callback: F)
    where
        F: Fn(FaultType, &str) + Send + Sync + 'static,
    {
        let mut callbacks = self.fault_callbacks.write();
        callbacks.push(std::sync::Arc::new(callback));
    }

    /// Enable or disable quarantine policy.
    pub fn set_quarantine_policy_enabled(&self, enabled: bool) {
        let mut policy = self.quarantine_policy_enabled.write();
        *policy = enabled;
    }

    /// Check if quarantine policy is enabled.
    #[must_use]
    pub fn is_quarantine_policy_enabled(&self) -> bool {
        *self.quarantine_policy_enabled.read()
    }

    /// Get the current configuration.
    #[must_use]
    pub fn get_config(&self) -> &super::WatchdogConfig {
        &self.config
    }
}
