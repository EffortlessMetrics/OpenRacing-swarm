//! Fuzzes the F1 native telemetry adapter normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_f1_native
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{F1NativeAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = F1NativeAdapter::new();
    let _ = adapter.normalize(data);
});
