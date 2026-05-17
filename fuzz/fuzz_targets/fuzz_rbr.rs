//! Fuzzes the Richard Burns Rally LiveData UDP packet normalizer.
//!
//! Exercises parse_rbr_packet (via TelemetryAdapter::normalize) against arbitrary byte input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_rbr
#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{RBRAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = RBRAdapter::new();
    let _ = adapter.normalize(data);
});
