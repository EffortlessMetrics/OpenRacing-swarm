//! Fuzzes the F1 Manager telemetry UDP packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_f1_manager

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{F1ManagerAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = F1ManagerAdapter::new();
    let _ = adapter.normalize(data);
});
