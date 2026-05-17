//! RaceRoom Experience telemetry adapter.
#![deny(static_mut_refs)]
pub use openracing_telemetry::{NormalizedTelemetry, TelemetryFrame};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::raceroom::RaceRoomAdapter;
