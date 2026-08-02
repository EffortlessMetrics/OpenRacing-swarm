#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";
const TEST_EFFICIENCY_REPORT: &str = "target/ripr/reports/test-efficiency.json";
const TEST_EFFICIENCY_MARKDOWN: &str = "target/ripr/reports/test-efficiency.md";
const TEST_EFFICIENCY_OBSERVATION_LIMIT: usize = 24;
const QUALITY_CLOSURE_DIR: &str = "target/xtask/quality-closure";
const UNSAFE_REVIEW_CLOSURE_DIR: &str = "target/xtask/unsafe-review-closure";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

#[derive(Clone, Debug)]
struct TestEfficiencyTest {
    path: String,
    name: String,
    line: usize,
    body: String,
}

#[derive(Clone, Debug, Serialize)]
struct TestEfficiencyObservation {
    line: usize,
    context: &'static str,
    value: String,
    text: String,
}

struct ClassifiedTest<'a> {
    test: &'a TestEfficiencyTest,
    owners: Vec<String>,
    class: &'static str,
    reasons: Vec<String>,
    observations: Vec<TestEfficiencyObservation>,
    oracle_kind: &'static str,
    oracle_strength: &'static str,
    limitations: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum CommandKind {
    Badges {
        check: bool,
    },
    TestEfficiencyReport,
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
    UnsafeReviewClosure {
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
        "test-efficiency-report" => {
            if !rest.is_empty() {
                bail!("unsupported argument for `test-efficiency-report`");
            }
            Ok(CommandKind::TestEfficiencyReport)
        }
        "impacted-evidence" => Ok(CommandKind::ImpactedEvidence),
        "mutants-pr" => Ok(CommandKind::MutantsPr { args: rest }),
        "quality-closure" => parse_quality_closure_args(rest),
        "unsafe-review-closure" => parse_unsafe_review_closure_args(rest),
        "check-file-policy" => Ok(CommandKind::CheckFilePolicy),
        "pr" => Ok(CommandKind::Pr),
        "-h" | "--help" | "help" => Ok(CommandKind::Help),
        _ => bail!("unknown xtask command `{command}`\n{}", usage()),
    }
}

fn run(command: CommandKind) -> anyhow::Result<()> {
    match command {
        CommandKind::Badges { check } => badges(check),
        CommandKind::TestEfficiencyReport => test_efficiency_report(),
        CommandKind::RiprPr { check } => ripr_pr(check),
        CommandKind::RiprReviewComments { check } => ripr_review_comments(check),
        CommandKind::ImpactedEvidence => impacted_evidence(),
        CommandKind::MutantsPr { args } => mutants_pr(&args),
        CommandKind::QualityClosure {
            check,
            json_out,
            md_out,
        } => quality_closure(check, &json_out, &md_out),
        CommandKind::UnsafeReviewClosure {
            check,
            json_out,
            md_out,
        } => unsafe_review_closure(check, &json_out, &md_out),
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
    "Usage: cargo xtask <command> [--check]\n\nCommands:\n  badges [--check]\n  test-efficiency-report\n  ripr-pr [--check]\n  ripr-review-comments [--check]\n  impacted-evidence\n  mutants-pr [--changed] [--full-owner] [--dry-run]\n  quality-closure [--check] [--json-out PATH] [--md-out PATH]\n  unsafe-review-closure [--check] [--json-out PATH] [--md-out PATH]\n  check-file-policy\n  docs-sync [--check]\n  pr"
}

fn test_efficiency_report() -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let tests = collect_test_efficiency_tests(&workspace_root)?;
    let report_path = workspace_root.join(TEST_EFFICIENCY_REPORT);
    let markdown_path = workspace_root.join(TEST_EFFICIENCY_MARKDOWN);
    let classified = classify_test_efficiency_tests(&tests);
    let report = test_efficiency_report_value(&classified);
    validate_test_efficiency_report(&report)?;
    write_test_efficiency_report_files(
        &report_path,
        &markdown_path,
        &report,
        &test_efficiency_report_markdown(&classified),
    )?;
    stdout_line(format_args!(
        "test-efficiency-report: scanned {} tests and wrote {}",
        tests.len(),
        report_path.display()
    ));
    Ok(())
}

fn classify_test_efficiency_tests(tests: &[TestEfficiencyTest]) -> Vec<ClassifiedTest<'_>> {
    tests
        .iter()
        .map(|test| {
            let owners = test_efficiency_reached_owners(&test.body);
            let observations = test_efficiency_observations(&test.body, test.line);
            let (class, reasons) =
                test_efficiency_class_and_reasons(&test.body, &owners, &observations);
            let oracle_kind = test_efficiency_oracle_kind(class, &reasons);
            let oracle_strength = test_efficiency_oracle_strength(class);
            let limitations = test_efficiency_limitations(class, &owners, &observations);
            ClassifiedTest {
                test,
                owners,
                class,
                reasons,
                observations,
                oracle_kind,
                oracle_strength,
                limitations,
            }
        })
        .collect()
}

fn write_test_efficiency_report_files(
    report_path: &Path,
    markdown_path: &Path,
    report: &serde_json::Value,
    markdown: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if let Some(parent) = markdown_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_json_pretty(report_path, report)?;
    fs::write(markdown_path, markdown)
        .with_context(|| format!("failed to write {}", markdown_path.display()))?;
    Ok(())
}

fn validate_test_efficiency_report(report: &serde_json::Value) -> anyhow::Result<()> {
    if report
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        != Some("0.1")
    {
        bail!("test-efficiency report schema_version must be 0.1");
    }
    let tests = report
        .get("tests")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("test-efficiency report is missing tests"))?;
    let scanned = report
        .get("metrics")
        .and_then(|metrics| metrics.get("tests_scanned"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow::anyhow!("test-efficiency report is missing metrics.tests_scanned")
        })?;
    if scanned != tests.len() as u64 {
        bail!("test-efficiency report tests_scanned does not match tests length");
    }
    for test in tests {
        let class = test
            .get("class")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("test-efficiency entry is missing class"))?;
        if !matches!(
            class,
            "strong_discriminator"
                | "useful_but_broad"
                | "smoke_only"
                | "likely_vacuous"
                | "possibly_circular"
                | "duplicative"
                | "opaque"
        ) {
            bail!("test-efficiency entry has unknown class `{class}`");
        }
    }
    Ok(())
}

fn collect_test_efficiency_tests(workspace_root: &Path) -> anyhow::Result<Vec<TestEfficiencyTest>> {
    let mut rust_files = Vec::new();
    for relative_root in ["crates", "tests", "src"] {
        let root = workspace_root.join(relative_root);
        if root.exists() {
            collect_rust_files(&root, &mut rust_files)?;
        }
    }
    rust_files.sort();

    let mut tests = Vec::new();
    for path in rust_files {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read Rust test source {}", path.display()))?;
        tests.extend(test_efficiency_tests_in_text(workspace_root, &path, &text));
    }
    tests.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.name.cmp(&right.name))
    });
    Ok(tests)
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read Rust source directory {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = path.file_name().and_then(OsStr::to_str);
            if matches!(name, Some(".git" | "target" | "node_modules" | "vendor")) {
                continue;
            }
            collect_rust_files(&path, files)?;
        } else if file_type.is_file() && path.extension().and_then(OsStr::to_str) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn test_efficiency_tests_in_text(
    workspace_root: &Path,
    path: &Path,
    text: &str,
) -> Vec<TestEfficiencyTest> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut tests = Vec::new();
    let mut pending_test_attribute = false;
    let mut pending_attribute_depth = 0usize;
    let mut index = 0usize;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if is_test_attribute(trimmed) {
            pending_test_attribute = true;
            pending_attribute_depth = attribute_parenthesis_depth(trimmed);
            index += 1;
            continue;
        }
        if pending_test_attribute {
            if pending_attribute_depth > 0 {
                pending_attribute_depth =
                    update_attribute_parenthesis_depth(pending_attribute_depth, trimmed);
                index += 1;
                continue;
            }
            if trimmed.is_empty() {
                index += 1;
                continue;
            }
            if trimmed.starts_with("#[") {
                pending_attribute_depth = attribute_parenthesis_depth(trimmed);
                index += 1;
                continue;
            }
            if trimmed.starts_with("//") {
                index += 1;
                continue;
            }
            if let Some(name) = test_function_name(trimmed) {
                let end = test_function_end(&lines, index);
                let relative_path = path
                    .strip_prefix(workspace_root)
                    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));
                tests.push(TestEfficiencyTest {
                    path: relative_path,
                    name,
                    line: index + 1,
                    body: lines[index..=end].join("\n"),
                });
                pending_test_attribute = false;
                index = end.saturating_add(1);
                continue;
            }
            pending_test_attribute = false;
        }
        index += 1;
    }
    tests
}

fn is_test_attribute(line: &str) -> bool {
    let compact = line.replace(' ', "");
    compact == "#[test]"
        || compact.starts_with("#[tokio::test")
        || compact.starts_with("#[async_std::test")
        || compact.starts_with("#[rstest")
}

fn attribute_parenthesis_depth(line: &str) -> usize {
    line.chars()
        .fold(0usize, |depth, character| match character {
            '(' => depth.saturating_add(1),
            ')' => depth.saturating_sub(1),
            _ => depth,
        })
}

fn update_attribute_parenthesis_depth(depth: usize, line: &str) -> usize {
    line.chars()
        .fold(depth, |depth, character| match character {
            '(' => depth.saturating_add(1),
            ')' => depth.saturating_sub(1),
            _ => depth,
        })
}

fn test_function_name(line: &str) -> Option<String> {
    let function_start = line.find("fn ")? + 3;
    let name = line[function_start..]
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn test_function_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0isize;
    let mut saw_body = false;
    let mut lexical_state = BraceScanState::Code;
    for (offset, line) in lines[start..].iter().enumerate() {
        for brace in brace_scan_events(line, &mut lexical_state) {
            match brace {
                '{' => {
                    depth += 1;
                    saw_body = true;
                }
                '}' if saw_body => depth -= 1,
                _ => {}
            }
        }
        if saw_body && depth <= 0 {
            return start + offset;
        }
    }
    lines.len().saturating_sub(1)
}

#[derive(Clone, Copy)]
enum BraceScanState {
    Code,
    DoubleQuoted { escaped: bool },
    CharLiteral { escaped: bool },
    BlockComment { depth: usize },
    RawString { hashes: usize },
}

fn brace_scan_events(line: &str, state: &mut BraceScanState) -> Vec<char> {
    let bytes = line.as_bytes();
    let mut braces = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match state {
            BraceScanState::Code => match bytes[index] {
                b'/' if bytes.get(index + 1) == Some(&b'/') => break,
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    *state = BraceScanState::BlockComment { depth: 1 };
                    index += 2;
                }
                b'"' => {
                    *state = BraceScanState::DoubleQuoted { escaped: false };
                    index += 1;
                }
                b'\'' => {
                    *state = BraceScanState::CharLiteral { escaped: false };
                    index += 1;
                }
                b'r' => {
                    if let Some((hashes, consumed)) = raw_string_prefix(bytes, index) {
                        *state = BraceScanState::RawString { hashes };
                        index += consumed;
                    } else {
                        index += 1;
                    }
                }
                b'{' | b'}' => {
                    braces.push(bytes[index] as char);
                    index += 1;
                }
                _ => index += 1,
            },
            BraceScanState::DoubleQuoted { escaped } => {
                if *escaped {
                    *escaped = false;
                } else if bytes[index] == b'\\' {
                    *escaped = true;
                } else if bytes[index] == b'"' {
                    *state = BraceScanState::Code;
                }
                index += 1;
            }
            BraceScanState::CharLiteral { escaped } => {
                if *escaped {
                    *escaped = false;
                } else if bytes[index] == b'\\' {
                    *escaped = true;
                } else if bytes[index] == b'\'' {
                    *state = BraceScanState::Code;
                }
                index += 1;
            }
            BraceScanState::BlockComment { depth } => {
                if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
                    *depth = depth.saturating_add(1);
                    index += 2;
                } else if bytes.get(index) == Some(&b'*') && bytes.get(index + 1) == Some(&b'/') {
                    *depth = depth.saturating_sub(1);
                    index += 2;
                    if *depth == 0 {
                        *state = BraceScanState::Code;
                    }
                } else {
                    index += 1;
                }
            }
            BraceScanState::RawString { hashes } => {
                if bytes[index] == b'"'
                    && bytes
                        .get(index + 1..index + 1 + *hashes)
                        .is_some_and(|closing| closing.iter().all(|byte| *byte == b'#'))
                {
                    index += 1 + *hashes;
                    *state = BraceScanState::Code;
                } else {
                    index += 1;
                }
            }
        }
    }
    braces
}

fn raw_string_prefix(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if bytes.get(start) != Some(&b'r') {
        return None;
    }
    let mut index = start + 1;
    let mut hashes = 0usize;
    while bytes.get(index) == Some(&b'#') {
        hashes += 1;
        index += 1;
    }
    (bytes.get(index) == Some(&b'"')).then_some((hashes, index + 1 - start))
}

fn test_efficiency_report_value(tests: &[ClassifiedTest<'_>]) -> serde_json::Value {
    let mut class_counts = BTreeMap::new();
    for class in [
        "strong_discriminator",
        "useful_but_broad",
        "smoke_only",
        "likely_vacuous",
        "possibly_circular",
        "duplicative",
        "opaque",
    ] {
        class_counts.insert(class.to_string(), 0usize);
    }
    let mut reason_counts = BTreeMap::new();
    let mut entries = Vec::new();
    for test in tests {
        let entry = test_efficiency_entry_value(test);
        entries.push(entry);
        if let Some(count) = class_counts.get_mut(test.class) {
            *count += 1;
        }
        for reason in &test.reasons {
            *reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
    }
    let has_advisory_signal = class_counts
        .iter()
        .any(|(class, count)| class != "strong_discriminator" && *count > 0);
    let class_counts_value = serde_json::json!(class_counts);
    let reason_counts_value = serde_json::json!(reason_counts);
    // These fields are upstream-contract placeholders, not measured results:
    // duplicate detection and test-intent matching are not implemented here.
    serde_json::json!({
        "schema_version": "0.1",
        "status": if has_advisory_signal { "warn" } else { "pass" },
        "advisory": true,
        "counts": class_counts_value.clone(),
        "reason_counts": reason_counts_value.clone(),
        "tests": entries,
        "duplicate_groups": [],
        "test_intent": {"path": ".ripr/test_intent.toml", "declared": 0, "matched": 0},
        "metrics": {
            "tests_scanned": tests.len(),
            "class_counts": class_counts_value.clone(),
            "reason_counts": reason_counts_value.clone(),
            "duplicate_discriminator_group_count": 0
        },
        "claim_boundary": ["static advisory evidence only", "not runtime mutation proof"]
    })
}

fn test_efficiency_entry_value(test: &ClassifiedTest<'_>) -> serde_json::Value {
    serde_json::json!({
        "path": test.test.path,
        "name": test.test.name,
        "line": test.test.line,
        "class": test.class,
        "oracle_kind": test.oracle_kind,
        "oracle_strength": test.oracle_strength,
        "reached_owners": test.owners,
        "reasons": test.reasons,
        "observed_values": test.observations,
        "static_limitations": test.limitations
    })
}

fn test_efficiency_oracle_kind(class: &str, reasons: &[String]) -> &'static str {
    match class {
        "strong_discriminator" => "exact assertion",
        "useful_but_broad" => "broad predicate",
        "smoke_only" => "smoke execution",
        _ if reasons
            .iter()
            .any(|reason| reason == "no_assertion_detected") =>
        {
            "no assertion detected"
        }
        _ => "opaque oracle",
    }
}

fn test_efficiency_oracle_strength(class: &str) -> &'static str {
    match class {
        "strong_discriminator" => "strong",
        "useful_but_broad" => "weak",
        "smoke_only" => "smoke",
        _ => "opaque",
    }
}

fn test_efficiency_class_and_reasons(
    body: &str,
    owners: &[String],
    observations: &[TestEfficiencyObservation],
) -> (&'static str, Vec<String>) {
    let exact = [
        "assert_eq!(",
        "assert_ne!(",
        "assert_matches!(",
        "matches!(",
    ]
    .iter()
    .any(|pattern| body.contains(pattern));
    let smoke = body.contains("status.success()") || body.contains("assert_cmd::");
    let broad = [
        ".is_ok()",
        ".is_err()",
        ".is_some()",
        ".is_none()",
        ".is_empty()",
        ".contains(",
    ]
    .iter()
    .any(|pattern| body.contains(pattern))
        || (body.contains("assert!(") && !exact && !smoke);
    let has_assertion = exact || broad;
    let circular = body.contains("expected =")
        && owners
            .iter()
            .any(|owner| body.contains(&format!("{owner}(")));
    let class = if circular {
        "possibly_circular"
    } else if owners.is_empty() {
        "opaque"
    } else if !has_assertion && smoke {
        "smoke_only"
    } else if !has_assertion {
        "likely_vacuous"
    } else if exact {
        "strong_discriminator"
    } else {
        "useful_but_broad"
    };
    let mut reasons = BTreeSet::new();
    if !has_assertion {
        reasons.insert("no_assertion_detected".to_string());
    }
    if smoke {
        reasons.insert("smoke_oracle_only".to_string());
    }
    if broad && !exact {
        reasons.insert("broad_oracle".to_string());
    }
    if owners.is_empty() {
        reasons.insert("opaque_helper_or_fixture_boundary".to_string());
    }
    // Literal activation values, when present, are retained in the entry evidence.
    if observations.is_empty() {
        reasons.insert("no_activation_literal_detected".to_string());
    }
    if circular {
        reasons.insert("expected_value_computed_from_detected_owner_path".to_string());
    }
    (class, reasons.into_iter().collect())
}

fn test_efficiency_limitations(
    class: &str,
    owners: &[String],
    observations: &[TestEfficiencyObservation],
) -> Vec<String> {
    let mut limitations = Vec::new();
    if owners.is_empty() {
        limitations.push(
            "no direct owner call detected; helper or fixture boundary may be opaque".to_string(),
        );
    }
    if observations.is_empty() {
        limitations.push("no literal activation values detected".to_string());
    }
    match class {
        "strong_discriminator" => {}
        "useful_but_broad" => {
            limitations.push("broad oracle may miss exact discriminator drift".to_string())
        }
        "smoke_only" => limitations.push(
            "smoke-only oracle proves execution with little discriminator detail".to_string(),
        ),
        _ => limitations
            .push("static classification is advisory and does not execute the test".to_string()),
    }
    limitations
}

fn test_efficiency_observations(body: &str, start_line: usize) -> Vec<TestEfficiencyObservation> {
    let mut observations = Vec::new();
    for (offset, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        for value in quoted_literals(trimmed) {
            observations.push(TestEfficiencyObservation {
                line: start_line.saturating_add(offset),
                context: if trimmed.contains("assert") {
                    "assertion_argument"
                } else {
                    "literal"
                },
                value,
                text: trimmed.to_string(),
            });
            if observations.len() >= TEST_EFFICIENCY_OBSERVATION_LIMIT {
                return observations;
            }
        }
    }
    observations
}

fn quoted_literals(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        let mut escaped = false;
        while index < bytes.len() {
            if escaped {
                escaped = false;
            } else if bytes[index] == b'\\' {
                escaped = true;
            } else if bytes[index] == b'"' {
                index += 1;
                literals.push(line[start..index].to_string());
                break;
            }
            index += 1;
        }
    }
    literals
}

fn test_efficiency_reached_owners(body: &str) -> Vec<String> {
    let mut owners = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || is_function_declaration(trimmed) {
            continue;
        }
        for call in call_names_in_line(trimmed) {
            if !ignored_test_efficiency_call(&call) {
                owners.insert(call);
            }
        }
    }
    owners.into_iter().collect()
}

fn is_function_declaration(line: &str) -> bool {
    line.split(['(', '{'])
        .next()
        .is_some_and(|prefix| prefix.split_whitespace().any(|word| word == "fn"))
}

fn call_names_in_line(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut calls = Vec::new();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'(' || index == 0 || bytes[index - 1] == b'!' {
            continue;
        }
        let mut start = index;
        while start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b':')
        {
            start -= 1;
        }
        if start == index {
            continue;
        }
        let token = line[start..index].trim_matches(':');
        let last = token.rsplit("::").next().unwrap_or(token);
        if !token.is_empty()
            && last
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic())
        {
            calls.push(token.to_string());
        }
    }
    calls
}

fn ignored_test_efficiency_call(call: &str) -> bool {
    matches!(
        call.rsplit("::").next().unwrap_or(call),
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "assert_matches"
            | "matches"
            | "format"
            | "format_args"
            | "println"
            | "eprintln"
            | "panic"
            | "dbg"
            | "vec"
            | "default"
            | "new"
            | "join"
            | "to_string"
            | "to_owned"
            | "contains"
            | "starts_with"
            | "ends_with"
            | "is_ok"
            | "is_err"
            | "is_some"
            | "is_none"
            | "is_empty"
            | "Ok"
            | "Err"
            | "Some"
            | "None"
            | "unwrap"
            | "expect"
            | "clone"
            | "collect"
            | "map"
            | "filter"
            | "iter"
            | "into_iter"
            | "push"
            | "len"
            | "get"
            | "insert"
            | "from"
    )
}

fn test_efficiency_report_markdown(tests: &[ClassifiedTest<'_>]) -> String {
    let mut body = String::from(
        "# Test efficiency report\n\nStatus: advisory\n\nThis report is conservative static evidence about test oracle shape. It does not execute tests or run mutation analysis.\n\n",
    );
    body.push_str(&format!("Tests scanned: {}\n\n", tests.len()));
    body.push_str("| Test | Class | Oracle | Reasons |\n| --- | --- | --- | --- |\n");
    for test in tests {
        let reason_text = if test.reasons.is_empty() {
            "none".to_string()
        } else {
            test.reasons.join(", ")
        };
        body.push_str(&format!(
            "| `{}`:{} `{}` | `{}` | `{}` | {} |\n",
            test.test.path,
            test.test.line,
            test.test.name,
            test.class,
            test.oracle_kind,
            reason_text
        ));
    }
    body
}

fn badges(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    test_efficiency_report()?;
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
    let format = "repo-badge-plus-shields";
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg(format)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to execute {ripr_bin}; set RIPR_BIN to override"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} {format} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let badge: ShieldsEndpointBadge = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{ripr_bin} emitted invalid Shields endpoint JSON"))?;
    validate_numeric_badge_message(&badge)?;
    Ok(badge)
}

fn validate_numeric_badge_message(badge: &ShieldsEndpointBadge) -> anyhow::Result<()> {
    badge.message.parse::<u64>().with_context(|| {
        format!(
            "ripr badge `{}` message `{}` is not numeric",
            badge.label, badge.message
        )
    })?;
    Ok(())
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

fn parse_unsafe_review_closure_args(args: Vec<String>) -> anyhow::Result<CommandKind> {
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
            _ => bail!("unsupported unsafe-review-closure argument `{arg}`"),
        }
    }

    Ok(CommandKind::UnsafeReviewClosure {
        check,
        json_out: json_out
            .unwrap_or_else(|| PathBuf::from(UNSAFE_REVIEW_CLOSURE_DIR).join("latest.json")),
        md_out: md_out
            .unwrap_or_else(|| PathBuf::from(UNSAFE_REVIEW_CLOSURE_DIR).join("latest.md")),
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

#[derive(Clone, Debug, Serialize)]
struct QualityExceptionBreakdown {
    id: String,
    owner: String,
    path: String,
    kind: String,
    review_after: String,
    review_expired: bool,
    receipt_status: String,
    follow_up_required: bool,
    test_surface_count: u64,
    removal_condition: String,
}

impl QualityExceptionBreakdown {
    fn from_exception(
        exception: &QualityException,
        coverage_workflow_skipped: bool,
        coverage_tool_status: &str,
        patch_coverage_status: &str,
        badge_endpoint_status: &str,
        today: &str,
    ) -> Self {
        let review_expired = exception.review_after.as_str() < today;
        let (mut receipt_status, mut follow_up_required) = quality_exception_receipt_state(
            exception,
            coverage_workflow_skipped,
            coverage_tool_status,
            patch_coverage_status,
            badge_endpoint_status,
        );
        if review_expired {
            receipt_status = "fail".to_string();
            follow_up_required = true;
        }
        Self {
            id: exception.id.clone(),
            owner: exception.owner.clone(),
            path: exception.path.clone(),
            kind: exception.kind.clone(),
            review_after: exception.review_after.clone(),
            review_expired,
            receipt_status,
            follow_up_required,
            test_surface_count: exception.test_surface.len() as u64,
            removal_condition: exception.removal_condition.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnsafeSite {
    path: String,
    line: usize,
}

#[derive(Debug, Deserialize)]
struct UnsafeReviewExceptionLedger {
    schema_version: String,
    exception: Vec<UnsafeReviewException>,
}

#[derive(Debug, Deserialize)]
struct UnsafeReviewException {
    id: String,
    owner: String,
    path: String,
    kind: String,
    reason: String,
    test_surface: Vec<String>,
    review_after: String,
    removal_condition: String,
    safety_contract: String,
    local_guard: String,
    witness: String,
    #[serde(default = "default_active_status")]
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct UnsafeExceptionBreakdown {
    id: String,
    owner: String,
    path: String,
    kind: String,
    unsafe_site_count: u64,
    changed_unsafe_site_count: u64,
    unsafe_contract_missing_count: u64,
    local_guard_missing_count: u64,
    witness_missing_count: u64,
    missing_evidence_count: u64,
}

impl UnsafeExceptionBreakdown {
    fn from_exception(exception: &UnsafeReviewException) -> Self {
        Self {
            id: exception.id.clone(),
            owner: exception.owner.clone(),
            path: exception.path.clone(),
            kind: exception.kind.clone(),
            unsafe_site_count: 0,
            changed_unsafe_site_count: 0,
            unsafe_contract_missing_count: 0,
            local_guard_missing_count: 0,
            witness_missing_count: 0,
            missing_evidence_count: 0,
        }
    }
}

fn default_active_status() -> String {
    "active".to_string()
}

fn quality_closure(check: bool, json_out: &Path, md_out: &Path) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let report_path = workspace_root.join(TEST_EFFICIENCY_REPORT);
    let markdown_path = workspace_root.join(TEST_EFFICIENCY_MARKDOWN);
    for path in [&report_path, &markdown_path] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove stale {}", path.display()));
            }
        }
    }
    if let Err(error) = test_efficiency_report() {
        let _ = fs::remove_file(&report_path);
        let _ = fs::remove_file(&markdown_path);
        stderr_line(format_args!(
            "quality-closure: test-efficiency producer unavailable: {error:#}"
        ));
    }
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
    let ripr_unresolved_gap_count =
        read_ripr_plus_badge_count(&workspace_root.join("badges/ripr-plus.json"))?;
    let coverage_workflow =
        fs::read_to_string(workspace_root.join(".github/workflows/coverage.yml"))
            .with_context(|| "failed to read .github/workflows/coverage.yml")?;
    let ci_workflow = fs::read_to_string(workspace_root.join(".github/workflows/ci.yml"))
        .with_context(|| "failed to read .github/workflows/ci.yml")?;
    let codecov = fs::read_to_string(workspace_root.join("codecov.yml"))
        .with_context(|| "failed to read codecov.yml")?;
    let coverage_tool_status = detect_coverage_tool_status(workspace_root)?;
    let badge_endpoint_status = detect_badge_endpoint_status(workspace_root)?;

    build_quality_closure_receipt_value(
        &ledger,
        ripr_unresolved_gap_count,
        &coverage_workflow,
        &ci_workflow,
        &codecov,
        &coverage_tool_status,
        &badge_endpoint_status,
        &current_utc_date_string()?,
    )
}

fn build_quality_closure_receipt_value(
    ledger: &QualityExceptionLedger,
    ripr_unresolved_gap_count: u64,
    coverage_workflow: &str,
    ci_workflow: &str,
    codecov: &str,
    coverage_tool_status: &str,
    badge_endpoint_status: &str,
    today: &str,
) -> anyhow::Result<serde_json::Value> {
    validate_quality_exception_ledger(ledger)?;
    validate_status_value(coverage_tool_status, "coverage_tool_status")?;
    validate_status_value(badge_endpoint_status, "badge_endpoint_status")?;
    let coverage_pr_label_gated = coverage_pr_job_is_label_gated(coverage_workflow);
    let legacy_ci_coverage_manual_only = ci_coverage_job_is_manual_only(ci_workflow);
    let coverage_workflow_skipped = coverage_pr_label_gated || legacy_ci_coverage_manual_only;
    let patch_coverage_informational = patch_coverage_is_informational(codecov)?;
    let patch_coverage_status = if patch_coverage_informational {
        "advisory"
    } else {
        "pass"
    };
    let coverage_required = !coverage_workflow_skipped
        && !patch_coverage_informational
        && coverage_tool_status == "pass";

    let active_exceptions: Vec<&QualityException> = ledger
        .exception
        .iter()
        .filter(|entry| entry.status == "active")
        .collect();
    let mut quality_exception_breakdown: Vec<QualityExceptionBreakdown> = active_exceptions
        .iter()
        .map(|entry| {
            QualityExceptionBreakdown::from_exception(
                entry,
                coverage_workflow_skipped,
                coverage_tool_status,
                patch_coverage_status,
                badge_endpoint_status,
                today,
            )
        })
        .collect();
    quality_exception_breakdown.sort_by(|left, right| {
        right
            .follow_up_required
            .cmp(&left.follow_up_required)
            .then_with(|| {
                receipt_status_rank(&left.receipt_status)
                    .cmp(&receipt_status_rank(&right.receipt_status))
            })
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.id.cmp(&right.id))
    });

    let ripr_plus_unowned_gap_count = active_exceptions
        .iter()
        .filter(|entry| entry.kind == "ripr_unowned_gap")
        .count();
    let uncovered_owned_surface_count = active_exceptions
        .iter()
        .filter(|entry| entry.kind == "owned_coverage_gap")
        .count();
    let expired_review_count = active_exceptions
        .iter()
        .filter(|entry| entry.review_after.as_str() < today)
        .count();

    let quality_closure_satisfied = ripr_unresolved_gap_count == 0
        && ripr_plus_unowned_gap_count == 0
        && coverage_required
        && !coverage_workflow_skipped
        && coverage_tool_status == "pass"
        && patch_coverage_status == "pass"
        && badge_endpoint_status == "pass"
        && uncovered_owned_surface_count == 0
        && expired_review_count == 0;
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
        "coverage_tool_status": coverage_tool_status,
        "patch_coverage_status": patch_coverage_status,
        "badge_endpoint_status": badge_endpoint_status,
        "uncovered_owned_surface_count": uncovered_owned_surface_count,
        "expired_review_count": expired_review_count,
        "exception_count": active_exceptions.len() as u64,
        "review_date": today,
        "quality_exception_breakdown": quality_exception_breakdown,
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
                "name": "badge_endpoint_regeneration",
                "status": badge_endpoint_status,
                "satisfied": badge_endpoint_status == "pass",
                "details": "ripr+ badge endpoint regeneration is reported separately; missing test-efficiency evidence is not a badge pass"
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
                "name": "coverage_tooling",
                "status": coverage_tool_status,
                "satisfied": coverage_tool_status == "pass",
                "details": "cargo-llvm-cov availability is reported separately; missing local coverage tooling is not coverage evidence"
            },
            {
                "name": "patch_coverage",
                "status": patch_coverage_status,
                "satisfied": patch_coverage_status == "pass",
                "details": "Codecov patch coverage remains advisory while its status is informational"
            },
            {
                "name": "quality_exception_reviews",
                "status": if expired_review_count == 0 { "pass" } else { "fail" },
                "satisfied": expired_review_count == 0,
                "details": "active quality exceptions must have non-expired review_after dates"
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

fn quality_exception_receipt_state(
    exception: &QualityException,
    coverage_workflow_skipped: bool,
    coverage_tool_status: &str,
    patch_coverage_status: &str,
    badge_endpoint_status: &str,
) -> (String, bool) {
    match exception.kind.as_str() {
        "ripr_gate_debt" => (
            badge_endpoint_status.to_string(),
            badge_endpoint_status != "pass",
        ),
        "coverage_gate_debt" => match exception.id.as_str() {
            "coverage-pr-label-gated" | "legacy-ci-coverage-manual-only" => {
                if coverage_workflow_skipped {
                    ("skipped".to_string(), true)
                } else {
                    ("pass".to_string(), false)
                }
            }
            "codecov-patch-informational" => (
                patch_coverage_status.to_string(),
                patch_coverage_status != "pass",
            ),
            "coverage-local-llvm-cov-tooling" => (
                coverage_tool_status.to_string(),
                coverage_tool_status != "pass",
            ),
            "coverage-windows-command-line-limit" => ("advisory".to_string(), true),
            _ => ("fail".to_string(), true),
        },
        "owned_coverage_gap" => ("fail".to_string(), true),
        "coverage_surface_deferred" => ("advisory".to_string(), true),
        "generated" | "intentionally_advisory" => ("not_applicable".to_string(), false),
        _ => ("advisory".to_string(), true),
    }
}

fn receipt_status_rank(status: &str) -> u8 {
    match status {
        "fail" => 0,
        "skipped" => 1,
        "advisory" => 2,
        "pass" => 3,
        "not_applicable" => 4,
        _ => 5,
    }
}

fn detect_badge_endpoint_status(workspace_root: &Path) -> anyhow::Result<String> {
    if let Ok(value) = env::var("OPENRACING_BADGE_ENDPOINT_STATUS") {
        validate_status_value(&value, "OPENRACING_BADGE_ENDPOINT_STATUS")?;
        return Ok(value);
    }

    let test_efficiency_report = workspace_root.join(TEST_EFFICIENCY_REPORT);
    if !test_efficiency_report.exists() {
        return Ok("skipped".to_string());
    }

    let committed_path = workspace_root.join("badges/ripr-plus.json");
    let Ok(committed_content) = fs::read_to_string(&committed_path) else {
        return Ok("fail".to_string());
    };
    let Ok(committed_badge) = serde_json::from_str::<ShieldsEndpointBadge>(&committed_content)
    else {
        return Ok("fail".to_string());
    };
    if validate_shields_badge(&committed_badge, Some("ripr+")).is_err() {
        return Ok("fail".to_string());
    }

    let Ok(generated_badge) = ripr_plus_badge(workspace_root) else {
        return Ok("fail".to_string());
    };
    if generated_badge == committed_badge {
        Ok("pass".to_string())
    } else {
        Ok("fail".to_string())
    }
}

fn detect_coverage_tool_status(workspace_root: &Path) -> anyhow::Result<String> {
    if let Ok(value) = env::var("OPENRACING_COVERAGE_TOOL_STATUS") {
        validate_status_value(&value, "OPENRACING_COVERAGE_TOOL_STATUS")?;
        return Ok(value);
    }

    if let Ok(output) = Command::new("bash")
        .arg("-lc")
        .arg(
            "command -v cargo-llvm-cov >/dev/null 2>&1 || cargo llvm-cov --version >/dev/null 2>&1",
        )
        .output()
    {
        return Ok(if output.status.success() {
            "pass"
        } else {
            "skipped"
        }
        .to_string());
    }

    let output = Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| "failed to probe cargo llvm-cov")?;
    Ok(if output.status.success() {
        "pass"
    } else {
        "skipped"
    }
    .to_string())
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
        "coverage_tool_status",
        "patch_coverage_status",
        "badge_endpoint_status",
        "uncovered_owned_surface_count",
        "expired_review_count",
        "exception_count",
    ] {
        let value = receipt
            .get(field)
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| "null".to_string());
        content.push_str(&format!("| `{field}` | `{value}` |\n"));
    }
    append_result_states_markdown(&mut content, receipt);
    append_quality_exception_breakdown_markdown(&mut content, receipt);
    content.push_str("\nSkipped coverage is not treated as a pass by this receipt.\n");
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn append_quality_exception_breakdown_markdown(content: &mut String, receipt: &serde_json::Value) {
    let Some(rows) = receipt
        .get("quality_exception_breakdown")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    content.push_str("\n## Quality Exception Breakdown\n\n");
    content.push_str(
        "| Exception | Owner | Kind | Status | Follow-up | Expired | Review After | Test Surfaces | Path |\n",
    );
    content.push_str("| --- | --- | --- | --- | ---: | ---: | --- | ---: | --- |\n");
    for row in rows {
        let id = markdown_cell(
            row.get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let owner = markdown_cell(
            row.get("owner")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let kind = markdown_cell(
            row.get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let status = markdown_cell(
            row.get("receipt_status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
        );
        let follow_up_required = row
            .get("follow_up_required")
            .and_then(serde_json::Value::as_bool)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string());
        let review_expired = row
            .get("review_expired")
            .and_then(serde_json::Value::as_bool)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string());
        let review_after = markdown_cell(
            row.get("review_after")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let test_surface_count = json_u64_field(row, "test_surface_count");
        let path = markdown_cell(
            row.get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        content.push_str(&format!(
            "| `{id}` | `{owner}` | `{kind}` | `{status}` | `{follow_up_required}` | `{review_expired}` | `{review_after}` | `{test_surface_count}` | `{path}` |\n"
        ));
    }
}

fn append_result_states_markdown(content: &mut String, receipt: &serde_json::Value) {
    let Some(states) = receipt
        .get("result_states")
        .and_then(serde_json::Value::as_array)
        .filter(|states| !states.is_empty())
    else {
        return;
    };

    content.push_str("\n## Result States\n\n");
    content.push_str("| Name | Status | Satisfied | Details |\n| --- | --- | ---: | --- |\n");
    for state in states {
        let name = markdown_cell(
            state
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
        );
        let status = markdown_cell(
            state
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
        );
        let satisfied = state
            .get("satisfied")
            .and_then(serde_json::Value::as_bool)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string());
        let details = markdown_cell(
            state
                .get("details")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        content.push_str(&format!(
            "| `{name}` | `{status}` | `{satisfied}` | {details} |\n"
        ));
    }
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn unsafe_review_closure(check: bool, json_out: &Path, md_out: &Path) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path()?;
    let receipt = build_unsafe_review_closure_receipt(&workspace_root)?;
    let json_path = workspace_root.join(json_out);
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_json_pretty(&json_path, &receipt)?;
    write_unsafe_review_closure_markdown(&workspace_root.join(md_out), &receipt)?;

    if check {
        validate_json_file(&json_path)?;
        ensure_non_empty_file(&workspace_root.join(md_out))?;
    }

    stdout_line(format_args!(
        "unsafe-review-closure: wrote {} and {}",
        json_out.display(),
        md_out.display()
    ));
    Ok(())
}

fn build_unsafe_review_closure_receipt(workspace_root: &Path) -> anyhow::Result<serde_json::Value> {
    let ledger = read_unsafe_review_exception_ledger(workspace_root)?;
    validate_unsafe_review_exception_ledger(&ledger)?;
    let unsafe_sites = discover_unsafe_sites(workspace_root)?;
    let changed_rust_paths = discover_changed_rust_paths(workspace_root);
    let miri_status = detect_miri_status(workspace_root, unsafe_sites.len())?;
    build_unsafe_review_closure_receipt_value(
        &ledger,
        &unsafe_sites,
        &changed_rust_paths,
        &current_utc_date_string()?,
        &miri_status,
    )
}

fn build_unsafe_review_closure_receipt_value(
    ledger: &UnsafeReviewExceptionLedger,
    unsafe_sites: &[UnsafeSite],
    changed_rust_paths: &BTreeSet<String>,
    today: &str,
    miri_status: &str,
) -> anyhow::Result<serde_json::Value> {
    validate_status_value(miri_status, "miri_status")?;
    let active_exceptions: Vec<&UnsafeReviewException> = ledger
        .exception
        .iter()
        .filter(|entry| entry.status == "active")
        .collect();
    let mut unsafe_exception_breakdown: Vec<UnsafeExceptionBreakdown> = active_exceptions
        .iter()
        .map(|entry| UnsafeExceptionBreakdown::from_exception(entry))
        .collect();

    let mut unsafe_contract_missing_count = 0_u64;
    let mut local_guard_missing_count = 0_u64;
    let mut witness_missing_count = 0_u64;
    let mut unreviewed_unsafe_gap_count = 0_u64;
    let mut changed_unsafe_site_count = 0_u64;
    let mut unreviewed_samples = Vec::new();

    for site in unsafe_sites {
        if changed_rust_paths.contains(&site.path) {
            changed_unsafe_site_count += 1;
        }

        let Some(exception) = matching_unsafe_exception(site, &active_exceptions) else {
            unreviewed_unsafe_gap_count += 1;
            unsafe_contract_missing_count += 1;
            local_guard_missing_count += 1;
            witness_missing_count += 1;
            if unreviewed_samples.len() < 20 {
                unreviewed_samples.push(format!("{}:{}", site.path, site.line));
            }
            continue;
        };
        let exception_breakdown = unsafe_exception_breakdown
            .iter_mut()
            .find(|entry| entry.id == exception.id);
        if let Some(entry) = exception_breakdown {
            entry.unsafe_site_count += 1;
            if changed_rust_paths.contains(&site.path) {
                entry.changed_unsafe_site_count += 1;
            }
        }

        if exception.safety_contract == "missing" {
            unsafe_contract_missing_count += 1;
            if let Some(entry) = unsafe_exception_breakdown
                .iter_mut()
                .find(|entry| entry.id == exception.id)
            {
                entry.unsafe_contract_missing_count += 1;
                entry.missing_evidence_count += 1;
            }
        }
        if exception.local_guard == "missing" {
            local_guard_missing_count += 1;
            if let Some(entry) = unsafe_exception_breakdown
                .iter_mut()
                .find(|entry| entry.id == exception.id)
            {
                entry.local_guard_missing_count += 1;
                entry.missing_evidence_count += 1;
            }
        }
        if exception.witness == "missing" {
            witness_missing_count += 1;
            if let Some(entry) = unsafe_exception_breakdown
                .iter_mut()
                .find(|entry| entry.id == exception.id)
            {
                entry.witness_missing_count += 1;
                entry.missing_evidence_count += 1;
            }
        }
    }
    unsafe_exception_breakdown.sort_by(|left, right| {
        right
            .missing_evidence_count
            .cmp(&left.missing_evidence_count)
            .then_with(|| right.unsafe_site_count.cmp(&left.unsafe_site_count))
            .then_with(|| left.id.cmp(&right.id))
    });

    let owner_missing_count = active_exceptions
        .iter()
        .filter(|entry| entry.owner.trim().is_empty())
        .count() as u64;
    let expired_review_count = active_exceptions
        .iter()
        .filter(|entry| entry.review_after.as_str() < today)
        .count() as u64;

    let unsafe_review_closure_satisfied = unreviewed_unsafe_gap_count == 0
        && unsafe_contract_missing_count == 0
        && local_guard_missing_count == 0
        && witness_missing_count == 0
        && owner_missing_count == 0
        && expired_review_count == 0;
    let status = if unsafe_review_closure_satisfied {
        "pass"
    } else {
        "advisory"
    };

    Ok(serde_json::json!({
        "schema_version": 1,
        "lane": "unsafe-review-closure",
        "status": status,
        "unsafe_site_count": unsafe_sites.len() as u64,
        "changed_unsafe_site_count": changed_unsafe_site_count,
        "unsafe_contract_missing_count": unsafe_contract_missing_count,
        "local_guard_missing_count": local_guard_missing_count,
        "witness_missing_count": witness_missing_count,
        "owner_missing_count": owner_missing_count,
        "expired_review_count": expired_review_count,
        "unreviewed_unsafe_gap_count": unreviewed_unsafe_gap_count,
        "unsafe_review_closure_satisfied": unsafe_review_closure_satisfied,
        "miri_status": miri_status,
        "exception_count": active_exceptions.len() as u64,
        "review_date": today,
        "unreviewed_samples": unreviewed_samples,
        "unsafe_exception_breakdown": unsafe_exception_breakdown,
        "result_states": [
            {
                "name": "unsafe_site_inventory",
                "status": "pass",
                "satisfied": true,
                "details": "tracked Rust files were scanned for unsafe keyword sites outside comments and string literals"
            },
            {
                "name": "unsafe_review_evidence",
                "status": if unsafe_review_closure_satisfied { "pass" } else { "fail" },
                "satisfied": unsafe_review_closure_satisfied,
                "details": "unsafe-review closure requires every unsafe site to have a local contract, local guard, witness, owner, and current review"
            },
            {
                "name": "unreviewed_unsafe_gaps",
                "status": if unreviewed_unsafe_gap_count == 0 { "pass" } else { "fail" },
                "satisfied": unreviewed_unsafe_gap_count == 0,
                "details": "unsafe sites without a matching active ledger entry are not treated as reviewed"
            },
            {
                "name": "miri",
                "status": miri_status,
                "satisfied": miri_status == "pass" || miri_status == "not_applicable",
                "details": "Miri is evidence about one execution model; skipped or missing Miri evidence is not an unsafe-review pass and does not prove or disprove soundness"
            },
            {
                "name": "hardware_execution",
                "status": "not_applicable",
                "satisfied": true,
                "details": "this unsafe-review receipt does not run hardware, open HID or serial, launch vendor tools, or execute motion paths"
            }
        ],
        "next_pr_queue": [
            "changed unsafe seams",
            "FFI / raw pointer / transmute-like seams",
            "shared-memory / concurrency / RT boundaries",
            "HID/USB/driver-facing unsafe boundaries",
            "generated or platform-specific unsafe surfaces",
            "Miri/property/fake-transport witnesses"
        ],
        "claim_boundary": [
            "unsafe-reviewability-only",
            "not-unsafe-rust-soundness",
            "not-ub-free",
            "not-miri-clean-unless-miri-passed",
            "not-hardware-validation"
        ]
    }))
}

fn read_unsafe_review_exception_ledger(
    workspace_root: &Path,
) -> anyhow::Result<UnsafeReviewExceptionLedger> {
    let path = workspace_root.join("policy/unsafe-review-exceptions.toml");
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("invalid TOML in {}", path.display()))
}

fn validate_unsafe_review_exception_ledger(
    ledger: &UnsafeReviewExceptionLedger,
) -> anyhow::Result<()> {
    if ledger.schema_version != "1.0" {
        bail!("unsafe-review exception ledger schema_version must be 1.0");
    }

    for entry in &ledger.exception {
        validate_required_text(&entry.id, "id")?;
        validate_required_text(&entry.owner, "owner")?;
        validate_required_text(&entry.path, "path")?;
        validate_required_text(&entry.kind, "kind")?;
        validate_required_text(&entry.reason, "reason")?;
        validate_required_text(&entry.review_after, "review_after")?;
        validate_required_text(&entry.removal_condition, "removal_condition")?;
        validate_evidence_value(&entry.safety_contract, &entry.id, "safety_contract")?;
        validate_evidence_value(&entry.local_guard, &entry.id, "local_guard")?;
        validate_evidence_value(&entry.witness, &entry.id, "witness")?;
        if entry.test_surface.is_empty() {
            bail!(
                "unsafe-review exception `{}` must list test_surface",
                entry.id
            );
        }
        if !is_yyyy_mm_dd(&entry.review_after) {
            bail!(
                "unsafe-review exception `{}` review_after must use YYYY-MM-DD",
                entry.id
            );
        }
        if entry.status != "active" && entry.status != "retired" {
            bail!(
                "unsafe-review exception `{}` status must be active or retired",
                entry.id
            );
        }
    }
    Ok(())
}

fn validate_evidence_value(value: &str, id: &str, field: &str) -> anyhow::Result<()> {
    match value {
        "present" | "missing" | "not_applicable" => Ok(()),
        _ => bail!(
            "unsafe-review exception `{id}` field `{field}` must be present, missing, or not_applicable"
        ),
    }
}

fn validate_status_value(value: &str, field: &str) -> anyhow::Result<()> {
    match value {
        "pass" | "fail" | "advisory" | "skipped" | "not_applicable" => Ok(()),
        _ => bail!("{field} must be pass, fail, advisory, skipped, or not_applicable"),
    }
}

fn is_yyyy_mm_dd(value: &str) -> bool {
    value.len() == 10
        && value.chars().enumerate().all(|(index, value)| {
            matches!(index, 4 | 7)
                .then_some(value == '-')
                .unwrap_or_else(|| value.is_ascii_digit())
        })
}

fn discover_unsafe_sites(workspace_root: &Path) -> anyhow::Result<Vec<UnsafeSite>> {
    let rust_files = git_lines(workspace_root, &["ls-files", "--", "*.rs"])?;
    let mut sites = Vec::new();
    for file in rust_files {
        let path = normalize_repo_path(&file);
        let source_path = workspace_root.join(&path);
        let source = fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        sites.extend(unsafe_keyword_sites_in_source(&path, &source));
    }
    Ok(sites)
}

fn discover_changed_rust_paths(workspace_root: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for args in [
        &[
            "diff",
            "--name-only",
            "--diff-filter=ACMRT",
            "origin/main...HEAD",
            "--",
            "*.rs",
        ][..],
        &["diff", "--name-only", "--diff-filter=ACMRT", "--", "*.rs"][..],
    ] {
        if let Ok(lines) = git_lines(workspace_root, args) {
            paths.extend(lines.into_iter().map(|path| normalize_repo_path(&path)));
        }
    }
    paths
}

fn git_lines(workspace_root: &Path, args: &[&str]) -> anyhow::Result<Vec<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(normalize_repo_path)
        .collect())
}

fn unsafe_keyword_sites_in_source(path: &str, source: &str) -> Vec<UnsafeSite> {
    let bytes = source.as_bytes();
    let mut sites = Vec::new();
    let mut index = 0_usize;
    let mut line = 1_usize;
    let mut block_comment_depth = 0_usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            line += 1;
            index += 1;
            continue;
        }

        if block_comment_depth > 0 {
            if starts_with(bytes, index, b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if starts_with(bytes, index, b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if starts_with(bytes, index, b"//") {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if starts_with(bytes, index, b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        if let Some(next_index) = skip_raw_string(bytes, index) {
            line += count_newlines(&bytes[index..next_index]);
            index = next_index;
            continue;
        }
        if byte == b'"' || starts_with(bytes, index, b"b\"") {
            let start = if byte == b'"' { index } else { index + 1 };
            let next_index = skip_quoted_string(bytes, start);
            line += count_newlines(&bytes[index..next_index]);
            index = next_index;
            continue;
        }
        if is_ident_start(byte) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_ident_continue(bytes[index]) {
                index += 1;
            }
            if is_unsafe_keyword(source, start, index) {
                sites.push(UnsafeSite {
                    path: path.to_string(),
                    line,
                });
            }
            continue;
        }
        index += 1;
    }

    sites
}

fn starts_with(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes
        .get(index..)
        .is_some_and(|remaining| remaining.starts_with(needle))
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_quoted_string(bytes: &[u8], mut index: usize) -> usize {
    index += 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = index.saturating_add(2),
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    index
}

fn skip_raw_string(bytes: &[u8], index: usize) -> Option<usize> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hash_start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let hash_count = cursor - hash_start;
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes
                .get(cursor + 1..cursor + 1 + hash_count)
                .is_some_and(|hashes| hashes.iter().all(|value| *value == b'#'))
        {
            return Some(cursor + 1 + hash_count);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|value| **value == b'\n').count()
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn is_unsafe_keyword(source: &str, start: usize, end: usize) -> bool {
    const UNSAFE_KEYWORD_BYTES: [u8; 6] = [117, 110, 115, 97, 102, 101];
    source.as_bytes().get(start..end) == Some(UNSAFE_KEYWORD_BYTES.as_slice())
}

fn matching_unsafe_exception<'a>(
    site: &UnsafeSite,
    active_exceptions: &[&'a UnsafeReviewException],
) -> Option<&'a UnsafeReviewException> {
    active_exceptions
        .iter()
        .copied()
        .find(|entry| repo_path_matches(&entry.path, &site.path))
}

fn repo_path_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize_repo_path(pattern);
    let path = normalize_repo_path(path);
    if pattern == path {
        return true;
    }
    if !pattern.contains('*') {
        let directory_prefix = format!("{}/", pattern.trim_end_matches('/'));
        return path.starts_with(&directory_prefix);
    }
    glob_segments_match(
        &pattern.split('/').collect::<Vec<_>>(),
        &path.split('/').collect::<Vec<_>>(),
    )
}

fn glob_segments_match(pattern: &[&str], path: &[&str]) -> bool {
    let Some((head, tail)) = pattern.split_first() else {
        return path.is_empty();
    };
    if *head == "**" {
        return tail.is_empty()
            || (0..=path.len()).any(|index| glob_segments_match(tail, &path[index..]));
    }
    let Some((path_head, path_tail)) = path.split_first() else {
        return false;
    };
    segment_wildcard_match(head, path_head) && glob_segments_match(tail, path_tail)
}

fn segment_wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut p = 0_usize;
    let mut v = 0_usize;
    let mut star: Option<usize> = None;
    let mut star_value_index = 0_usize;

    while v < value.len() {
        if p < pattern.len() && pattern[p] == value[v] {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            star_value_index = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            star_value_index += 1;
            v = star_value_index;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn normalize_repo_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn detect_miri_status(workspace_root: &Path, unsafe_site_count: usize) -> anyhow::Result<String> {
    if unsafe_site_count == 0 {
        return Ok("not_applicable".to_string());
    }
    if let Ok(value) = env::var("OPENRACING_MIRI_STATUS") {
        validate_status_value(&value, "OPENRACING_MIRI_STATUS")?;
        return Ok(value);
    }
    let tracked = git_lines(workspace_root, &["ls-files"])?;
    let has_miri_surface = tracked.iter().any(|path| {
        path.ends_with(".yml")
            || path.ends_with(".yaml")
            || path.ends_with(".toml")
            || path.ends_with(".md")
    }) && tracked
        .iter()
        .filter(|path| {
            path.starts_with(".github/")
                || path.starts_with("scripts/")
                || path.as_str() == "Cargo.toml"
                || path.starts_with("docs/")
        })
        .filter_map(|path| fs::read_to_string(workspace_root.join(path)).ok())
        .any(|content| content.contains("cargo miri") || content.contains("MIRIFLAGS"));
    Ok(if has_miri_surface {
        "advisory"
    } else {
        "skipped"
    }
    .to_string())
}

fn current_utc_date_string() -> anyhow::Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .with_context(|| "system clock is before UNIX_EPOCH")?;
    let days_since_epoch = (now.as_secs() / 86_400) as i64;
    let (year, month, day) = civil_from_days(days_since_epoch);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}

fn write_unsafe_review_closure_markdown(
    path: &Path,
    receipt: &serde_json::Value,
) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        bail!(
            "unsafe-review closure markdown path has no parent: {}",
            path.display()
        );
    };
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let status = receipt
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut content = String::new();
    content.push_str("# Unsafe Review Closure Receipt\n\n");
    content.push_str(&format!("Status: `{status}`\n\n"));
    content.push_str("| Field | Value |\n| --- | ---: |\n");
    for field in [
        "unsafe_site_count",
        "changed_unsafe_site_count",
        "unsafe_contract_missing_count",
        "local_guard_missing_count",
        "witness_missing_count",
        "owner_missing_count",
        "expired_review_count",
        "unreviewed_unsafe_gap_count",
        "unsafe_review_closure_satisfied",
        "miri_status",
        "exception_count",
    ] {
        let value = receipt
            .get(field)
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| "null".to_string());
        content.push_str(&format!("| `{field}` | `{value}` |\n"));
    }
    append_result_states_markdown(&mut content, receipt);
    append_unsafe_exception_breakdown_markdown(&mut content, receipt);
    content.push_str(
        "\nUnsafe-review closure makes unsafe seams reviewable; it does not prove soundness, UB-freedom, or Miri-clean status.\n",
    );
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn append_unsafe_exception_breakdown_markdown(content: &mut String, receipt: &serde_json::Value) {
    let Some(rows) = receipt
        .get("unsafe_exception_breakdown")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    content.push_str("\n## Unsafe Exception Breakdown\n\n");
    content.push_str(
        "| Exception | Owner | Path | Sites | Changed | Missing Contract | Missing Guard | Missing Witness | Missing Evidence |\n",
    );
    content.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in rows {
        let id = markdown_cell(
            row.get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let owner = markdown_cell(
            row.get("owner")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let path = markdown_cell(
            row.get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        let site_count = json_u64_field(row, "unsafe_site_count");
        let changed_count = json_u64_field(row, "changed_unsafe_site_count");
        let contract_missing = json_u64_field(row, "unsafe_contract_missing_count");
        let guard_missing = json_u64_field(row, "local_guard_missing_count");
        let witness_missing = json_u64_field(row, "witness_missing_count");
        let missing_evidence = json_u64_field(row, "missing_evidence_count");
        content.push_str(&format!(
            "| `{id}` | `{owner}` | `{path}` | `{site_count}` | `{changed_count}` | `{contract_missing}` | `{guard_missing}` | `{witness_missing}` | `{missing_evidence}` |\n"
        ));
    }
}

fn json_u64_field(value: &serde_json::Value, field: &str) -> u64 {
    value
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
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
    fn test_efficiency_classifier_covers_conservative_fixture_shapes() {
        let exact = "fn test() {\n let result = service(1);\n assert_eq!(result, \"ok\");\n}";
        let broad = "fn test() {\n let result = service(1);\n assert!(result.is_ok());\n}";
        let smoke = "fn test() {\n let result = service(1);\n assert!(result.status.success());\n}";
        let vacuous = "fn test() {\n let _result = service(1);\n}";
        let opaque = "fn test() {\n assert_eq!(1, 1);\n}";
        let circular =
            "fn test() {\n let expected = service(1);\n assert_eq!(expected, service(1));\n}";

        for (body, expected) in [
            (exact, "strong_discriminator"),
            (broad, "useful_but_broad"),
            (smoke, "smoke_only"),
            (vacuous, "likely_vacuous"),
            (opaque, "opaque"),
            (circular, "possibly_circular"),
        ] {
            let owners = test_efficiency_reached_owners(body);
            let observations = test_efficiency_observations(body, 1);
            let (actual, _) = test_efficiency_class_and_reasons(body, &owners, &observations);
            assert_eq!(actual, expected, "fixture body: {body}");
        }
    }

    #[test]
    fn test_efficiency_observations_preserve_absolute_lines() {
        let observations =
            test_efficiency_observations("assert_eq!(value, \"ok\");\nassert!(ready);", 41);
        assert_eq!(
            observations
                .iter()
                .map(|observation| observation.line)
                .collect::<Vec<_>>(),
            vec![41]
        );
    }

    #[test]
    fn scanner_ignores_comments_strings_and_multiline_attributes() -> anyhow::Result<()> {
        let source = r###"
#[test]
// fn obsolete_name() { }
fn real_name() {
    let json = r#"{"not_a_body": true}"#;
    /* a comment with { braces } */
    assert_eq!(json, r#"{"not_a_body": true}"#);
}

#[test]
#[cfg_attr(
    feature = "platform-test",
)]
async fn gated_name() {
    assert_eq!(1, 1);
}
"###;
        let tests = test_efficiency_tests_in_text(Path::new("."), Path::new("fixture.rs"), source);
        let names = tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>();
        if names != ["real_name", "gated_name"] {
            anyhow::bail!("unexpected test names: {names:?}");
        }
        Ok(())
    }

    #[test]
    fn test_efficiency_report_contract_is_valid_without_tests() -> anyhow::Result<()> {
        let classified = classify_test_efficiency_tests(&[]);
        let report = test_efficiency_report_value(&classified);
        assert_eq!(
            report
                .get("schema_version")
                .and_then(|value| value.as_str()),
            Some("0.1")
        );
        assert_eq!(
            report.get("advisory").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            report
                .get("tests")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            report
                .get("metrics")
                .and_then(|value| value.get("tests_scanned"))
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        for class in [
            "strong_discriminator",
            "useful_but_broad",
            "smoke_only",
            "likely_vacuous",
            "possibly_circular",
            "duplicative",
            "opaque",
        ] {
            assert_eq!(
                report
                    .get("metrics")
                    .and_then(|value| value.get("class_counts"))
                    .and_then(|value| value.get(class))
                    .and_then(|value| value.as_u64()),
                Some(0),
                "missing class count {class}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_efficiency_report_rejects_malformed_contract() {
        let malformed = serde_json::json!({
            "schema_version": "0.1",
            "tests": [],
            "metrics": {"tests_scanned": 1}
        });
        assert!(validate_test_efficiency_report(&malformed).is_err());
    }

    #[test]
    fn test_efficiency_report_rejects_wrong_schema_version() {
        let wrong = serde_json::json!({
            "schema_version": "0.2",
            "tests": [],
            "metrics": {"tests_scanned": 0}
        });
        assert!(validate_test_efficiency_report(&wrong).is_err());
    }

    #[test]
    fn test_efficiency_report_rejects_unknown_class() {
        let unknown = serde_json::json!({
            "schema_version": "0.1",
            "tests": [{"class": "not_a_class"}],
            "metrics": {"tests_scanned": 1}
        });
        assert!(validate_test_efficiency_report(&unknown).is_err());
    }

    #[test]
    fn test_efficiency_report_creates_nested_outputs_and_handles_missing_root() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let missing_root = temp.path().join("does-not-exist");
        assert!(collect_test_efficiency_tests(&missing_root)?.is_empty());

        let report_path = temp.path().join("nested/reports/test-efficiency.json");
        let markdown_path = temp.path().join("nested/reports/test-efficiency.md");
        let classified = classify_test_efficiency_tests(&[]);
        let report = test_efficiency_report_value(&classified);
        write_test_efficiency_report_files(&report_path, &markdown_path, &report, "advisory\n")?;
        validate_json_file(&report_path)?;
        ensure_non_empty_file(&markdown_path)?;
        Ok(())
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
    fn parses_test_efficiency_report() -> anyhow::Result<()> {
        let command = parse_args(["test-efficiency-report".to_string()].into_iter())?;
        assert_eq!(command, CommandKind::TestEfficiencyReport);
        Ok(())
    }

    #[test]
    fn rejects_arguments_for_test_efficiency_report() {
        let result =
            parse_args(["test-efficiency-report".to_string(), "--check".to_string()].into_iter());
        assert!(result.is_err());
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
        let ledger = QualityExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![QualityException {
                id: "coverage-pr-label-gated".to_string(),
                owner: "release/ci".to_string(),
                path: ".github/workflows/coverage.yml".to_string(),
                kind: "coverage_gate_debt".to_string(),
                reason: "PR coverage is label-gated while the closure lane is measured."
                    .to_string(),
                test_surface: vec!["cargo xtask quality-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition:
                    "Make patch coverage required or add a required non-skipped coverage sentinel."
                        .to_string(),
                status: "active".to_string(),
            }],
        };
        let coverage_workflow = "name: Code Coverage\njobs:\n  coverage:\n    if: >-\n      github.event_name == 'push' ||\n      github.event_name == 'workflow_dispatch' ||\n      contains(github.event.pull_request.labels.*.name, 'coverage')\n";
        let ci_workflow = "jobs:\n  coverage:\n    name: Code Coverage\n    if: github.event_name == 'workflow_dispatch'\n";
        let codecov =
            "coverage:\n  status:\n    patch:\n      default:\n        informational: true\n";

        let receipt = build_quality_closure_receipt_value(
            &ledger,
            0,
            coverage_workflow,
            ci_workflow,
            codecov,
            "skipped",
            "skipped",
            "2026-06-05",
        )?;
        assert_eq!(receipt["ripr_unresolved_gap_count"], 0);
        assert_eq!(receipt["coverage_required"], false);
        assert_eq!(receipt["coverage_workflow_skipped"], true);
        assert_eq!(receipt["coverage_tool_status"], "skipped");
        assert_eq!(receipt["badge_endpoint_status"], "skipped");
        assert_eq!(receipt["patch_coverage_status"], "advisory");
        assert_eq!(receipt["expired_review_count"], 0);
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["id"],
            "coverage-pr-label-gated"
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["receipt_status"],
            "skipped"
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["follow_up_required"],
            true
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["review_expired"],
            false
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["test_surface_count"],
            1
        );
        assert_eq!(
            receipt["result_states"][1]["name"],
            serde_json::Value::String("badge_endpoint_regeneration".to_string())
        );
        assert_eq!(
            receipt["result_states"][1]["status"],
            serde_json::Value::String("skipped".to_string())
        );
        assert_eq!(
            receipt["result_states"][3]["status"],
            serde_json::Value::String("skipped".to_string())
        );
        assert_eq!(
            receipt["result_states"][4]["name"],
            serde_json::Value::String("coverage_tooling".to_string())
        );
        assert_eq!(
            receipt["result_states"][4]["status"],
            serde_json::Value::String("skipped".to_string())
        );
        assert_eq!(receipt["quality_closure_satisfied"], false);
        Ok(())
    }

    #[test]
    fn quality_closure_counts_expired_exception_reviews() -> anyhow::Result<()> {
        let ledger = QualityExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![QualityException {
                id: "coverage-generated-protobuf".to_string(),
                owner: "schemas".to_string(),
                path: "**/*.pb.rs".to_string(),
                kind: "generated".to_string(),
                reason: "Generated protobuf output is excluded from human-authored coverage."
                    .to_string(),
                test_surface: vec!["cargo xtask quality-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "Generated protobuf coverage is accounted elsewhere."
                    .to_string(),
                status: "active".to_string(),
            }],
        };
        let codecov =
            "coverage:\n  status:\n    patch:\n      default:\n        informational: false\n";

        let receipt = build_quality_closure_receipt_value(
            &ledger,
            0,
            "",
            "",
            codecov,
            "pass",
            "pass",
            "2026-07-01",
        )?;

        assert_eq!(receipt["expired_review_count"], 1);
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["receipt_status"],
            "fail"
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["review_expired"],
            true
        );
        assert_eq!(
            receipt["quality_exception_breakdown"][0]["follow_up_required"],
            true
        );
        assert_eq!(receipt["quality_closure_satisfied"], false);
        assert_eq!(
            receipt["result_states"][6]["name"],
            serde_json::Value::String("quality_exception_reviews".to_string())
        );
        assert_eq!(
            receipt["result_states"][6]["status"],
            serde_json::Value::String("fail".to_string())
        );
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

    #[test]
    fn parses_unsafe_review_closure_outputs() -> anyhow::Result<()> {
        let command = parse_args(
            [
                "unsafe-review-closure".to_string(),
                "--check".to_string(),
                "--json-out".to_string(),
                "target/unsafe.json".to_string(),
                "--md-out".to_string(),
                "target/unsafe.md".to_string(),
            ]
            .into_iter(),
        )?;

        match command {
            CommandKind::UnsafeReviewClosure {
                check,
                json_out,
                md_out,
            } => {
                assert!(check);
                assert_eq!(json_out, PathBuf::from("target/unsafe.json"));
                assert_eq!(md_out, PathBuf::from("target/unsafe.md"));
            }
            other => bail!("expected unsafe-review closure command, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn quality_closure_markdown_surfaces_result_states() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("quality.md");
        let receipt = serde_json::json!({
            "status": "advisory",
            "ripr_unresolved_gap_count": 0,
            "ripr_plus_unowned_gap_count": 0,
            "coverage_required": false,
            "coverage_workflow_skipped": true,
            "coverage_tool_status": "skipped",
            "patch_coverage_status": "advisory",
            "badge_endpoint_status": "skipped",
            "uncovered_owned_surface_count": 0,
            "expired_review_count": 0,
            "exception_count": 12,
            "quality_exception_breakdown": [
                {
                    "id": "coverage-pr-label-gated",
                    "owner": "release/ci",
                    "path": ".github/workflows/coverage.yml",
                    "kind": "coverage_gate_debt",
                    "review_after": "2026-06-30",
                    "review_expired": false,
                    "receipt_status": "skipped",
                    "follow_up_required": true,
                    "test_surface_count": 2,
                    "removal_condition": "Patch coverage is required on PRs."
                }
            ],
            "result_states": [
                {
                    "name": "badge_endpoint_regeneration",
                    "status": "skipped",
                    "satisfied": false,
                    "details": "missing test-efficiency report"
                },
                {
                    "name": "coverage_workflow_execution",
                    "status": "skipped",
                    "satisfied": false,
                    "details": "skipped coverage is not a pass"
                },
                {
                    "name": "coverage_tooling",
                    "status": "skipped",
                    "satisfied": false,
                    "details": "cargo-llvm-cov did not run"
                },
                {
                    "name": "patch_coverage",
                    "status": "advisory",
                    "satisfied": false,
                    "details": "patch coverage remains informational"
                },
                {
                    "name": "quality_exception_reviews",
                    "status": "pass",
                    "satisfied": true,
                    "details": "reviews are current"
                },
                {
                    "name": "mutation_expansion",
                    "status": "not_applicable",
                    "satisfied": true,
                    "details": "outside this scaffold"
                }
            ]
        });

        write_quality_closure_markdown(&path, &receipt)?;
        let content = fs::read_to_string(&path)?;

        assert!(content.contains("## Result States"));
        assert!(content.contains("| `coverage_tool_status` | `\"skipped\"` |"));
        assert!(content.contains("| `badge_endpoint_status` | `\"skipped\"` |"));
        assert!(content.contains("| `badge_endpoint_regeneration` | `skipped` | `false` |"));
        assert!(content.contains("| `coverage_workflow_execution` | `skipped` | `false` |"));
        assert!(content.contains("| `coverage_tooling` | `skipped` | `false` |"));
        assert!(content.contains("| `patch_coverage` | `advisory` | `false` |"));
        assert!(content.contains("| `quality_exception_reviews` | `pass` | `true` |"));
        assert!(content.contains("| `mutation_expansion` | `not_applicable` | `true` |"));
        assert!(content.contains("## Quality Exception Breakdown"));
        assert!(content.contains(
            "| `coverage-pr-label-gated` | `release/ci` | `coverage_gate_debt` | `skipped` | `true` | `false` | `2026-06-30` | `2` | `.github/workflows/coverage.yml` |"
        ));
        Ok(())
    }

    #[test]
    fn unsafe_review_markdown_surfaces_result_states() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("unsafe.md");
        let receipt = serde_json::json!({
            "status": "advisory",
            "unsafe_site_count": 375,
            "changed_unsafe_site_count": 0,
            "unsafe_contract_missing_count": 329,
            "local_guard_missing_count": 329,
            "witness_missing_count": 329,
            "owner_missing_count": 0,
            "expired_review_count": 0,
            "unreviewed_unsafe_gap_count": 0,
            "unsafe_review_closure_satisfied": false,
            "miri_status": "skipped",
            "exception_count": 17,
            "unsafe_exception_breakdown": [
                {
                    "id": "unsafe-native-plugin-ffi",
                    "owner": "plugins",
                    "path": "crates/openracing-native-plugin/src/**",
                    "kind": "ffi_shared_memory_or_abi_unsafe",
                    "unsafe_site_count": 12,
                    "changed_unsafe_site_count": 0,
                    "unsafe_contract_missing_count": 12,
                    "local_guard_missing_count": 12,
                    "witness_missing_count": 12,
                    "missing_evidence_count": 36
                }
            ],
            "result_states": [
                {
                    "name": "unsafe_review_evidence",
                    "status": "fail",
                    "satisfied": false,
                    "details": "missing contracts remain visible"
                },
                {
                    "name": "miri",
                    "status": "skipped",
                    "satisfied": false,
                    "details": "Miri did not run"
                },
                {
                    "name": "hardware_execution",
                    "status": "not_applicable",
                    "satisfied": true,
                    "details": "no hardware execution in this receipt"
                }
            ]
        });

        write_unsafe_review_closure_markdown(&path, &receipt)?;
        let content = fs::read_to_string(&path)?;

        assert!(content.contains("## Result States"));
        assert!(content.contains("| `unsafe_review_evidence` | `fail` | `false` |"));
        assert!(content.contains("| `miri` | `skipped` | `false` |"));
        assert!(content.contains("| `hardware_execution` | `not_applicable` | `true` |"));
        assert!(content.contains("## Unsafe Exception Breakdown"));
        assert!(content.contains(
            "| `unsafe-native-plugin-ffi` | `plugins` | `crates/openracing-native-plugin/src/**` | `12` | `0` | `12` | `12` | `12` | `36` |"
        ));
        assert!(content.contains("does not prove soundness, UB-freedom, or Miri-clean status"));
        Ok(())
    }

    #[test]
    fn unsafe_review_ledger_requires_reviewable_entries() {
        let ledger = UnsafeReviewExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![UnsafeReviewException {
                id: "unsafe-engine".to_string(),
                owner: "core".to_string(),
                path: "crates/engine/src/**".to_string(),
                kind: "rt_raw_memory".to_string(),
                reason: "needs contracts".to_string(),
                test_surface: vec!["cargo xtask unsafe-review-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "add contracts".to_string(),
                safety_contract: "missing".to_string(),
                local_guard: "missing".to_string(),
                witness: "missing".to_string(),
                status: "active".to_string(),
            }],
        };

        assert!(validate_unsafe_review_exception_ledger(&ledger).is_ok());

        let invalid = UnsafeReviewExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![UnsafeReviewException {
                id: "unsafe-engine".to_string(),
                owner: String::new(),
                path: "crates/engine/src/**".to_string(),
                kind: "rt_raw_memory".to_string(),
                reason: "needs contracts".to_string(),
                test_surface: vec!["cargo xtask unsafe-review-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "add contracts".to_string(),
                safety_contract: "missing".to_string(),
                local_guard: "missing".to_string(),
                witness: "missing".to_string(),
                status: "active".to_string(),
            }],
        };

        assert!(validate_unsafe_review_exception_ledger(&invalid).is_err());
    }

    #[test]
    fn unsafe_review_receipt_counts_missing_evidence_without_miri_claim() -> anyhow::Result<()> {
        let ledger = UnsafeReviewExceptionLedger {
            schema_version: "1.0".to_string(),
            exception: vec![UnsafeReviewException {
                id: "unsafe-engine".to_string(),
                owner: "core".to_string(),
                path: "crates/engine/src/**".to_string(),
                kind: "rt_raw_memory".to_string(),
                reason: "needs contracts".to_string(),
                test_surface: vec!["cargo xtask unsafe-review-closure --check".to_string()],
                review_after: "2026-06-30".to_string(),
                removal_condition: "add contracts".to_string(),
                safety_contract: "missing".to_string(),
                local_guard: "missing".to_string(),
                witness: "missing".to_string(),
                status: "active".to_string(),
            }],
        };
        let sites = vec![
            UnsafeSite {
                path: "crates/engine/src/protocol.rs".to_string(),
                line: 10,
            },
            UnsafeSite {
                path: "crates/new/src/lib.rs".to_string(),
                line: 5,
            },
        ];
        let changed = BTreeSet::from(["crates/new/src/lib.rs".to_string()]);
        let receipt = build_unsafe_review_closure_receipt_value(
            &ledger,
            &sites,
            &changed,
            "2026-05-31",
            "skipped",
        )?;

        assert_eq!(receipt["unsafe_site_count"], 2);
        assert_eq!(receipt["changed_unsafe_site_count"], 1);
        assert_eq!(receipt["unsafe_contract_missing_count"], 2);
        assert_eq!(receipt["local_guard_missing_count"], 2);
        assert_eq!(receipt["witness_missing_count"], 2);
        assert_eq!(receipt["unreviewed_unsafe_gap_count"], 1);
        assert_eq!(receipt["miri_status"], "skipped");
        assert_eq!(receipt["unsafe_review_closure_satisfied"], false);
        assert_eq!(
            receipt["unsafe_exception_breakdown"][0]["id"],
            "unsafe-engine"
        );
        assert_eq!(
            receipt["unsafe_exception_breakdown"][0]["unsafe_site_count"],
            1
        );
        assert_eq!(
            receipt["unsafe_exception_breakdown"][0]["changed_unsafe_site_count"],
            0
        );
        assert_eq!(
            receipt["unsafe_exception_breakdown"][0]["missing_evidence_count"],
            3
        );
        Ok(())
    }

    #[test]
    fn unsafe_scanner_ignores_comments_and_strings() {
        let source = r#"
// unsafe { ignored(); }
const TEXT: &str = "unsafe";
fn demo() {
    unsafe { call(); }
}
"#;
        let sites = unsafe_keyword_sites_in_source("crates/example/src/lib.rs", source);
        assert_eq!(
            sites,
            vec![UnsafeSite {
                path: "crates/example/src/lib.rs".to_string(),
                line: 5,
            }]
        );
    }
}
