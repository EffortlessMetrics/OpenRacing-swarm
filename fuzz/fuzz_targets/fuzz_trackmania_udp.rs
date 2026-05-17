//! Fuzzes the Trackmania OpenPlanet JSON UDP bridge parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_trackmania_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::trackmania::parse_trackmania_packet;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let _ = parse_trackmania_packet(data);
});
