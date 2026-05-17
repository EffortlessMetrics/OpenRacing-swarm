//! Fuzzes the AC Rally UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ac_rally
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{ACRallyAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = ACRallyAdapter::new();
    let _ = adapter.normalize(data);
});
