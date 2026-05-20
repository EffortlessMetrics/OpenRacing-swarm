use anyhow::Result;
use std::path::{Component, Path};

pub(super) fn normalize_role_id(value: &str, flag: &str) -> Result<String> {
    let role = value.trim();
    if role.is_empty() {
        anyhow::bail!("{flag} role id cannot be empty");
    }
    if !role
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        anyhow::bail!(
            "{flag} role id '{role}' may contain only ASCII letters, numbers, '_' or '-'"
        );
    }
    Ok(role.to_string())
}

pub(super) fn validate_relative_artifact_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute() {
        anyhow::bail!("role artifact paths must be relative to the lane directory");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        anyhow::bail!("role artifact paths must stay within the lane directory");
    }
    Ok(())
}

pub(super) fn validate_connection_path(connection: &str) -> Result<()> {
    if matches!(
        connection,
        "wheelbase_hub" | "standalone_usb" | "cross_device" | "unknown"
    ) {
        return Ok(());
    }
    anyhow::bail!(
        "role connection '{connection}' must be one of wheelbase_hub, standalone_usb, cross_device, unknown"
    )
}
