//! Fuzzes the Automobilista 1 telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_automobilista
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{Automobilista1Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = Automobilista1Adapter::new();
    let _ = adapter.normalize(data);
});
