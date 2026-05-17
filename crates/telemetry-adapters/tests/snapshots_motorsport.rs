//! Dedicated snapshot tests for motorsport-themed telemetry adapters.
//!
//! Covers: MotoGP, NASCAR, NASCAR 21, Le Mans Ultimate, Wreckfest, WTCR, RIDE 5.
//! Each test constructs a realistic race-scenario packet and snapshots the
//! normalised output via `insta::assert_yaml_snapshot!`.

use openracing_telemetry_adapters::{
    LeMansUltimateAdapter, MotoGPAdapter, Nascar21Adapter, NascarAdapter, Ride5Adapter,
    TelemetryAdapter, WreckfestAdapter, WtcrAdapter,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── MotoGP: race snapshot ──────────────────────────────────────────────────
// Mugello straight, 6th gear at 340 km/h (~94.4 m/s), high RPM near redline,
// full throttle, minimal lean angle producing moderate lateral G.

#[test]
fn motogp_race_snapshot() -> TestResult {
    let adapter = MotoGPAdapter::new();
    let json = br#"{"SpeedMs":94.4,"Rpms":16200.0,"MaxRpms":17500.0,"Gear":"6","Throttle":100.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":5.0,"FuelPercent":32.0,"LateralGForce":0.3,"LongitudinalGForce":1.4,"FFBValue":0.15,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── NASCAR: oval race snapshot ─────────────────────────────────────────────
// Daytona superspeedway, 4th gear at ~200 mph (~89.4 m/s), banked turn
// producing ~1.8 lateral G (17.66 m/s²), full throttle, slight left steer.

#[test]
fn nascar_oval_race_snapshot() -> TestResult {
    let adapter = NascarAdapter::new();
    let mut data = vec![0u8; 92];
    write_f32_le(&mut data, 16, 89.4); // speed_ms (~200 mph)
    write_f32_le(&mut data, 32, 2.94); // acc_x (0.3 G longitudinal, m/s²)
    write_f32_le(&mut data, 36, 17.66); // acc_y (1.8 G lateral, m/s²)
    write_f32_le(&mut data, 68, 4.0); // gear (4th)
    write_f32_le(&mut data, 72, 9200.0); // rpm — high revs on superspeedway
    write_f32_le(&mut data, 80, 1.0); // throttle — wide-open
    write_f32_le(&mut data, 84, 0.0); // brake — none in banked turn
    write_f32_le(&mut data, 88, -0.35); // steer — slight left for banking
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── NASCAR 21: race snapshot ───────────────────────────────────────────────
// Bristol short track, 3rd gear at ~130 mph (~58.1 m/s), heavy braking into
// tight corner, moderate lateral G.

#[test]
fn nascar_21_race_snapshot() -> TestResult {
    let adapter = Nascar21Adapter::new();
    let mut data = vec![0u8; 92];
    write_f32_le(&mut data, 16, 58.1); // speed_ms (~130 mph)
    write_f32_le(&mut data, 32, -4.91); // acc_x (-0.5 G decel, m/s²)
    write_f32_le(&mut data, 36, 14.72); // acc_y (1.5 G lateral, m/s²)
    write_f32_le(&mut data, 68, 3.0); // gear (3rd)
    write_f32_le(&mut data, 72, 7800.0); // rpm
    write_f32_le(&mut data, 80, 0.0); // throttle — off during braking
    write_f32_le(&mut data, 84, 0.75); // brake — heavy braking
    write_f32_le(&mut data, 88, -0.55); // steer — turning left into corner
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Le Mans Ultimate: endurance race snapshot ──────────────────────────────
// Mulsanne straight, 6th gear at ~340 km/h (~94.4 m/s), high RPM,
// partial throttle (lifting slightly), no braking.

#[test]
fn le_mans_ultimate_endurance_race_snapshot() -> TestResult {
    let adapter = LeMansUltimateAdapter::new();
    let mut data = vec![0u8; 20];
    write_f32_le(&mut data, 0, 94.4); // speed_ms (~340 km/h)
    write_f32_le(&mut data, 4, 8400.0); // rpm — LMP2 high revs
    write_f32_le(&mut data, 8, 6.0); // gear (6th)
    write_f32_le(&mut data, 12, 0.92); // throttle — near full
    write_f32_le(&mut data, 16, 0.0); // brake — none on straight
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Wreckfest: demolition derby snapshot ───────────────────────────────────
// Mid-collision in a derby arena, 2nd gear, moderate speed, extreme G-forces
// from impacts in both axes.

#[test]
fn wreckfest_demolition_derby_snapshot() -> TestResult {
    let adapter = WreckfestAdapter::new();
    let mut data = vec![0u8; 28];
    data[0..4].copy_from_slice(b"WRKF"); // magic
    write_f32_le(&mut data, 8, 18.5); // speed_ms (~67 km/h)
    write_f32_le(&mut data, 12, 5200.0); // rpm
    data[16] = 2; // gear (2nd)
    write_f32_le(&mut data, 20, 2.4); // lateral_g — heavy side-impact
    write_f32_le(&mut data, 24, -1.8); // longitudinal_g — decel from collision
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── WTCR: touring car snapshot ─────────────────────────────────────────────
// Macau Guia Circuit, 3rd gear at ~140 km/h, late apex, trail-braking with
// high lateral G, mid-pack position, tyre temps elevated, moderate fuel.

#[test]
fn wtcr_touring_car_snapshot() -> TestResult {
    let adapter = WtcrAdapter::new();
    let mut buf = vec![0u8; 264];
    // Wheel speeds (all ~38.9 m/s ≈ 140 km/h)
    write_f32_le(&mut buf, 108, 38.5); // wheel_speed_fl
    write_f32_le(&mut buf, 112, 39.3); // wheel_speed_fr
    write_f32_le(&mut buf, 100, 38.2); // wheel_speed_rl
    write_f32_le(&mut buf, 104, 39.6); // wheel_speed_rr
    write_f32_le(&mut buf, 116, 0.45); // throttle — partial, trail-brake
    write_f32_le(&mut buf, 120, -0.42); // steer — right-hander
    write_f32_le(&mut buf, 124, 0.30); // brake — trail-braking
    write_f32_le(&mut buf, 132, 3.0); // gear (3rd)
    write_f32_le(&mut buf, 136, 2.1); // gforce_lat — high for touring car
    write_f32_le(&mut buf, 140, -0.6); // gforce_lon — deceleration
    write_f32_le(&mut buf, 144, 7.0); // current_lap (lap 8 after +1)
    write_f32_le(&mut buf, 148, 7200.0); // rpm
    write_f32_le(&mut buf, 156, 8.0); // car_position (P8)
    write_f32_le(&mut buf, 180, 22.0); // fuel_in_tank
    write_f32_le(&mut buf, 184, 50.0); // fuel_capacity
    write_f32_le(&mut buf, 188, 0.0); // in_pit — on track
    // Brake temps (elevated from hard braking)
    write_f32_le(&mut buf, 212, 185.0); // FL brake temp
    write_f32_le(&mut buf, 216, 192.0); // FR brake temp
    write_f32_le(&mut buf, 220, 160.0); // RL brake temp
    write_f32_le(&mut buf, 224, 168.0); // RR brake temp
    // Tyre pressures
    write_f32_le(&mut buf, 228, 28.5); // FL tyre pressure psi
    write_f32_le(&mut buf, 232, 29.0); // FR tyre pressure psi
    write_f32_le(&mut buf, 236, 27.8); // RL tyre pressure psi
    write_f32_le(&mut buf, 240, 28.2); // RR tyre pressure psi
    write_f32_le(&mut buf, 248, 135.8); // last_lap_time_s (~2:15.8)
    write_f32_le(&mut buf, 252, 8000.0); // max_rpm
    write_f32_le(&mut buf, 260, 6.0); // max_gears
    let normalized = adapter.normalize(&buf)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── RIDE 5: motorcycle race snapshot ───────────────────────────────────────
// Imola circuit, 4th gear at ~180 km/h (~50 m/s), hard acceleration out of
// chicane, lean angle producing moderate lateral G, low fuel (late-race).

#[test]
fn ride5_motorcycle_race_snapshot() -> TestResult {
    let adapter = Ride5Adapter::new();
    let json = br#"{"SpeedMs":50.0,"Rpms":11200.0,"MaxRpms":14500.0,"Gear":"4","Throttle":92.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-18.0,"FuelPercent":18.0,"LateralGForce":1.3,"LongitudinalGForce":0.9,"FFBValue":0.55,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
