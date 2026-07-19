//! Black-box tests for the `wheelctl controls` command group (issue #172).
//!
//! These drive the built binary end-to-end against the committed deterministic
//! fixture, exercising list/capture/replay without any running service or
//! hardware.

// `assert_cmd::Command::cargo_bin` is deprecated but is the repository-wide
// convention for these black-box CLI tests (see `cli_command_tests.rs`).
#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

/// Path to the committed sample capture fixture.
fn sample_fixture() -> String {
    format!(
        "{}/tests/fixtures/controls/sample.json",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn controls_help_lists_subcommands() -> TestResult {
    let out = wheelctl()?.args(["controls", "--help"]).output()?;
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for sub in ["list", "monitor", "capture", "replay"] {
        assert!(text.contains(sub), "controls help missing `{sub}`");
    }
    Ok(())
}

#[test]
fn controls_replay_reproduces_ordered_stream() -> TestResult {
    let out = wheelctl()?
        .args(["controls", "replay", &sample_fixture()])
        .output()?;
    assert!(out.status.success(), "replay failed: {out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    // Descriptor and non-actionable baseline lead the stream, followed by
    // actionable events and an explicit reset.
    assert!(text.contains("descriptor:"));
    assert!(text.contains("baseline:"));
    assert!(text.contains("event:"));
    assert!(text.contains("reset:"));
    Ok(())
}

#[test]
fn controls_replay_json_is_machine_readable() -> TestResult {
    let out = wheelctl()?
        .args(["--json", "controls", "replay", &sample_fixture()])
        .output()?;
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout)?;
    let items = value.as_array().ok_or("expected a JSON array of items")?;
    assert!(!items.is_empty(), "replay produced no items");
    Ok(())
}

#[test]
fn controls_list_reports_bindable_controls_as_json() -> TestResult {
    let out = wheelctl()?
        .args(["--json", "controls", "list", "--capture", &sample_fixture()])
        .output()?;
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout)?;
    let controls = value.as_array().ok_or("expected a JSON array")?;
    // 128 buttons + hat + 8 encoders.
    assert_eq!(controls.len(), 137);
    // Semantic status is `raw` for every control in this observe-only listing.
    assert!(
        controls
            .iter()
            .all(|c| c.get("status").and_then(Value::as_str) == Some("raw")),
        "observe-only list must report raw provenance"
    );
    Ok(())
}

#[test]
fn controls_capture_then_replay_roundtrips() -> TestResult {
    let dir = std::env::temp_dir();
    let out_path = dir.join(format!(
        "wheelctl-controls-capture-{}.json",
        std::process::id()
    ));
    let out_str = out_path.to_string_lossy().to_string();

    let generated = wheelctl()?
        .args(["controls", "capture", "--out", &out_str])
        .output()?;
    assert!(generated.status.success(), "capture failed: {generated:?}");

    let replay = wheelctl()?
        .args(["--json", "controls", "replay", &out_str])
        .output()?;
    assert!(replay.status.success());
    let value: Value = serde_json::from_slice(&replay.stdout)?;
    assert!(value.as_array().is_some_and(|a| !a.is_empty()));

    let _ = std::fs::remove_file(&out_path);
    Ok(())
}
