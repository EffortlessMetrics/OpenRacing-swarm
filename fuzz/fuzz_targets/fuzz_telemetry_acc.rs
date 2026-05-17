//! Fuzzes the ACC (Assetto Corsa Competizione) telemetry parsing: both
//! ACC 1 and ACC 2 UDP adapters, plus the base Assetto Corsa adapter.
//!
//! Feeds arbitrary bytes through all ACC-family normalizers to catch panics
//! on malformed shared-memory snapshots and UDP broadcast packets.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_telemetry_acc

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{
    ACC2Adapter, ACCAdapter, ACEvoAdapter, AssettoCorsaAdapter, TelemetryAdapter,
};

fuzz_target!(|data: &[u8]| {
    // ACC (Assetto Corsa Competizione) UDP broadcast.
    let acc = ACCAdapter::new();
    let _ = acc.normalize(data);

    // ACC 2 UDP broadcast.
    let acc2 = ACC2Adapter::new();
    let _ = acc2.normalize(data);

    // Assetto Corsa Evo.
    let evo = ACEvoAdapter::new();
    let _ = evo.normalize(data);

    // Assetto Corsa (original).
    let ac = AssettoCorsaAdapter::new();
    let _ = ac.normalize(data);
});
