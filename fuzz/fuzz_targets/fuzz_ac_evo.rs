//! Fuzzes the Assetto Corsa Evo telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ac_evo

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{ACEvoAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = ACEvoAdapter::new();
    let _ = adapter.normalize(data);
});
