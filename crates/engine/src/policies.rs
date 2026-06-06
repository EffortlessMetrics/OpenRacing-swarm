//! Domain policies for safety and business rules
//!
//! This module contains the core business logic and safety policies that govern
//! the behavior of the racing wheel system. These policies are pure domain logic
//! with no dependencies on infrastructure concerns.

use racing_wheel_schemas::prelude::*;
use std::time::{Duration, Instant};

/// Safety policy for torque management and fault handling
///
/// This policy encapsulates all the business rules around when high torque
/// can be enabled, what the limits are, and how faults should be handled.
pub struct SafetyPolicy {
    /// Maximum torque allowed in safe mode (Nm)
    max_safe_torque: TorqueNm,

    /// Maximum torque allowed in high torque mode (Nm)
    max_high_torque: TorqueNm,

    /// Maximum temperature before thermal protection (°C)
    max_temperature_c: u8,

    /// Maximum hands-off duration before torque reduction (seconds)
    max_hands_off_duration: Duration,

    /// Minimum time between high torque requests (seconds)
    min_high_torque_interval: Duration,

    /// Last high torque request time
    last_high_torque_request: Option<Instant>,
}

impl SafetyPolicy {
    /// Create a new safety policy with default limits
    pub fn new() -> Result<Self, DomainError> {
        Ok(Self {
            max_safe_torque: TorqueNm::new(5.0)?,  // 5 Nm safe limit
            max_high_torque: TorqueNm::new(25.0)?, // 25 Nm high torque limit
            max_temperature_c: 80,                 // 80°C thermal limit
            max_hands_off_duration: Duration::from_secs(5), // 5 second hands-off limit
            min_high_torque_interval: Duration::from_secs(2), // 2 second cooldown
            last_high_torque_request: None,
        })
    }

    /// Create a safety policy with custom limits
    pub fn with_limits(
        max_safe_torque: TorqueNm,
        max_high_torque: TorqueNm,
        max_temperature_c: u8,
        max_hands_off_duration: Duration,
    ) -> Self {
        Self {
            max_safe_torque,
            max_high_torque,
            max_temperature_c,
            max_hands_off_duration,
            min_high_torque_interval: Duration::from_secs(2),
            last_high_torque_request: None,
        }
    }

    /// Check if high torque can be enabled for a device
    ///
    /// This method evaluates all safety conditions and returns whether
    /// high torque mode can be safely enabled.
    pub fn can_enable_high_torque(
        &mut self,
        device: &Device,
        hands_off_duration: Duration,
        temperature_c: u8,
    ) -> Result<(), SafetyViolation> {
        // Check device state
        if !device.is_operational() {
            return Err(SafetyViolation::DeviceNotOperational(device.state));
        }

        // Check for active faults
        if device.has_faults() {
            return Err(SafetyViolation::ActiveFaults(device.fault_flags));
        }

        // Check temperature
        if temperature_c >= self.max_temperature_c {
            return Err(SafetyViolation::TemperatureTooHigh {
                current: temperature_c,
                limit: self.max_temperature_c,
            });
        }

        // Check hands-on requirement
        if hands_off_duration > self.max_hands_off_duration {
            return Err(SafetyViolation::HandsOffTooLong {
                duration: hands_off_duration,
                limit: self.max_hands_off_duration,
            });
        }

        // Check rate limiting
        if let Some(last_request) = self.last_high_torque_request {
            let elapsed = last_request.elapsed();
            if elapsed < self.min_high_torque_interval {
                return Err(SafetyViolation::RateLimited {
                    elapsed,
                    required: self.min_high_torque_interval,
                });
            }
        }

        // Check device capabilities
        if device.capabilities.max_torque < self.max_high_torque {
            return Err(SafetyViolation::DeviceCapabilityInsufficient {
                requested: self.max_high_torque,
                available: device.capabilities.max_torque,
            });
        }

        // Update rate limiting
        self.last_high_torque_request = Some(Instant::now());

        Ok(())
    }

    /// Validate torque limits for a given safety state
    ///
    /// This method checks if a requested torque value is within the allowed
    /// limits for the current safety state.
    pub fn validate_torque_limits(
        &self,
        requested_torque: TorqueNm,
        is_high_torque_enabled: bool,
        device_capabilities: &DeviceCapabilities,
    ) -> Result<TorqueNm, SafetyViolation> {
        // Determine the current torque limit
        let current_limit = if is_high_torque_enabled {
            self.max_high_torque.min(device_capabilities.max_torque)
        } else {
            self.max_safe_torque.min(device_capabilities.max_torque)
        };

        // Check if requested torque exceeds the limit
        if requested_torque > current_limit {
            return Err(SafetyViolation::TorqueExceedsLimit {
                requested: requested_torque,
                limit: current_limit,
                is_high_torque_enabled,
            });
        }

        // Return the validated (and potentially clamped) torque
        Ok(requested_torque.min(current_limit))
    }

    /// Get the current maximum allowed torque
    pub fn get_max_torque(&self, is_high_torque_enabled: bool) -> TorqueNm {
        if is_high_torque_enabled {
            self.max_high_torque
        } else {
            self.max_safe_torque
        }
    }

    /// Check if a fault condition requires immediate torque shutdown
    pub fn requires_immediate_shutdown(&self, fault_flags: u8) -> bool {
        // Define critical fault flags that require immediate shutdown
        const CRITICAL_FAULTS: u8 = 0x01 | 0x02 | 0x04 | 0x08; // USB, encoder, thermal, overcurrent

        (fault_flags & CRITICAL_FAULTS) != 0
    }

    /// Get the maximum safe temperature
    pub fn max_temperature(&self) -> u8 {
        self.max_temperature_c
    }

    /// Get the maximum hands-off duration
    pub fn max_hands_off_duration(&self) -> Duration {
        self.max_hands_off_duration
    }
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        const DEFAULT_SAFE: f32 = 5.0;
        const DEFAULT_HIGH: f32 = 25.0;

        Self {
            // SAFETY: Both constants are finite positive torque values within
            // the `TorqueNm` domain and are covered by safety policy tests.
            max_safe_torque: unsafe { TorqueNm::new_unchecked(DEFAULT_SAFE) },
            // SAFETY: 25.0 Nm is the policy's bounded high-torque ceiling and
            // is within the `TorqueNm` domain.
            max_high_torque: unsafe { TorqueNm::new_unchecked(DEFAULT_HIGH) },
            max_temperature_c: 80,
            max_hands_off_duration: Duration::from_secs(5),
            min_high_torque_interval: Duration::from_secs(2),
            last_high_torque_request: None,
        }
    }
}

/// Safety violation types
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SafetyViolation {
    #[error("Device is not operational: {0:?}")]
    DeviceNotOperational(DeviceState),

    #[error("Device has active faults: 0x{0:02X}")]
    ActiveFaults(u8),

    #[error("Temperature too high: {current}°C (limit: {limit}°C)")]
    TemperatureTooHigh { current: u8, limit: u8 },

    #[error("Hands off too long: {duration:?} (limit: {limit:?})")]
    HandsOffTooLong { duration: Duration, limit: Duration },

    #[error("Rate limited: {elapsed:?} elapsed (required: {required:?})")]
    RateLimited {
        elapsed: Duration,
        required: Duration,
    },

    #[error("Device capability insufficient: requested {requested}, available {available}")]
    DeviceCapabilityInsufficient {
        requested: TorqueNm,
        available: TorqueNm,
    },

    #[error(
        "Torque exceeds limit: requested {requested}, limit {limit} (high torque: {is_high_torque_enabled})"
    )]
    TorqueExceedsLimit {
        requested: TorqueNm,
        limit: TorqueNm,
        is_high_torque_enabled: bool,
    },
}

/// Profile hierarchy resolution policy
///
/// This policy implements the deterministic profile merging logic that
/// resolves the final configuration from multiple profile layers.
pub struct ProfileHierarchyPolicy;

impl ProfileHierarchyPolicy {
    /// Resolve profiles in hierarchical order: Global → Game → Car → Session
    ///
    /// This method implements deterministic profile merging where more specific
    /// profiles override less specific ones. The merge is deterministic and
    /// produces the same result for the same inputs.
    pub fn resolve_profile_hierarchy(
        global_profile: &Profile,
        game_profile: Option<&Profile>,
        car_profile: Option<&Profile>,
        session_overrides: Option<&BaseSettings>,
    ) -> Profile {
        let mut resolved = global_profile.clone();

        // Apply game-specific profile
        if let Some(game_prof) = game_profile {
            resolved = Self::merge_profiles(&resolved, game_prof);
        }

        // Apply car-specific profile
        if let Some(car_prof) = car_profile {
            resolved = Self::merge_profiles(&resolved, car_prof);
        }

        // Apply session overrides to base settings only
        if let Some(session_settings) = session_overrides {
            resolved.base_settings = session_settings.clone();
            // Update metadata to reflect the merge
            resolved.metadata.modified_at = chrono::Utc::now().to_rfc3339();
        }

        resolved
    }

    /// Merge two profiles deterministically
    ///
    /// The `override_profile` takes precedence over `base_profile` for all
    /// non-None values. This ensures deterministic behavior.
    fn merge_profiles(base_profile: &Profile, override_profile: &Profile) -> Profile {
        let mut merged = base_profile.clone();

        // Base settings: override profile takes complete precedence
        merged.base_settings = override_profile.base_settings.clone();

        // LED config: use override if present, otherwise keep base
        if override_profile.led_config.is_some() {
            merged.led_config = override_profile.led_config.clone();
        }

        // Haptics config: use override if present, otherwise keep base
        if override_profile.haptics_config.is_some() {
            merged.haptics_config = override_profile.haptics_config.clone();
        }

        // Update metadata to reflect the merge
        merged.metadata.modified_at = chrono::Utc::now().to_rfc3339();
        merged.metadata.version = Self::increment_version(&merged.metadata.version);

        merged
    }

    /// Find the most specific matching profile for a given context
    ///
    /// This method evaluates profile scopes and returns the most specific
    /// profile that matches the given context.
    pub fn find_most_specific_profile<'a>(
        profiles: &'a [Profile],
        game: Option<&str>,
        car: Option<&str>,
        track: Option<&str>,
    ) -> Option<&'a Profile> {
        let mut best_match: Option<&Profile> = None;
        let mut best_specificity = 0u8;

        for profile in profiles {
            if profile.scope.matches(game, car, track) {
                let specificity = profile.scope.specificity_level();
                if specificity >= best_specificity {
                    best_match = Some(profile);
                    best_specificity = specificity;
                }
            }
        }
        best_match
    }

    /// Validate that a profile hierarchy is consistent
    ///
    /// This method checks that the profile hierarchy makes sense and
    /// doesn't contain conflicting or invalid configurations.
    pub fn validate_profile_hierarchy(
        profiles: &[Profile],
        device_capabilities: &DeviceCapabilities,
    ) -> Result<(), ProfileHierarchyError> {
        // Check for duplicate scopes
        let mut seen_scopes = std::collections::HashSet::new();
        for profile in profiles {
            if !seen_scopes.insert(&profile.scope) {
                return Err(ProfileHierarchyError::DuplicateScope(profile.scope.clone()));
            }
        }

        // Validate each profile against device capabilities
        for profile in profiles {
            if let Err(e) = profile.validate_for_device(device_capabilities) {
                return Err(ProfileHierarchyError::InvalidProfile {
                    profile_id: profile.id.clone(),
                    error: e,
                });
            }
        }

        // Check for circular dependencies (shouldn't happen with current design, but good to check)
        // This is a placeholder for future extension if profile inheritance becomes more complex

        Ok(())
    }

    /// Calculate a deterministic hash for the resolved profile
    ///
    /// This hash can be used to detect when the effective configuration
    /// has changed and needs to be reapplied.
    pub fn calculate_hierarchy_hash(
        global_profile: &Profile,
        game_profile: Option<&Profile>,
        car_profile: Option<&Profile>,
        session_overrides: Option<&BaseSettings>,
    ) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the resolved profile
        let resolved = Self::resolve_profile_hierarchy(
            global_profile,
            game_profile,
            car_profile,
            session_overrides,
        );

        resolved.calculate_hash().hash(&mut hasher);

        hasher.finish()
    }

    /// Increment a semantic version string
    fn increment_version(version: &str) -> String {
        // Simple version increment for merged profiles
        // In a real implementation, this might be more sophisticated
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() >= 3
            && let Ok(patch) = parts[2].parse::<u32>()
        {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }

        // Fallback: append merge indicator
        format!("{}-merged", version)
    }
}

/// Profile hierarchy error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProfileHierarchyError {
    #[error("Duplicate profile scope: {0:?}")]
    DuplicateScope(ProfileScope),

    #[error("Invalid profile {profile_id}: {error}")]
    InvalidProfile {
        profile_id: ProfileId,
        error: DomainError,
    },

    #[error("Circular dependency detected in profile hierarchy")]
    CircularDependency,

    #[error("Profile hierarchy validation failed: {0}")]
    ValidationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    fn create_test_device() -> Result<Device, Box<dyn std::error::Error>> {
        let id = "test-device".parse::<DeviceId>()?;
        let capabilities =
            DeviceCapabilities::new(false, true, true, true, TorqueNm::new(25.0)?, 10000, 1000);

        let mut device = Device::new(
            id,
            "Test Wheel".to_string(),
            DeviceType::WheelBase,
            capabilities,
        );
        device.set_state(DeviceState::Active); // Make sure device is operational
        Ok(device)
    }

    #[test]
    fn test_safety_policy_can_enable_high_torque_success() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut policy = SafetyPolicy::new()?;
        let device = create_test_device()?;

        let result = policy.can_enable_high_torque(
            &device,
            Duration::from_secs(1), // Hands on
            50,                     // Normal temperature
        );

        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_safety_policy_temperature_too_high() -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = SafetyPolicy::new()?;
        let device = create_test_device()?;

        let result = policy.can_enable_high_torque(
            &device,
            Duration::from_secs(1),
            85, // Too hot
        );

        assert!(matches!(
            result,
            Err(SafetyViolation::TemperatureTooHigh { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_safety_policy_hands_off_too_long() -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = SafetyPolicy::new()?;
        let device = create_test_device()?;

        let result = policy.can_enable_high_torque(
            &device,
            Duration::from_secs(10), // Hands off too long
            50,
        );

        assert!(matches!(
            result,
            Err(SafetyViolation::HandsOffTooLong { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_safety_policy_device_faulted() -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = SafetyPolicy::new()?;
        let mut device = create_test_device()?;
        device.set_fault_flags(0x04); // Thermal fault

        let result = policy.can_enable_high_torque(&device, Duration::from_secs(1), 50);
        assert!(matches!(
            result,
            Err(SafetyViolation::DeviceNotOperational(_))
        ));
        Ok(())
    }

    #[test]
    fn test_safety_policy_rate_limiting() -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = SafetyPolicy::new()?;
        let device = create_test_device()?;

        // First request should succeed
        let result1 = policy.can_enable_high_torque(&device, Duration::from_secs(1), 50);
        assert!(result1.is_ok());

        // Immediate second request should be rate limited
        let result2 = policy.can_enable_high_torque(&device, Duration::from_secs(1), 50);
        assert!(matches!(result2, Err(SafetyViolation::RateLimited { .. })));
        Ok(())
    }

    #[test]
    fn test_safety_policy_validate_torque_limits() -> Result<(), Box<dyn std::error::Error>> {
        let policy = SafetyPolicy::new()?;
        let capabilities =
            DeviceCapabilities::new(false, true, true, true, TorqueNm::new(25.0)?, 10000, 1000);

        // Test safe mode limits
        let result = policy.validate_torque_limits(
            TorqueNm::new(3.0)?,
            false, // Safe mode
            &capabilities,
        );
        assert!(result.is_ok());
        assert_eq!(result?.value(), 3.0);

        // Test safe mode exceeds limit
        let result = policy.validate_torque_limits(
            TorqueNm::new(10.0)?,
            false, // Safe mode
            &capabilities,
        );
        assert!(matches!(
            result,
            Err(SafetyViolation::TorqueExceedsLimit { .. })
        ));

        // Test high torque mode
        let result = policy.validate_torque_limits(
            TorqueNm::new(20.0)?,
            true, // High torque mode
            &capabilities,
        );
        assert!(result.is_ok());
        assert_eq!(result?.value(), 20.0);
        Ok(())
    }

    #[test]
    fn test_safety_policy_requires_immediate_shutdown() -> Result<(), Box<dyn std::error::Error>> {
        let policy = SafetyPolicy::new()?;

        // No faults
        assert!(!policy.requires_immediate_shutdown(0x00));

        // Critical faults
        assert!(policy.requires_immediate_shutdown(0x01)); // USB fault
        assert!(policy.requires_immediate_shutdown(0x02)); // Encoder fault
        assert!(policy.requires_immediate_shutdown(0x04)); // Thermal fault
        assert!(policy.requires_immediate_shutdown(0x08)); // Overcurrent fault

        // Non-critical fault
        assert!(!policy.requires_immediate_shutdown(0x10)); // Plugin fault
        Ok(())
    }

    fn create_test_profile(
        id: &str,
        scope: ProfileScope,
    ) -> Result<Profile, Box<dyn std::error::Error>> {
        let profile_id = id.parse::<ProfileId>()?;
        Ok(Profile::new(
            profile_id,
            scope,
            BaseSettings::default(),
            format!("Test Profile {}", id),
        ))
    }

    #[test]
    fn test_profile_hierarchy_resolution() -> Result<(), Box<dyn std::error::Error>> {
        let global_profile = create_test_profile("global", ProfileScope::global())?;
        let game_profile =
            create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()))?;
        let car_profile = create_test_profile(
            "gt3",
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
        )?;

        let resolved = ProfileHierarchyPolicy::resolve_profile_hierarchy(
            &global_profile,
            Some(&game_profile),
            Some(&car_profile),
            None,
        );

        // The resolved profile should have the car profile's settings
        // (since it's the most specific)
        assert_eq!(
            resolved.base_settings.ffb_gain,
            car_profile.base_settings.ffb_gain
        );
        Ok(())
    }

    #[test]
    fn test_profile_hierarchy_find_most_specific() -> Result<(), Box<dyn std::error::Error>> {
        let profiles = vec![
            create_test_profile("global", ProfileScope::global())?,
            create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()))?,
            create_test_profile(
                "gt3",
                ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            )?,
        ];

        // Should find the most specific matching profile
        let result = ProfileHierarchyPolicy::find_most_specific_profile(
            &profiles,
            Some("iracing"),
            Some("gt3"),
            None,
        );

        assert!(
            result.is_some(),
            "Should find a matching profile for iracing/gt3"
        );
        assert_eq!(result.ok_or("expected profile")?.id.as_str(), "gt3");

        // Should find game profile when car doesn't match
        let result = ProfileHierarchyPolicy::find_most_specific_profile(
            &profiles,
            Some("iracing"),
            Some("f1"),
            None,
        );

        assert!(
            result.is_some(),
            "Should find a matching profile for iracing/f1"
        );
        assert_eq!(result.ok_or("expected profile")?.id.as_str(), "iracing");

        // Should find global profile when nothing else matches

        let result =
            ProfileHierarchyPolicy::find_most_specific_profile(&profiles, Some("acc"), None, None);

        assert!(result.is_some(), "Should find global profile for acc");
        assert_eq!(result.ok_or("expected profile")?.id.as_str(), "global");
        Ok(())
    }

    #[test]
    fn test_profile_hierarchy_validation() -> Result<(), Box<dyn std::error::Error>> {
        let capabilities =
            DeviceCapabilities::new(false, true, true, true, TorqueNm::new(25.0)?, 10000, 1000);

        let profiles = vec![
            create_test_profile("global", ProfileScope::global())?,
            create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()))?,
        ];

        let result = ProfileHierarchyPolicy::validate_profile_hierarchy(&profiles, &capabilities);
        assert!(result.is_ok());

        // Test duplicate scopes
        let duplicate_profiles = vec![
            create_test_profile("global1", ProfileScope::global())?,
            create_test_profile("global2", ProfileScope::global())?, // Duplicate scope
        ];

        let result =
            ProfileHierarchyPolicy::validate_profile_hierarchy(&duplicate_profiles, &capabilities);
        assert!(matches!(
            result,
            Err(ProfileHierarchyError::DuplicateScope(_))
        ));
        Ok(())
    }

    #[test]
    fn test_profile_hierarchy_hash_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let global_profile = create_test_profile("global", ProfileScope::global())?;
        let game_profile =
            create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()))?;

        let hash1 = ProfileHierarchyPolicy::calculate_hierarchy_hash(
            &global_profile,
            Some(&game_profile),
            None,
            None,
        );

        let hash2 = ProfileHierarchyPolicy::calculate_hierarchy_hash(
            &global_profile,
            Some(&game_profile),
            None,
            None,
        );

        // Same inputs should produce same hash
        assert_eq!(hash1, hash2);

        // Different inputs should produce different hash
        let mut different_game_profile =
            create_test_profile("acc", ProfileScope::for_game("acc".to_string()))?;
        different_game_profile.base_settings.ffb_gain = Gain::new(0.5)?; // Different gain
        let hash3 = ProfileHierarchyPolicy::calculate_hierarchy_hash(
            &global_profile,
            Some(&different_game_profile),
            None,
            None,
        );

        assert_ne!(hash1, hash3);
        Ok(())
    }
}
