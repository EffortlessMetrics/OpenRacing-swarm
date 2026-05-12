//! Snapshot tests for CLI help text, error output, and status display.
//!
//! Uses `assert_cmd` to invoke the binary and capture output for snapshots.

#![allow(deprecated)]

/// Normalize binary name across platforms (strip `.exe` suffix from usage strings)
fn normalize_cli_output(s: &str) -> String {
    s.replace("wheelctl.exe", "wheelctl")
}

// ---------------------------------------------------------------------------
// CLI help text snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_cli_help_text() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.arg("--help").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_help_text", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_device_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["device", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_device_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_diag_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["diag", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_diag_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_safety_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["safety", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_safety_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_hardware_doctor_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["hardware", "doctor", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_hardware_doctor_help", stdout);
    Ok(())
}

// ---------------------------------------------------------------------------
// Error output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_cli_unknown_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.arg("nonexistent").output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = normalize_cli_output(&stderr);
    insta::assert_snapshot!("cli_unknown_subcommand_error", stderr);
    Ok(())
}

#[test]
fn snapshot_cli_missing_args() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = normalize_cli_output(&stderr);
    insta::assert_snapshot!("cli_missing_args_error", stderr);
    Ok(())
}

// ---------------------------------------------------------------------------
// Status display snapshots (using serde types that mirror CLI output)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_device_status_json() {
    let status = serde_json::json!({
        "success": true,
        "device": {
            "id": "wheel-001",
            "name": "Fanatec DD Pro",
            "device_type": "WheelBase",
            "state": "Connected",
            "capabilities": {
                "supports_pid": true,
                "supports_raw_torque_1khz": true,
                "supports_health_stream": true,
                "supports_led_bus": true,
                "max_torque_nm": 8.0,
                "encoder_cpr": 2048,
                "min_report_period_us": 1000
            }
        },
        "telemetry": {
            "wheel_angle_deg": 0.0,
            "wheel_speed_rad_s": 0.0,
            "temperature_c": 45,
            "fault_flags": 0,
            "hands_on": true
        },
        "active_faults": []
    });
    insta::assert_json_snapshot!("device_status_json", status);
}

#[test]
fn snapshot_diagnostics_json() {
    let diag = serde_json::json!({
        "success": true,
        "device_id": "wheel-001",
        "system_info": {
            "os": "windows",
            "arch": "x86_64"
        },
        "performance": {
            "p99_jitter_us": 0.15,
            "missed_tick_rate": 0.0001,
            "total_ticks": 1000000,
            "missed_ticks": 1
        },
        "recent_faults": []
    });
    insta::assert_json_snapshot!("diagnostics_json", diag);
}

#[test]
fn snapshot_game_status_json() {
    let game = serde_json::json!({
        "success": true,
        "active_game": "iracing",
        "telemetry_active": true,
        "car_id": "gt3",
        "track_id": "spa"
    });
    insta::assert_json_snapshot!("game_status_json", game);
}

#[test]
fn snapshot_health_event_json() {
    let event = serde_json::json!({
        "device_id": "wheel-001",
        "event_type": "PerformanceWarning",
        "message": "High jitter detected",
        "metadata": {}
    });
    insta::assert_json_snapshot!("health_event_json", event);
}

#[test]
fn snapshot_error_json_format() {
    let error_output = serde_json::json!({
        "success": false,
        "error": {
            "message": "Device not found: wheel-999",
            "type": "DeviceNotFound"
        }
    });
    insta::assert_json_snapshot!("error_json_format", error_output);
}

// ---------------------------------------------------------------------------
// Expanded help-text snapshots for remaining subcommand groups
// ---------------------------------------------------------------------------

#[test]
fn snapshot_cli_profile_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["profile", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_profile_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_plugin_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["plugin", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_plugin_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_game_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["game", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_game_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_telemetry_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["telemetry", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_telemetry_help", stdout);
    Ok(())
}

#[test]
fn snapshot_cli_health_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["health", "--help"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = normalize_cli_output(&stdout);
    insta::assert_snapshot!("cli_health_help", stdout);
    Ok(())
}

// ---------------------------------------------------------------------------
// Error message snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_cli_device_missing_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["device"]).output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = normalize_cli_output(&stderr);
    insta::assert_snapshot!("cli_device_missing_subcommand", stderr);
    Ok(())
}

#[test]
fn snapshot_cli_profile_missing_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd.args(["profile"]).output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = normalize_cli_output(&stderr);
    insta::assert_snapshot!("cli_profile_missing_subcommand", stderr);
    Ok(())
}

#[test]
fn snapshot_cli_invalid_calibration_type() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = assert_cmd::Command::cargo_bin("wheelctl")?;
    let output = cmd
        .args(["device", "calibrate", "wheel-001", "bogus"])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = normalize_cli_output(&stderr);
    insta::assert_snapshot!("cli_invalid_calibration_type", stderr);
    Ok(())
}

#[test]
fn snapshot_validation_error_json() {
    let error_output = serde_json::json!({
        "success": false,
        "error": {
            "message": "Validation error: ffbGain must be between 0.0 and 1.0",
            "type": "ValidationError"
        }
    });
    insta::assert_json_snapshot!("validation_error_json", error_output);
}

#[test]
fn snapshot_service_unavailable_error_json() {
    let error_output = serde_json::json!({
        "success": false,
        "error": {
            "message": "Service unavailable: Connection refused",
            "type": "ServiceUnavailable"
        }
    });
    insta::assert_json_snapshot!("service_unavailable_error_json", error_output);
}
