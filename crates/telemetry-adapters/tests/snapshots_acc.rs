//! Snapshot tests for ACC (Assetto Corsa Competizione) telemetry adapter.
//!
//! Three scenarios: normal race pace, formation lap, and pit stop.

use openracing_telemetry_adapters::{ACCAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Push a minimal ACC lap-time block (no splits).
fn push_acc_lap(buf: &mut Vec<u8>, lap_time_ms: i32) {
    buf.extend_from_slice(&lap_time_ms.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // car_index
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(0); // split_count = 0
    buf.push(0); // is_invalid
    buf.push(1); // is_valid_for_best
    buf.push(0); // is_outlap
    buf.push(0); // is_inlap
}

struct AccCarScenario {
    car_index: u16,
    gear_raw: u8,
    car_location: u8,
    speed_kmh: u16,
    position: u16,
    cup_position: u16,
    track_position: u16,
    laps: u16,
    delta_ms: i32,
    best_lap_ms: i32,
    last_lap_ms: i32,
    current_lap_ms: i32,
}

/// Build an ACC RealtimeCarUpdate (message type 3) packet.
fn build_car_update(s: &AccCarScenario) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.push(3); // MSG_REALTIME_CAR_UPDATE
    buf.extend_from_slice(&s.car_index.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    buf.push(1); // driver_count
    buf.push(s.gear_raw);
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_x
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // world_pos_y
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
    buf.push(s.car_location);
    buf.extend_from_slice(&s.speed_kmh.to_le_bytes());
    buf.extend_from_slice(&s.position.to_le_bytes());
    buf.extend_from_slice(&s.cup_position.to_le_bytes());
    buf.extend_from_slice(&s.track_position.to_le_bytes());
    buf.extend_from_slice(&0.5f32.to_le_bytes()); // spline_position
    buf.extend_from_slice(&s.laps.to_le_bytes());
    buf.extend_from_slice(&s.delta_ms.to_le_bytes());
    push_acc_lap(&mut buf, s.best_lap_ms);
    push_acc_lap(&mut buf, s.last_lap_ms);
    push_acc_lap(&mut buf, s.current_lap_ms);
    buf
}

// ─── Scenario 1: Normal race pace ───────────────────────────────────────────
// P3, 5th gear at 220 km/h, lap 8, 0.35s ahead of reference, solid lap times.

#[test]
fn acc_normal_race_pace_snapshot() -> TestResult {
    let raw = build_car_update(&AccCarScenario {
        car_index: 7,
        gear_raw: 6,     // → 5th gear
        car_location: 1, // on track
        speed_kmh: 220,
        position: 3,
        cup_position: 3,
        track_position: 4,
        laps: 8,
        delta_ms: -350,         // 0.35s ahead
        best_lap_ms: 98_500,    // 1:38.500
        last_lap_ms: 99_200,    // 1:39.200
        current_lap_ms: 42_300, // 0:42.300
    });
    let adapter = ACCAdapter::new();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Formation lap ──────────────────────────────────────────────
// P12 rolling at 80 km/h in 2nd gear, lap 0, no reference times yet.

#[test]
fn acc_formation_lap_snapshot() -> TestResult {
    let raw = build_car_update(&AccCarScenario {
        car_index: 12,
        gear_raw: 3,     // → 2nd gear
        car_location: 1, // on track
        speed_kmh: 80,
        position: 12, // grid slot
        cup_position: 12,
        track_position: 12,
        laps: 0,                // formation
        delta_ms: 0,            // no reference
        best_lap_ms: -1,        // none
        last_lap_ms: -1,        // none
        current_lap_ms: 15_000, // 0:15.000 rolling
    });
    let adapter = ACCAdapter::new();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Pit stop ───────────────────────────────────────────────────
// P6, in pit lane at 60 km/h in 1st gear, lap 14, pit limiter active.

#[test]
fn acc_pit_stop_snapshot() -> TestResult {
    let raw = build_car_update(&AccCarScenario {
        car_index: 3,
        gear_raw: 2,     // → 1st gear
        car_location: 2, // pit lane (triggers pit_limiter + in_pits)
        speed_kmh: 60,
        position: 6,
        cup_position: 5,
        track_position: 8,
        laps: 14,
        delta_ms: 2_100,        // 2.1s behind
        best_lap_ms: 101_200,   // 1:41.200
        last_lap_ms: 108_900,   // 1:48.900 — pit-in lap
        current_lap_ms: 55_000, // 0:55.000 — pit-out
    });
    let adapter = ACCAdapter::new();
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
