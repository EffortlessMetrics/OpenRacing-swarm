//! Check for private module imports in integration tests.
//!
//! Integration tests should exercise public crate APIs rather than reaching
//! into private, internal, or test-only modules.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
struct Args {
    root: PathBuf,
}

#[derive(Debug, Eq, PartialEq)]
struct Violation {
    path: PathBuf,
    line: usize,
    content: String,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut root = PathBuf::from(".");
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--root requires a path".to_string())?;
                root = PathBuf::from(value);
            }
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }

    Ok(Args { root })
}

fn usage() -> String {
    "Usage: check-private-imports [--root <path>]".to_string()
}

fn visit_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("failed to read {}: {error}", dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to read entry in {}: {error}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to read file type for {}: {error}", path.display()))?;

        if file_type.is_dir() {
            visit_rust_files(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }

    Ok(())
}

fn is_private_import(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("use ")
        && (line.contains("::tests")
            || line.contains("::internal")
            || line.contains("::private")
            || line.contains("crate::tests")
            || line.contains("crate::internal")
            || line.contains("crate::private"))
}

fn check_private_imports(root: &Path) -> Result<Vec<Violation>, String> {
    let integration_tests_dir = root.join("crates/integration-tests");
    if !integration_tests_dir.exists() {
        stdout_line(format_args!("Integration tests directory not found"));
        return Ok(Vec::new());
    }

    let mut rust_files = Vec::new();
    visit_rust_files(&integration_tests_dir, &mut rust_files)?;
    rust_files.sort();

    let mut violations = Vec::new();
    for rust_file in rust_files {
        let content = fs::read_to_string(&rust_file)
            .map_err(|error| format!("failed to read {}: {error}", rust_file.display()))?;

        for (line_index, line) in content.lines().enumerate() {
            if is_private_import(line) {
                violations.push(Violation {
                    path: rust_file.clone(),
                    line: line_index + 1,
                    content: line.trim().to_string(),
                });
            }
        }
    }

    Ok(violations)
}

fn run(args: Args) -> Result<std::process::ExitCode, String> {
    let violations = check_private_imports(&args.root)?;

    if violations.is_empty() {
        stdout_line(format_args!(
            "No private module imports found in integration tests"
        ));
        return Ok(std::process::ExitCode::SUCCESS);
    }

    stderr_line(format_args!(
        "Found private module imports in integration tests:"
    ));
    for violation in &violations {
        stderr_line(format_args!(
            "  {}:{}: {}",
            violation.path.display(),
            violation.line,
            violation.content
        ));
    }

    Ok(std::process::ExitCode::from(1))
}

fn stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{args}");
}

fn stderr_line(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

fn main() -> std::process::ExitCode {
    let args = match parse_args(env::args()) {
        Ok(args) => args,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return std::process::ExitCode::from(2);
        }
    };

    match run(args) {
        Ok(code) => code,
        Err(error) => {
            stderr_line(format_args!("ERROR: {error}"));
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_private_imports() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/integration-tests/src");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("lib.rs"), "use openracing_engine::internal;\n")?;

        let violations = check_private_imports(temp.path())?;

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
        assert_eq!(violations[0].content, "use openracing_engine::internal;");
        Ok(())
    }

    #[test]
    fn ignores_public_imports() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/integration-tests/src");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("lib.rs"), "use openracing_engine::public;\n")?;

        let violations = check_private_imports(temp.path())?;

        assert!(violations.is_empty());
        Ok(())
    }
}
