//! BDD-style acceptance tests for key user journeys.
//!
//! Each scenario follows the **Given / When / Then** pattern and is mock-based:
//! no real USB hardware is required.  Virtual device implementations record every
//! byte sent to them so assertions can verify wire-level behaviour.
//!
//! # Features covered
//!
//! * **Plug and Play Device Support** – devices are auto-detected and their
//!   vendor protocols selected when plugged in.
//! * **Game Telemetry Auto-Configure** – OpenRacing writes a telemetry config
//!   on first launch and preserves the existing config on subsequent launches.
//! * **Real-Time Safety Interlocks** – fault conditions trigger motor shutdown
//!   within defined timing budgets.

use racing_wheel_engine::policies::SafetyPolicy;
use racing_wheel_hid_fanatec_protocol::{
    ids::report_ids as fanatec_report_ids, product_ids as fanatec_product_ids,
};
use racing_wheel_hid_moza_protocol::{MozaInitState, product_ids as moza_product_ids};
use racing_wheel_integration_tests::fanatec_virtual::FanatecScenario;
use racing_wheel_integration_tests::moza_virtual::MozaScenario;
use racing_wheel_telemetry_config_writers::{ConfigWriter, IRacingConfigWriter, TelemetryConfig};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Returns a [`TelemetryConfig`] that enables iRacing telemetry at 60 Hz.
fn iracing_telemetry_config() -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "disk".to_string(),
        output_target: String::new(),
        fields: vec!["speed".to_string(), "rpm".to_string(), "gear".to_string()],
        enable_high_rate_iracing_360hz: false,
    }
}

/// Models the communication-timeout logic described in the safety specification.
///
/// The real-time loop tracks `last_received_at` and calls
/// `motor_must_be_disabled` on every tick.  When `silence_duration` meets
/// or exceeds `timeout`, the loop stops torque output and raises a fault.
struct CommTimeoutChecker {
    timeout: Duration,
}

impl CommTimeoutChecker {
    fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Returns `true` when the elapsed silence meets or exceeds the configured
    /// communication timeout, meaning the motor output must be zeroed.
    fn motor_must_be_disabled(&self, silence_duration: Duration) -> bool {
        silence_duration >= self.timeout
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Plug and Play Device Support
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: User plugs in a Moza wheel
///
/// ```text
/// Given  no wheel is connected
/// When   the user plugs in a Moza R9 wheel (VID 0x346E)
/// Then   the device is automatically detected
/// And    the Moza protocol is selected
/// And    force feedback is available
/// ```
#[test]
fn scenario_moza_r9_wheel_plugged_in_protocol_selected_and_ffb_available()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: no wheel is connected – protocol not yet initialised
    let mut scenario = MozaScenario::wheelbase(moza_product_ids::R9_V2);
    assert!(
        !scenario.protocol.is_ffb_ready(),
        "FFB must not be available before the device is initialised"
    );
    assert_eq!(
        scenario.protocol.init_state(),
        MozaInitState::Uninitialized,
        "protocol must start in Uninitialized state"
    );

    // When: the Moza R9 is plugged in and the protocol handshake runs
    scenario.initialize()?;

    // Then: the device is automatically detected (handshake completed)
    assert_eq!(
        scenario.protocol.init_state(),
        MozaInitState::Ready,
        "Moza R9 must reach Ready state after plug-in"
    );

    // And: the Moza protocol is selected (handshake feature reports were sent)
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "Moza protocol handshake must send feature reports to the device"
    );

    // And: force feedback is available
    assert!(
        scenario.protocol.is_ffb_ready(),
        "FFB must be available after successful Moza R9 initialisation"
    );

    Ok(())
}

/// Scenario: User plugs in a Fanatec wheel
///
/// ```text
/// Given  no wheel is connected
/// When   the user plugs in a Fanatec ClubSport wheel (VID 0x0EB7, PID 0x0004)
/// Then   the device is automatically detected
/// And    the Fanatec protocol is selected
/// ```
#[test]
fn scenario_fanatec_clubsport_wheel_plugged_in_protocol_selected()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: no Fanatec wheel is connected (ClubSport V2.5 – PID 0x0004)
    let mut scenario = FanatecScenario::wheelbase(fanatec_product_ids::CLUBSPORT_V2_5);
    assert!(
        scenario.device.feature_reports().is_empty(),
        "no feature reports expected before initialisation"
    );

    // When: the Fanatec ClubSport is plugged in
    scenario.initialize()?;

    // Then: the device is automatically detected and the Fanatec protocol is
    //       selected – a MODE_SWITCH report must have been written to the device
    assert!(
        scenario
            .device
            .sent_feature_report_id(fanatec_report_ids::MODE_SWITCH),
        "Fanatec protocol must send a MODE_SWITCH report to select the correct \
         device operating mode"
    );

    Ok(())
}

/// Scenario: Device plug-in recovers after a transient failure
///
/// ```text
/// Given  a Moza wheel with a transient I/O error on first connect
/// When   the error is cleared and the device is retried
/// Then   force feedback becomes available
/// ```
#[test]
fn scenario_moza_plug_in_failure_recovers_on_retry() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a Moza R9 with a simulated I/O error on first plug-in
    let mut scenario = MozaScenario::wheelbase_failing(moza_product_ids::R9_V2);

    // When: first initialisation attempt fails (cable/USB error)
    assert!(
        scenario.initialize().is_err(),
        "transient write failure must surface while leaving retry state"
    );
    assert_eq!(
        scenario.protocol.init_state(),
        MozaInitState::Failed,
        "state must be Failed after a transient I/O error"
    );
    assert!(
        scenario.protocol.can_retry(),
        "device must be eligible for a retry after a transient failure"
    );

    // And: the error clears (user re-seats the cable)
    scenario.device.reconnect();

    // When: the protocol retries
    scenario.initialize()?;

    // Then: force feedback is now available
    assert_eq!(
        scenario.protocol.init_state(),
        MozaInitState::Ready,
        "Moza device must reach Ready state after a successful retry"
    );
    assert!(
        scenario.protocol.is_ffb_ready(),
        "FFB must be available after the device recovers"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Game Telemetry Auto-Configure
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: User launches iRacing for the first time
///
/// ```text
/// Given  OpenRacing is running
/// And    iRacing has never been configured
/// When   iRacing process starts
/// Then   the iRacing telemetry config file is written
/// And    the telemetry adapter starts automatically
/// ```
#[test]
fn scenario_iracing_first_launch_writes_config_and_adapter_starts()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: OpenRacing is running, iRacing directory is empty (never configured)
    let game_dir = TempDir::new()?;
    let writer = IRacingConfigWriter;
    let config = iracing_telemetry_config();

    let initially_configured = writer.validate_config(game_dir.path())?;
    assert!(
        !initially_configured,
        "iRacing must not appear configured before the first launch"
    );

    // When: iRacing starts and OpenRacing writes the telemetry config
    let diffs = writer.write_config(game_dir.path(), &config)?;

    // Then: the iRacing telemetry config file is written
    assert!(
        !diffs.is_empty(),
        "first-time configuration must produce at least one config diff"
    );

    let app_ini = game_dir.path().join("Documents/iRacing/app.ini");
    assert!(
        app_ini.exists(),
        "iRacing app.ini must exist after first-launch configuration"
    );

    // And: the telemetry adapter starts automatically (config validates as active)
    let is_configured = writer.validate_config(game_dir.path())?;
    assert!(
        is_configured,
        "telemetry adapter must be startable after writing the iRacing config"
    );

    Ok(())
}

/// Scenario: User launches iRacing after previous config
///
/// ```text
/// Given  iRacing has been previously configured
/// When   iRacing process starts
/// Then   the existing config is not overwritten
/// And    the telemetry adapter starts
/// ```
#[test]
fn scenario_iracing_subsequent_launch_preserves_config() -> Result<(), Box<dyn std::error::Error>> {
    // Given: iRacing was previously configured and the user has added their own
    //        custom [Video] settings to app.ini (simulating real-world customisation)
    let game_dir = TempDir::new()?;
    let iracing_dir = game_dir.path().join("Documents").join("iRacing");
    std::fs::create_dir_all(&iracing_dir)?;

    let app_ini = iracing_dir.join("app.ini");
    std::fs::write(
        &app_ini,
        "[Video]\nresolution=2560x1440\n\n[Telemetry]\ntelemetryDiskFile=1\n",
    )?;

    let writer = IRacingConfigWriter;
    let config = iracing_telemetry_config();

    // Verify the pre-existing config is recognised as valid
    let pre_configured = writer.validate_config(game_dir.path())?;
    assert!(
        pre_configured,
        "pre-existing iRacing config must be recognised as already configured"
    );

    // When: iRacing starts again and OpenRacing re-applies the telemetry config
    writer.write_config(game_dir.path(), &config)?;

    // Then: the existing [Video] settings are not overwritten
    let final_content = std::fs::read_to_string(&app_ini)?;
    assert!(
        final_content.contains("[Video]"),
        "custom [Video] section must be preserved after re-configuration"
    );
    assert!(
        final_content.contains("resolution=2560x1440"),
        "custom display resolution must not be overwritten"
    );
    assert!(
        final_content.contains("telemetryDiskFile=1"),
        "telemetry setting must still be present after re-configuration"
    );

    // And: the telemetry adapter starts
    let still_configured = writer.validate_config(game_dir.path())?;
    assert!(
        still_configured,
        "telemetry adapter must start on subsequent iRacing launch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Real-Time Safety Interlocks
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: Motor overcurrent detected
///
/// ```text
/// Given  the motor is running at 50% torque
/// When   an overcurrent event is detected
/// Then   the motor is stopped within 50ms
/// And    a safety fault is logged
/// ```
#[test]
fn scenario_overcurrent_triggers_motor_stop_within_50ms() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: the motor is running (no active faults)
    let policy = SafetyPolicy::new()?;
    assert!(
        !policy.requires_immediate_shutdown(0x00),
        "motor must not stop when there are no fault flags"
    );

    // When: an overcurrent event is detected (fault flag bit 3 = 0x08)
    let overcurrent_fault: u8 = 0x08;
    let check_start = Instant::now();
    let must_stop = policy.requires_immediate_shutdown(overcurrent_fault);
    let check_elapsed = check_start.elapsed();

    // Then: the motor is stopped
    assert!(
        must_stop,
        "overcurrent fault (0x08) must trigger immediate motor shutdown"
    );

    // And: the safety check completes well within the 50ms stop budget,
    //      leaving the real-time loop time to ramp torque to zero
    assert!(
        check_elapsed < Duration::from_millis(50),
        "safety fault detection must complete in <50ms (actual: {check_elapsed:?})"
    );

    Ok(())
}

/// Scenario: All critical fault types trigger immediate motor shutdown
///
/// ```text
/// Given  the motor is running
/// When   any critical fault is detected (USB / encoder / thermal / overcurrent)
/// Then   the motor is stopped immediately
/// ```
#[test]
fn scenario_all_critical_faults_trigger_motor_stop() -> Result<(), Box<dyn std::error::Error>> {
    let policy = SafetyPolicy::new()?;

    let critical_faults: &[(u8, &str)] = &[
        (0x01, "USB fault"),
        (0x02, "encoder fault"),
        (0x04, "thermal fault"),
        (0x08, "overcurrent fault"),
    ];

    for &(fault_flag, fault_name) in critical_faults {
        // When: each individual critical fault is raised
        let must_stop = policy.requires_immediate_shutdown(fault_flag);

        // Then: immediate shutdown is required
        assert!(
            must_stop,
            "{fault_name} (0x{fault_flag:02X}) must trigger an immediate motor shutdown"
        );
    }

    Ok(())
}

/// Scenario: Non-critical (plugin) faults do not trigger immediate shutdown
///
/// ```text
/// Given  the motor is running
/// When   a plugin fault (non-critical) is detected
/// Then   the motor continues running (handled gracefully, not stopped immediately)
/// ```
#[test]
fn scenario_plugin_fault_does_not_stop_motor() -> Result<(), Box<dyn std::error::Error>> {
    // Given: the motor is running
    let policy = SafetyPolicy::new()?;

    // When: a plugin fault is detected (bit 4 = 0x10, non-critical)
    let plugin_fault: u8 = 0x10;
    let must_stop = policy.requires_immediate_shutdown(plugin_fault);

    // Then: the motor is NOT stopped immediately
    //       (plugin faults are handled gracefully at a higher layer)
    assert!(
        !must_stop,
        "plugin fault (0x10) must NOT trigger an immediate motor shutdown"
    );

    Ok(())
}

/// Scenario: Communication timeout
///
/// ```text
/// Given  the wheel is connected and active
/// When   no communication is received for 500ms
/// Then   the motor is disabled
/// And    a timeout fault is raised
/// ```
#[test]
fn scenario_communication_timeout_disables_motor() -> Result<(), Box<dyn std::error::Error>> {
    // Given: the wheel is connected with a 500ms communication timeout (per spec)
    let checker = CommTimeoutChecker::new(Duration::from_millis(500));

    // Then: motor must NOT be disabled while communication is healthy
    assert!(
        !checker.motor_must_be_disabled(Duration::ZERO),
        "motor must not be disabled at t=0ms (active communication)"
    );
    assert!(
        !checker.motor_must_be_disabled(Duration::from_millis(499)),
        "motor must not be disabled at t=499ms (still within timeout window)"
    );

    // When: no communication has been received for exactly 500ms
    // Then: the motor is disabled
    assert!(
        checker.motor_must_be_disabled(Duration::from_millis(500)),
        "motor must be disabled once the 500ms communication timeout is reached"
    );
    assert!(
        checker.motor_must_be_disabled(Duration::from_millis(750)),
        "motor must remain disabled beyond the timeout threshold"
    );

    // And: the resulting USB/communication fault triggers the safety policy
    let policy = SafetyPolicy::new()?;
    let comm_fault: u8 = 0x01; // USB / communication fault flag
    assert!(
        policy.requires_immediate_shutdown(comm_fault),
        "USB/communication fault raised by a timeout must trigger the safety policy"
    );

    Ok(())
}
