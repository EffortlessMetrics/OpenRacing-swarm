//! Fuzzes the Forza Horizon 4 and Forza Horizon 5 UDP telemetry packet normalizers.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_forza_horizon
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{
    ForzaHorizon4Adapter, ForzaHorizon5Adapter, TelemetryAdapter,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter4 = ForzaHorizon4Adapter::new();
    let _ = adapter4.normalize(data);
    let adapter5 = ForzaHorizon5Adapter::new();
    let _ = adapter5.normalize(data);
});
