//! Fuzzes the Seb Loeb Rally Evo telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_seb_loeb_rally

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{SebLoebRallyAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = SebLoebRallyAdapter::new();
    let _ = adapter.normalize(data);
});
