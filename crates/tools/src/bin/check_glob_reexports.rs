//! Check for glob re-exports in Rust source files.
//!
//! This checker preserves the legacy report-only behavior: it prints found
//! glob re-exports but exits successfully so the known migration backlog does
//! not block CI.

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
    "Usage: check-glob-reexports [--root <path>]".to_string()
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

fn is_glob_reexport(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("pub use ") && line.contains("::*") && line.ends_with(';')
}

fn check_glob_reexports(root: &Path) -> Result<Vec<Violation>, String> {
    let crates_dir = root.join("crates");
    if !crates_dir.exists() {
        stderr_line(format_args!(
            "Warning: {} does not exist",
            crates_dir.display()
        ));
        return Ok(Vec::new());
    }

    let mut rust_files = Vec::new();
    visit_rust_files(&crates_dir, &mut rust_files)?;
    rust_files.sort();

    let mut violations = Vec::new();
    for rust_file in rust_files {
        let content = fs::read_to_string(&rust_file)
            .map_err(|error| format!("failed to read {}: {error}", rust_file.display()))?;

        for (line_index, line) in content.lines().enumerate() {
            if is_glob_reexport(line) {
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
    let violations = check_glob_reexports(&args.root)?;

    if violations.is_empty() {
        stdout_line(format_args!("No glob re-exports found"));
        return Ok(std::process::ExitCode::SUCCESS);
    }

    stdout_line(format_args!(
        "Found glob re-exports; these are report-only and should be refactored later:"
    ));
    for violation in &violations {
        stdout_line(format_args!(
            "  {}:{}: {}",
            violation.path.display(),
            violation.line,
            violation.content
        ));
    }
    stdout_line(format_args!(
        "Total: {} glob re-exports found",
        violations.len()
    ));

    Ok(std::process::ExitCode::SUCCESS)
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
    fn reports_glob_reexports() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/example/src");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("lib.rs"), "pub use crate::module::*;\n")?;

        let violations = check_glob_reexports(temp.path())?;

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
        assert_eq!(violations[0].content, "pub use crate::module::*;");
        Ok(())
    }

    #[test]
    fn ignores_non_glob_reexports() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/example/src");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("lib.rs"), "pub use crate::module::Thing;\n")?;

        let violations = check_glob_reexports(temp.path())?;

        assert!(violations.is_empty());
        Ok(())
    }

    #[test]
    fn parse_args_defaults_root_to_current_dir() -> Result<(), Box<dyn std::error::Error>> {
        let args = parse_args(vec!["check-glob-reexports".to_string()])?;
        assert_eq!(args.root, PathBuf::from("."));
        Ok(())
    }

    #[test]
    fn parse_args_accepts_root_option() -> Result<(), Box<dyn std::error::Error>> {
        let args = parse_args(vec![
            "check-glob-reexports".to_string(),
            "--root".to_string(),
            "/some/path".to_string(),
        ])?;
        assert_eq!(args.root, PathBuf::from("/some/path"));
        Ok(())
    }

    #[test]
    fn parse_args_errors_when_root_value_missing() {
        let result = parse_args(vec![
            "check-glob-reexports".to_string(),
            "--root".to_string(),
        ]);
        match result {
            Err(message) => assert!(message.contains("--root requires a path")),
            Ok(args) => panic!("expected error, got {args:?}"),
        }
    }

    #[test]
    fn parse_args_help_returns_usage_for_short_and_long_flags() {
        for flag in ["-h", "--help"] {
            let result = parse_args(vec!["check-glob-reexports".to_string(), flag.to_string()]);
            match result {
                Err(message) => assert!(message.starts_with("Usage:")),
                Ok(args) => panic!("expected usage error for {flag}, got {args:?}"),
            }
        }
    }

    #[test]
    fn parse_args_rejects_unknown_argument() {
        let result = parse_args(vec![
            "check-glob-reexports".to_string(),
            "--bogus".to_string(),
        ]);
        match result {
            Err(message) => {
                assert!(message.contains("unknown argument: --bogus"));
                assert!(message.contains("Usage:"));
            }
            Ok(args) => panic!("expected error, got {args:?}"),
        }
    }

    #[test]
    fn is_glob_reexport_detects_indented_lines() {
        assert!(is_glob_reexport("    pub use crate::module::*;"));
        assert!(is_glob_reexport("\tpub use foo::bar::*;"));
    }

    #[test]
    fn is_glob_reexport_rejects_non_pub_use() {
        assert!(!is_glob_reexport("use crate::module::*;"));
        assert!(!is_glob_reexport("pub mod module;"));
        assert!(!is_glob_reexport("// pub use crate::module::*;"));
    }

    #[test]
    fn is_glob_reexport_requires_glob_and_semicolon() {
        assert!(!is_glob_reexport("pub use crate::module::Thing;"));
        assert!(!is_glob_reexport("pub use crate::module::*"));
        assert!(!is_glob_reexport("pub use crate::module"));
    }

    #[test]
    fn check_glob_reexports_missing_crates_dir_returns_empty()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        // No `crates/` directory created.
        let violations = check_glob_reexports(temp.path())?;
        assert!(violations.is_empty());
        Ok(())
    }

    #[test]
    fn check_glob_reexports_ignores_non_rust_files() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/example/src");
        fs::create_dir_all(&src_dir)?;
        // A `.txt` file containing what looks like a glob re-export should be ignored.
        fs::write(src_dir.join("notes.txt"), "pub use crate::module::*;\n")?;
        // A `.md` file as well.
        fs::write(src_dir.join("README.md"), "pub use foo::*;\n")?;

        let violations = check_glob_reexports(temp.path())?;

        assert!(violations.is_empty());
        Ok(())
    }

    #[test]
    fn check_glob_reexports_walks_nested_directories() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let nested = temp.path().join("crates/alpha/src/inner/deeper");
        fs::create_dir_all(&nested)?;
        fs::write(nested.join("mod.rs"), "pub use crate::a::*;\n")?;

        let violations = check_glob_reexports(temp.path())?;

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
        assert_eq!(violations[0].content, "pub use crate::a::*;");
        Ok(())
    }

    #[test]
    fn check_glob_reexports_reports_multiple_violations_in_one_file()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let src_dir = temp.path().join("crates/example/src");
        fs::create_dir_all(&src_dir)?;
        let content = "// header comment\n\
                       pub use crate::a::*;\n\
                       pub use crate::b::Thing;\n\
                       pub use crate::c::*;\n";
        fs::write(src_dir.join("lib.rs"), content)?;

        let violations = check_glob_reexports(temp.path())?;

        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].line, 2);
        assert_eq!(violations[0].content, "pub use crate::a::*;");
        assert_eq!(violations[1].line, 4);
        assert_eq!(violations[1].content, "pub use crate::c::*;");
        Ok(())
    }

    #[test]
    fn check_glob_reexports_handles_multiple_files() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let a_src = temp.path().join("crates/alpha/src");
        let b_src = temp.path().join("crates/beta/src");
        fs::create_dir_all(&a_src)?;
        fs::create_dir_all(&b_src)?;
        fs::write(a_src.join("lib.rs"), "pub use crate::a::*;\n")?;
        fs::write(b_src.join("lib.rs"), "pub use crate::b::Item;\n")?;

        let violations = check_glob_reexports(temp.path())?;

        assert_eq!(violations.len(), 1);
        assert!(violations[0].path.ends_with("alpha/src/lib.rs"));
        assert_eq!(violations[0].content, "pub use crate::a::*;");
        Ok(())
    }
}
