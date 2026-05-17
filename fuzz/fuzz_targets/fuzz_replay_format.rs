//! Fuzzes the telemetry replay/recording file format parser.
//!
//! Treats arbitrary bytes as a serialized `TelemetryRecording` JSON document
//! and verifies that deserialization never panics on malformed input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_replay_format

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_recorder::TelemetryRecording;

fuzz_target!(|data: &[u8]| {
    // Attempt JSON deserialization from raw bytes.
    let _ = serde_json::from_slice::<TelemetryRecording>(data);

    // Also try from UTF-8 string (slightly different error path).
    if let Ok(text) = core::str::from_utf8(data) {
        let _ = serde_json::from_str::<TelemetryRecording>(text);
    }
});
