//! Fuzzes the ACC (Assetto Corsa Competizione) UDP packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_acc_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{ACCAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = ACCAdapter::new();
    let _ = adapter.normalize(data);
});
