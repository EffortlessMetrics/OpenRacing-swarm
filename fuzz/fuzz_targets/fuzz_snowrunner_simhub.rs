//! Fuzzes the SnowRunner SimHub UDP bridge adapter normalizer.
//!
//! SnowRunner uses the SimHub JSON UDP bridge on port 8877 (shared with
//! MudRunner via the MudRunnerVariant enum). This target ensures the
//! normalizer never panics on arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_snowrunner_simhub
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::TelemetryAdapter;
use openracing_telemetry_adapters::mudrunner::{MudRunnerAdapter, MudRunnerVariant};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let _ = adapter.normalize(data);
});
