//! Fuzzes the WRC Kylotonn UDP telemetry packet normalizer (WRC 9 and WRC 10).
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_wrc_kylotonn_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::wrc_kylotonn::WrcKylotonnVariant;
use openracing_telemetry_adapters::{TelemetryAdapter, WrcKylotonnAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter9 = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc9);
    let _ = adapter9.normalize(data);
    let adapter10 = WrcKylotonnAdapter::new(WrcKylotonnVariant::Wrc10);
    let _ = adapter10.normalize(data);
});
