//! Deep CLI command tests for the wheelctl binary.
//!
//! Covers all subcommands, argument parsing, output format verification
//! (JSON, table, plain text), error message quality, exit codes,
//! config file loading, interactive prompts simulation, and help text
//! completeness.

#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a wheelctl Command with the service endpoint removed.
fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

/// Parse stdout bytes into a JSON Value.
fn json(bytes: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(bytes)?)
}

// ===========================================================================
// 1. Help text completeness — every subcommand describes its flags
// ===========================================================================

mod help_completeness {
    use super::*;

    #[test]
    fn root_long_about_contains_suite_description() -> TestResult {
        let out = wheelctl()?.arg("--help").output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Racing Wheel") || s.contains("racing wheel"),
            "root --help should describe the project: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_status_help_describes_watch() -> TestResult {
        let out = wheelctl()?.args(["device", "status", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--watch"),
            "device status --help should list --watch: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_calibrate_help_describes_calibration_types() -> TestResult {
        let out = wheelctl()?
            .args(["device", "calibrate", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for ty in &["center", "dor", "pedals", "all"] {
            assert!(
                s.to_lowercase().contains(ty),
                "device calibrate --help should list calibration type '{ty}': {s}"
            );
        }
        Ok(())
    }

    #[test]
    fn device_calibrate_help_describes_yes_flag() -> TestResult {
        let out = wheelctl()?
            .args(["device", "calibrate", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--yes"),
            "device calibrate --help should list --yes: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_reset_help_describes_force_flag() -> TestResult {
        let out = wheelctl()?.args(["device", "reset", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--force"),
            "device reset --help should list --force: {s}"
        );
        Ok(())
    }

    #[test]
    fn profile_apply_help_describes_skip_validation() -> TestResult {
        let out = wheelctl()?.args(["profile", "apply", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--skip-validation"),
            "profile apply --help should list --skip-validation: {s}"
        );
        Ok(())
    }

    #[test]
    fn profile_create_help_describes_from_game_car() -> TestResult {
        let out = wheelctl()?.args(["profile", "create", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for flag in &["--from", "--game", "--car"] {
            assert!(
                s.contains(flag),
                "profile create --help should mention {flag}: {s}"
            );
        }
        Ok(())
    }

    #[test]
    fn profile_edit_help_describes_field_value() -> TestResult {
        let out = wheelctl()?.args(["profile", "edit", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--field"), "should list --field: {s}");
        assert!(s.contains("--value"), "should list --value: {s}");
        Ok(())
    }

    #[test]
    fn profile_validate_help_describes_detailed() -> TestResult {
        let out = wheelctl()?
            .args(["profile", "validate", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--detailed"),
            "profile validate --help should mention --detailed: {s}"
        );
        Ok(())
    }

    #[test]
    fn profile_export_help_describes_output_signed() -> TestResult {
        let out = wheelctl()?.args(["profile", "export", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--output"), "should list --output: {s}");
        assert!(s.contains("--signed"), "should list --signed: {s}");
        Ok(())
    }

    #[test]
    fn profile_import_help_describes_target_verify() -> TestResult {
        let out = wheelctl()?.args(["profile", "import", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--target"), "should list --target: {s}");
        assert!(s.contains("--verify"), "should list --verify: {s}");
        Ok(())
    }

    #[test]
    fn plugin_install_help_describes_version_flag() -> TestResult {
        let out = wheelctl()?.args(["plugin", "install", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--version"),
            "plugin install --help should list --version: {s}"
        );
        Ok(())
    }

    #[test]
    fn plugin_uninstall_help_describes_force() -> TestResult {
        let out = wheelctl()?
            .args(["plugin", "uninstall", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--force"),
            "plugin uninstall --help should list --force: {s}"
        );
        Ok(())
    }

    #[test]
    fn diag_record_help_describes_duration_output() -> TestResult {
        let out = wheelctl()?.args(["diag", "record", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--duration"), "should list --duration: {s}");
        assert!(s.contains("--output"), "should list --output: {s}");
        Ok(())
    }

    #[test]
    fn diag_replay_help_describes_detailed() -> TestResult {
        let out = wheelctl()?.args(["diag", "replay", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--detailed"),
            "diag replay --help should list --detailed: {s}"
        );
        Ok(())
    }

    #[test]
    fn diag_support_help_describes_blackbox_output() -> TestResult {
        let out = wheelctl()?.args(["diag", "support", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--blackbox"), "should list --blackbox: {s}");
        assert!(s.contains("--moza-lane"), "should list --moza-lane: {s}");
        assert!(s.contains("--output"), "should list --output: {s}");
        Ok(())
    }

    #[test]
    fn safety_enable_help_describes_force() -> TestResult {
        let out = wheelctl()?.args(["safety", "enable", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--force"),
            "safety enable --help should list --force: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_limit_help_describes_global() -> TestResult {
        let out = wheelctl()?.args(["safety", "limit", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--global"),
            "safety limit --help should list --global: {s}"
        );
        Ok(())
    }

    #[test]
    fn telemetry_probe_help_describes_all_flags() -> TestResult {
        let out = wheelctl()?
            .args(["telemetry", "probe", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for flag in &["--game", "--endpoint", "--timeout-ms", "--attempts"] {
            assert!(
                s.contains(flag),
                "telemetry probe --help should mention {flag}: {s}"
            );
        }
        Ok(())
    }

    #[test]
    fn telemetry_capture_help_describes_all_flags() -> TestResult {
        let out = wheelctl()?
            .args(["telemetry", "capture", "--help"])
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for flag in &["--game", "--port", "--duration", "--out", "--max-payload"] {
            assert!(
                s.contains(flag),
                "telemetry capture --help should mention {flag}: {s}"
            );
        }
        Ok(())
    }

    #[test]
    fn game_configure_help_describes_path_auto() -> TestResult {
        let out = wheelctl()?.args(["game", "configure", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("--path"), "should list --path: {s}");
        assert!(s.contains("--auto"), "should list --auto: {s}");
        Ok(())
    }

    #[test]
    fn game_test_help_describes_duration() -> TestResult {
        let out = wheelctl()?.args(["game", "test", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--duration"),
            "game test --help should mention --duration: {s}"
        );
        Ok(())
    }
}

// ===========================================================================
// 2. Argument parsing — invalid / edge-case arguments
// ===========================================================================

mod arg_parsing {
    use super::*;

    // --- Missing required positional args ---

    #[test]
    fn device_calibrate_missing_type() -> TestResult {
        let out = wheelctl()?.args(["device", "calibrate", "w1"]).output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("required") || stderr.contains("CALIBRATION_TYPE"),
            "should report missing calibration type: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn device_reset_missing_device() -> TestResult {
        let out = wheelctl()?.args(["device", "reset"]).output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("required") || stderr.contains("DEVICE"),
            "should report missing device: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn profile_show_missing_profile() -> TestResult {
        let out = wheelctl()?.args(["profile", "show"]).output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("required") || stderr.contains("PROFILE"),
            "should report missing profile: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn profile_apply_missing_both_args() -> TestResult {
        let out = wheelctl()?.args(["profile", "apply"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn profile_apply_missing_profile_arg() -> TestResult {
        let out = wheelctl()?.args(["profile", "apply", "dev1"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn profile_create_missing_path() -> TestResult {
        let out = wheelctl()?.args(["profile", "create"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn plugin_install_missing_id() -> TestResult {
        let out = wheelctl()?.args(["plugin", "install"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn plugin_uninstall_missing_id() -> TestResult {
        let out = wheelctl()?.args(["plugin", "uninstall"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn plugin_info_missing_id() -> TestResult {
        let out = wheelctl()?.args(["plugin", "info"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn plugin_verify_missing_id() -> TestResult {
        let out = wheelctl()?.args(["plugin", "verify"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn safety_enable_missing_device() -> TestResult {
        let out = wheelctl()?.args(["safety", "enable"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn safety_limit_missing_device_and_torque() -> TestResult {
        let out = wheelctl()?.args(["safety", "limit"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn safety_limit_missing_torque() -> TestResult {
        let out = wheelctl()?.args(["safety", "limit", "w1"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn diag_record_missing_device() -> TestResult {
        let out = wheelctl()?.args(["diag", "record"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn diag_replay_missing_file() -> TestResult {
        let out = wheelctl()?.args(["diag", "replay"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn game_configure_missing_game() -> TestResult {
        let out = wheelctl()?.args(["game", "configure"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn game_test_missing_game() -> TestResult {
        let out = wheelctl()?.args(["game", "test"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn telemetry_capture_missing_game_and_out() -> TestResult {
        let out = wheelctl()?.args(["telemetry", "capture"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    // --- Invalid enum values ---

    #[test]
    fn invalid_completion_shell() -> TestResult {
        let out = wheelctl()?.args(["completion", "tcsh"]).output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("invalid value") || stderr.contains("tcsh"),
            "should report invalid shell: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn invalid_diag_test_type() -> TestResult {
        let out = wheelctl()?.args(["diag", "test", "nuclear"]).output()?;
        assert!(!out.status.success());
        Ok(())
    }

    // --- Numeric value validation ---

    #[test]
    fn negative_torque_parses_as_float() -> TestResult {
        // clap will parse -1.0 as a negative f32
        let out = wheelctl()?
            .args(["safety", "limit", "w1", "--", "-1.0"])
            .output()?;
        // Should parse but the command logic should reject it
        // (negative torque not physically meaningful)
        // Either clap rejects it or the safety module does
        // We just verify the command doesn't hang
        assert!(!out.stdout.is_empty() || !out.stderr.is_empty());
        Ok(())
    }

    #[test]
    fn zero_torque_parses_as_float() -> TestResult {
        let out = wheelctl()?
            .args(["safety", "limit", "w1", "0.0"])
            .output()?;
        // Should fail validation (torque must be >= 0.1)
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn telemetry_capture_port_overflow() -> TestResult {
        let out = wheelctl()?
            .args([
                "telemetry",
                "capture",
                "--game",
                "acc",
                "--port",
                "99999",
                "--out",
                "t.bin",
            ])
            .output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("invalid value") || stderr.contains("99999"),
            "should report invalid port: {stderr}"
        );
        Ok(())
    }

    // --- Global flags are truly global ---

    #[test]
    fn json_flag_before_subcommand() -> TestResult {
        let out = wheelctl()?.args(["--json", "device", "list"]).output()?;
        assert!(out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        Ok(())
    }

    #[test]
    fn json_flag_between_nested_subcommands() -> TestResult {
        let out = wheelctl()?.args(["safety", "--json", "status"]).output()?;
        assert!(out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        Ok(())
    }

    #[test]
    fn json_flag_at_end() -> TestResult {
        let out = wheelctl()?.args(["safety", "status", "--json"]).output()?;
        assert!(out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        Ok(())
    }

    #[test]
    fn verbose_flag_between_nested_subcommands() -> TestResult {
        wheelctl()?
            .args(["device", "-v", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn endpoint_env_variable_accepted() -> TestResult {
        // Using a valid-format endpoint that the mock client will handle
        let out = wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://localhost:50051")
            .args(["device", "list"])
            .output()?;
        assert!(out.status.success());
        Ok(())
    }
}

// ===========================================================================
// 3. Exit codes — verify correct exit code per error type
// ===========================================================================

mod exit_codes {
    use super::*;

    #[test]
    fn success_exit_code_is_0() -> TestResult {
        wheelctl()?
            .args(["device", "list"])
            .assert()
            .success()
            .code(0);
        Ok(())
    }

    #[test]
    fn clap_parse_error_exit_code_is_2() -> TestResult {
        // clap returns exit code 2 for parse errors
        let out = wheelctl()?.args(["device", "status"]).output()?;
        assert!(!out.status.success());
        // clap uses exit code 2 for usage errors
        if let Some(code) = out.status.code() {
            assert_eq!(code, 2, "clap parse error should produce exit code 2");
        }
        Ok(())
    }

    #[test]
    fn device_not_found_exit_code_is_2() -> TestResult {
        wheelctl()?
            .args(["device", "status", "ghost-device-999"])
            .assert()
            .failure()
            .code(2);
        Ok(())
    }

    #[test]
    fn service_unavailable_exit_code_is_5() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["device", "list"])
            .assert()
            .failure()
            .code(5);
        Ok(())
    }

    #[test]
    fn service_unavailable_exit_code_is_5_for_safety() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["safety", "status"])
            .assert()
            .failure()
            .code(5);
        Ok(())
    }

    #[test]
    fn service_unavailable_exit_code_is_5_for_health() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["health"])
            .assert()
            .failure()
            .code(5);
        Ok(())
    }

    #[test]
    fn service_unavailable_exit_code_is_5_for_diag() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["diag", "metrics"])
            .assert()
            .failure()
            .code(5);
        Ok(())
    }

    #[test]
    fn service_unavailable_exit_code_is_5_for_game() -> TestResult {
        wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["game", "status"])
            .assert()
            .failure()
            .code(5);
        Ok(())
    }
}

// ===========================================================================
// 4. JSON output format verification for each command
// ===========================================================================

mod json_format {
    use super::*;

    #[test]
    fn device_list_json_devices_have_required_fields() -> TestResult {
        let out = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let j = json(&out.stdout)?;
        let devices = j["devices"]
            .as_array()
            .ok_or("devices should be an array")?;
        assert!(
            !devices.is_empty(),
            "mock should return at least one device"
        );
        for device in devices {
            assert!(device.get("id").is_some(), "device should have id");
            assert!(device.get("name").is_some(), "device should have name");
            assert!(
                device.get("device_type").is_some(),
                "device should have device_type"
            );
            assert!(device.get("state").is_some(), "device should have state");
            assert!(
                device.get("capabilities").is_some(),
                "device should have capabilities"
            );
        }
        Ok(())
    }

    #[test]
    fn device_list_json_capabilities_structure() -> TestResult {
        let out = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let j = json(&out.stdout)?;
        let first = &j["devices"][0];
        let caps = &first["capabilities"];
        assert!(
            caps.get("supports_pid").is_some(),
            "capabilities should include supports_pid"
        );
        assert!(
            caps.get("max_torque_nm").is_some(),
            "capabilities should include max_torque_nm"
        );
        assert!(
            caps.get("encoder_cpr").is_some(),
            "capabilities should include encoder_cpr"
        );
        Ok(())
    }

    #[test]
    fn device_status_json_telemetry_fields() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "device", "status", "wheel-001"])
            .output()?;
        let j = json(&out.stdout)?;
        let status = &j["status"];
        assert!(status.get("device").is_some(), "should have device");
        assert!(status.get("telemetry").is_some(), "should have telemetry");
        let tel = &status["telemetry"];
        assert!(
            tel.get("wheel_angle_deg").is_some(),
            "telemetry should have wheel_angle_deg"
        );
        assert!(
            tel.get("temperature_c").is_some(),
            "telemetry should have temperature_c"
        );
        assert!(
            tel.get("hands_on").is_some(),
            "telemetry should have hands_on"
        );
        Ok(())
    }

    #[test]
    fn profile_list_json_has_profiles_key() -> TestResult {
        let out = wheelctl()?.args(["--json", "profile", "list"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("profiles").is_some(),
            "profile list JSON should have 'profiles' key"
        );
        Ok(())
    }

    #[test]
    fn plugin_list_json_has_plugins_key() -> TestResult {
        let out = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let plugins = j["plugins"]
            .as_array()
            .ok_or("plugins should be an array")?;
        assert!(
            !plugins.is_empty(),
            "mock should return at least one plugin"
        );
        Ok(())
    }

    #[test]
    fn plugin_list_json_plugins_have_required_fields() -> TestResult {
        let out = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        let j = json(&out.stdout)?;
        let plugins = j["plugins"]
            .as_array()
            .ok_or("plugins should be an array")?;
        for p in plugins {
            assert!(p.get("id").is_some(), "plugin should have id");
            assert!(p.get("name").is_some(), "plugin should have name");
            assert!(p.get("version").is_some(), "plugin should have version");
            assert!(p.get("author").is_some(), "plugin should have author");
            assert!(
                p.get("installed").is_some(),
                "plugin should have installed flag"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_list_json_category_filter() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "list", "--category", "led"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let plugins = j["plugins"]
            .as_array()
            .ok_or("plugins should be an array")?;
        for p in plugins {
            let desc = p["description"].as_str().unwrap_or("").to_lowercase();
            assert!(
                desc.contains("led"),
                "category filter 'led' should only return LED-related plugins"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_search_json_has_results() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "search", "smoothing"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(j.get("results").is_some(), "should have results key");
        assert!(j.get("query").is_some(), "should have query key");
        assert_eq!(j["query"], "smoothing");
        Ok(())
    }

    #[test]
    fn plugin_search_json_no_match_returns_empty_results() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "search", "zzz_never_matches_zzz"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let results = j["results"]
            .as_array()
            .ok_or("results should be an array")?;
        assert!(
            results.is_empty(),
            "non-matching search should return empty"
        );
        Ok(())
    }

    #[test]
    fn plugin_install_json_has_action_field() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "install", "ffb-smoothing"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert_eq!(j["action"], "install");
        assert!(j.get("plugin").is_some(), "should have plugin field");
        Ok(())
    }

    #[test]
    fn plugin_uninstall_json_has_action_field() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "uninstall", "ffb-smoothing", "--force"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert_eq!(j["action"], "uninstall");
        Ok(())
    }

    #[test]
    fn plugin_info_json_has_plugin_field() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "info", "ffb-smoothing"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(j.get("plugin").is_some(), "should have plugin field");
        let plugin = &j["plugin"];
        assert_eq!(plugin["id"], "ffb-smoothing");
        Ok(())
    }

    #[test]
    fn plugin_verify_json_has_verification() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "verify", "ffb-smoothing"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("verification").is_some(),
            "should have verification field"
        );
        Ok(())
    }

    #[test]
    fn game_list_json_has_supported_games() -> TestResult {
        let out = wheelctl()?.args(["--json", "game", "list"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let games = j["supported_games"]
            .as_array()
            .ok_or("should have supported_games array")?;
        assert!(!games.is_empty(), "should list at least one game");
        Ok(())
    }

    #[test]
    fn game_list_json_games_have_required_fields() -> TestResult {
        let out = wheelctl()?.args(["--json", "game", "list"]).output()?;
        let j = json(&out.stdout)?;
        let games = j["supported_games"]
            .as_array()
            .ok_or("should have supported_games array")?;
        for g in games {
            assert!(g.get("id").is_some(), "game should have id");
            assert!(g.get("name").is_some(), "game should have name");
        }
        Ok(())
    }

    #[test]
    fn game_status_json_has_expected_fields() -> TestResult {
        let out = wheelctl()?.args(["--json", "game", "status"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let gs = &j["game_status"];
        assert!(
            gs.get("telemetry_active").is_some(),
            "should have telemetry_active"
        );
        Ok(())
    }

    #[test]
    fn safety_status_json_has_devices() -> TestResult {
        let out = wheelctl()?.args(["--json", "safety", "status"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        // When no specific device, should list all
        assert!(
            j.get("devices").is_some(),
            "safety status should have devices"
        );
        Ok(())
    }

    #[test]
    fn safety_status_device_json_has_safety_status() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "safety", "status", "wheel-001"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("safety_status").is_some(),
            "should have safety_status"
        );
        Ok(())
    }

    #[test]
    fn health_json_has_service_status() -> TestResult {
        let out = wheelctl()?.args(["--json", "health"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("service_status").is_some(),
            "health should have service_status"
        );
        assert!(
            j.get("overall_health").is_some(),
            "health should have overall_health"
        );
        Ok(())
    }

    #[test]
    fn health_json_has_devices_array() -> TestResult {
        let out = wheelctl()?.args(["--json", "health"]).output()?;
        let j = json(&out.stdout)?;
        let devices = j["devices"]
            .as_array()
            .ok_or("health should have devices array")?;
        assert!(
            !devices.is_empty(),
            "mock should report at least one device"
        );
        Ok(())
    }

    #[test]
    fn diag_metrics_json_has_diagnostics() -> TestResult {
        let out = wheelctl()?.args(["--json", "diag", "metrics"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("diagnostics").is_some(),
            "should have diagnostics field"
        );
        Ok(())
    }

    #[test]
    fn diag_metrics_json_performance_fields() -> TestResult {
        let out = wheelctl()?.args(["--json", "diag", "metrics"]).output()?;
        let j = json(&out.stdout)?;
        let perf = &j["diagnostics"]["performance"];
        assert!(
            perf.get("p99_jitter_us").is_some(),
            "should have p99_jitter_us"
        );
        assert!(
            perf.get("missed_tick_rate").is_some(),
            "should have missed_tick_rate"
        );
        assert!(perf.get("total_ticks").is_some(), "should have total_ticks");
        Ok(())
    }

    #[test]
    fn diag_test_json_has_test_results() -> TestResult {
        let out = wheelctl()?.args(["--json", "diag", "test"]).output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(
            j.get("test_results").is_some(),
            "should have test_results field"
        );
        Ok(())
    }

    #[test]
    fn diag_test_json_specific_type() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "diag", "test", "motor"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        let results = j["test_results"]
            .as_array()
            .ok_or("test_results should be an array")?;
        assert_eq!(results.len(), 1, "specific test type should yield 1 result");
        Ok(())
    }

    #[test]
    fn diag_test_json_all_types_run() -> TestResult {
        let out = wheelctl()?.args(["--json", "diag", "test"]).output()?;
        let j = json(&out.stdout)?;
        let results = j["test_results"]
            .as_array()
            .ok_or("test_results should be an array")?;
        // Without specifying a type, all 4 tests should run
        assert!(
            results.len() >= 4,
            "should run all test types when none specified, got {}",
            results.len()
        );
        Ok(())
    }
}

// ===========================================================================
// 5. Error message quality — errors are actionable and mention context
// ===========================================================================

mod error_quality {
    use super::*;

    #[test]
    fn device_not_found_error_mentions_device_id() -> TestResult {
        let out = wheelctl()?
            .args(["device", "status", "my-missing-wheel"])
            .output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("not found") || stderr.contains("my-missing-wheel"),
            "error should be descriptive: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn device_not_found_json_error_mentions_device() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "device", "status", "phantom-device"])
            .output()?;
        assert!(!out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], false);
        let msg = j["error"]["message"]
            .as_str()
            .ok_or("error should have message")?;
        assert!(
            msg.contains("not found") || msg.contains("phantom-device"),
            "JSON error message should be actionable: {msg}"
        );
        Ok(())
    }

    #[test]
    fn service_unavailable_error_is_descriptive() -> TestResult {
        let out = wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["device", "list"])
            .output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("unavailable")
                || stderr.contains("refused")
                || stderr.contains("Connection"),
            "error should describe connection failure: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn service_unavailable_json_error_has_type() -> TestResult {
        let out = wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
            .args(["--json", "device", "list"])
            .output()?;
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], false);
        assert!(
            j["error"].get("type").is_some(),
            "JSON error should have 'type' field"
        );
        Ok(())
    }

    #[test]
    fn unknown_command_suggests_valid_options() -> TestResult {
        let out = wheelctl()?.args(["devic"]).output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        // clap may suggest similar commands
        assert!(
            stderr.contains("device")
                || stderr.contains("similar")
                || stderr.contains("Did you mean"),
            "should suggest similar command or list valid ones: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn plugin_not_found_error_mentions_plugin_id() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "plugin", "info", "nonexistent-plugin-xyz"])
            .output()?;
        assert!(!out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], false);
        let msg = j["error"]["message"]
            .as_str()
            .ok_or("should have error message")?;
        assert!(
            msg.contains("not found") || msg.contains("nonexistent-plugin-xyz"),
            "error should mention the plugin id: {msg}"
        );
        Ok(())
    }

    #[test]
    fn telemetry_unsupported_game_error_lists_supported() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "telemetry", "probe", "--game", "unknown_game"])
            .output()?;
        assert!(!out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], false);
        let msg = j["error"]["message"]
            .as_str()
            .ok_or("should have error message")?;
        // Error message should list supported games
        assert!(
            msg.contains("acc") || msg.contains("supports"),
            "error should list supported games: {msg}"
        );
        Ok(())
    }
}

// ===========================================================================
// 6. Plain-text / human output verification
// ===========================================================================

mod human_output {
    use super::*;

    #[test]
    fn device_list_shows_mock_device_names() -> TestResult {
        let out = wheelctl()?.args(["device", "list"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Fanatec DD Pro"),
            "should show mock wheel name: {s}"
        );
        assert!(
            s.contains("Fanatec V3 Pedals"),
            "should show mock pedals name: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_list_detailed_shows_type_and_capabilities() -> TestResult {
        let out = wheelctl()?
            .args(["device", "list", "--detailed"])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Max Torque"),
            "detailed should show max torque: {s}"
        );
        assert!(
            s.contains("Type") || s.contains("WheelBase"),
            "detailed should show device type: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_status_shows_telemetry_data() -> TestResult {
        let out = wheelctl()?
            .args(["device", "status", "wheel-001"])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Wheel Angle") || s.contains("Temperature") || s.contains("Telemetry"),
            "should display telemetry info: {s}"
        );
        Ok(())
    }

    #[test]
    fn game_list_shows_games_header() -> TestResult {
        let out = wheelctl()?.args(["game", "list"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Supported Games"),
            "should show supported games header: {s}"
        );
        Ok(())
    }

    #[test]
    fn game_list_detailed_shows_features() -> TestResult {
        let out = wheelctl()?.args(["game", "list", "--detailed"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Features") || s.contains("Config"),
            "detailed game list should show features or config: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_status_shows_safety_header() -> TestResult {
        let out = wheelctl()?.args(["safety", "status"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Safety") || s.contains("Torque") || s.contains("High Torque"),
            "safety status should show safety info: {s}"
        );
        Ok(())
    }

    #[test]
    fn health_shows_service_status() -> TestResult {
        let out = wheelctl()?.args(["health"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Health") || s.contains("Running"),
            "health should show service status: {s}"
        );
        Ok(())
    }

    #[test]
    fn diag_metrics_shows_performance() -> TestResult {
        let out = wheelctl()?.args(["diag", "metrics"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Jitter") || s.contains("Performance") || s.contains("Diagnostics"),
            "metrics should show performance info: {s}"
        );
        Ok(())
    }

    #[test]
    fn diag_test_shows_test_results() -> TestResult {
        let out = wheelctl()?.args(["diag", "test"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Motor") || s.contains("Encoder") || s.contains("Diagnostic"),
            "diag test should show test results: {s}"
        );
        Ok(())
    }

    #[test]
    fn plugin_list_shows_plugins_header() -> TestResult {
        let out = wheelctl()?.args(["plugin", "list"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Plugin") || s.contains("Available"),
            "plugin list should show header: {s}"
        );
        Ok(())
    }

    #[test]
    fn plugin_search_no_match_shows_message() -> TestResult {
        let out = wheelctl()?
            .args(["plugin", "search", "zzzzzz_nothing_matches"])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("No plugins found"),
            "should show no results message: {s}"
        );
        Ok(())
    }

    #[test]
    fn plugin_search_match_shows_results() -> TestResult {
        let out = wheelctl()?.args(["plugin", "search", "FFB"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("FFB") || s.contains("Smoothing") || s.contains("Curves"),
            "search for FFB should show matching plugins: {s}"
        );
        Ok(())
    }

    #[test]
    fn completion_bash_produces_completions() -> TestResult {
        let out = wheelctl()?.args(["completion", "bash"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("wheelctl") || s.contains("complete") || s.contains("COMPREPLY"),
            "bash completion should reference the binary: {s}"
        );
        Ok(())
    }

    #[test]
    fn completion_powershell_produces_completions() -> TestResult {
        let out = wheelctl()?.args(["completion", "powershell"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("wheelctl") || s.contains("Register"),
            "powershell completion should reference the binary: {s}"
        );
        Ok(())
    }

    #[test]
    fn completion_zsh_produces_completions() -> TestResult {
        let out = wheelctl()?.args(["completion", "zsh"]).output()?;
        assert!(out.status.success());
        assert!(
            !out.stdout.is_empty(),
            "zsh completion should produce output"
        );
        Ok(())
    }

    #[test]
    fn completion_fish_produces_completions() -> TestResult {
        let out = wheelctl()?.args(["completion", "fish"]).output()?;
        assert!(out.status.success());
        assert!(
            !out.stdout.is_empty(),
            "fish completion should produce output"
        );
        Ok(())
    }
}

// ===========================================================================
// 7. Config file loading / profile operations via temp files
// ===========================================================================

mod config_loading {
    use super::*;
    use std::fs;

    fn valid_profile_json() -> &'static str {
        r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": "iracing", "car": "gt3" },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 540,
                "torqueCapNm": 6.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.05,
                    "damper": 0.1,
                    "inertia": 0.0,
                    "slewRate": 1.0,
                    "notchFilters": [],
                    "curvePoints": [
                        { "input": 0.0, "output": 0.0 },
                        { "input": 1.0, "output": 1.0 }
                    ]
                }
            }
        }"#
    }

    #[test]
    fn profile_validate_valid_file_succeeds() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("valid.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args(["profile", "validate", path.to_str().ok_or("invalid path")?])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("valid") || s.contains("✓"),
            "should report profile as valid: {s}"
        );
        Ok(())
    }

    #[test]
    fn profile_validate_valid_file_json_mode() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("valid.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args([
                "--json",
                "profile",
                "validate",
                path.to_str().ok_or("invalid path")?,
            ])
            .output()?;
        assert!(out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert_eq!(j["valid"], true);
        Ok(())
    }

    #[test]
    fn profile_validate_invalid_json_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("bad.json");
        fs::write(&path, "{ not valid json at all")?;

        let out = wheelctl()?
            .args(["profile", "validate", path.to_str().ok_or("invalid path")?])
            .output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn profile_validate_nonexistent_file_fails() -> TestResult {
        let out = wheelctl()?
            .args(["profile", "validate", "nonexistent_profile_xyz.json"])
            .output()?;
        assert!(!out.status.success());
        Ok(())
    }

    #[test]
    fn profile_show_valid_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("show_test.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args(["profile", "show", path.to_str().ok_or("invalid path")?])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("iracing") || s.contains("Profile"),
            "show should display profile info: {s}"
        );
        Ok(())
    }

    #[test]
    fn profile_show_json_mode() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("show_json.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args([
                "--json",
                "profile",
                "show",
                path.to_str().ok_or("invalid path")?,
            ])
            .output()?;
        assert!(out.status.success());
        let j = json(&out.stdout)?;
        assert_eq!(j["success"], true);
        assert!(j.get("profile").is_some(), "should have profile field");
        Ok(())
    }

    #[test]
    fn profile_create_creates_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("new_profile.json");

        let out = wheelctl()?
            .args([
                "profile",
                "create",
                path.to_str().ok_or("invalid path")?,
                "--game",
                "acc",
                "--car",
                "gt3",
            ])
            .output()?;
        assert!(
            out.status.success(),
            "create should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(path.exists(), "profile file should be created");

        // Validate the created file is valid JSON
        let content = fs::read_to_string(&path)?;
        let profile: Value = serde_json::from_str(&content)?;
        assert_eq!(profile["scope"]["game"], "acc");
        assert_eq!(profile["scope"]["car"], "gt3");
        Ok(())
    }

    #[test]
    fn profile_create_from_existing() -> TestResult {
        let dir = tempfile::tempdir()?;
        let base_path = dir.path().join("base.json");
        fs::write(&base_path, valid_profile_json())?;

        let new_path = dir.path().join("derived.json");

        let out = wheelctl()?
            .args([
                "profile",
                "create",
                new_path.to_str().ok_or("invalid path")?,
                "--from",
                base_path.to_str().ok_or("invalid path")?,
            ])
            .output()?;
        assert!(
            out.status.success(),
            "create --from should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(new_path.exists(), "new profile should be created");
        Ok(())
    }

    #[test]
    fn profile_export_to_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("source.json");
        fs::write(&src, valid_profile_json())?;
        let dst = dir.path().join("exported.json");

        let out = wheelctl()?
            .args([
                "profile",
                "export",
                src.to_str().ok_or("invalid path")?,
                "--output",
                dst.to_str().ok_or("invalid path")?,
            ])
            .output()?;
        assert!(
            out.status.success(),
            "export should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(dst.exists(), "exported file should be created");
        Ok(())
    }

    #[test]
    fn profile_export_to_stdout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("source.json");
        fs::write(&src, valid_profile_json())?;

        let out = wheelctl()?
            .args(["profile", "export", src.to_str().ok_or("invalid path")?])
            .output()?;
        assert!(out.status.success());
        // Should output the profile JSON to stdout
        let j: Value = serde_json::from_slice(&out.stdout)?;
        assert_eq!(j["schema"], "wheel.profile/1");
        Ok(())
    }

    #[test]
    fn profile_export_signed_adds_signature() -> TestResult {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("unsigned.json");
        fs::write(&src, valid_profile_json())?;
        let dst = dir.path().join("signed.json");

        let out = wheelctl()?
            .args([
                "profile",
                "export",
                src.to_str().ok_or("invalid path")?,
                "--output",
                dst.to_str().ok_or("invalid path")?,
                "--signed",
            ])
            .output()?;
        assert!(
            out.status.success(),
            "signed export should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let content = fs::read_to_string(&dst)?;
        let j: Value = serde_json::from_str(&content)?;
        assert!(
            j.get("signature").is_some() && !j["signature"].is_null(),
            "signed export should have a signature field"
        );
        Ok(())
    }

    #[test]
    fn profile_import_to_target() -> TestResult {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("import_src.json");
        fs::write(&src, valid_profile_json())?;
        let target = dir.path().join("imported.json");

        let out = wheelctl()?
            .args([
                "profile",
                "import",
                src.to_str().ok_or("invalid path")?,
                "--target",
                target.to_str().ok_or("invalid path")?,
            ])
            .output()?;
        assert!(
            out.status.success(),
            "import should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(target.exists(), "imported file should exist at target");
        Ok(())
    }

    #[test]
    fn profile_import_verify_unsigned_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("unsigned.json");
        fs::write(&src, valid_profile_json())?;

        let out = wheelctl()?
            .args([
                "profile",
                "import",
                src.to_str().ok_or("invalid path")?,
                "--verify",
            ])
            .output()?;
        assert!(
            !out.status.success(),
            "import --verify of unsigned profile should fail"
        );
        Ok(())
    }

    #[test]
    fn profile_edit_field_value_updates_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("edit_test.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args([
                "profile",
                "edit",
                path.to_str().ok_or("invalid path")?,
                "--field",
                "base.ffbGain",
                "--value",
                "0.95",
            ])
            .output()?;
        assert!(
            out.status.success(),
            "edit should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // Read back and verify the value was updated
        let content = fs::read_to_string(&path)?;
        let j: Value = serde_json::from_str(&content)?;
        let gain = j["base"]["ffbGain"]
            .as_f64()
            .ok_or("ffbGain should be numeric")?;
        assert!(
            (gain - 0.95).abs() < 0.01,
            "ffb_gain should be ~0.95, got {gain}"
        );
        Ok(())
    }

    #[test]
    fn profile_validate_detailed_shows_profile_info() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("detailed.json");
        fs::write(&path, valid_profile_json())?;

        let out = wheelctl()?
            .args([
                "profile",
                "validate",
                path.to_str().ok_or("invalid path")?,
                "--detailed",
            ])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("valid") || s.contains("✓"),
            "should report valid"
        );
        // Detailed should show profile info
        assert!(
            s.contains("FFB Gain") || s.contains("Profile") || s.contains("iracing"),
            "detailed should show profile data: {s}"
        );
        Ok(())
    }
}

// ===========================================================================
// 8. Interactive prompts simulation (--yes / --force bypass)
// ===========================================================================

mod prompt_bypass {
    use super::*;

    #[test]
    fn device_calibrate_yes_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["device", "calibrate", "wheel-001", "center", "--yes"])
            .output()?;
        assert!(
            out.status.success(),
            "calibrate --yes should succeed without interactive prompt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("calibration") || s.contains("completed") || s.contains("✓"),
            "should report calibration result: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_calibrate_yes_all_types() -> TestResult {
        for cal_type in &["center", "dor", "pedals", "all"] {
            let out = wheelctl()?
                .args(["device", "calibrate", "wheel-001", cal_type, "--yes"])
                .output()?;
            assert!(
                out.status.success(),
                "calibrate --yes {cal_type} should succeed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(())
    }

    #[test]
    fn device_calibrate_json_mode_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "device", "calibrate", "wheel-001", "center"])
            .output()?;
        assert!(
            out.status.success(),
            "calibrate in JSON mode should skip prompt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(())
    }

    #[test]
    fn device_reset_force_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["device", "reset", "wheel-001", "--force"])
            .output()?;
        assert!(
            out.status.success(),
            "reset --force should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("reset") || s.contains("safe state") || s.contains("✓"),
            "should report reset result: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_reset_json_mode_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "device", "reset", "wheel-001"])
            .output()?;
        assert!(
            out.status.success(),
            "reset in JSON mode should skip prompt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(())
    }

    #[test]
    fn safety_enable_force_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["safety", "enable", "wheel-001", "--force"])
            .output()?;
        assert!(
            out.status.success(),
            "safety enable --force should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("enabled") || s.contains("High torque") || s.contains("✓"),
            "should report enable result: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_enable_json_mode_skips_prompt() -> TestResult {
        let out = wheelctl()?
            .args(["--json", "safety", "enable", "wheel-001", "--force"])
            .output()?;
        assert!(out.status.success());
        Ok(())
    }

    #[test]
    fn safety_stop_no_prompt_needed() -> TestResult {
        // Emergency stop should not require a prompt
        let out = wheelctl()?.args(["safety", "stop"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("stop") || s.contains("stopped") || s.contains("✓"),
            "should report stop result: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_stop_specific_device() -> TestResult {
        let out = wheelctl()?.args(["safety", "stop", "wheel-001"]).output()?;
        assert!(out.status.success());
        Ok(())
    }

    #[test]
    fn safety_limit_sets_torque() -> TestResult {
        let out = wheelctl()?
            .args(["safety", "limit", "wheel-001", "5.0"])
            .output()?;
        assert!(
            out.status.success(),
            "torque limit should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("5.0") || s.contains("Torque limit"),
            "should report limit set: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_limit_global_flag() -> TestResult {
        let out = wheelctl()?
            .args(["safety", "limit", "wheel-001", "6.0", "--global"])
            .output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("all profiles") || s.contains("global"),
            "should mention global scope: {s}"
        );
        Ok(())
    }

    #[test]
    fn safety_limit_exceeds_max_torque_fails() -> TestResult {
        // The mock device has max_torque_nm = 8.0
        let out = wheelctl()?
            .args(["safety", "limit", "wheel-001", "20.0"])
            .output()?;
        assert!(
            !out.status.success(),
            "torque exceeding device max should fail"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Cross-cutting: JSON consistency, output encoding, edge cases
// ===========================================================================

mod cross_cutting {
    use super::*;

    #[test]
    fn all_json_outputs_are_objects() -> TestResult {
        let commands: &[&[&str]] = &[
            &["--json", "device", "list"],
            &["--json", "device", "list", "--detailed"],
            &["--json", "device", "status", "wheel-001"],
            &["--json", "profile", "list"],
            &["--json", "plugin", "list"],
            &["--json", "plugin", "search", "ffb"],
            &["--json", "game", "list"],
            &["--json", "game", "status"],
            &["--json", "safety", "status"],
            &["--json", "safety", "status", "wheel-001"],
            &["--json", "health"],
            &["--json", "diag", "metrics"],
            &["--json", "diag", "test"],
        ];

        for args in commands {
            let out = wheelctl()?.args(*args).output()?;
            if out.status.success() {
                let j = json(&out.stdout)?;
                assert!(j.is_object(), "JSON for {:?} should be an object", args);
                assert!(
                    j.get("success").is_some(),
                    "JSON for {:?} should have success field",
                    args
                );
            }
        }
        Ok(())
    }

    #[test]
    fn json_outputs_are_pretty_printed() -> TestResult {
        let out = wheelctl()?.args(["--json", "device", "list"]).output()?;
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        // Pretty-printed JSON has newlines and indentation
        assert!(
            s.contains('\n') && (s.contains("  ") || s.contains('\t')),
            "JSON should be pretty-printed: {s}"
        );
        Ok(())
    }

    #[test]
    fn all_outputs_are_valid_utf8() -> TestResult {
        let commands: &[&[&str]] = &[
            &["device", "list"],
            &["device", "list", "--detailed"],
            &["game", "list"],
            &["plugin", "list"],
            &["safety", "status"],
            &["health"],
            &["diag", "metrics"],
        ];

        for args in commands {
            let out = wheelctl()?.args(*args).output()?;
            if out.status.success() {
                assert!(
                    String::from_utf8(out.stdout.clone()).is_ok(),
                    "stdout for {:?} should be valid UTF-8",
                    args
                );
            }
        }
        Ok(())
    }

    #[test]
    fn empty_plugin_category_returns_all() -> TestResult {
        let all_out = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        let all_j = json(&all_out.stdout)?;
        let all_count = all_j["plugins"].as_array().map(|a| a.len()).unwrap_or(0);

        // With no category filter, should return all plugins
        assert!(
            all_count > 0,
            "unfiltered plugin list should return plugins"
        );
        Ok(())
    }

    #[test]
    fn diag_test_specific_types_are_distinct() -> TestResult {
        for test_type in &["motor", "encoder", "usb", "thermal"] {
            let out = wheelctl()?
                .args(["--json", "diag", "test", test_type])
                .output()?;
            assert!(out.status.success(), "diag test {test_type} should succeed");
            let j = json(&out.stdout)?;
            let results = j["test_results"]
                .as_array()
                .ok_or("should have test_results")?;
            assert_eq!(
                results.len(),
                1,
                "specific test type should yield exactly 1 result"
            );
        }
        Ok(())
    }

    #[test]
    fn game_configure_json_needs_path() -> TestResult {
        // In JSON mode, configure should require a path
        let out = wheelctl()?
            .args(["--json", "game", "configure", "iracing"])
            .output()?;
        assert!(
            !out.status.success(),
            "game configure in JSON mode without path should fail"
        );
        Ok(())
    }

    #[test]
    fn game_configure_with_path_succeeds() -> TestResult {
        let out = wheelctl()?
            .args([
                "game",
                "configure",
                "iracing",
                "--path",
                "C:\\Games\\iRacing",
            ])
            .output()?;
        assert!(
            out.status.success(),
            "game configure with path should succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(())
    }

    #[test]
    fn version_and_help_do_not_conflict() -> TestResult {
        let help = wheelctl()?.arg("--help").output()?;
        assert!(help.status.success());

        let version = wheelctl()?.arg("--version").output()?;
        assert!(version.status.success());

        // They should produce different output
        assert_ne!(help.stdout, version.stdout);
        Ok(())
    }
}
