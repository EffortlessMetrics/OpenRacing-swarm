//! MudRunner / SnowRunner SimHub UDP bridge telemetry adapter crate.
//!
//! This crate provides the [MudRunnerAdapter] and [MudRunnerVariant] for
//! receiving telemetry from **MudRunner** and **SnowRunner** via a SimHub
//! UDP JSON bridge (port 8877).
//!
//! # Usage
//!
//! `rust,no_run
//! use racing_wheel_telemetry_mudrunner::{MudRunnerAdapter, MudRunnerVariant};
//! use openracing_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
//! assert_eq!(adapter.game_id(), "snowrunner");
//! # Ok(())
//! # }
//! `

#![deny(static_mut_refs)]

pub use openracing_telemetry::{NormalizedTelemetry, TelemetryFrame};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::mudrunner::{MudRunnerAdapter, MudRunnerVariant};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_mudrunner_adapter_game_id() {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::MudRunner);
        assert_eq!(adapter.game_id(), "mudrunner");
    }

    #[test]
    fn test_snowrunner_adapter_game_id() {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
        assert_eq!(adapter.game_id(), "snowrunner");
    }

    #[test]
    fn test_mudrunner_adapter_update_rate() {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::MudRunner);
        assert!(adapter.expected_update_rate() > Duration::ZERO);
    }

    #[test]
    fn test_mudrunner_adapter_as_trait_object() {
        let adapter: Box<dyn TelemetryAdapter> =
            Box::new(MudRunnerAdapter::with_variant(MudRunnerVariant::MudRunner));
        assert_eq!(adapter.game_id(), "mudrunner");
    }

    #[test]
    fn test_mudrunner_adapter_rejects_empty_data() {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::MudRunner);
        assert!(adapter.normalize(&[]).is_err());
    }
}
