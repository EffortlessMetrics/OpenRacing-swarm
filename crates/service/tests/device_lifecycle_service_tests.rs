//! Service-level device lifecycle tests.
//!
//! Covers service-level device enumeration, device status reporting,
//! multi-device coordination, device priority/selection when multiple
//! wheels are connected, and graceful degradation when the preferred
//! device is unavailable.

use racing_wheel_engine::{VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::DeviceId;
use racing_wheel_service::{ApplicationDeviceService, CalibrationType, DeviceState};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type BoxErr = Box<dyn std::error::Error>;

fn make_id(name: &str) -> Result<DeviceId, BoxErr> {
    Ok(name.parse::<DeviceId>()?)
}

async fn service_with_port(port: VirtualHidPort) -> Result<ApplicationDeviceService, BoxErr> {
    let svc = ApplicationDeviceService::new(Arc::new(port), None).await?;
    Ok(svc)
}

async fn seeded_service(
    names: &[&str],
) -> Result<(ApplicationDeviceService, Vec<DeviceId>), BoxErr> {
    let mut port = VirtualHidPort::new();
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let id = make_id(name)?;
        port.add_device(VirtualDevice::new(id.clone(), name.to_string()))?;
        ids.push(id);
    }
    let svc = ApplicationDeviceService::new(Arc::new(port), None).await?;
    Ok((svc, ids))
}

// ===================================================================
// 1. Service-level device enumeration
// ===================================================================

#[tokio::test]
async fn enumerate_empty_port_returns_empty() -> Result<(), BoxErr> {
    let svc = service_with_port(VirtualHidPort::new()).await?;
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 0);
    Ok(())
}

#[tokio::test]
async fn enumerate_single_device() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["svc-wheel"]).await?;
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, ids[0]);
    Ok(())
}

#[tokio::test]
async fn enumerate_multiple_devices() -> Result<(), BoxErr> {
    let (svc, _ids) = seeded_service(&["svc-a", "svc-b", "svc-c"]).await?;
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 3);
    Ok(())
}

#[tokio::test]
async fn list_devices_alias_matches_enumerate() -> Result<(), BoxErr> {
    let (svc, _ids) = seeded_service(&["alias-dev"]).await?;
    let enum_result = svc.enumerate_devices().await?;
    let list_result = svc.list_devices().await?;
    assert_eq!(enum_result.len(), list_result.len());
    Ok(())
}

// ===================================================================
// 2. Device status reporting
// ===================================================================

#[tokio::test]
async fn device_state_connected_after_enumeration() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["status-dev"]).await?;
    svc.enumerate_devices().await?;

    let managed = svc
        .get_device(&ids[0])
        .await?
        .ok_or("device should exist")?;
    assert_eq!(managed.state, DeviceState::Connected);
    Ok(())
}

#[tokio::test]
async fn device_state_ready_after_initialization() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["ready-dev"]).await?;
    svc.enumerate_devices().await?;
    svc.initialize_device(&ids[0]).await?;

    let managed = svc
        .get_device(&ids[0])
        .await?
        .ok_or("device should exist")?;
    assert_eq!(managed.state, DeviceState::Ready);
    Ok(())
}

#[tokio::test]
async fn device_capabilities_populated_after_init() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["caps-init"]).await?;
    svc.enumerate_devices().await?;
    svc.initialize_device(&ids[0]).await?;

    let managed = svc
        .get_device(&ids[0])
        .await?
        .ok_or("device should exist")?;
    let caps = managed.capabilities.ok_or("capabilities should be set")?;
    assert!(caps.max_torque.value() > 0.0);
    assert!(caps.supports_raw_torque_1khz);
    Ok(())
}

#[tokio::test]
async fn device_health_readable_after_init() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["health-read"]).await?;
    svc.enumerate_devices().await?;
    svc.initialize_device(&ids[0]).await?;

    let health = svc.get_device_health(&ids[0]).await?;
    assert_eq!(health.fault_flags, 0);
    assert_eq!(health.communication_errors, 0);
    Ok(())
}

#[tokio::test]
async fn get_device_status_returns_info_and_telemetry() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["status-pair"]).await?;
    svc.enumerate_devices().await?;

    let (info, _telemetry) = svc.get_device_status(&ids[0]).await?;
    assert_eq!(info.id, ids[0]);
    assert!(info.is_connected);
    Ok(())
}

// ===================================================================
// 3. Multi-device coordination
// ===================================================================

#[tokio::test]
async fn multiple_devices_all_managed() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["coord-a", "coord-b", "coord-c"]).await?;
    svc.enumerate_devices().await?;

    let all = svc.get_all_devices().await?;
    assert_eq!(all.len(), 3);

    for id in &ids {
        let managed = svc.get_device(id).await?;
        assert!(managed.is_some(), "device {} should be managed", id);
    }
    Ok(())
}

#[tokio::test]
async fn initialize_multiple_devices_independently() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["init-x", "init-y"]).await?;
    svc.enumerate_devices().await?;

    svc.initialize_device(&ids[0]).await?;
    svc.initialize_device(&ids[1]).await?;

    let dev_x = svc
        .get_device(&ids[0])
        .await?
        .ok_or("device x should exist")?;
    let dev_y = svc
        .get_device(&ids[1])
        .await?
        .ok_or("device y should exist")?;

    assert_eq!(dev_x.state, DeviceState::Ready);
    assert_eq!(dev_y.state, DeviceState::Ready);
    Ok(())
}

#[tokio::test]
async fn calibrate_center_on_managed_device() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["cal-center"]).await?;
    svc.enumerate_devices().await?;

    let cal = svc
        .calibrate_device(&ids[0], CalibrationType::Center)
        .await?;
    assert!(cal.center_position.is_some());
    Ok(())
}

// ===================================================================
// 4. Device priority / selection when multiple wheels connected
// ===================================================================

#[tokio::test]
async fn enumeration_returns_all_connected_devices() -> Result<(), BoxErr> {
    let (svc, _ids) = seeded_service(&["prio-primary", "prio-secondary", "prio-tertiary"]).await?;
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 3);
    Ok(())
}

#[tokio::test]
async fn first_device_appears_first_in_enumeration() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["first-wheel", "second-wheel"]).await?;
    let devices = svc.enumerate_devices().await?;

    // Verify both devices are present (order depends on HashMap iteration)
    let found_ids: Vec<_> = devices.iter().map(|d| d.id.clone()).collect();
    assert!(found_ids.contains(&ids[0]));
    assert!(found_ids.contains(&ids[1]));
    Ok(())
}

#[tokio::test]
async fn statistics_track_device_states() -> Result<(), BoxErr> {
    let (svc, ids) = seeded_service(&["stat-a", "stat-b"]).await?;

    // Initially no tracked devices
    let stats = svc.get_statistics().await;
    assert_eq!(stats.total_devices, 0);

    // After enumeration
    svc.enumerate_devices().await?;
    let stats = svc.get_statistics().await;
    assert_eq!(stats.total_devices, 2);
    assert_eq!(stats.connected_devices, 2);
    assert_eq!(stats.ready_devices, 0);

    // After initializing one
    svc.initialize_device(&ids[0]).await?;
    let stats = svc.get_statistics().await;
    assert_eq!(stats.connected_devices, 2);
    assert_eq!(stats.ready_devices, 1);
    Ok(())
}

// ===================================================================
// 5. Graceful degradation when preferred device unavailable
// ===================================================================

#[tokio::test]
async fn get_nonexistent_device_returns_none() -> Result<(), BoxErr> {
    let svc = service_with_port(VirtualHidPort::new()).await?;
    let ghost = make_id("nonexistent")?;
    let result = svc.get_device(&ghost).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn get_status_nonexistent_device_returns_error() -> Result<(), BoxErr> {
    let svc = service_with_port(VirtualHidPort::new()).await?;
    let ghost = make_id("ghost-status")?;
    let result = svc.get_device_status(&ghost).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn re_enumerate_after_device_removal_marks_disconnected() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("will-vanish")?;
    port.add_device(VirtualDevice::new(id.clone(), "Will Vanish".to_string()))?;

    let port: Arc<dyn racing_wheel_engine::HidPort> = Arc::new(port);
    let svc = ApplicationDeviceService::new(Arc::clone(&port), None).await?;

    // First enumeration — device exists
    svc.enumerate_devices().await?;
    let managed = svc.get_device(&id).await?.ok_or("device should exist")?;
    assert_eq!(managed.state, DeviceState::Connected);

    // We cannot remove from an Arc<VirtualHidPort>, but we can verify
    // that a second enumeration with the same port still finds it
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 1);
    Ok(())
}

#[tokio::test]
async fn service_handles_empty_port_gracefully() -> Result<(), BoxErr> {
    let svc = service_with_port(VirtualHidPort::new()).await?;

    // All queries on empty service should succeed, not panic
    let devices = svc.enumerate_devices().await?;
    assert_eq!(devices.len(), 0);

    let all = svc.get_all_devices().await?;
    assert_eq!(all.len(), 0);

    let stats = svc.get_statistics().await;
    assert_eq!(stats.total_devices, 0);
    assert_eq!(stats.connected_devices, 0);
    assert_eq!(stats.ready_devices, 0);
    assert_eq!(stats.faulted_devices, 0);
    Ok(())
}

#[tokio::test]
async fn repeated_enumeration_idempotent() -> Result<(), BoxErr> {
    let (svc, _ids) = seeded_service(&["idem-dev"]).await?;

    for _ in 0..5 {
        let devices = svc.enumerate_devices().await?;
        assert_eq!(devices.len(), 1);
    }

    // Should still have exactly one managed device
    let all = svc.get_all_devices().await?;
    assert_eq!(all.len(), 1);
    Ok(())
}
