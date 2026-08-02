//! Error types for wheelctl CLI

use thiserror::Error;
#[derive(Error, Debug)]
pub enum CliError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("{0}")]
    ReceiptFailure(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("Schema error: {0}")]
    SchemaError(#[from] racing_wheel_schemas::config::SchemaError),
}

impl CliError {
    /// A short, stable machine-readable discriminator for this error.
    ///
    /// Exposed in `--json` output so scripts can branch on the kind of failure
    /// without string-matching the human message.
    pub fn kind(&self) -> &'static str {
        match self {
            CliError::DeviceNotFound(_) => "device_not_found",
            CliError::ProfileNotFound(_) => "profile_not_found",
            CliError::ValidationError(_) => "validation_error",
            CliError::ReceiptFailure(_) => "receipt_failure",
            CliError::ServiceUnavailable(_) => "service_unavailable",
            CliError::PermissionDenied(_) => "permission_denied",
            CliError::InvalidConfiguration(_) => "invalid_configuration",
            CliError::IoError(_) => "io_error",
            CliError::JsonError(_) => "json_error",
            CliError::YamlError(_) => "yaml_error",
            CliError::SchemaError(_) => "schema_error",
        }
    }

    /// What the user can actually do about this error.
    ///
    /// An error that only states what went wrong leaves a first-time user
    /// stuck, so every variant with a plausible next step names a command to
    /// run. Returns `None` where no generic advice would be honest: guessing
    /// at a fix is worse than staying quiet, and the underlying message
    /// already carries the specific parse or I/O failure in those cases.
    pub fn hint(&self) -> Option<String> {
        match self {
            CliError::DeviceNotFound(_) => Some(
                "List the devices that are actually connected:\n  \
                 wheelctl device list\n\
                 If that list is empty, run `wheelctl doctor` to check the \
                 service, permissions, and udev rules."
                    .to_string(),
            ),
            CliError::ProfileNotFound(_) => {
                Some("List available profiles:\n  wheelctl profile list".to_string())
            }
            CliError::ServiceUnavailable(_) => Some(format!(
                "Start the service:\n  {}\nThen confirm it is up:\n  wheelctl health",
                crate::client::start_service_hint()
            )),
            CliError::PermissionDenied(message) => permission_denied_hint(message),
            // `SchemaError` only ever comes from the profile validator, so the
            // profile-schema hint is unconditionally right for it.
            CliError::SchemaError(_) => Some(
                "Check the file against the profile schema:\n  \
                 wheelctl profile validate <file>"
                    .to_string(),
            ),
            CliError::ValidationError(message) => validation_error_hint(message),
            CliError::ReceiptFailure(_)
            | CliError::InvalidConfiguration(_)
            | CliError::IoError(_)
            | CliError::JsonError(_)
            | CliError::YamlError(_) => None,
        }
    }
}

/// Guidance for a `PermissionDenied`, which is not always about permissions.
///
/// `safety enable` reuses this variant to report a refused *interlock* —
/// active faults, temperature, or hands-off state — where device access
/// succeeded and nothing about udev rules or group membership is relevant.
/// Telling that user to reinstall udev rules sends them to fix a system that
/// is already working while an interlock is holding torque back, which is a
/// bad thing to be wrong about. Today the interlock is in fact the only
/// in-tree producer of this variant, so the access-permission text below is
/// reachable only from a future call site.
fn permission_denied_hint(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("safety") || lower.contains("interlock") || lower.contains("conditions") {
        return Some(
            "This is a safety interlock, not a file-permission problem. Check \
             what is blocking it:\n  \
             wheelctl safety status <device>\n\
             Faults, temperature, and hands-on state each gate high torque \
             independently."
                .to_string(),
        );
    }

    if cfg!(target_os = "linux") {
        Some(
            "On Linux this usually means the udev rules are not installed, or your \
             user is not in the input group:\n  \
             sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/\n  \
             sudo udevadm control --reload-rules && sudo udevadm trigger\n  \
             sudo usermod -a -G input,plugdev \"$USER\"    (then log out and back in)\n\
             Run `wheelctl doctor` to see which of these is missing."
                .to_string(),
        )
    } else {
        Some("Run `wheelctl doctor` to check device access permissions.".to_string())
    }
}

/// Guidance for a `ValidationError`, which is the catch-all validation variant.
///
/// It is returned for blackbox format and version failures, plugin registry
/// lookups, firmware bundles, and torque-limit range checks as well as profile
/// problems. A blanket "run `wheelctl profile validate`" is wrong for most of
/// those, so the hint is offered only when the message is actually about a
/// profile or schema. Everywhere else the message already names the specific
/// failure and silence beats a misdirection.
fn validation_error_hint(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("profile") || lower.contains("schema") {
        return Some(
            "Check the file against the profile schema:\n  \
             wheelctl profile validate <file>"
                .to_string(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_device_not_found() {
        let err = CliError::DeviceNotFound("wheel-001".to_string());
        assert_eq!(err.to_string(), "Device not found: wheel-001");
    }

    #[test]
    fn display_profile_not_found() {
        let err = CliError::ProfileNotFound("default.json".to_string());
        assert_eq!(err.to_string(), "Profile not found: default.json");
    }

    #[test]
    fn display_validation_error() {
        let err = CliError::ValidationError("invalid gain".to_string());
        assert_eq!(err.to_string(), "Validation error: invalid gain");
    }

    #[test]
    fn display_receipt_failure() {
        let err = CliError::ReceiptFailure("receipt failed".to_string());
        assert_eq!(err.to_string(), "receipt failed");
    }

    #[test]
    fn display_service_unavailable() {
        let err = CliError::ServiceUnavailable("Connection refused".to_string());
        assert_eq!(err.to_string(), "Service unavailable: Connection refused");
    }

    #[test]
    fn display_permission_denied() {
        let err = CliError::PermissionDenied("root required".to_string());
        assert_eq!(err.to_string(), "Permission denied: root required");
    }

    #[test]
    fn display_invalid_configuration() {
        let err = CliError::InvalidConfiguration("bad path".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: bad path");
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let cli_err = CliError::from(io_err);
        assert!(cli_err.to_string().contains("missing"));
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CliError>();
    }
}
