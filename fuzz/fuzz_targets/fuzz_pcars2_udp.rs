//! Fuzzes the Project CARS 2 UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_pcars2_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{PCars2Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = PCars2Adapter::new();
    let _ = adapter.normalize(data);
});
