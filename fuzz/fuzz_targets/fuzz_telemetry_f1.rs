//! Fuzzes all F1-game telemetry adapters: F1 (classic), F1 2025, and F1 Native.
//!
//! Feeds arbitrary bytes through every F1-family normalizer to ensure none of
//! the UDP packet parsers panic on truncated, oversized, or corrupted input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_telemetry_f1

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{
    F1_25Adapter, F1Adapter, F1ManagerAdapter, F1NativeAdapter, TelemetryAdapter,
};

fuzz_target!(|data: &[u8]| {
    // F1 classic UDP telemetry.
    let classic = F1Adapter::new();
    let _ = classic.normalize(data);

    // F1 25 (latest generation) UDP telemetry.
    let f1_25 = F1_25Adapter::new();
    let _ = f1_25.normalize(data);

    // F1 Native (shared memory) telemetry.
    let native = F1NativeAdapter::new();
    let _ = native.normalize(data);

    // F1 Manager (strategy game variant).
    let manager = F1ManagerAdapter::new();
    let _ = manager.normalize(data);
});
