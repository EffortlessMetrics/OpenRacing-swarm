use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

pub(super) fn write_json_receipt<T: Serialize>(path: Option<&Path>, value: &T) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("failed to serialize JSON receipt")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))
}

pub(super) fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("failed to serialize JSON file")?;
    fs::write(path, json).with_context(|| format!("failed to write '{}'", path.display()))
}

pub(super) fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse '{}'", path.display()))
}

pub(super) fn write_text_file(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    fs::write(path, value).with_context(|| format!("failed to write '{}'", path.display()))
}
