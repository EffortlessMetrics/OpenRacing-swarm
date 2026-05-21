use crate::error::CliError;
use anyhow::Result;
use std::time::Duration;

pub(super) const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:50051";

pub(super) fn resolve_endpoint(endpoint: Option<&str>) -> Result<&str> {
    let endpoint_str = endpoint.unwrap_or(DEFAULT_ENDPOINT);
    validate_endpoint_format(endpoint_str)?;
    Ok(endpoint_str)
}

pub(super) async fn connect_channel(endpoint: &str) -> Result<tonic::transport::Channel> {
    tonic::transport::Endpoint::from_shared(endpoint.to_string())
        .map_err(|e| CliError::ServiceUnavailable(format!("Invalid endpoint: {}", e)))?
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .connect()
        .await
        .map_err(|e| {
            CliError::ServiceUnavailable(format!(
                "Could not connect to wheeld service at {}: {}. Is wheeld running?",
                endpoint, e
            ))
        })
        .map_err(Into::into)
}

fn validate_endpoint_format(endpoint: &str) -> Result<()> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Ok(())
    } else {
        Err(CliError::ServiceUnavailable("Invalid endpoint format".to_string()).into())
    }
}
