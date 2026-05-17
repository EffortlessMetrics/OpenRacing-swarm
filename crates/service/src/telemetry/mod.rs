pub mod game_telemetry;
pub mod normalized;
pub mod rate_limiter;
pub mod recorder;

#[cfg(test)]
mod disconnection_property_tests;
#[cfg(test)]
mod telemetry_property_tests;

pub use adapters::{
    ACCAdapter, ACRallyAdapter, AMS2Adapter, Dirt5Adapter, EAWRCAdapter, F1Adapter, IRacingAdapter,
    MockAdapter, RFactor2Adapter, TelemetryAdapter, TelemetryReceiver, telemetry_now_ns,
};
pub use game_telemetry::*;
pub use openracing_telemetry::TelemetryService;
pub use openracing_telemetry_adapters as adapters;
pub use recorder::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_now_ns_is_monotonic() {
        let first = telemetry_now_ns();
        let second = telemetry_now_ns();
        assert!(second >= first);
    }
}
