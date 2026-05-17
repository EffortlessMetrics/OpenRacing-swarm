//! Snapshot tests for Assetto Corsa EVO telemetry adapter (stub).
//!
//! AC EVO has no public telemetry API yet; `normalize` always returns
//! `NormalizedTelemetry::default()`.  These snapshots lock that behaviour so
//! any future protocol implementation will surface as a snapshot diff.
//!
//! Two scenarios: normal race pace input, and pit-stop / idle input.

use openracing_telemetry_adapters::{ACEvoAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Scenario 1: Normal race pace ───────────────────────────────────────────
// Simulated raw packet representing race conditions.  Because the adapter is a
// stub, the exact bytes do not matter — we verify the stub returns defaults.

#[test]
fn ac_evo_normal_race_pace_snapshot() -> TestResult {
    let raw: Vec<u8> = vec![
        0x01, 0x00, 0x00, 0x00, // hypothetical header / message type
        0x00, 0x00, 0xDC, 0x42, // 110.0 speed placeholder (f32 LE)
        0x00, 0x80, 0xBB, 0x45, // 6000.0 rpm placeholder (f32 LE)
        0x05, // gear = 5
        0x00, 0x00, 0x00, 0x00, // padding
    ];
    let adapter = ACEvoAdapter::new();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Pit stop / idle ────────────────────────────────────────────
// Empty payload — car stationary in pits, engine off.

#[test]
fn ac_evo_pit_stop_idle_snapshot() -> TestResult {
    let raw: Vec<u8> = vec![0u8; 16];
    let adapter = ACEvoAdapter::new();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
