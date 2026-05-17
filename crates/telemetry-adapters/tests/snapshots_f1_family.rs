//! Snapshot tests for the F1 adapter family.
//!
//! Covers four adapters that handle the EA Sports / Codemasters F1 titles:
//!   1. F1 (generic) — Codemasters custom-UDP bridge (mode 3)
//!   2. F1 25        — EA F1 2025 native UDP (packet format 2025)
//!   3. F1 Native    — EA F1 2023/2024 native UDP with extended telemetry
//!   4. F1 Manager   — stub adapter (strategy game, no driving telemetry)

use openracing_telemetry_adapters::{
    F1_25Adapter, F1Adapter, F1ManagerAdapter, F1NativeAdapter, TelemetryAdapter,
    f1_25::build_car_telemetry_packet, f1_native::build_car_telemetry_packet_native,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Build a Codemasters custom-UDP mode 3 race packet (22 fields × 4 bytes = 88 bytes).
///
/// Simulates an F1 car at ~72 m/s in 7th gear, heavy throttle, light trail
/// braking, with wheel-patch speeds showing mild rear oversteer.
fn build_f1_race_packet() -> Vec<u8> {
    let mut data = Vec::with_capacity(88);
    // base fields (7 × f32/i32)
    data.extend_from_slice(&72.0f32.to_le_bytes()); // speed (m/s)
    data.extend_from_slice(&750.0f32.to_le_bytes()); // engine_rate (rad/s ≈ 7162 RPM)
    data.extend_from_slice(&7i32.to_le_bytes()); // gear
    data.extend_from_slice(&(-0.08f32).to_le_bytes()); // steering_input
    data.extend_from_slice(&0.92f32.to_le_bytes()); // throttle_input
    data.extend_from_slice(&0.05f32.to_le_bytes()); // brake_input (trail brake)
    data.extend_from_slice(&0.0f32.to_le_bytes()); // clutch_input
    // mode 3 extra: wheel_patch_speed fl/fr/rl/rr
    data.extend_from_slice(&71.5f32.to_le_bytes()); // FL
    data.extend_from_slice(&72.0f32.to_le_bytes()); // FR
    data.extend_from_slice(&73.5f32.to_le_bytes()); // RL (slight oversteer)
    data.extend_from_slice(&73.0f32.to_le_bytes()); // RR
    // suspension_velocity fl/fr/rl/rr
    data.extend_from_slice(&0.02f32.to_le_bytes());
    data.extend_from_slice(&(-0.01f32).to_le_bytes());
    data.extend_from_slice(&0.03f32.to_le_bytes());
    data.extend_from_slice(&0.01f32.to_le_bytes());
    // suspension_position fl/fr/rl/rr
    data.extend_from_slice(&0.04f32.to_le_bytes());
    data.extend_from_slice(&0.035f32.to_le_bytes());
    data.extend_from_slice(&0.05f32.to_le_bytes());
    data.extend_from_slice(&0.045f32.to_le_bytes());
    // accelerations
    data.extend_from_slice(&0.8f32.to_le_bytes()); // long_accel
    data.extend_from_slice(&(-1.2f32).to_le_bytes()); // lat_accel
    data.extend_from_slice(&0.1f32.to_le_bytes()); // vert_accel
    data
}

// ─── 1. F1 (generic / Codemasters bridge) — race snapshot ────────────────────

#[test]
fn f1_race_snapshot() -> TestResult {
    let adapter = F1Adapter::new();
    let normalized = adapter.normalize(&build_f1_race_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 2. F1 25 — race snapshot ────────────────────────────────────────────────

#[test]
fn f1_25_race_snapshot() -> TestResult {
    let adapter = F1_25Adapter::new();
    let packet = build_car_telemetry_packet(
        0,                        // player_index
        310,                      // speed_kmh (≈ 86.1 m/s)
        8,                        // gear (top gear)
        11_500,                   // engine_rpm
        1.0,                      // throttle (full)
        0.0,                      // brake
        1,                        // drs = active
        [23.5, 23.5, 24.0, 24.0], // tyre pressures (PSI, RL/RR/FL/FR)
    );
    let normalized = adapter.normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 3. F1 Native — race with extended telemetry snapshot ────────────────────

#[test]
fn f1_native_extended_snapshot() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let packet = build_car_telemetry_packet_native(
        2024,                     // packet_format (F1 24)
        0,                        // player_index
        285,                      // speed_kmh (≈ 79.2 m/s)
        7,                        // gear
        10_800,                   // engine_rpm
        0.75,                     // throttle
        0.15,                     // brake (trail-braking)
        -0.22,                    // steer (left turn)
        0,                        // drs = off
        [25.0, 25.0, 25.5, 25.5], // tyre pressures (PSI, RL/RR/FL/FR)
    );
    let normalized = adapter.normalize(&packet)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── 4. F1 Manager — strategy view snapshot (stub, returns default) ──────────

#[test]
fn f1_manager_strategy_view_snapshot() -> TestResult {
    let adapter = F1ManagerAdapter::new();
    let normalized = adapter.normalize(&[])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
