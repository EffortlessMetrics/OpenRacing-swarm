//! Fuzzes the RIDE 5 SimHub UDP bridge adapter normalizer.
//!
//! RIDE 5 (Milestone) uses the SimHub JSON UDP bridge on port 5558. This
//! target ensures the normalizer never panics on arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ride5_simhub
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{Ride5Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = Ride5Adapter::new();
    let _ = adapter.normalize(data);
});
