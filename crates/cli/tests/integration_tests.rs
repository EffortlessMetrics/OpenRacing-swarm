//! Integration tests for wheelctl CLI
//!
//! These tests cover all major command workflows with error code validation
//! as required by the task specification.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

// Test helpers for Result and Option values
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("must failed: {:?}", e),
    }
}

fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("must_some failed: {}", msg),
    }
}

/// Custom predicate to check if output is valid JSON
fn is_json() -> impl predicates::Predicate<[u8]> {
    predicates::function::function(|s: &[u8]| {
        if let Ok(text) = std::str::from_utf8(s) {
            serde_json::from_str::<Value>(text).is_ok()
        } else {
            false
        }
    })
}

/// Test helper to create a wheelctl command
fn wheelctl() -> Command {
    #[allow(deprecated)]
    must(Command::cargo_bin("wheelctl"))
}

/// Test helper to create temporary profile
fn create_test_profile(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let profile = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "iracing",
            "car": "gt3"
        },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 540,
            "torqueCapNm": 8.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.12,
                "damper": 0.18,
                "inertia": 0.08,
                "notchFilters": [],
                "slewRate": 0.85,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    });

    let path = dir.path().join(format!("{}.json", name));
    must(fs::write(
        &path,
        must(serde_json::to_string_pretty(&profile)),
    ));
    path
}

#[test]
fn test_cli_help() {
    wheelctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Racing Wheel Software Suite"));
}

#[test]
fn test_cli_version() {
    wheelctl()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("wheelctl"));
}

#[test]
fn test_completion_generation() {
    wheelctl()
        .args(["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_wheelctl"));
}

// Device Management Tests

#[test]
fn test_device_list_human_output() {
    wheelctl()
        .args(["device", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Connected Devices"));
}

#[test]
fn test_device_list_json_output() {
    wheelctl()
        .args(["--json", "device", "list"])
        .assert()
        .success()
        .stdout(is_json());

    // Verify JSON structure
    let output = must(wheelctl().args(["--json", "device", "list"]).output());

    let json: Value = must(serde_json::from_slice(&output.stdout));
    assert_eq!(json["success"], true);
    assert!(json["devices"].is_array());
}

#[test]
fn test_device_list_detailed() {
    wheelctl()
        .args(["device", "list", "--detailed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Capabilities"));
}

#[test]
fn test_device_status() {
    wheelctl()
        .args(["device", "status", "wheel-001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Device:"));
}

#[test]
fn test_device_status_json() {
    wheelctl()
        .args(["--json", "device", "status", "wheel-001"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_device_not_found_error() {
    wheelctl()
        .args(["device", "status", "nonexistent-device"])
        .assert()
        .failure()
        .code(2); // Device not found error code
}

#[test]
fn test_device_calibrate() {
    wheelctl()
        .args(["device", "calibrate", "wheel-001", "center", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("calibration"));
}

#[test]
fn test_device_reset() {
    wheelctl()
        .args(["device", "reset", "wheel-001", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("reset"));
}

// Profile Management Tests

#[test]
fn test_profile_list() {
    wheelctl().args(["profile", "list"]).assert().success();
}

#[test]
fn test_profile_list_json() {
    wheelctl()
        .args(["--json", "profile", "list"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_profile_create_and_validate() {
    let temp_dir = must(TempDir::new());
    let profile_path = temp_dir.path().join("test_profile.json");

    // Create profile
    wheelctl()
        .args([
            "profile",
            "create",
            must_some(profile_path.to_str(), "expected path to_str"),
            "--game",
            "iracing",
            "--car",
            "gt3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile created"));

    // Validate profile
    wheelctl()
        .args([
            "profile",
            "validate",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_profile_show() {
    let temp_dir = must(TempDir::new());
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args([
            "profile",
            "show",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile Schema"));
}

#[test]
fn test_profile_show_json() {
    let temp_dir = must(TempDir::new());
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args([
            "--json",
            "profile",
            "show",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_profile_apply() {
    let temp_dir = must(TempDir::new());
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args([
            "profile",
            "apply",
            "wheel-001",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("applied"));
}

#[test]
fn test_profile_not_found_error() {
    wheelctl()
        .args(["profile", "show", "nonexistent.json"])
        .assert()
        .failure()
        .code(3); // Profile not found error code
}

#[test]
fn test_profile_validation_error() {
    let temp_dir = must(TempDir::new());
    let invalid_profile = temp_dir.path().join("invalid.json");
    must(fs::write(&invalid_profile, "{ invalid json }"));

    wheelctl()
        .args([
            "profile",
            "validate",
            must_some(invalid_profile.to_str(), "expected path to_str"),
        ])
        .assert()
        .failure()
        .code(4); // Validation error code
}

#[test]
fn test_profile_edit_field() {
    let temp_dir = must(TempDir::new());
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args([
            "profile",
            "edit",
            must_some(profile_path.to_str(), "expected path to_str"),
            "--field",
            "base.ffbGain",
            "--value",
            "0.8",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));
}

#[test]
fn test_profile_export_import() {
    let temp_dir = must(TempDir::new());
    let profile_path = create_test_profile(&temp_dir, "test");
    let export_path = temp_dir.path().join("exported.json");
    let import_path = temp_dir.path().join("imported.json");

    // Export profile
    wheelctl()
        .args([
            "profile",
            "export",
            must_some(profile_path.to_str(), "expected path to_str"),
            "--output",
            must_some(export_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported"));

    // Import profile
    wheelctl()
        .args([
            "profile",
            "import",
            must_some(export_path.to_str(), "expected path to_str"),
            "--target",
            must_some(import_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported"));
}

// Diagnostic Tests

#[test]
fn test_diag_test() {
    wheelctl()
        .args(["diag", "test", "--device", "wheel-001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostic Results"));
}

#[test]
fn test_diag_test_json() {
    wheelctl()
        .args(["--json", "diag", "test", "--device", "wheel-001"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_diag_record() {
    let temp_dir = must(TempDir::new());
    let output_path = temp_dir.path().join("test.wbb");

    wheelctl()
        .args([
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "1",
            "--output",
            must_some(output_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("recorded"));
}

#[test]
fn test_diag_replay() {
    let temp_dir = must(TempDir::new());
    let blackbox_path = temp_dir.path().join("test.wbb");
    must(fs::write(
        &blackbox_path,
        "WBB1\x00\x00\x00\x00Mock blackbox data",
    ));

    wheelctl()
        .args([
            "diag",
            "replay",
            must_some(blackbox_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Replay completed"));
}

#[test]
fn test_diag_support_bundle() {
    let temp_dir = must(TempDir::new());
    let bundle_path = temp_dir.path().join("support.zip");

    wheelctl()
        .args([
            "diag",
            "support",
            "--output",
            must_some(bundle_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Support bundle created"));
}

#[test]
fn test_diag_metrics() {
    wheelctl()
        .args(["diag", "metrics"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostics"));
}

// Game Integration Tests

#[test]
fn test_game_list() {
    wheelctl()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Supported Games"));
}

#[test]
fn test_game_list_detailed() {
    wheelctl()
        .args(["game", "list", "--detailed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Features"));
}

#[test]
fn test_game_configure() {
    wheelctl()
        .args([
            "game",
            "configure",
            "iracing",
            "--path",
            "/test/path",
            "--auto",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configured"));
}

#[test]
fn test_game_status() {
    wheelctl()
        .args(["game", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Game Status"));
}

#[test]
fn test_game_test_telemetry() {
    wheelctl()
        .args(["game", "test", "iracing", "--duration", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Results"));
}

#[test]
fn test_telemetry_probe_ac_rally() {
    wheelctl()
        .args([
            "telemetry",
            "probe",
            "--game",
            "ac_rally",
            "--attempts",
            "1",
            "--timeout-ms",
            "10",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Telemetry probe"));
}

#[test]
fn test_telemetry_capture_json() {
    let temp_dir = must(TempDir::new());
    let capture_path = temp_dir.path().join("ac_rally_capture.bin");

    wheelctl()
        .args([
            "--json",
            "telemetry",
            "capture",
            "--game",
            "ac_rally",
            "--port",
            "0",
            "--duration",
            "1",
            "--out",
            must_some(capture_path.to_str(), "expected capture path"),
        ])
        .assert()
        .success()
        .stdout(is_json());

    assert!(capture_path.exists());
}

// Safety Tests

#[test]
fn test_safety_status() {
    wheelctl()
        .args(["safety", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Safety Status"));
}

#[test]
// Safety writes require a live wheeld service. With none reachable they exit 5
// (service unavailable) rather than reporting a completed action against the
// simulated backend -- a "High torque mode enabled" or "Emergency stop" line
// printed when nothing reached a device is the worst outcome this CLI can
// produce. Behaviour with a service present is covered by the service suite.
fn test_safety_enable_high_torque_requires_service() {
    wheelctl()
        .args(["safety", "enable", "wheel-001", "--force"])
        .assert()
        .code(5)
        .stdout(predicate::str::contains("High torque mode enabled").not());
}

#[test]
fn test_safety_emergency_stop_requires_service() {
    wheelctl()
        .args(["safety", "stop"])
        .assert()
        .code(5)
        .stdout(predicate::str::contains("Emergency stop").not());
}

#[test]
fn test_safety_set_limit_requires_service() {
    wheelctl()
        .args(["safety", "limit", "wheel-001", "5.0"])
        .assert()
        .code(5)
        .stdout(predicate::str::contains("Torque limit set").not());
}

#[test]
fn test_safety_invalid_limit() {
    // Offline the service check (5) precedes the value validation (4).
    wheelctl()
        .args(["safety", "limit", "wheel-001", "50.0"])
        .assert()
        .failure()
        .code(5);
}

// Health Monitoring Tests

#[test]
fn test_health_status() {
    wheelctl()
        .args(["health"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Service Health Status"));
}

#[test]
fn test_health_status_json() {
    wheelctl()
        .args(["--json", "health"])
        .assert()
        .success()
        .stdout(is_json());
}

// Error Code Tests

#[test]
fn test_service_unavailable_error() {
    wheelctl()
        .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
        .args(["device", "list"])
        .assert()
        .failure()
        .code(5); // Service unavailable error code
}

#[test]
fn test_invalid_command() {
    wheelctl()
        .args(["invalid", "command"])
        .assert()
        .failure()
        .code(2); // clap returns exit code 2 for usage errors
}

// JSON Output Validation Tests

#[test]
fn test_all_commands_support_json() {
    let commands = vec![
        vec!["device", "list"],
        vec!["profile", "list"],
        vec!["game", "list"],
        vec!["safety", "status"],
        vec!["health"],
        vec!["diag", "metrics"],
    ];

    for cmd in commands {
        let mut full_cmd = vec!["--json"];
        full_cmd.extend(cmd.iter());

        wheelctl()
            .args(full_cmd.as_slice())
            .assert()
            .success()
            .stdout(is_json());
    }
}

// Verbose Logging Tests

#[test]
fn test_verbose_logging() {
    wheelctl().args(["-v", "device", "list"]).assert().success();

    wheelctl()
        .args(["-vv", "device", "list"])
        .assert()
        .success();

    wheelctl()
        .args(["-vvv", "device", "list"])
        .assert()
        .success();
}

// End-to-End Workflow Tests

#[test]
fn test_complete_profile_workflow() {
    let temp_dir = must(TempDir::new());
    let profile_path = temp_dir.path().join("workflow_test.json");

    // Create profile
    wheelctl()
        .args([
            "profile",
            "create",
            must_some(profile_path.to_str(), "expected path to_str"),
            "--game",
            "iracing",
        ])
        .assert()
        .success();

    // Validate profile
    wheelctl()
        .args([
            "profile",
            "validate",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success();

    // Edit profile
    wheelctl()
        .args([
            "profile",
            "edit",
            must_some(profile_path.to_str(), "expected path to_str"),
            "--field",
            "base.ffbGain",
            "--value",
            "0.9",
        ])
        .assert()
        .success();

    // Apply profile
    wheelctl()
        .args([
            "profile",
            "apply",
            "wheel-001",
            must_some(profile_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success();
}

// ============================================================
// Additional CLI Integration Tests
// ============================================================

/// --help output lists all major subcommand categories
#[test]
fn test_help_lists_all_subcommands() {
    wheelctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("device"))
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("plugin"))
        .stdout(predicate::str::contains("diag"));
}

/// `device --help` prints device subcommand help and lists its operations
#[test]
fn test_device_subcommand_help() {
    wheelctl()
        .args(["device", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("status"));
}

/// `profile --help` prints profile subcommand help and lists its operations
#[test]
fn test_profile_subcommand_help() {
    wheelctl()
        .args(["profile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"));
}

/// `device list` smoke test: succeeds without physical hardware (mock IPC)
#[test]
fn test_device_list_smoke_no_hardware() {
    // The CLI uses a mock IPC client — no actual device hardware is required
    wheelctl().args(["device", "list"]).assert().success();
}

/// `health` command smoke test acts as a system status check
#[test]
fn test_health_status_smoke() {
    wheelctl()
        .args(["health"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status"));
}

/// `profile list` scans the local profile directory, so a bad endpoint is
/// irrelevant to it.
///
/// This used to assert exit code 5, which only happened because the profile
/// dispatcher built a client before matching the subcommand. Listing files on
/// disk must not depend on a reachable daemon.
#[test]
fn test_profiles_list_ignores_the_service_endpoint() {
    wheelctl()
        .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
        .args(["profile", "list"])
        .assert()
        .success();
}

/// An unknown top-level command produces a non-zero exit and non-empty stderr
#[test]
fn test_invalid_command_produces_stderr() {
    let output = must(wheelctl().args(["unknowncommand"]).output());
    assert!(
        !output.status.success(),
        "Expected non-zero exit for unknown command"
    );
    let stderr = std::str::from_utf8(&output.stderr).unwrap_or("");
    assert!(
        !stderr.is_empty(),
        "Expected non-empty stderr with error info"
    );
}

/// An unknown subcommand under a known command exits with error code 2
#[test]
fn test_invalid_device_subcommand_exits_nonzero() {
    wheelctl()
        .args(["device", "unknownaction"])
        .assert()
        .failure()
        .code(2);
}

/// `plugin list` smoke test: works without a service running
#[test]
fn test_plugin_list_smoke() {
    wheelctl().args(["plugin", "list"]).assert().success();
}

#[test]
fn test_complete_diagnostic_workflow() {
    let temp_dir = must(TempDir::new());
    let blackbox_path = temp_dir.path().join("diag_test.wbb");
    let support_path = temp_dir.path().join("support_test.zip");

    // Run diagnostics
    wheelctl()
        .args(["diag", "test", "--device", "wheel-001"])
        .assert()
        .success();

    // Record blackbox
    wheelctl()
        .args([
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "1",
            "--output",
            must_some(blackbox_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success();

    // Generate support bundle
    wheelctl()
        .args([
            "diag",
            "support",
            "--output",
            must_some(support_path.to_str(), "expected path to_str"),
        ])
        .assert()
        .success();
}
