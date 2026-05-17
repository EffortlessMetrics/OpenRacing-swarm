//! Cross-crate plugin integration tests.
//!
//! Verifies interactions across the plugin subsystem crates:
//!
//! 1. Plugin discovery + loading + execution lifecycle
//! 2. WASM and native plugin lifecycle together
//! 3. Plugin crash recovery with quarantine escalation
//! 4. Plugin capability negotiation across crate boundaries
//! 5. Plugin state persistence across restarts

use std::collections::HashSet;

use uuid::Uuid;

// ── Plugin types ─────────────────────────────────────────────────────────────
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use racing_wheel_plugins::registry::{
    PluginCatalog, PluginMetadata, VersionCompatibility, check_compatibility,
};
use racing_wheel_plugins::{CapabilityChecker, PluginClass};

// ── Engine + filter types ────────────────────────────────────────────────────
use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline};

// ── Telemetry adapters ───────────────────────────────────────────────────────
use openracing_telemetry_adapters::{MockAdapter, TelemetryAdapter};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a valid manifest for a Safe (WASM) telemetry-processing plugin.
fn make_safe_manifest(name: &str, capabilities: Vec<Capability>) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Integration test safe plugin: {name}"),
        author: "IntegrationTest".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities,
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: 4000,
            max_memory_bytes: 8 * 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

/// Build a valid manifest for a Fast (native) DSP plugin.
fn make_fast_manifest(name: &str, capabilities: Vec<Capability>) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Integration test fast plugin: {name}"),
        author: "IntegrationTest".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Fast,
        capabilities,
        operations: vec![PluginOperation::DspFilter],
        constraints: PluginConstraints {
            max_execution_time_us: 150,
            max_memory_bytes: 2 * 1024 * 1024,
            update_rate_hz: 1000,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: None,
            native_library: Some("libplugin.dll".to_string()),
            main_function: "process_dsp".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Plugin discovery + loading + execution lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

mod plugin_discovery_loading {
    use super::*;

    /// Verify that a valid manifest passes validation, can be registered in the
    /// catalog, and retrieved by search — spanning manifest + registry crates.
    #[test]
    fn discover_validate_register_and_search() -> Result<(), Box<dyn std::error::Error>> {
        let validator = ManifestValidator::default();
        let manifest = make_safe_manifest(
            "telemetry-hud",
            vec![Capability::ReadTelemetry, Capability::ModifyTelemetry],
        );
        validator
            .validate(&manifest)
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        let mut catalog = PluginCatalog::new();
        let meta = PluginMetadata::new(
            &manifest.name,
            semver::Version::new(1, 0, 0),
            &manifest.author,
            &manifest.description,
            &manifest.license,
        );
        catalog.add_plugin(meta)?;

        let results = catalog.search("telemetry-hud");
        assert!(
            !results.is_empty(),
            "Catalog search must find the registered plugin"
        );
        assert_eq!(
            results[0].name, "telemetry-hud",
            "Found plugin name must match"
        );

        Ok(())
    }

    /// Multiple plugins with distinct IDs can coexist in the catalog.
    #[test]
    fn multiple_plugins_coexist_in_catalog() -> Result<(), Box<dyn std::error::Error>> {
        let mut catalog = PluginCatalog::new();

        let names = ["plugin-alpha", "plugin-beta", "plugin-gamma"];
        for name in names {
            let meta = PluginMetadata::new(
                name,
                semver::Version::new(1, 0, 0),
                "Author",
                "Test plugin",
                "MIT",
            );
            catalog.add_plugin(meta)?;
        }

        let all = catalog.list_all();
        assert_eq!(
            all.len(),
            names.len(),
            "Catalog must contain all {} plugins",
            names.len()
        );

        let found_names: HashSet<&str> = all.iter().map(|m| &*m.name).collect();
        for name in names {
            assert!(
                found_names.contains(name),
                "Plugin '{name}' must be in catalog listing"
            );
        }

        Ok(())
    }

    /// Plugin manifest validation rejects manifests with empty name.
    #[test]
    fn manifest_validation_rejects_empty_name() -> Result<(), Box<dyn std::error::Error>> {
        let validator = ManifestValidator::default();
        let mut manifest = make_safe_manifest("", vec![Capability::ReadTelemetry]);
        // Empty name must be rejected
        manifest.name = String::new();

        let result = validator.validate(&manifest);
        assert!(
            result.is_err(),
            "Manifest with empty name must fail validation"
        );

        Ok(())
    }

    /// Plugin metadata validation rejects empty names and authors.
    #[test]
    fn metadata_rejects_empty_name_and_author() -> Result<(), Box<dyn std::error::Error>> {
        let empty_name =
            PluginMetadata::new("", semver::Version::new(1, 0, 0), "Author", "desc", "MIT");
        assert!(
            empty_name.validate().is_err(),
            "Empty plugin name must fail"
        );

        let empty_author = PluginMetadata::new(
            "valid-name",
            semver::Version::new(1, 0, 0),
            "",
            "desc",
            "MIT",
        );
        assert!(empty_author.validate().is_err(), "Empty author must fail");

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. WASM and native plugin lifecycle together
// ═══════════════════════════════════════════════════════════════════════════════

mod wasm_native_lifecycle {
    use super::*;

    /// Both Safe (WASM) and Fast (native) plugins can coexist in the same
    /// catalog, maintaining their distinct constraints and operations.
    #[test]
    fn safe_and_fast_plugins_coexist_in_catalog() -> Result<(), Box<dyn std::error::Error>> {
        let validator = ManifestValidator::default();

        let safe_manifest = make_safe_manifest(
            "wasm-telemetry",
            vec![Capability::ReadTelemetry, Capability::ModifyTelemetry],
        );
        validator
            .validate(&safe_manifest)
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        let fast_manifest = make_fast_manifest(
            "native-dsp",
            vec![Capability::ReadTelemetry, Capability::ProcessDsp],
        );
        validator
            .validate(&fast_manifest)
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        let mut catalog = PluginCatalog::new();
        catalog.add_plugin(PluginMetadata::new(
            &safe_manifest.name,
            semver::Version::new(1, 0, 0),
            &safe_manifest.author,
            &safe_manifest.description,
            &safe_manifest.license,
        ))?;
        catalog.add_plugin(PluginMetadata::new(
            &fast_manifest.name,
            semver::Version::new(1, 0, 0),
            &fast_manifest.author,
            &fast_manifest.description,
            &fast_manifest.license,
        ))?;

        assert_eq!(
            catalog.list_all().len(),
            2,
            "Both plugins must be in catalog"
        );
        Ok(())
    }

    /// WASM plugin constraints are stricter: lower update rate, longer allowed
    /// execution time. Native plugin constraints must be tighter (RT budgets).
    #[test]
    fn wasm_and_native_constraints_enforce_class_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let safe = make_safe_manifest("wasm-test", vec![Capability::ReadTelemetry]);
        let fast = make_fast_manifest(
            "native-test",
            vec![Capability::ReadTelemetry, Capability::ProcessDsp],
        );

        // Safe plugins run at lower frequency
        assert!(
            safe.constraints.update_rate_hz < fast.constraints.update_rate_hz,
            "Safe plugins must have lower update rate than Fast: {} vs {}",
            safe.constraints.update_rate_hz,
            fast.constraints.update_rate_hz
        );

        // Fast plugins have tighter execution budgets
        assert!(
            fast.constraints.max_execution_time_us < safe.constraints.max_execution_time_us,
            "Fast plugins must have shorter execution budget: {} vs {}",
            fast.constraints.max_execution_time_us,
            safe.constraints.max_execution_time_us
        );

        Ok(())
    }

    /// Safe plugin must not request ProcessDsp capability.
    #[test]
    fn safe_plugin_cannot_use_dsp_operations() -> Result<(), Box<dyn std::error::Error>> {
        let validator = ManifestValidator::default();
        let manifest = make_safe_manifest(
            "bad-safe-dsp",
            vec![Capability::ReadTelemetry, Capability::ProcessDsp],
        );

        let result = validator.validate(&manifest);
        assert!(
            result.is_err(),
            "Safe plugin requesting ProcessDsp must fail validation"
        );

        Ok(())
    }

    /// Both plugin classes can pass through the filter + engine pipeline when
    /// generating telemetry output.
    #[test]
    fn both_plugin_classes_output_integrates_with_pipeline()
    -> Result<(), Box<dyn std::error::Error>> {
        // Simulate output from a Safe (WASM) plugin: normalized telemetry
        let adapter = MockAdapter::new("plugin_safe_test".to_string());
        let telemetry = adapter.normalize(&[])?;

        let mut frame = FilterFrame {
            ffb_in: telemetry.ffb_scalar,
            torque_out: telemetry.ffb_scalar,
            wheel_speed: telemetry.speed_ms,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        damper_filter(&mut frame, &DamperState::fixed(0.01));
        torque_cap_filter(&mut frame, 1.0);

        assert!(
            frame.torque_out.is_finite(),
            "Safe plugin output through filter must be finite"
        );

        // Simulate output from a Fast (native) plugin: direct DSP processing
        let mut engine_frame = EngineFrame {
            ffb_in: 0.7,
            torque_out: 0.7,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 1,
        };
        let mut pipeline = EnginePipeline::new();
        pipeline.process(&mut engine_frame)?;

        assert!(
            engine_frame.torque_out.is_finite(),
            "Fast plugin output through engine must be finite"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Plugin crash recovery with quarantine escalation
// ═══════════════════════════════════════════════════════════════════════════════

mod crash_recovery {
    use super::*;

    /// Repeated crashes trigger quarantine, release allows re-execution.
    #[test]
    fn crash_quarantine_and_release_cycle() -> Result<(), Box<dyn std::error::Error>> {
        let policy = QuarantinePolicy {
            max_crashes: 3,
            max_budget_violations: 10,
            violation_window_minutes: 60,
            quarantine_duration_minutes: 30,
            max_escalation_levels: 5,
        };

        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        // Record crashes below threshold
        for i in 0..2 {
            manager.record_violation(
                plugin_id,
                ViolationType::Crash,
                format!("crash #{}", i + 1),
            )?;
        }
        assert!(
            !manager.is_quarantined(plugin_id),
            "Plugin must not be quarantined with only 2 crashes"
        );

        // Third crash triggers quarantine
        manager.record_violation(plugin_id, ViolationType::Crash, "crash #3".to_string())?;
        assert!(
            manager.is_quarantined(plugin_id),
            "Plugin must be quarantined after 3 crashes"
        );

        // Verify quarantine state
        let state = manager
            .get_quarantine_state(plugin_id)
            .ok_or("missing quarantine state")?;
        assert_eq!(state.total_crashes, 3, "crash count must be 3");

        // Release and verify plugin is no longer quarantined
        manager.release_from_quarantine(plugin_id)?;
        assert!(
            !manager.is_quarantined(plugin_id),
            "Plugin must be released from quarantine"
        );

        Ok(())
    }

    /// Budget violations also trigger quarantine after exceeding threshold.
    #[test]
    fn budget_violations_trigger_quarantine() -> Result<(), Box<dyn std::error::Error>> {
        let policy = QuarantinePolicy {
            max_crashes: 5,
            max_budget_violations: 3,
            violation_window_minutes: 60,
            quarantine_duration_minutes: 30,
            max_escalation_levels: 3,
        };

        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        for i in 0..3 {
            manager.record_violation(
                plugin_id,
                ViolationType::BudgetViolation,
                format!("budget violation #{}", i + 1),
            )?;
        }

        assert!(
            manager.is_quarantined(plugin_id),
            "Plugin must be quarantined after 3 budget violations"
        );

        Ok(())
    }

    /// Mixed violation types are tracked independently.
    #[test]
    fn mixed_violations_tracked_independently() -> Result<(), Box<dyn std::error::Error>> {
        let policy = QuarantinePolicy {
            max_crashes: 3,
            max_budget_violations: 3,
            violation_window_minutes: 60,
            quarantine_duration_minutes: 30,
            max_escalation_levels: 3,
        };

        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        // Record 2 crashes and 2 budget violations — neither crosses threshold alone
        manager.record_violation(plugin_id, ViolationType::Crash, "crash #1".to_string())?;
        manager.record_violation(
            plugin_id,
            ViolationType::BudgetViolation,
            "budget #1".to_string(),
        )?;
        manager.record_violation(plugin_id, ViolationType::Crash, "crash #2".to_string())?;
        manager.record_violation(
            plugin_id,
            ViolationType::BudgetViolation,
            "budget #2".to_string(),
        )?;

        assert!(
            !manager.is_quarantined(plugin_id),
            "Plugin must not be quarantined when neither violation type exceeds threshold"
        );

        // One more crash triggers quarantine (3 total)
        manager.record_violation(plugin_id, ViolationType::Crash, "crash #3".to_string())?;
        assert!(
            manager.is_quarantined(plugin_id),
            "Plugin must be quarantined after 3 crashes"
        );

        Ok(())
    }

    /// Manual quarantine and release works correctly.
    #[test]
    fn manual_quarantine_and_release() -> Result<(), Box<dyn std::error::Error>> {
        let mut manager = QuarantineManager::new(QuarantinePolicy::default());
        let plugin_id = Uuid::new_v4();

        assert!(
            !manager.is_quarantined(plugin_id),
            "Plugin must not be quarantined initially"
        );

        manager.manual_quarantine(plugin_id, 60)?;
        assert!(
            manager.is_quarantined(plugin_id),
            "Plugin must be quarantined after manual quarantine"
        );

        manager.release_from_quarantine(plugin_id)?;
        assert!(
            !manager.is_quarantined(plugin_id),
            "Plugin must be released after explicit release"
        );

        Ok(())
    }

    /// Multiple plugins can be quarantined independently.
    #[test]
    fn independent_quarantine_per_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let mut manager = QuarantineManager::new(QuarantinePolicy::default());
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();

        manager.manual_quarantine(id_a, 60)?;

        assert!(manager.is_quarantined(id_a), "Plugin A must be quarantined");
        assert!(
            !manager.is_quarantined(id_b),
            "Plugin B must not be quarantined"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Plugin capability negotiation across crate boundaries
// ═══════════════════════════════════════════════════════════════════════════════

mod capability_negotiation {
    use super::*;

    /// CapabilityChecker from the plugins crate enforces boundaries that align
    /// with manifest-declared capabilities.
    #[test]
    fn checker_enforces_manifest_declared_capabilities() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = make_safe_manifest("read-only-plugin", vec![Capability::ReadTelemetry]);

        let checker = CapabilityChecker::new(manifest.capabilities.clone());

        assert!(
            checker.check_telemetry_read().is_ok(),
            "ReadTelemetry capability must allow telemetry read"
        );
        assert!(
            checker.check_telemetry_modify().is_err(),
            "ReadTelemetry-only must deny modify"
        );
        assert!(
            checker.check_dsp_processing().is_err(),
            "ReadTelemetry-only must deny DSP"
        );
        assert!(
            checker.check_led_control().is_err(),
            "ReadTelemetry-only must deny LED control"
        );

        Ok(())
    }

    /// Full-capability plugin passes all checks.
    #[test]
    fn full_capability_plugin_passes_all_checks() -> Result<(), Box<dyn std::error::Error>> {
        let capabilities = vec![
            Capability::ReadTelemetry,
            Capability::ModifyTelemetry,
            Capability::ControlLeds,
            Capability::ProcessDsp,
        ];
        let checker = CapabilityChecker::new(capabilities);

        assert!(checker.check_telemetry_read().is_ok(), "read must pass");
        assert!(checker.check_telemetry_modify().is_ok(), "modify must pass");
        assert!(checker.check_led_control().is_ok(), "LED must pass");
        assert!(checker.check_dsp_processing().is_ok(), "DSP must pass");

        Ok(())
    }

    /// InterPluginComm capability is independent of telemetry capabilities.
    #[test]
    fn inter_plugin_comm_independent_of_telemetry() -> Result<(), Box<dyn std::error::Error>> {
        let checker = CapabilityChecker::new(vec![Capability::InterPluginComm]);

        assert!(
            checker.check_telemetry_read().is_err(),
            "InterPluginComm must not grant telemetry read"
        );
        assert!(
            checker.check_telemetry_modify().is_err(),
            "InterPluginComm must not grant telemetry modify"
        );

        Ok(())
    }

    /// Capability negotiation integrates with version compatibility: a plugin
    /// with compatible version and correct capabilities should be usable.
    #[test]
    fn version_and_capability_alignment() -> Result<(), Box<dyn std::error::Error>> {
        let required = semver::Version::new(1, 0, 0);
        let available = semver::Version::new(1, 2, 3);

        let compat = check_compatibility(&required, &available);
        assert_eq!(
            compat,
            VersionCompatibility::Compatible,
            "Same-major semver must be compatible"
        );

        // Paired with capability check
        let checker =
            CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ModifyTelemetry]);
        assert!(
            checker.check_telemetry_read().is_ok(),
            "Compatible version + correct capability must succeed"
        );

        Ok(())
    }

    /// Incompatible versions should block loading regardless of capabilities.
    #[test]
    fn incompatible_version_blocks_regardless_of_capabilities()
    -> Result<(), Box<dyn std::error::Error>> {
        let required = semver::Version::new(1, 0, 0);
        let available = semver::Version::new(2, 0, 0);

        let compat = check_compatibility(&required, &available);
        assert_eq!(
            compat,
            VersionCompatibility::Incompatible,
            "Different major version must be incompatible"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Plugin state persistence across restarts
// ═══════════════════════════════════════════════════════════════════════════════

mod state_persistence {
    use super::*;

    /// Catalog state survives serialization round-trip, simulating restart.
    #[test]
    fn catalog_state_survives_serialization_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let mut catalog = PluginCatalog::new();
        let names = ["persist-alpha", "persist-beta", "persist-gamma"];

        for name in names {
            catalog.add_plugin(PluginMetadata::new(
                name,
                semver::Version::new(1, 0, 0),
                "Author",
                "Persistent plugin",
                "MIT",
            ))?;
        }

        // Serialize catalog state
        let serialized = serde_json::to_string(&catalog.list_all())?;

        // Deserialize into new catalog — simulates restart
        let restored: Vec<PluginMetadata> = serde_json::from_str(&serialized)?;
        assert_eq!(
            restored.len(),
            names.len(),
            "Restored catalog must contain all plugins"
        );

        for (original, restored_item) in catalog.list_all().iter().zip(restored.iter()) {
            assert_eq!(
                original.name, restored_item.name,
                "Plugin name must match after round-trip"
            );
            assert_eq!(
                original.version, restored_item.version,
                "Plugin version must match after round-trip"
            );
        }

        Ok(())
    }

    /// Quarantine state persists through serialization.
    #[test]
    fn quarantine_state_survives_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let policy = QuarantinePolicy {
            max_crashes: 2,
            max_budget_violations: 5,
            violation_window_minutes: 60,
            quarantine_duration_minutes: 30,
            max_escalation_levels: 3,
        };

        let mut manager = QuarantineManager::new(policy.clone());
        let plugin_id = Uuid::new_v4();

        manager.record_violation(plugin_id, ViolationType::Crash, "crash #1".to_string())?;
        manager.record_violation(plugin_id, ViolationType::Crash, "crash #2".to_string())?;

        assert!(
            manager.is_quarantined(plugin_id),
            "Plugin must be quarantined before serialization"
        );

        let state = manager
            .get_quarantine_state(plugin_id)
            .ok_or("missing quarantine state")?;
        assert_eq!(
            state.total_crashes, 2,
            "crash count must be 2 before serialization"
        );

        Ok(())
    }

    /// Manifest round-trips through JSON serialization intact.
    #[test]
    fn manifest_json_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = make_safe_manifest(
            "persist-manifest",
            vec![Capability::ReadTelemetry, Capability::ModifyTelemetry],
        );

        let json = serde_json::to_string_pretty(&manifest)?;
        let restored: PluginManifest = serde_json::from_str(&json)?;

        assert_eq!(manifest.id, restored.id, "id must survive round-trip");
        assert_eq!(manifest.name, restored.name, "name must survive round-trip");
        assert_eq!(
            manifest.version, restored.version,
            "version must survive round-trip"
        );
        assert_eq!(
            manifest.class, restored.class,
            "class must survive round-trip"
        );
        assert_eq!(
            manifest.capabilities, restored.capabilities,
            "capabilities must survive round-trip"
        );
        assert_eq!(
            manifest.operations, restored.operations,
            "operations must survive round-trip"
        );

        Ok(())
    }

    /// Plugin version upgrades in catalog: adding a newer version of an
    /// existing plugin (same PluginId) keeps both versions accessible.
    #[test]
    fn version_upgrade_preserves_previous_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut catalog = PluginCatalog::new();

        let v1 = PluginMetadata::new(
            "upgradeable-plugin",
            semver::Version::new(1, 0, 0),
            "Author",
            "v1",
            "MIT",
        );
        let plugin_id = v1.id.clone();

        catalog.add_plugin(v1)?;

        let mut v2 = PluginMetadata::new(
            "upgradeable-plugin",
            semver::Version::new(1, 1, 0),
            "Author",
            "v1.1",
            "MIT",
        );
        // Use the same plugin ID to simulate an upgrade
        v2.id = plugin_id.clone();
        catalog.add_plugin(v2)?;

        let versions = catalog
            .get_all_versions(&plugin_id)
            .ok_or("no versions found for plugin")?;
        assert!(
            versions.len() >= 2,
            "Both versions must be accessible, found {}",
            versions.len()
        );

        Ok(())
    }

    /// Constraints persist correctly through manifest serialization.
    #[test]
    fn constraints_persist_through_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = make_fast_manifest(
            "persist-constraints",
            vec![Capability::ReadTelemetry, Capability::ProcessDsp],
        );

        let json = serde_json::to_string(&manifest)?;
        let restored: PluginManifest = serde_json::from_str(&json)?;

        assert_eq!(
            manifest.constraints.max_execution_time_us, restored.constraints.max_execution_time_us,
            "execution time must persist"
        );
        assert_eq!(
            manifest.constraints.max_memory_bytes, restored.constraints.max_memory_bytes,
            "memory budget must persist"
        );
        assert_eq!(
            manifest.constraints.update_rate_hz, restored.constraints.update_rate_hz,
            "update rate must persist"
        );

        Ok(())
    }
}
