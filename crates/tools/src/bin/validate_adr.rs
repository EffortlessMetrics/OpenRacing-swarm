//! ADR validation binary - validates that ADR files follow the required format.
//!
//! Usage:
//!     cargo run -p openracing-tools --bin validate-adr -- [options]
//!
//! Options:
//!     --adr-dir <path>      Path to ADR directory (default: docs/adr)
//!     --requirements <path> Path to requirements file
//!     -v, --verbose         Verbose output

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Args {
    adr_dir: PathBuf,
    requirements: PathBuf,
    verbose: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            adr_dir: PathBuf::from("docs/adr"),
            requirements: PathBuf::from(".kiro/specs/racing-wheel-suite/requirements.md"),
            verbose: false,
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
            "--requirements" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--requirements requires a path".to_string())?;
                args.requirements = PathBuf::from(value);
            }
            "-v" | "--verbose" => args.verbose = true,
            "-h" | "--help" => {
                stdout_line(format_args!(
                    "Usage: validate-adr [--adr-dir <path>] [--requirements <path>] [-v|--verbose]"
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

fn is_adr_file_name(name: &str) -> bool {
    let Some((number, _rest)) = name.split_once('-') else {
        return false;
    };

    number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && name.ends_with(".md")
}

pub(crate) fn find_adr_files(adr_dir: &Path) -> Vec<PathBuf> {
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

fn has_adr_title(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("# ADR-") else {
        return false;
    };
    let Some((number, title)) = rest.split_once(": ") else {
        return false;
    };

    number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && !title.is_empty()
}

pub(crate) fn validate_adr_format(adr_path: &Path) -> Vec<String> {
    let content = match fs::read_to_string(adr_path) {
        Ok(c) => c,
        Err(e) => return vec![format!("Could not read file: {e}")],
    };

    let mut found_sections = [false; 9];
    for line in content.lines() {
        if has_adr_title(line) {
            found_sections[0] = true;
        } else if line.starts_with("**Status:**") {
            found_sections[1] = true;
        } else if line.starts_with("**Date:**") {
            found_sections[2] = true;
        } else if line.starts_with("**Authors:**") {
            found_sections[3] = true;
        } else if line.starts_with("## Context") {
            found_sections[4] = true;
        } else if line.starts_with("## Decision") {
            found_sections[5] = true;
        } else if line.starts_with("## Rationale") {
            found_sections[6] = true;
        } else if line.starts_with("## Consequences") {
            found_sections[7] = true;
        } else if line.starts_with("## References") {
            found_sections[8] = true;
        }
    }

    let section_names = [
        "Title (# ADR-XXXX: Title)",
        "Status metadata",
        "Date metadata",
        "Authors metadata",
        "Context section",
        "Decision section",
        "Rationale section",
        "Consequences section",
        "References section",
    ];

    let mut errors = Vec::new();
    let missing_sections = section_names
        .iter()
        .enumerate()
        .filter_map(|(idx, name)| (!found_sections[idx]).then_some(*name))
        .collect::<Vec<_>>();

    if !missing_sections.is_empty() {
        errors.push(format!(
            "Missing required sections: {}",
            missing_sections.join(", ")
        ));
    }

    if let Some(status_line) = content.lines().find(|line| line.starts_with("**Status:**")) {
        let valid_statuses = ["Proposed", "Accepted", "Deprecated", "Superseded"];
        let status = status_line.trim().trim_start_matches("**Status:**").trim();
        if !valid_statuses.contains(&status) {
            errors.push(format!(
                "Invalid status '{}'. Must be one of: {}",
                status,
                valid_statuses.join(", ")
            ));
        }
    }

    if !content.contains("Requirements:") {
        errors.push(
            "No requirement references found. ADRs should reference specific requirement IDs."
                .to_string(),
        );
    }

    errors
}

fn requirement_ids(content: &str) -> HashSet<String> {
    content
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter_map(|token| {
            let (prefix, suffix) = token.split_once('-')?;
            let is_req = prefix.len() >= 2
                && prefix.chars().all(|c| c.is_ascii_uppercase())
                && suffix.len() == 2
                && suffix.chars().all(|c| c.is_ascii_digit());
            is_req.then(|| token.to_string())
        })
        .collect()
}

pub(crate) fn extract_requirement_references(adr_path: &Path) -> HashSet<String> {
    fs::read_to_string(adr_path)
        .map(|content| requirement_ids(&content))
        .unwrap_or_default()
}

fn validate_requirement_references(
    adr_files: &[PathBuf],
    requirements_file: &Path,
) -> Vec<(String, Vec<String>)> {
    let req_content = match fs::read_to_string(requirements_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return vec![(
                "global".to_string(),
                vec!["Requirements file not found".to_string()],
            )];
        }
        Err(e) => {
            return vec![(
                "global".to_string(),
                vec![format!("Could not read requirements file: {e}")],
            )];
        }
    };

    let valid_reqs = requirement_ids(&req_content);
    let mut errors = Vec::new();

    for adr_path in adr_files {
        let adr_errors = extract_requirement_references(adr_path)
            .into_iter()
            .filter(|req_id| !valid_reqs.contains(req_id))
            .map(|req_id| format!("References invalid requirement: {req_id}"))
            .collect::<Vec<_>>();

        if !adr_errors.is_empty()
            && let Some(name) = adr_path.file_name().and_then(|n| n.to_str())
        {
            errors.push((name.to_string(), adr_errors));
        }
    }

    errors
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

    stdout_line(format_args!("[INFO] Validating ADR files..."));

    let adr_files = find_adr_files(&args.adr_dir);

    if adr_files.is_empty() {
        stderr_line(format_args!("[ERROR] No ADR files found"));
        return std::process::ExitCode::from(1);
    }

    if args.verbose {
        stdout_line(format_args!("[INFO] Found {} ADR files", adr_files.len()));
    }

    let mut total_errors = 0;

    for adr_path in &adr_files {
        let errors = validate_adr_format(adr_path);
        if !errors.is_empty() {
            if let Some(name) = adr_path.file_name().and_then(|n| n.to_str()) {
                stderr_line(format_args!("\n[ERROR] {name}:"));
                for error in &errors {
                    stderr_line(format_args!("   - {error}"));
                }
            }
            total_errors += errors.len();
        } else if args.verbose
            && let Some(name) = adr_path.file_name().and_then(|n| n.to_str())
        {
            stdout_line(format_args!("[OK] {name}: Format OK"));
        }
    }

    let req_errors = validate_requirement_references(&adr_files, &args.requirements);
    for (file_name, errors) in &req_errors {
        if !errors.is_empty() {
            stderr_line(format_args!("\n[ERROR] {file_name} (requirements):"));
            for error in errors {
                stderr_line(format_args!("   - {error}"));
            }
            total_errors += errors.len();
        }
    }

    if total_errors == 0 {
        stdout_line(format_args!(
            "\n[OK] All {} ADR files are valid!",
            adr_files.len()
        ));
        std::process::ExitCode::from(0)
    } else {
        stderr_line(format_args!(
            "\n[ERROR] Found {total_errors} validation errors"
        ));
        std::process::ExitCode::from(1)
    }
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
    fn validates_complete_adr() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, adr_path) = create_temp_adr(
            "0001-test.md",
            r#"# ADR-0001: Test Title

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Test Author

## Context
Requirements: RT-01

## Decision
Decision.

## Rationale
Rationale.

## Consequences
Consequences.

## References
References.
"#,
        )?;

        assert!(validate_adr_format(&adr_path).is_empty());
        Ok(())
    }

    #[test]
    fn reports_missing_sections() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, adr_path) = create_temp_adr("0001-test.md", "# ADR-0001: Test\n")?;
        let errors = validate_adr_format(&adr_path);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("Status metadata"));
        Ok(())
    }

    #[test]
    fn extracts_requirement_ids() {
        let ids = requirement_ids("Requirements: RT-01, API-22; invalid X-01 RT-1 rt-01");
        assert!(ids.contains("RT-01"));
        assert!(ids.contains("API-22"));
        assert!(!ids.contains("X-01"));
        assert!(!ids.contains("RT-1"));
        assert!(!ids.contains("rt-01"));
    }
}
