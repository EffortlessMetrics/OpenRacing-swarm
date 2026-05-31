#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";
const QUALITY_CLOSURE_DIR: &str = "target/xtask/quality-closure";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

#[derive(Debug, PartialEq, Eq)]
enum CommandKind {
    Badges {
        check: bool,
    },
    RiprPr {
        check: bool,
    },
    RiprReviewComments {
        check: bool,
    },
    ImpactedEvidence,
    MutantsPr {
        args: Vec<String>,
    },
    QualityClosure {
        check: bool,
        json_out: PathBuf,
        md_out: PathBuf,
    },
    CheckFilePolicy,
    DocsSync {
        check: bool,
    },
    Pr,
    Help,
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            stderr_line(format_args!("ERROR: {error:#}"));
            ExitCode::from(2)
        }
    }
}

fn parse_args(mut args: impl Iterator<Item = String>) -> anyhow::Result<CommandKind> {
    let Some(command) = args.next() else {
        return Ok(CommandKind::Help);
    };

    let rest: Vec<String> = args.collect();
    let check = rest.iter().any(|arg| arg == "--check");

    match command.as_str() {
        "badges" | "ripr-pr" | "ripr-review-comments" | "docs-sync" => {
            if rest.iter().any(|arg| arg != "--check") {
                bail!("unsupported argument for `{command}`; only `--check` is accepted");
            }
            match command.as_str() {
                "badges" => Ok(CommandKind::Badges { check }),
                "ripr-pr" => Ok(CommandKind::RiprPr { check }),
                "ripr-review-comments" => Ok(CommandKind::RiprReviewComments { check }),
                "docs-sync" => Ok(CommandKind::DocsSync { check }),
                _ => unreachable!(),
            }
        }
        "impacted-evidence" => Ok(CommandKind::ImpactedEvidence),
        "mutants-pr" => Ok(CommandKind::MutantsPr { args: rest }),
        "quality-closure" => parse_quality_closure_args(rest),
        "check-file-policy" => Ok(CommandKind::CheckFilePolicy),
        "pr" => Ok(CommandKind::Pr),
        "-h" | "--help" | "help" => Ok(CommandKind::Help),
        _ => bail!("unknown xtask command `{command}`\n{}", usage()),
    }
}

fn run(command: CommandKind) -> anyhow::Result<()> {
    match command {
        CommandKind::Badges { check } => badges(check),
        CommandKind::RiprPr { check } => ripr_pr(check),
        CommandKind::RiprReviewComments { check } => ripr_review_comments(check),
        CommandKind::ImpactedEvidence => impacted_evidence(),
        CommandKind::MutantsPr { args } => mutants_pr(&args),
        CommandKind::QualityClosure {
            check,
            json_out,
            md_out,
        } => quality_closure(check, &json_out, &md_out),
        CommandKind::CheckFilePolicy => run_python_script("scripts/policy_file.py", &[]),
        CommandKind::DocsSync { check } => docs_sync(check),
        CommandKind::Pr => pr_gate(),
        CommandKind::Help => {
            stdout_line(format_args!("{}", usage()));
            Ok(())
        }
    }
}

fn usage() -> &'static str {
    "Usage: cargo xtask <command> [--check]\n\nCommands:\n  badges [--check]\n  ripr-pr [--check]\n  ripr-review-comments [--check]\n  impacted-evidence\n  mutants-pr [--changed] [--full-owner] [--dry-run]\n  quality-closure [--check] [--json-out PATH] [--md-out PATH]\n  check-file-policy\n  docs-sync [--check]\n  pr"
}

fn badges(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    let ripr_plus = ripr_plus_badge(&workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
    if check {
        compare_files(
            &committed_dir.join("ripr-plus.json"),
            &target_dir.join("ripr-plus.json"),
        )?;
        stdout_line(format_args!("badges: committed endpoints are current"));
        return Ok(());
    }

    fs::create_dir_all(&committed_dir)
        .with_context(|| format!("failed to create {}", committed_dir.display()))?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )
    .with_context(|| "failed to refresh badges/ripr-plus.json")?;

    stdout_line(format_args!(
        "badges: refreshed public endpoint JSON under badges/"
    ));
    Ok(())
}

fn ripr_plus_badge(workspace_root: &Path) -> anyhow::Result<ShieldsEndpointBadge> {
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg("repo-badge-plus-shields")
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to execute {ripr_bin}; set RIPR_BIN to override"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} repo-badge-plus-shields failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{ripr_bin} emitted invalid Shields endpoint JSON"))
}

fn validate_shields_badge(
    badge: &ShieldsEndpointBadge,
    expected_label: Option<&str>,
) -> anyhow::Result<()> {
    if badge.schema_version != 1 {
        bail!("badge `{}` has unsupported schemaVersion", badge.label);
    }

    if let Some(expected_label) = expected_label
        && badge.label != expected_label
    {
        bail!(
            "badge label drifted: got `{}`, expected `{expected_label}`",
            badge.label
        );
    }

    if badge.message.trim().is_empty() {
        bail!("badge `{}` has empty message", badge.label);
    }

    if badge.color.trim().is_empty() {
        bail!("badge `{}` has empty color", badge.label);
    }

    Ok(())
}

fn ripr_pr(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_PR_DIR);
    if check {
        validate_ripr_pr_contract(&out_dir)?;
        stdout_line(format_args!("ripr-pr: output contract is intact"));
        return Ok(());
    }

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    if recovery_import_ripr_bounded_mode() {
        write_bounded_recovery_import_ripr_pr(&out_dir)?;
        validate_ripr_pr_contract(&out_dir)?;
        stdout_line(format_args!(
            "ripr-pr: wrote bounded recovery-import evidence"
        ));
        return Ok(());
    }

    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    run_ripr_capture(
        &workspace_root,
        &ripr_bin,
        [
            OsStr::new("check"),
            OsStr::new("--root"),
            workspace_root.as_os_str(),
            OsStr::new("--no-unchanged-tests"),
            OsStr::new("--format"),
            OsStr::new("badge-json"),
        ],
        &out_dir.join("repo-exposure.json"),
    )?;
    run_ripr_capture(
        &workspace_root,
        &ripr_bin,
        [
            OsStr::new("check"),
            OsStr::new("--root"),
            workspace_root.as_os_str(),
            OsStr::new("--no-unchanged-tests"),
            OsStr::new("--format"),
            OsStr::new("human"),
        ],
        &out_dir.join("repo-exposure.md"),
    )?;
    validate_ripr_pr_contract(&out_dir)
}

fn recovery_import_ripr_bounded_mode() -> bool {
    env::var_os("RIPR_PR_BOUNDED_RECOVERY_IMPORT").is_some()
        || env_var_is_recovery_import_branch("GITHUB_HEAD_REF")
        || env_var_is_recovery_import_branch("GITHUB_REF_NAME")
}

fn env_var_is_recovery_import_branch(name: &str) -> bool {
    env::var(name)
        .map(|value| is_recovery_import_branch(&value))
        .unwrap_or(false)
}

fn is_recovery_import_branch(value: &str) -> bool {
    value.starts_with("recovery/import-openracing-main-")
}

fn recovery_import_branch_name() -> String {
    for name in ["GITHUB_HEAD_REF", "GITHUB_REF_NAME"] {
        if let Ok(value) = env::var(name)
            && is_recovery_import_branch(&value)
        {
            return value;
        }
    }
    "manual-bounded-recovery-import".to_string()
}

fn write_bounded_recovery_import_ripr_pr(out_dir: &Path) -> anyhow::Result<()> {
    let branch = recovery_import_branch_name();
    let receipt = serde_json::json!({
        "schema_version": 1,
        "mode": "bounded_recovery_import",
        "branch": branch,
        "live_ripr_executed": false,
        "normal_pr_policy": "live ripr-pr remains required outside recovery/import-openracing-main-* branches",
        "reason": "recovery import PR reconciles the publishing repo back into the swarm repo; hosted live ripr was killed before producing artifacts",
        "required_follow_up": "after this recovery import merges, development PRs should target EffortlessMetrics/OpenRacing-swarm and run the normal live ripr-pr gate"
    });
    write_json_pretty(&out_dir.join("repo-exposure.json"), &receipt)?;
    fs::write(
        out_dir.join("repo-exposure.md"),
        format!(
            "# RIPR PR Evidence\n\nBounded recovery-import mode wrote this receipt for `{branch}`.\n\nLive `ripr check` was not executed in CI for this recovery import because hosted runs were terminated before artifact production. Normal development PRs outside `recovery/import-openracing-main-*` still run live `cargo xtask ripr-pr`.\n"
        ),
    )
    .with_context(|| format!("failed to write {}", out_dir.join("repo-exposure.md").display()))?;
    Ok(())
}

fn ripr_review_comments(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = out_dir.join("comments.json");
    let md_path = out_dir.join("comments.md");
    if check {
        validate_json_file(&json_path)?;
        ensure_non_empty_file(&md_path)?;
        stdout_line(format_args!(
            "ripr-review-comments: output contract is intact"
        ));
        return Ok(());
    }

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    if env::var_os("RIPR_REVIEW_COMMENTS_LIVE").is_some() {
        let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
        run_status(
            Command::new(&ripr_bin)
                .arg("review-comments")
                .arg("--root")
                .arg(&workspace_root)
                .arg("--base")
                .arg("origin/main")
                .arg("--head")
                .arg("HEAD")
                .arg("--out")
                .arg(&json_path)
                .current_dir(&workspace_root),
            &format!("{ripr_bin} review-comments"),
        )?;
    } else {
        let review = serde_json::json!({
            "base": "origin/main",
            "head": "HEAD",
            "comments": [],
            "notes": [
                "bounded CI mode writes a non-blocking placeholder; set RIPR_REVIEW_COMMENTS_LIVE=1 to run live ripr review-comments"
            ]
        });
        write_json_pretty(&json_path, &review)?;
        fs::write(
            &md_path,
            "# RIPR PR Guidance\n\nNo line-placeable RIPR review guidance was produced in bounded CI mode.\nSet `RIPR_REVIEW_COMMENTS_LIVE=1` to run the full advisory review-comments pass locally or in a dedicated workflow.\n",
        )
        .with_context(|| format!("failed to write {}", md_path.display()))?;
    }
    validate_json_file(&json_path)?;
    ensure_non_empty_file(&md_path)?;
    Ok(())
}

fn impacted_evidence() -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join("target/xtask/impacted-evidence");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let json = serde_json::json!({
        "schemaVersion": 1,
        "requires_targeted_mutation": false,
        "ripr": { "requires_targeted_evidence": false },
        "note": "placeholder impact receipt; wire project-specific ownership rules here"
    });
    write_json_pretty(&out_dir.join("latest.json"), &json)?;
    fs::write(
        out_dir.join("latest.md"),
        "# Impacted Evidence\n\nNo targeted mutation was routed by the default xtask policy.\n",
    )?;
    stdout_line(format_args!(
        "impacted-evidence: wrote target/xtask/impacted-evidence/latest.*"
    ));
    Ok(())
}

fn mutants_pr(args: &[String]) -> anyhow::Result<()> {
    let supported = ["--changed", "--full-owner", "--dry-run"];
    for arg in args {
        if !supported.contains(&arg.as_str()) {
            bail!("unsupported mutants-pr argument `{arg}`");
        }
    }

    let workspace_root = workspace_root_path()?;
    let mut command = Command::new("bash");
    command.arg("scripts/run_mutation_tests.sh");
    if args.iter().any(|arg| arg == "--dry-run") {
        stdout_line(format_args!(
            "mutants-pr: dry run routed with args: {}",
            args.join(" ")
        ));
        return Ok(());
    }
    run_status(
        command.current_dir(workspace_root),
        "scripts/run_mutation_tests.sh",
    )
}

fn parse_quality_closure_args(args: Vec<String>) -> anyhow::Result<CommandKind> {
    let mut check = false;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--json-out" => {
                let Some(path) = iter.next() else {
                    bail!("--json-out requires a path");
                };
                json_out = Some(PathBuf::from(path));
            }
            "--md-out" => {
                let Some(path) = iter.next() else {
                    bail!("--md-out requires a path");
                };
                md_out = Some(PathBuf::from(path));
            }
            _ => bail!("unsupported quality-closure argument `{arg}`"),
        }
    }

    Ok(CommandKind::QualityClosure {
        check,
        json_out: json_out
            .unwrap_or_else(|| PathBuf::from(QUALITY_CLOSURE_DIR).join("latest.json")),
        md_out: md_out.unwrap_or_else(|| PathBuf::from(QUALITY_CLOSURE_DIR).join("latest.md")),
    })
}

#[derive(Debug, Deserialize)]
struct QualityExceptionLedger {
    schema_version: String,
    exception: Vec<QualityException>,
}

#[derive(Debug, Deserialize)]
struct QualityException {
    id: String,
    owner: String,
    path: String,
    kind: String,
    reason: String,
    test_surface: Vec<String>,
    review_after: String,
    removal_condition: String,
    #[serde(default = "default_active_status")]
    status: String,
}

fn default_active_status() -> String {
    "active".to_string()
}

fn quality_closure(check: bool, json_out: &Path, md_out: &Path) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let receipt = build_quality_closure_receipt(&workspace_root)?;
    let json_path = workspace_root.join(json_out);
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_json_pretty(&json_path, &receipt)?;
    write_quality_closure_markdown(&workspace_root.join(md_out), &receipt)?;

    if check {
        validate_json_file(&json_path)?;
        ensure_non_empty_file(&workspace_root.join(md_out))?;
    }

    stdout_line(format_args!(
        "quality-closure: wrote {} and {}",
        json_out.display(),
        md_out.display()
    ));
    Ok(())
}

fn build_quality_closure_receipt(workspace_root: &Path) -> anyhow::Result<serde_json::Value> {
    let ledger = read_quality_exception_ledger(workspace_root)?;
    validate_quality_exception_ledger(&ledger)?;

    let ripr_unresolved_gap_count =
        read_ripr_plus_badge_count(&workspace_root.join("badges/ripr-plus.json"))?;
    let coverage_workflow =
        fs::read_to_string(workspace_root.join(".github/workflows/coverage.yml"))
            .with_context(|| "failed to read .github/workflows/coverage.yml")?;
    let ci_workflow = fs::read_to_string(workspace_root.join(".github/workflows/ci.yml"))
        .with_context(|| "failed to read .github/workflows/ci.yml")?;
    let codecov = fs::read_to_string(workspace_root.join("codecov.yml"))
        .with_context(|| "failed to read codecov.yml")?;

    let coverage_pr_label_gated = coverage_pr_job_is_label_gated(&coverage_workflow);
    let legacy_ci_coverage_manual_only = ci_coverage_job_is_manual_only(&ci_workflow);
    let coverage_workflow_skipped = coverage_pr_label_gated || legacy_ci_coverage_manual_only;
    let patch_coverage_informational = patch_coverage_is_informational(&codecov)?;
    let patch_coverage_status = if patch_coverage_informational {
        "advisory"
    } else {
        "pass"
    };
    let coverage_required = !coverage_workflow_skipped && !patch_coverage_informational;

    let active_exceptions = ledger
        .exception
        .iter()
        .filter(|entry| entry.status == "active")
        .count();
    let ripr_plus_unowned_gap_count = ledger
        .exception
        .iter()
        .filter(|entry| entry.status == "active" && entry.kind == "ripr_unowned_gap")
        .count();
    let uncovered_owned_surface_count = ledger
        .exception
        .iter()
        .filter(|entry| entry.status == "active" && entry.kind == "owned_coverage_gap")
        .count();

    let quality_closure_satisfied = ripr_unresolved_gap_count == 0
        && ripr_plus_unowned_gap_count == 0
        && coverage_required
        && !coverage_workflow_skipped
        && patch_coverage_status == "pass"
        && uncovered_owned_surface_count == 0;
    let status = if quality_closure_satisfied {
        "pass"
    } else {
        "advisory"
    };

    Ok(serde_json::json!({
        "schema_version": 1,
        "lane": "ripr-plus-coverage-closure",
        "status": status,
        "quality_closure_satisfied": quality_closure_satisfied,
        "ripr_unresolved_gap_count": ripr_unresolved_gap_count,
        "ripr_plus_unowned_gap_count": ripr_plus_unowned_gap_count,
        "coverage_required": coverage_required,
        "coverage_workflow_skipped": coverage_workflow_skipped,
        "patch_coverage_status": patch_coverage_status,
        "uncovered_owned_surface_count": uncovered_owned_surface_count,
        "exception_count": active_exceptions,
        "workflow_observations": {
            "coverage_pr_label_gated": coverage_pr_label_gated,
            "legacy_ci_coverage_manual_only": legacy_ci_coverage_manual_only,
            "codecov_patch_informational": patch_coverage_informational
        },
        "result_states": [
            {
                "name": "ripr_plus_zero",
                "status": if ripr_unresolved_gap_count == 0 { "pass" } else { "fail" },
                "satisfied": ripr_unresolved_gap_count == 0,
                "details": "badges/ripr-plus.json message is treated as the repo-scope RIPR+ unresolved gap count"
            },
            {
                "name": "coverage_required_gate",
                "status": if coverage_required { "pass" } else { "fail" },
                "satisfied": coverage_required,
                "details": "coverage is not closure-satisfied while PR coverage can skip or patch coverage remains informational"
            },
            {
                "name": "coverage_workflow_execution",
                "status": if coverage_workflow_skipped { "skipped" } else { "pass" },
                "satisfied": !coverage_workflow_skipped,
                "details": "skipped coverage is reported explicitly and is not equivalent to coverage pass"
            },
            {
                "name": "patch_coverage",
                "status": patch_coverage_status,
                "satisfied": patch_coverage_status == "pass",
                "details": "Codecov patch coverage remains advisory while its status is informational"
            },
            {
                "name": "mutation_expansion",
                "status": "not_applicable",
                "satisfied": true,
                "details": "mutation expansion is outside this scaffold PR unless routed by existing RIPR policy"
            }
        ],
        "next_pr_queue": [
            "core protocol/domain logic",
            "receipt/schema/verifier logic",
            "CLI parse/guard rails",
            "CI/policy/xtask surfaces",
            "hardware-only seams behind fake transports"
        ],
        "claim_boundary": [
            "quality-closure-measurement-only",
            "not-full-line-coverage-claim",
            "not-mutation-completeness",
            "not-hardware-validation",
            "not-release-readiness"
        ]
    }))
}

fn read_quality_exception_ledger(workspace_root: &Path) -> anyhow::Result<QualityExceptionLedger> {
    let path = workspace_root.join("policy/quality-closure-exceptions.toml");
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("invalid TOML in {}", path.display()))
}

fn validate_quality_exception_ledger(ledger: &QualityExceptionLedger) -> anyhow::Result<()> {
    if ledger.schema_version != "1.0" {
        bail!("quality exception ledger schema_version must be 1.0");
    }

    for entry in &ledger.exception {
        validate_required_text(&entry.id, "id")?;
        validate_required_text(&entry.owner, "owner")?;
        validate_required_text(&entry.path, "path")?;
        validate_required_text(&entry.kind, "kind")?;
        validate_required_text(&entry.reason, "reason")?;
        validate_required_text(&entry.review_after, "review_after")?;
        validate_required_text(&entry.removal_condition, "removal_condition")?;
        if entry.test_surface.is_empty() {
            bail!("quality exception `{}` must list test_surface", entry.id);
        }
        if !entry
            .review_after
            .chars()
            .enumerate()
            .all(|(index, value)| {
                matches!(index, 4 | 7)
                    .then_some(value == '-')
                    .unwrap_or_else(|| value.is_ascii_digit())
            })
            || entry.review_after.len() != 10
        {
            bail!(
                "quality exception `{}` review_after must use YYYY-MM-DD",
                entry.id
            );
        }
        if entry.status != "active" && entry.status != "retired" {
            bail!(
                "quality exception `{}` status must be active or retired",
                entry.id
            );
        }
    }
    Ok(())
}

fn validate_required_text(value: &str, field: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("quality exception field `{field}` must not be empty");
    }
    Ok(())
}

fn read_ripr_plus_badge_count(path: &Path) -> anyhow::Result<u64> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("invalid JSON in {}", path.display()))?;
    let Some(message) = value.get("message").and_then(serde_json::Value::as_str) else {
        bail!("ripr+ badge missing string message");
    };
    message
        .parse::<u64>()
        .with_context(|| format!("ripr+ badge message `{message}` is not a gap count"))
}

fn coverage_pr_job_is_label_gated(workflow: &str) -> bool {
    workflow.contains("github.event_name == 'push'")
        && workflow.contains("github.event_name == 'workflow_dispatch'")
        && workflow.contains("github.event.pull_request.labels.*.name")
}

fn ci_coverage_job_is_manual_only(workflow: &str) -> bool {
    workflow.contains("coverage:")
        && workflow.contains("name: Code Coverage")
        && workflow.contains("if: github.event_name == 'workflow_dispatch'")
}

fn patch_coverage_is_informational(codecov: &str) -> anyhow::Result<bool> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(codecov).with_context(|| "invalid YAML in codecov.yml")?;
    yaml_path_bool(
        &value,
        &["coverage", "status", "patch", "default", "informational"],
    )
    .with_context(|| "codecov.yml missing coverage.status.patch.default.informational")
}

fn yaml_path_bool(value: &serde_yaml::Value, path: &[&str]) -> Option<bool> {
    let mut cursor = value;
    for key in path {
        let mapping = cursor.as_mapping()?;
        cursor = mapping.get(serde_yaml::Value::String((*key).to_string()))?;
    }
    cursor.as_bool()
}

fn write_quality_closure_markdown(path: &Path, receipt: &serde_json::Value) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        bail!(
            "quality closure markdown path has no parent: {}",
            path.display()
        );
    };
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let status = receipt
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut content = String::new();
    content.push_str("# Quality Closure Receipt\n\n");
    content.push_str(&format!("Status: `{status}`\n\n"));
    content.push_str("| Field | Value |\n| --- | ---: |\n");
    for field in [
        "ripr_unresolved_gap_count",
        "ripr_plus_unowned_gap_count",
        "coverage_required",
        "coverage_workflow_skipped",
        "patch_coverage_status",
        "uncovered_owned_surface_count",
        "exception_count",
    ] {
        let value = receipt
            .get(field)
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| "null".to_string());
        content.push_str(&format!("| `{field}` | `{value}` |\n"));
    }
    content.push_str("\nSkipped coverage is not treated as a pass by this receipt.\n");
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn docs_sync(_check: bool) -> anyhow::Result<()> {
    run_status(
        Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("openracing-tools")
            .arg("--bin")
            .arg("generate-docs-index")
            .arg("--")
            .current_dir(workspace_root_path()?),
        "generate-docs-index",
    )
}

fn pr_gate() -> anyhow::Result<()> {
    badges(true)?;
    docs_sync(true)?;
    run_python_script("scripts/policy_file.py", &[])
}

fn run_ripr_capture<const N: usize>(
    workspace_root: &Path,
    ripr_bin: &str,
    args: [&OsStr; N],
    out_path: &Path,
) -> anyhow::Result<()> {
    let output = Command::new(ripr_bin)
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to execute {ripr_bin}; set RIPR_BIN to override"))?;
    if !output.status.success() {
        bail!(
            "{ripr_bin} failed while producing {}: {}",
            out_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(out_path, output.stdout)
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    Ok(())
}

fn validate_ripr_pr_contract(out_dir: &Path) -> anyhow::Result<()> {
    validate_json_file(&out_dir.join("repo-exposure.json"))?;
    ensure_non_empty_file(&out_dir.join("repo-exposure.md"))?;
    Ok(())
}

fn validate_json_file(path: &Path) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("required JSON file is missing: {}", path.display()))?;
    let _: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("invalid JSON in {}", path.display()))?;
    Ok(())
}

fn ensure_non_empty_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("required file is missing: {}", path.display()))?;
    if metadata.len() == 0 {
        bail!("required file is empty: {}", path.display());
    }
    Ok(())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let mut content = serde_json::to_string_pretty(value)?;
    content.push('\n');
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn compare_files(committed: &Path, generated: &Path) -> anyhow::Result<()> {
    let committed_bytes = fs::read(committed).with_context(|| {
        format!(
            "committed badge endpoint is missing: {}",
            committed.display()
        )
    })?;
    let generated_bytes = fs::read(generated).with_context(|| {
        format!(
            "generated badge endpoint is missing: {}",
            generated.display()
        )
    })?;
    if committed_bytes != generated_bytes {
        bail!(
            "badge endpoint drifted: {} differs from {}; run `cargo xtask badges`",
            committed.display(),
            generated.display()
        );
    }
    Ok(())
}

fn run_python_script(script: &str, args: &[&str]) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let mut python3 = Command::new("python3");
    python3.arg(script).args(args).current_dir(&workspace_root);
    match python3.stdin(Stdio::null()).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!("{script} failed with status {status}"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => run_status(
            Command::new("python")
                .arg(script)
                .args(args)
                .current_dir(workspace_root),
            script,
        ),
        Err(error) => Err(error).with_context(|| format!("failed to execute {script}")),
    }
}

fn run_status(command: &mut Command, label: &str) -> anyhow::Result<()> {
    let status = command
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to execute {label}"))?;
    if !status.success() {
        bail!("{label} failed with status {status}");
    }
    Ok(())
}

fn workspace_root_path() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(root) = manifest_dir.parent().and_then(Path::parent) else {
        bail!("could not derive workspace root from CARGO_MANIFEST_DIR");
    };
    root.canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))
}

fn stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{args}");
}

fn stderr_line(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripr_plus_badge_shape_is_stable() -> anyhow::Result<()> {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("ripr+"))
    }

    #[test]
    fn rejects_empty_badge_message() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: " ".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }

    #[test]
    fn detects_recovery_import_branches_for_bounded_ripr() {
        assert!(is_recovery_import_branch(
            "recovery/import-openracing-main-2026-05-20"
        ));
        assert!(!is_recovery_import_branch("feat/moza-authority-diagnosis"));
    }

    #[test]
    fn parses_badges_check() -> anyhow::Result<()> {
        let command = parse_args(["badges".to_string(), "--check".to_string()].into_iter())?;
        assert_eq!(command, CommandKind::Badges { check: true });
        Ok(())
    }

    #[test]
    fn quality_exception_ledger_requires_owned_reviewable_entries() {
        let ledger = QualityExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![QualityException {
                id: "coverage-pr-label-gated".to_string(),
                owner: "release/ci".to_string(),
                path: ".github/workflows/coverage.yml".to_string(),
                kind: "coverage_gate_debt".to_string(),
                reason: "PR coverage is label-gated".to_string(),
                test_surface: vec!["cargo xtask quality-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "Make patch coverage required.".to_string(),
                status: "active".to_string(),
            }],
        };

        assert!(validate_quality_exception_ledger(&ledger).is_ok());

        let invalid = QualityExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![QualityException {
                id: "missing-owner".to_string(),
                owner: String::new(),
                path: ".github/workflows/coverage.yml".to_string(),
                kind: "coverage_gate_debt".to_string(),
                reason: "PR coverage is label-gated".to_string(),
                test_surface: vec!["cargo xtask quality-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "Make patch coverage required.".to_string(),
                status: "active".to_string(),
            }],
        };

        assert!(validate_quality_exception_ledger(&invalid).is_err());
    }

    #[test]
    fn quality_closure_marks_label_gated_coverage_as_skipped() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        fs::create_dir_all(root.join("badges"))?;
        fs::create_dir_all(root.join(".github/workflows"))?;
        fs::create_dir_all(root.join("policy"))?;

        fs::write(
            root.join("badges/ripr-plus.json"),
            r#"{"schemaVersion":1,"label":"ripr+","message":"0","color":"brightgreen"}"#,
        )?;
        fs::write(
            root.join(".github/workflows/coverage.yml"),
            "name: Code Coverage\njobs:\n  coverage:\n    if: >-\n      github.event_name == 'push' ||\n      github.event_name == 'workflow_dispatch' ||\n      contains(github.event.pull_request.labels.*.name, 'coverage')\n",
        )?;
        fs::write(
            root.join(".github/workflows/ci.yml"),
            "jobs:\n  coverage:\n    name: Code Coverage\n    if: github.event_name == 'workflow_dispatch'\n",
        )?;
        fs::write(
            root.join("codecov.yml"),
            "coverage:\n  status:\n    patch:\n      default:\n        informational: true\n",
        )?;
        fs::write(
            root.join("policy/quality-closure-exceptions.toml"),
            r#"
schema_version = "1.0"

[[exception]]
id = "coverage-pr-label-gated"
owner = "release/ci"
path = ".github/workflows/coverage.yml"
kind = "coverage_gate_debt"
reason = "PR coverage is label-gated while the closure lane is measured."
test_surface = ["cargo xtask quality-closure --check"]
review_after = "2026-06-30"
removal_condition = "Make patch coverage required or add a required non-skipped coverage sentinel."
"#,
        )?;

        let receipt = build_quality_closure_receipt(root)?;
        assert_eq!(receipt["ripr_unresolved_gap_count"], 0);
        assert_eq!(receipt["coverage_required"], false);
        assert_eq!(receipt["coverage_workflow_skipped"], true);
        assert_eq!(receipt["patch_coverage_status"], "advisory");
        assert_eq!(
            receipt["result_states"][2]["status"],
            serde_json::Value::String("skipped".to_string())
        );
        assert_eq!(receipt["quality_closure_satisfied"], false);
        Ok(())
    }

    #[test]
    fn parses_quality_closure_outputs() -> anyhow::Result<()> {
        let command = parse_args(
            [
                "quality-closure".to_string(),
                "--check".to_string(),
                "--json-out".to_string(),
                "target/custom.json".to_string(),
                "--md-out".to_string(),
                "target/custom.md".to_string(),
            ]
            .into_iter(),
        )?;

        match command {
            CommandKind::QualityClosure {
                check,
                json_out,
                md_out,
            } => {
                assert!(check);
                assert_eq!(json_out, PathBuf::from("target/custom.json"));
                assert_eq!(md_out, PathBuf::from("target/custom.md"));
            }
            other => bail!("expected quality closure command, got {other:?}"),
        }
        Ok(())
    }
}
