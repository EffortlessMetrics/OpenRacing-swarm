//! Fuzzes the Assetto Corsa UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_assetto_corsa
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{AssettoCorsaAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = AssettoCorsaAdapter::new();
    let _ = adapter.normalize(data);
});
