//! Fuzzes the V-Rally 4 UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_v_rally_4_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{TelemetryAdapter, VRally4Adapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = VRally4Adapter::new();
    let _ = adapter.normalize(data);
});
