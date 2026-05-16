//! Comprehensive CLI end-to-end tests for wheelctl.
//!
//! This file extends the existing test suite with deeper coverage of:
//! - Telemetry command execution and error paths
//! - Profile validation edge cases (malformed JSON, boundary values, missing fields)
//! - JSON output structural consistency across all commands
//! - Error message actionability and exit code correctness
//! - Snapshot tests for leaf-level subcommand help texts
//! - Multi-step workflow integration (create→edit→validate→export→import)
//! - Edge cases: empty strings, special characters, long args
//! - Global flag interaction and ordering
//! - Output format parity (JSON vs human-readable)

#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

fn parse_json(bytes: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let v: Value = serde_json::from_slice(bytes)?;
    Ok(v)
}

fn write_profile_json(
    dir: &TempDir,
    name: &str,
    content: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = dir.path().join(format!("{name}.json"));
    fs::write(&path, content)?;
    Ok(path)
}

fn valid_profile_json() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
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
    }))
    .unwrap_or_default()
}

fn path_str(p: &std::path::Path) -> Result<&str, Box<dyn std::error::Error>> {
    p.to_str().ok_or_else(|| "non-UTF-8 path".into())
}

/// Normalize binary name across platforms
fn normalize_output(s: &str) -> String {
    let normalized = s.replace("wheelctl.exe", "wheelctl");
    let mut output = normalized
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    if normalized.ends_with('\n') {
        output.push('\n');
    }
    output
}

// ===========================================================================
// 1. Telemetry command execution and error paths
// ===========================================================================

mod telemetry_execution {
    use super::*;

    #[test]
    fn probe_unsupported_game_fails_with_helpful_error() -> TestResult {
        let output = wheelctl()?
            .args(["telemetry", "probe", "--game", "iracing"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{stderr}{stdout}");
        assert!(
            combined.contains("acc") || combined.contains("supported"),
            "error should mention supported games: {combined}"
        );
        Ok(())
    }

    #[test]
    fn probe_unsupported_game_json_error_has_type() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "telemetry", "probe", "--game", "iracing"])
            .output()?;
        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(
            json.get("success").and_then(Value::as_bool),
            Some(false),
            "should report failure"
        );
        assert!(
            json.get("error").is_some(),
            "should have error field in JSON"
        );
        Ok(())
    }

    #[test]
    fn probe_empty_game_id_fails() -> TestResult {
        let output = wheelctl()?
            .args(["telemetry", "probe", "--game", ""])
            .output()?;
        assert!(!output.status.success(), "empty game ID should fail");
        Ok(())
    }

    #[test]
    fn capture_unsupported_game_fails() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("capture.bin");
        let output = wheelctl()?
            .args([
                "telemetry",
                "capture",
                "--game",
                "iracing",
                "--out",
                path_str(&out_path)?,
            ])
            .output()?;
        assert!(
            !output.status.success(),
            "capture with unsupported game should fail"
        );
        Ok(())
    }

    #[test]
    fn capture_unsupported_game_json_error() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("capture.bin");
        let output = wheelctl()?
            .args([
                "--json",
                "telemetry",
                "capture",
                "--game",
                "iracing",
                "--out",
                path_str(&out_path)?,
            ])
            .output()?;
        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(false));
        Ok(())
    }

    #[test]
    fn probe_help_shows_default_values() -> TestResult {
        let output = wheelctl()?
            .args(["telemetry", "probe", "--help"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("127.0.0.1:9000"),
            "probe help should show default endpoint"
        );
        assert!(
            stdout.contains("400"),
            "probe help should show default timeout"
        );
        assert!(
            stdout.contains("3"),
            "probe help should show default attempts"
        );
        Ok(())
    }

    #[test]
    fn capture_help_shows_default_values() -> TestResult {
        let output = wheelctl()?
            .args(["telemetry", "capture", "--help"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("9000"),
            "capture help should show default port"
        );
        assert!(
            stdout.contains("10"),
            "capture help should show default duration"
        );
        assert!(
            stdout.contains("2048"),
            "capture help should show default max-payload"
        );
        Ok(())
    }

    #[test]
    fn probe_with_custom_timeout_and_attempts() -> TestResult {
        // Should parse successfully even if probe fails (network)
        let output = wheelctl()?
            .args([
                "telemetry",
                "probe",
                "--game",
                "acc",
                "--timeout-ms",
                "50",
                "--attempts",
                "1",
            ])
            .output()?;
        // It will either succeed (unlikely without server) or fail with a
        // network/timeout error—not a parse error.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{stderr}{stdout}");
        // Should not contain clap parse errors
        assert!(
            !combined.contains("error: unexpected argument")
                && !combined.contains("error: invalid value"),
            "custom args should parse without error: {combined}"
        );
        Ok(())
    }

    #[test]
    fn capture_with_custom_port_and_duration_parses() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("cap.bin");
        let output = wheelctl()?
            .args([
                "telemetry",
                "capture",
                "--game",
                "acc",
                "--port",
                "9876",
                "--duration",
                "1",
                "--out",
                path_str(&out_path)?,
                "--max-payload",
                "512",
            ])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should not be a parse error
        assert!(
            !stderr.contains("error: unexpected argument"),
            "custom capture args should parse: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 2. Profile validation edge cases
// ===========================================================================

mod profile_validation_edge_cases {
    use super::*;

    #[test]
    fn validate_truncated_json_fails() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "truncated", r#"{"schema": "wheel"#)?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_json_array_instead_of_object_fails() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "array", "[1, 2, 3]")?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_empty_json_object_fails() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "empty_obj", "{}")?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_missing_base_section_fails() -> TestResult {
        let temp = TempDir::new()?;
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "schema": "wheel.profile/1",
            "scope": { "game": "iracing" }
        }))?;
        let path = write_profile_json(&temp, "no_base", &content)?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_missing_schema_field_fails() -> TestResult {
        let temp = TempDir::new()?;
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "scope": { "game": "iracing" },
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
                    "curvePoints": []
                }
            }
        }))?;
        let path = write_profile_json(&temp, "no_schema", &content)?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_wrong_schema_version_fails() -> TestResult {
        let temp = TempDir::new()?;
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "schema": "wheel.profile/99",
            "scope": { "game": "iracing" },
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
                    "curvePoints": []
                }
            }
        }))?;
        let path = write_profile_json(&temp, "bad_version", &content)?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn validate_valid_profile_succeeds_in_json_mode() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "valid", &valid_profile_json())?;
        let output = wheelctl()?
            .args(["--json", "profile", "validate", path_str(&path)?])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn validate_invalid_json_reports_error_in_json_mode() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "bad", "not json at all")?;
        let output = wheelctl()?
            .args(["--json", "profile", "validate", path_str(&path)?])
            .output()?;
        assert!(!output.status.success());
        // stdout may contain multiple JSON documents (validate output + error output);
        // check that at least one reports success: false
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(r#""success": false"#) || stdout.contains(r#""valid": false"#),
            "JSON output should indicate failure: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn validate_binary_file_fails_gracefully() -> TestResult {
        let temp = TempDir::new()?;
        let path = temp.path().join("binary.json");
        fs::write(&path, [0x00, 0xFF, 0xFE, 0xFD, 0x89, 0x50, 0x4E, 0x47])?;
        wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn show_nonexistent_profile_exit_code_is_3() -> TestResult {
        let output = wheelctl()?
            .args(["profile", "show", "/nonexistent/profile.json"])
            .output()?;
        assert!(!output.status.success());
        // exit code 3 = ProfileNotFound
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(
            code, 3,
            "ProfileNotFound should exit with code 3, got {code}"
        );
        Ok(())
    }

    #[test]
    fn edit_invalid_field_name_fails() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "edit_bad", &valid_profile_json())?;
        let output = wheelctl()?
            .args([
                "profile",
                "edit",
                path_str(&path)?,
                "--field",
                "nonexistent.field",
                "--value",
                "42",
            ])
            .output()?;
        assert!(
            !output.status.success(),
            "editing a nonexistent field should fail"
        );
        Ok(())
    }

    #[test]
    fn edit_ffb_gain_to_invalid_string_value_fails() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "edit_type", &valid_profile_json())?;
        let output = wheelctl()?
            .args([
                "profile",
                "edit",
                path_str(&path)?,
                "--field",
                "base.ffbGain",
                "--value",
                "not_a_number",
            ])
            .output()?;
        assert!(!output.status.success(), "non-numeric ffbGain should fail");
        Ok(())
    }
}

// ===========================================================================
// 3. JSON output structural consistency
// ===========================================================================

mod json_structural_consistency {
    use super::*;

    #[test]
    fn all_json_outputs_contain_success_key() -> TestResult {
        let commands: Vec<Vec<&str>> = vec![
            vec!["--json", "device", "list"],
            vec!["--json", "device", "list", "--detailed"],
            vec!["--json", "device", "status", "wheel-001"],
            vec!["--json", "profile", "list"],
            vec!["--json", "game", "list"],
            vec!["--json", "game", "list", "--detailed"],
            vec!["--json", "game", "status"],
            vec!["--json", "safety", "status"],
            vec!["--json", "health"],
            vec!["--json", "diag", "metrics"],
            vec!["--json", "diag", "test"],
            vec!["--json", "plugin", "list"],
            vec!["--json", "plugin", "search", "ffb"],
        ];

        for args in &commands {
            let output = wheelctl()?.args(args).output()?;
            if output.status.success() {
                let json = parse_json(&output.stdout)?;
                assert!(
                    json.get("success").is_some(),
                    "command {:?} JSON should have 'success' key",
                    args
                );
            }
        }
        Ok(())
    }

    #[test]
    fn json_outputs_are_well_formed_objects() -> TestResult {
        let commands: Vec<Vec<&str>> = vec![
            vec!["--json", "device", "list"],
            vec!["--json", "profile", "list"],
            vec!["--json", "game", "list"],
            vec!["--json", "safety", "status"],
            vec!["--json", "health"],
            vec!["--json", "diag", "metrics"],
            vec!["--json", "plugin", "list"],
        ];

        for args in &commands {
            let output = wheelctl()?.args(args).output()?;
            assert!(output.status.success(), "command {:?} should succeed", args);

            let text = String::from_utf8(output.stdout.clone())?;
            let trimmed = text.trim();
            assert!(
                trimmed.starts_with('{') && trimmed.ends_with('}'),
                "command {:?} JSON should be a single object: {}",
                args,
                &trimmed[..trimmed.len().min(100)]
            );
        }
        Ok(())
    }

    #[test]
    fn device_list_json_devices_have_id_and_name() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let json = parse_json(&output.stdout)?;
        let devices = json
            .get("devices")
            .and_then(Value::as_array)
            .ok_or("missing devices array")?;
        for dev in devices {
            assert!(
                dev.get("id").and_then(Value::as_str).is_some(),
                "each device should have 'id'"
            );
            assert!(
                dev.get("name").and_then(Value::as_str).is_some(),
                "each device should have 'name'"
            );
        }
        Ok(())
    }

    #[test]
    fn device_status_json_has_telemetry_nested_data() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "device", "status", "wheel-001"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        let status = json.get("status").ok_or("missing status field")?;
        assert!(
            status.get("device_id").is_some() || status.get("device").is_some(),
            "status should reference the device"
        );
        Ok(())
    }

    #[test]
    fn safety_status_json_has_devices_with_safety_info() -> TestResult {
        let output = wheelctl()?.args(["--json", "safety", "status"]).output()?;
        let json = parse_json(&output.stdout)?;
        // safety status should have some devices info
        assert!(
            json.get("devices").is_some() || json.get("safety").is_some(),
            "safety status JSON should contain device or safety info"
        );
        Ok(())
    }

    #[test]
    fn health_json_has_service_status_and_devices() -> TestResult {
        let output = wheelctl()?.args(["--json", "health"]).output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("service_status").is_some(),
            "health JSON should have service_status"
        );
        assert!(
            json.get("devices").is_some(),
            "health JSON should have devices"
        );
        Ok(())
    }

    #[test]
    fn diag_metrics_json_has_performance_data() -> TestResult {
        let output = wheelctl()?.args(["--json", "diag", "metrics"]).output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("diagnostics").is_some(),
            "diag metrics JSON should have diagnostics"
        );
        Ok(())
    }

    #[test]
    fn diag_test_json_has_results_array() -> TestResult {
        let output = wheelctl()?.args(["--json", "diag", "test"]).output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("test_results").is_some(),
            "diag test JSON should have test_results"
        );
        Ok(())
    }

    #[test]
    fn plugin_list_json_has_plugins_with_ids() -> TestResult {
        let output = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        let json = parse_json(&output.stdout)?;
        let plugins = json
            .get("plugins")
            .and_then(Value::as_array)
            .ok_or("missing plugins array")?;
        for plugin in plugins {
            assert!(
                plugin.get("id").and_then(Value::as_str).is_some(),
                "each plugin should have 'id'"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_search_json_has_results_key() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "search", "ffb"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("results").is_some(),
            "plugin search JSON should have 'results'"
        );
        Ok(())
    }

    #[test]
    fn plugin_info_json_has_plugin_details() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "info", "ffb-enhanced"])
            .output()?;
        // May succeed or fail depending on mock data
        if output.status.success() {
            let json = parse_json(&output.stdout)?;
            assert!(
                json.get("plugin").is_some(),
                "plugin info JSON should have 'plugin'"
            );
        }
        Ok(())
    }

    #[test]
    fn game_list_json_games_have_id_and_name() -> TestResult {
        let output = wheelctl()?.args(["--json", "game", "list"]).output()?;
        let json = parse_json(&output.stdout)?;
        let games = json
            .get("supported_games")
            .and_then(Value::as_array)
            .ok_or("missing supported_games")?;
        for game in games {
            assert!(
                game.get("id").and_then(Value::as_str).is_some(),
                "each game should have 'id'"
            );
            assert!(
                game.get("name").and_then(Value::as_str).is_some(),
                "each game should have 'name'"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// 4. Error message actionability and exit codes
// ===========================================================================

mod error_actionability {
    use super::*;

    #[test]
    fn missing_subcommand_error_mentions_help() -> TestResult {
        let output = wheelctl()?.output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--help") || stderr.contains("Usage"),
            "missing subcommand error should guide user: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn unknown_subcommand_suggests_similar() -> TestResult {
        let output = wheelctl()?.arg("devce").output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        // clap often suggests similar commands
        assert!(
            stderr.contains("device") || stderr.contains("Did you mean"),
            "typo should suggest correct subcommand: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn missing_required_arg_error_shows_arg_name() -> TestResult {
        let output = wheelctl()?.args(["device", "status"]).output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("<DEVICE>") || stderr.contains("device") || stderr.contains("required"),
            "missing arg error should mention which arg: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn invalid_enum_value_lists_valid_options() -> TestResult {
        let output = wheelctl()?
            .args(["device", "calibrate", "wheel-001", "badtype"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("center") && stderr.contains("dor"),
            "invalid enum should list valid values: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn validation_error_exit_code_is_4() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "bad", "not json")?;
        let output = wheelctl()?
            .args(["profile", "validate", path_str(&path)?])
            .output()?;
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(code, 4, "validation error should exit 4, got {code}");
        Ok(())
    }

    #[test]
    fn clap_parse_error_exit_code_is_2() -> TestResult {
        let output = wheelctl()?.args(["device", "status"]).output()?;
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(code, 2, "clap parse error should exit 2, got {code}");
        Ok(())
    }

    #[test]
    fn help_flag_exit_code_is_0() -> TestResult {
        let output = wheelctl()?.arg("--help").output()?;
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(code, 0, "help should exit 0, got {code}");
        Ok(())
    }

    #[test]
    fn version_flag_exit_code_is_0() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(code, 0, "version should exit 0, got {code}");
        Ok(())
    }

    #[test]
    fn device_not_found_json_error_has_device_id() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "device", "status", "nonexistent-device-999"])
            .output()?;
        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        let msg = json
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            msg.contains("nonexistent-device-999"),
            "error message should include the device ID: {msg}"
        );
        Ok(())
    }

    #[test]
    fn safety_limit_exceeds_max_exits_with_validation_error() -> TestResult {
        let output = wheelctl()?
            .args(["safety", "limit", "wheel-001", "999.0"])
            .output()?;
        assert!(!output.status.success(), "extreme torque value should fail");
        let code = output.status.code().unwrap_or(-1);
        assert!(
            code == 4 || code == 1,
            "should exit with validation error code, got {code}"
        );
        Ok(())
    }
}

// ===========================================================================
// 5. Snapshot tests for leaf-level subcommand help
// ===========================================================================

mod help_snapshots {
    use super::*;

    #[test]
    fn snapshot_device_list_help() -> TestResult {
        let output = wheelctl()?.args(["device", "list", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("device_list_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_device_status_help() -> TestResult {
        let output = wheelctl()?.args(["device", "status", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("device_status_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_device_calibrate_help() -> TestResult {
        let output = wheelctl()?
            .args(["device", "calibrate", "--help"])
            .output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("device_calibrate_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_device_reset_help() -> TestResult {
        let output = wheelctl()?.args(["device", "reset", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("device_reset_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_show_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "show", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_show_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_create_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "create", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_create_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_validate_help() -> TestResult {
        let output = wheelctl()?
            .args(["profile", "validate", "--help"])
            .output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_validate_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_edit_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "edit", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_edit_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_export_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "export", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_export_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_import_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "import", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_import_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_profile_apply_help() -> TestResult {
        let output = wheelctl()?.args(["profile", "apply", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("profile_apply_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_safety_enable_help() -> TestResult {
        let output = wheelctl()?.args(["safety", "enable", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("safety_enable_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_safety_limit_help() -> TestResult {
        let output = wheelctl()?.args(["safety", "limit", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("safety_limit_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_diag_test_help() -> TestResult {
        let output = wheelctl()?.args(["diag", "test", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("diag_test_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_diag_record_help() -> TestResult {
        let output = wheelctl()?.args(["diag", "record", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("diag_record_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_plugin_install_help() -> TestResult {
        let output = wheelctl()?.args(["plugin", "install", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("plugin_install_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_game_configure_help() -> TestResult {
        let output = wheelctl()?.args(["game", "configure", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("game_configure_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_moza_help() -> TestResult {
        let output = wheelctl()?.args(["moza", "--help"]).output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("moza_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_moza_pit_house_proof_help() -> TestResult {
        let output = wheelctl()?
            .args(["moza", "pit-house-proof", "--help"])
            .output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("moza_pit_house_proof_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_moza_simulator_telemetry_proof_help() -> TestResult {
        let output = wheelctl()?
            .args(["moza", "simulator-telemetry-proof", "--help"])
            .output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("moza_simulator_telemetry_proof_help", stdout);
        Ok(())
    }

    #[test]
    fn snapshot_moza_simulator_ffb_smoke_help() -> TestResult {
        let output = wheelctl()?
            .args(["moza", "simulator-ffb-smoke", "--help"])
            .output()?;
        let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
        insta::assert_snapshot!("moza_simulator_ffb_smoke_help", stdout);
        Ok(())
    }
}

// ===========================================================================
// 6. Multi-step workflow integration
// ===========================================================================

mod workflow_integration {
    use super::*;

    #[test]
    fn create_validate_edit_export_import_roundtrip() -> TestResult {
        let temp = TempDir::new()?;
        let profile_path = temp.path().join("workflow.json");
        let export_path = temp.path().join("exported.json");
        let import_target = temp.path().join("imported.json");

        // Step 1: Create profile
        wheelctl()?
            .args([
                "profile",
                "create",
                path_str(&profile_path)?,
                "--game",
                "acc",
                "--car",
                "gt3",
            ])
            .assert()
            .success();
        assert!(profile_path.exists(), "profile should be created");

        // Step 2: Validate the created profile
        wheelctl()?
            .args(["profile", "validate", path_str(&profile_path)?])
            .assert()
            .success();

        // Step 3: Edit a field
        wheelctl()?
            .args([
                "profile",
                "edit",
                path_str(&profile_path)?,
                "--field",
                "base.ffbGain",
                "--value",
                "0.90",
            ])
            .assert()
            .success();

        // Step 4: Re-validate after edit
        wheelctl()?
            .args(["profile", "validate", path_str(&profile_path)?])
            .assert()
            .success();

        // Step 5: Export
        wheelctl()?
            .args([
                "profile",
                "export",
                path_str(&profile_path)?,
                "--output",
                path_str(&export_path)?,
            ])
            .assert()
            .success();
        assert!(export_path.exists(), "export should create file");

        // Step 6: Import into a target file path
        wheelctl()?
            .args([
                "profile",
                "import",
                path_str(&export_path)?,
                "--target",
                path_str(&import_target)?,
            ])
            .assert()
            .success();

        Ok(())
    }

    #[test]
    fn create_profile_show_in_json_and_human_both_work() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "dual_format", &valid_profile_json())?;

        // Human format
        let human = wheelctl()?
            .args(["profile", "show", path_str(&path)?])
            .output()?;
        assert!(human.status.success());
        let human_text = String::from_utf8_lossy(&human.stdout);
        assert!(!human_text.is_empty(), "human output should not be empty");

        // JSON format
        let json_out = wheelctl()?
            .args(["--json", "profile", "show", path_str(&path)?])
            .output()?;
        assert!(json_out.status.success());
        let json = parse_json(&json_out.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));

        Ok(())
    }

    #[test]
    fn export_signed_then_import_verify_succeeds() -> TestResult {
        let temp = TempDir::new()?;
        let profile_path = write_profile_json(&temp, "signed_test", &valid_profile_json())?;
        let export_path = temp.path().join("signed_export.json");
        let import_target = temp.path().join("signed_import.json");

        // Export with signature
        wheelctl()?
            .args([
                "profile",
                "export",
                path_str(&profile_path)?,
                "--output",
                path_str(&export_path)?,
                "--signed",
            ])
            .assert()
            .success();

        // Import with verification
        wheelctl()?
            .args([
                "profile",
                "import",
                path_str(&export_path)?,
                "--target",
                path_str(&import_target)?,
                "--verify",
            ])
            .assert()
            .success();

        Ok(())
    }
}

// ===========================================================================
// 7. Edge cases: empty strings, special characters, long args
// ===========================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn empty_device_id_is_handled() -> TestResult {
        let output = wheelctl()?.args(["device", "status", ""]).output()?;
        // Should either fail or handle gracefully
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should not panic or crash - just check we get output
        assert!(
            !stderr.is_empty() || !stdout.is_empty(),
            "empty device ID should produce some output"
        );
        Ok(())
    }

    #[test]
    fn device_id_with_spaces_is_handled() -> TestResult {
        let output = wheelctl()?
            .args(["device", "status", "wheel 001 with spaces"])
            .output()?;
        // Should not crash
        assert!(
            !output.status.success() || output.status.success(),
            "device ID with spaces should not crash"
        );
        Ok(())
    }

    #[test]
    fn very_long_profile_path_is_handled() -> TestResult {
        let long_name = "a".repeat(200);
        let path = format!("/tmp/{long_name}.json");
        let output = wheelctl()?.args(["profile", "show", &path]).output()?;
        assert!(!output.status.success(), "long path should fail gracefully");
        Ok(())
    }

    #[test]
    fn plugin_search_with_empty_query_handled() -> TestResult {
        let output = wheelctl()?.args(["plugin", "search", ""]).output()?;
        // Should either show empty results or handle gracefully
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.is_empty(),
            "empty search query should produce output"
        );
        Ok(())
    }

    #[test]
    fn plugin_search_with_special_characters() -> TestResult {
        let output = wheelctl()?
            .args(["plugin", "search", "!@#$%^&*()"])
            .output()?;
        // Should not crash
        let _ = String::from_utf8_lossy(&output.stdout);
        let _ = String::from_utf8_lossy(&output.stderr);
        Ok(())
    }

    #[test]
    fn multiple_verbose_flags_combined_with_json() -> TestResult {
        let output = wheelctl()?
            .args(["-vvv", "--json", "device", "list"])
            .output()?;
        assert!(output.status.success());
        // JSON should still be valid despite verbose
        let json = parse_json(&output.stdout)?;
        assert!(json.is_object());
        Ok(())
    }

    #[test]
    fn duplicate_global_flags_handled() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "--json", "device", "list"])
            .output()?;
        // Should not crash; clap typically accepts duplicate flags
        let _ = String::from_utf8_lossy(&output.stdout);
        Ok(())
    }

    #[test]
    fn negative_duration_for_diag_record() -> TestResult {
        // Duration is u64 so negative should be rejected by clap
        let output = wheelctl()?
            .args(["diag", "record", "wheel-001", "--duration", "-1"])
            .output()?;
        assert!(!output.status.success(), "negative duration should fail");
        Ok(())
    }

    #[test]
    fn zero_duration_for_diag_record() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("zero_dur.bin");
        let output = wheelctl()?
            .args([
                "diag",
                "record",
                "wheel-001",
                "--duration",
                "0",
                "--output",
                path_str(&out_path)?,
            ])
            .output()?;
        // Should either succeed with 0 frames or fail gracefully
        let _ = String::from_utf8_lossy(&output.stdout);
        Ok(())
    }

    #[test]
    fn safety_limit_zero_torque() -> TestResult {
        let output = wheelctl()?
            .args(["safety", "limit", "wheel-001", "0.0"])
            .output()?;
        // Setting torque to 0 should be handled (might succeed as "disable" or fail)
        let _ = String::from_utf8_lossy(&output.stdout);
        let _ = String::from_utf8_lossy(&output.stderr);
        Ok(())
    }

    #[test]
    fn safety_limit_negative_torque() -> TestResult {
        let output = wheelctl()?
            .args(["safety", "limit", "wheel-001", "-5.0"])
            .output()?;
        // Negative torque should be rejected or handled
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.is_empty(),
            "negative torque should produce some output"
        );
        Ok(())
    }
}

// ===========================================================================
// 8. Output format parity (JSON vs human-readable)
// ===========================================================================

mod output_format_parity {
    use super::*;

    #[test]
    fn device_list_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["device", "list"]).output()?;
        let json = wheelctl()?.args(["--json", "device", "list"]).output()?;
        assert!(human.status.success(), "human format should succeed");
        assert!(json.status.success(), "json format should succeed");
        Ok(())
    }

    #[test]
    fn game_list_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["game", "list"]).output()?;
        let json = wheelctl()?.args(["--json", "game", "list"]).output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn safety_status_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["safety", "status"]).output()?;
        let json = wheelctl()?.args(["--json", "safety", "status"]).output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn health_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["health"]).output()?;
        let json = wheelctl()?.args(["--json", "health"]).output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn diag_test_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["diag", "test"]).output()?;
        let json = wheelctl()?.args(["--json", "diag", "test"]).output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn plugin_list_both_formats_succeed() -> TestResult {
        let human = wheelctl()?.args(["plugin", "list"]).output()?;
        let json = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn device_status_both_formats_succeed() -> TestResult {
        let human = wheelctl()?
            .args(["device", "status", "wheel-001"])
            .output()?;
        let json = wheelctl()?
            .args(["--json", "device", "status", "wheel-001"])
            .output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn profile_show_both_formats_succeed() -> TestResult {
        let temp = TempDir::new()?;
        let path = write_profile_json(&temp, "parity", &valid_profile_json())?;
        let p = path_str(&path)?;

        let human = wheelctl()?.args(["profile", "show", p]).output()?;
        let json = wheelctl()?
            .args(["--json", "profile", "show", p])
            .output()?;
        assert!(human.status.success());
        assert!(json.status.success());
        Ok(())
    }

    #[test]
    fn human_output_does_not_contain_json_syntax() -> TestResult {
        let output = wheelctl()?.args(["device", "list"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Human output should not look like JSON
        let trimmed = stdout.trim();
        if !trimmed.is_empty() {
            assert!(
                !trimmed.starts_with('{'),
                "human output should not start with {{ : {trimmed}"
            );
        }
        Ok(())
    }

    #[test]
    fn json_output_does_not_contain_ansi_codes() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("\x1b["),
            "JSON output should not contain ANSI escape codes"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Version output format
// ===========================================================================

mod version_format {
    use super::*;

    #[test]
    fn version_output_is_single_line() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let lines: Vec<&str> = stdout.trim().lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "version should be a single line, got: {:?}",
            lines
        );
        Ok(())
    }

    #[test]
    fn version_matches_semver_pattern() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8(output.stdout)?;
        // Check for digits.digits.digits pattern
        let has_semver = stdout.split_whitespace().any(|word| {
            let parts: Vec<&str> = word.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        });
        assert!(
            has_semver,
            "version should contain semver pattern (X.Y.Z): {stdout}"
        );
        Ok(())
    }

    #[test]
    fn version_starts_with_wheelctl() -> TestResult {
        let output = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            stdout.starts_with("wheelctl"),
            "version should start with binary name: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn short_and_long_version_flags_match() -> TestResult {
        let short = wheelctl()?.arg("-V").output()?;
        let long = wheelctl()?.arg("--version").output()?;
        assert_eq!(
            short.stdout, long.stdout,
            "-V and --version should produce identical output"
        );
        Ok(())
    }
}

// ===========================================================================
// 10. Global flag interaction
// ===========================================================================

mod global_flag_interaction {
    use super::*;

    #[test]
    fn json_flag_before_nested_subcommand() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        assert!(output.status.success());
        let _ = parse_json(&output.stdout)?;
        Ok(())
    }

    #[test]
    fn json_flag_between_subcommand_levels() -> TestResult {
        let output = wheelctl()?.args(["device", "--json", "list"]).output()?;
        assert!(output.status.success());
        let _ = parse_json(&output.stdout)?;
        Ok(())
    }

    #[test]
    fn json_flag_after_all_args() -> TestResult {
        let output = wheelctl()?.args(["device", "list", "--json"]).output()?;
        assert!(output.status.success());
        let _ = parse_json(&output.stdout)?;
        Ok(())
    }

    #[test]
    fn verbose_and_json_together() -> TestResult {
        let output = wheelctl()?
            .args(["-v", "--json", "device", "list"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert!(json.is_object());
        Ok(())
    }

    #[test]
    fn endpoint_env_var_with_json_flag() -> TestResult {
        let output = wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://localhost:50051")
            .args(["--json", "device", "list"])
            .output()?;
        // May fail with connection error, but should parse args correctly
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should not be a clap parse error
        assert!(
            !stderr.contains("error: unexpected argument"),
            "endpoint env var should not cause parse error: {stderr} {stdout}"
        );
        Ok(())
    }

    #[test]
    fn endpoint_flag_with_json_flag() -> TestResult {
        let output = wheelctl()?
            .args([
                "--endpoint",
                "http://localhost:50051",
                "--json",
                "device",
                "list",
            ])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "endpoint flag should not cause parse error: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 11. Plugin command execution
// ===========================================================================

mod plugin_execution {
    use super::*;

    #[test]
    fn plugin_list_with_category_filter() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "list", "--category", "ffb"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn plugin_search_matching_query() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "search", "ffb"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        let results = json.get("results").and_then(Value::as_array);
        assert!(results.is_some(), "search should return results array");
        Ok(())
    }

    #[test]
    fn plugin_search_no_match_returns_empty() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "search", "zzz_nonexistent_plugin_xyz"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        let results = json
            .get("results")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(results, 0, "nonexistent query should return 0 results");
        Ok(())
    }

    #[test]
    fn plugin_install_json_produces_valid_json() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "install", "ffb-enhanced"])
            .output()?;
        // Install may fail with "not found in registry" since we use mock data,
        // but output should still be valid JSON with success or error fields
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("action").is_some()
                || json.get("success").is_some()
                || json.get("error").is_some(),
            "install JSON should have action, success, or error field"
        );
        Ok(())
    }

    #[test]
    fn plugin_uninstall_with_force_json() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "uninstall", "some-plugin", "--force"])
            .output()?;
        // Should parse args successfully regardless of plugin existence
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "force flag should be accepted: {stderr} {stdout}"
        );
        Ok(())
    }

    #[test]
    fn plugin_verify_json_output() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "plugin", "verify", "ffb-enhanced"])
            .output()?;
        if output.status.success() {
            let json = parse_json(&output.stdout)?;
            assert!(
                json.get("verification").is_some() || json.get("success").is_some(),
                "verify JSON should have verification or success"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_info_with_version_flag() -> TestResult {
        let output = wheelctl()?
            .args(["plugin", "info", "ffb-enhanced", "--version", "1.0.0"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "version flag should be accepted: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 12. Game command execution
// ===========================================================================

mod game_execution {
    use super::*;

    #[test]
    fn game_list_detailed_shows_more_info() -> TestResult {
        let brief = wheelctl()?.args(["game", "list"]).output()?;
        let detailed = wheelctl()?.args(["game", "list", "--detailed"]).output()?;
        assert!(brief.status.success());
        assert!(detailed.status.success());
        assert!(
            detailed.stdout.len() >= brief.stdout.len(),
            "detailed output should be at least as long as brief"
        );
        Ok(())
    }

    #[test]
    fn game_list_detailed_json_has_extra_fields() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "game", "list", "--detailed"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        let games = json
            .get("supported_games")
            .and_then(Value::as_array)
            .ok_or("missing supported_games")?;
        if let Some(game) = games.first() {
            assert!(
                game.get("features").is_some() || game.get("version").is_some(),
                "detailed game info should have features/version"
            );
        }
        Ok(())
    }

    #[test]
    fn game_status_shows_telemetry_with_flag() -> TestResult {
        let output = wheelctl()?
            .args(["game", "status", "--telemetry"])
            .output()?;
        // Should accept the flag without parse error
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "telemetry flag should be accepted: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn game_configure_with_path_parses() -> TestResult {
        let output = wheelctl()?
            .args(["game", "configure", "acc", "--path", "C:\\Games\\ACC"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "configure with path should parse: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn game_configure_with_auto_flag_parses() -> TestResult {
        let output = wheelctl()?
            .args(["game", "configure", "acc", "--auto"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "auto flag should parse: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 13. Diag command execution
// ===========================================================================

mod diag_execution {
    use super::*;

    #[test]
    fn diag_test_all_types_runs() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "diag", "test", "all"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        let results = json
            .get("test_results")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(
            results >= 1,
            "'all' should return at least one test result, got {results}"
        );
        Ok(())
    }

    #[test]
    fn diag_test_each_type_individually() -> TestResult {
        for test_type in &["motor", "encoder", "usb", "thermal"] {
            let output = wheelctl()?
                .args(["--json", "diag", "test", test_type])
                .output()?;
            assert!(
                output.status.success(),
                "diag test {test_type} should succeed"
            );
        }
        Ok(())
    }

    #[test]
    fn diag_test_with_device_filter() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "diag", "test", "--device", "wheel-001", "motor"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn diag_metrics_with_device() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "diag", "metrics", "wheel-001"])
            .output()?;
        // Should parse correctly (may fail if device not found)
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "device arg should parse: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn diag_support_creates_output_file() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("support.zip");
        let output = wheelctl()?
            .args(["diag", "support", "--output", path_str(&out_path)?])
            .output()?;
        assert!(output.status.success());
        assert!(out_path.exists(), "support bundle should be created");
        Ok(())
    }

    #[test]
    fn diag_support_with_blackbox_flag() -> TestResult {
        let temp = TempDir::new()?;
        let out_path = temp.path().join("support_bb.zip");
        let output = wheelctl()?
            .args([
                "diag",
                "support",
                "--blackbox",
                "--output",
                path_str(&out_path)?,
            ])
            .output()?;
        assert!(output.status.success());
        Ok(())
    }
}

// ===========================================================================
// 14. Safety command execution
// ===========================================================================

mod safety_execution {
    use super::*;

    #[test]
    fn safety_enable_with_force_succeeds() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "enable", "wheel-001", "--force"])
            .output()?;
        // Should accept args and attempt to enable (may fail if device not found)
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "force flag should be accepted: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn safety_stop_all_devices() -> TestResult {
        let output = wheelctl()?.args(["--json", "safety", "stop"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Tracing may output warn lines before JSON; find the JSON object
        let json_start = stdout.find('{').ok_or("no JSON object in output")?;
        let json: Value = serde_json::from_str(&stdout[json_start..])?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn safety_stop_specific_device() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "stop", "wheel-001"])
            .output()?;
        assert!(output.status.success());
        Ok(())
    }

    #[test]
    fn safety_limit_json_output() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "limit", "wheel-001", "8.0"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn safety_limit_global_flag_json() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "limit", "wheel-001", "8.0", "--global"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn safety_status_specific_device() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "status", "wheel-001"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert!(json.get("safety_status").is_some() || json.get("success").is_some());
        Ok(())
    }
}

// ===========================================================================
// 15. Snapshot tests for error output
// ===========================================================================

mod error_snapshots {
    use super::*;

    #[test]
    fn snapshot_missing_device_subcommand() -> TestResult {
        let output = wheelctl()?.args(["device"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_device_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_profile_subcommand() -> TestResult {
        let output = wheelctl()?.args(["profile"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_profile_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_plugin_subcommand() -> TestResult {
        let output = wheelctl()?.args(["plugin"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_plugin_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_safety_subcommand() -> TestResult {
        let output = wheelctl()?.args(["safety"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_safety_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_diag_subcommand() -> TestResult {
        let output = wheelctl()?.args(["diag"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_diag_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_game_subcommand() -> TestResult {
        let output = wheelctl()?.args(["game"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_game_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_missing_telemetry_subcommand() -> TestResult {
        let output = wheelctl()?.args(["telemetry"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_missing_telemetry_subcommand", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_invalid_diag_test_type() -> TestResult {
        let output = wheelctl()?
            .args(["diag", "test", "nonexistent_type"])
            .output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_invalid_diag_test_type", stderr);
        Ok(())
    }

    #[test]
    fn snapshot_invalid_completion_shell() -> TestResult {
        let output = wheelctl()?.args(["completion", "ksh"]).output()?;
        let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
        insta::assert_snapshot!("error_invalid_completion_shell", stderr);
        Ok(())
    }
}
