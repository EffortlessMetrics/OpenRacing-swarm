//! Fuzzes the Wreckfest UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_wreckfest_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{TelemetryAdapter, WreckfestAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = WreckfestAdapter::new();
    let _ = adapter.normalize(data);
});
