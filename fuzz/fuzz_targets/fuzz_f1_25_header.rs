//! Fuzzes the F1 25 packet header parser.
//!
//! Run with:
//!   cargo fuzz run fuzz_f1_25_header

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::f1_25::parse_header;

fuzz_target!(|data: &[u8]| {
    // Must never panic — errors are acceptable, panics are not.
    let _ = parse_header(data);
});
