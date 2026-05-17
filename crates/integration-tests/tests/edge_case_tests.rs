//! Edge-case and error-path integration tests for RC quality.
//!
//! Covers zero-length packets, truncated packets, maximum-value packets,
//! all-zero packets, unknown device VIDs, FFB torque clamping, and
//! concurrent telemetry parsing.

use anyhow::Result;

use openracing_telemetry_adapters::adapter_factories;
use racing_wheel_engine::ports::{HidDevice, HidPort};
use racing_wheel_engine::safety::{FaultType, SafetyService};
use racing_wheel_engine::{DeviceInfo, Frame, Pipeline, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;

// ---------------------------------------------------------------------------
// Helper: collect all adapter game IDs for parameterized tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Section a: Zero-length packets
// ---------------------------------------------------------------------------

#[test]
fn test_all_adapters_handle_zero_length_packet_without_panic() -> Result<()> {
    let empty: &[u8] = &[];
    let mut panicked = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        // We only care that it doesn't panic; error is acceptable.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = adapter.normalize(empty);
        }));
        if result.is_err() {
            panicked.push(*id);
        }
    }

    assert!(
        panicked.is_empty(),
        "Adapters panicked on zero-length packet: {:?}",
        panicked
    );
    Ok(())
}

#[test]
fn test_zero_length_packet_returns_error_or_default() -> Result<()> {
    let empty: &[u8] = &[];

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        let result = adapter.normalize(empty);
        // Each adapter must either return Err or a valid (non-NaN) NormalizedTelemetry
        if let Ok(telem) = result {
            let validated = telem.validated();
            assert!(
                validated.speed_ms.is_finite(),
                "Adapter '{}' produced NaN speed on empty input",
                id
            );
            assert!(
                validated.rpm.is_finite(),
                "Adapter '{}' produced NaN rpm on empty input",
                id
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Section b: Truncated packets
// ---------------------------------------------------------------------------

#[test]
fn test_all_adapters_handle_truncated_packets_without_panic() -> Result<()> {
    let truncated_sizes: &[usize] = &[1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 100];
    let mut panicked = Vec::new();

    for (id, factory) in adapter_factories() {
        for &size in truncated_sizes {
            let adapter = factory();
            let data = vec![0xABu8; size];
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = adapter.normalize(&data);
            }));
            if result.is_err() {
                panicked.push((*id, size));
            }
        }
    }

    assert!(
        panicked.is_empty(),
        "Adapters panicked on truncated packets: {:?}",
        panicked
    );
    Ok(())
}

#[test]
fn test_single_byte_packet_does_not_panic() -> Result<()> {
    let single = [0x42u8];
    let mut panicked = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = adapter.normalize(&single);
        }));
        if result.is_err() {
            panicked.push(*id);
        }
    }

    assert!(
        panicked.is_empty(),
        "Adapters panicked on single-byte packet: {:?}",
        panicked
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Section c: Maximum-value packets (all 0xFF)
// ---------------------------------------------------------------------------

#[test]
fn test_all_adapters_handle_max_value_packet_without_panic() -> Result<()> {
    // Use a large 0xFF buffer; adapters will read up to their expected packet size
    let max_packet = vec![0xFFu8; 2048];
    let mut panicked = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = adapter.normalize(&max_packet);
        }));
        if result.is_err() {
            panicked.push(*id);
        }
    }

    assert!(
        panicked.is_empty(),
        "Adapters panicked on all-0xFF packet: {:?}",
        panicked
    );
    Ok(())
}

#[test]
fn test_max_value_packet_produces_no_nan() -> Result<()> {
    let max_packet = vec![0xFFu8; 2048];
    let mut nan_producers = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        if let Ok(telem) = adapter.normalize(&max_packet) {
            let v = telem.validated();
            let has_nan = !v.speed_ms.is_finite()
                || !v.rpm.is_finite()
                || !v.steering_angle.is_finite()
                || !v.lateral_g.is_finite()
                || !v.longitudinal_g.is_finite()
                || !v.vertical_g.is_finite()
                || !v.throttle.is_finite()
                || !v.brake.is_finite()
                || !v.ffb_scalar.is_finite()
                || !v.ffb_torque_nm.is_finite()
                || !v.fuel_percent.is_finite()
                || !v.engine_temp_c.is_finite();
            if has_nan {
                nan_producers.push(*id);
            }
        }
    }

    assert!(
        nan_producers.is_empty(),
        "Adapters produced NaN after validated() on 0xFF packet: {:?}",
        nan_producers
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Section d: All-zero packets
// ---------------------------------------------------------------------------

#[test]
fn test_all_adapters_handle_zero_packet_without_panic() -> Result<()> {
    let zero_packet = vec![0x00u8; 2048];
    let mut panicked = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = adapter.normalize(&zero_packet);
        }));
        if result.is_err() {
            panicked.push(*id);
        }
    }

    assert!(
        panicked.is_empty(),
        "Adapters panicked on all-zero packet: {:?}",
        panicked
    );
    Ok(())
}

#[test]
fn test_zero_packet_produces_valid_defaults() -> Result<()> {
    let zero_packet = vec![0x00u8; 2048];
    let mut invalid = Vec::new();

    for (id, factory) in adapter_factories() {
        let adapter = factory();
        if let Ok(telem) = adapter.normalize(&zero_packet) {
            let v = telem.validated();
            // All-zero input should produce finite, non-NaN output
            if !v.speed_ms.is_finite()
                || !v.rpm.is_finite()
                || !v.steering_angle.is_finite()
                || !v.ffb_scalar.is_finite()
            {
                invalid.push((*id, "non-finite field"));
            }
            // Speed and RPM should be non-negative on zero input
            if v.speed_ms < 0.0 {
                invalid.push((*id, "negative speed"));
            }
            if v.rpm < 0.0 {
                invalid.push((*id, "negative rpm"));
            }
        }
    }

    assert!(
        invalid.is_empty(),
        "Adapters produced invalid output on all-zero packet: {:?}",
        invalid
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Section e: Device enumeration with unknown VIDs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_enumeration_unknown_device_id_is_safe() -> Result<()> {
    let unknown_id: DeviceId = "unknown-vendor-0000".parse()?;
    let device = VirtualDevice::new(unknown_id.clone(), "Unknown Device XYZ".to_string());

    let caps = device.capabilities();
    // Unknown devices should still have valid capabilities with safe defaults
    assert!(
        caps.max_torque.value() >= 0.0,
        "Unknown device max_torque should be non-negative"
    );
    assert!(
        caps.max_torque.value().is_finite(),
        "Unknown device max_torque must be finite"
    );
    Ok(())
}

#[tokio::test]
async fn test_enumeration_unknown_vid_device_appears_in_port() -> Result<()> {
    let mut port = VirtualHidPort::new();
    let unknown_id: DeviceId = "unknown-vendor-ffff".parse()?;
    let device = VirtualDevice::new(unknown_id.clone(), "Mystery Wheel".to_string());

    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    assert_eq!(devices.len(), 1, "Unknown-VID device should be listed");
    let info: &DeviceInfo = &devices[0];
    assert_eq!(info.id, unknown_id);
    assert!(info.is_connected, "Unknown-VID device should be connected");
    Ok(())
}

#[tokio::test]
async fn test_enumeration_unknown_vid_telemetry_returns_valid_data() -> Result<()> {
    let unknown_id: DeviceId = "unknown-vendor-abcd".parse()?;
    let mut device = VirtualDevice::new(unknown_id, "Unrecognized Wheel".to_string());

    let telemetry = device.read_telemetry();
    // Virtual devices should always return telemetry, even with unknown VID
    assert!(
        telemetry.is_some(),
        "Unknown-VID device should still return telemetry"
    );
    let telem = telemetry.ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
    assert!(
        telem.temperature_c <= 150,
        "Temperature should be in a sane range, got {}",
        telem.temperature_c
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Section f: FFB torque clamping
// ---------------------------------------------------------------------------

#[test]
fn test_ffb_clamp_positive_overflow() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    // Values well above safe max should be clamped, not overflow
    for extreme in &[10.0f32, 100.0, 1000.0, f32::MAX, f32::INFINITY] {
        let clamped = safety.clamp_torque_nm(*extreme);
        assert!(
            clamped.is_finite(),
            "Clamped torque must be finite for input {}",
            extreme
        );
        assert!(
            clamped <= 5.0 + f32::EPSILON,
            "Clamped torque should not exceed safe limit for input {}, got {}",
            extreme,
            clamped
        );
    }
    Ok(())
}

#[test]
fn test_ffb_clamp_negative_overflow() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    for extreme in &[-10.0f32, -100.0, -1000.0, f32::MIN, f32::NEG_INFINITY] {
        let clamped = safety.clamp_torque_nm(*extreme);
        assert!(
            clamped.is_finite(),
            "Clamped torque must be finite for input {}",
            extreme
        );
        assert!(
            clamped >= -5.0 - f32::EPSILON,
            "Clamped torque should not go below -safe limit for input {}, got {}",
            extreme,
            clamped
        );
    }
    Ok(())
}

#[test]
fn test_ffb_clamp_special_float_values() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    // NaN should clamp to zero (safety-critical)
    let nan_result = safety.clamp_torque_nm(f32::NAN);
    assert!(
        nan_result.is_finite(),
        "NaN input must produce finite output"
    );
    assert!(
        nan_result.abs() < f32::EPSILON,
        "NaN input should clamp to 0.0, got {}",
        nan_result
    );

    // Negative zero should be valid
    let neg_zero = safety.clamp_torque_nm(-0.0);
    assert!(
        neg_zero.is_finite(),
        "Negative zero must produce finite output"
    );
    assert!(
        neg_zero.abs() < f32::EPSILON,
        "Negative zero should stay near zero, got {}",
        neg_zero
    );

    // Subnormal values should still be valid
    let subnormal = safety.clamp_torque_nm(f32::MIN_POSITIVE / 2.0);
    assert!(
        subnormal.is_finite(),
        "Subnormal input must produce finite output"
    );
    Ok(())
}

#[test]
fn test_ffb_clamp_faulted_state_always_zero() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::Overcurrent);

    // In faulted state, ALL inputs must clamp to zero
    let test_values = [
        0.0f32,
        1.0,
        -1.0,
        5.0,
        -5.0,
        25.0,
        -25.0,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for val in &test_values {
        let clamped = safety.clamp_torque_nm(*val);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Faulted state must clamp {} to 0, got {}",
            val,
            clamped
        );
    }
    Ok(())
}

#[test]
fn test_ffb_pipeline_extreme_frame_values() -> Result<()> {
    let mut pipeline = Pipeline::new();

    // Frame with extreme values should not panic
    let mut frame = Frame {
        ffb_in: f32::MAX,
        torque_out: f32::MAX,
        wheel_speed: f32::MAX,
        hands_off: false,
        ts_mono_ns: u64::MAX,
        seq: u16::MAX,
    };
    let result = pipeline.process(&mut frame);
    // Pipeline should handle extreme values without panic; error is acceptable
    if let Ok(()) = result {
        assert!(
            frame.torque_out.is_finite() || frame.torque_out == f32::MAX,
            "Pipeline output should be finite or saturated"
        );
    }
    Ok(())
}

#[test]
fn test_ffb_pipeline_nan_input_does_not_propagate() -> Result<()> {
    let mut pipeline = Pipeline::new();

    let mut frame = Frame {
        ffb_in: f32::NAN,
        torque_out: f32::NAN,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };

    // Empty pipeline just passes through; the safety layer should catch NaN
    let _result = pipeline.process(&mut frame);
    // Verify the safety service would catch it
    let safety = SafetyService::new(5.0, 25.0);
    let safe_torque = safety.clamp_torque_nm(frame.torque_out);
    assert!(
        safe_torque.is_finite(),
        "Safety layer must prevent NaN from reaching output"
    );
    Ok(())
}

#[test]
fn test_ffb_virtual_device_write_extreme_torque() -> Result<()> {
    let id: DeviceId = "ffb-edge-001".parse()?;
    let mut device = VirtualDevice::new(id, "Edge Case Wheel".to_string());

    // Writing extreme torque values should not panic
    let extreme_values = [
        0.0f32,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -0.0,
    ];
    for (seq, val) in extreme_values.iter().enumerate() {
        let result = device.write_ffb_report(*val, seq as u16);
        // The device may reject invalid values with an error, but must not panic
        if result.is_ok() {
            // Verify the device is still functional
            assert!(
                device.is_connected(),
                "Device should remain connected after writing torque {}",
                val
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Section g: Concurrent telemetry parsing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_adapter_parsing() -> Result<()> {
    let factories = adapter_factories();
    let zero_packet = vec![0x00u8; 2048];
    let max_packet = vec![0xFFu8; 2048];

    let mut handles = Vec::new();

    for (id, factory) in factories {
        let id = *id;
        let zero = zero_packet.clone();
        let max = max_packet.clone();
        let adapter = factory();

        let handle = tokio::task::spawn_blocking(move || {
            // Parse zero packet
            let _ = adapter.normalize(&zero);
            // Parse max packet
            let _ = adapter.normalize(&max);
            // Parse truncated packet
            let _ = adapter.normalize(&[0xAB; 16]);
            // Parse empty packet
            let _ = adapter.normalize(&[]);
            id
        });
        handles.push(handle);
    }

    let mut errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(_id) => {}
            Err(e) => {
                // JoinError with panic payload indicates an adapter panicked
                errors.push(format!("Task failed: {}", e));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "Concurrent parsing failures: {:?}",
        errors
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_adapter_parsing_same_adapter() -> Result<()> {
    // Verify that creating multiple instances of the same adapter and parsing
    // concurrently is safe (no shared mutable state issues).
    let factories = adapter_factories();

    // Pick a few well-known adapters
    let target_ids = ["forza_motorsport", "acc", "iracing"];
    let packet = vec![0x00u8; 2048];

    let mut handles = Vec::new();

    for target_id in &target_ids {
        let entry = factories.iter().find(|(id, _)| id == target_id);
        if let Some((_, factory)) = entry {
            // Spawn multiple tasks for the same adapter type
            for i in 0u32..4 {
                let adapter = factory();
                let data = packet.clone();
                let tid = *target_id;
                let handle = tokio::task::spawn_blocking(move || {
                    for _ in 0..100 {
                        let _ = adapter.normalize(&data);
                    }
                    (tid, i)
                });
                handles.push(handle);
            }
        }
    }

    let mut errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(_) => {}
            Err(e) => errors.push(format!("Task failed: {}", e)),
        }
    }

    assert!(
        errors.is_empty(),
        "Concurrent same-adapter parsing failures: {:?}",
        errors
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_device_operations() -> Result<()> {
    // Verify that multiple virtual devices can operate concurrently
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "concurrent-a".parse()?;
    let id_b: DeviceId = "concurrent-b".parse()?;

    port.add_device(VirtualDevice::new(id_a.clone(), "Wheel A".to_string()))
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Wheel B".to_string()))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Operate both devices concurrently
    let handle_a = tokio::task::spawn_blocking(move || -> Result<()> {
        for i in 0u16..100 {
            let _ = dev_a.write_ffb_report(1.0, i);
            let _ = dev_a.read_telemetry();
        }
        Ok(())
    });

    let handle_b = tokio::task::spawn_blocking(move || -> Result<()> {
        for i in 0u16..100 {
            let _ = dev_b.write_ffb_report(-1.0, i);
            let _ = dev_b.read_telemetry();
        }
        Ok(())
    });

    handle_a
        .await
        .map_err(|e| anyhow::anyhow!("Device A task panicked: {}", e))??;
    handle_b
        .await
        .map_err(|e| anyhow::anyhow!("Device B task panicked: {}", e))??;

    Ok(())
}

// ---------------------------------------------------------------------------
// Additional edge-case: rapid sequential fault/clear cycles
// ---------------------------------------------------------------------------

#[test]
fn test_safety_rapid_fault_reporting_does_not_corrupt_state() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 25.0);

    let fault_types = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
    ];

    // Rapidly report different faults
    for fault in &fault_types {
        safety.report_fault(*fault);
        let clamped = safety.clamp_torque_nm(25.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Torque must be zero after {:?} fault, got {}",
            fault,
            clamped
        );
    }

    // State should still be faulted
    let final_clamp = safety.clamp_torque_nm(100.0);
    assert!(
        final_clamp.abs() < f32::EPSILON,
        "Torque must remain zero after multiple faults"
    );
    Ok(())
}
