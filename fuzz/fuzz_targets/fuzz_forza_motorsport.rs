//! Fuzzes the Forza Motorsport Sled and CarDash UDP telemetry packet normalizers.
//!
//! Covers the 232-byte Sled format (FM7 and earlier) and the 311-byte CarDash format
//! (FM8, FH4+). Both use little-endian f32 fields.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_forza_motorsport
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{ForzaAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = ForzaAdapter::new();
    let _ = adapter.normalize(data);
});
