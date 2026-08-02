//! Plugin Registry Service Interface
//!
//! This module defines the service-level trait for plugin registry operations,
//! providing a high-level API for searching, downloading, installing, and
//! managing plugins from remote registries.
//!
//! The service layer provides:
//! - Service-level caching (both in-memory and disk)
//! - Progress callbacks for downloads
//! - Offline mode support with graceful degradation
//! - Plugin signature verification using the trust store
//! - Installed plugin management
//!
//! Note: This module defines its own types to avoid circular dependencies
//! with the plugins crate. These types mirror the plugins crate types and
//! can be converted where needed.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::TrustLevel;

/// Errors that can occur during plugin registry operations
#[derive(Error, Debug)]
pub enum PluginRegistryError {
    /// Plugin not found in registry
    #[error("Plugin not found: {plugin_id}")]
    PluginNotFound { plugin_id: String },

    /// Plugin version not found
    #[error("Plugin version not found: {plugin_id} v{version}")]
    VersionNotFound { plugin_id: String, version: String },

    /// Registry is offline and no cached data is available
    #[error("Registry is offline and no cached data is available")]
    RegistryOffline,

    /// Network error during registry operation
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Plugin signature verification failed
    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Plugin is unsigned but signatures are required
    #[error("Plugin is unsigned but signatures are required")]
    UnsignedPlugin,

    /// Plugin installation failed
    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    /// Plugin uninstallation failed
    #[error("Uninstallation failed: {0}")]
    UninstallationFailed(String),

    /// IO error during plugin operation
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Invalid plugin path
    #[error("Invalid plugin path: {0}")]
    InvalidPath(String),

    /// Registry refresh failed
    #[error("Registry refresh failed: {0}")]
    RefreshFailed(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Result type for plugin registry operations
pub type PluginRegistryResult<T> = Result<T, PluginRegistryError>;

/// Unique identifier for a plugin in the registry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub Uuid);

impl PluginId {
    /// Create a new random plugin ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a plugin ID from an existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for PluginId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Plugin capability type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCapability {
    /// Read telemetry data
    ReadTelemetry,
    /// Write FFB output
    WriteFfb,
    /// Access LED control
    LedControl,
    /// Access file system (sandboxed)
    FileSystem,
    /// Network access (sandboxed)
    Network,
    /// Access to device configuration
    DeviceConfig,
}

/// Plugin metadata for registry
///
/// Contains all information about a plugin that is stored in the registry,
/// including identification, authorship, versioning, and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique identifier for this plugin
    pub id: PluginId,
    /// Human-readable name of the plugin
    pub name: String,
    /// Semantic version of the plugin
    pub version: Version,
    /// Author or organization that created the plugin
    pub author: String,
    /// Description of what the plugin does
    pub description: String,
    /// Optional homepage URL for the plugin
    pub homepage: Option<String>,
    /// License identifier (e.g., "MIT", "Apache-2.0")
    pub license: String,
    /// List of capabilities this plugin requires
    pub capabilities: Vec<PluginCapability>,
    /// Optional Ed25519 signature fingerprint for verification
    pub signature_fingerprint: Option<String>,
    /// Download URL for the plugin package
    pub download_url: Option<String>,
    /// SHA256 hash of the plugin package
    pub package_hash: Option<String>,
}

impl PluginMetadata {
    /// Create a new PluginMetadata with required fields
    pub fn new(
        name: impl Into<String>,
        version: Version,
        author: impl Into<String>,
        description: impl Into<String>,
        license: impl Into<String>,
    ) -> Self {
        Self {
            id: PluginId::new(),
            name: name.into(),
            version,
            author: author.into(),
            description: description.into(),
            homepage: None,
            license: license.into(),
            capabilities: Vec::new(),
            signature_fingerprint: None,
            download_url: None,
            package_hash: None,
        }
    }

    /// Builder method to set homepage
    pub fn with_homepage(mut self, homepage: impl Into<String>) -> Self {
        self.homepage = Some(homepage.into());
        self
    }

    /// Builder method to set capabilities
    pub fn with_capabilities(mut self, capabilities: Vec<PluginCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Builder method to set signature fingerprint
    pub fn with_signature_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.signature_fingerprint = Some(fingerprint.into());
        self
    }

    /// Builder method to set download URL
    pub fn with_download_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = Some(url.into());
        self
    }

    /// Validate that the metadata has all required non-empty fields
    pub fn validate(&self) -> PluginRegistryResult<()> {
        if self.name.is_empty() {
            return Err(PluginRegistryError::ValidationError(
                "Plugin name cannot be empty".to_string(),
            ));
        }
        if self.author.is_empty() {
            return Err(PluginRegistryError::ValidationError(
                "Plugin author cannot be empty".to_string(),
            ));
        }
        if self.description.is_empty() {
            return Err(PluginRegistryError::ValidationError(
                "Plugin description cannot be empty".to_string(),
            ));
        }
        if self.license.is_empty() {
            return Err(PluginRegistryError::ValidationError(
                "Plugin license cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Information about an installed plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin metadata
    pub metadata: PluginMetadata,

    /// Installation path
    pub install_path: PathBuf,

    /// Installation timestamp
    pub installed_at: chrono::DateTime<chrono::Utc>,

    /// Whether the plugin is currently enabled
    pub enabled: bool,

    /// Trust level of the plugin's signature
    pub trust_level: Option<TrustLevel>,
}

/// Progress information for download operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Total bytes to download (if known)
    pub total_bytes: Option<u64>,

    /// Bytes downloaded so far
    pub downloaded_bytes: u64,

    /// Download speed in bytes per second
    pub bytes_per_second: u64,

    /// Estimated time remaining in seconds (if calculable)
    pub eta_seconds: Option<u64>,

    /// Current phase of the download
    pub phase: DownloadPhase,
}

/// Phase of a download operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadPhase {
    /// Connecting to the registry
    Connecting,
    /// Downloading plugin data
    Downloading,
    /// Verifying downloaded content
    Verifying,
    /// Extracting plugin files
    Extracting,
    /// Download complete
    Complete,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            total_bytes: None,
            downloaded_bytes: 0,
            bytes_per_second: 0,
            eta_seconds: None,
            phase: DownloadPhase::Connecting,
        }
    }
}

/// Plugin registry service statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRegistryStatistics {
    /// Number of installed plugins
    pub installed_count: usize,

    /// Number of plugins with available updates
    pub updates_available: usize,

    /// Number of plugins in the registry cache
    pub cached_plugins: usize,

    /// Whether the registry is currently online
    pub registry_online: bool,

    /// Last successful registry refresh timestamp
    pub last_refresh: Option<chrono::DateTime<chrono::Utc>>,

    /// Total size of installed plugins in bytes
    pub installed_size_bytes: u64,

    /// Number of signed plugins installed
    pub signed_plugins: usize,
}

/// Callback type for download progress updates
pub type ProgressCallback = Box<dyn Fn(DownloadProgress) + Send + Sync>;

/// Plugin Registry Service Trait
///
/// Provides a high-level interface for plugin registry operations including
/// searching, downloading, installing, and managing plugins.
///
/// This trait is designed to be implemented by services that need to interact
/// with the plugin registry system at a higher level than the raw registry client.
#[async_trait]
pub trait PluginRegistryService: Send + Sync {
    /// Search for plugins matching a query string
    ///
    /// Searches plugin names and descriptions (case-insensitive).
    /// Results are returned sorted by relevance.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    ///
    /// # Returns
    ///
    /// A list of plugin metadata matching the query
    async fn search_plugins(&self, query: &str) -> PluginRegistryResult<Vec<PluginMetadata>>;

    /// Get plugin metadata by ID and optional version
    ///
    /// If version is None, returns the latest version.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier
    /// * `version` - Optional specific version to retrieve
    ///
    /// # Returns
    ///
    /// Plugin metadata if found
    async fn get_plugin(
        &self,
        id: &PluginId,
        version: Option<&Version>,
    ) -> PluginRegistryResult<PluginMetadata>;

    /// Download a plugin to the specified path
    ///
    /// Downloads the plugin package and verifies its integrity.
    /// The download can be monitored using an optional progress callback.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier
    /// * `version` - Specific version to download
    /// * `path` - Target directory for the download
    ///
    /// # Returns
    ///
    /// Path to the downloaded plugin package
    async fn download_plugin(
        &self,
        id: &PluginId,
        version: &Version,
        path: &Path,
    ) -> PluginRegistryResult<PathBuf>;

    /// Download a plugin with progress callback
    ///
    /// Same as `download_plugin` but with progress updates.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier
    /// * `version` - Specific version to download
    /// * `path` - Target directory for the download
    /// * `progress_callback` - Callback for progress updates
    ///
    /// # Returns
    ///
    /// Path to the downloaded plugin package
    async fn download_plugin_with_progress(
        &self,
        id: &PluginId,
        version: &Version,
        path: &Path,
        progress_callback: ProgressCallback,
    ) -> PluginRegistryResult<PathBuf>;

    /// Verify a plugin's signature
    ///
    /// Verifies the Ed25519 signature of a plugin package against
    /// the configured trust store.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the plugin package
    ///
    /// # Returns
    ///
    /// `true` if the signature is valid, `false` if unsigned or invalid
    async fn verify_plugin_signature(&self, path: &Path) -> PluginRegistryResult<bool>;

    /// Install a plugin from a downloaded package
    ///
    /// Extracts and installs the plugin to the configured plugin directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the plugin package
    ///
    /// # Returns
    ///
    /// The installed plugin's ID
    async fn install_plugin(&self, path: &Path) -> PluginRegistryResult<PluginId>;

    /// List all installed plugins
    ///
    /// # Returns
    ///
    /// List of installed plugin information
    async fn list_installed_plugins(&self) -> PluginRegistryResult<Vec<InstalledPlugin>>;

    /// Uninstall a plugin
    ///
    /// Removes the plugin from the system.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier to uninstall
    async fn uninstall_plugin(&self, id: &PluginId) -> PluginRegistryResult<()>;

    /// Refresh the registry index
    ///
    /// Forces a refresh of the registry index from the remote server.
    async fn refresh_registry(&self) -> PluginRegistryResult<()>;

    /// Check if the registry is online
    ///
    /// # Returns
    ///
    /// `true` if the registry is reachable
    async fn is_registry_online(&self) -> bool;

    /// Get registry service statistics
    ///
    /// # Returns
    ///
    /// Current statistics about the registry service
    async fn get_statistics(&self) -> PluginRegistryResult<PluginRegistryStatistics>;

    /// Check for updates to installed plugins
    ///
    /// Returns a list of plugins that have newer versions available.
    ///
    /// # Returns
    ///
    /// List of (installed plugin, available version) pairs
    async fn check_for_updates(&self) -> PluginRegistryResult<Vec<(InstalledPlugin, Version)>>;

    /// Get a plugin's installation status
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier
    ///
    /// # Returns
    ///
    /// `Some(InstalledPlugin)` if installed, `None` otherwise
    async fn get_installed_plugin(
        &self,
        id: &PluginId,
    ) -> PluginRegistryResult<Option<InstalledPlugin>>;

    /// Enable or disable a plugin
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin identifier
    /// * `enabled` - Whether the plugin should be enabled
    async fn set_plugin_enabled(&self, id: &PluginId, enabled: bool) -> PluginRegistryResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry_error_display() {
        let err = PluginRegistryError::PluginNotFound {
            plugin_id: "test-plugin".to_string(),
        };
        assert!(err.to_string().contains("test-plugin"));

        let err = PluginRegistryError::VersionNotFound {
            plugin_id: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
        };
        assert!(err.to_string().contains("1.0.0"));
    }

    #[test]
    fn test_download_progress_default() {
        let progress = DownloadProgress::default();
        assert_eq!(progress.downloaded_bytes, 0);
        assert!(progress.total_bytes.is_none());
        assert_eq!(progress.phase, DownloadPhase::Connecting);
    }

    #[test]
    fn test_plugin_registry_statistics_default() {
        let stats = PluginRegistryStatistics::default();
        assert_eq!(stats.installed_count, 0);
        assert_eq!(stats.updates_available, 0);
        assert!(!stats.registry_online);
    }

    #[test]
    fn test_download_phase_equality() {
        assert_eq!(DownloadPhase::Connecting, DownloadPhase::Connecting);
        assert_ne!(DownloadPhase::Connecting, DownloadPhase::Downloading);
    }

    #[test]
    fn test_plugin_id_creation() {
        let id1 = PluginId::new();
        let id2 = PluginId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_plugin_id_display() {
        let id = PluginId::new();
        let display = id.to_string();
        assert!(!display.is_empty());
    }

    #[test]
    fn test_plugin_metadata_creation() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        );
        assert_eq!(metadata.name, "Test Plugin");
        assert_eq!(metadata.version, Version::new(1, 0, 0));
        assert_eq!(metadata.author, "Test Author");
        assert_eq!(metadata.license, "MIT");
    }

    #[test]
    fn test_plugin_metadata_builder() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        )
        .with_homepage("https://example.com")
        .with_capabilities(vec![PluginCapability::ReadTelemetry])
        .with_signature_fingerprint("abc123");

        assert_eq!(metadata.homepage, Some("https://example.com".to_string()));
        assert_eq!(metadata.capabilities.len(), 1);
        assert_eq!(metadata.signature_fingerprint, Some("abc123".to_string()));
    }

    #[test]
    fn test_plugin_metadata_validation_success() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        );
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn test_plugin_metadata_validation_empty_name() {
        let metadata = PluginMetadata::new(
            "",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        );
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
                .contains("name")
        );
    }

    #[test]
    fn test_plugin_metadata_validation_empty_author() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "",
            "A test plugin",
            "MIT",
        );
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
                .contains("author")
        );
    }

    #[test]
    fn test_plugin_metadata_validation_empty_description() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "",
            "MIT",
        );
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
                .contains("description")
        );
    }

    #[test]
    fn test_plugin_metadata_validation_empty_license() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "",
        );
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
                .contains("license")
        );
    }

    #[test]
    fn test_installed_plugin_creation() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        );

        let installed = InstalledPlugin {
            metadata,
            install_path: PathBuf::from("/test/path"),
            installed_at: chrono::Utc::now(),
            enabled: true,
            trust_level: Some(TrustLevel::Trusted),
        };

        assert!(installed.enabled);
        assert_eq!(installed.trust_level, Some(TrustLevel::Trusted));
    }

    #[test]
    fn test_plugin_capability_variants() {
        let caps = [
            PluginCapability::ReadTelemetry,
            PluginCapability::WriteFfb,
            PluginCapability::LedControl,
            PluginCapability::FileSystem,
            PluginCapability::Network,
            PluginCapability::DeviceConfig,
        ];
        assert_eq!(caps.len(), 6);
    }

    #[test]
    fn test_plugin_metadata_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 2, 3),
            "Test Author",
            "A test plugin",
            "MIT",
        )
        .with_capabilities(vec![PluginCapability::ReadTelemetry]);

        let json = serde_json::to_string(&metadata)?;
        let restored: PluginMetadata = serde_json::from_str(&json)?;

        assert_eq!(restored.name, metadata.name);
        assert_eq!(restored.version, metadata.version);
        assert_eq!(restored.capabilities.len(), 1);

        Ok(())
    }

    #[test]
    fn test_installed_plugin_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            Version::new(1, 0, 0),
            "Test Author",
            "A test plugin",
            "MIT",
        );

        let installed = InstalledPlugin {
            metadata,
            install_path: PathBuf::from("/test/path"),
            installed_at: chrono::Utc::now(),
            enabled: true,
            trust_level: Some(TrustLevel::Trusted),
        };

        let json = serde_json::to_string(&installed)?;
        let restored: InstalledPlugin = serde_json::from_str(&json)?;

        assert_eq!(restored.enabled, installed.enabled);
        assert_eq!(restored.metadata.name, "Test Plugin");

        Ok(())
    }
}
