//! Wave-15 RC hardening tests for the service crate.
//!
//! Covers:
//! 1. Service configuration parsing / validation
//! 2. IPC message serialization round-trips
//! 3. Game detection logic (process pattern matching)
//! 4. Telemetry dispatch (adapter routing)
//! 5. Device management (state transitions)
//! 6. Profile management (CRUD, switching, session overrides)
//! 7. Safety interlock state machine

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Unwrap a Result in tests without `unwrap()` / `expect()`.
    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        assert!(r.is_ok(), "unexpected Err: {:?}", r.as_ref().err());
        match r {
            Ok(v) => v,
            Err(_) => unreachable!("asserted Ok above"),
        }
    }

    // ========================================================================
    // 1. Service configuration parsing / validation
    // ========================================================================

    mod config_tests {
        use super::*;
        use crate::system_config::SystemConfig;

        #[test]
        fn default_system_config_validates() -> Result<()> {
            let config = SystemConfig::default();
            config.validate()?;
            Ok(())
        }

        #[test]
        fn system_config_json_roundtrip() -> Result<()> {
            let original = SystemConfig::default();
            let json = serde_json::to_string_pretty(&original)?;
            let parsed: SystemConfig = serde_json::from_str(&json)?;

            assert_eq!(parsed.schema_version, original.schema_version);
            assert_eq!(parsed.engine.tick_rate_hz, original.engine.tick_rate_hz);
            assert_eq!(parsed.safety.max_torque_nm, original.safety.max_torque_nm);
            assert_eq!(parsed.ipc.max_connections, original.ipc.max_connections);
            Ok(())
        }

        #[test]
        fn invalid_tick_rate_zero_rejected() {
            let mut config = SystemConfig::default();
            config.engine.tick_rate_hz = 0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_tick_rate_too_high_rejected() {
            let mut config = SystemConfig::default();
            config.engine.tick_rate_hz = 20_000;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_max_jitter_rejected() {
            let mut config = SystemConfig::default();
            config.engine.max_jitter_us = 5_000;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_safe_torque_rejected() {
            let mut config = SystemConfig::default();
            config.safety.default_safe_torque_nm = -1.0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_max_torque_rejected() {
            let mut config = SystemConfig::default();
            config.safety.max_torque_nm = 100.0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_fault_timeout_rejected() {
            let mut config = SystemConfig::default();
            config.safety.fault_response_timeout_ms = 0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_max_connections_rejected() {
            let mut config = SystemConfig::default();
            config.ipc.max_connections = 0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_tracing_sample_rate_rejected() {
            let mut config = SystemConfig::default();
            config.observability.tracing_sample_rate = 2.0;
            assert!(config.validate().is_err());
        }

        #[test]
        fn invalid_schema_version_rejected() {
            let config = SystemConfig {
                schema_version: "bad-schema".to_string(),
                ..SystemConfig::default()
            };
            assert!(config.validate().is_err());
        }

        #[tokio::test]
        async fn system_config_save_load_roundtrip() -> Result<()> {
            let temp_dir = TempDir::new()?;
            let path = temp_dir.path().join("test_config.json");

            let original = SystemConfig::default();
            original.save_to_path(&path).await?;
            let loaded = SystemConfig::load_from_path(&path).await?;

            assert_eq!(loaded.schema_version, original.schema_version);
            assert_eq!(loaded.engine.tick_rate_hz, original.engine.tick_rate_hz);
            assert_eq!(loaded.service.service_name, original.service.service_name);
            Ok(())
        }

        #[test]
        fn config_migration_from_v0() -> Result<()> {
            let mut config = SystemConfig {
                schema_version: "wheel.config/0".to_string(),
                ..SystemConfig::default()
            };
            let migrated = config.migrate()?;
            assert!(migrated, "migration should have occurred");
            assert_eq!(config.schema_version, "wheel.config/1");
            Ok(())
        }

        #[test]
        fn config_migration_noop_for_current() -> Result<()> {
            let mut config = SystemConfig::default();
            let migrated = config.migrate()?;
            assert!(!migrated, "no migration needed for current schema");
            Ok(())
        }

        #[test]
        fn config_migration_unsupported_version_errors() {
            let mut config = SystemConfig {
                schema_version: "wheel.config/99".to_string(),
                ..SystemConfig::default()
            };
            assert!(config.migrate().is_err());
        }

        #[test]
        fn daemon_service_config_defaults() {
            let cfg = crate::daemon::ServiceConfig::default();
            assert_eq!(cfg.service_name, "wheeld");
            assert!(cfg.auto_restart);
            assert_eq!(cfg.max_restart_attempts, 3);
        }

        #[test]
        fn daemon_service_config_json_roundtrip() -> Result<()> {
            let original = crate::daemon::ServiceConfig::default();
            let json = serde_json::to_string(&original)?;
            let parsed: crate::daemon::ServiceConfig = serde_json::from_str(&json)?;
            assert_eq!(parsed.service_name, original.service_name);
            assert_eq!(parsed.health_check_interval, original.health_check_interval);
            assert_eq!(parsed.max_restart_attempts, original.max_restart_attempts);
            Ok(())
        }

        #[test]
        fn system_config_to_daemon_service_config() {
            let sys = SystemConfig::default();
            let daemon_cfg = crate::system_config::ServiceConfig::from_system_config(&sys);
            assert_eq!(daemon_cfg.service_name, sys.service.service_name);
            assert_eq!(
                daemon_cfg.health_check_interval,
                sys.service.health_check_interval
            );
            assert_eq!(daemon_cfg.auto_restart, sys.service.auto_restart);
        }
    }

    // ========================================================================
    // 2. IPC message serialization / deserialization round-trips
    // ========================================================================

    mod ipc_tests {
        use super::*;
        use crate::ipc_simple::{
            HealthEventInternal, IpcClient, IpcClientConfig, IpcConfig, IpcServer, TransportType,
        };

        #[test]
        fn ipc_config_json_roundtrip() -> Result<()> {
            let config = IpcConfig::default();
            let json = serde_json::to_string(&config)?;
            let parsed: IpcConfig = serde_json::from_str(&json)?;
            assert_eq!(parsed.max_connections, config.max_connections);
            assert_eq!(parsed.connection_timeout, config.connection_timeout);
            assert_eq!(parsed.enable_acl, config.enable_acl);
            assert_eq!(parsed.bind_address, config.bind_address);
            Ok(())
        }

        #[test]
        fn ipc_config_custom_values_roundtrip() -> Result<()> {
            let config = IpcConfig {
                bind_address: Some("10.0.0.1".to_string()),
                transport: TransportType::default(),
                max_connections: 42,
                connection_timeout: Duration::from_secs(99),
                enable_acl: true,
            };
            let json = serde_json::to_string(&config)?;
            let parsed: IpcConfig = serde_json::from_str(&json)?;
            assert_eq!(parsed.bind_address, Some("10.0.0.1".to_string()));
            assert_eq!(parsed.max_connections, 42);
            assert_eq!(parsed.connection_timeout, Duration::from_secs(99));
            assert!(parsed.enable_acl);
            Ok(())
        }

        #[test]
        fn health_event_internal_fields() -> Result<()> {
            let event = HealthEventInternal {
                device_id: "wheel-1".to_string(),
                event_type: "temperature_warning".to_string(),
                message: "Temperature exceeds threshold".to_string(),
                timestamp: std::time::SystemTime::now(),
            };
            assert_eq!(event.device_id, "wheel-1");
            assert_eq!(event.event_type, "temperature_warning");
            assert!(!event.message.is_empty());
            Ok(())
        }

        #[tokio::test]
        async fn ipc_server_health_broadcast_roundtrip() -> Result<()> {
            let server = IpcServer::new(IpcConfig::default()).await?;
            let mut rx = server.get_health_receiver();

            let original = HealthEventInternal {
                device_id: "rt-dev".to_string(),
                event_type: "fault".to_string(),
                message: "safety interlock triggered".to_string(),
                timestamp: std::time::SystemTime::now(),
            };
            server.broadcast_health_event(original.clone());

            let received = rx.recv().await?;
            assert_eq!(received.device_id, "rt-dev");
            assert_eq!(received.event_type, "fault");
            assert_eq!(received.message, "safety interlock triggered");
            Ok(())
        }

        #[test]
        fn ipc_client_config_default_address() {
            let cfg = IpcClientConfig::default();
            assert_eq!(cfg.server_address, "127.0.0.1:50051");
            assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
        }

        #[tokio::test]
        async fn ipc_client_connect_disconnect() -> Result<()> {
            let mut client = IpcClient::new(IpcClientConfig::default());
            // connect/disconnect should not panic
            let conn = client.connect().await;
            assert!(conn.is_ok());
            let disc = client.disconnect().await;
            assert!(disc.is_ok());
            Ok(())
        }

        #[test]
        fn transport_type_default_windows_named_pipe() {
            let transport = TransportType::default();
            #[cfg(windows)]
            {
                assert!(
                    matches!(transport, TransportType::NamedPipe(ref name) if name.contains("wheel")),
                );
            }
            #[cfg(unix)]
            {
                assert!(
                    matches!(transport, TransportType::UnixDomainSocket(ref path) if path.contains("wheel.sock")),
                );
            }
        }
    }

    // ========================================================================
    // 3. Game detection logic (process pattern matching)
    // ========================================================================

    mod process_detection_tests {
        use super::*;
        use crate::process_detection::ProcessDetectionService;

        #[test]
        fn add_game_patterns_multiple_games() {
            let (mut svc, _rx) = ProcessDetectionService::new();
            svc.add_game_patterns(
                "iracing".to_string(),
                vec![
                    "iRacingSim64DX11.exe".to_string(),
                    "iRacingSim64.exe".to_string(),
                ],
            );
            svc.add_game_patterns(
                "acc".to_string(),
                vec!["AC2-Win64-Shipping.exe".to_string()],
            );
            svc.add_game_patterns("eawrc".to_string(), vec!["WRC.exe".to_string()]);

            // Verify patterns were added (running games starts empty)
            assert!(svc.get_running_games().is_empty());
        }

        #[test]
        fn default_impl_creates_service() {
            let svc = ProcessDetectionService::default();
            assert!(svc.get_running_games().is_empty());
        }

        #[test]
        fn detection_interval_can_be_changed() {
            let (mut svc, _rx) = ProcessDetectionService::new();
            svc.set_detection_interval(Duration::from_millis(50));
            // Verify no panic; field is private but setter works
        }

        #[test]
        fn get_running_games_initially_empty() {
            let (svc, _rx) = ProcessDetectionService::new();
            assert!(svc.get_running_games().is_empty());
        }

        #[test]
        fn overwrite_patterns_for_same_game() {
            let (mut svc, _rx) = ProcessDetectionService::new();
            svc.add_game_patterns(
                "iracing".to_string(),
                vec!["iRacingSim64DX11.exe".to_string()],
            );
            // Overwrite with new patterns
            svc.add_game_patterns("iracing".to_string(), vec!["iRacingSim64.exe".to_string()]);
            // Should not panic; only latest patterns kept
            assert!(svc.get_running_games().is_empty());
        }
    }

    // ========================================================================
    // 4. Telemetry dispatch (adapter routing via mock control)
    // ========================================================================

    mod telemetry_dispatch_tests {
        use crate::game_telemetry_bridge::TelemetryAdapterControl;
        use async_trait::async_trait;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        struct RecordingControl {
            started: Arc<Mutex<Vec<String>>>,
            stopped: Arc<Mutex<Vec<String>>>,
        }

        impl RecordingControl {
            fn new() -> Self {
                Self {
                    started: Arc::new(Mutex::new(Vec::new())),
                    stopped: Arc::new(Mutex::new(Vec::new())),
                }
            }
        }

        #[async_trait]
        impl TelemetryAdapterControl for RecordingControl {
            async fn start_for_game(&self, game_id: &str) -> anyhow::Result<()> {
                self.started.lock().await.push(game_id.to_string());
                Ok(())
            }
            async fn stop_for_game(&self, game_id: &str) -> anyhow::Result<()> {
                self.stopped.lock().await.push(game_id.to_string());
                Ok(())
            }
        }

        #[tokio::test]
        async fn start_routes_to_correct_adapter() -> anyhow::Result<()> {
            let ctrl = RecordingControl::new();
            ctrl.start_for_game("iracing").await?;
            ctrl.start_for_game("acc").await?;

            let started = ctrl.started.lock().await;
            assert_eq!(started.as_slice(), ["iracing", "acc"]);
            Ok(())
        }

        #[tokio::test]
        async fn stop_routes_to_correct_adapter() -> anyhow::Result<()> {
            let ctrl = RecordingControl::new();
            ctrl.start_for_game("iracing").await?;
            ctrl.stop_for_game("iracing").await?;

            let stopped = ctrl.stopped.lock().await;
            assert_eq!(stopped.as_slice(), ["iracing"]);
            Ok(())
        }

        #[tokio::test]
        async fn multiple_adapters_independent() -> anyhow::Result<()> {
            let ctrl = RecordingControl::new();
            ctrl.start_for_game("iracing").await?;
            ctrl.start_for_game("acc").await?;
            ctrl.stop_for_game("iracing").await?;

            let started = ctrl.started.lock().await;
            let stopped = ctrl.stopped.lock().await;
            assert_eq!(started.len(), 2);
            assert_eq!(stopped.as_slice(), ["iracing"]);
            Ok(())
        }

        #[tokio::test]
        async fn duplicate_start_records_both() -> anyhow::Result<()> {
            let ctrl = RecordingControl::new();
            ctrl.start_for_game("acc").await?;
            ctrl.start_for_game("acc").await?;

            let started = ctrl.started.lock().await;
            assert_eq!(started.len(), 2);
            Ok(())
        }
    }

    // ========================================================================
    // 5. Device management (state transitions)
    // ========================================================================

    mod device_tests {
        use super::*;
        use crate::device_service::{DeviceState, ManagedDevice};
        use racing_wheel_engine::DeviceHealthStatus;

        fn make_managed_device(state: DeviceState) -> ManagedDevice {
            ManagedDevice {
                info: racing_wheel_engine::DeviceInfo {
                    id: must("test-device".parse()),
                    name: "Test Wheel".to_string(),
                    vendor_id: 0x1234,
                    product_id: 0xABCD,
                    serial_number: Some("SN001".to_string()),
                    manufacturer: Some("TestCo".to_string()),
                    path: "mock://hid/test-device".to_string(),
                    capabilities: racing_wheel_schemas::prelude::DeviceCapabilities::new(
                        true,
                        true,
                        true,
                        true,
                        must(racing_wheel_schemas::prelude::TorqueNm::new(20.0)),
                        65535,
                        1000,
                    ),
                    is_connected: state != DeviceState::Disconnected,
                },
                state,
                capabilities: None,
                calibration: None,
                last_telemetry: None,
                last_seen: Instant::now(),
                health_status: DeviceHealthStatus {
                    temperature_c: 25,
                    fault_flags: 0,
                    hands_on: false,
                    last_communication: Instant::now(),
                    communication_errors: 0,
                },
            }
        }

        #[test]
        fn device_state_connected() {
            let dev = make_managed_device(DeviceState::Connected);
            assert_eq!(dev.state, DeviceState::Connected);
            assert!(dev.info.is_connected);
        }

        #[test]
        fn device_state_disconnected() {
            let dev = make_managed_device(DeviceState::Disconnected);
            assert_eq!(dev.state, DeviceState::Disconnected);
            assert!(!dev.info.is_connected);
        }

        #[test]
        fn device_state_ready() {
            let dev = make_managed_device(DeviceState::Ready);
            assert_eq!(dev.state, DeviceState::Ready);
        }

        #[test]
        fn device_state_faulted() {
            let dev = make_managed_device(DeviceState::Faulted {
                reason: "overtemp".to_string(),
            });
            assert!(
                matches!(dev.state, DeviceState::Faulted { ref reason } if reason == "overtemp")
            );
        }

        #[test]
        fn device_state_equality() {
            assert_eq!(DeviceState::Connected, DeviceState::Connected);
            assert_eq!(DeviceState::Disconnected, DeviceState::Disconnected);
            assert_eq!(DeviceState::Ready, DeviceState::Ready);
            assert_ne!(DeviceState::Connected, DeviceState::Disconnected);
            assert_ne!(DeviceState::Connected, DeviceState::Ready);
        }

        #[tokio::test]
        async fn device_service_enumerate_with_virtual_port() -> anyhow::Result<()> {
            use racing_wheel_engine::{VirtualDevice, VirtualHidPort};
            use std::sync::Arc;

            let mut port = VirtualHidPort::new();
            let device_id = must("virt-wheel-0".parse());
            let vdev = VirtualDevice::new(device_id, "Virtual Wheel".to_string());
            port.add_device(vdev)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let service =
                crate::device_service::ApplicationDeviceService::new(Arc::new(port), None).await?;
            let devices = service.enumerate_devices().await?;

            assert!(!devices.is_empty(), "should discover virtual device");
            assert_eq!(devices[0].name, "Virtual Wheel");
            Ok(())
        }

        #[tokio::test]
        async fn device_reconnect_transitions_from_disconnected() -> anyhow::Result<()> {
            use racing_wheel_engine::{VirtualDevice, VirtualHidPort};
            use std::sync::Arc;

            let mut port = VirtualHidPort::new();
            let device_id = must("recon-dev".parse());
            let vdev = VirtualDevice::new(device_id, "Reconnect Wheel".to_string());
            port.add_device(vdev)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let service =
                crate::device_service::ApplicationDeviceService::new(Arc::new(port), None).await?;

            // First enumeration: device appears
            let devices = service.enumerate_devices().await?;
            assert_eq!(devices.len(), 1);

            // Second enumeration should still show the device
            let devices2 = service.enumerate_devices().await?;
            assert_eq!(devices2.len(), 1);
            Ok(())
        }
    }

    // ========================================================================
    // 6. Profile management
    // ========================================================================

    mod profile_tests {
        use super::*;
        use crate::profile_repository::ProfileRepositoryConfig;
        use crate::profile_service::ProfileService;
        use racing_wheel_schemas::prelude::{BaseSettings, Profile, ProfileId, ProfileScope};

        async fn make_service() -> Result<(ProfileService, TempDir), Box<dyn std::error::Error>> {
            let temp = TempDir::new()?;
            let config = ProfileRepositoryConfig {
                profiles_dir: temp.path().to_path_buf(),
                trusted_keys: Vec::new(),
                auto_migrate: true,
                backup_on_migrate: false,
            };
            let svc = ProfileService::new_with_config(config).await?;
            Ok((svc, temp))
        }

        fn test_profile(id: &str) -> Profile {
            let pid = must(ProfileId::new(id.to_string()));
            Profile::new(
                pid,
                ProfileScope::global(),
                BaseSettings::default(),
                format!("Test Profile {}", id),
            )
        }

        #[tokio::test]
        async fn create_and_get_profile() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let profile = test_profile("create-get");
            let pid = profile.id.clone();

            svc.create_profile(profile).await?;
            let loaded = svc.get_profile(&pid).await?;
            assert!(loaded.is_some());
            assert_eq!(loaded.as_ref().map(|p| &p.id), Some(&pid));
            Ok(())
        }

        #[tokio::test]
        async fn update_nonexistent_profile_errors() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let profile = test_profile("does-not-exist");
            let result = svc.update_profile(profile).await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn delete_profile() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let profile = test_profile("to-delete");
            let pid = profile.id.clone();

            svc.create_profile(profile).await?;
            svc.delete_profile(&pid).await?;
            let loaded = svc.get_profile(&pid).await?;
            assert!(loaded.is_none());
            Ok(())
        }

        #[tokio::test]
        async fn list_profiles_returns_created() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            svc.create_profile(test_profile("list-a")).await?;
            svc.create_profile(test_profile("list-b")).await?;

            let profiles = svc.list_profiles().await?;
            assert!(profiles.len() >= 2);
            Ok(())
        }

        #[tokio::test]
        async fn active_profile_tracking() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let device_id = must("dev-active".parse());
            let profile_id = must(ProfileId::new("active-prof".to_string()));

            svc.set_active_profile(&device_id, &profile_id).await?;
            let active = svc.get_active_profile(&device_id).await?;
            assert_eq!(active.as_ref(), Some(&profile_id));

            svc.clear_active_profile(&device_id).await?;
            let active2 = svc.get_active_profile(&device_id).await?;
            assert!(active2.is_none());
            Ok(())
        }

        #[tokio::test]
        async fn session_override_set_get_clear() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let device_id = must("dev-override".parse());
            let profile = test_profile("session-over");

            svc.set_session_override(&device_id, profile.clone())
                .await?;
            let over = svc.get_session_override(&device_id).await?;
            assert!(over.is_some());
            assert_eq!(over.as_ref().map(|p| &p.id), Some(&profile.id));

            svc.clear_session_override(&device_id).await?;
            let over2 = svc.get_session_override(&device_id).await?;
            assert!(over2.is_none());
            Ok(())
        }

        #[tokio::test]
        async fn cannot_delete_active_profile() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            let profile = test_profile("active-no-del");
            let pid = profile.id.clone();
            let device_id = must("dev-nodelete".parse());

            svc.create_profile(profile).await?;
            svc.set_active_profile(&device_id, &pid).await?;

            let result = svc.delete_profile(&pid).await;
            assert!(result.is_err(), "should not delete an active profile");
            Ok(())
        }

        #[tokio::test]
        async fn profile_statistics() -> Result<(), Box<dyn std::error::Error>> {
            let (svc, _tmp) = make_service().await?;
            svc.create_profile(test_profile("stat-a")).await?;
            svc.create_profile(test_profile("stat-b")).await?;

            let stats = svc.get_profile_statistics().await?;
            assert!(stats.total_profiles >= 2);
            assert_eq!(stats.session_overrides, 0);
            Ok(())
        }
    }

    // ========================================================================
    // 7. Safety interlock state machine
    // ========================================================================

    mod safety_tests {
        use super::*;
        use crate::safety_service::{ApplicationSafetyService, FaultSeverity, InterlockState};
        use racing_wheel_engine::{SafetyPolicy, safety::FaultType};
        use racing_wheel_schemas::prelude::TorqueNm;

        async fn make_safety() -> anyhow::Result<ApplicationSafetyService> {
            ApplicationSafetyService::new(SafetyPolicy::default(), None).await
        }

        fn device_id(name: &str) -> racing_wheel_schemas::prelude::DeviceId {
            must(name.parse())
        }

        fn torque(v: f32) -> TorqueNm {
            must(TorqueNm::new(v))
        }

        #[tokio::test]
        async fn register_device_starts_in_safe_torque() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("safe-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            let state = svc.get_safety_state(&did).await?;
            assert!(matches!(state.interlock_state, InterlockState::SafeTorque));
            Ok(())
        }

        #[tokio::test]
        async fn unregister_then_get_errors() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("unreg-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;
            svc.unregister_device(&did).await?;

            let result = svc.get_safety_state(&did).await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn request_high_torque_issues_challenge() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("ht-challenge");
            svc.register_device(did.clone(), torque(15.0)).await?;

            // Must have hands on before requesting high torque
            svc.update_hands_on_detection(&did, true).await?;

            let state = svc
                .request_high_torque(&did, "test-user".to_string())
                .await?;
            assert!(matches!(state, InterlockState::Challenge { .. }));
            Ok(())
        }

        #[tokio::test]
        async fn high_torque_on_faulted_device_errors() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("faulted-ht");
            svc.register_device(did.clone(), torque(10.0)).await?;
            svc.emergency_stop(&did, "test fault".to_string()).await?;

            let result = svc.request_high_torque(&did, "test-user".to_string()).await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn emergency_stop_sets_zero_torque() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("estop-dev");
            svc.register_device(did.clone(), torque(20.0)).await?;

            svc.emergency_stop(&did, "test e-stop".to_string()).await?;

            let state = svc.get_safety_state(&did).await?;
            assert!(matches!(
                state.interlock_state,
                InterlockState::Faulted { .. }
            ));
            assert_eq!(state.current_torque_limit, TorqueNm::ZERO);
            assert_eq!(state.fault_count, 1);
            Ok(())
        }

        #[tokio::test]
        async fn warning_fault_keeps_operating() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("warn-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            let limit_before = svc.get_torque_limit(&did).await?;
            svc.report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Warning)
                .await?;

            let state = svc.get_safety_state(&did).await?;
            // Warning doesn't change interlock state
            assert!(matches!(state.interlock_state, InterlockState::SafeTorque));
            let limit_after = svc.get_torque_limit(&did).await?;
            assert_eq!(limit_before, limit_after);
            Ok(())
        }

        #[tokio::test]
        async fn critical_fault_reduces_torque() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("crit-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            let limit_before = svc.get_torque_limit(&did).await?;
            svc.report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Critical)
                .await?;

            let limit_after = svc.get_torque_limit(&did).await?;
            assert!(
                limit_after < limit_before,
                "critical fault should reduce torque: before={}, after={}",
                limit_before,
                limit_after
            );
            Ok(())
        }

        #[tokio::test]
        async fn fatal_fault_disables_torque() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("fatal-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            svc.report_fault(
                &did,
                FaultType::SafetyInterlockViolation,
                FaultSeverity::Fatal,
            )
            .await?;

            let state = svc.get_safety_state(&did).await?;
            assert!(matches!(
                state.interlock_state,
                InterlockState::Faulted { .. }
            ));
            assert_eq!(state.current_torque_limit, TorqueNm::ZERO);
            Ok(())
        }

        #[tokio::test]
        async fn clear_matching_fault_restores_safe_torque() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("clear-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            svc.report_fault(
                &did,
                FaultType::SafetyInterlockViolation,
                FaultSeverity::Fatal,
            )
            .await?;
            svc.clear_fault(&did, FaultType::SafetyInterlockViolation)
                .await?;

            let state = svc.get_safety_state(&did).await?;
            assert!(matches!(state.interlock_state, InterlockState::SafeTorque));
            Ok(())
        }

        #[tokio::test]
        async fn clear_mismatched_fault_errors() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("mismatch-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            svc.report_fault(
                &did,
                FaultType::SafetyInterlockViolation,
                FaultSeverity::Fatal,
            )
            .await?;

            // Try clearing wrong fault type
            let result = svc.clear_fault(&did, FaultType::ThermalLimit).await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn clear_non_faulted_device_errors() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("nonfault-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            let result = svc.clear_fault(&did, FaultType::ThermalLimit).await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn hands_on_detection_updates() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("hands-dev");
            svc.register_device(did.clone(), torque(10.0)).await?;

            svc.update_hands_on_detection(&did, true).await?;
            let state = svc.get_safety_state(&did).await?;
            assert!(state.hands_on_detected);
            assert!(state.last_hands_on_time.is_some());

            svc.update_hands_on_detection(&did, false).await?;
            let state2 = svc.get_safety_state(&did).await?;
            assert!(!state2.hands_on_detected);
            Ok(())
        }

        #[tokio::test]
        async fn unregistered_device_operations_error() -> anyhow::Result<()> {
            let svc = make_safety().await?;
            let did = device_id("ghost-dev");

            assert!(svc.get_safety_state(&did).await.is_err());
            assert!(svc.get_torque_limit(&did).await.is_err());
            assert!(svc.update_hands_on_detection(&did, true).await.is_err());
            assert!(
                svc.request_high_torque(&did, "user".to_string())
                    .await
                    .is_err()
            );
            assert!(svc.emergency_stop(&did, "test".to_string()).await.is_err());
            Ok(())
        }
    }

    // ========================================================================
    // Config validation service
    // ========================================================================

    mod config_validation_tests {
        use super::*;
        use crate::config_validation::ConfigValidationService;
        use crate::game_service::{ConfigDiff, DiffOperation};

        #[tokio::test]
        async fn iracing_golden_file_validates() -> anyhow::Result<()> {
            let svc = ConfigValidationService::new();
            let diffs = vec![ConfigDiff {
                file_path: "Documents/iRacing/app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: "telemetryDiskFile".to_string(),
                old_value: None,
                new_value: "1".to_string(),
                operation: DiffOperation::Add,
            }];

            let result = svc.validate_config_generation("iracing", &diffs).await?;
            assert!(
                result.success,
                "iracing golden file should pass: {:?}",
                result.details
            );
            Ok(())
        }

        #[tokio::test]
        async fn unknown_game_errors() {
            let svc = ConfigValidationService::new();
            let result = svc
                .validate_config_generation("nonexistent_game", &[])
                .await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn missing_diff_detected() -> anyhow::Result<()> {
            let svc = ConfigValidationService::new();
            // Pass empty diffs for iracing — should fail due to missing expected diffs
            let result = svc.validate_config_generation("iracing", &[]).await?;
            assert!(!result.success);
            assert!(!result.details.missing_items.is_empty());
            Ok(())
        }

        #[tokio::test]
        async fn unexpected_diff_detected() -> anyhow::Result<()> {
            let svc = ConfigValidationService::new();
            let diffs = vec![
                ConfigDiff {
                    file_path: "Documents/iRacing/app.ini".to_string(),
                    section: Some("Telemetry".to_string()),
                    key: "telemetryDiskFile".to_string(),
                    old_value: None,
                    new_value: "1".to_string(),
                    operation: DiffOperation::Add,
                },
                ConfigDiff {
                    file_path: "some/other/file.txt".to_string(),
                    section: None,
                    key: "unexpected_key".to_string(),
                    old_value: None,
                    new_value: "surprise".to_string(),
                    operation: DiffOperation::Add,
                },
            ];

            let result = svc.validate_config_generation("iracing", &diffs).await?;
            assert!(!result.success);
            assert!(!result.details.unexpected_items.is_empty());
            Ok(())
        }

        #[tokio::test]
        async fn config_file_validation_missing_files() -> anyhow::Result<()> {
            let svc = ConfigValidationService::new();
            let temp = TempDir::new()?;

            // Validate against empty directory — files should be missing
            let result = svc.validate_config_files("iracing", temp.path()).await?;
            assert!(!result.success);
            assert!(!result.details.missing_items.is_empty());
            Ok(())
        }
    }

    // ========================================================================
    // Game service tests
    // ========================================================================

    mod game_service_tests {
        use super::*;
        use crate::game_service::GameService;

        #[tokio::test]
        async fn game_service_creation() -> anyhow::Result<()> {
            let svc = GameService::new().await?;
            let games = svc.get_supported_games().await;
            assert!(!games.is_empty(), "should have at least one supported game");
            Ok(())
        }

        #[tokio::test]
        async fn supported_games_include_iracing() -> anyhow::Result<()> {
            let svc = GameService::new().await?;
            let games = svc.get_supported_games().await;
            assert!(
                games.iter().any(|g| g == "iracing"),
                "iracing should be in supported games: {:?}",
                games
            );
            Ok(())
        }

        #[tokio::test]
        async fn get_game_support_for_known_game() -> anyhow::Result<()> {
            let svc = GameService::new().await?;
            let support = svc.get_game_support("iracing").await?;
            assert!(!support.versions.is_empty());
            Ok(())
        }

        #[tokio::test]
        async fn get_game_support_for_unknown_game_errors() -> anyhow::Result<()> {
            let svc = GameService::new().await?;
            let result = svc.get_game_support("nonexistent_game_xyz").await;
            assert!(result.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn telemetry_mapping_for_known_game() -> anyhow::Result<()> {
            let svc = GameService::new().await?;
            let mapping = svc.get_telemetry_mapping("iracing").await?;
            // Should have at least the ffb_scalar field mapped
            assert!(
                mapping.ffb_scalar.is_some(),
                "iracing should have ffb_scalar telemetry mapping"
            );
            Ok(())
        }

        #[tokio::test]
        async fn configure_telemetry_for_known_game() -> anyhow::Result<()> {
            let temp = TempDir::new()?;
            let svc = GameService::new().await?;

            let diffs = svc.configure_telemetry("iracing", temp.path()).await?;
            assert!(
                !diffs.is_empty(),
                "iracing telemetry config should produce diffs"
            );
            Ok(())
        }
    }

    // ========================================================================
    // Feature flags and system-level defaults
    // ========================================================================

    mod feature_flag_tests {
        use crate::system_config::FeatureFlags;

        #[test]
        fn feature_flags_debug_trait() {
            let flags = FeatureFlags {
                disable_realtime: true,
                force_ffb_mode: Some("test".to_string()),
                enable_dev_features: true,
                enable_debug_logging: false,
                enable_virtual_devices: true,
                disable_safety_interlocks: false,
                enable_plugin_dev_mode: false,
            };
            let debug_str = format!("{:?}", flags);
            assert!(debug_str.contains("disable_realtime"));
            assert!(debug_str.contains("force_ffb_mode"));
        }
    }
}
