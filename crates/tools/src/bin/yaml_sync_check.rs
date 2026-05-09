//! YAML sync tool for the duplicated game support matrix files.
//!
//! The default check mode compares the canonical telemetry-config matrix with
//! the telemetry-support mirror. Passing `--fix` copies the canonical file to
//! the mirror. Two explicit paths can still be provided for ad-hoc checks.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use serde_yaml::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const CANONICAL_MATRIX: &str = "crates/telemetry-config/src/game_support_matrix.yaml";
const MIRROR_MATRIX: &str = "crates/telemetry-support/src/game_support_matrix.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Fix,
}

#[derive(Debug, PartialEq, Eq)]
struct Config {
    mode: Mode,
    path_a: PathBuf,
    path_b: PathBuf,
}

pub(crate) fn sorted_yaml(value: &Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), sorted_yaml(value)))
                .collect::<Vec<_>>();
            sorted.sort_by_key(|(key, _)| value_sort_key(key));
            Value::Mapping(sorted.into_iter().collect())
        }
        Value::Sequence(sequence) => Value::Sequence(sequence.iter().map(sorted_yaml).collect()),
        _ => value.clone(),
    }
}

fn value_sort_key(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{value:?}"))
}

pub(crate) fn render_games(data: &Value) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(games) = data.get("games").and_then(Value::as_mapping) {
        let mut keys = games.keys().filter_map(Value::as_str).collect::<Vec<_>>();
        keys.sort_unstable();

        for key in keys {
            if let Some(game) = games.get(key) {
                let name = game.get("name").and_then(Value::as_str).unwrap_or(key);
                lines.push(format!("{key}: {name}"));
            }
        }
    }

    lines
}

fn parse_config(args: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut mode = Mode::Check;
    let mut paths = Vec::new();

    for arg in args.into_iter().skip(1) {
        match arg.as_str() {
            "--check" => mode = Mode::Check,
            "--fix" => mode = Mode::Fix,
            "-h" | "--help" => return Err(usage()),
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}\n{}", usage())),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    match (mode, paths.len()) {
        (Mode::Check, 0) => Ok(Config {
            mode,
            path_a: PathBuf::from(CANONICAL_MATRIX),
            path_b: PathBuf::from(MIRROR_MATRIX),
        }),
        (Mode::Check, 2) => Ok(Config {
            mode,
            path_a: paths.remove(0),
            path_b: paths.remove(0),
        }),
        (Mode::Fix, 0) => Ok(Config {
            mode,
            path_a: PathBuf::from(CANONICAL_MATRIX),
            path_b: PathBuf::from(MIRROR_MATRIX),
        }),
        (Mode::Fix, _) => Err(format!(
            "--fix only supports the default game support matrix paths\n{}",
            usage()
        )),
        (Mode::Check, _) => Err(usage()),
    }
}

fn usage() -> String {
    format!(
        "Usage:\n  yaml-sync-check [--check]\n  yaml-sync-check --fix\n  yaml-sync-check <file_a> <file_b>\n\nDefault paths:\n  {CANONICAL_MATRIX}\n  {MIRROR_MATRIX}"
    )
}

fn read_yaml(path: &Path) -> Result<Value, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("ERROR: {}: {error}", path.display()))?;
    serde_yaml::from_str(&content)
        .map_err(|error| format!("ERROR: failed to parse {}: {error}", path.display()))
}

fn check_files(path_a: &Path, path_b: &Path) -> Result<(), u8> {
    let data_a = match read_yaml(path_a) {
        Ok(value) => value,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return Err(2);
        }
    };
    let data_b = match read_yaml(path_b) {
        Ok(value) => value,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return Err(2);
        }
    };

    let norm_a = sorted_yaml(&data_a);
    let norm_b = sorted_yaml(&data_b);

    if norm_a == norm_b {
        stdout_line(format_args!(
            "OK: {} and {} are identical.",
            path_a.display(),
            path_b.display()
        ));
        return Ok(());
    }

    report_difference(path_a, path_b, &data_a, &data_b, &norm_a, &norm_b);
    Err(1)
}

fn fix_default_files(path_a: &Path, path_b: &Path) -> Result<(), u8> {
    let content_a = match fs::read_to_string(path_a) {
        Ok(content) => content,
        Err(error) => {
            stderr_line(format_args!("ERROR: {}: {error}", path_a.display()));
            return Err(2);
        }
    };

    match fs::read_to_string(path_b) {
        Ok(content_b) if content_b == content_a => {
            stdout_line(format_args!(
                "OK: {} and {} are identical.",
                path_a.display(),
                path_b.display()
            ));
            Ok(())
        }
        _ => match fs::copy(path_a, path_b) {
            Ok(_) => {
                stdout_line(format_args!(
                    "Fixed: copied {} to {}",
                    path_a.display(),
                    path_b.display()
                ));
                Ok(())
            }
            Err(error) => {
                stderr_line(format_args!(
                    "ERROR: failed to copy {} to {}: {error}",
                    path_a.display(),
                    path_b.display()
                ));
                Err(2)
            }
        },
    }
}

fn report_difference(
    path_a: &Path,
    path_b: &Path,
    data_a: &Value,
    data_b: &Value,
    norm_a: &Value,
    norm_b: &Value,
) {
    let games_a = render_games(data_a);
    let games_b = render_games(data_b);

    let set_a = games_a.iter().cloned().collect::<BTreeSet<_>>();
    let set_b = games_b.iter().cloned().collect::<BTreeSet<_>>();

    let only_a = set_a.difference(&set_b).cloned().collect::<Vec<_>>();
    let only_b = set_b.difference(&set_a).cloned().collect::<Vec<_>>();

    stderr_line(format_args!(
        "ERROR: game support matrix files are out of sync!"
    ));
    stderr_line(format_args!("  {}", path_a.display()));
    stderr_line(format_args!("  {}", path_b.display()));
    stderr_line(format_args!(""));

    if !only_a.is_empty() {
        stderr_line(format_args!("Games only in {}:", path_a.display()));
        for game in &only_a {
            stderr_line(format_args!("  + {game}"));
        }
    }

    if !only_b.is_empty() {
        stderr_line(format_args!("Games only in {}:", path_b.display()));
        for game in &only_b {
            stderr_line(format_args!("  + {game}"));
        }
    }

    if only_a.is_empty() && only_b.is_empty() {
        report_content_diff(path_a, path_b, norm_a, norm_b);
    }

    stderr_line(format_args!(""));
    stderr_line(format_args!(
        "Fix: run `cargo run -p openracing-tools --bin yaml-sync-check -- --fix` to copy"
    ));
    stderr_line(format_args!(
        "     the canonical telemetry-config matrix to the telemetry-support mirror."
    ));
}

fn report_content_diff(path_a: &Path, path_b: &Path, norm_a: &Value, norm_b: &Value) {
    let text_a = match serde_yaml::to_string(norm_a) {
        Ok(text) => text,
        Err(error) => {
            stderr_line(format_args!(
                "ERROR: failed to render {} for diff: {error}",
                path_a.display()
            ));
            return;
        }
    };
    let text_b = match serde_yaml::to_string(norm_b) {
        Ok(text) => text,
        Err(error) => {
            stderr_line(format_args!(
                "ERROR: failed to render {} for diff: {error}",
                path_b.display()
            ));
            return;
        }
    };

    stderr_line(format_args!(""));
    stderr_line(format_args!("Content diff:"));

    let lines_a = text_a.lines().collect::<Vec<_>>();
    let lines_b = text_b.lines().collect::<Vec<_>>();
    let max_lines = lines_a.len().max(lines_b.len());

    for index in 0..max_lines {
        match (lines_a.get(index), lines_b.get(index)) {
            (Some(left), Some(right)) if left == right => stderr_line(format_args!("  {left}")),
            (Some(left), Some(right)) => {
                stderr_line(format_args!("- {left}"));
                stderr_line(format_args!("+ {right}"));
            }
            (Some(left), None) => stderr_line(format_args!("- {left}")),
            (None, Some(right)) => stderr_line(format_args!("+ {right}")),
            (None, None) => {}
        }
    }
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
    let config = match parse_config(env::args()) {
        Ok(config) => config,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return std::process::ExitCode::from(2);
        }
    };

    let result = match config.mode {
        Mode::Check => check_files(&config.path_a, &config.path_b),
        Mode::Fix => fix_default_files(&config.path_a, &config.path_b),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(code) => std::process::ExitCode::from(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_yaml(s: &str) -> Result<Value, serde_yaml::Error> {
        serde_yaml::from_str(s)
    }

    #[test]
    fn parse_config_defaults_to_check() -> Result<(), Box<dyn std::error::Error>> {
        let config = parse_config(["yaml-sync-check".to_string()])?;
        assert_eq!(config.mode, Mode::Check);
        assert_eq!(config.path_a, PathBuf::from(CANONICAL_MATRIX));
        assert_eq!(config.path_b, PathBuf::from(MIRROR_MATRIX));
        Ok(())
    }

    #[test]
    fn parse_config_accepts_two_explicit_check_paths() -> Result<(), Box<dyn std::error::Error>> {
        let config = parse_config([
            "yaml-sync-check".to_string(),
            "a.yaml".to_string(),
            "b.yaml".to_string(),
        ])?;
        assert_eq!(config.mode, Mode::Check);
        assert_eq!(config.path_a, PathBuf::from("a.yaml"));
        assert_eq!(config.path_b, PathBuf::from("b.yaml"));
        Ok(())
    }

    #[test]
    fn parse_config_fix_uses_default_paths() -> Result<(), Box<dyn std::error::Error>> {
        let config = parse_config(["yaml-sync-check".to_string(), "--fix".to_string()])?;
        assert_eq!(config.mode, Mode::Fix);
        assert_eq!(config.path_a, PathBuf::from(CANONICAL_MATRIX));
        assert_eq!(config.path_b, PathBuf::from(MIRROR_MATRIX));
        Ok(())
    }

    #[test]
    fn parse_config_rejects_fix_with_explicit_paths() {
        let result = parse_config([
            "yaml-sync-check".to_string(),
            "--fix".to_string(),
            "a.yaml".to_string(),
            "b.yaml".to_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn sorted_yaml_orders_map_keys() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = parse_yaml("b: 2\na: 1")?;
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted)?;
        assert!(text.starts_with("a: 1\nb: 2"));
        Ok(())
    }

    #[test]
    fn sorted_yaml_preserves_sequence_order() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = parse_yaml("[3, 1, 2]")?;
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted)?;
        assert!(text.contains('3') && text.contains('1') && text.contains('2'));
        Ok(())
    }

    #[test]
    fn render_games_uses_sorted_game_keys() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = parse_yaml(
            r#"
games:
  z_game:
    name: Z Game
  a_game:
    name: A Game
  m_game:
    name: M Game
"#,
        )?;
        let games = render_games(&yaml);
        assert_eq!(
            games,
            ["a_game: A Game", "m_game: M Game", "z_game: Z Game"]
        );
        Ok(())
    }

    #[test]
    fn render_games_falls_back_to_key_without_name() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = parse_yaml(
            r#"
games:
  game_a: {}
"#,
        )?;
        let games = render_games(&yaml);
        assert_eq!(games, ["game_a: game_a"]);
        Ok(())
    }

    #[test]
    fn normalized_yaml_is_key_order_independent() -> Result<(), Box<dyn std::error::Error>> {
        let yaml_a = parse_yaml("b: 2\na: 1")?;
        let yaml_b = parse_yaml("a: 1\nb: 2")?;
        assert_eq!(sorted_yaml(&yaml_a), sorted_yaml(&yaml_b));
        Ok(())
    }
}
