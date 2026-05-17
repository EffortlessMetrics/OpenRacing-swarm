//! Fuzzes the Codemasters custom UDP packet decoder.
//!
//! Exercises all built-in mode specs (0–3) against arbitrary byte input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_codemasters_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::CustomUdpSpec;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    // Use first byte to select a mode, then fuzz the rest as packet payload.
    let mode = data[0] % 4;
    let payload = &data[1..];
    let spec = CustomUdpSpec::from_mode(mode);
    let _ = spec.decode(payload);
});
