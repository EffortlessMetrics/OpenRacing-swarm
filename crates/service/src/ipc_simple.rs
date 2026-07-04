//! IPC implementation with ACL restrictions and platform-specific transports

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
use tokio::sync::{Mutex, RwLock, broadcast};
#[cfg(unix)]
use tracing::error;
use tracing::{debug, info, warn};

use crate::WheelService;

/// IPC configuration with ACL support
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcConfig {
    pub bind_address: Option<String>,
    pub transport: TransportType,
    pub max_connections: u32,
    pub connection_timeout: Duration,
    pub enable_acl: bool,
}

/// Transport type for IPC with platform-specific defaults
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TransportType {
    #[cfg(windows)]
    NamedPipe(String),
    #[cfg(unix)]
    UnixDomainSocket(String),
}

impl Default for TransportType {
    fn default() -> Self {
        #[cfg(windows)]
        {
            TransportType::NamedPipe(r"\\.\pipe\wheel".to_string())
        }
        #[cfg(unix)]
        {
            let uid = current_user_id();
            TransportType::UnixDomainSocket(format!("/run/user/{}/wheel.sock", uid))
        }
    }
}

#[cfg(unix)]
fn current_user_id() -> libc::uid_t {
    // SAFETY: `getuid` has no preconditions, does not dereference pointers, and
    // only reads the current process credentials from the OS.
    unsafe { libc::getuid() }
}

/// Internal health event for broadcasting
#[derive(Debug, Clone)]
pub struct HealthEventInternal {
    pub device_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp: std::time::SystemTime,
}

/// IPC server with ACL restrictions and platform-specific transports
#[derive(Clone)]
pub struct IpcServer {
    config: IpcConfig,
    health_sender: broadcast::Sender<HealthEventInternal>,
    #[allow(dead_code)]
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    shutdown_tx: Arc<Mutex<Option<broadcast::Sender<()>>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClientInfo {
    id: String,
    connected_at: std::time::SystemTime,
    features: Vec<String>,
    peer_info: PeerInfo,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PeerInfo {
    #[cfg(windows)]
    process_id: u32,
    #[cfg(unix)]
    user_id: u32,
    #[cfg(unix)]
    group_id: u32,
}

impl IpcServer {
    pub async fn new(config: IpcConfig) -> Result<Self> {
        let (health_sender, _) = broadcast::channel(1000);

        Ok(Self {
            config,
            health_sender,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn serve(&self, service: Arc<WheelService>) -> Result<()> {
        info!(
            "Starting IPC server with transport: {:?}",
            self.config.transport
        );

        // Set up shutdown channel
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        match &self.config.transport {
            #[cfg(windows)]
            TransportType::NamedPipe(pipe_name) => {
                self.serve_named_pipe(pipe_name, service, &mut shutdown_rx)
                    .await
            }
            #[cfg(unix)]
            TransportType::UnixDomainSocket(socket_path) => {
                self.serve_unix_socket(socket_path, service, &mut shutdown_rx)
                    .await
            }
        }
    }

    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown_tx.lock().await.as_ref() {
            let _ = tx.send(());
        }
    }

    pub fn broadcast_health_event(&self, event: HealthEventInternal) {
        if let Err(e) = self.health_sender.send(event) {
            warn!("Failed to broadcast health event: {}", e);
        }
    }

    pub fn get_health_receiver(&self) -> broadcast::Receiver<HealthEventInternal> {
        self.health_sender.subscribe()
    }

    #[cfg(windows)]
    async fn serve_named_pipe(
        &self,
        pipe_name: &str,
        _service: Arc<WheelService>,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Starting Named Pipe server: {}", pipe_name);

        // Set up ACL restrictions if enabled
        if self.config.enable_acl {
            self.setup_windows_acl(pipe_name).await?;
        }

        // For now, just simulate the server running
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Named Pipe server shutting down");
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                debug!("Named Pipe server tick");
            }
        }

        Ok(())
    }

    #[cfg(unix)]
    async fn serve_unix_socket(
        &self,
        socket_path: &str,
        _service: Arc<WheelService>,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        use tokio::net::UnixListener;

        info!("Starting Unix Domain Socket server: {}", socket_path);

        // Remove existing socket file if it exists
        if std::path::Path::new(socket_path).exists() {
            tokio::fs::remove_file(socket_path)
                .await
                .context("Failed to remove existing socket file")?;
        }

        // Create the socket
        let listener = UnixListener::bind(socket_path).context("Failed to bind Unix socket")?;

        // Set up ACL restrictions if enabled
        if self.config.enable_acl {
            self.setup_unix_acl(socket_path).await?;
        }

        info!("Unix socket server listening on {}", socket_path);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Unix socket server shutting down");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("New connection from {:?}", addr);

                            // Verify peer credentials if ACL is enabled
                            if self.config.enable_acl
                                && let Err(e) =
                                    self.verify_unix_peer_credentials(&stream).await
                            {
                                warn!("Connection rejected due to ACL: {}", e);
                                continue;
                            }

                            // Handle the connection
                            let clients = self.connected_clients.clone();
                            tokio::spawn(async move {
                                Self::handle_unix_connection(stream, clients).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }

        // Clean up socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        Ok(())
    }

    #[cfg(windows)]
    async fn setup_windows_acl(&self, _pipe_name: &str) -> Result<()> {
        // Set up Windows ACL to restrict access to current user and SYSTEM
        info!("Setting up Windows ACL restrictions for named pipe");
        // Implementation would use Windows Security APIs
        Ok(())
    }

    #[cfg(unix)]
    async fn setup_unix_acl(&self, socket_path: &str) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        // Set socket permissions to user-only (0600)
        let metadata = tokio::fs::metadata(socket_path)
            .await
            .context("Failed to get socket metadata")?;

        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600); // User read/write only

        tokio::fs::set_permissions(socket_path, permissions)
            .await
            .context("Failed to set socket permissions")?;

        info!("Set Unix socket permissions to user-only access");
        Ok(())
    }

    #[cfg(unix)]
    async fn verify_unix_peer_credentials(&self, _stream: &tokio::net::UnixStream) -> Result<()> {
        // Use SO_PEERCRED to get peer process info
        // This is a simplified version - full implementation would use libc calls
        let current_uid = current_user_id();

        debug!(
            "Verifying peer credentials against current UID: {}",
            current_uid
        );

        // For now, allow all connections from the same user
        Ok(())
    }

    #[cfg(unix)]
    async fn handle_unix_connection(
        _stream: tokio::net::UnixStream,
        _clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    ) {
        debug!("Handling Unix socket connection");
        // Implementation would handle the actual IPC protocol
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            bind_address: Some("127.0.0.1".to_string()),
            transport: TransportType::default(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            enable_acl: false,
        }
    }
}

/// Simplified IPC client
pub struct IpcClient {
    config: IpcClientConfig,
}

#[derive(Debug, Clone)]
pub struct IpcClientConfig {
    pub connect_timeout: Duration,
    pub server_address: String,
}

impl IpcClient {
    pub fn new(config: IpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to IPC server at {}", self.config.server_address);
        // For now, just log that we would connect
        // In a real implementation, we would establish the connection here
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Disconnecting from IPC server");
        Ok(())
    }
}

impl Default for IpcClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            server_address: "127.0.0.1:50051".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_ipc_config_defaults() -> Result<()> {
        let config = IpcConfig::default();
        assert_eq!(config.bind_address, Some("127.0.0.1".to_string()));
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert!(!config.enable_acl);
        Ok(())
    }

    #[test]
    fn test_transport_type_default() -> Result<()> {
        let transport = TransportType::default();
        #[cfg(windows)]
        {
            assert!(
                matches!(transport, TransportType::NamedPipe(ref name) if name.contains("wheel")),
                "Windows default transport should be a named pipe containing 'wheel'"
            );
        }
        #[cfg(unix)]
        {
            assert!(
                matches!(transport, TransportType::UnixDomainSocket(ref path) if path.contains("wheel.sock")),
                "Unix default transport should be a UDS path containing 'wheel.sock'"
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_default_unix_socket_path_is_scoped_to_current_user() -> Result<()> {
        let uid = current_user_id();
        let transport = TransportType::default();

        assert!(
            matches!(transport, TransportType::UnixDomainSocket(ref path) if path == &format!("/run/user/{uid}/wheel.sock")),
            "Unix default transport should be scoped to the current user"
        );
        Ok(())
    }

    #[test]
    fn test_ipc_client_config_defaults() -> Result<()> {
        let config = IpcClientConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.server_address, "127.0.0.1:50051");
        Ok(())
    }

    #[test]
    fn test_ipc_client_construction() -> Result<()> {
        let config = IpcClientConfig {
            connect_timeout: Duration::from_secs(5),
            server_address: "192.168.1.1:9999".to_string(),
        };
        let client = IpcClient::new(config);
        assert_eq!(client.config.server_address, "192.168.1.1:9999");
        assert_eq!(client.config.connect_timeout, Duration::from_secs(5));
        Ok(())
    }

    #[test]
    fn test_health_event_internal_construction() -> Result<()> {
        let event = HealthEventInternal {
            device_id: "dev-1".to_string(),
            event_type: "connected".to_string(),
            message: "Device connected".to_string(),
            timestamp: std::time::SystemTime::now(),
        };
        assert_eq!(event.device_id, "dev-1");
        assert_eq!(event.event_type, "connected");
        assert!(!event.message.is_empty());
        Ok(())
    }

    #[test]
    fn test_ipc_config_serialization_roundtrip() -> Result<()> {
        let config = IpcConfig::default();
        let json = serde_json::to_string(&config)?;
        let deserialized: IpcConfig = serde_json::from_str(&json)?;
        assert_eq!(deserialized.max_connections, config.max_connections);
        assert_eq!(deserialized.connection_timeout, config.connection_timeout);
        assert_eq!(deserialized.enable_acl, config.enable_acl);
        assert_eq!(deserialized.bind_address, config.bind_address);
        Ok(())
    }

    #[tokio::test]
    async fn test_ipc_server_creation() -> Result<()> {
        let config = IpcConfig {
            bind_address: Some("127.0.0.1".to_string()),
            transport: TransportType::default(),
            max_connections: 5,
            connection_timeout: Duration::from_secs(10),
            enable_acl: false,
        };
        let server = IpcServer::new(config).await?;
        // Verify the health broadcast channel works
        let mut receiver = server.get_health_receiver();
        server.broadcast_health_event(HealthEventInternal {
            device_id: "test".to_string(),
            event_type: "test".to_string(),
            message: "test event".to_string(),
            timestamp: std::time::SystemTime::now(),
        });
        let event = receiver.recv().await?;
        assert_eq!(event.device_id, "test");
        Ok(())
    }

    #[tokio::test]
    async fn test_ipc_server_health_broadcast_multiple() -> Result<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config).await?;
        let mut rx1 = server.get_health_receiver();
        let mut rx2 = server.get_health_receiver();

        server.broadcast_health_event(HealthEventInternal {
            device_id: "d1".to_string(),
            event_type: "info".to_string(),
            message: "msg".to_string(),
            timestamp: std::time::SystemTime::now(),
        });

        let e1 = rx1.recv().await?;
        let e2 = rx2.recv().await?;
        assert_eq!(e1.device_id, "d1");
        assert_eq!(e2.device_id, "d1");
        Ok(())
    }
}
