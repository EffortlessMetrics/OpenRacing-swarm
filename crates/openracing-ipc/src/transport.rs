//! Transport abstraction for IPC

use std::time::Duration;

#[cfg(unix)]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::DEFAULT_TCP_PORT;

#[allow(unused_imports)]
use crate::error::IpcResult;

/// Transport type configuration
///
/// # Examples
///
/// ```
/// use openracing_ipc::TransportType;
///
/// // Create TCP transport with defaults
/// let tcp = TransportType::tcp();
/// assert!(tcp.description().contains("TCP"));
///
/// // TCP with custom address and port
/// let custom = TransportType::tcp_with_address("0.0.0.0", 8080);
/// assert!(custom.description().contains("8080"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    /// TCP transport (cross-platform)
    Tcp {
        /// Bind address
        address: String,
        /// Port number
        port: u16,
    },
    /// Unix Domain Socket (Unix only)
    #[cfg(unix)]
    UnixSocket {
        /// Socket file path
        socket_path: PathBuf,
    },
    /// Named Pipe (Windows only)
    #[cfg(windows)]
    NamedPipe {
        /// Pipe name (e.g., `\\.\pipe\wheel`)
        pipe_name: String,
    },
}

impl TransportType {
    /// Create a TCP transport on localhost with default port
    pub fn tcp() -> Self {
        TransportType::Tcp {
            address: "127.0.0.1".to_string(),
            port: DEFAULT_TCP_PORT,
        }
    }

    /// Create a TCP transport with custom address and port
    pub fn tcp_with_address(address: impl Into<String>, port: u16) -> Self {
        TransportType::Tcp {
            address: address.into(),
            port,
        }
    }

    /// Create a Unix Domain Socket transport
    #[cfg(unix)]
    pub fn unix_socket(path: impl Into<PathBuf>) -> Self {
        TransportType::UnixSocket {
            socket_path: path.into(),
        }
    }

    /// Create a Named Pipe transport
    #[cfg(windows)]
    pub fn named_pipe(name: impl Into<String>) -> Self {
        TransportType::NamedPipe {
            pipe_name: name.into(),
        }
    }

    /// Get the default transport for the current platform
    pub fn platform_default() -> Self {
        #[cfg(windows)]
        {
            TransportType::NamedPipe {
                pipe_name: r"\\.\pipe\openracing".to_string(),
            }
        }
        #[cfg(unix)]
        {
            let socket_path = user_runtime_socket_path(current_user_id());
            TransportType::UnixSocket { socket_path }
        }
    }

    /// Get a human-readable description of the transport
    pub fn description(&self) -> String {
        match self {
            TransportType::Tcp { address, port } => format!("TCP {}:{}", address, port),
            #[cfg(unix)]
            TransportType::UnixSocket { socket_path } => {
                format!("Unix socket {:?}", socket_path)
            }
            #[cfg(windows)]
            TransportType::NamedPipe { pipe_name } => format!("Named pipe {}", pipe_name),
        }
    }
}

#[cfg(unix)]
fn current_user_id() -> libc::uid_t {
    // SAFETY: getuid has no pointer arguments, no caller-provided buffers, and
    // no documented preconditions; it only reads the current process UID.
    unsafe { libc::getuid() }
}

#[cfg(unix)]
fn user_runtime_socket_path(uid: libc::uid_t) -> PathBuf {
    // Keep the platform default path derived from a numeric UID instead of
    // caller-provided path text.
    PathBuf::from(format!("/run/user/{uid}/openracing.sock"))
}

impl Default for TransportType {
    fn default() -> Self {
        Self::platform_default()
    }
}

/// Transport trait for platform-specific implementations
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Start listening for connections
    async fn listen(&mut self) -> IpcResult<()>;

    /// Stop listening and clean up resources
    async fn shutdown(&mut self) -> IpcResult<()>;

    /// Check if the transport is currently listening
    fn is_listening(&self) -> bool;

    /// Get the transport type
    fn transport_type(&self) -> TransportType;
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Transport type
    pub transport: TransportType,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Enable ACL restrictions
    pub enable_acl: bool,
    /// Receive buffer size
    pub recv_buffer_size: usize,
    /// Send buffer size
    pub send_buffer_size: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport: TransportType::default(),
            max_connections: 100,
            connection_timeout: Duration::from_secs(30),
            enable_acl: false,
            recv_buffer_size: 64 * 1024,
            send_buffer_size: 64 * 1024,
        }
    }
}

/// Transport builder for creating transports with configuration
///
/// # Examples
///
/// ```
/// use openracing_ipc::prelude::*;
/// use std::time::Duration;
///
/// let config = TransportBuilder::new()
///     .transport(TransportType::tcp())
///     .max_connections(50)
///     .connection_timeout(Duration::from_secs(10))
///     .enable_acl(true)
///     .build();
///
/// assert_eq!(config.max_connections, 50);
/// assert!(config.enable_acl);
/// ```
pub struct TransportBuilder {
    config: TransportConfig,
}

impl TransportBuilder {
    /// Create a new transport builder with default configuration
    pub fn new() -> Self {
        Self {
            config: TransportConfig::default(),
        }
    }

    /// Set the transport type
    pub fn transport(mut self, transport: TransportType) -> Self {
        self.config.transport = transport;
        self
    }

    /// Set the maximum connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.config.max_connections = max;
        self
    }

    /// Set the connection timeout
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Enable ACL restrictions
    pub fn enable_acl(mut self, enable: bool) -> Self {
        self.config.enable_acl = enable;
        self
    }

    /// Build the configuration
    pub fn build(self) -> TransportConfig {
        self.config
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_tcp() {
        let transport = TransportType::tcp();
        assert!(matches!(transport, TransportType::Tcp { .. }));
    }

    #[test]
    fn test_transport_type_description() {
        let transport = TransportType::tcp();
        assert!(transport.description().contains("TCP"));
    }

    #[cfg(unix)]
    #[test]
    fn test_unix_socket_path_uses_numeric_uid_runtime_dir() {
        let socket_path = user_runtime_socket_path(1000);
        assert_eq!(socket_path, PathBuf::from("/run/user/1000/openracing.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn test_platform_default_unix_socket_path_matches_current_uid() -> Result<(), String> {
        let expected = user_runtime_socket_path(current_user_id());
        let transport = TransportType::platform_default();

        let TransportType::UnixSocket { socket_path } = transport else {
            return Err(format!(
                "expected UnixSocket platform default, got {transport:?}"
            ));
        };
        assert_eq!(socket_path, expected);
        Ok(())
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.max_connections, 100);
        assert!(!config.enable_acl);
    }

    #[test]
    fn test_transport_builder() {
        let config = TransportBuilder::new()
            .max_connections(50)
            .connection_timeout(Duration::from_secs(10))
            .enable_acl(true)
            .build();

        assert_eq!(config.max_connections, 50);
        assert!(config.enable_acl);
    }
}
