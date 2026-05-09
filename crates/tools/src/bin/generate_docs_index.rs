//! Documentation index generator - generates an index of all ADRs.
//!
//! Usage:
//!     cargo run -p openracing-tools --bin generate-docs-index -- [options]
//!
//! Options:
//!     --adr-dir <path>  Path to ADR directory (default: docs/adr)

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Args {
    adr_dir: PathBuf,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            adr_dir: PathBuf::from("docs/adr"),
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--adr-dir" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--adr-dir requires a path".to_string())?;
                args.adr_dir = PathBuf::from(value);
            }
            "-h" | "--help" => {
                stdout_line(format_args!(
                    "Usage: generate-docs-index [--adr-dir <path>]"
                ));
                std::process::exit(0);
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }

    Ok(args)
}

fn stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{args}");
}

fn stderr_line(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

#[derive(Debug, Default)]
pub(crate) struct AdrInfo {
    title: String,
    description: String,
    status: String,
    date: String,
    authors: String,
}

fn is_adr_file_name(name: &str) -> bool {
    let Some((number, _rest)) = name.split_once('-') else {
        return false;
    };

    number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && name.ends_with(".md")
}

fn adr_title(line: &str) -> Option<String> {
    let rest = line.strip_prefix("# ADR-")?;
    let (number, title) = rest.split_once(": ")?;

    (number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && !title.is_empty())
        .then(|| format!("ADR-{number}: {title}"))
}

pub(crate) fn extract_adr_info(adr_path: &Path) -> AdrInfo {
    let mut info = AdrInfo::default();

    let content = match fs::read_to_string(adr_path) {
        Ok(c) => c,
        Err(_) => return info,
    };

    let lines = content.lines().collect::<Vec<_>>();

    if let Some(first_line) = lines.first() {
        info.title = adr_title(first_line).unwrap_or_else(|| {
            adr_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        });
    }

    for line in lines.iter().take(20) {
        if let Some(status) = line.strip_prefix("**Status:**") {
            info.status = status.trim().to_string();
        } else if let Some(date) = line.strip_prefix("**Date:**") {
            info.date = date.trim().to_string();
        } else if let Some(authors) = line.strip_prefix("**Authors:**") {
            info.authors = authors.trim().to_string();
        }
    }

    let mut context_started = false;
    for line in &lines {
        if line.starts_with("## Context") {
            context_started = true;
            continue;
        }

        if context_started {
            if line.starts_with("##") {
                break;
            }
            if !line.trim().is_empty() && info.description.is_empty() {
                info.description = line.trim().to_string();
                break;
            }
        }
    }

    if info.status.is_empty() {
        info.status = "Unknown".to_string();
    }
    if info.date.is_empty() {
        info.date = "Unknown".to_string();
    }
    if info.authors.is_empty() {
        info.authors = "Unknown".to_string();
    }

    info
}

fn find_adr_files(adr_dir: &Path) -> Vec<PathBuf> {
    let mut adr_files = Vec::new();

    if let Ok(entries) = fs::read_dir(adr_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name != "template.md"
                && name != "README.md"
                && is_adr_file_name(name)
            {
                adr_files.push(path);
            }
        }
    }

    adr_files.sort();
    adr_files
}

fn is_iso_date(date: &str) -> bool {
    let mut parts = date.split('-');
    let year = parts.next();
    let month = parts.next();
    let day = parts.next();

    parts.next().is_none()
        && year.is_some_and(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()))
        && month.is_some_and(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_digit()))
        && day.is_some_and(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_digit()))
}

pub(crate) fn generate_adr_index(adr_dir: &Path) -> String {
    let adr_files = find_adr_files(adr_dir);

    let mut index_lines = vec![
        "# Architecture Decision Records Index".to_string(),
        String::new(),
        format!("Total ADRs: {}", adr_files.len()),
        String::new(),
        "| ADR | Title | Status | Date | Authors |".to_string(),
        "|-----|-------|--------|------|---------|".to_string(),
    ];

    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        let file_name = adr_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let adr_num = file_name.chars().take(4).collect::<String>();

        index_lines.push(format!(
            "| [{adr_num}]({file_name}) | {} | {} | {} | {} |",
            info.title, info.status, info.date, info.authors
        ));
    }

    index_lines.push(String::new());
    index_lines.push("## Status Summary".to_string());
    index_lines.push(String::new());

    let mut status_counts: HashMap<String, usize> = HashMap::new();
    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        *status_counts.entry(info.status).or_insert(0) += 1;
    }

    let mut statuses = status_counts.keys().collect::<Vec<_>>();
    statuses.sort();
    for status in statuses {
        if let Some(count) = status_counts.get(status) {
            index_lines.push(format!("- **{status}**: {count}"));
        }
    }

    index_lines.push(String::new());
    index_lines.push("## Recent Changes".to_string());
    index_lines.push(String::new());

    let mut dated_adrs = Vec::new();
    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        if is_iso_date(&info.date) {
            dated_adrs.push((info.date.clone(), adr_path, info));
        }
    }

    dated_adrs.sort_by(|a, b| b.0.cmp(&a.0));

    for (_, adr_path, info) in dated_adrs.iter().take(5) {
        let file_name = adr_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        index_lines.push(format!("- {}: [{}]({})", info.date, info.title, file_name));
    }

    index_lines.join("\n")
}

fn main() -> std::process::ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            stderr_line(format_args!("[ERROR] {e}"));
            return std::process::ExitCode::from(2);
        }
    };

    if !args.adr_dir.exists() {
        stderr_line(format_args!(
            "[ERROR] ADR directory not found: {:?}",
            args.adr_dir
        ));
        return std::process::ExitCode::from(1);
    }

    stdout_line(format_args!("[INFO] Generating documentation index..."));

    let index_content = generate_adr_index(&args.adr_dir);
    let index_file = args.adr_dir.join("INDEX.md");

    if let Err(e) = fs::write(&index_file, &index_content) {
        stderr_line(format_args!("[ERROR] Failed to write index file: {e}"));
        return std::process::ExitCode::from(1);
    }

    stdout_line(format_args!("[OK] Generated ADR index: {:?}", index_file));
    std::process::ExitCode::from(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_adr(
        name: &str,
        content: &str,
    ) -> Result<(tempfile::TempDir, PathBuf), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::TempDir::new()?;
        let file_path = temp_dir.path().join(name);
        let mut file = std::fs::File::create(&file_path)?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, file_path))
    }

    #[test]
    fn extracts_adr_info() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, adr_path) = create_temp_adr(
            "0001-test.md",
            r#"# ADR-0001: Test Title

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Test Author

## Context
This is context.

## Decision
Decision.
"#,
        )?;

        let info = extract_adr_info(&adr_path);
        assert_eq!(info.title, "ADR-0001: Test Title");
        assert_eq!(info.status, "Proposed");
        assert_eq!(info.date, "2026-01-15");
        assert_eq!(info.authors, "Test Author");
        assert_eq!(info.description, "This is context.");
        Ok(())
    }

    #[test]
    fn generates_adr_index() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::TempDir::new()?;
        std::fs::write(
            temp_dir.path().join("0001-first.md"),
            r#"# ADR-0001: First ADR

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Author One

## Context
Context.
"#,
        )?;
        std::fs::write(
            temp_dir.path().join("0002-second.md"),
            r#"# ADR-0002: Second ADR

**Status:** Accepted
**Date:** 2026-01-10
**Authors:** Author Two

## Context
Context.
"#,
        )?;

        let index = generate_adr_index(temp_dir.path());
        assert!(index.contains("Total ADRs: 2"));
        assert!(index.contains("ADR-0001"));
        assert!(index.contains("ADR-0002"));
        assert!(index.contains("- **Accepted**: 1"));
        assert!(index.contains("- **Proposed**: 1"));
        Ok(())
    }
}
