//! Fuzzes the DiRT Showdown UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_dirt_showdown
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{DirtShowdownAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = DirtShowdownAdapter::new();
    let _ = adapter.normalize(data);
});
