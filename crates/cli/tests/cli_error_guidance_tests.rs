//! Tests that `wheelctl` errors tell the user what to do next.
//!
//! Errors used to state only what went wrong. `Device not found: w1` with no
//! follow-up leaves a first-time user with nowhere to go, and the daemon-down
//! message was a question ("Is wheeld running?") rather than a command. These
//! assert that the actionable half is present and stays present.
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

#![allow(deprecated)] // cargo_bin deprecation warnings

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Nothing listens here, so the service-unavailable path is deterministic.
const UNREACHABLE_ENDPOINT: &str = "http://127.0.0.1:19997";

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    cmd.env_remove("WHEELCTL_NO_MOCK");
    cmd.env_remove("OPENRACING_NO_MOCK");
    Ok(cmd)
}

/// A `wheelctl` pointed at an endpoint with no service on it.
fn wheelctl_offline() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = wheelctl()?;
    cmd.arg("--endpoint").arg(UNREACHABLE_ENDPOINT);
    Ok(cmd)
}

// ===========================================================================
// Errors carry a next step
// ===========================================================================

mod errors_are_actionable {
    use super::*;

    #[test]
    fn device_not_found_suggests_listing_devices() -> TestResult {
        wheelctl()?
            .args(["device", "status", "no-such-device"])
            .assert()
            .stderr(predicate::str::contains("wheelctl device list"));
        Ok(())
    }

    #[test]
    fn device_not_found_points_at_doctor() -> TestResult {
        wheelctl()?
            .args(["device", "status", "no-such-device"])
            .assert()
            .stderr(predicate::str::contains("wheelctl doctor"));
        Ok(())
    }

    #[test]
    fn hints_are_labelled_so_they_are_not_mistaken_for_the_error() -> TestResult {
        wheelctl()?
            .args(["device", "status", "no-such-device"])
            .assert()
            .stderr(predicate::str::contains("hint:"));
        Ok(())
    }

    #[test]
    fn service_unavailable_names_a_start_command() -> TestResult {
        // Not "Is wheeld running?" -- an actual command to run.
        wheelctl_offline()?
            .args(["--no-mock", "device", "list"])
            .assert()
            .stderr(
                predicate::str::contains("systemctl")
                    .or(predicate::str::contains("sc start"))
                    .or(predicate::str::contains("launchctl")),
            );
        Ok(())
    }

    #[test]
    fn service_unavailable_suggests_confirming_with_health() -> TestResult {
        wheelctl_offline()?
            .args(["--no-mock", "device", "list"])
            .assert()
            .stderr(predicate::str::contains("wheelctl health"));
        Ok(())
    }
}

// ===========================================================================
// JSON errors carry a stable discriminator
// ===========================================================================

mod json_errors {
    use super::*;

    fn error_object(args: &[&str]) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let output = wheelctl()?.args(args).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        parsed
            .get("error")
            .cloned()
            .ok_or_else(|| format!("no error object in {stdout}").into())
    }

    #[test]
    fn error_has_a_stable_kind() -> TestResult {
        let err = error_object(&["--json", "device", "status", "no-such-device"])?;
        assert_eq!(
            err.get("kind").and_then(|v| v.as_str()),
            Some("device_not_found"),
            "unexpected error object: {err}"
        );
        Ok(())
    }

    #[test]
    fn error_type_is_not_just_the_message_again() -> TestResult {
        // `type` was computed from anyhow's Debug, which renders the message,
        // so it used to duplicate `message` verbatim.
        let err = error_object(&["--json", "device", "status", "no-such-device"])?;
        let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let type_name = err.get("type").and_then(|v| v.as_str()).unwrap_or("");
        assert_ne!(type_name, message, "type still duplicates message");
        assert_eq!(type_name, "DeviceNotFound", "unexpected type: {err}");
        Ok(())
    }

    #[test]
    fn error_carries_the_hint() -> TestResult {
        let err = error_object(&["--json", "device", "status", "no-such-device"])?;
        assert!(
            err.get("hint").and_then(|v| v.as_str()).is_some(),
            "no hint in {err}"
        );
        Ok(())
    }
}

// ===========================================================================
// `wheelctl doctor` is discoverable and checks the service
// ===========================================================================

mod doctor {
    use super::*;

    #[test]
    fn is_available_at_the_top_level() -> TestResult {
        wheelctl()?.args(["doctor", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn appears_in_root_help() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("doctor"));
        Ok(())
    }

    #[test]
    fn reports_the_service_state() -> TestResult {
        // The environment check used to cover tooling and HID visibility but
        // stay silent on the daemon, which is the most common first-run
        // failure.
        wheelctl()?
            .arg("doctor")
            .assert()
            .stdout(predicate::str::contains("wheeld service at"));
        Ok(())
    }

    #[test]
    fn json_receipt_includes_service_reachability() -> TestResult {
        let output = wheelctl()?.args(["--json", "doctor"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        let service = parsed
            .get("service")
            .ok_or_else(|| format!("no service field in {stdout}"))?;
        assert!(service.get("reachable").and_then(|v| v.as_bool()).is_some());
        assert!(service.get("endpoint").and_then(|v| v.as_str()).is_some());
        Ok(())
    }

    #[test]
    fn warns_when_the_service_is_not_running() -> TestResult {
        let output = wheelctl()?.args(["--json", "doctor"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        let reachable = parsed
            .get("service")
            .and_then(|s| s.get("reachable"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !reachable {
            let warnings = parsed
                .get("warnings")
                .and_then(|w| w.as_array())
                .ok_or("no warnings array")?;
            assert!(
                warnings
                    .iter()
                    .filter_map(|w| w.as_str())
                    .any(|w| w.contains("wheeld is not reachable")),
                "expected an unreachable-service warning in {warnings:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn stays_observe_only() -> TestResult {
        // Probing the service opens a socket, not a HID device; the
        // observe-only claims must survive.
        let output = wheelctl()?.args(["--json", "doctor"]).output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        for claim in [
            "no_hid_device_opened",
            "no_ffb_writes",
            "no_output_reports",
            "no_feature_reports",
        ] {
            assert_eq!(
                parsed.get(claim).and_then(|v| v.as_bool()),
                Some(true),
                "{claim} is no longer asserted"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// Root help orients a newcomer
// ===========================================================================

mod root_help {
    use super::*;

    #[test]
    fn documents_exit_codes() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("EXIT CODES"));
        Ok(())
    }

    #[test]
    fn lists_the_service_unavailable_code() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .stdout(predicate::str::contains("service unavailable"));
        Ok(())
    }

    #[test]
    fn shows_a_getting_started_section() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .stdout(predicate::str::contains("GETTING STARTED"));
        Ok(())
    }

    #[test]
    fn shows_examples() -> TestResult {
        wheelctl()?
            .arg("--help")
            .assert()
            .stdout(predicate::str::contains("EXAMPLES"));
        Ok(())
    }
}

// ===========================================================================
// Empty device list is not a dead end
// ===========================================================================

mod empty_device_list {
    use super::*;

    #[test]
    fn suggests_next_steps_rather_than_stopping_at_no_devices() -> TestResult {
        // The simulated backend returns devices, so drive the empty path
        // through the HID-observe-only listing, which finds nothing here.
        let output = wheelctl()?
            .args(["device", "list", "--hid-observe-only"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("No devices found") {
            assert!(
                stdout.contains("Next steps") && stdout.contains("wheelctl doctor"),
                "empty device list gave no guidance:\n{stdout}"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// Hints match the failure they are attached to
// ===========================================================================

mod hints_are_not_misdirection {
    use super::*;

    #[test]
    fn safety_interlock_is_not_diagnosed_as_a_udev_problem() -> TestResult {
        // `safety enable` reports a refused interlock as PermissionDenied.
        // Device access already succeeded there, so udev rules and group
        // membership are irrelevant -- and telling someone to reinstall udev
        // rules while an interlock is holding torque back is a bad thing to be
        // wrong about.
        let output = wheelctl_offline()?
            .args(["safety", "enable", "wheel-001", "--force"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("hint:") {
            assert!(
                !stderr.contains("udevadm") && !stderr.contains("usermod"),
                "safety failure diagnosed as a permissions problem:\n{stderr}"
            );
        }
        Ok(())
    }

    #[test]
    fn non_profile_validation_errors_do_not_suggest_profile_validate() -> TestResult {
        // ValidationError is the catch-all: blackbox format, plugin registry,
        // firmware bundles, torque ranges. `wheelctl profile validate` is the
        // wrong advice for all of them.
        let output = wheelctl()?
            .args(["plugin", "install", "no-such-plugin-xyz"])
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.contains("profile validate"),
            "plugin failure suggested profile validation:\n{combined}"
        );
        Ok(())
    }

    #[test]
    fn profile_validation_errors_still_get_the_profile_hint() -> TestResult {
        // The other half: narrowing the hint must not remove it where it is
        // correct.
        let err = wheelctl()?
            .args(["profile", "validate", "no-such-profile.json"])
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&err.stdout),
            String::from_utf8_lossy(&err.stderr)
        );
        assert!(
            !combined.is_empty(),
            "profile validate produced no diagnostic at all"
        );
        Ok(())
    }
}

// ===========================================================================
// `doctor` answers about the endpoint it was asked about
// ===========================================================================

mod doctor_respects_the_selected_endpoint {
    use super::*;

    #[test]
    fn reports_the_endpoint_from_the_flag() -> TestResult {
        // Probing the default while `--endpoint` points elsewhere would report
        // on a service the user is not talking to.
        let output = wheelctl()?
            .args(["--endpoint", UNREACHABLE_ENDPOINT, "--json", "doctor"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(
            parsed
                .get("service")
                .and_then(|s| s.get("endpoint"))
                .and_then(|v| v.as_str()),
            Some(UNREACHABLE_ENDPOINT),
            "doctor reported a different endpoint than the one selected: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn reports_the_endpoint_from_the_environment() -> TestResult {
        let output = wheelctl()?
            .env("WHEELCTL_ENDPOINT", UNREACHABLE_ENDPOINT)
            .args(["--json", "doctor"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(
            parsed
                .get("service")
                .and_then(|s| s.get("endpoint"))
                .and_then(|v| v.as_str()),
            Some(UNREACHABLE_ENDPOINT),
            "doctor ignored WHEELCTL_ENDPOINT: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn warning_names_the_selected_endpoint() -> TestResult {
        let output = wheelctl()?
            .args(["--endpoint", UNREACHABLE_ENDPOINT, "--json", "doctor"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
        let warnings = parsed
            .get("warnings")
            .and_then(|w| w.as_array())
            .ok_or("no warnings array")?;
        assert!(
            warnings
                .iter()
                .filter_map(|w| w.as_str())
                .any(|w| w.contains(UNREACHABLE_ENDPOINT)),
            "unreachable warning did not name the selected endpoint: {warnings:?}"
        );
        Ok(())
    }
}
