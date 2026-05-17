//! Fuzzes the F1 25 CarStatus packet parser.
//!
//! Run with:
//!   cargo fuzz run fuzz_f1_25_car_status

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::f1_25::parse_car_status;

fuzz_target!(|data: &[u8]| {
    let player_index = if data.is_empty() {
        0usize
    } else {
        (data[0] % 22) as usize
    };
    let _ = parse_car_status(data, player_index);
});
