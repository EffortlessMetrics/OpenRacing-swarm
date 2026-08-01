//! Tests that `wheelctl` never presents simulated data as real.
//!
//! When `wheeld` is not running, the CLI falls back to a built-in canned
//! backend so it stays usable offline. That fallback used to be silent: a
//! first-time user with no daemon and no hardware saw two invented Fanatec
//! devices and `Service: Running`, with nothing to indicate any of it was
//! fake.
//!
//! These tests exercise the compiled binary with no daemon reachable, which is
//! exactly the first-run situation they are protecting.
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

#![allow(deprecated)] // cargo_bin deprecation warnings

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A port chosen so nothing is listening on it, making the fallback path
/// deterministic regardless of whether the developer happens to be running a
/// real `wheeld`.
const UNREACHABLE_ENDPOINT: &str = "http://127.0.0.1:19998";

/// Exit code the CLI maps `CliError::ServiceUnavailable` to.
const EXIT_SERVICE_UNAVAILABLE: i32 = 5;

/// Build a `wheelctl` command pointed at an endpoint with no service on it.
fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    cmd.env_remove("WHEELCTL_NO_MOCK");
    cmd.env_remove("OPENRACING_NO_MOCK");
    cmd.arg("--endpoint").arg(UNREACHABLE_ENDPOINT);
    Ok(cmd)
}

// ===========================================================================
// The fallback announces itself
// ===========================================================================

mod fallback_is_visible {
    use super::*;

    #[test]
    fn device_list_warns_that_data_is_simulated() -> TestResult {
        wheelctl()?
            .args(["device", "list"])
            .assert()
            .success()
            .stderr(predicate::str::contains("SIMULATED"));
        Ok(())
    }

    #[test]
    fn warning_names_the_unreachable_endpoint() -> TestResult {
        wheelctl()?
            .args(["device", "list"])
            .assert()
            .stderr(predicate::str::contains(UNREACHABLE_ENDPOINT));
        Ok(())
    }

    #[test]
    fn warning_tells_the_user_how_to_start_the_service() -> TestResult {
        // The remediation must be a command to run, not just a question.
        wheelctl()?.args(["device", "list"]).assert().stderr(
            predicate::str::contains("systemctl")
                .or(predicate::str::contains("sc start"))
                .or(predicate::str::contains("launchctl")),
        );
        Ok(())
    }

    #[test]
    fn warning_mentions_the_no_mock_opt_out() -> TestResult {
        wheelctl()?
            .args(["device", "list"])
            .assert()
            .stderr(predicate::str::contains("--no-mock"));
        Ok(())
    }

    #[test]
    fn simulated_devices_are_labelled_in_human_output() -> TestResult {
        wheelctl()?
            .args(["device", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("simulated"));
        Ok(())
    }

    #[test]
    fn warning_goes_to_stderr_so_json_stdout_stays_parseable() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert!(
            parsed.get("devices").is_some(),
            "expected a devices field in {stdout}"
        );
        Ok(())
    }
}

// ===========================================================================
// --no-mock makes an unreachable service an error
// ===========================================================================

mod no_mock_opt_out {
    use super::*;

    #[test]
    fn flag_fails_instead_of_simulating() -> TestResult {
        wheelctl()?
            .args(["--no-mock", "device", "list"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn flag_is_documented_in_help() -> TestResult {
        Command::cargo_bin("wheelctl")?
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--no-mock"));
        Ok(())
    }

    #[test]
    fn env_var_fails_instead_of_simulating() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_NO_MOCK", "1")
            .args(["device", "list"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn openracing_env_var_is_also_honoured() -> TestResult {
        wheelctl()?
            .env("OPENRACING_NO_MOCK", "1")
            .args(["device", "list"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn unset_env_var_leaves_the_fallback_enabled() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_NO_MOCK", "0")
            .args(["device", "list"])
            .assert()
            .success();
        Ok(())
    }
}

// ===========================================================================
// `wheelctl health` can say no
// ===========================================================================

mod health_reports_real_service_state {
    use super::*;

    #[test]
    fn human_output_does_not_claim_the_service_is_running() -> TestResult {
        let output = wheelctl()?.arg("health").output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            !stdout.contains("Service: Running"),
            "health claimed the service was running with no daemon:\n{stdout}"
        );
        Ok(())
    }

    #[test]
    fn human_output_says_the_service_is_not_running() -> TestResult {
        wheelctl()?
            .arg("health")
            .assert()
            .stdout(predicate::str::contains("Not running"));
        Ok(())
    }

    #[test]
    fn json_service_status_is_not_running() -> TestResult {
        let output = wheelctl()?.args(["--json", "health"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(
            parsed.get("service_status").and_then(|v| v.as_str()),
            Some("not_running"),
            "unexpected service_status in {stdout}"
        );
        Ok(())
    }

    #[test]
    fn json_reports_the_simulated_backend() -> TestResult {
        let output = wheelctl()?.args(["--json", "health"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(
            parsed.get("backend").and_then(|v| v.as_str()),
            Some("simulated"),
            "unexpected backend in {stdout}"
        );
        Ok(())
    }

    #[test]
    fn json_overall_health_is_not_healthy_without_a_service() -> TestResult {
        let output = wheelctl()?.args(["--json", "health"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(
            parsed.get("overall_health").and_then(|v| v.as_str()),
            Some("service_unavailable"),
            "unexpected overall_health in {stdout}"
        );
        Ok(())
    }
}

// ===========================================================================
// Safety writes refuse to pretend
// ===========================================================================

mod safety_requires_a_live_service {
    use super::*;

    #[test]
    fn emergency_stop_fails_rather_than_reporting_success() -> TestResult {
        // An e-stop that reports success without reaching a device is the
        // worst possible outcome for a torque-producing product.
        wheelctl()?
            .args(["safety", "stop"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn emergency_stop_error_says_how_to_start_the_service() -> TestResult {
        wheelctl()?.args(["safety", "stop"]).assert().stderr(
            predicate::str::contains("systemctl")
                .or(predicate::str::contains("sc start"))
                .or(predicate::str::contains("launchctl")),
        );
        Ok(())
    }

    #[test]
    fn enabling_high_torque_fails() -> TestResult {
        wheelctl()?
            .args(["safety", "enable", "wheel-001"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn setting_a_torque_limit_fails() -> TestResult {
        wheelctl()?
            .args(["safety", "limit", "wheel-001", "5.0"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn read_only_status_still_works_offline() -> TestResult {
        // Read-only commands keep the offline fallback; only writes refuse.
        wheelctl()?.args(["safety", "status"]).assert().success();
        Ok(())
    }
}

// ===========================================================================
// `--no-mock` only constrains commands that need the service
// ===========================================================================

mod no_mock_leaves_local_commands_alone {
    use super::*;

    /// `--no-mock` says "do not hand me simulated device data". Commands that
    /// only read files or bundled tables have no device data to simulate, so
    /// requiring a daemon for them is pure obstruction -- and it was the
    /// default, because the profile and game dispatchers built a client before
    /// matching the subcommand.
    fn assert_runs_without_a_daemon(args: &[&str]) -> TestResult {
        let output = wheelctl()?.arg("--no-mock").args(args).output()?;
        assert_ne!(
            output.status.code(),
            Some(EXIT_SERVICE_UNAVAILABLE),
            "`{}` demanded a running service it never uses; stderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    #[test]
    fn game_list_does_not_need_a_service() -> TestResult {
        assert_runs_without_a_daemon(&["game", "list"])
    }

    #[test]
    fn profile_list_does_not_need_a_service() -> TestResult {
        assert_runs_without_a_daemon(&["profile", "list"])
    }

    #[test]
    fn profile_validate_does_not_need_a_service() -> TestResult {
        // Validation reads a file off disk; a missing file should be reported
        // as a missing file, never as a missing daemon.
        assert_runs_without_a_daemon(&["profile", "validate", "no-such-profile.json"])
    }

    #[test]
    fn profile_apply_still_requires_a_service() -> TestResult {
        // The other half of the contract: the one profile subcommand that does
        // talk to the service must keep refusing under --no-mock.
        wheelctl()?
            .args(["--no-mock", "profile", "apply", "wheel-001", "p.json"])
            .assert()
            .code(EXIT_SERVICE_UNAVAILABLE);
        Ok(())
    }
}

// ===========================================================================
// `safety limit` does not point at another silent no-op
// ===========================================================================

mod safety_limit_guidance {
    use super::*;

    #[test]
    fn does_not_recommend_profile_apply() -> TestResult {
        // `apply_profile` ignores its profile argument and sends `base: None`,
        // so `torqueCapNm` never reaches the service. Recommending it as a
        // torque-cap workaround would send the user chasing a cap that is
        // silently dropped.
        let output = wheelctl()?
            .args(["safety", "limit", "wheel-001", "5.0"])
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.contains("profile apply"),
            "safety limit points at profile apply, which drops the cap:\n{combined}"
        );
        Ok(())
    }
}
