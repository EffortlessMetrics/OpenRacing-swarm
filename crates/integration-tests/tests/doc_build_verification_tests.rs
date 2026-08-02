//! Documentation build and accuracy verification tests.
//!
//! Validates that:
//! 1. All rustdoc examples compile (via `cargo test --doc`)
//! 2. README code examples reference valid commands
//! 3. ADR format matches the template structure
//! 4. Cross-references between docs point to existing files
//! 5. CLI help text matches documented behaviour
//! 6. All public crates have crate-level documentation
//! 7. Doc attribute consistency (`///` vs `#[doc = ...]`)

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the repository root (two levels up from the integration-tests crate).
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| manifest.clone())
}

/// Read a file relative to the repo root.
fn read_repo_file(rel_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = repo_root().join(rel_path);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(content)
}

/// Collect all `.md` files under a directory (recursively).
fn collect_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_md_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }
    files
}

/// Extract relative markdown links from content (e.g. `[text](path.md)`).
/// Ignores URLs (http/https), anchors-only links, and badge images.
fn extract_md_links(content: &str) -> Vec<String> {
    let Ok(link_re) = regex::Regex::new(r"\[([^\]]*)\]\(([^)]+)\)") else {
        return vec![];
    };
    link_re
        .captures_iter(content)
        .filter_map(|cap| {
            let target = cap.get(2).map(|m| m.as_str().to_string())?;
            // Skip absolute URLs, anchor-only links, and mailto
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
                || target.starts_with("mailto:")
            {
                return None;
            }
            // Strip optional anchor fragment
            let path_part = target.split('#').next()?.to_string();
            if path_part.is_empty() {
                return None;
            }
            Some(path_part)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Rustdoc examples compile (lightweight check — verifies doc comments exist)
// ---------------------------------------------------------------------------

/// Verify that key crate `lib.rs` files contain `//!` crate-level doc comments,
/// which is a prerequisite for rustdoc examples to be discoverable.
#[test]
fn doc_crate_level_docs_present_in_key_crates() -> Result<(), Box<dyn std::error::Error>> {
    let key_crates = [
        "crates/schemas/src/lib.rs",
        "crates/engine/src/lib.rs",
        "crates/service/src/lib.rs",
        "crates/plugins/src/lib.rs",
        "crates/compat/src/lib.rs",
    ];

    let mut missing = Vec::new();
    for rel_path in &key_crates {
        let content = read_repo_file(rel_path)?;
        let has_crate_doc = content
            .lines()
            .any(|line| line.trim_start().starts_with("//!"));
        if !has_crate_doc {
            missing.push(*rel_path);
        }
    }

    assert!(
        missing.is_empty(),
        "The following key crates lack crate-level doc comments (//!):\n  {}",
        missing.join("\n  ")
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. README code examples reference valid CLI commands
// ---------------------------------------------------------------------------

/// The README documents `wheelctl` subcommands — verify they match the real CLI
/// definition so users aren't pointed at non-existent commands.
#[test]
fn doc_readme_cli_commands_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let readme = read_repo_file("README.md")?;

    // Known top-level wheelctl subcommands (from clap Commands enum).
    let valid_subcommands: HashSet<&str> = [
        "device",
        "profile",
        "plugin",
        "diag",
        "game",
        "telemetry",
        "hardware",
        "safety",
        "health",
        "completion",
    ]
    .iter()
    .copied()
    .collect();

    // Extract `wheelctl <subcmd>` occurrences from fenced code blocks.
    let cmd_re = regex::Regex::new(r"wheelctl\s+(\w+)")?;
    let mut unknown = Vec::new();
    for cap in cmd_re.captures_iter(&readme) {
        if let Some(m) = cap.get(1) {
            let sub = m.as_str();
            if !valid_subcommands.contains(sub) {
                unknown.push(sub.to_string());
            }
        }
    }

    // Deduplicate
    unknown.sort();
    unknown.dedup();

    assert!(
        unknown.is_empty(),
        "README references unknown wheelctl subcommands: {:?}",
        unknown
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. ADR format validation — each ADR matches the template structure
// ---------------------------------------------------------------------------

/// Required sections in every ADR, derived from `docs/adr/template.md`.
const ADR_REQUIRED_SECTIONS: &[&str] = &[
    "## Context",
    "## Decision",
    "## Rationale",
    "## Consequences",
];

/// Required header fields in the YAML-like front matter block.
const ADR_REQUIRED_HEADERS: &[&str] = &["**Status:**", "**Date:**", "**Authors:**"];

#[test]
fn doc_adr_files_match_template_format() -> Result<(), Box<dyn std::error::Error>> {
    let adr_dir = repo_root().join("docs").join("adr");
    let adr_re = regex::Regex::new(r"^\d{4}-.+\.md$")?;
    let title_re = regex::Regex::new(r"^# ADR-\d{4}:")?;
    let status_re =
        regex::Regex::new(r"\*\*Status:\*\*\s*(Proposed|Accepted|Deprecated|Superseded)")?;
    let date_re = regex::Regex::new(r"\*\*Date:\*\*\s*\d{4}-\d{2}-\d{2}")?;

    let mut issues: Vec<String> = Vec::new();

    for entry in fs::read_dir(&adr_dir)?.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !adr_re.is_match(&name) {
            continue;
        }

        let content =
            fs::read_to_string(entry.path()).map_err(|e| format!("Failed to read {name}: {e}"))?;

        // Check required header fields
        for header in ADR_REQUIRED_HEADERS {
            if !content.contains(header) {
                issues.push(format!("{name}: missing header field {header}"));
            }
        }

        // Check required sections
        for section in ADR_REQUIRED_SECTIONS {
            if !content.contains(section) {
                issues.push(format!("{name}: missing section {section}"));
            }
        }

        // Title must start with `# ADR-NNNN:`
        let first_line = content.lines().next().unwrap_or("");
        if !title_re.is_match(first_line) {
            issues.push(format!(
                "{name}: title must match '# ADR-NNNN: ...' (got: {first_line})"
            ));
        }

        // Status must be one of the allowed lifecycle values
        if !status_re.is_match(&content) {
            issues.push(format!(
                "{name}: **Status:** must be Proposed, Accepted, Deprecated, or Superseded"
            ));
        }

        // Date must be YYYY-MM-DD
        if !date_re.is_match(&content) {
            issues.push(format!("{name}: **Date:** must be in YYYY-MM-DD format"));
        }
    }

    assert!(
        issues.is_empty(),
        "ADR format issues:\n  {}",
        issues.join("\n  ")
    );
    Ok(())
}

/// ADR numbering must be sequential without gaps.
#[test]
fn doc_adr_numbering_is_sequential() -> Result<(), Box<dyn std::error::Error>> {
    let adr_dir = repo_root().join("docs").join("adr");
    let num_re = regex::Regex::new(r"^(\d{4})-.+\.md$")?;

    let mut numbers: Vec<u32> = Vec::new();
    for entry in fs::read_dir(&adr_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = num_re.captures(&name)
            && let Some(n) = caps.get(1)
        {
            numbers.push(n.as_str().parse::<u32>()?);
        }
    }

    numbers.sort();
    if let Some(&first) = numbers.first() {
        for (i, &num) in numbers.iter().enumerate() {
            let expected = first + i as u32;
            assert_eq!(
                num, expected,
                "ADR numbering gap: expected {expected:04} but found {num:04}"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Cross-references between docs are valid
// ---------------------------------------------------------------------------

/// Verify that every relative link inside `docs/` points to an existing file.
#[test]
fn doc_cross_references_point_to_existing_files() -> Result<(), Box<dyn std::error::Error>> {
    let docs_dir = repo_root().join("docs");
    let md_files = collect_md_files(&docs_dir);

    let mut broken: Vec<String> = Vec::new();

    for md_file in &md_files {
        let content = fs::read_to_string(md_file)
            .map_err(|e| format!("Failed to read {}: {e}", md_file.display()))?;
        let links = extract_md_links(&content);

        let parent = md_file.parent().unwrap_or(&docs_dir);

        for link in &links {
            let target = parent.join(link);
            // Normalise — on Windows this resolves `..\` segments
            let canonical = if target.exists() {
                true
            } else {
                // Try with forward-slash normalisation
                let normalised = parent.join(link.replace('/', "\\"));
                normalised.exists()
            };
            if !canonical {
                broken.push(format!(
                    "{}  →  {}",
                    md_file
                        .strip_prefix(repo_root())
                        .unwrap_or(md_file)
                        .display(),
                    link
                ));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "Broken cross-references in docs/:\n  {}",
        broken.join("\n  ")
    );
    Ok(())
}

/// Verify that README.md links to existing files.
#[test]
fn doc_readme_links_point_to_existing_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let readme_path = root.join("README.md");
    let content =
        fs::read_to_string(&readme_path).map_err(|e| format!("Failed to read README.md: {e}"))?;
    let links = extract_md_links(&content);

    let mut broken: Vec<String> = Vec::new();
    for link in &links {
        let target = root.join(link);
        if !target.exists() {
            let normalised = root.join(link.replace('/', "\\"));
            if !normalised.exists() {
                broken.push(link.clone());
            }
        }
    }

    assert!(
        broken.is_empty(),
        "Broken links in README.md:\n  {}",
        broken.join("\n  ")
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. CLI help text matches documented behaviour
// ---------------------------------------------------------------------------

/// The README documents specific CLI usage patterns — verify the documented
/// subcommands exist by cross-referencing the Commands enum in the CLI crate.
#[test]
fn doc_cli_subcommand_coverage_in_readme() -> Result<(), Box<dyn std::error::Error>> {
    let readme = read_repo_file("README.md")?;

    // The README "Basic Usage" section should mention at least these core commands
    let expected_patterns = [
        "wheelctl device list",
        "wheelctl health",
        "wheelctl profile apply",
        "wheelctl device status",
        "wheelctl diag",
    ];

    let mut missing_from_readme = Vec::new();
    for pattern in &expected_patterns {
        if !readme.contains(pattern) {
            missing_from_readme.push(*pattern);
        }
    }

    assert!(
        missing_from_readme.is_empty(),
        "README is missing documentation for core CLI commands:\n  {}",
        missing_from_readme.join("\n  ")
    );
    Ok(())
}

/// Verify the CLI crate's main.rs defines every subcommand documented in the README.
#[test]
fn doc_cli_source_defines_documented_commands() -> Result<(), Box<dyn std::error::Error>> {
    let cli_main = read_repo_file("crates/cli/src/main.rs")?;

    // Top-level subcommands from the README's "Basic Usage" section
    let documented_commands = ["Device", "Profile", "Diag", "Health"];

    let mut undefined = Vec::new();
    for cmd in &documented_commands {
        // clap derive uses enum variant names like `Device(DeviceCommands)`
        if !cli_main.contains(cmd) {
            undefined.push(*cmd);
        }
    }

    assert!(
        undefined.is_empty(),
        "CLI main.rs does not define these documented command variants: {:?}",
        undefined
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. All public crates have crate-level documentation
// ---------------------------------------------------------------------------

/// Every workspace crate that publishes a lib should have `//!` doc comments
/// in its `lib.rs`.
#[test]
fn doc_all_workspace_crates_have_lib_docs() -> Result<(), Box<dyn std::error::Error>> {
    let root_cargo = read_repo_file("Cargo.toml")?;

    // Parse workspace members from the root Cargo.toml
    let member_re = regex::Regex::new(r#""(crates/[^"]+)""#)?;
    let mut missing_docs: Vec<String> = Vec::new();

    for cap in member_re.captures_iter(&root_cargo) {
        if let Some(m) = cap.get(1) {
            let crate_path = m.as_str();
            let lib_rs = repo_root().join(crate_path).join("src").join("lib.rs");
            if !lib_rs.exists() {
                // Some crates may be bin-only; skip them
                continue;
            }
            let content = fs::read_to_string(&lib_rs)
                .map_err(|e| format!("Failed to read {}: {e}", lib_rs.display()))?;

            let has_crate_doc = content
                .lines()
                .any(|line| line.trim_start().starts_with("//!"));
            if !has_crate_doc {
                missing_docs.push(crate_path.to_string());
            }
        }
    }

    assert!(
        missing_docs.is_empty(),
        "These workspace crates lack crate-level doc comments (//! ...) in lib.rs:\n  {}",
        missing_docs.join("\n  ")
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Doc attribute consistency
// ---------------------------------------------------------------------------

/// Detect crates that mix `#[doc = "..."]` attributes with `///` comments in
/// the same file.  While both are valid, mixed usage in a single file suggests
/// inconsistency and should be flagged for review.
#[test]
fn doc_attribute_style_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let root_cargo = read_repo_file("Cargo.toml")?;
    let member_re = regex::Regex::new(r#""(crates/[^"]+)""#)?;

    let doc_attr_re = regex::Regex::new(r#"^\s*#\[doc\s*="#)?;
    let triple_slash_re = regex::Regex::new(r"^\s*///")?;

    let mut inconsistent: Vec<String> = Vec::new();

    for cap in member_re.captures_iter(&root_cargo) {
        if let Some(m) = cap.get(1) {
            let crate_path = m.as_str();
            let lib_rs = repo_root().join(crate_path).join("src").join("lib.rs");
            if !lib_rs.exists() {
                continue;
            }
            let content = fs::read_to_string(&lib_rs)
                .map_err(|e| format!("Failed to read {}: {e}", lib_rs.display()))?;

            let has_doc_attr = content.lines().any(|l| doc_attr_re.is_match(l));
            let has_triple_slash = content.lines().any(|l| triple_slash_re.is_match(l));

            if has_doc_attr && has_triple_slash {
                inconsistent.push(crate_path.to_string());
            }
        }
    }

    // This is a soft check — mixing is not an error in all projects, but
    // in this codebase we prefer `///` for consistency.
    assert!(
        inconsistent.is_empty(),
        "These crates mix #[doc = \"...\"] and /// comments in lib.rs (prefer ///):\n  {}",
        inconsistent.join("\n  ")
    );
    Ok(())
}

/// Crate-level docs should use `//!` (not `#![doc = "..."]`).
#[test]
fn doc_crate_level_uses_inner_doc_comments() -> Result<(), Box<dyn std::error::Error>> {
    let root_cargo = read_repo_file("Cargo.toml")?;
    let member_re = regex::Regex::new(r#""(crates/[^"]+)""#)?;

    let crate_doc_attr_re = regex::Regex::new(r#"^\s*#!\[doc\s*="#)?;

    let mut using_attr: Vec<String> = Vec::new();

    for cap in member_re.captures_iter(&root_cargo) {
        if let Some(m) = cap.get(1) {
            let crate_path = m.as_str();
            let lib_rs = repo_root().join(crate_path).join("src").join("lib.rs");
            if !lib_rs.exists() {
                continue;
            }
            let content = fs::read_to_string(&lib_rs)
                .map_err(|e| format!("Failed to read {}: {e}", lib_rs.display()))?;

            let uses_crate_doc_attr = content.lines().any(|l| crate_doc_attr_re.is_match(l));
            if uses_crate_doc_attr {
                using_attr.push(crate_path.to_string());
            }
        }
    }

    assert!(
        using_attr.is_empty(),
        "These crates use #![doc = \"...\"] instead of //! for crate-level docs:\n  {}",
        using_attr.join("\n  ")
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// ADR INDEX consistency
// ---------------------------------------------------------------------------

/// Every numbered ADR file must appear in `docs/adr/INDEX.md`.
#[test]
fn doc_adr_index_lists_all_adr_files() -> Result<(), Box<dyn std::error::Error>> {
    let adr_dir = repo_root().join("docs").join("adr");
    let index_content = read_repo_file("docs/adr/INDEX.md")?;

    let adr_re = regex::Regex::new(r"^\d{4}-.+\.md$")?;
    let mut missing_from_index: Vec<String> = Vec::new();

    for entry in fs::read_dir(&adr_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !adr_re.is_match(&name) {
            continue;
        }
        if !index_content.contains(&name) {
            missing_from_index.push(name);
        }
    }

    assert!(
        missing_from_index.is_empty(),
        "ADR files not referenced in INDEX.md:\n  {}",
        missing_from_index.join("\n  ")
    );
    Ok(())
}

/// The `docs/README.md` should reference each ADR listed in `docs/adr/INDEX.md`.
/// This catches stale doc indexes when new ADRs are added.
#[test]
fn doc_docs_readme_references_all_adrs() -> Result<(), Box<dyn std::error::Error>> {
    let adr_dir = repo_root().join("docs").join("adr");
    let docs_readme = read_repo_file("docs/README.md")?;

    let adr_re = regex::Regex::new(r"^\d{4}-.+\.md$")?;
    let mut unreferenced: Vec<String> = Vec::new();

    for entry in fs::read_dir(&adr_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !adr_re.is_match(&name) {
            continue;
        }
        if !docs_readme.contains(&name) {
            unreferenced.push(name);
        }
    }

    // Allow up to 2 unreferenced ADRs since docs/README.md may lag slightly
    // behind the ADR directory (new ADRs are sometimes merged before the
    // readme table is updated).
    if unreferenced.len() > 2 {
        panic!(
            "docs/README.md is missing references to {} ADRs (max 2 tolerated):\n  {}",
            unreferenced.len(),
            unreferenced.join("\n  ")
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Docs directory structure
// ---------------------------------------------------------------------------

/// Essential documentation files must exist.
#[test]
fn doc_essential_files_exist() -> Result<(), Box<dyn std::error::Error>> {
    let required_files = [
        "README.md",
        "docs/README.md",
        "docs/DEVELOPMENT.md",
        "docs/CONTRIBUTING.md",
        "docs/adr/README.md",
        "docs/adr/template.md",
        "docs/adr/INDEX.md",
        "CHANGELOG.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
    ];

    let root = repo_root();
    let mut missing: Vec<&str> = Vec::new();
    for f in &required_files {
        if !root.join(f).exists() {
            missing.push(f);
        }
    }

    assert!(
        missing.is_empty(),
        "Required documentation files are missing:\n  {}",
        missing.join("\n  ")
    );
    Ok(())
}
