//! Fuzzes the Gran Turismo Sport UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_gran_turismo_sport
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{GranTurismo7SportsAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = GranTurismo7SportsAdapter::new();
    let _ = adapter.normalize(data);
});
