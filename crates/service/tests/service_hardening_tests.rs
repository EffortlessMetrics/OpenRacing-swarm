//! Service integration hardening tests.
//!
//! Fills gaps left by the existing service_integration_tests:
//!   • Device service: multi-device management, re-enumeration, state queries
//!   • Game service: active game lifecycle, support lookup, telemetry mapping,
//!     error paths, stable/experimental categorization, config writing
//!   • Profile service: signed profiles, load-by-string, concurrent CRUD,
//!     update-nonexistent, clear active profile
//!   • Diagnostic service: per-test execution, metadata validation
//!   • Safety service: wrong-fault-type clear, no-challenge response,
//!     unregistered device errors, concurrent fault reporting
//!   • Anti-cheat: report generation and markdown output

use std::sync::Arc;
use std::time::Duration;

use racing_wheel_engine::{SafetyPolicy, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::{
    BaseSettings, Degrees, DeviceCapabilities, DeviceId, FilterConfig, Gain, Profile, ProfileId,
    ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    AntiCheatReport, ApplicationDeviceService, DeviceState, DiagnosticService, DiagnosticStatus,
    FaultSeverity, GameService, WheelService, profile_repository::ProfileRepositoryConfig,
    safety_service::ApplicationSafetyService,
};
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

async fn temp_service() -> Result<(WheelService, TempDir), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let svc =
        WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config).await?;
    Ok((svc, tmp))
}

fn make_profile(id: &str) -> Result<Profile, BoxErr> {
    let pid = ProfileId::new(id.to_string())?;
    Ok(Profile::new(
        pid,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Test Profile {id}"),
    ))
}

fn test_device_capabilities() -> Result<DeviceCapabilities, BoxErr> {
    Ok(DeviceCapabilities::new(
        false,
        true,
        true,
        true,
        TorqueNm::new(25.0)?,
        10000,
        1000,
    ))
}

// =========================================================================
// Device service: multi-device management
// =========================================================================

#[tokio::test]
async fn device_multi_device_port_enumerates_all() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    for i in 0..3 {
        let id: DeviceId = format!("multi-dev-{i}").parse()?;
        port.add_device(VirtualDevice::new(id, format!("Wheel {i}")))
            .map_err(|e| -> BoxErr { format!("{e}").into() })?;
    }
    let svc = ApplicationDeviceService::new(Arc::new(port), None).await?;
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 3, "all three virtual devices should appear");
    Ok(())
}

#[tokio::test]
async fn device_get_all_devices_after_enumerate() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    // Before enumeration, get_all_devices should be empty
    let before = svc.device_service().get_all_devices().await?;
    assert!(before.is_empty(), "no devices tracked before enumeration");

    let _ = svc.device_service().enumerate_devices().await?;
    let after = svc.device_service().get_all_devices().await?;
    assert!(!after.is_empty(), "devices tracked after enumeration");
    Ok(())
}

#[tokio::test]
async fn device_list_devices_alias_matches_enumerate() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let enumerated = svc.device_service().enumerate_devices().await?;
    let listed = svc.device_service().list_devices().await?;
    assert_eq!(enumerated.len(), listed.len());
    Ok(())
}

#[tokio::test]
async fn device_status_after_init() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    let dev_id = &devices[0].id;

    svc.device_service().initialize_device(dev_id).await?;

    let (info, telemetry) = svc.device_service().get_device_status(dev_id).await?;
    assert!(info.is_connected);
    // Telemetry is None until data arrives
    assert!(telemetry.is_none());
    Ok(())
}

#[tokio::test]
async fn device_status_nonexistent_errors() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let bad_id: DeviceId = "no-such-device".parse()?;
    let result = svc.device_service().get_device_status(&bad_id).await;
    assert!(result.is_err(), "status of unknown device should error");
    Ok(())
}

#[tokio::test]
async fn device_re_enumerate_preserves_state() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices1 = svc.device_service().enumerate_devices().await?;
    let dev_id = &devices1[0].id;

    // Initialize the device
    svc.device_service().initialize_device(dev_id).await?;
    let managed = svc.device_service().get_device(dev_id).await?;
    let dev = managed.ok_or("expected device")?;
    assert_eq!(dev.state, DeviceState::Ready);

    // Re-enumerate should re-discover the same device
    let devices2 = svc.device_service().enumerate_devices().await?;
    assert_eq!(devices1.len(), devices2.len());
    Ok(())
}

#[tokio::test]
async fn device_statistics_zero_when_empty_port() -> Result<(), BoxErr> {
    let port = VirtualHidPort::new();
    let svc = ApplicationDeviceService::new(Arc::new(port), None).await?;
    let _ = svc.enumerate_devices().await?;
    let stats = svc.get_statistics().await;
    assert_eq!(stats.total_devices, 0);
    assert_eq!(stats.connected_devices, 0);
    Ok(())
}

#[tokio::test]
async fn device_health_reports_valid_temperature() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    let dev_id = &devices[0].id;

    let health = svc.device_service().get_device_health(dev_id).await?;
    // Virtual device temperature defaults to 35°C
    assert!(
        health.temperature_c > 0 && health.temperature_c < 100,
        "temperature should be in reasonable range"
    );
    assert_eq!(health.communication_errors, 0);
    Ok(())
}

// =========================================================================
// Game service: active game lifecycle and support lookup
// =========================================================================

#[tokio::test]
async fn game_service_set_and_get_active_game() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;

    gs.set_active_game(Some("iracing".to_string())).await?;
    let active = gs.get_active_game().await;
    assert_eq!(active.as_deref(), Some("iracing"));

    // Status should reflect active game
    let status = gs.get_game_status().await?;
    assert_eq!(status.active_game.as_deref(), Some("iracing"));
    Ok(())
}

#[tokio::test]
async fn game_service_clear_active_game() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    gs.set_active_game(Some("acc".to_string())).await?;
    gs.set_active_game(None).await?;

    let active = gs.get_active_game().await;
    assert!(active.is_none(), "active game should be cleared");
    Ok(())
}

#[tokio::test]
async fn game_service_get_support_for_known_game() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let games = gs.get_supported_games().await;

    // Pick the first supported game and retrieve its support info
    let first_game = games.first().ok_or("no supported games in matrix")?;
    let support = gs.get_game_support(first_game).await?;
    assert!(!support.versions.is_empty(), "game should have versions");
    Ok(())
}

#[tokio::test]
async fn game_service_unsupported_game_errors() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let result = gs.get_game_support("totally_fake_game_xyz").await;
    assert!(result.is_err(), "unsupported game should error");
    Ok(())
}

#[tokio::test]
async fn game_service_telemetry_mapping_for_known_game() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let games = gs.get_supported_games().await;
    let first_game = games.first().ok_or("no supported games")?;

    let _mapping = gs.get_telemetry_mapping(first_game).await?;
    // Just verify it doesn't error — the mapping struct is opaque
    Ok(())
}

#[tokio::test]
async fn game_service_telemetry_mapping_unsupported_errors() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let result = gs.get_telemetry_mapping("not_a_real_game").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn game_service_stable_and_experimental_partition() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let all = gs.get_supported_games().await;
    let stable = gs.get_stable_games().await;
    let experimental = gs.get_experimental_games().await;

    // Stable and experimental should be subsets of all
    for g in &stable {
        assert!(all.contains(g), "stable game {g} should be in full list");
    }
    for g in &experimental {
        assert!(
            all.contains(g),
            "experimental game {g} should be in full list"
        );
    }
    Ok(())
}

#[tokio::test]
async fn game_service_writer_bdd_metrics_valid() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let metrics = gs.writer_bdd_metrics();
    // parity_ok should be true at startup (enforced by constructor)
    assert!(
        metrics.parity_ok,
        "writer BDD parity should hold at startup"
    );
    Ok(())
}

#[tokio::test]
async fn game_service_configure_telemetry_for_supported_game() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let games = gs.get_supported_games().await;
    let first_game = games.first().ok_or("no supported games")?;

    let tmp = TempDir::new()?;
    // configure_telemetry may write to game_path; use temp dir as a stand-in
    let diffs = gs.configure_telemetry(first_game, tmp.path()).await?;
    // We just assert it doesn't panic; diffs may be empty for some writers
    assert!(diffs.len() <= 100, "sanity bound on diff count");
    Ok(())
}

#[tokio::test]
async fn game_service_configure_telemetry_unsupported_errors() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let tmp = TempDir::new()?;
    let result = gs.configure_telemetry("fake_game_xyzzy", tmp.path()).await;
    assert!(result.is_err(), "configuring unsupported game should error");
    Ok(())
}

#[tokio::test]
async fn game_service_game_id_alias_normalization() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    // normalize_game_id maps known aliases: ea_wrc → eawrc, f1_2025 → f1_25
    let support = gs.get_game_support("ea_wrc").await?;
    assert!(!support.versions.is_empty(), "ea_wrc alias should resolve");
    Ok(())
}

// =========================================================================
// Profile service: signed profiles, load-by-string, error paths
// =========================================================================

#[tokio::test]
async fn profile_signed_create_and_verify() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();

    let profile = make_profile("signed-harden")?;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);

    let pid = ps
        .create_signed_profile(profile.clone(), &signing_key)
        .await?;

    let sig = ps.get_profile_signature(&pid).await?;
    let sig = sig.ok_or("expected signature on signed profile")?;
    assert!(!sig.signature.is_empty());
    assert!(!sig.public_key.is_empty());
    Ok(())
}

#[tokio::test]
async fn profile_load_by_string_id() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();

    let profile = make_profile("load-str")?;
    ps.create_profile(profile.clone()).await?;

    let loaded = ps.load_profile("load-str").await?;
    assert_eq!(loaded.id, profile.id);
    Ok(())
}

#[tokio::test]
async fn profile_load_by_string_nonexistent_errors() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let result = ps.load_profile("does-not-exist").await;
    assert!(result.is_err(), "loading missing profile should error");
    Ok(())
}

#[tokio::test]
async fn profile_update_nonexistent_errors() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let profile = make_profile("ghost-profile")?;
    let result = ps.update_profile(profile).await;
    assert!(result.is_err(), "updating missing profile should error");
    Ok(())
}

#[tokio::test]
async fn profile_clear_active() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let device_id: DeviceId = "clear-active-dev".parse()?;
    let profile = make_profile("clear-active")?;
    let pid = ps.create_profile(profile).await?;

    ps.set_active_profile(&device_id, &pid).await?;
    let active = ps.get_active_profile(&device_id).await?;
    assert!(active.is_some());

    ps.clear_active_profile(&device_id).await?;
    let active = ps.get_active_profile(&device_id).await?;
    assert!(active.is_none(), "active profile should be cleared");
    Ok(())
}

#[tokio::test]
async fn profile_list_returns_all_created() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();

    for i in 0..4 {
        ps.create_profile(make_profile(&format!("list-{i}"))?)
            .await?;
    }

    let profiles = ps.list_profiles().await?;
    assert_eq!(profiles.len(), 4);
    Ok(())
}

#[tokio::test]
async fn profile_hierarchy_falls_back_to_global() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let caps = test_device_capabilities()?;
    let device_id: DeviceId = "fallback-dev".parse()?;

    let global = Profile::new(
        ProfileId::new("global-fb".to_string())?,
        ProfileScope::global(),
        BaseSettings {
            ffb_gain: Gain::new(0.6)?,
            degrees_of_rotation: Degrees::new_dor(900.0)?,
            torque_cap: TorqueNm::new(8.0)?,
            filters: FilterConfig::default(),
        },
        "Global Fallback".to_string(),
    );
    ps.create_profile(global).await?;

    // Request with a game that has no specific profile → should get global
    let resolved = ps
        .apply_profile_to_device(&device_id, Some("unknown_game"), None, None, &caps)
        .await?;
    assert!(
        (resolved.base_settings.ffb_gain.value() - 0.6).abs() < f32::EPSILON,
        "should fall back to global profile"
    );
    Ok(())
}

#[tokio::test]
async fn profile_session_override_priority() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let caps = test_device_capabilities()?;
    let device_id: DeviceId = "override-prio-dev".parse()?;

    // Create global profile
    let global = Profile::new(
        ProfileId::new("global-prio".to_string())?,
        ProfileScope::global(),
        BaseSettings {
            ffb_gain: Gain::new(0.5)?,
            degrees_of_rotation: Degrees::new_dor(900.0)?,
            torque_cap: TorqueNm::new(8.0)?,
            filters: FilterConfig::default(),
        },
        "Global".to_string(),
    );
    ps.create_profile(global).await?;

    // Set a session override with different gain
    let override_profile = Profile::new(
        ProfileId::new("session-prio".to_string())?,
        ProfileScope::global(),
        BaseSettings {
            ffb_gain: Gain::new(0.9)?,
            degrees_of_rotation: Degrees::new_dor(540.0)?,
            torque_cap: TorqueNm::new(8.0)?,
            filters: FilterConfig::default(),
        },
        "Session Override".to_string(),
    );
    ps.set_session_override(&device_id, override_profile)
        .await?;

    // Apply profile — session override should take priority
    let resolved = ps
        .apply_profile_to_device(&device_id, None, None, None, &caps)
        .await?;
    assert!(
        (resolved.base_settings.ffb_gain.value() - 0.9).abs() < f32::EPSILON,
        "session override should take priority over global"
    );
    Ok(())
}

#[tokio::test]
async fn profile_statistics_include_signed() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);

    ps.create_profile(make_profile("stat-unsigned")?).await?;
    ps.create_signed_profile(make_profile("stat-signed")?, &signing_key)
        .await?;

    let stats = ps.get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 2);
    assert_eq!(stats.signed_profiles, 1);
    Ok(())
}

// =========================================================================
// Diagnostic service: per-test execution and metadata
// =========================================================================

#[tokio::test]
async fn diagnostic_all_registered_tests_runnable() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let tests = diag.list_tests();
    assert!(tests.len() >= 5, "should have several diagnostic tests");

    for (name, _desc) in &tests {
        let result = diag.run_test(name).await?;
        assert_eq!(&result.name, name, "result name should match request");
        assert!(
            matches!(
                result.status,
                DiagnosticStatus::Pass | DiagnosticStatus::Warn | DiagnosticStatus::Fail
            ),
            "each test should produce a valid status"
        );
    }
    Ok(())
}

#[tokio::test]
async fn diagnostic_full_suite_result_count_matches_tests() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let test_names = diag.list_tests();
    let results = diag.run_full_diagnostics().await?;
    assert_eq!(
        results.len(),
        test_names.len(),
        "full diagnostics should run all registered tests"
    );
    Ok(())
}

#[tokio::test]
async fn diagnostic_system_requirements_has_metadata() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let result = diag.run_test("system_requirements").await?;
    assert!(
        result.metadata.contains_key("cpu_count"),
        "system_requirements should report cpu_count"
    );
    assert!(
        result.metadata.contains_key("memory_mb"),
        "system_requirements should report memory_mb"
    );
    assert!(
        result.metadata.contains_key("architecture"),
        "system_requirements should report architecture"
    );
    Ok(())
}

#[tokio::test]
async fn diagnostic_timing_test_runs() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let result = diag.run_test("timing").await?;
    assert!(!result.message.is_empty());
    Ok(())
}

// =========================================================================
// Safety service: error paths and edge cases
// =========================================================================

#[tokio::test]
async fn safety_clear_wrong_fault_type_errors() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "wrong-fault-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;
    ss.report_fault(
        &dev_id,
        racing_wheel_engine::safety::FaultType::Overcurrent,
        FaultSeverity::Fatal,
    )
    .await?;

    // Try to clear with a different fault type
    let result = ss
        .clear_fault(
            &dev_id,
            racing_wheel_engine::safety::FaultType::ThermalLimit,
        )
        .await;
    assert!(
        result.is_err(),
        "clearing wrong fault type should be rejected"
    );
    Ok(())
}

#[tokio::test]
async fn safety_clear_non_faulted_device_errors() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "not-faulted-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;

    let result = ss
        .clear_fault(&dev_id, racing_wheel_engine::safety::FaultType::Overcurrent)
        .await;
    assert!(
        result.is_err(),
        "clearing fault on non-faulted device should error"
    );
    Ok(())
}

#[tokio::test]
async fn safety_respond_challenge_without_active_challenge() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "no-challenge-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;

    let result = ss.respond_to_challenge(&dev_id, 12345).await;
    assert!(
        result.is_err(),
        "responding without an active challenge should error"
    );
    Ok(())
}

#[tokio::test]
async fn safety_operations_on_unregistered_device_error() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "unregistered-dev".parse()?;

    assert!(ss.get_safety_state(&dev_id).await.is_err());
    assert!(ss.get_torque_limit(&dev_id).await.is_err());
    assert!(
        ss.request_high_torque(&dev_id, "test".to_string())
            .await
            .is_err()
    );
    assert!(
        ss.report_fault(
            &dev_id,
            racing_wheel_engine::safety::FaultType::Overcurrent,
            FaultSeverity::Fatal,
        )
        .await
        .is_err()
    );
    assert!(
        ss.emergency_stop(&dev_id, "test".to_string())
            .await
            .is_err()
    );
    assert!(ss.update_hands_on_detection(&dev_id, true).await.is_err());
    Ok(())
}

#[tokio::test]
async fn safety_multiple_sequential_faults_increment_count() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "multi-fault-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;

    for _ in 0..3 {
        ss.report_fault(
            &dev_id,
            racing_wheel_engine::safety::FaultType::UsbStall,
            FaultSeverity::Warning,
        )
        .await?;
    }

    let state = ss.get_safety_state(&dev_id).await?;
    assert_eq!(state.fault_count, 3, "three faults should be counted");
    Ok(())
}

#[tokio::test]
async fn safety_critical_fault_halves_torque_repeatedly() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "crit-repeat-dev".parse()?;
    let torque = TorqueNm::new(20.0)?;

    ss.register_device(dev_id.clone(), torque).await?;
    let initial = ss.get_torque_limit(&dev_id).await?.value();

    ss.report_fault(
        &dev_id,
        racing_wheel_engine::safety::FaultType::ThermalLimit,
        FaultSeverity::Critical,
    )
    .await?;
    let after_first = ss.get_torque_limit(&dev_id).await?.value();
    assert!(
        (after_first - initial * 0.5).abs() < 0.01,
        "first critical fault should halve torque"
    );

    ss.report_fault(
        &dev_id,
        racing_wheel_engine::safety::FaultType::ThermalLimit,
        FaultSeverity::Critical,
    )
    .await?;
    let after_second = ss.get_torque_limit(&dev_id).await?.value();
    assert!(
        (after_second - initial * 0.25).abs() < 0.01,
        "second critical fault should halve again"
    );
    Ok(())
}

#[tokio::test]
async fn safety_concurrent_fault_reporting() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = Arc::new(ApplicationSafetyService::new(policy, None).await?);
    let dev_id: DeviceId = "conc-fault-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;

    let mut handles = Vec::new();
    for _ in 0..10 {
        let ss = ss.clone();
        let dev_id = dev_id.clone();
        handles.push(tokio::spawn(async move {
            ss.report_fault(
                &dev_id,
                racing_wheel_engine::safety::FaultType::UsbStall,
                FaultSeverity::Warning,
            )
            .await
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    }

    let state = ss.get_safety_state(&dev_id).await?;
    assert_eq!(
        state.fault_count, 10,
        "all concurrent faults should be counted"
    );
    Ok(())
}

#[tokio::test]
async fn safety_hands_on_toggle() -> Result<(), BoxErr> {
    let policy = SafetyPolicy::default();
    let ss = ApplicationSafetyService::new(policy, None).await?;
    let dev_id: DeviceId = "hands-toggle-dev".parse()?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(dev_id.clone(), torque).await?;

    let state = ss.get_safety_state(&dev_id).await?;
    assert!(!state.hands_on_detected, "hands-on starts false");

    ss.update_hands_on_detection(&dev_id, true).await?;
    let state = ss.get_safety_state(&dev_id).await?;
    assert!(state.hands_on_detected);
    assert!(state.last_hands_on_time.is_some());

    ss.update_hands_on_detection(&dev_id, false).await?;
    let state = ss.get_safety_state(&dev_id).await?;
    assert!(!state.hands_on_detected);
    Ok(())
}

// =========================================================================
// Anti-cheat: report generation
// =========================================================================

#[tokio::test]
async fn anticheat_report_generation_and_markdown() -> Result<(), BoxErr> {
    let report = AntiCheatReport::generate().await?;

    assert!(!report.process_info.dll_injection);
    assert!(report.process_info.kernel_drivers.is_empty());
    assert!(!report.telemetry_methods.is_empty());
    assert!(!report.security_measures.is_empty());

    for method in &report.telemetry_methods {
        assert!(
            method.anticheat_compatible,
            "method '{}' should be anti-cheat compatible",
            method.method_type
        );
    }

    let md = report.to_markdown();
    assert!(md.contains("Anti-Cheat Compatibility Report"));
    assert!(md.contains("No DLL Injection"));
    assert!(md.contains("No Kernel Drivers"));
    Ok(())
}

#[tokio::test]
async fn anticheat_report_serializable() -> Result<(), BoxErr> {
    let report = AntiCheatReport::generate().await?;
    let json = serde_json::to_string(&report)?;
    let _roundtrip: AntiCheatReport = serde_json::from_str(&json)?;
    Ok(())
}

// =========================================================================
// Concurrent cross-service operations
// =========================================================================

#[tokio::test]
async fn concurrent_profile_and_device_operations() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service().clone();
    let ds = svc.device_service().clone();
    let ss = svc.safety_service().clone();

    // Spawn profile creation, device enumeration, and safety registration concurrently
    let profile_handle = {
        let ps = ps.clone();
        tokio::spawn(async move {
            for i in 0..5 {
                let p = make_profile(&format!("cross-svc-{i}"))?;
                ps.create_profile(p).await?;
            }
            Ok::<(), BoxErr>(())
        })
    };

    let device_handle = {
        let ds = ds.clone();
        tokio::spawn(async move {
            for _ in 0..3 {
                let _ = ds.enumerate_devices().await?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<(), BoxErr>(())
        })
    };

    let safety_handle = {
        let ss = ss.clone();
        tokio::spawn(async move {
            for i in 0..3 {
                let dev_id: DeviceId = format!("cross-dev-{i}").parse()?;
                let torque = TorqueNm::new(10.0)?;
                ss.register_device(dev_id, torque).await?;
            }
            Ok::<(), BoxErr>(())
        })
    };

    profile_handle
        .await
        .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    device_handle
        .await
        .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    safety_handle
        .await
        .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;

    let profiles = ps.list_profiles().await?;
    assert_eq!(profiles.len(), 5, "all profiles should be created");

    let safety_stats = ss.get_statistics().await;
    assert_eq!(
        safety_stats.total_devices, 3,
        "all devices should be registered"
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_game_service_queries() -> Result<(), BoxErr> {
    let gs = Arc::new(GameService::new().await?);

    let mut handles = Vec::new();
    for _ in 0..5 {
        let gs = gs.clone();
        handles.push(tokio::spawn(async move {
            let _games = gs.get_supported_games().await;
            let _stable = gs.get_stable_games().await;
            let _status = gs.get_game_status().await?;
            Ok::<(), BoxErr>(())
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    }
    Ok(())
}
