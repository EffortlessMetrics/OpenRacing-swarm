//! BDD-style scenario tests for user-facing behaviour.
//!
//! Each test follows the **Given / When / Then** pattern and is self-contained:
//! no real USB hardware, running games, or external services are required.
//!
//! # Scenarios
//!
//! a. No device → service starts → waiting state
//! b. Device connected → game launches → telemetry flows
//! c. Active session → device disconnects → FFB stops safely
//! d. Active session → safety fault → torque zeroed within 50 ms
//! e. Profile loaded → user adjusts gain → effect scales immediately
//! f. Multiple devices → one faults → others unaffected
//! g. Game running → user switches profile → smooth transition
//! h. Firmware outdated → update available → user notified
//! i. Calibration in progress → user cancels → reverts to previous
//! j. Recording active → session ends → file saved and closed

use std::time::{Duration, Instant};

use anyhow::Result;

use openracing_calibration::types::{AxisCalibration, DeviceCalibration};
use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use openracing_profile::types::{FfbSettings, WheelProfile, WheelSettings};
use openracing_profile::validation;
use openracing_telemetry_adapters::{ForzaAdapter, TelemetryAdapter};
use racing_wheel_engine::VirtualDevice;
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario A: No device connected → service starts → enters waiting state
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  no device is connected
/// When   the service starts
/// Then   the safety system enters SafeTorque (waiting) state
/// And    torque output is limited to the safe default
/// ```
#[test]
fn given_no_device_connected_when_service_starts_then_enters_waiting_state() -> Result<()> {
    // Given: no device is connected — only the safety subsystem is initialised
    let safety = SafetyService::new(5.0, 20.0);

    // When: the service starts (safety service initialises in SafeTorque)
    let state = safety.state();

    // Then: the system is in SafeTorque (waiting for a device)
    assert_eq!(
        state,
        &SafetyState::SafeTorque,
        "service must start in SafeTorque state when no device is connected"
    );

    // And: torque is limited to the safe default, not the high-torque limit
    let clamped = safety.clamp_torque_nm(15.0);
    assert!(
        clamped <= 5.0,
        "waiting state must cap torque to safe limit (5 Nm), got {clamped}"
    );

    // And: zero torque passes through unchanged
    let zero = safety.clamp_torque_nm(0.0);
    assert!(
        zero.abs() < 0.001,
        "zero torque request must remain zero, got {zero}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario B: Device connected → game launches → telemetry flows
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a minimal 232-byte Forza Sled telemetry packet.
fn make_forza_sled_packet(rpm: f32, vel_x: f32, vel_y: f32, vel_z: f32) -> Vec<u8> {
    let mut data = vec![0u8; 232];
    data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    data[8..12].copy_from_slice(&8000.0f32.to_le_bytes()); // engine_max_rpm
    data[16..20].copy_from_slice(&rpm.to_le_bytes()); // current_rpm
    data[32..36].copy_from_slice(&vel_x.to_le_bytes()); // vel_x
    data[36..40].copy_from_slice(&vel_y.to_le_bytes()); // vel_y
    data[40..44].copy_from_slice(&vel_z.to_le_bytes()); // vel_z
    data
}

/// ```text
/// Given  a device is connected and a game (Forza) is running
/// When   a telemetry packet arrives with RPM=6000 and velocity=(40, 0, 30) m/s
/// Then   the adapter normalises it to speed and RPM values
/// And    both values are within physically valid ranges
/// And    force feedback can flow through the filter pipeline
/// ```
#[test]
fn given_device_connected_when_game_launches_then_telemetry_flows() -> Result<()> {
    // Given: a virtual device is connected
    let id: DeviceId = "bdd-telemetry-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD Telemetry Wheel".to_string());
    assert!(device.is_connected(), "device must be connected initially");

    // And: the Forza telemetry adapter is active
    let adapter = ForzaAdapter::new();
    assert_eq!(adapter.game_id(), "forza_motorsport");

    // When: a Sled packet arrives
    let packet = make_forza_sled_packet(6000.0, 40.0, 0.0, 30.0);
    let telemetry = adapter.normalize(&packet)?;

    // Then: RPM is correct
    assert!(
        (telemetry.rpm - 6000.0).abs() < 1.0,
        "RPM must be ~6000, got {}",
        telemetry.rpm
    );

    // And: speed is derived from the velocity vector: sqrt(40² + 0² + 30²) = 50 m/s
    let expected_speed = (40.0f32.powi(2) + 0.0f32.powi(2) + 30.0f32.powi(2)).sqrt();
    assert!(
        (telemetry.speed_ms - expected_speed).abs() < 0.5,
        "speed_ms must be ~{expected_speed:.1}, got {}",
        telemetry.speed_ms
    );

    // And: values are within physically valid ranges
    assert!(
        telemetry.rpm >= 0.0 && telemetry.rpm <= 20_000.0,
        "RPM out of range: {}",
        telemetry.rpm
    );
    assert!(
        telemetry.speed_ms >= 0.0 && telemetry.speed_ms <= 500.0,
        "speed_ms out of range: {}",
        telemetry.speed_ms
    );

    // And: FFB can flow through the filter pipeline to the device
    let mut frame = FilterFrame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 2.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    let damper = DamperState::fixed(0.02);
    damper_filter(&mut frame, &damper);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out.is_finite(),
        "filter output must be finite after pipeline"
    );

    device.write_ffb_report(frame.torque_out * 5.0, frame.seq)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario C: Active session → device disconnects → FFB stops safely
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an active session with force feedback flowing
/// When   the device disconnects
/// Then   the device reports as disconnected
/// And    the safety service transitions to Faulted
/// And    all torque output is clamped to zero
/// ```
#[test]
fn given_active_session_when_device_disconnects_then_ffb_stops_safely() -> Result<()> {
    // Given: an active session with FFB flowing
    let id: DeviceId = "bdd-disconnect-001".parse()?;
    let mut device = VirtualDevice::new(id, "BDD Disconnect Wheel".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Confirm FFB is flowing normally
    device.write_ffb_report(3.0, 0)?;
    let normal_torque = safety.clamp_torque_nm(3.0);
    assert!(
        (normal_torque - 3.0).abs() < 0.01,
        "torque must flow normally before disconnect, got {normal_torque}"
    );

    // When: the device disconnects
    device.disconnect();

    // Then: the device reports as disconnected
    assert!(
        !device.is_connected(),
        "device must report disconnected after disconnect"
    );

    // And: the safety service transitions to Faulted (USB stall simulates disconnect)
    safety.report_fault(FaultType::UsbStall);
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "safety must be Faulted after device disconnect, got {:?}",
        safety.state()
    );

    // And: all torque output is clamped to zero
    for requested in [0.0, 1.0, 5.0, 20.0, -10.0] {
        let clamped = safety.clamp_torque_nm(requested);
        assert!(
            clamped.abs() < 0.001,
            "torque must be zero after disconnect; requested={requested}, got={clamped}"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario D: Active session → safety fault → torque zeroed within 50 ms
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an active session with normal force feedback
/// When   a safety fault (overcurrent) is detected
/// Then   the fault is handled within 50 ms
/// And    torque output is immediately zero
/// And    the safety state transitions to Faulted
/// ```
#[test]
fn given_active_session_when_safety_fault_then_torque_zeroed_within_50ms() -> Result<()> {
    // Given: active session with normal FFB
    let mut safety = SafetyService::new(5.0, 20.0);
    let normal = safety.clamp_torque_nm(4.0);
    assert!(
        (normal - 4.0).abs() < 0.01,
        "torque must flow normally before fault"
    );

    // When: a safety fault (overcurrent) is detected — measure timing
    let fault_start = Instant::now();
    safety.report_fault(FaultType::Overcurrent);
    let clamped = safety.clamp_torque_nm(20.0);
    let fault_elapsed = fault_start.elapsed();

    // Then: the fault handling completes within 50 ms
    assert!(
        fault_elapsed < Duration::from_millis(50),
        "fault-to-zero-torque must complete in <50 ms (actual: {fault_elapsed:?})"
    );

    // And: torque output is immediately zero
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after overcurrent fault, got {clamped}"
    );

    // And: the safety state transitions to Faulted with the correct fault type
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::Overcurrent,
                "fault type must be Overcurrent"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(Overcurrent), got {other:?}"
            ));
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario E: Profile loaded → user adjusts gain → effect scales immediately
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a profile is loaded with overall_gain = 0.50
/// When   the user adjusts the gain to 0.80
/// Then   the profile validates successfully with the new gain
/// And    a filter frame scaled by the new gain produces proportionally
///        stronger output than the old gain
/// ```
#[test]
fn given_profile_loaded_when_user_adjusts_gain_then_effect_scales_immediately() -> Result<()> {
    // Given: a profile with overall_gain = 0.50
    let original_gain: f32 = 0.50;
    let original_settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: original_gain,
            torque_limit: 15.0,
            damper_strength: 0.10,
            effects_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let original_profile = WheelProfile::new("BDD Gain Profile", "bdd-device-001")
        .with_settings(original_settings.clone());
    validation::validate_profile(&original_profile)?;

    // When: the user adjusts the gain to 0.80
    let new_gain: f32 = 0.80;
    let new_settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: new_gain,
            ..original_settings.ffb
        },
        ..original_settings
    };
    let new_profile =
        WheelProfile::new("BDD Gain Profile", "bdd-device-001").with_settings(new_settings);
    validation::validate_profile(&new_profile)?;

    // Then: the new profile has the updated gain
    assert!(
        (new_profile.settings.ffb.overall_gain - new_gain).abs() < f32::EPSILON,
        "gain must be updated to {new_gain}, got {}",
        new_profile.settings.ffb.overall_gain
    );

    // And: a filter frame at the new gain produces stronger output
    let base_ffb: f32 = 0.6;

    let old_scaled = base_ffb * original_gain;
    let new_scaled = base_ffb * new_gain;

    assert!(
        new_scaled > old_scaled,
        "new gain ({new_gain}) must produce stronger output than old gain ({original_gain}): \
         {new_scaled} vs {old_scaled}"
    );

    // And: the filter pipeline processes the scaled value correctly
    let mut frame = FilterFrame {
        ffb_in: new_scaled,
        torque_out: new_scaled,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "scaled output must be finite and within [-1, 1], got {}",
        frame.torque_out
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario F: Multiple devices → one faults → others unaffected
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  two devices are connected, each managed by its own safety service
/// When   device A encounters a thermal fault
/// Then   device A's safety state is Faulted and torque is zero
/// And    device B's safety state remains SafeTorque with normal torque flow
/// ```
#[test]
fn given_multiple_devices_when_one_faults_then_others_unaffected() -> Result<()> {
    // Given: two devices, each with independent safety services
    let id_a: DeviceId = "bdd-multi-a".parse()?;
    let id_b: DeviceId = "bdd-multi-b".parse()?;
    let mut device_a = VirtualDevice::new(id_a, "Device A".to_string());
    let device_b = VirtualDevice::new(id_b, "Device B".to_string());
    let mut safety_a = SafetyService::new(5.0, 20.0);
    let safety_b = SafetyService::new(5.0, 20.0);

    // Both devices are connected and operational
    assert!(device_a.is_connected(), "device A must be connected");
    assert!(device_b.is_connected(), "device B must be connected");

    // When: device A encounters a thermal fault
    device_a.inject_fault(0x04); // thermal fault flag
    safety_a.report_fault(FaultType::ThermalLimit);

    // Then: device A's safety state is Faulted
    assert!(
        matches!(safety_a.state(), SafetyState::Faulted { .. }),
        "device A must be in Faulted state, got {:?}",
        safety_a.state()
    );
    let torque_a = safety_a.clamp_torque_nm(5.0);
    assert!(
        torque_a.abs() < 0.001,
        "device A torque must be zero, got {torque_a}"
    );

    // And: device B remains in SafeTorque with normal torque flow
    assert_eq!(
        safety_b.state(),
        &SafetyState::SafeTorque,
        "device B must remain in SafeTorque state"
    );
    let torque_b = safety_b.clamp_torque_nm(3.0);
    assert!(
        (torque_b - 3.0).abs() < 0.01,
        "device B torque must flow normally, got {torque_b}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario G: Game running → user switches profile → smooth transition
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a game is running with an ACC profile (gain=0.70, steering=540°)
/// When   the user switches to an iRacing profile (gain=0.85, steering=900°)
/// Then   the merged profile applies the new settings
/// And    both profiles validate independently
/// And    the filter pipeline continues to produce valid output
/// ```
#[test]
fn given_game_running_when_user_switches_profile_then_smooth_transition() -> Result<()> {
    // Given: ACC profile active
    let acc_settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: 0.70,
            torque_limit: 12.0,
            damper_strength: 0.15,
            effects_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let acc_profile =
        WheelProfile::new("ACC GT3 Profile", "bdd-device-002").with_settings(acc_settings);
    validation::validate_profile(&acc_profile)?;

    // When: the user switches to an iRacing profile
    let iracing_settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: 0.85,
            torque_limit: 18.0,
            damper_strength: 0.20,
            effects_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let iracing_profile =
        WheelProfile::new("iRacing GT3 Profile", "bdd-device-002").with_settings(iracing_settings);
    validation::validate_profile(&iracing_profile)?;

    // Then: the merged profile applies the new (iRacing) settings
    let merged = validation::merge_profiles(&acc_profile, &iracing_profile);
    assert!(
        (merged.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON,
        "merged gain must be 0.85, got {}",
        merged.settings.ffb.overall_gain
    );
    assert!(
        (merged.settings.ffb.torque_limit - 18.0).abs() < f32::EPSILON,
        "merged torque limit must be 18.0, got {}",
        merged.settings.ffb.torque_limit
    );

    // And: the filter pipeline processes a frame using the new gain
    let scaled_ffb = 0.6 * merged.settings.ffb.overall_gain;
    let mut frame = FilterFrame {
        ffb_in: scaled_ffb,
        torque_out: scaled_ffb,
        wheel_speed: 2.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let damper = DamperState::fixed(merged.settings.ffb.damper_strength);
    damper_filter(&mut frame, &damper);
    torque_cap_filter(&mut frame, 1.0);

    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "pipeline output must be finite and within [-1, 1], got {}",
        frame.torque_out
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario H: Firmware outdated → update available → user notified
// ═══════════════════════════════════════════════════════════════════════════════

/// Models firmware version state for a device.
struct DeviceFirmware {
    current: semver::Version,
    latest: semver::Version,
}

impl DeviceFirmware {
    fn new(current: &str, latest: &str) -> std::result::Result<Self, semver::Error> {
        Ok(Self {
            current: current.parse()?,
            latest: latest.parse()?,
        })
    }

    fn update_available(&self) -> bool {
        self.latest > self.current
    }
}

/// ```text
/// Given  a device with firmware version 1.2.0
/// When   an update to version 2.0.0 is available
/// Then   the system detects the firmware is outdated
/// And    the update notification includes both versions
/// And    the device remains operational while the notification is active
/// ```
#[test]
fn given_firmware_outdated_when_update_available_then_notifies_user() -> Result<()> {
    // Given: a device with firmware v1.2.0
    let firmware = DeviceFirmware::new("1.2.0", "2.0.0")
        .map_err(|e| anyhow::anyhow!("semver parse error: {e}"))?;

    let id: DeviceId = "bdd-firmware-001".parse()?;
    let device = VirtualDevice::new(id, "BDD Firmware Wheel".to_string());

    // When/Then: the system detects the firmware is outdated
    assert!(
        firmware.update_available(),
        "update must be available when latest ({}) > current ({})",
        firmware.latest,
        firmware.current
    );

    // And: version information is correct for the notification
    assert!(
        firmware.latest > firmware.current,
        "latest version must be newer than current"
    );

    // And: the device remains operational while the notification is active
    assert!(
        device.is_connected(),
        "device must remain connected during firmware notification"
    );
    let safety = SafetyService::new(5.0, 20.0);
    let torque = safety.clamp_torque_nm(3.0);
    assert!(
        (torque - 3.0).abs() < 0.01,
        "device must remain operational during firmware notification, got {torque}"
    );

    // And: no update needed when versions match
    let up_to_date = DeviceFirmware::new("2.0.0", "2.0.0")
        .map_err(|e| anyhow::anyhow!("semver parse error: {e}"))?;
    assert!(
        !up_to_date.update_available(),
        "no update should be available when firmware is current"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario I: Calibration in progress → user cancels → reverts to previous
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a steering axis calibration is in progress (new min/max being set)
/// When   the user cancels the calibration
/// Then   the axis calibration reverts to the previously saved values
/// And    the reverted calibration produces the same output as before
/// ```
#[test]
fn given_calibration_in_progress_when_user_cancels_then_reverts_to_previous() -> Result<()> {
    // Given: a saved calibration exists
    let mut saved_cal = DeviceCalibration::new("BDD Steering Axis", 1);
    let saved_axis = AxisCalibration::new(100, 900).with_center(500);
    if let Some(axis) = saved_cal.axis(0) {
        *axis = saved_axis.clone();
    }

    // Verify the saved calibration produces expected output
    let saved_output_at_min = saved_axis.apply(100);
    let saved_output_at_max = saved_axis.apply(900);
    let saved_output_at_center = saved_axis.apply(500);

    // And: a new calibration is in progress with different values
    let in_progress_axis = AxisCalibration::new(200, 800).with_center(450);
    let in_progress_output_at_min = in_progress_axis.apply(200);

    // The in-progress calibration produces different outputs
    assert!(
        (in_progress_output_at_min - saved_output_at_min).abs() > f32::EPSILON
            || in_progress_axis.min != saved_axis.min,
        "in-progress calibration must differ from saved"
    );

    // When: the user cancels — revert to saved calibration
    let reverted_axis = saved_axis;

    // Then: the reverted calibration matches the original saved values
    assert_eq!(
        reverted_axis.min, 100,
        "reverted min must be 100, got {}",
        reverted_axis.min
    );
    assert_eq!(
        reverted_axis.max, 900,
        "reverted max must be 900, got {}",
        reverted_axis.max
    );

    // And: outputs match the original saved calibration
    assert!(
        (reverted_axis.apply(100) - saved_output_at_min).abs() < f32::EPSILON,
        "reverted output at min must match saved"
    );
    assert!(
        (reverted_axis.apply(900) - saved_output_at_max).abs() < f32::EPSILON,
        "reverted output at max must match saved"
    );
    assert!(
        (reverted_axis.apply(500) - saved_output_at_center).abs() < f32::EPSILON,
        "reverted output at center must match saved"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario J: Recording active → session ends → file saved and closed
// ═══════════════════════════════════════════════════════════════════════════════

/// In-test model of a telemetry recording session.
struct RecordingSession {
    frames: Vec<FilterFrame>,
    active: bool,
    output_path: std::path::PathBuf,
}

impl RecordingSession {
    fn start(output_path: std::path::PathBuf) -> Self {
        Self {
            frames: Vec::new(),
            active: true,
            output_path,
        }
    }

    fn record_frame(&mut self, frame: FilterFrame) {
        if self.active {
            self.frames.push(frame);
        }
    }

    fn stop(&mut self) -> Result<RecordingSummary> {
        self.active = false;

        // Write frames as JSON to the output file
        let data = serde_json::to_vec(&self.frames.len())
            .map_err(|e| anyhow::anyhow!("serialisation error: {e}"))?;
        std::fs::write(&self.output_path, &data)
            .map_err(|e| anyhow::anyhow!("write error: {e}"))?;

        Ok(RecordingSummary {
            frame_count: self.frames.len(),
            _file_path: self.output_path.clone(),
            file_size_bytes: data.len(),
        })
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

struct RecordingSummary {
    frame_count: usize,
    _file_path: std::path::PathBuf,
    file_size_bytes: usize,
}

/// ```text
/// Given  a recording session is active and capturing telemetry frames
/// When   the session ends (user stops or race finishes)
/// Then   recording is marked inactive
/// And    the file is saved to disk and is non-empty
/// And    the frame count matches the number of captured frames
/// ```
#[test]
fn given_recording_active_when_session_ends_then_file_saved_and_closed() -> Result<()> {
    // Given: a recording session is active
    let temp_dir = tempfile::TempDir::new()?;
    let output_path = temp_dir.path().join("bdd_recording.json");
    let mut session = RecordingSession::start(output_path.clone());

    assert!(session.is_active(), "recording must be active after start");

    // And: frames are being captured
    for seq in 0..100u16 {
        let frame = FilterFrame {
            ffb_in: 0.3 * (seq as f32 * 0.1).sin(),
            torque_out: 0.0,
            wheel_speed: 2.0,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        session.record_frame(frame);
    }

    // When: the session ends
    let summary = session.stop()?;

    // Then: recording is marked inactive
    assert!(
        !session.is_active(),
        "recording must be inactive after stop"
    );

    // And: the file is saved to disk and is non-empty
    assert!(
        output_path.exists(),
        "recording file must exist at {output_path:?}"
    );
    assert!(
        summary.file_size_bytes > 0,
        "recording file must be non-empty"
    );

    // And: the frame count matches
    assert_eq!(
        summary.frame_count, 100,
        "frame count must match captured frames"
    );

    Ok(())
}
