use anyhow::{Context, Result, anyhow};
use std::path::{Component, Path, PathBuf};

pub(super) fn lane_relative_artifact_path(lane: &Path, artifact: &Path) -> Result<String> {
    let relative = if artifact.is_absolute() {
        let absolute_lane = std::path::absolute(lane)
            .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
        let absolute_artifact = std::path::absolute(artifact)
            .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
        absolute_artifact
            .strip_prefix(&absolute_lane)
            .with_context(|| {
                format!(
                    "artifact '{}' must be under lane '{}'",
                    artifact.display(),
                    lane.display()
                )
            })?
            .to_path_buf()
    } else if let Some(relative) = lane_prefixed_relative_path(lane, artifact) {
        relative
    } else if let Some(relative) = cwd_relative_lane_prefixed_path(lane, artifact)? {
        relative
    } else {
        artifact.to_path_buf()
    };
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "artifact '{}' must be a simple lane-relative path",
            artifact.display()
        ));
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub(super) fn simple_lane_relative_path_string(path: &Path, label: &str) -> Result<String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "{label} '{}' must be a simple lane-relative path",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn lane_prefixed_relative_path(lane: &Path, artifact: &Path) -> Option<PathBuf> {
    if lane.is_absolute() || artifact.is_absolute() {
        return None;
    }

    let relative = artifact.strip_prefix(lane).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

fn cwd_relative_lane_prefixed_path(lane: &Path, artifact: &Path) -> Result<Option<PathBuf>> {
    if artifact.is_absolute() {
        return Ok(None);
    }

    let absolute_lane = std::path::absolute(lane)
        .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
    let absolute_artifact = std::path::absolute(artifact)
        .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
    let Some(relative) = absolute_artifact
        .strip_prefix(&absolute_lane)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(relative.to_path_buf()))
}
