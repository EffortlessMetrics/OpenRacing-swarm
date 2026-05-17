//! Fuzzes the rFactor 2 telemetry normalizer over its struct-layout raw byte path.
//!
//! The `normalize()` method casts raw bytes to `RF2VehicleTelemetry`; this target
//! ensures arbitrary bytes never cause panics or UB.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_rfactor2
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{RFactor2Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = RFactor2Adapter::new();
    let _ = adapter.normalize(data);
});
