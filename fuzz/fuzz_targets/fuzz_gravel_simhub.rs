//! Fuzzes the Gravel SimHub UDP bridge adapter normalizer.
//!
//! Gravel (Milestone, 2018) uses the SimHub JSON UDP bridge on port 5555.
//! This target ensures the normalizer never panics on arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_gravel_simhub
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{GravelAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = GravelAdapter::new();
    let _ = adapter.normalize(data);
});
