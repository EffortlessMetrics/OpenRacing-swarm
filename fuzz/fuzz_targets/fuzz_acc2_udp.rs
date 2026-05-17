//! Fuzzes the ACC 2 (Assetto Corsa Competizione 2) UDP packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_acc2_udp

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{ACC2Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = ACC2Adapter::new();
    let _ = adapter.normalize(data);
});
