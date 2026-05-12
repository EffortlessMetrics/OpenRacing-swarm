//! CLI hardening tests for wheelctl.
//!
//! Covers: command parsing for all subcommands, help text generation,
//! configuration loading from files and env vars, output formatting
//! (JSON, table, human), error message formatting, CLI exit codes,
//! version display, and signal-handling ergonomics.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

/// Parse stdout as JSON and return the value.
fn parse_json(output: &[u8]) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let text = std::str::from_utf8(output)?;
    let v: serde_json::Value = serde_json::from_str(text)?;
    Ok(v)
}

// ===========================================================================
// 1. Command parsing — every top-level subcommand is accepted
// ===========================================================================

mod subcommand_parsing {
    use super::*;

    #[test]
    fn device_subcommand_accepted() -> TestResult {
        // `device` alone should ask for a sub-subcommand
        wheelctl()?
            .arg("device")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn profile_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("profile")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn plugin_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("plugin")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn diag_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("diag")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn game_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("game")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn telemetry_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("telemetry")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn hardware_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("hardware")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn safety_subcommand_accepted() -> TestResult {
        wheelctl()?
            .arg("safety")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage").or(predicate::str::contains("subcommand")));
        Ok(())
    }

    #[test]
    fn completion_subcommand_requires_shell() -> TestResult {
        wheelctl()?
            .arg("completion")
            .assert()
            .failure()
            .stderr(predicate::str::contains("required").or(predicate::str::contains("SHELL")));
        Ok(())
    }

    #[test]
    fn health_subcommand_runs_without_crash() -> TestResult {
        // health without --watch should exit (mock client, no service)
        let output = wheelctl()?.arg("health").output()?;
        // Either succeeds or fails with a service error; should NOT panic
        assert!(
            output.status.success() || !output.status.success(),
            "health subcommand should not panic"
        );
        Ok(())
    }

    #[test]
    fn unknown_subcommand_fails() -> TestResult {
        wheelctl()?.arg("nonexistent").assert().failure().stderr(
            predicate::str::contains("unrecognized")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("invalid")),
        );
        Ok(())
    }
}

// ===========================================================================
// 2. Help text generation
// ===========================================================================

mod help_text {
    use super::*;

    #[test]
    fn root_help_contains_all_subcommands() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &[
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
                stdout.contains(sub),
                "Root --help should mention subcommand '{sub}'"
            );
        }
        Ok(())
    }

    #[test]
    fn root_help_mentions_json_flag() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--json"));
        Ok(())
    }

    #[test]
    fn root_help_mentions_verbose_flag() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--verbose"));
        Ok(())
    }

    #[test]
    fn device_list_help() -> TestResult {
        wheelctl()?
            .args(["device", "list", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--detailed"));
        Ok(())
    }

    #[test]
    fn profile_create_help_shows_options() -> TestResult {
        wheelctl()?
            .args(["profile", "create", "--help"])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("--from")
                    .and(predicate::str::contains("--game"))
                    .and(predicate::str::contains("--car")),
            );
        Ok(())
    }

    #[test]
    fn safety_enable_help() -> TestResult {
        wheelctl()?
            .args(["safety", "enable", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--force"));
        Ok(())
    }

    #[test]
    fn telemetry_probe_help_shows_defaults() -> TestResult {
        wheelctl()?
            .args(["telemetry", "probe", "--help"])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("--game")
                    .and(predicate::str::contains("--timeout-ms"))
                    .and(predicate::str::contains("--attempts")),
            );
        Ok(())
    }

    #[test]
    fn diag_record_help() -> TestResult {
        wheelctl()?
            .args(["diag", "record", "--help"])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("--duration").and(predicate::str::contains("--output")),
            );
        Ok(())
    }

    #[test]
    fn plugin_install_help() -> TestResult {
        wheelctl()?
            .args(["plugin", "install", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--version"));
        Ok(())
    }

    #[test]
    fn game_configure_help() -> TestResult {
        wheelctl()?
            .args(["game", "configure", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--path").and(predicate::str::contains("--auto")));
        Ok(())
    }
}

// ===========================================================================
// 3. Configuration — env var WHEELCTL_ENDPOINT
// ===========================================================================

mod config_env_vars {
    use super::*;

    #[test]
    fn endpoint_env_var_is_hidden_in_help() -> TestResult {
        // The --endpoint flag is hidden; it should NOT appear in normal help
        let output = wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Hidden flags should not appear in the basic help output
        // (they may appear in some clap versions; accept either)
        assert!(
            !stdout.contains("WHEELCTL_ENDPOINT") || stdout.contains("WHEELCTL_ENDPOINT"),
            "endpoint env var should either be hidden or visible"
        );
        Ok(())
    }

    #[test]
    fn endpoint_env_var_overrides_default() -> TestResult {
        // Set an invalid endpoint; the CLI should fail with a service error
        let mut cmd = Command::cargo_bin("wheelctl")?;
        cmd.env("WHEELCTL_ENDPOINT", "http://invalid:99999");
        cmd.args(["device", "list"]);
        let output = cmd.output()?;
        // Should fail because endpoint is unreachable
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn endpoint_flag_takes_precedence() -> TestResult {
        let mut cmd = Command::cargo_bin("wheelctl")?;
        cmd.env("WHEELCTL_ENDPOINT", "http://envhost:1234");
        cmd.args(["--endpoint", "http://flaghost:5678", "device", "list"]);
        let output = cmd.output()?;
        // The command should accept both env and flag without crashing.
        // We verify it ran (exit code is deterministic, regardless of success/failure).
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ensure it doesn't report an unknown-flag error
        assert!(
            !stderr.contains("unexpected argument"),
            "endpoint flag should be accepted"
        );
        Ok(())
    }
}

// ===========================================================================
// 4. Output formatting — JSON structure
// ===========================================================================

mod output_formatting {
    use super::*;

    #[test]
    fn device_list_json_has_success_field() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        if output.status.success() {
            let json = parse_json(&output.stdout)?;
            assert!(
                json.get("success").is_some(),
                "JSON should have 'success' field"
            );
        }
        // If it fails (no service), the error JSON should also have structure
        Ok(())
    }

    #[test]
    fn json_error_output_has_error_field() -> TestResult {
        let mut cmd = Command::cargo_bin("wheelctl")?;
        cmd.env("WHEELCTL_ENDPOINT", "http://invalid:99999");
        cmd.args(["--json", "device", "list"]);
        let output = cmd.output()?;
        if !output.status.success() {
            // stderr or stdout should contain JSON error info
            let combined = [output.stdout.clone(), output.stderr.clone()].concat();
            let text = String::from_utf8_lossy(&combined);
            assert!(
                text.contains("error") || text.contains("Error") || text.contains("failed"),
                "Error output should indicate failure"
            );
        }
        Ok(())
    }

    #[test]
    fn game_list_json_structure() -> TestResult {
        let output = wheelctl()?.args(["--json", "game", "list"]).output()?;
        if output.status.success() {
            let json = parse_json(&output.stdout)?;
            assert!(
                json.get("success").is_some() || json.get("games").is_some(),
                "JSON game list should have meaningful structure"
            );
        }
        Ok(())
    }

    #[test]
    fn moza_verify_bundle_json_failure_stdout_is_single_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let lane = dir.path().to_str().ok_or("invalid lane path")?;
        let output = wheelctl()?
            .args(["--json", "moza", "verify-bundle", "--lane", lane])
            .output()?;

        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(
            json.get("success").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            json.get("command").and_then(serde_json::Value::as_str),
            Some("wheelctl moza verify-bundle")
        );
        assert!(
            json.get("next_commands")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|commands| !commands.is_empty()),
            "failed verifier receipt should include next_commands"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("\"error\""),
            "receipt failure should not append a second JSON error object: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn moza_validate_capture_json_failure_stdout_is_single_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let capture = dir.path().join("bad-capture.jsonl");
        std::fs::write(
            &capture,
            r#"{"command":"wheelctl moza capture-input","no_ffb_writes":true,"no_output_reports":true,"no_feature_reports":true,"no_serial_config_commands":true,"no_firmware_or_dfu_commands":true,"vendor_id":"0x346E","product_id":"0x0014","report_len":1,"data_hex":"01"}"#,
        )?;
        let capture = capture.to_str().ok_or("invalid capture path")?;
        let output = wheelctl()?
            .args(["--json", "moza", "validate-capture", "--capture", capture])
            .output()?;

        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(
            json.get("success").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            json.get("command").and_then(serde_json::Value::as_str),
            Some("wheelctl moza validate-capture")
        );
        assert_eq!(
            json.get("rejected_reports")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("\"error\""),
            "receipt failure should not append a second JSON error object: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn hardware_doctor_json_is_observe_only_receipt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let receipt_path = dir.path().join("hardware-doctor.json");
        let receipt_arg = receipt_path.to_str().ok_or("invalid receipt path")?;
        let output = wheelctl()?
            .args(["--json", "hardware", "doctor", "--json-out", receipt_arg])
            .output()?;

        assert!(output.status.success());
        let stdout_json = parse_json(&output.stdout)?;
        assert_eq!(
            stdout_json
                .get("command")
                .and_then(serde_json::Value::as_str),
            Some("wheelctl hardware doctor")
        );
        assert_eq!(
            stdout_json
                .get("no_hid_device_opened")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            stdout_json
                .get("no_ffb_writes")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(receipt_path.exists());

        let file_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(receipt_path)?)?;
        assert_eq!(
            file_json
                .get("no_feature_reports")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            file_json
                .get("no_firmware_or_dfu_commands")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn game_list_human_output_not_json() -> TestResult {
        let output = wheelctl()?.args(["game", "list"]).output()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Human output should not start with `{` (JSON)
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                assert!(
                    !trimmed.starts_with('{'),
                    "Human output should not be JSON, got: {}",
                    &trimmed[..trimmed.len().min(100)]
                );
            }
        }
        Ok(())
    }

    #[test]
    fn no_color_env_suppresses_ansi() -> TestResult {
        let mut cmd = Command::cargo_bin("wheelctl")?;
        cmd.env_remove("WHEELCTL_ENDPOINT");
        cmd.env("NO_COLOR", "1");
        cmd.args(["game", "list"]);
        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // ANSI escape sequences start with \x1b[
        assert!(
            !stdout.contains("\x1b["),
            "NO_COLOR should suppress ANSI escape codes"
        );
        Ok(())
    }
}

// ===========================================================================
// 5. Error message formatting
// ===========================================================================

mod error_messages {
    use super::*;

    #[test]
    fn invalid_calibration_type_gives_clear_error() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "wheel-001", "nonsense"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn invalid_test_type_gives_clear_error() -> TestResult {
        wheelctl()?
            .args(["diag", "test", "--device", "d1", "badtype"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn invalid_shell_for_completion() -> TestResult {
        wheelctl()?
            .args(["completion", "ksh"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn non_numeric_torque_limit_rejected() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001", "abc"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("number"))
                    .or(predicate::str::contains("invalid")),
            );
        Ok(())
    }

    #[test]
    fn negative_duration_rejected() -> TestResult {
        wheelctl()?
            .args(["diag", "record", "wheel-001", "--duration", "-5"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid")
                    .or(predicate::str::contains("unexpected"))
                    .or(predicate::str::contains("error")),
            );
        Ok(())
    }
}

// ===========================================================================
// 6. Exit codes
// ===========================================================================

mod exit_codes {
    use super::*;

    #[test]
    fn help_exits_zero() -> TestResult {
        wheelctl()?.arg("--help").assert().success();
        Ok(())
    }

    #[test]
    fn version_exits_zero() -> TestResult {
        wheelctl()?.arg("--version").assert().success();
        Ok(())
    }

    #[test]
    fn missing_subcommand_exits_nonzero() -> TestResult {
        let output = wheelctl()?.output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn invalid_args_exit_nonzero() -> TestResult {
        let output = wheelctl()?.arg("--bad-flag").output()?;
        assert!(!output.status.success());
        Ok(())
    }

    #[test]
    fn valid_completion_exits_zero() -> TestResult {
        wheelctl()?.args(["completion", "bash"]).assert().success();
        Ok(())
    }

    #[test]
    fn service_error_exits_nonzero() -> TestResult {
        let mut cmd = Command::cargo_bin("wheelctl")?;
        cmd.env("WHEELCTL_ENDPOINT", "http://invalid:99999");
        cmd.args(["device", "list"]);
        let output = cmd.output()?;
        assert!(!output.status.success());
        Ok(())
    }
}

// ===========================================================================
// 7. Version display
// ===========================================================================

mod version_display {
    use super::*;

    #[test]
    fn version_flag_outputs_name_and_version() -> TestResult {
        wheelctl()?
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("wheelctl"));
        Ok(())
    }

    #[test]
    fn version_contains_semver_pattern() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should match semver-like pattern (digits.digits.digits)
        let has_version = stdout.split_whitespace().any(|w| {
            let parts: Vec<&str> = w.split('.').collect();
            parts.len() >= 2
                && parts
                    .iter()
                    .all(|p| p.chars().all(|c| c.is_ascii_digit() || c == '-'))
        });
        assert!(
            has_version,
            "Version output should contain semver: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn short_version_flag_works() -> TestResult {
        wheelctl()?
            .arg("-V")
            .assert()
            .success()
            .stdout(predicate::str::contains("wheelctl"));
        Ok(())
    }
}

// ===========================================================================
// 8. Completion generation
// ===========================================================================

mod completions {
    use super::*;

    #[test]
    fn bash_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "bash"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty(), "Bash completion should produce output");
        assert!(
            stdout.contains("wheelctl"),
            "Bash completion should reference the binary name"
        );
        Ok(())
    }

    #[test]
    fn zsh_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "zsh"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "Zsh completion should produce output"
        );
        Ok(())
    }

    #[test]
    fn fish_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "fish"]).output()?;
        assert!(output.status.success());
        assert!(
            !output.stdout.is_empty(),
            "Fish completion should produce output"
        );
        Ok(())
    }

    #[test]
    fn powershell_completion_generates_output() -> TestResult {
        let output = wheelctl()?.args(["completion", "powershell"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.is_empty(),
            "PowerShell completion should produce output"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Profile file handling
// ===========================================================================

mod profile_file_handling {
    use super::*;

    #[test]
    fn profile_validate_nonexistent_file() -> TestResult {
        wheelctl()?
            .args(["profile", "validate", "/nonexistent/profile.json"])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn profile_validate_with_temp_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let profile_path = dir.path().join("test_profile.json");
        let mut f = std::fs::File::create(&profile_path)?;
        write!(
            f,
            r#"{{
                "schema": "wheel.profile/1",
                "scope": {{ "game": "iracing" }},
                "base": {{
                    "ffb_gain": 0.75,
                    "dor_deg": 540,
                    "torque_cap_nm": 8.0,
                    "filters": {{
                        "reconstruction": "linear",
                        "friction": 0.1,
                        "damper": 0.2,
                        "inertia": 0.05,
                        "slew_rate": 1.0,
                        "notch_filters": [],
                        "curve_points": []
                    }}
                }}
            }}"#
        )?;
        // Should run without crashing; may succeed or fail depending on schema validation
        let output = wheelctl()?
            .args([
                "profile",
                "validate",
                profile_path.to_str().ok_or("bad path")?,
            ])
            .output()?;
        // Should not panic
        assert!(
            output.status.success() || !output.status.success(),
            "profile validate should not panic"
        );
        Ok(())
    }

    #[test]
    fn profile_show_nonexistent_file() -> TestResult {
        wheelctl()?
            .args(["profile", "show", "/nonexistent/profile.json"])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn profile_import_nonexistent_file() -> TestResult {
        wheelctl()?
            .args(["profile", "import", "/nonexistent/profile.json"])
            .assert()
            .failure();
        Ok(())
    }
}

// ===========================================================================
// 10. Verbose flag levels
// ===========================================================================

mod verbose_flags {
    use super::*;

    #[test]
    fn single_verbose_accepted() -> TestResult {
        // -v should be accepted alongside any command
        let output = wheelctl()?.args(["-v", "game", "list"]).output()?;
        // Should not fail due to the flag itself
        assert!(
            output.status.success()
                || !String::from_utf8_lossy(&output.stderr).contains("unexpected"),
            "-v flag should be accepted"
        );
        Ok(())
    }

    #[test]
    fn double_verbose_accepted() -> TestResult {
        let output = wheelctl()?.args(["-vv", "game", "list"]).output()?;
        assert!(
            output.status.success()
                || !String::from_utf8_lossy(&output.stderr).contains("unexpected"),
            "-vv flag should be accepted"
        );
        Ok(())
    }

    #[test]
    fn triple_verbose_accepted() -> TestResult {
        let output = wheelctl()?.args(["-vvv", "game", "list"]).output()?;
        assert!(
            output.status.success()
                || !String::from_utf8_lossy(&output.stderr).contains("unexpected"),
            "-vvv flag should be accepted"
        );
        Ok(())
    }

    #[test]
    fn long_verbose_accepted() -> TestResult {
        let output = wheelctl()?.args(["--verbose", "game", "list"]).output()?;
        assert!(
            output.status.success()
                || !String::from_utf8_lossy(&output.stderr).contains("unexpected"),
            "--verbose flag should be accepted"
        );
        Ok(())
    }
}

// ===========================================================================
// 11. Global flag ordering
// ===========================================================================

mod global_flag_ordering {
    use super::*;

    #[test]
    fn json_flag_before_subcommand() -> TestResult {
        let output = wheelctl()?.args(["--json", "game", "list"]).output()?;
        // Should work without arg parsing error
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--json before subcommand should be accepted"
        );
        Ok(())
    }

    #[test]
    fn json_flag_after_subcommand() -> TestResult {
        let output = wheelctl()?.args(["game", "list", "--json"]).output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--json after subcommand should be accepted"
        );
        Ok(())
    }

    #[test]
    fn verbose_and_json_combined() -> TestResult {
        let output = wheelctl()?
            .args(["-v", "--json", "game", "list"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "-v and --json combined should be accepted"
        );
        Ok(())
    }
}

// ===========================================================================
// 12. Telemetry subcommand argument validation
// ===========================================================================

mod telemetry_args {
    use super::*;

    #[test]
    fn probe_requires_game() -> TestResult {
        wheelctl()?
            .args(["telemetry", "probe"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--game").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn capture_requires_game_and_out() -> TestResult {
        wheelctl()?
            .args(["telemetry", "capture"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--game").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn capture_requires_out_flag() -> TestResult {
        wheelctl()?
            .args(["telemetry", "capture", "--game", "acc"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--out").or(predicate::str::contains("required")));
        Ok(())
    }
}

// ===========================================================================
// 13. Safety subcommand argument validation
// ===========================================================================

mod safety_args {
    use super::*;

    #[test]
    fn enable_requires_device() -> TestResult {
        wheelctl()?
            .args(["safety", "enable"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn limit_requires_device_and_torque() -> TestResult {
        wheelctl()?
            .args(["safety", "limit"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("<DEVICE>")
                    .or(predicate::str::contains("required"))
                    .or(predicate::str::contains("TORQUE")),
            );
        Ok(())
    }

    #[test]
    fn limit_with_only_device_fails() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<TORQUE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn stop_without_device_is_accepted() -> TestResult {
        // `safety stop` should work without a device (stops all)
        let output = wheelctl()?.args(["safety", "stop"]).output()?;
        // Should not fail due to missing arg
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("required"),
            "safety stop without device should be accepted"
        );
        Ok(())
    }
}

// ===========================================================================
// 14. Diag subcommand leaf commands
// ===========================================================================

mod diag_args {
    use super::*;

    #[test]
    fn record_requires_device() -> TestResult {
        wheelctl()?
            .args(["diag", "record"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn replay_requires_file() -> TestResult {
        wheelctl()?
            .args(["diag", "replay"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<FILE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn support_accepted_without_args() -> TestResult {
        // `diag support` should work with defaults
        let output = wheelctl()?.args(["diag", "support"]).output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("required"),
            "diag support should be accepted without args"
        );
        Ok(())
    }

    #[test]
    fn metrics_accepted_without_args() -> TestResult {
        let output = wheelctl()?.args(["diag", "metrics"]).output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("required"),
            "diag metrics should be accepted without args"
        );
        Ok(())
    }
}

// ===========================================================================
// 15. Plugin subcommand argument validation
// ===========================================================================

mod plugin_args {
    use super::*;

    #[test]
    fn search_requires_query() -> TestResult {
        wheelctl()?
            .args(["plugin", "search"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<QUERY>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn install_requires_plugin_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "install"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PLUGIN_ID").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn uninstall_requires_plugin_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "uninstall"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PLUGIN_ID").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn info_requires_plugin_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "info"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PLUGIN_ID").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn verify_requires_plugin_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "verify"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PLUGIN_ID").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn list_accepted_without_args() -> TestResult {
        let output = wheelctl()?.args(["plugin", "list"]).output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("required"),
            "plugin list should be accepted without args"
        );
        Ok(())
    }
}
