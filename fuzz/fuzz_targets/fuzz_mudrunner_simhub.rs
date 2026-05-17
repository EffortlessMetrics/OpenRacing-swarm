//! Fuzzes the MudRunner SimHub UDP bridge adapter normalizer.
//!
//! MudRunner uses the SimHub JSON UDP bridge on port 8877. This target ensures
//! the normalizer never panics on arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_mudrunner_simhub
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{MudRunnerAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = MudRunnerAdapter::new();
    let _ = adapter.normalize(data);
});
