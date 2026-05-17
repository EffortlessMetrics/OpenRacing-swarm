//! Fuzzes the F1 25 CarTelemetry packet parser.
//!
//! Run with:
//!   cargo fuzz run fuzz_f1_25_car_telemetry

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::f1_25::parse_car_telemetry;

fuzz_target!(|data: &[u8]| {
    // parse_car_telemetry takes (data, player_index: usize).
    // Vary player_index across the valid range (0..22) using the first byte of
    // the fuzz input so libFuzzer explores both dimensions.
    let player_index = if data.is_empty() {
        0usize
    } else {
        (data[0] % 22) as usize
    };
    let _ = parse_car_telemetry(data, player_index);
});
