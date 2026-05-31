//! Integration tests for HID adapters with virtual device validation
//!
//! These tests validate the HID adapter implementations using virtual devices
//! to ensure proper enumeration, I/O operations, and RT optimizations.

#![allow(unused_comparisons)]
use racing_wheel_engine::{
    DeviceEvent, HidPort, VirtualDevice, VirtualHidPort,
    hid::{RTSetup, create_hid_port},
};
use racing_wheel_schemas::prelude::DeviceId;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::time::{Duration, timeout};

fn create_test_hid_port() -> Result<Box<dyn HidPort>, Box<dyn std::error::Error>> {
    if env::var_os("OPENRACING_LIVE_HID_TESTS").is_some() {
        return create_hid_port();
    }

    let mut port = VirtualHidPort::new();
    let device_id: DeviceId = "virtual-hid-integration-wheel".parse()?;
    port.add_device(VirtualDevice::new(
        device_id,
        "Virtual HID Integration Wheel".to_string(),
    ))?;
    Ok(Box::new(port))
}

/// Test HID port creation for current platform
#[tokio::test]
async fn test_hid_port_creation() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;

    // Should be able to list devices (even if empty)
    let devices = port.list_devices().await?;
    println!("Found {} devices", devices.len());

    // Devices should have valid information
    for device in &devices {
        assert!(!device.name.is_empty());
        assert!(device.vendor_id != 0);
        assert!(device.product_id != 0);
        assert!(!device.path.is_empty());
        assert!(device.capabilities.max_torque.value() > 0.0);
        assert!(device.capabilities.encoder_cpr > 0);
        assert!(device.capabilities.min_report_period_us > 0);
    }
    Ok(())
}

/// Test device enumeration and refresh
#[tokio::test]
async fn test_device_enumeration_and_refresh() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;

    // Initial enumeration
    let devices1 = port.list_devices().await?;

    // Refresh devices
    port.refresh_devices().await?;

    // Second enumeration should work
    let devices2 = port.list_devices().await?;

    // Should have same number of devices (assuming no hot-plug during test)
    assert_eq!(devices1.len(), devices2.len());

    // Device IDs should be consistent
    for (d1, d2) in devices1.iter().zip(devices2.iter()) {
        assert_eq!(d1.id, d2.id);
        assert_eq!(d1.vendor_id, d2.vendor_id);
        assert_eq!(d1.product_id, d2.product_id);
    }
    Ok(())
}

/// Test device opening and basic operations
#[tokio::test]
async fn test_device_opening_and_operations() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;
    let devices = port.list_devices().await?;

    if devices.is_empty() {
        println!("No devices found, skipping device opening test");
        return Ok(());
    }

    let device_info = &devices[0];
    println!("Testing with device: {}", device_info.name);

    // Open device
    let mut device = port.open_device(&device_info.id).await?;

    // Device should be connected
    assert!(device.is_connected());

    // Capabilities should match enumeration
    let max_torque = {
        let caps = device.capabilities();
        assert_eq!(caps.max_torque, device_info.capabilities.max_torque);
        assert_eq!(caps.encoder_cpr, device_info.capabilities.encoder_cpr);
        caps.max_torque.value()
    };

    // Test FFB report writing (should not fail for mock devices)
    let result = device.write_ffb_report(5.0, 1);
    assert!(result.is_ok(), "FFB write failed: {:?}", result);

    // Test torque limit validation
    let _result = device.write_ffb_report(max_torque + 1.0, 2);
    // This might succeed for mock devices, but shouldn't crash

    // Test telemetry reading
    let telemetry = device.read_telemetry();
    if let Some(tel) = telemetry {
        assert!(tel.temperature_c >= 20 && tel.temperature_c <= 100);
        // Other fields can be any value for mock devices
    }

    // Test health status
    let health = device.health_status();
    assert!(health.temperature_c >= 20 && health.temperature_c <= 100);
    Ok(())
}

/// Test device monitoring for connect/disconnect events
#[tokio::test]
async fn test_device_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;

    // Start monitoring
    let mut event_rx = port.monitor_devices().await?;

    // Wait for initial events or timeout
    let timeout_duration = Duration::from_millis(1000);

    // Try to receive an event (might timeout if no devices are hot-plugged)
    match timeout(timeout_duration, event_rx.recv()).await {
        Ok(Some(event)) => match event {
            DeviceEvent::Connected(info) => {
                println!("Device connected: {}", info.name);
                assert!(!info.name.is_empty());
                assert!(info.is_connected);
            }
            DeviceEvent::Disconnected(info) => {
                println!("Device disconnected: {}", info.name);
                assert!(!info.name.is_empty());
            }
        },
        Ok(None) => {
            println!("Event channel closed");
        }
        Err(_) => {
            println!("No device events received within timeout (expected for static test)");
        }
    }
    Ok(())
}

/// Test RT-safe FFB writing under load
#[tokio::test]
async fn test_rt_safe_ffb_writing() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;
    let devices = port.list_devices().await?;

    if devices.is_empty() {
        println!("No devices found, skipping RT FFB test");
        return Ok(());
    }

    let device_info = &devices[0];
    let mut device = port.open_device(&device_info.id).await?;

    // Test rapid FFB writes (simulating 1kHz operation)
    let write_count = Arc::new(AtomicU32::new(0));
    let error_count = Arc::new(AtomicU32::new(0));

    let start_time = std::time::Instant::now();
    let test_duration = Duration::from_millis(100); // 100ms test

    let mut seq = 0u16;
    while start_time.elapsed() < test_duration {
        let torque = (seq as f32 / 1000.0).sin() * 5.0; // Sine wave torque

        match device.write_ffb_report(torque, seq) {
            Ok(()) => {
                write_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        seq = seq.wrapping_add(1);

        // Small delay to prevent overwhelming the system
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let writes = write_count.load(Ordering::Relaxed);
    let errors = error_count.load(Ordering::Relaxed);

    println!("FFB writes: {}, errors: {}", writes, errors);

    // Should have completed many writes
    assert!(writes > 0, "No successful FFB writes");

    // Error rate should be reasonable (< 10% for mock devices)
    let error_rate = errors as f32 / (writes + errors) as f32;
    assert!(
        error_rate < 0.1,
        "Error rate too high: {:.2}%",
        error_rate * 100.0
    );
    Ok(())
}

/// Test telemetry reading consistency
#[tokio::test]
async fn test_telemetry_reading_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;
    let devices = port.list_devices().await?;

    if devices.is_empty() {
        println!("No devices found, skipping telemetry test");
        return Ok(());
    }

    let device_info = &devices[0];
    let mut device = port.open_device(&device_info.id).await?;

    // Read telemetry multiple times
    let mut telemetry_readings = Vec::new();

    for _ in 0..10 {
        if let Some(telemetry) = device.read_telemetry() {
            telemetry_readings.push(telemetry);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    if telemetry_readings.is_empty() {
        println!("No telemetry data received (expected for some mock devices)");
        return Ok(());
    }

    println!("Received {} telemetry readings", telemetry_readings.len());

    // Validate telemetry data consistency
    for (i, telemetry) in telemetry_readings.iter().enumerate() {
        // Temperature should be in reasonable range
        assert!(
            telemetry.temperature_c >= 20 && telemetry.temperature_c <= 100,
            "Reading {}: Invalid temperature: {}°C",
            i,
            telemetry.temperature_c
        );

        // Wheel angle should be in reasonable range (±1080°)
        assert!(
            telemetry.wheel_angle_deg >= -1080.0 && telemetry.wheel_angle_deg <= 1080.0,
            "Reading {}: Invalid wheel angle: {}°",
            i,
            telemetry.wheel_angle_deg
        );

        // Wheel speed should be reasonable (±100 rad/s)
        assert!(
            telemetry.wheel_speed_rad_s >= -100.0 && telemetry.wheel_speed_rad_s <= 100.0,
            "Reading {}: Invalid wheel speed: {} rad/s",
            i,
            telemetry.wheel_speed_rad_s
        );

        // Fault flags should be a valid bitfield (u8 is always <= 0xFF)
        // This assertion is always true but kept for documentation
        // assert!(
        //     true,
        //     "Reading {}: Invalid fault flags: 0x{:02X}",
        //     i,
        //     telemetry.fault_flags
        // );
    }
    Ok(())
}

/// Test RT setup and cleanup
#[test]
fn test_rt_setup_and_cleanup() -> Result<(), Box<dyn std::error::Error>> {
    // Apply RT optimizations
    let setup_result = RTSetup::apply_rt_optimizations();

    // Should not fail (might warn about permissions)
    match setup_result {
        Ok(()) => println!("RT optimizations applied successfully"),
        Err(e) => println!("RT optimizations failed (expected on some systems): {}", e),
    }

    // Revert RT optimizations
    let revert_result = RTSetup::revert_rt_optimizations();

    // Should not fail
    match revert_result {
        Ok(()) => println!("RT optimizations reverted successfully"),
        Err(e) => println!("RT optimizations revert failed: {}", e),
    }
    Ok(())
}

/// Test device capabilities validation
#[tokio::test]
async fn test_device_capabilities_validation() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;
    let devices = port.list_devices().await?;

    for device_info in &devices {
        let caps = &device_info.capabilities;

        // Validate capability flags are reasonable
        if caps.supports_raw_torque_1khz {
            assert!(
                caps.min_report_period_us <= 1000,
                "Device claims 1kHz support but min period is {}μs",
                caps.min_report_period_us
            );
        }

        // Validate torque range
        assert!(
            caps.max_torque.value() > 0.0 && caps.max_torque.value() <= 50.0,
            "Invalid max torque: {} Nm",
            caps.max_torque.value()
        );

        // Validate encoder resolution
        assert!(
            caps.encoder_cpr >= 100,
            "Invalid encoder CPR: {}",
            caps.encoder_cpr
        );

        // Validate minimum report period
        assert!(
            caps.min_report_period_us >= 100,
            "Invalid min report period: {}μs",
            caps.min_report_period_us
        );

        // At least one FFB mode should be supported
        assert!(
            caps.supports_pid || caps.supports_raw_torque_1khz,
            "Device supports no FFB modes"
        );
    }
    Ok(())
}

/// Test error handling and recovery
#[tokio::test]
async fn test_error_handling_and_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;

    // Try to open non-existent device
    let fake_id = DeviceId::new("non-existent-device".to_string())?;
    let result = port.open_device(&fake_id).await;
    assert!(result.is_err(), "Opening non-existent device should fail");

    // List devices should still work after error
    let devices = port.list_devices().await?;
    println!(
        "Device enumeration recovered, found {} devices",
        devices.len()
    );

    // If we have real devices, test with them
    if !devices.is_empty() {
        let device_info = &devices[0];
        let mut device = port.open_device(&device_info.id).await?;

        // Device operations should work normally
        assert!(device.is_connected());
        let result = device.write_ffb_report(1.0, 1);
        assert!(result.is_ok(), "FFB write should work after error recovery");
    }
    Ok(())
}

/// Benchmark FFB write latency
#[tokio::test]
async fn test_ffb_write_latency_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let port = create_test_hid_port()?;
    let devices = port.list_devices().await?;

    if devices.is_empty() {
        println!("No devices found, skipping latency benchmark");
        return Ok(());
    }

    let device_info = &devices[0];
    let mut device = port.open_device(&device_info.id).await?;

    // Warm up
    for i in 0..10 {
        let _ = device.write_ffb_report(1.0, i);
    }

    // Benchmark FFB write latency
    let iterations = 1000;
    let mut latencies = Vec::with_capacity(iterations);

    for seq in 0..iterations {
        let start = std::time::Instant::now();
        let result = device.write_ffb_report(2.0, seq as u16);
        let latency = start.elapsed();

        if result.is_ok() {
            latencies.push(latency);
        }
    }

    if latencies.is_empty() {
        println!("No successful FFB writes for latency measurement");
        return Ok(());
    }

    // Calculate statistics
    latencies.sort();
    let count = latencies.len();
    let min = latencies[0];
    let max = latencies[count - 1];
    let median = latencies[count / 2];
    let p99 = latencies[(count * 99) / 100];
    let avg: Duration = latencies.iter().sum::<Duration>() / count as u32;

    println!("FFB Write Latency Benchmark ({} samples):", count);
    println!("  Min:    {:?}", min);
    println!("  Avg:    {:?}", avg);
    println!("  Median: {:?}", median);
    println!("  P99:    {:?}", p99);
    println!("  Max:    {:?}", max);

    // Performance assertions (for mock devices, these should be very fast)
    assert!(
        p99 < Duration::from_millis(1),
        "P99 latency too high: {:?}",
        p99
    );
    assert!(
        avg < Duration::from_micros(500),
        "Average latency too high: {:?}",
        avg
    );
    Ok(())
}
