//! Extended snapshot tests for telemetry adapter normalization.
//!
//! Covers adapters not yet snapshotted in `snapshot_tests.rs`:
//! Forza (Sled + CarDash), BeamNG, LFS, ETS2/ATS, PCars2, iRacing, rFactor2, AMS2.

use openracing_telemetry_adapters::{
    AMS2Adapter, BeamNGAdapter, Ets2Adapter, ForzaAdapter, IRacingAdapter, LFSAdapter,
    PCars2Adapter, PCars3Adapter, RFactor2Adapter, TelemetryAdapter, ams2::AMS2SharedMemory,
    ets2::Ets2Variant, rfactor2::RF2VehicleTelemetry,
};
use std::mem;
mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

// ─── Forza ───────────────────────────────────────────────────────────────────

fn make_forza_sled() -> Vec<u8> {
    let mut data = vec![0u8; 232];
    write_i32(&mut data, 0, 1); // is_race_on = 1
    write_f32_le(&mut data, 8, 8000.0); // engine_max_rpm
    write_f32_le(&mut data, 16, 6000.0); // current_rpm
    write_f32_le(&mut data, 32, 40.0); // vel_x → speed = 40.0 m/s
    data
}

fn make_forza_cardash() -> Vec<u8> {
    let mut data = vec![0u8; 311];
    write_i32(&mut data, 0, 1); // is_race_on = 1
    write_f32_le(&mut data, 8, 8000.0); // engine_max_rpm
    write_f32_le(&mut data, 16, 5500.0); // current_rpm
    write_f32_le(&mut data, 32, 30.0); // vel_x
    write_f32_le(&mut data, 244, 30.0); // dash_speed (more accurate)
    data[303] = 200; // dash_accel: 200/255 ≈ 0.784
    data[304] = 0; // dash_brake
    data[307] = 4; // dash_gear: 4 → gear 3
    data[308] = 25i8 as u8; // dash_steer: 25/127 ≈ 0.197
    data
}

#[test]
fn forza_sled_normalized_snapshot() -> TestResult {
    let adapter = ForzaAdapter::new();
    let normalized = adapter.normalize(&make_forza_sled())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn forza_cardash_normalized_snapshot() -> TestResult {
    let adapter = ForzaAdapter::new();
    let normalized = adapter.normalize(&make_forza_cardash())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── BeamNG ──────────────────────────────────────────────────────────────────

fn make_beamng_packet() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    data[10] = 3; // gear_raw 3 → normalized gear 2
    write_f32_le(&mut data, 12, 25.0); // speed_ms
    write_f32_le(&mut data, 16, 5000.0); // rpm
    write_f32_le(&mut data, 48, 0.7); // throttle
    write_f32_le(&mut data, 52, 0.1); // brake
    data
}

#[test]
fn beamng_outgauge_normalized_snapshot() -> TestResult {
    let adapter = BeamNGAdapter::new();
    let normalized = adapter.normalize(&make_beamng_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── LFS ─────────────────────────────────────────────────────────────────────

fn make_lfs_packet() -> Vec<u8> {
    let mut data = vec![0u8; 96];
    data[10] = 3; // gear_raw 3 → normalized gear 2
    write_f32_le(&mut data, 12, 30.0); // speed_ms
    write_f32_le(&mut data, 16, 4500.0); // rpm
    write_f32_le(&mut data, 28, 0.7); // fuel
    write_f32_le(&mut data, 48, 0.7); // throttle
    write_f32_le(&mut data, 52, 0.2); // brake
    data
}

#[test]
fn lfs_outgauge_normalized_snapshot() -> TestResult {
    let adapter = LFSAdapter::new();
    let normalized = adapter.normalize(&make_lfs_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── ETS2 / ATS ──────────────────────────────────────────────────────────────

fn make_scs_packet() -> Vec<u8> {
    let mut data = vec![0u8; 512];
    write_u32(&mut data, 0, 1); // version = 1
    write_f32_le(&mut data, 4, 22.0); // speed_ms
    write_f32_le(&mut data, 8, 1500.0); // engine_rpm
    write_i32(&mut data, 12, 5); // gear = 5
    write_f32_le(&mut data, 16, 0.75); // fuel_ratio
    write_f32_le(&mut data, 20, 0.65); // engine_load
    write_f32_le(&mut data, 24, 0.6); // throttle
    write_f32_le(&mut data, 28, 0.0); // brake
    write_f32_le(&mut data, 32, 0.0); // clutch
    write_f32_le(&mut data, 36, 0.0); // steering
    write_f32_le(&mut data, 40, 90.0); // engine_temp_c
    write_f32_le(&mut data, 44, 2100.0); // max_rpm
    data
}

#[test]
fn ets2_scs_normalized_snapshot() -> TestResult {
    let adapter = Ets2Adapter::new();
    let normalized = adapter.normalize(&make_scs_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn ats_scs_normalized_snapshot() -> TestResult {
    let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
    let normalized = adapter.normalize(&make_scs_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── PCars2 ──────────────────────────────────────────────────────────────────

fn make_pcars2_packet() -> Vec<u8> {
    let mut data = vec![0u8; 46];
    data[44] = (0.1f32 * 127.0) as i8 as u8; // steering i8
    data[30] = (0.85f32 * 255.0) as u8; // throttle u8
    data[29] = 0; // brake u8
    write_f32_le(&mut data, 36, 50.0); // speed f32 m/s
    data[40..42].copy_from_slice(&6500u16.to_le_bytes()); // rpm u16
    data[42..44].copy_from_slice(&8500u16.to_le_bytes()); // max_rpm u16
    data[45] = 4 | (6 << 4); // gear=4, num_gears=6
    data
}

#[test]
fn pcars2_udp_normalized_snapshot() -> TestResult {
    let adapter = PCars2Adapter::new();
    let normalized = adapter.normalize(&make_pcars2_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── PCars3 ──────────────────────────────────────────────────────────────────

fn make_pcars3_packet() -> Vec<u8> {
    let mut data = vec![0u8; 46];
    data[44] = (0.15f32 * 127.0) as i8 as u8; // steering i8
    data[30] = (0.75f32 * 255.0) as u8; // throttle u8
    data[29] = 0; // brake u8
    write_f32_le(&mut data, 36, 45.0); // speed f32 m/s (≈162 km/h)
    data[40..42].copy_from_slice(&7200u16.to_le_bytes()); // rpm u16
    data[42..44].copy_from_slice(&8500u16.to_le_bytes()); // max_rpm u16
    data[45] = 4 | (6 << 4); // gear=4, num_gears=6
    data
}

#[test]
fn project_cars_3_snapshot() -> TestResult {
    let adapter = PCars3Adapter::new();
    let normalized = adapter.normalize(&make_pcars3_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── iRacing ─────────────────────────────────────────────────────────────────

/// 8 KiB of zeroes covers both IRacingLegacyData and IRacingData.
#[test]
fn iracing_zeroed_buffer_snapshot() -> TestResult {
    let adapter = IRacingAdapter::new();
    let normalized = adapter.normalize(&[0u8; 8192])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor2 ────────────────────────────────────────────────────────────────

#[test]
fn rfactor2_zeroed_vehicle_snapshot() -> TestResult {
    let adapter = RFactor2Adapter::new();
    let raw = vec![0u8; mem::size_of::<RF2VehicleTelemetry>()];
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── AMS2 ────────────────────────────────────────────────────────────────────

#[test]
fn ams2_zeroed_shared_memory_snapshot() -> TestResult {
    let adapter = AMS2Adapter::new();
    let raw = vec![0u8; mem::size_of::<AMS2SharedMemory>()];
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
