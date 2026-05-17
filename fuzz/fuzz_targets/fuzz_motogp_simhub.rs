//! Fuzzes the MotoGP 23/24 SimHub UDP bridge adapter normalizer.
//!
//! MotoGP uses the SimHub JSON UDP bridge on port 5556. This target ensures
//! the normalizer never panics on arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_motogp_simhub
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{MotoGPAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = MotoGPAdapter::new();
    let _ = adapter.normalize(data);
});
