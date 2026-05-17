//! SimHub UDP JSON bridge telemetry adapter crate.
//!
//! This crate provides the [SimHubAdapter] for receiving generic JSON
//! telemetry from **SimHub** (SHWotever) over UDP (default port 5555).
//!
//! # Protocol
//!
//! SimHub broadcasts normalised telemetry as UTF-8 JSON packets at ~60 Hz.
//! The adapter parses `SpeedMs`, `Rpm`, `Gear`, `Throttle`, `Brake`,
//! `Steer`, `LatAcc`, `LonAcc`, `FFBValue`, and `FuelPercent` fields.
//!
//! # Usage
//!
//! `rust,no_run
//! use racing_wheel_telemetry_simhub::SimHubAdapter;
//! use openracing_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = SimHubAdapter::new();
//! assert_eq!(adapter.game_id(), "simhub");
//! # Ok(())
//! # }
//! `

#![deny(static_mut_refs)]

pub use openracing_telemetry::{NormalizedTelemetry, TelemetryFrame};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::simhub::SimHubAdapter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_simhub_adapter_game_id() {
        let adapter = SimHubAdapter::new();
        assert_eq!(adapter.game_id(), "simhub");
    }

    #[test]
    fn test_simhub_adapter_update_rate() {
        let adapter = SimHubAdapter::new();
        assert!(adapter.expected_update_rate() > Duration::ZERO);
    }

    #[test]
    fn test_simhub_adapter_as_trait_object() {
        let adapter: Box<dyn TelemetryAdapter> = Box::new(SimHubAdapter::new());
        assert_eq!(adapter.game_id(), "simhub");
    }

    #[test]
    fn test_simhub_adapter_rejects_empty_data() {
        let adapter = SimHubAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }
}
