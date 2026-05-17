//! Snapshot tests for rFactor 1 / Automobilista 1 / rFactor 2 telemetry adapters.
//!
//! Scenarios not covered by existing snapshot files:
//!   - rFactor 1: heavy braking at turn entry
//!   - Automobilista 1: high-speed cornering with lateral G
//!   - rFactor 2: stationary pit idle

use openracing_telemetry_adapters::{
    Automobilista1Adapter, RFactor1Adapter, RFactor2Adapter, TelemetryAdapter,
    rfactor2::{RF2VehicleTelemetry, RF2WheelTelemetry},
};
use std::mem;

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Byte-level helpers ──────────────────────────────────────────────────────

fn write_f64(buf: &mut [u8], offset: usize, val: f64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn struct_to_bytes<T: Copy>(val: &T) -> Vec<u8> {
    let size = mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    unsafe {
        std::ptr::copy_nonoverlapping(val as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    buf
}

// ─── rFactor 1: heavy braking at turn entry ─────────────────────────────────
// The car decelerates from ~120 km/h (33 m/s) into a slow corner: full brake,
// no throttle, 2nd gear, moderate RPM, steering right.

#[test]
fn rfactor1_heavy_braking_snapshot() -> TestResult {
    let mut data = vec![0u8; 1025]; // covers OFF_GEAR + 1
    write_f64(&mut data, 24, 5.0); // vel_x (slight lateral drift)
    write_f64(&mut data, 32, 0.0); // vel_y
    write_f64(&mut data, 40, 32.5); // vel_z → speed ≈ 32.88 m/s
    write_f64(&mut data, 312, 4200.0); // engine_rpm
    write_f64(&mut data, 992, 0.35); // steer_input (right)
    write_f64(&mut data, 1000, 0.0); // throttle (off)
    write_f64(&mut data, 1008, 1.0); // brake (full)
    data[1024] = 2u8; // gear = 2

    let adapter = RFactor1Adapter::new();
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Automobilista 1: high-speed cornering ──────────────────────────────────
// Fast sweeper at ~55 m/s in 5th gear, partial throttle, strong lateral G,
// fuel at about half capacity.

#[test]
fn ams1_high_speed_cornering_snapshot() -> TestResult {
    let mut data = vec![0u8; 532]; // AMS1_MIN_SHARED_MEMORY_SIZE
    write_f64(&mut data, 216, 9.81 * 1.8); // lateral accel ≈ 1.8 G
    write_f64(&mut data, 232, 9.81 * 0.1); // longitudinal accel ≈ 0.1 G
    write_i32(&mut data, 360, 5); // gear (5th)
    write_f64(&mut data, 368, 9200.0); // engine_rpm
    write_f64(&mut data, 384, 11_000.0); // engine_max_rpm
    data[457] = 80u8; // fuel_capacity (litres)
    write_f32_le(&mut data, 460, 38.0); // fuel_in_tank
    write_f32_le(&mut data, 492, 0.55); // throttle (partial)
    write_f32_le(&mut data, 496, 0.0); // brake (off)
    write_f32_le(&mut data, 500, -0.45); // steering (left, mid-corner)
    write_f32_le(&mut data, 528, 55.0); // speed_ms

    let adapter = Automobilista1Adapter::new();
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor 2: stationary pit idle ─────────────────────────────────────────
// Car sitting in the pits: zero speed, neutral gear, low idle RPM, no inputs.

#[test]
fn rfactor2_pit_idle_snapshot() -> TestResult {
    let vehicle = RF2VehicleTelemetry {
        local_vel: [0.0, 0.0, 0.0],
        gear: 0, // neutral
        engine_rpm: 850.0,
        engine_max_rpm: 11_000.0,
        engine_water_temp: 78.0,
        engine_oil_temp: 72.0,
        fuel: 55.0,
        unfiltered_throttle: 0.0,
        unfiltered_brake: 0.0,
        unfiltered_steering: 0.0,
        unfiltered_clutch: 0.0,
        steering_shaft_torque: 0.0,
        wheels: [RF2WheelTelemetry::default(); 4],
        ..Default::default()
    };

    let adapter = RFactor2Adapter::new();
    let raw = struct_to_bytes(&vehicle);
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
