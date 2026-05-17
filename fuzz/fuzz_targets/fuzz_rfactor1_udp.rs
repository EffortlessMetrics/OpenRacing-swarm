//! Fuzzes the rFactor 1 UDP telemetry packet normalizer.
//!
//! Exercises all four rFactor1 variants (rFactor1, GTR2, RACE07, GSC).
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_rfactor1_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::rfactor1::RFactor1Variant;
use openracing_telemetry_adapters::{RFactor1Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    for variant in [
        RFactor1Variant::RFactor1,
        RFactor1Variant::Gtr2,
        RFactor1Variant::Race07,
        RFactor1Variant::GameStockCar,
    ] {
        let adapter = RFactor1Adapter::with_variant(variant);
        let _ = adapter.normalize(data);
    }
});
