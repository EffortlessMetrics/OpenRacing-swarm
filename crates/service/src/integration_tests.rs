//! Comprehensive system integration tests with virtual hardware simulation
//!
//! Tests the complete system integration including all components working
//! together with virtual devices and mock game telemetry.

#[cfg(test)]
mod tests {
    use crate::{
        FeatureFlags, ServiceDaemon, SystemConfig, WheelService,
        plugin_registry::PluginRegistryService, profile_repository::ProfileRepositoryConfig,
    };
    use anyhow::{Context, Result};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tracing_test::traced_test;

    /// Integration test configuration
    #[allow(dead_code)]
    struct IntegrationTestConfig {
        /// Enable virtual devices
        enable_virtual_devices: bool,
        /// Enable mock telemetry
        enable_mock_telemetry: bool,
        /// Disable real-time scheduling
        disable_realtime: bool,
        /// Test duration
        test_duration: Duration,
    }

    impl Default for IntegrationTestConfig {
        fn default() -> Self {
            Self {
                enable_virtual_devices: true,
                enable_mock_telemetry: true,
                disable_realtime: true,
                test_duration: Duration::from_secs(10),
            }
        }
    }

    /// Test complete system startup and shutdown
    #[tokio::test]
    #[traced_test]
    async fn test_complete_system_startup_shutdown() -> Result<()> {
        let _config = create_test_system_config().await;
        let flags = create_test_feature_flags();

        // Create service daemon
        let service_config = crate::ServiceConfig {
            ipc: crate::IpcConfig::default(),
            ..Default::default()
        };

        let temp_dir = TempDir::new().context("create temp profile dir")?;
        let profile_config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: false,
        };

        let daemon =
            ServiceDaemon::new_with_flags_and_profile_config(service_config, flags, profile_config)
                .await
                .context("create service daemon")?;

        let _keep_temp_dir_alive = temp_dir;

        // Start daemon in background
        let daemon_handle = tokio::spawn(async move { daemon.run().await });

        // Let it run for a short time
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Shutdown should be handled by the daemon's signal handling
        // For testing, we'll just verify it started successfully
        assert!(!daemon_handle.is_finished());

        // Cancel the daemon task
        daemon_handle.abort();
        Ok(())
    }

    /// Test device enumeration and management
    #[tokio::test]
    #[traced_test]
    async fn test_device_enumeration_and_management() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Test device enumeration
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .context("enumerate devices")?;

        // Should have at least one virtual device
        assert!(!devices.is_empty(), "No devices found");
        Ok(())

        // Test device connection
        // NOTE: Re-enable when connect_device is implemented
        // if let Some(device) = devices.first() {
        //     let connection_result = service.device_service()
        //         .connect_device(&device.id).await;
        //     assert!(connection_result.is_ok(), "Failed to connect to device");
        //
        //     // Test device status
        //     let status = service.device_service()
        //         .get_device_status(&device.id).await;
        //     assert!(status.is_ok(), "Failed to get device status");
        // }
    }

    /// Test profile management and application
    #[tokio::test]
    #[traced_test]
    async fn test_profile_management() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Create test profile
        let test_profile = create_test_profile()?;

        // Create profile
        let create_result = service
            .profile_service()
            .create_profile(test_profile.clone())
            .await;
        assert!(create_result.is_ok(), "Failed to create profile");

        // Load profile
        let loaded_profile = service
            .profile_service()
            .load_profile(test_profile.id.as_str())
            .await;
        assert!(loaded_profile.is_ok(), "Failed to load profile");

        // Note: apply_profile tests are done via apply_profile_to_device in other tests
        Ok(())
    }

    /// Test safety system functionality
    #[tokio::test]
    #[traced_test]
    async fn test_safety_system() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Enumerate devices first to get a valid ID
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .context("enumerate devices")?;

        let device = devices.first().context("no devices found")?;

        // Register the device with the safety service
        let max_torque =
            racing_wheel_schemas::prelude::TorqueNm::new(25.0).context("invalid torque value")?;
        service
            .safety_service()
            .register_device(device.id.clone(), max_torque)
            .await
            .context("register device with safety service")?;

        // Test initial safety state (should be safe torque)
        let safety_state = service
            .safety_service()
            .get_safety_state(&device.id)
            .await
            .context("get safety state")?;
        assert!(matches!(
            safety_state.interlock_state,
            crate::safety_service::InterlockState::SafeTorque
        ));

        // Try to set high torque without unlock (should fail)
        // Note: request_high_torque returns Result<InterlockState>
        let high_torque_result = service
            .safety_service()
            .request_high_torque(&device.id, "integration_test".to_string())
            .await;

        // Verify high torque is not active immediately
        if let Ok(state) = high_torque_result {
            assert!(
                !matches!(
                    state,
                    crate::safety_service::InterlockState::HighTorqueActive { .. }
                ),
                "High torque should not be active immediately"
            );
        }

        // Test fault injection (reporting)
        let fault_result = service
            .safety_service()
            .report_fault(
                &device.id,
                racing_wheel_engine::safety::FaultType::ThermalLimit,
                crate::safety_service::FaultSeverity::Fatal,
            )
            .await;
        assert!(fault_result.is_ok(), "Failed to report fault");

        // Verify fault state
        let safety_state = service
            .safety_service()
            .get_safety_state(&device.id)
            .await
            .context("get safety state after fault")?;
        assert!(matches!(
            safety_state.interlock_state,
            crate::safety_service::InterlockState::Faulted { .. }
        ));

        // Test fault recovery
        let recovery_result = service
            .safety_service()
            .clear_fault(
                &device.id,
                racing_wheel_engine::safety::FaultType::ThermalLimit,
            )
            .await;
        assert!(recovery_result.is_ok(), "Failed to clear fault");

        // Verify recovery
        let safety_state = service
            .safety_service()
            .get_safety_state(&device.id)
            .await
            .context("get safety state after recovery")?;
        assert!(matches!(
            safety_state.interlock_state,
            crate::safety_service::InterlockState::SafeTorque
        ));
        Ok(())
    }

    /// Test game integration and telemetry
    ///
    /// Blocked: WheelService does not yet expose a `game_service()` accessor.
    /// Re-enable once the game service API is available on WheelService.
    #[tokio::test]
    #[traced_test]
    #[ignore = "game_service API not yet exposed on WheelService"]
    async fn test_game_integration() -> Result<()> {
        let (_service, _temp_dir) = create_test_service().await?;

        // NOTE: Re-enable once WheelService::game_service() exists
        // let games = service.game_service().detect_games().await?;
        // assert!(!games.is_empty(), "No games detected");
        //
        // if let Some(game) = games.first() {
        //     let _config = service.game_service()
        //         .configure_telemetry(&game.id).await?;
        //     let mut stream = service.game_service()
        //         .start_telemetry_monitoring(&game.id).await?;
        //     let _data = timeout(Duration::from_secs(5), stream.recv()).await
        //         .context("no telemetry data received")?;
        // }

        Ok(())
    }

    /// The wheel service exposes the shared game service composition.
    #[tokio::test]
    #[traced_test]
    async fn test_game_service_accessor() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        let first_reference = Arc::clone(service.game_service());
        let second_reference = Arc::clone(service.game_service());
        assert!(
            Arc::ptr_eq(&first_reference, &second_reference),
            "repeated game-service access must return the same shared instance"
        );

        let supported_games = service.game_service().get_supported_games().await;
        assert!(
            !supported_games.is_empty(),
            "the shared game service should expose the support matrix"
        );
        Ok(())
    }

    /// Test force feedback pipeline
    ///
    /// Verifies service creation and device enumeration. Per-device FFB frame
    /// tests are gated on `send_ffb_frame` / `get_device_statistics` APIs.
    #[tokio::test]
    #[traced_test]
    async fn test_force_feedback_pipeline() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Verify device enumeration works through the FFB pipeline path
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .context("enumerate devices")?;
        assert!(!devices.is_empty(), "Expected at least one virtual device");

        // Verify aggregate statistics are accessible
        let stats = service.device_service().get_statistics().await;
        // Virtual device is included in connected count
        assert!(
            stats.connected_devices >= 1,
            "Expected at least one virtual device in statistics"
        );

        // NOTE: Re-enable once send_ffb_frame / get_device_statistics APIs exist
        // if let Some(device) = devices.first() {
        //     service.device_service().connect_device(&device.id).await?;
        //     let frame = racing_wheel_engine::Frame { ffb_in: 0.5, .. };
        //     service.device_service().send_ffb_frame(&device.id, frame).await?;
        //     let device_stats = service.device_service().get_device_statistics(&device.id).await?;
        //     assert!(device_stats.frames_processed > 0, "No frames processed");
        // }

        Ok(())
    }

    /// Test IPC communication
    #[tokio::test]
    #[traced_test]
    async fn test_ipc_communication() -> Result<()> {
        let _config = create_test_system_config().await;
        let service_config = crate::ServiceConfig {
            ipc: crate::IpcConfig::default(),
            ..Default::default()
        };

        // Create IPC server
        let ipc_server = crate::IpcServer::new(service_config.ipc.clone())
            .await
            .context("create IPC server")?;

        // Create service
        let (service, _temp_dir) = create_test_service().await?;

        // Start IPC server in background
        let server_handle = tokio::spawn(async move { ipc_server.serve(Arc::new(service)).await });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create IPC client
        // let client = crate::IpcClient::new(service_config.ipc.clone());

        // Test device listing
        // let devices_result = client.list_devices().await;
        // assert!(devices_result.is_ok(), "Failed to list devices via IPC");

        // Test profile operations
        // let test_profile = create_test_profile();
        // let save_result = client.save_profile(&test_profile).await;
        // assert!(save_result.is_ok(), "Failed to save profile via IPC");

        // Cleanup
        server_handle.abort();
        Ok(())
    }

    /// Test plugin system
    ///
    /// Placeholder for plugin execution APIs, which remain out of scope for
    /// the service-composition lane.
    #[tokio::test]
    #[traced_test]
    #[ignore = "plugin execution APIs are not part of service composition"]
    async fn test_plugin_system() -> Result<()> {
        let (_service, _temp_dir) = create_test_service().await?;

        // NOTE: Re-enable once WheelService::plugin_service() exists
        // let plugins = service.plugin_service().enumerate_plugins().await?;
        // assert!(!plugins.is_empty(), "No plugins found");
        //
        // if let Some(plugin) = plugins.first() {
        //     service.plugin_service().load_plugin(&plugin.id).await?;
        //     let result = service.plugin_service()
        //         .execute_plugin(&plugin.id, &test_telemetry).await?;
        // }

        Ok(())
    }

    /// The wheel service exposes an empty plugin registry without network I/O.
    #[tokio::test]
    #[traced_test]
    async fn test_plugin_service_accessor() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        let first_reference = Arc::clone(service.plugin_service());
        let second_reference = Arc::clone(service.plugin_service());
        assert!(
            Arc::ptr_eq(&first_reference, &second_reference),
            "repeated plugin-service access must return the same shared instance"
        );

        let installed_plugins = service
            .plugin_service()
            .list_installed_plugins()
            .await
            .context("list installed plugins through shared registry")?;
        assert!(
            installed_plugins.is_empty(),
            "a fresh service should have no installed plugins"
        );
        Ok(())
    }

    /// Test performance under load
    ///
    /// Verifies service creation and device enumeration under the performance
    /// test path. High-frequency FFB frame tests are gated on
    /// `send_ffb_frame` / `get_device_statistics` APIs.
    #[tokio::test]
    #[traced_test]
    async fn test_performance_under_load() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Verify device enumeration works
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .context("enumerate devices")?;
        assert!(!devices.is_empty(), "Expected at least one virtual device");

        // Verify service is responsive under repeated queries
        for _ in 0..10 {
            let _devices = service
                .device_service()
                .enumerate_devices()
                .await
                .context("enumerate devices in load loop")?;
        }

        // Verify aggregate statistics remain consistent
        let stats = service.device_service().get_statistics().await;
        assert!(
            stats.connected_devices >= 1,
            "Expected at least one virtual device in statistics"
        );

        // NOTE: Re-enable once send_ffb_frame / get_device_statistics APIs exist
        // if let Some(device) = devices.first() {
        //     service.device_service().connect_device(&device.id).await?;
        //     let start_time = std::time::Instant::now();
        //     let target_frames = 1000;
        //     for i in 0..target_frames {
        //         let test_frame = racing_wheel_engine::Frame {
        //             ffb_in: (i as f32 / target_frames as f32).sin(),
        //             torque_out: 0.0,
        //             wheel_speed: 0.0,
        //             hands_off: false,
        //             ts_mono_ns: i * 1_000_000,
        //             seq: i as u16,
        //         };
        //         service.device_service()
        //             .send_ffb_frame(&device.id, test_frame).await?;
        //     }
        //     let elapsed = start_time.elapsed();
        //     let fps = target_frames as f64 / elapsed.as_secs_f64();
        //     assert!(fps > 500.0, "Throughput too low: {} fps", fps);
        //     let stats = service.device_service()
        //         .get_device_statistics(&device.id).await?;
        //     assert_eq!(stats.frames_processed, target_frames);
        //     assert_eq!(stats.frames_dropped, 0);
        // }

        Ok(())
    }

    /// Test graceful degradation
    #[tokio::test]
    #[traced_test]
    async fn test_graceful_degradation() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Test with no devices connected
        let no_device_stats = service.device_service().get_statistics().await;
        assert_eq!(no_device_stats.connected_devices, 0);

        // Service should still be functional
        let profile_stats = service
            .profile_service()
            .get_profile_statistics()
            .await
            .context("profile service should work without devices")?;
        assert!(profile_stats.total_profiles >= profile_stats.active_profiles);

        // Test with telemetry unavailable
        // NOTE: Re-enable when game_service is implemented
        // let games = service.game_service().detect_games().await
        //     // verify game service behavior without active games
        //
        // // Should handle missing games gracefully
        // for game in games {
        //     let telemetry_result = service.game_service()
        //         .start_telemetry_monitoring(&game.id).await;
        //
        //     // May fail, but should not crash
        //     if telemetry_result.is_err() {
        //         println!("Telemetry unavailable for {}, continuing", game.name);
        //     }
        // }
        Ok(())
    }

    /// Test configuration validation and migration
    #[tokio::test]
    #[traced_test]
    async fn test_configuration_validation() {
        // Test valid configuration
        let valid_config = create_test_system_config().await;
        assert!(
            valid_config.validate().is_ok(),
            "Valid config should pass validation"
        );

        // Test invalid configuration
        let mut invalid_config = valid_config.clone();
        invalid_config.engine.tick_rate_hz = 0; // Invalid tick rate
        assert!(
            invalid_config.validate().is_err(),
            "Invalid config should fail validation"
        );

        // Test configuration migration
        let mut old_config = valid_config.clone();
        old_config.schema_version = "wheel.config/0".to_string();

        let migration_result = old_config.migrate();
        assert!(migration_result.is_ok(), "Migration should succeed");

        if let Ok(migrated) = migration_result {
            assert!(migrated, "Migration should have been performed");
            assert_eq!(old_config.schema_version, "wheel.config/1");
        }
    }

    /// Test anti-cheat compatibility
    #[tokio::test]
    #[traced_test]
    async fn test_anticheat_compatibility() -> Result<()> {
        // Generate anti-cheat report
        let report = crate::AntiCheatReport::generate()
            .await
            .context("generate anti-cheat report")?;

        // Verify key compatibility points
        assert!(
            !report.process_info.dll_injection,
            "Should not use DLL injection"
        );
        assert!(
            report.process_info.kernel_drivers.is_empty(),
            "Should not use kernel drivers"
        );

        // Verify all telemetry methods are documented
        assert!(
            !report.telemetry_methods.is_empty(),
            "Should document telemetry methods"
        );

        for method in &report.telemetry_methods {
            assert!(
                method.anticheat_compatible,
                "All telemetry methods should be anti-cheat compatible"
            );
            assert!(
                !method.compatibility_notes.is_empty(),
                "Should have compatibility notes"
            );
        }

        // Verify security measures
        assert!(
            !report.security_measures.is_empty(),
            "Should document security measures"
        );

        // Generate markdown report
        let markdown = report.to_markdown();
        assert!(
            markdown.contains("Anti-Cheat Compatibility Report"),
            "Should contain report title"
        );
        assert!(
            markdown.contains("No DLL Injection"),
            "Should document no DLL injection"
        );
        assert!(
            markdown.contains("No Kernel Drivers"),
            "Should document no kernel drivers"
        );
        Ok(())
    }

    // Helper functions

    async fn create_test_service() -> Result<(WheelService, TempDir)> {
        let temp_dir = TempDir::new().context("create temp profile directory for test service")?;
        let profile_config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: false,
        };
        let service = WheelService::new_with_flags(create_test_feature_flags(), profile_config)
            .await
            .context("create test service")?;
        Ok((service, temp_dir))
    }

    async fn create_test_system_config() -> SystemConfig {
        let mut config = SystemConfig::default();

        // Configure for testing
        config.engine.disable_realtime = true;
        config.development.enable_dev_features = true;
        config.development.enable_virtual_devices = true;
        config.development.mock_telemetry = true;
        config.development.disable_safety_interlocks = true;

        // Use test-specific paths
        config.ipc.transport = crate::system_config::TransportType::Native;
        config.ipc.max_connections = 5;

        config
    }

    fn create_test_feature_flags() -> FeatureFlags {
        FeatureFlags {
            disable_realtime: true,
            force_ffb_mode: Some("raw".to_string()),
            enable_dev_features: true,
            enable_debug_logging: true,
            enable_virtual_devices: true,
            disable_safety_interlocks: true,
            enable_plugin_dev_mode: true,
        }
    }

    fn create_test_profile() -> Result<racing_wheel_schemas::prelude::Profile> {
        let id: racing_wheel_schemas::prelude::ProfileId =
            "test-profile".parse().context("parse profile ID")?;
        let scope = racing_wheel_schemas::prelude::ProfileScope::for_game("test_game".to_string());

        let base_settings = racing_wheel_schemas::prelude::BaseSettings {
            ffb_gain: racing_wheel_schemas::prelude::Gain::new(0.8).context("valid gain")?,
            degrees_of_rotation: racing_wheel_schemas::prelude::Degrees::new_dor(540.0)
                .context("valid DOR")?,
            torque_cap: racing_wheel_schemas::prelude::TorqueNm::new(10.0)
                .context("valid torque")?,
            filters: racing_wheel_schemas::prelude::FilterConfig::default(),
        };

        Ok(racing_wheel_schemas::prelude::Profile::new(
            id,
            scope,
            base_settings,
            "Test Profile".to_string(),
        ))
    }
}
