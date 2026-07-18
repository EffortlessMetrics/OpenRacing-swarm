//! Racing Wheel Service - Complete system integration with graceful degradation
//!
//! Ref: [ADR-0002: IPC Transport](file:///h:/Code/Rust/OpenRacing/docs/adr/0002-ipc-transport.md)
//! Ref: [ADR-0008: Game Auto-Configure](file:///h:/Code/Rust/OpenRacing/docs/adr/0008-game-auto-configure-telemetry-bridge.md)

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

pub mod anticheat;
pub mod auto_profile_switching;
pub mod changelog;
#[cfg(test)]
mod changelog_property_tests;
pub mod config_validation;
pub mod config_writers;
pub mod control_broadcast;
pub mod control_input;
pub mod crypto;
pub mod daemon;
mod daemon_platform;
#[cfg(test)]
pub mod daemon_tests;
pub mod device;
pub mod device_service;
pub mod diagnostic_service;
pub mod game_auto_configure;
pub mod game_integration;
#[cfg(test)]
pub mod game_integration_e2e_tests;
pub mod game_integration_service;
#[cfg(test)]
pub mod game_integration_tests;
pub mod game_service;
pub mod game_support_matrix;
pub mod game_telemetry_bridge;
#[cfg(test)]
pub mod integration_tests;
pub mod ipc_service;
pub mod ipc_simple;
pub mod observability;
pub mod process_detection;
pub mod profile_repository;
#[cfg(test)]
pub mod profile_repository_test;
pub mod profile_service;
pub mod safety_service;
pub mod service;
pub mod service_tests;
pub mod system_config;
pub mod telemetry;
pub mod update;
#[cfg(test)]
pub mod wave15_hardening_tests;
pub mod windows_packaging;

pub use anticheat::AntiCheatReport;
pub use daemon::{ServiceConfig, ServiceDaemon};
pub use device::*;
pub use device_service::*;
pub use diagnostic_service::{DiagnosticResult, DiagnosticService, DiagnosticStatus};
pub use ipc_service::WheelServiceImpl;
pub use ipc_simple::{
    HealthEventInternal, IpcClient, IpcClientConfig, IpcConfig, IpcServer, TransportType,
};
pub use openracing_errors;
pub use profile_service::*;
pub use safety_service::*;
pub use service::*;
pub use system_config::{FeatureFlags, SystemConfig};

// Type aliases for application services to match expected naming
pub type ApplicationProfileService = profile_service::ProfileService;
pub type ApplicationDeviceService = device_service::ApplicationDeviceService;
pub type ApplicationSafetyService = safety_service::ApplicationSafetyService;
pub type ApplicationGameService = game_service::GameService;
pub use auto_profile_switching::*;
pub use config_validation::*;
pub use control_input::ControlInputCollector;
pub use game_integration_service::*;
pub use game_service::GameService;
pub use observability::*;
pub use process_detection::*;
pub use telemetry::*;
