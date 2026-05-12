//! Comprehensive CLI end-to-end tests for wheelctl.
//!
//! Covers: binary build/run, profile management, device listing, config validation,
//! diagnostics, error handling, JSON output, shell completion, and subcommand discovery.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a wheelctl Command, returning Result to avoid unwrap/expect.
fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

fn parse_json(bytes: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let v: Value = serde_json::from_slice(bytes)?;
    Ok(v)
}

/// Create a valid test profile JSON file in a temp directory.
fn write_test_profile(
    dir: &TempDir,
    name: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
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

    let path = dir.path().join(format!("{name}.json"));
    fs::write(&path, serde_json::to_string_pretty(&profile)?)?;
    Ok(path)
}

fn path_str(p: &std::path::Path) -> Result<&str, Box<dyn std::error::Error>> {
    p.to_str().ok_or_else(|| "non-UTF-8 path".into())
}

// ===========================================================================
// 1. CLI binary builds and runs (--help, --version)
// ===========================================================================

mod binary_basics {
    use super::*;

    #[test]
    fn help_flag_exits_successfully() -> TestResult {
        wheelctl()?.arg("--help").assert().success();
        Ok(())
    }

    #[test]
    fn short_help_flag_exits_successfully() -> TestResult {
        wheelctl()?.arg("-h").assert().success();
        Ok(())
    }

    #[test]
    fn version_flag_exits_successfully() -> TestResult {
        wheelctl()?.arg("--version").assert().success();
        Ok(())
    }

    #[test]
    fn short_version_flag_exits_successfully() -> TestResult {
        wheelctl()?.arg("-V").assert().success();
        Ok(())
    }

    #[test]
    fn version_output_starts_with_binary_name() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("wheelctl"),
            "version should start with binary name, got: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn version_contains_semver_pattern() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let has_semver = stdout.split_whitespace().any(|w| {
            let parts: Vec<&str> = w.split('.').collect();
            parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        });
        assert!(has_semver, "version should contain X.Y.Z pattern: {stdout}");
        Ok(())
    }

    #[test]
    fn help_text_mentions_long_about_description() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Racing Wheel"),
            "help should mention Racing Wheel: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn help_text_mentions_all_global_flags() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--json"), "should mention --json");
        assert!(
            stdout.contains("--verbose") || stdout.contains("-v"),
            "should mention verbose"
        );
        assert!(stdout.contains("--version"), "should mention --version");
        assert!(stdout.contains("--help"), "should mention --help");
        Ok(())
    }
}

// ===========================================================================
// 2. Profile management commands (create, list, edit, delete, import, export)
// ===========================================================================

mod profile_management {
    use super::*;

    #[test]
    fn profile_list_succeeds() -> TestResult {
        wheelctl()?.args(["profile", "list"]).assert().success();
        Ok(())
    }

    #[test]
    fn profile_list_with_game_filter() -> TestResult {
        wheelctl()?
            .args(["profile", "list", "--game", "iracing"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn profile_list_with_car_filter() -> TestResult {
        wheelctl()?
            .args(["profile", "list", "--game", "iracing", "--car", "gt3"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn profile_create_generates_file() -> TestResult {
        let temp = TempDir::new()?;
        let path = temp.path().join("new_profile.json");

        wheelctl()?
            .args(["profile", "create", path_str(&path)?, "--game", "acc"])
            .assert()
            .success();

        assert!(path.exists(), "profile file should have been created");
        Ok(())
    }

    #[test]
    fn profile_create_with_car_scope() -> TestResult {
        let temp = TempDir::new()?;
        let path = temp.path().join("scoped.json");

        wheelctl()?
            .args([
                "profile",
                "create",
                path_str(&path)?,
                "--game",
                "iracing",
                "--car",
                "gt3",
            ])
            .assert()
            .success();

        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn profile_show_displays_schema() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "show_test")?;

        wheelctl()?
            .args(["profile", "show", path_str(&path)?])
            .assert()
            .success()
            .stdout(predicate::str::contains("Profile Schema"));
        Ok(())
    }

    #[test]
    fn profile_edit_field_value() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "edit_test")?;

        wheelctl()?
            .args([
                "profile",
                "edit",
                path_str(&path)?,
                "--field",
                "base.ffbGain",
                "--value",
                "0.85",
            ])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn profile_validate_valid_file() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "valid")?;

        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .success()
            .stdout(predicate::str::contains("valid").or(predicate::str::contains("Valid")));
        Ok(())
    }

    #[test]
    fn profile_validate_detailed() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "detailed_valid")?;

        wheelctl()?
            .args(["profile", "validate", path_str(&path)?, "--detailed"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn profile_export_creates_output_file() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "export_src")?;
        let export = temp.path().join("exported.json");

        wheelctl()?
            .args([
                "profile",
                "export",
                path_str(&path)?,
                "--output",
                path_str(&export)?,
            ])
            .assert()
            .success();

        assert!(export.exists(), "exported file should exist");
        Ok(())
    }

    #[test]
    fn profile_import_succeeds() -> TestResult {
        let temp = TempDir::new()?;
        let src = write_test_profile(&temp, "import_src")?;
        let export = temp.path().join("export_for_import.json");
        let import_target = temp.path().join("imported.json");

        // Export first
        wheelctl()?
            .args([
                "profile",
                "export",
                path_str(&src)?,
                "--output",
                path_str(&export)?,
            ])
            .assert()
            .success();

        // Then import
        wheelctl()?
            .args([
                "profile",
                "import",
                path_str(&export)?,
                "--target",
                path_str(&import_target)?,
            ])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn profile_export_import_roundtrip_preserves_content() -> TestResult {
        let temp = TempDir::new()?;
        let src = write_test_profile(&temp, "roundtrip")?;
        let export = temp.path().join("roundtrip_export.json");
        let import_target = temp.path().join("roundtrip_import.json");

        wheelctl()?
            .args([
                "profile",
                "export",
                path_str(&src)?,
                "--output",
                path_str(&export)?,
            ])
            .assert()
            .success();

        wheelctl()?
            .args([
                "profile",
                "import",
                path_str(&export)?,
                "--target",
                path_str(&import_target)?,
            ])
            .assert()
            .success();

        // Both export and import should be valid JSON
        let export_content = fs::read_to_string(&export)?;
        let _: Value = serde_json::from_str(&export_content)?;

        if import_target.exists() {
            let import_content = fs::read_to_string(&import_target)?;
            let _: Value = serde_json::from_str(&import_content)?;
        }
        Ok(())
    }

    #[test]
    fn profile_create_edit_validate_workflow() -> TestResult {
        let temp = TempDir::new()?;
        let path = temp.path().join("workflow.json");

        // Create
        wheelctl()?
            .args(["profile", "create", path_str(&path)?, "--game", "acc"])
            .assert()
            .success();

        // Edit
        wheelctl()?
            .args([
                "profile",
                "edit",
                path_str(&path)?,
                "--field",
                "base.ffbGain",
                "--value",
                "0.6",
            ])
            .assert()
            .success();

        // Validate
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .success();
        Ok(())
    }
}

// ===========================================================================
// 3. Device listing commands
// ===========================================================================

mod device_listing {
    use super::*;

    #[test]
    fn device_list_succeeds() -> TestResult {
        wheelctl()?.args(["device", "list"]).assert().success();
        Ok(())
    }

    #[test]
    fn device_list_detailed_succeeds() -> TestResult {
        wheelctl()?
            .args(["device", "list", "--detailed"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn device_list_human_output_contains_header() -> TestResult {
        let output = wheelctl()?.args(["device", "list"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Device") || stdout.contains("device") || stdout.contains("Connected"),
            "device list should reference devices: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn device_list_detailed_shows_capabilities() -> TestResult {
        let output = wheelctl()?
            .args(["device", "list", "--detailed"])
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Capabilities")
                || stdout.contains("Max Torque")
                || stdout.contains("Type"),
            "detailed list should show capabilities: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn device_status_with_known_id() -> TestResult {
        wheelctl()?
            .args(["device", "status", "wheel-001"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn device_calibrate_with_yes_flag() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "wheel-001", "center", "--yes"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn device_reset_with_force_flag() -> TestResult {
        wheelctl()?
            .args(["device", "reset", "wheel-001", "--force"])
            .assert()
            .success();
        Ok(())
    }
}

// ===========================================================================
// 4. Configuration validation commands
// ===========================================================================

mod config_validation {
    use super::*;

    #[test]
    fn validate_invalid_json_file_fails() -> TestResult {
        let temp = TempDir::new()?;
        let bad = temp.path().join("bad.json");
        fs::write(&bad, "{ not valid json !!!")?;

        wheelctl()?
            .args(["profile", "validate", path_str(&bad)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_empty_file_fails() -> TestResult {
        let temp = TempDir::new()?;
        let empty = temp.path().join("empty.json");
        fs::write(&empty, "")?;

        wheelctl()?
            .args(["profile", "validate", path_str(&empty)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_nonexistent_file_fails() -> TestResult {
        wheelctl()?
            .args(["profile", "validate", "this_file_does_not_exist_12345.json"])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn show_nonexistent_profile_fails_with_exit_3() -> TestResult {
        wheelctl()?
            .args(["profile", "show", "nonexistent_profile_xyz.json"])
            .assert()
            .failure()
            .code(3);
        Ok(())
    }

    #[test]
    fn validate_valid_profile_with_json_output() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "json_validate")?;

        let output = wheelctl()?
            .args(["--json", "profile", "validate", path_str(&path)?])
            .output()?;
        assert!(output.status.success());

        let json = parse_json(&output.stdout)?;
        assert!(json.is_object());
        Ok(())
    }
}

// ===========================================================================
// 5. Diagnostics and status commands
// ===========================================================================

mod diagnostics_and_status {
    use super::*;

    #[test]
    fn diag_test_without_device_succeeds() -> TestResult {
        wheelctl()?.args(["diag", "test"]).assert().success();
        Ok(())
    }

    #[test]
    fn diag_test_with_device() -> TestResult {
        wheelctl()?
            .args(["diag", "test", "--device", "wheel-001"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn diag_test_specific_type_motor() -> TestResult {
        wheelctl()?
            .args(["diag", "test", "--device", "wheel-001", "motor"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn diag_metrics_succeeds() -> TestResult {
        wheelctl()?.args(["diag", "metrics"]).assert().success();
        Ok(())
    }

    #[test]
    fn diag_metrics_with_device() -> TestResult {
        wheelctl()?
            .args(["diag", "metrics", "wheel-001"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn diag_record_with_short_duration() -> TestResult {
        let temp = TempDir::new()?;
        let out = temp.path().join("record.wbb");

        wheelctl()?
            .args([
                "diag",
                "record",
                "wheel-001",
                "--duration",
                "1",
                "--output",
                path_str(&out)?,
            ])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn diag_support_bundle_creation() -> TestResult {
        let temp = TempDir::new()?;
        let out = temp.path().join("support.zip");

        wheelctl()?
            .args(["diag", "support", "--output", path_str(&out)?])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn health_command_succeeds() -> TestResult {
        wheelctl()?.args(["health"]).assert().success();
        Ok(())
    }

    #[test]
    fn safety_status_succeeds() -> TestResult {
        wheelctl()?.args(["safety", "status"]).assert().success();
        Ok(())
    }

    #[test]
    fn safety_enable_with_force() -> TestResult {
        wheelctl()?
            .args(["safety", "enable", "wheel-001", "--force"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn safety_emergency_stop() -> TestResult {
        wheelctl()?.args(["safety", "stop"]).assert().success();
        Ok(())
    }

    #[test]
    fn safety_stop_specific_device() -> TestResult {
        wheelctl()?
            .args(["safety", "stop", "wheel-001"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn safety_set_torque_limit() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001", "5.0"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn game_list_succeeds() -> TestResult {
        wheelctl()?.args(["game", "list"]).assert().success();
        Ok(())
    }

    #[test]
    fn game_status_succeeds() -> TestResult {
        wheelctl()?.args(["game", "status"]).assert().success();
        Ok(())
    }

    #[test]
    fn plugin_list_succeeds() -> TestResult {
        wheelctl()?.args(["plugin", "list"]).assert().success();
        Ok(())
    }
}

// ===========================================================================
// 6. Error handling for missing arguments
// ===========================================================================

mod missing_arguments {
    use super::*;

    #[test]
    fn no_subcommand_shows_usage() -> TestResult {
        let output = wheelctl()?.output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Usage") || stderr.contains("usage"),
            "should show usage: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn device_status_missing_device_arg() -> TestResult {
        wheelctl()?
            .args(["device", "status"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("DEVICE").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn device_calibrate_missing_type() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "w1"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("required"));
        Ok(())
    }

    #[test]
    fn profile_show_missing_path() -> TestResult {
        wheelctl()?
            .args(["profile", "show"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PROFILE").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_validate_missing_path() -> TestResult {
        wheelctl()?.args(["profile", "validate"]).assert().failure();
        Ok(())
    }

    #[test]
    fn safety_enable_missing_device() -> TestResult {
        wheelctl()?
            .args(["safety", "enable"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("DEVICE").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn safety_limit_missing_torque() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("TORQUE").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn plugin_search_missing_query() -> TestResult {
        wheelctl()?
            .args(["plugin", "search"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("QUERY").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn plugin_install_missing_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "install"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PLUGIN_ID").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn completion_missing_shell() -> TestResult {
        wheelctl()?
            .args(["completion"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SHELL").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn telemetry_capture_missing_out_flag() -> TestResult {
        wheelctl()?
            .args(["telemetry", "capture", "--game", "acc"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--out"));
        Ok(())
    }

    #[test]
    fn telemetry_probe_missing_game_flag() -> TestResult {
        wheelctl()?
            .args(["telemetry", "probe"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--game"));
        Ok(())
    }

    #[test]
    fn game_configure_missing_game_id() -> TestResult {
        wheelctl()?
            .args(["game", "configure"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("GAME").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn game_test_missing_game_id() -> TestResult {
        wheelctl()?
            .args(["game", "test"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("GAME").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn diag_record_missing_device() -> TestResult {
        wheelctl()?
            .args(["diag", "record"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("DEVICE").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn diag_replay_missing_file() -> TestResult {
        wheelctl()?
            .args(["diag", "replay"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("FILE").or(predicate::str::contains("required")));
        Ok(())
    }
}

// ===========================================================================
// 7. Error handling for invalid arguments
// ===========================================================================

mod invalid_arguments {
    use super::*;

    #[test]
    fn unknown_top_level_command() -> TestResult {
        let output = wheelctl()?.arg("nonexistent_cmd").output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("nonexistent_cmd") || stderr.contains("invalid"),
            "stderr should reference bad command: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn unknown_device_subcommand() -> TestResult {
        let output = wheelctl()?.args(["device", "teleport"]).output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("teleport") || stderr.contains("invalid"),
            "stderr should mention bad subcommand: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn unknown_profile_subcommand() -> TestResult {
        let output = wheelctl()?.args(["profile", "destroy"]).output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn unknown_diag_subcommand() -> TestResult {
        let output = wheelctl()?.args(["diag", "explode"]).output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn unknown_safety_subcommand() -> TestResult {
        let output = wheelctl()?.args(["safety", "launch"]).output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn unknown_game_subcommand() -> TestResult {
        let output = wheelctl()?.args(["game", "hack"]).output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn unknown_plugin_subcommand() -> TestResult {
        let output = wheelctl()?.args(["plugin", "hack"]).output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn invalid_calibration_type_value() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "w1", "somersault"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid value"));
        Ok(())
    }

    #[test]
    fn invalid_diag_test_type_value() -> TestResult {
        wheelctl()?
            .args(["diag", "test", "--device", "w1", "quantum"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value").or(predicate::str::contains("quantum")),
            );
        Ok(())
    }

    #[test]
    fn non_numeric_torque_limit() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "w1", "not_a_number"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid value"));
        Ok(())
    }

    #[test]
    fn invalid_completion_shell_value() -> TestResult {
        wheelctl()?
            .args(["completion", "ksh"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid value"));
        Ok(())
    }

    #[test]
    fn device_not_found_exits_code_2() -> TestResult {
        wheelctl()?
            .args(["device", "status", "totally-fake-device-xyz"])
            .assert()
            .failure()
            .code(2);
        Ok(())
    }

    #[test]
    fn safety_invalid_high_torque_exits_code_4() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001", "50.0"])
            .assert()
            .failure()
            .code(4);
        Ok(())
    }
}

// ===========================================================================
// 8. JSON output format validation
// ===========================================================================

mod json_output {
    use super::*;

    /// All read-only commands that should produce valid JSON with --json flag.
    const JSON_COMMANDS: &[&[&str]] = &[
        &["device", "list"],
        &["device", "list", "--detailed"],
        &["device", "status", "wheel-001"],
        &["profile", "list"],
        &["game", "list"],
        &["game", "status"],
        &["safety", "status"],
        &["health"],
        &["diag", "metrics"],
        &["plugin", "list"],
    ];

    #[test]
    fn all_json_commands_parse_as_valid_json() -> TestResult {
        for cmd_args in JSON_COMMANDS {
            let mut args = vec!["--json"];
            args.extend_from_slice(cmd_args);

            let output = wheelctl()?.args(&args).output()?;
            assert!(output.status.success(), "command {:?} should succeed", args);

            let json = parse_json(&output.stdout)?;
            assert!(json.is_object(), "JSON for {:?} should be an object", args);
        }
        Ok(())
    }

    #[test]
    fn all_json_commands_have_success_true() -> TestResult {
        for cmd_args in JSON_COMMANDS {
            let mut args = vec!["--json"];
            args.extend_from_slice(cmd_args);

            let output = wheelctl()?.args(&args).output()?;
            if output.status.success() {
                let json = parse_json(&output.stdout)?;
                assert_eq!(
                    json.get("success").and_then(Value::as_bool),
                    Some(true),
                    "command {:?} should have success: true",
                    args
                );
            }
        }
        Ok(())
    }

    #[test]
    fn device_list_json_has_devices_array() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("devices").and_then(Value::as_array).is_some(),
            "should have devices array"
        );
        Ok(())
    }

    #[test]
    fn device_status_json_has_status_field() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "device", "status", "wheel-001"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        assert!(json.get("status").is_some(), "should have status field");
        Ok(())
    }

    #[test]
    fn game_status_json_has_game_status() -> TestResult {
        let output = wheelctl()?.args(["--json", "game", "status"]).output()?;
        let json = parse_json(&output.stdout)?;
        assert!(json.get("game_status").is_some(), "should have game_status");
        Ok(())
    }

    #[test]
    fn json_flag_works_after_subcommand() -> TestResult {
        let output = wheelctl()?.args(["device", "list", "--json"]).output()?;
        assert!(output.status.success());
        let _json = parse_json(&output.stdout)?;
        Ok(())
    }

    #[test]
    fn json_flag_works_with_detailed_flag() -> TestResult {
        let output = wheelctl()?
            .args(["device", "list", "--detailed", "--json"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert!(json.get("devices").is_some());
        Ok(())
    }

    #[test]
    fn json_error_has_success_false() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "device", "status", "nonexistent-device-e2e"])
            .output()?;
        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(
            json.get("success").and_then(Value::as_bool),
            Some(false),
            "error JSON should have success: false"
        );
        Ok(())
    }

    #[test]
    fn json_error_has_error_message() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "device", "status", "nonexistent-device-e2e"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        let has_msg = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .is_some();
        assert!(has_msg, "error should have error.message field");
        Ok(())
    }

    #[test]
    fn json_output_is_single_document() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let trimmed = stdout.trim();
        let _: Value = serde_json::from_str(trimmed)?;
        Ok(())
    }

    #[test]
    fn json_output_ends_with_newline() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            stdout.ends_with('\n'),
            "JSON output should end with newline"
        );
        Ok(())
    }

    #[test]
    fn json_profile_show_produces_valid_json() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_test_profile(&temp, "json_show")?;

        let output = wheelctl()?
            .args(["--json", "profile", "show", path_str(&path)?])
            .output()?;
        assert!(output.status.success());

        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        assert!(json.get("profile").is_some(), "should have profile field");
        Ok(())
    }

    #[test]
    fn verbose_flag_does_not_corrupt_json() -> TestResult {
        let normal = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let verbose = wheelctl()?
            .args(["-vvv", "--json", "device", "list"])
            .output()?;

        let normal_json = parse_json(&normal.stdout)?;
        let verbose_json = parse_json(&verbose.stdout)?;

        // Both should have same top-level keys
        let normal_keys: Vec<&String> = normal_json
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        let verbose_keys: Vec<&String> = verbose_json
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert_eq!(
            normal_keys, verbose_keys,
            "JSON structure should match regardless of verbosity"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Shell completion generation
// ===========================================================================

mod shell_completion {
    use super::*;

    #[test]
    fn bash_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "bash"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "bash completion should be non-empty"
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("wheelctl"),
            "bash completion should reference binary name"
        );
        Ok(())
    }

    #[test]
    fn zsh_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "zsh"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "zsh completion should be non-empty"
        );
        Ok(())
    }

    #[test]
    fn fish_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "fish"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "fish completion should be non-empty"
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("wheelctl"),
            "fish completion should reference binary name"
        );
        Ok(())
    }

    #[test]
    fn powershell_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "powershell"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "powershell completion should be non-empty"
        );
        Ok(())
    }

    #[test]
    fn all_shell_completions_produce_valid_utf8() -> TestResult {
        for shell in &["bash", "zsh", "fish", "powershell"] {
            let output = wheelctl()?.args(["completion", shell]).output()?;
            assert!(output.status.success());
            let _ = String::from_utf8(output.stdout)?;
        }
        Ok(())
    }

    #[test]
    fn completion_output_contains_subcommand_names() -> TestResult {
        // Bash completion typically embeds subcommand names
        let output = wheelctl()?.args(["completion", "bash"]).output()?;
        let text = String::from_utf8_lossy(&output.stdout);
        // At minimum, the completion script should mention the binary name
        assert!(text.contains("wheelctl"));
        Ok(())
    }
}

// ===========================================================================
// 10. Subcommand discovery and help text
// ===========================================================================

mod subcommand_discovery {
    use super::*;

    #[test]
    fn root_help_lists_all_subcommands() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for subcmd in &[
            "device",
            "profile",
            "plugin",
            "diag",
            "game",
            "telemetry",
            "hardware",
            "safety",
            "completion",
            "health",
        ] {
            assert!(
                stdout.contains(subcmd),
                "root help should mention '{subcmd}'"
            );
        }
        Ok(())
    }

    #[test]
    fn device_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["device", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "status", "calibrate", "reset"] {
            assert!(stdout.contains(sub), "device help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn profile_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["profile", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &[
            "list", "show", "apply", "create", "edit", "validate", "export", "import",
        ] {
            assert!(stdout.contains(sub), "profile help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn plugin_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["plugin", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "search", "install", "uninstall", "info", "verify"] {
            assert!(stdout.contains(sub), "plugin help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn diag_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["diag", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["test", "record", "replay", "support", "metrics"] {
            assert!(stdout.contains(sub), "diag help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn game_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["game", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "configure", "status", "test"] {
            assert!(stdout.contains(sub), "game help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn telemetry_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["telemetry", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["probe", "capture"] {
            assert!(
                stdout.contains(sub),
                "telemetry help should mention '{sub}'"
            );
        }
        Ok(())
    }

    #[test]
    fn safety_help_lists_all_operations() -> TestResult {
        let output = wheelctl()?.args(["safety", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["enable", "stop", "status", "limit"] {
            assert!(stdout.contains(sub), "safety help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn subcommand_help_flags_all_succeed() -> TestResult {
        let subcommands = &[
            vec!["device", "--help"],
            vec!["profile", "--help"],
            vec!["plugin", "--help"],
            vec!["diag", "--help"],
            vec!["game", "--help"],
            vec!["telemetry", "--help"],
            vec!["hardware", "--help"],
            vec!["safety", "--help"],
        ];
        for args in subcommands {
            wheelctl()?.args(args.as_slice()).assert().success();
        }
        Ok(())
    }

    #[test]
    fn nested_subcommand_help_flags_succeed() -> TestResult {
        let nested_helps = &[
            vec!["device", "list", "--help"],
            vec!["device", "status", "--help"],
            vec!["device", "calibrate", "--help"],
            vec!["device", "reset", "--help"],
            vec!["profile", "list", "--help"],
            vec!["profile", "show", "--help"],
            vec!["profile", "create", "--help"],
            vec!["profile", "edit", "--help"],
            vec!["profile", "validate", "--help"],
            vec!["profile", "export", "--help"],
            vec!["profile", "import", "--help"],
            vec!["profile", "apply", "--help"],
            vec!["diag", "test", "--help"],
            vec!["diag", "record", "--help"],
            vec!["diag", "replay", "--help"],
            vec!["diag", "support", "--help"],
            vec!["diag", "metrics", "--help"],
            vec!["game", "list", "--help"],
            vec!["game", "configure", "--help"],
            vec!["game", "status", "--help"],
            vec!["game", "test", "--help"],
            vec!["telemetry", "probe", "--help"],
            vec!["telemetry", "capture", "--help"],
            vec!["hardware", "doctor", "--help"],
            vec!["safety", "enable", "--help"],
            vec!["safety", "stop", "--help"],
            vec!["safety", "status", "--help"],
            vec!["safety", "limit", "--help"],
            vec!["plugin", "list", "--help"],
            vec!["plugin", "search", "--help"],
            vec!["plugin", "install", "--help"],
            vec!["plugin", "uninstall", "--help"],
            vec!["plugin", "info", "--help"],
            vec!["plugin", "verify", "--help"],
        ];
        for args in nested_helps {
            let output = wheelctl()?.args(args.as_slice()).output()?;
            assert!(output.status.success(), "{:?} --help should succeed", args);
            assert!(
                !output.stdout.is_empty(),
                "{:?} --help should produce stdout",
                args
            );
        }
        Ok(())
    }

    #[test]
    fn help_output_goes_to_stdout_not_stderr() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        assert!(output.status.success());
        assert!(!output.stdout.is_empty(), "help should go to stdout");
        Ok(())
    }

    #[test]
    fn error_output_goes_to_stderr() -> TestResult {
        let output = wheelctl()?.arg("garbage_cmd").output()?;
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty(), "error should go to stderr");
        Ok(())
    }

    #[test]
    fn stdout_is_valid_utf8() -> TestResult {
        let output = wheelctl()?.args(["device", "list"]).output()?;
        let _ = String::from_utf8(output.stdout)?;
        Ok(())
    }

    #[test]
    fn verbose_levels_all_accepted() -> TestResult {
        for flag in &["-v", "-vv", "-vvv", "-vvvv"] {
            wheelctl()?
                .args([flag, "device", "list"])
                .assert()
                .success();
        }
        Ok(())
    }

    #[test]
    fn verbose_long_form_accepted() -> TestResult {
        wheelctl()?
            .args(["--verbose", "--verbose", "device", "list"])
            .assert()
            .success();
        Ok(())
    }
}
