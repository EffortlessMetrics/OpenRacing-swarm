//! Fuzzes the NASCAR 21 UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_nascar21_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{Nascar21Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = Nascar21Adapter::new();
    let _ = adapter.normalize(data);
});
