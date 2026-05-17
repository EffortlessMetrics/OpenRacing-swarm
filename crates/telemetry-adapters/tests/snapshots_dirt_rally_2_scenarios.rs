//! Scenario-specific snapshot tests for DiRT Rally 2.0 (Codemasters Mode 1).
//!
//! These tests exercise DiRT-specific driving conditions beyond the generic
//! Codemasters Mode 1 tests in `snapshots_games_v3` and `snapshots_games_v7`:
//!
//! - **Gravel stage**: high wheel-speed variance (tyre slip on loose surface),
//!   moderate speed, counter-steer, partial throttle.
//! - **Tarmac stage**: consistent wheel speeds, high lat-G cornering at speed,
//!   trail-braking with heavy steering input.
//! - **Service area**: vehicle stationary in pits, engine idling, no inputs.

use openracing_telemetry_adapters::{DirtRally2Adapter, TelemetryAdapter};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Gravel stage ────────────────────────────────────────────────────────────
// Simulates a fast gravel section: rear wheels spinning faster than fronts
// (oversteer on loose surface), counter-steer right, partial throttle, 3rd gear.

fn make_gravel_stage_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    // Wheel speeds: rears faster than fronts (gravel wheelspin)
    write_f32_le(&mut buf, 100, 28.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 29.5); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 18.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 19.0); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.55); // throttle (partial, managing traction)
    write_f32_le(&mut buf, 120, 0.40); // steer (counter-steer right)
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 3.0); // gear (3rd)
    write_f32_le(&mut buf, 136, 1.20); // gforce_lat (moderate slide)
    write_f32_le(&mut buf, 140, 0.35); // gforce_lon (accelerating)
    write_f32_le(&mut buf, 144, 2.0); // current_lap (lap 3, 0-indexed)
    write_f32_le(&mut buf, 148, 5800.0); // rpm
    write_f32_le(&mut buf, 156, 5.0); // car_position (5th)
    write_f32_le(&mut buf, 180, 22.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 40.0); // fuel_capacity
    write_f32_le(&mut buf, 248, 312.5); // last_lap_time (5:12.5)
    write_f32_le(&mut buf, 252, 7500.0); // max_rpm
    write_f32_le(&mut buf, 260, 5.0); // max_gears
    buf
}

#[test]
fn dirt_rally_2_gravel_stage_snapshot() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let normalized = adapter.normalize(&make_gravel_stage_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Tarmac stage ────────────────────────────────────────────────────────────
// Simulates a fast tarmac hairpin: consistent wheel speeds, high lateral G,
// trail-braking into the corner, heavy steering left, 2nd gear.

fn make_tarmac_stage_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    // Wheel speeds: consistent (good tarmac grip)
    write_f32_le(&mut buf, 100, 30.0); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 30.5); // wheel_speed_rr
    write_f32_le(&mut buf, 108, 31.0); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 31.5); // wheel_speed_fr
    write_f32_le(&mut buf, 116, 0.20); // throttle (trail-brake transition)
    write_f32_le(&mut buf, 120, -0.72); // steer (heavy left)
    write_f32_le(&mut buf, 124, 0.45); // brake (trail-braking)
    write_f32_le(&mut buf, 132, 2.0); // gear (2nd)
    write_f32_le(&mut buf, 136, 2.40); // gforce_lat (hard cornering)
    write_f32_le(&mut buf, 140, -0.80); // gforce_lon (braking)
    write_f32_le(&mut buf, 144, 4.0); // current_lap (lap 5, 0-indexed)
    write_f32_le(&mut buf, 148, 6200.0); // rpm
    write_f32_le(&mut buf, 156, 2.0); // car_position (2nd)
    write_f32_le(&mut buf, 180, 18.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 40.0); // fuel_capacity
    write_f32_le(&mut buf, 212, 280.0); // brake_temp_fl (hot from braking)
    write_f32_le(&mut buf, 216, 285.0); // brake_temp_fr (clamped to 255)
    write_f32_le(&mut buf, 220, 180.0); // brake_temp_rl
    write_f32_le(&mut buf, 224, 175.0); // brake_temp_rr
    write_f32_le(&mut buf, 228, 32.0); // tyre_pressure_fl (psi)
    write_f32_le(&mut buf, 232, 32.5); // tyre_pressure_fr
    write_f32_le(&mut buf, 236, 30.0); // tyre_pressure_rl
    write_f32_le(&mut buf, 240, 30.5); // tyre_pressure_rr
    write_f32_le(&mut buf, 248, 295.8); // last_lap_time (4:55.8)
    write_f32_le(&mut buf, 252, 7500.0); // max_rpm
    write_f32_le(&mut buf, 260, 5.0); // max_gears
    buf
}

#[test]
fn dirt_rally_2_tarmac_stage_snapshot() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let normalized = adapter.normalize(&make_tarmac_stage_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Service area ────────────────────────────────────────────────────────────
// Simulates the car stationary in the service area between stages: in_pits
// flag set, engine idling, no driver inputs, neutral gear (0.0 → reverse/-1
// in Codemasters mapping, but service area typically shows gear 1).

fn make_service_area_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 264];
    // All wheel speeds zero (stationary)
    write_f32_le(&mut buf, 116, 0.0); // throttle
    write_f32_le(&mut buf, 120, 0.0); // steer
    write_f32_le(&mut buf, 124, 0.0); // brake
    write_f32_le(&mut buf, 132, 1.0); // gear (1st / neutral-ish in service)
    write_f32_le(&mut buf, 136, 0.0); // gforce_lat
    write_f32_le(&mut buf, 140, 0.0); // gforce_lon
    write_f32_le(&mut buf, 144, 3.0); // current_lap (after lap 3, 0-indexed)
    write_f32_le(&mut buf, 148, 850.0); // rpm (idle)
    write_f32_le(&mut buf, 156, 5.0); // car_position (5th)
    write_f32_le(&mut buf, 180, 40.0); // fuel_in_tank (refuelled)
    write_f32_le(&mut buf, 184, 40.0); // fuel_capacity (full tank)
    write_f32_le(&mut buf, 188, 1.0); // in_pit = true
    write_f32_le(&mut buf, 248, 305.2); // last_lap_time (5:05.2)
    write_f32_le(&mut buf, 252, 7500.0); // max_rpm
    write_f32_le(&mut buf, 260, 5.0); // max_gears
    buf
}

#[test]
fn dirt_rally_2_service_area_snapshot() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let normalized = adapter.normalize(&make_service_area_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
