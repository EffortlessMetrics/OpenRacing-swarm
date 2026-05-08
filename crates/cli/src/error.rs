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
