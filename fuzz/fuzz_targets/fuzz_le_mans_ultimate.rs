//! Fuzzes the Le Mans Ultimate UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_le_mans_ultimate
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{LeMansUltimateAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = LeMansUltimateAdapter::new();
    let _ = adapter.normalize(data);
});
