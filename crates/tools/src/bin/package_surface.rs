use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, TargetKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
struct Args {
    check: bool,
    policy: PathBuf,
    json_out: Option<PathBuf>,
    md_out: Option<PathBuf>,
    allow_current_names: bool,
}

#[derive(Debug, Deserialize)]
struct Policy {
    schema_version: u32,
    public: PackageList,
    internal: PackageList,
    #[serde(default)]
    collapse: Vec<CollapseEntry>,
}

#[derive(Debug, Deserialize)]
struct PackageList {
    packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CollapseEntry {
    from: String,
    to: String,
    owner: String,
    reason: String,
    transition: Option<String>,
}

#[derive(Debug, Serialize)]
struct Report {
    success: bool,
    generated_at_utc: String,
    policy: String,
    public_packages: Vec<String>,
    internal_packages: Vec<String>,
    collapse_packages: Vec<CollapsePackage>,
    violations: Vec<String>,
    warnings: Vec<String>,
    workspace_members: Vec<String>,
    publishable_packages: Vec<String>,
    path_dependency_findings: Vec<PathDependencyFinding>,
}

#[derive(Debug, Serialize)]
struct CollapsePackage {
    from: String,
    to: String,
    owner: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct PathDependencyFinding {
    package: String,
    dependency: String,
    kind: String,
    requirement: String,
    path: String,
    severity: String,
    message: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "error: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<bool> {
    let args = parse_args(env::args().skip(1))?;
    let policy = load_policy(&args.policy)?;
    let metadata = MetadataCommand::new()
        .manifest_path("Cargo.toml")
        .exec()
        .context("failed to read cargo metadata")?;

    let report = build_report(&args, &policy, &metadata)?;

    if let Some(path) = &args.json_out {
        write_parented(path, &serde_json::to_string_pretty(&report)?)?;
    }
    if let Some(path) = &args.md_out {
        write_parented(path, &render_markdown(&report))?;
    }

    if args.check {
        print_check_summary(&report)?;
    }

    Ok(report.success)
}

fn parse_args<I>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut check = false;
    let mut policy = PathBuf::from("policy/crate-boundaries.toml");
    let mut json_out = None;
    let mut md_out = None;
    let mut allow_current_names = true;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--policy" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("--policy requires a path"))?;
                policy = PathBuf::from(value);
            }
            "--json-out" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("--json-out requires a path"))?;
                json_out = Some(PathBuf::from(value));
            }
            "--md-out" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("--md-out requires a path"))?;
                md_out = Some(PathBuf::from(value));
            }
            "--allow-current-names" => allow_current_names = true,
            "--strict-target-names" => allow_current_names = false,
            "--help" | "-h" => {
                let mut stdout = io::stdout().lock();
                writeln!(
                    stdout,
                    "package-surface [--check] [--policy <path>] [--json-out <path>] [--md-out <path>] [--allow-current-names]"
                )
                .context("failed to write package-surface usage")?;
                return Ok(Args {
                    check,
                    policy,
                    json_out,
                    md_out,
                    allow_current_names,
                });
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }

    Ok(Args {
        check,
        policy,
        json_out,
        md_out,
        allow_current_names,
    })
}

fn load_policy(path: &Path) -> Result<Policy> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read policy file {}", path.display()))?;
    let policy: Policy = toml::from_str(&contents)
        .with_context(|| format!("failed to parse policy file {}", path.display()))?;
    if policy.schema_version != 1 {
        bail!(
            "unsupported crate-boundaries schema_version: {}",
            policy.schema_version
        );
    }
    Ok(policy)
}

fn build_report(args: &Args, policy: &Policy, metadata: &Metadata) -> Result<Report> {
    let public = set_from(&policy.public.packages);
    let internal = set_from(&policy.internal.packages);
    let collapse = collapse_map(policy);
    let aliases = current_name_aliases();

    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut path_dependency_findings = Vec::new();
    let workspace_members = workspace_members(metadata);
    let publish_allow = publish_allowlist(metadata);

    verify_publish_allowlist(&public, &publish_allow, &mut violations);
    verify_workspace_default_members(metadata, &mut warnings);
    verify_duplicate_policy_names(&public, &internal, &collapse, &mut violations);

    let package_by_id = package_by_id(metadata);
    let publishable_packages = publishable_packages(metadata);
    let collapse_names: BTreeSet<_> = collapse.keys().cloned().collect();
    let former_microcrate_names = former_microcrate_names(&collapse_names);

    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.name))
    {
        let class = classify_package(
            &package.name,
            &public,
            &internal,
            &collapse_names,
            &aliases,
            args.allow_current_names,
        );

        if class.is_none() {
            violations.push(format!(
                "workspace package `{}` is not classified as public, internal, collapse, or temporary",
                package.name
            ));
        }

        if internal.contains(&package.name) && is_publishable(package) {
            violations.push(format!(
                "internal package `{}` must have publish = false",
                package.name
            ));
        }

        if let Some(entry) = collapse.get(&package.name)
            && is_publishable(package)
            && entry.transition.is_none()
        {
            violations.push(format!(
                "collapse package `{}` is publishable and lacks a transitional note",
                package.name
            ));
        }

        if is_publishable(package) && !publish_allow.contains(&package.name) {
            warnings.push(format!(
                "package `{}` has publish=true/default but is not in [workspace.metadata.publish].allow",
                package.name
            ));
        }

        check_feature_surface(
            package,
            &former_microcrate_names,
            &mut violations,
            &mut warnings,
        );

        if is_target_public_package(&package.name, &public, &aliases, args.allow_current_names) {
            check_path_dependencies(
                package,
                &internal,
                &package_by_id,
                args.allow_current_names,
                &mut violations,
                &mut warnings,
                &mut path_dependency_findings,
            );
        }
    }

    let collapse_packages = policy
        .collapse
        .iter()
        .map(|entry| CollapsePackage {
            from: entry.from.clone(),
            to: entry.to.clone(),
            owner: entry.owner.clone(),
            reason: entry.reason.clone(),
        })
        .collect();

    Ok(Report {
        success: violations.is_empty(),
        generated_at_utc: generated_at_utc(),
        policy: args.policy.display().to_string(),
        public_packages: sorted_vec(public),
        internal_packages: sorted_vec(internal),
        collapse_packages,
        violations,
        warnings,
        workspace_members: sorted_vec(workspace_members),
        publishable_packages,
        path_dependency_findings,
    })
}

fn verify_publish_allowlist(
    public: &BTreeSet<String>,
    publish_allow: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    for package in publish_allow.difference(public) {
        violations.push(format!(
            "publish allowlist package `{package}` is missing from policy public packages"
        ));
    }
    for package in public.difference(publish_allow) {
        violations.push(format!(
            "policy public package `{package}` is missing from [workspace.metadata.publish].allow"
        ));
    }
}

fn verify_workspace_default_members(metadata: &Metadata, warnings: &mut Vec<String>) {
    if metadata.workspace_default_members.is_empty() {
        warnings.push("workspace.default-members is missing".to_string());
    }
}

fn verify_duplicate_policy_names(
    public: &BTreeSet<String>,
    internal: &BTreeSet<String>,
    collapse: &BTreeMap<String, &CollapseEntry>,
    violations: &mut Vec<String>,
) {
    for package in public.intersection(internal) {
        violations.push(format!(
            "package `{package}` appears in both public and internal policy lists"
        ));
    }
    for package in public {
        if collapse.contains_key(package) {
            violations.push(format!(
                "package `{package}` appears as both public and collapse policy"
            ));
        }
    }
    for package in internal {
        if collapse.contains_key(package) {
            violations.push(format!(
                "package `{package}` appears as both internal and collapse policy"
            ));
        }
    }
}

fn check_feature_surface(
    package: &Package,
    former_microcrate_names: &BTreeSet<String>,
    violations: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    if package.features.len() > 12 {
        warnings.push(format!(
            "package `{}` exposes {} features; public packages should stay below 12 where possible",
            package.name,
            package.features.len()
        ));
    }

    for feature_name in package.features.keys() {
        if former_microcrate_names.contains(feature_name) {
            let message = format!(
                "package `{}` has feature `{feature_name}` matching a former microcrate name",
                package.name
            );
            violations.push(message.clone());
            warnings.push(message);
        }
    }
}

fn check_path_dependencies(
    package: &Package,
    internal: &BTreeSet<String>,
    package_by_id: &BTreeMap<String, &Package>,
    allow_current_names: bool,
    violations: &mut Vec<String>,
    warnings: &mut Vec<String>,
    findings: &mut Vec<PathDependencyFinding>,
) {
    let is_library = package.targets.iter().any(|target| {
        target.kind.iter().any(|kind| {
            matches!(
                kind,
                TargetKind::Lib
                    | TargetKind::RLib
                    | TargetKind::DyLib
                    | TargetKind::CDyLib
                    | TargetKind::StaticLib
                    | TargetKind::ProcMacro
            )
        })
    });

    for dependency in &package.dependencies {
        if matches!(dependency.kind, DependencyKind::Development) {
            continue;
        }

        let Some(path) = &dependency.path else {
            continue;
        };

        let kind = dependency_kind_name(dependency.kind);
        let path_only = dependency.req.to_string() == "*";
        if path_only {
            let message = format!(
                "public package `{}` has path-only {kind} dependency `{}`",
                package.name, dependency.name
            );
            let severity = transitional_severity(allow_current_names);
            if allow_current_names {
                warnings.push(message.clone());
            } else {
                violations.push(message.clone());
            }
            findings.push(PathDependencyFinding {
                package: package.name.clone(),
                dependency: dependency.name.clone(),
                kind: kind.to_string(),
                requirement: dependency.req.to_string(),
                path: path.to_string(),
                severity: severity.to_string(),
                message,
            });
        }

        if is_library
            && dependency.name != "workspace-hack"
            && let Some(dep_package) = package_by_id.values().find(|candidate| {
                candidate.name == dependency.name
                    || candidate.name.replace('-', "_") == dependency.name
            })
            && (internal.contains(&dep_package.name)
                || looks_internal_dependency(&dep_package.name))
        {
            let message = format!(
                "public library package `{}` depends on internal/tool/test package `{}`",
                package.name, dep_package.name
            );
            if allow_current_names {
                warnings.push(message);
            } else {
                violations.push(message);
            }
        }
    }
}

fn classify_package(
    name: &str,
    public: &BTreeSet<String>,
    internal: &BTreeSet<String>,
    collapse: &BTreeSet<String>,
    aliases: &BTreeMap<String, String>,
    allow_current_names: bool,
) -> Option<&'static str> {
    if public.contains(name) {
        Some("public")
    } else if internal.contains(name) {
        Some("internal")
    } else if collapse.contains(name) {
        Some("collapse")
    } else if allow_current_names && aliases.contains_key(name) {
        Some("temporary")
    } else {
        None
    }
}

fn is_target_public_package(
    name: &str,
    public: &BTreeSet<String>,
    aliases: &BTreeMap<String, String>,
    allow_current_names: bool,
) -> bool {
    public.contains(name) || (allow_current_names && aliases.contains_key(name))
}

fn looks_internal_dependency(name: &str) -> bool {
    name.contains("tools")
        || name.contains("test")
        || name.contains("integration")
        || name.contains("compat")
        || name == "workspace-hack"
}

fn transitional_severity(allow_current_names: bool) -> &'static str {
    if allow_current_names {
        "warning"
    } else {
        "violation"
    }
}

fn publish_allowlist(metadata: &Metadata) -> BTreeSet<String> {
    metadata
        .workspace_metadata
        .get("publish")
        .and_then(|publish| publish.get("allow"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn workspace_members(metadata: &Metadata) -> BTreeSet<String> {
    let ids: BTreeSet<_> = metadata
        .workspace_members
        .iter()
        .map(ToString::to_string)
        .collect();
    metadata
        .packages
        .iter()
        .filter(|package| ids.contains(&package.id.to_string()))
        .map(|package| package.name.clone())
        .collect()
}

fn package_by_id(metadata: &Metadata) -> BTreeMap<String, &Package> {
    metadata
        .packages
        .iter()
        .map(|package| (package.id.to_string(), package))
        .collect()
}

fn publishable_packages(metadata: &Metadata) -> Vec<String> {
    let members = workspace_members(metadata);
    let mut packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| members.contains(&package.name) && is_publishable(package))
        .map(|package| package.name.clone())
        .collect();
    packages.sort();
    packages
}

fn is_publishable(package: &Package) -> bool {
    match &package.publish {
        None => true,
        Some(registries) => !registries.is_empty(),
    }
}

fn dependency_kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Normal => "normal",
        DependencyKind::Development => "dev",
        DependencyKind::Build => "build",
        _ => "unknown",
    }
}

fn set_from(items: &[String]) -> BTreeSet<String> {
    items.iter().cloned().collect()
}

fn sorted_vec(items: BTreeSet<String>) -> Vec<String> {
    items.into_iter().collect()
}

fn collapse_map(policy: &Policy) -> BTreeMap<String, &CollapseEntry> {
    policy
        .collapse
        .iter()
        .map(|entry| (entry.from.clone(), entry))
        .collect()
}

fn former_microcrate_names(collapse_names: &BTreeSet<String>) -> BTreeSet<String> {
    collapse_names
        .iter()
        .flat_map(|name| {
            let mut names = vec![name.clone()];
            if let Some(stripped) = name.strip_prefix("racing-wheel-") {
                names.push(stripped.to_string());
            }
            if let Some(stripped) = name.strip_prefix("openracing-") {
                names.push(stripped.to_string());
            }
            names
        })
        .collect()
}

fn current_name_aliases() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "racing-wheel-engine".to_string(),
            "openracing-engine".to_string(),
        ),
        (
            "racing-wheel-service".to_string(),
            "openracing-service".to_string(),
        ),
        (
            "openracing-pidff-common".to_string(),
            "openracing-pidff".to_string(),
        ),
    ])
}

fn generated_at_utc() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}Z", duration.as_secs()),
        Err(_) => "0Z".to_string(),
    }
}

fn write_parented(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn render_markdown(report: &Report) -> String {
    let mut output = String::new();
    output.push_str("# Package Surface Report\n\n");
    output.push_str(&format!("- Success: `{}`\n", report.success));
    output.push_str(&format!("- Generated: `{}`\n", report.generated_at_utc));
    output.push_str(&format!("- Policy: `{}`\n\n", report.policy));

    output.push_str("## Violations\n\n");
    push_list(&mut output, &report.violations);
    output.push_str("\n## Warnings\n\n");
    push_list(&mut output, &report.warnings);
    output.push_str("\n## Public Packages\n\n");
    push_list(&mut output, &report.public_packages);
    output.push_str("\n## Internal Packages\n\n");
    push_list(&mut output, &report.internal_packages);
    output.push_str("\n## Collapse Packages\n\n");
    for entry in &report.collapse_packages {
        output.push_str(&format!(
            "- `{}` -> `{}` (owner `{}`): {}\n",
            entry.from, entry.to, entry.owner, entry.reason
        ));
    }
    output
}

fn push_list(output: &mut String, items: &[String]) {
    if items.is_empty() {
        output.push_str("- None\n");
    } else {
        for item in items {
            output.push_str(&format!("- {item}\n"));
        }
    }
}

fn print_check_summary(report: &Report) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if report.success {
        writeln!(
            stdout,
            "package surface check passed: {} workspace packages classified, {} warnings",
            report.workspace_members.len(),
            report.warnings.len()
        )?;
    } else {
        writeln!(
            stdout,
            "package surface check failed: {} violations, {} warnings",
            report.violations.len(),
            report.warnings.len()
        )?;
    }

    for violation in &report.violations {
        writeln!(stdout, "violation: {violation}")?;
    }
    for warning in &report.warnings {
        writeln!(stdout, "warning: {warning}")?;
    }
    Ok(())
}
