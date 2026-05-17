//! Fuzzes the iRacing telemetry normalizer over its struct-layout raw byte path.
//!
//! The `normalize()` method casts raw bytes to `IRacingData`/`IRacingLegacyData`
//! structs; this target ensures arbitrary bytes never cause panics or UB.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_iracing
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{IRacingAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = IRacingAdapter::new();
    let _ = adapter.normalize(data);
});
