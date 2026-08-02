//! Tests that the top-level command list stays about what users came to do.
//!
//! `wheelctl --help` listed `moza` beside `device`, `profile`, and `safety`.
//! That namespace holds 55 subcommands for the receipt-gated Moza validation
//! lane — `vendor-status-framing-diagnosis`, `authorize-vendor-authority`,
//! `pit-house-evidence` — none of which a wheel owner runs. Research
//! scaffolding had equal billing with the commands people actually need.
//!
//! It is hidden, not gated. These tests assert both halves: gone from the
//! top-level listing, and still fully invocable for the validation lane that
//! depends on it.
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

#![allow(deprecated)] // cargo_bin deprecation warnings

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

/// The top-level `Commands:` block, without the per-flag `Options:` section
/// that follows it.
fn top_level_command_list() -> Result<String, Box<dyn std::error::Error>> {
    let output = wheelctl()?.arg("--help").output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let commands = stdout
        .split("Commands:")
        .nth(1)
        .ok_or("no Commands: section in --help")?;
    Ok(commands
        .split("Options:")
        .next()
        .unwrap_or(commands)
        .to_string())
}

// ===========================================================================
// Hidden from the top level
// ===========================================================================

mod hidden_from_top_level {
    use super::*;

    #[test]
    fn moza_is_not_listed() -> TestResult {
        let commands = top_level_command_list()?;
        assert!(
            !commands.contains("moza"),
            "`moza` is back in the top-level command list:\n{commands}"
        );
        Ok(())
    }

    #[test]
    fn the_commands_users_need_are_still_listed() -> TestResult {
        // Guard against hiding the wrong thing: the core namespaces must
        // survive whatever we do to the research ones.
        let commands = top_level_command_list()?;
        for expected in ["device", "profile", "safety", "health", "diag", "game"] {
            assert!(
                commands.contains(expected),
                "`{expected}` disappeared from the top-level command list:\n{commands}"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// Hidden, not removed
// ===========================================================================

mod still_invocable {
    use super::*;

    #[test]
    fn moza_help_still_works() -> TestResult {
        wheelctl()?.args(["moza", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn moza_help_still_lists_its_subcommands() -> TestResult {
        // The validation lane drives these by name; hiding the parent must not
        // make them undiscoverable once you are inside the namespace.
        wheelctl()?
            .args(["moza", "--help"])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("probe")
                    .and(predicate::str::contains("status"))
                    .and(predicate::str::contains("descriptor")),
            );
        Ok(())
    }

    #[test]
    fn a_moza_subcommand_still_parses() -> TestResult {
        // Exercises dispatch, not just the help text: `moza probe --help`
        // resolves only if the subcommand is still wired up.
        wheelctl()?
            .args(["moza", "probe", "--help"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn moza_is_not_reported_as_an_unknown_command() -> TestResult {
        // A hidden command that stopped resolving would fail here with clap's
        // unrecognized-subcommand error rather than its own usage.
        let output = wheelctl()?.args(["moza", "--help"]).output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.contains("unrecognized subcommand"),
            "`moza` no longer resolves:\n{combined}"
        );
        Ok(())
    }
}
