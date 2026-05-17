//! Fuzzes the Dirt 4 UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_dirt4_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{Dirt4Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = Dirt4Adapter::new();
    let _ = adapter.normalize(data);
});
