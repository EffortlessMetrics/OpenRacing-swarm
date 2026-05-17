//! WRC Generations telemetry adapter.
#![deny(static_mut_refs)]
pub use openracing_telemetry::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::wrc_generations::WrcGenerationsAdapter;
