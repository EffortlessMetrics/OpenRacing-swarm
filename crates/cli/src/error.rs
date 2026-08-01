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
            CliError::PermissionDenied(_) => Some(permission_denied_hint()),
            CliError::ValidationError(_) | CliError::SchemaError(_) => Some(
                "Check the file against the profile schema:\n  \
                 wheelctl profile validate <file>"
                    .to_string(),
            ),
            CliError::ReceiptFailure(_)
            | CliError::InvalidConfiguration(_)
            | CliError::IoError(_)
            | CliError::JsonError(_)
            | CliError::YamlError(_) => None,
        }
    }
}

/// Platform-specific guidance for device permission failures.
///
/// On Linux this is nearly always missing udev rules or group membership,
/// which is concrete and fixable rather than a mystery.
fn permission_denied_hint() -> String {
    if cfg!(target_os = "linux") {
        "On Linux this usually means the udev rules are not installed, or your \
         user is not in the input group:\n  \
         sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/\n  \
         sudo udevadm control --reload-rules && sudo udevadm trigger\n  \
         sudo usermod -a -G input,plugdev \"$USER\"    (then log out and back in)\n\
         Run `wheelctl doctor` to see which of these is missing."
            .to_string()
    } else {
        "Run `wheelctl doctor` to check device access permissions.".to_string()
    }
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
