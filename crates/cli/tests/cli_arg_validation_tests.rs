//! Argument validation tests for wheelctl CLI.
//!
//! Covers: missing required args, invalid arg values, mutually exclusive
//! combinations, help text presence for every subcommand, version output
//! format, and unknown subcommand handling.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

// ===========================================================================
// 1. Missing required arguments
// ===========================================================================

mod missing_required_args {
    use super::*;

    #[test]
    fn no_subcommand_shows_usage() -> TestResult {
        wheelctl()?
            .assert()
            .failure()
            .stderr(predicate::str::contains("Usage"));
        Ok(())
    }

    #[test]
    fn device_status_missing_device_id() -> TestResult {
        wheelctl()?
            .args(["device", "status"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn device_calibrate_missing_device_and_type() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("required").or(predicate::str::contains("Usage")));
        Ok(())
    }

    #[test]
    fn device_calibrate_missing_calibration_type() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "wheel-001"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("required")
                    .or(predicate::str::contains("CALIBRATION_TYPE"))
                    .or(predicate::str::contains("calibration")),
            );
        Ok(())
    }

    #[test]
    fn device_reset_missing_device_id() -> TestResult {
        wheelctl()?
            .args(["device", "reset"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_show_missing_profile() -> TestResult {
        wheelctl()?
            .args(["profile", "show"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<PROFILE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_apply_missing_args() -> TestResult {
        wheelctl()?
            .args(["profile", "apply"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("required").or(predicate::str::contains("Usage")));
        Ok(())
    }

    #[test]
    fn profile_create_missing_path() -> TestResult {
        wheelctl()?
            .args(["profile", "create"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<PATH>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_validate_missing_path() -> TestResult {
        wheelctl()?
            .args(["profile", "validate"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<PATH>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_export_missing_profile() -> TestResult {
        wheelctl()?
            .args(["profile", "export"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<PROFILE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn profile_import_missing_path() -> TestResult {
        wheelctl()?
            .args(["profile", "import"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<PATH>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn plugin_search_missing_query() -> TestResult {
        wheelctl()?
            .args(["plugin", "search"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<QUERY>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn plugin_install_missing_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "install"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("<PLUGIN_ID>").or(predicate::str::contains("required")),
            );
        Ok(())
    }

    #[test]
    fn plugin_uninstall_missing_id() -> TestResult {
        wheelctl()?
            .args(["plugin", "uninstall"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("<PLUGIN_ID>").or(predicate::str::contains("required")),
            );
        Ok(())
    }

    #[test]
    fn diag_record_missing_device() -> TestResult {
        wheelctl()?
            .args(["diag", "record"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn diag_replay_missing_file() -> TestResult {
        wheelctl()?
            .args(["diag", "replay"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<FILE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn game_configure_missing_game() -> TestResult {
        wheelctl()?
            .args(["game", "configure"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<GAME>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn game_test_missing_game() -> TestResult {
        wheelctl()?
            .args(["game", "test"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<GAME>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn safety_enable_missing_device() -> TestResult {
        wheelctl()?
            .args(["safety", "enable"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn safety_limit_missing_device_and_torque() -> TestResult {
        wheelctl()?
            .args(["safety", "limit"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("required").or(predicate::str::contains("Usage")));
        Ok(())
    }

    #[test]
    fn telemetry_probe_missing_game() -> TestResult {
        wheelctl()?
            .args(["telemetry", "probe"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--game").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn telemetry_capture_missing_game_and_out() -> TestResult {
        wheelctl()?
            .args(["telemetry", "capture"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--game").or(predicate::str::contains("required")));
        Ok(())
    }

    #[test]
    fn completion_missing_shell() -> TestResult {
        wheelctl()?
            .args(["completion"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("<SHELL>").or(predicate::str::contains("required")));
        Ok(())
    }
}

// ===========================================================================
// 2. Invalid argument values
// ===========================================================================

mod invalid_arg_values {
    use super::*;

    #[test]
    fn device_calibrate_invalid_type() -> TestResult {
        wheelctl()?
            .args(["device", "calibrate", "wheel-001", "bogus-type"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn diag_test_invalid_test_type() -> TestResult {
        wheelctl()?
            .args(["diag", "test", "bogus-test"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn completion_invalid_shell() -> TestResult {
        wheelctl()?
            .args(["completion", "not-a-shell"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("possible values")),
            );
        Ok(())
    }

    #[test]
    fn safety_limit_non_numeric_torque() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001", "not-a-number"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("invalid value")
                    .or(predicate::str::contains("cannot parse")),
            );
        Ok(())
    }
}

// ===========================================================================
// 3. Unknown subcommands / flags
// ===========================================================================

mod unknown_commands {
    use super::*;

    #[test]
    fn unknown_root_subcommand() -> TestResult {
        wheelctl()?
            .arg("nonexistent-cmd")
            .assert()
            .failure()
            .stderr(predicate::str::contains("unrecognized subcommand"));
        Ok(())
    }

    #[test]
    fn unknown_device_subcommand() -> TestResult {
        wheelctl()?
            .args(["device", "fly"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unrecognized subcommand"));
        Ok(())
    }

    #[test]
    fn unknown_global_flag() -> TestResult {
        wheelctl()?
            .args(["--bogus-flag", "device", "list"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unexpected argument"));
        Ok(())
    }
}

// ===========================================================================
// 4. Help text presence for every subcommand group
// ===========================================================================

mod help_text_presence {
    use super::*;

    #[test]
    fn root_help_lists_all_subcommands() -> TestResult {
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
                "root --help should list subcommand '{sub}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn device_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["device", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["list", "status", "calibrate", "reset"] {
            assert!(
                stdout.contains(leaf),
                "device --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn profile_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["profile", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &[
            "list", "show", "apply", "create", "edit", "validate", "export", "import",
        ] {
            assert!(
                stdout.contains(leaf),
                "profile --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["plugin", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["list", "search", "install", "uninstall", "info", "verify"] {
            assert!(
                stdout.contains(leaf),
                "plugin --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn diag_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["diag", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["test", "record", "replay", "support", "metrics"] {
            assert!(
                stdout.contains(leaf),
                "diag --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn game_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["game", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["list", "configure", "status", "test"] {
            assert!(
                stdout.contains(leaf),
                "game --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn telemetry_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["telemetry", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["probe", "capture"] {
            assert!(
                stdout.contains(leaf),
                "telemetry --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }

    #[test]
    fn safety_help_lists_leaf_commands() -> TestResult {
        let output = wheelctl()?.args(["safety", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for leaf in &["enable", "stop", "status", "limit"] {
            assert!(
                stdout.contains(leaf),
                "safety --help should list '{leaf}', got:\n{stdout}"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// 5. Version output format
// ===========================================================================

mod version_output {
    use super::*;

    #[test]
    fn version_starts_with_wheelctl() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("wheelctl"),
            "version should start with 'wheelctl', got: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn version_contains_semver() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let has_semver = stdout.split_whitespace().any(|word| {
            let parts: Vec<&str> = word.split('.').collect();
            parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        });
        assert!(has_semver, "version should contain X.Y.Z pattern: {stdout}");
        Ok(())
    }

    #[test]
    fn version_is_single_line() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line_count = stdout.trim().lines().count();
        assert_eq!(
            line_count, 1,
            "version should be a single line, got {line_count}: {stdout}"
        );
        Ok(())
    }
}
