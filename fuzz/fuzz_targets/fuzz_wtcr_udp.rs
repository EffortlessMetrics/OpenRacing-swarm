//! Fuzzes the WTCR (World Touring Car Cup) Codemasters Mode-1 UDP packet normalizer.
//!
//! WTCR uses the 252-byte Codemasters legacy binary packet on UDP port 6778.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_wtcr_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{TelemetryAdapter, WtcrAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = WtcrAdapter::new();
    let _ = adapter.normalize(data);
});
