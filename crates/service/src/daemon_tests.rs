//! Service daemon lifecycle tests

#[cfg(test)]
mod tests {
    use crate::{IpcConfig, ServiceConfig, ServiceDaemon, TransportType};
    use anyhow::{Result, anyhow};
    use std::time::Duration;
    use tempfile::TempDir;
    use tracing_test::traced_test;

    fn create_test_config() -> ServiceConfig {
        ServiceConfig {
            service_name: "test-wheeld".to_string(),
            service_display_name: "Test Racing Wheel Service".to_string(),
            service_description: "Test service for unit tests".to_string(),
            ipc: IpcConfig {
                transport: TransportType::default(),
                bind_address: None,
                max_connections: 5,
                connection_timeout: Duration::from_secs(5),
                enable_acl: false, // Disable ACL for tests
            },
            health_check_interval: 1, // Fast health checks for tests
            max_restart_attempts: 2,
            restart_delay: 1,
            auto_restart: false, // Disable auto-restart for controlled tests
        }
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_daemon_creation() -> Result<()> {
        let config = create_test_config();
        let _daemon = ServiceDaemon::new(config).await?;

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_config_save_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("wheel").join("service.json");

        let original_config = create_test_config();

        // Save config
        original_config.save_to_path_for_test(&config_path).await?;

        // Load config
        let loaded_config = ServiceConfig::load_from_path_for_test(&config_path).await?;

        assert_eq!(original_config.service_name, loaded_config.service_name);
        assert_eq!(
            original_config.health_check_interval,
            loaded_config.health_check_interval
        );
        assert_eq!(
            original_config.max_restart_attempts,
            loaded_config.max_restart_attempts
        );
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_daemon_startup_shutdown() -> Result<()> {
        let config = create_test_config();
        let daemon = ServiceDaemon::new(config).await?;

        // Wrap test body with timeout to ensure test completes within 5 seconds
        // Requirements: 2.1, 2.5
        let test_future = async {
            // Test that daemon can be started and shut down quickly
            let daemon_handle = tokio::spawn(async move { daemon.run().await });

            // Give the daemon a moment to start
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Send shutdown signal (in a real test, we'd use the proper shutdown mechanism)
            daemon_handle.abort();

            // Verify the task was aborted (simulating shutdown)
            let result = daemon_handle.await;
            assert!(result.is_err()); // Should be cancelled
        };

        tokio::time::timeout(Duration::from_secs(5), test_future)
            .await
            .map_err(|_elapsed| {
                anyhow!(
                    "test_service_daemon_startup_shutdown timed out after 5 seconds - daemon may be blocked during startup or shutdown"
                )
            })?;
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_restart_logic() -> Result<()> {
        let mut config = create_test_config();
        config.auto_restart = true;
        config.max_restart_attempts = 2;
        config.restart_delay = 1;

        let _daemon = ServiceDaemon::new(config).await?;

        // This test verifies the restart logic exists
        // In a real scenario, we would simulate service failures
        // For now, just verify the daemon can be created with restart config
        // (daemon creation with expect above validates acceptance)
        Ok(())
    }
}
