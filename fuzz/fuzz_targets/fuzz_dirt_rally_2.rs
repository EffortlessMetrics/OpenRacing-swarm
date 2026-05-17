//! Fuzzes the DiRT Rally 2.0 UDP telemetry packet normalizer.
//!
//! Exercises parse_packet (via TelemetryAdapter::normalize) against arbitrary byte input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_dirt_rally_2
#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{DirtRally2Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = DirtRally2Adapter::new();
    let _ = adapter.normalize(data);
});
