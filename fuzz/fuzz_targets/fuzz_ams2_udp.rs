//! Fuzzes the AMS2 (Automobilista 2) shared memory telemetry normalizer.
//!
//! AMS2 uses the Project CARS 2 shared memory format. The `normalize` method
//! accepts raw bytes and reinterprets them as the AMS2SharedMemory struct.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ams2_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{AMS2Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = AMS2Adapter::new();
    let _ = adapter.normalize(data);
});
