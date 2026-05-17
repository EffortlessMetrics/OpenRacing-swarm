//! F1 2023/2024 native UDP telemetry adapter crate.
//!
//! This crate provides the [`F1NativeAdapter`] for receiving telemetry from
//! **EA F1 23** and **EA F1 24** via the Codemasters binary UDP protocol.
//!
//! # Protocol
//!
//! Both F1 23 (packet format `2023`) and F1 24 (packet format `2024`) send
//! little-endian binary UDP packets on port **20777** by default.  The packet
//! format is auto-detected from each packet header.
//!
//! ## Key packet types
//!
//! | Packet ID | Name          | Fields extracted                           |
//! |-----------|---------------|--------------------------------------------|
//! | 1         | Session        | track ID, session type, temperatures       |
//! | 6         | Car Telemetry  | speed (km/hΓåÆm/s), gear, RPM, throttle,    |
//! |           |               | brake, steer, DRS, tyre pressures/temps    |
//! | 7         | Car Status     | fuel (kg), ERS (J), pit limiter,          |
//! |           |               | tyre compound, traction control, ABS       |
//!
//! ## Protocol differences between F1 23 and F1 24
//!
//! The header and CarTelemetry layouts are **identical** in both versions.
//! CarStatusData differs:
//! - F1 23: 47 bytes per car ΓÇö no engine-power fields.
//! - F1 24: 55 bytes per car ΓÇö adds `enginePowerICE` and `enginePowerMGUK`.
//!
//! # Usage
//!
//! ```rust,no_run
//! use racing_wheel_telemetry_f1::F1NativeAdapter;
//! use openracing_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = F1NativeAdapter::new();
//! assert_eq!(adapter.game_id(), "f1_native");
//! # Ok(())
//! # }
//! ```

#![deny(static_mut_refs)]

pub use openracing_telemetry::{NormalizedTelemetry, TelemetryFrame};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::f1::F1NativeAdapter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_f1_adapter_game_id() {
        let adapter = F1NativeAdapter::new();
        assert_eq!(adapter.game_id(), "f1_native");
    }

    #[test]
    fn test_f1_adapter_update_rate() {
        let adapter = F1NativeAdapter::new();
        assert!(adapter.expected_update_rate() > Duration::ZERO);
    }

    #[test]
    fn test_f1_adapter_as_trait_object() {
        let adapter: Box<dyn TelemetryAdapter> = Box::new(F1NativeAdapter::new());
        assert_eq!(adapter.game_id(), "f1_native");
    }

    #[test]
    fn test_f1_adapter_rejects_empty_data() {
        let adapter = F1NativeAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }
}
