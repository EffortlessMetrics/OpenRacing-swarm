//! Fuzzes the Gran Turismo 7 UDP packet decryptor and normalizer.
//!
//! Exercises Salsa20 decryption + packet parsing against arbitrary byte input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_gran_turismo_7
#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{GranTurismo7Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = GranTurismo7Adapter::new();
    let _ = adapter.normalize(data);
});
