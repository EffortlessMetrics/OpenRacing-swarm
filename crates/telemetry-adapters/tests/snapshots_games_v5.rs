//! Snapshot tests for additional telemetry adapters (v5).
//!
//! Covers: SimHub (direct adapter), F1 Manager (stub), F1 Native 2023,
//! F1 Native 2024, Sébastien Loeb Rally (stub), rFactor 1 variants
//! (GTR2, Race 07, GSC), Forza Horizon 4, Forza Horizon 5.

use openracing_telemetry_adapters::{
    F1ManagerAdapter, F1NativeAdapter, ForzaHorizon4Adapter, ForzaHorizon5Adapter, RFactor1Adapter,
    SebLoebRallyAdapter, SimHubAdapter, TelemetryAdapter,
    f1_native::build_car_telemetry_packet_native, rfactor1::RFactor1Variant,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_f64(buf: &mut [u8], offset: usize, val: f64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

// ─── SimHub direct adapter ────────────────────────────────────────────────────

#[test]
fn simhub_snapshot() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":22.5,"Rpms":4500.0,"MaxRpms":8000.0,"Gear":"3","Throttle":75.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":82.3,"LateralGForce":1.2,"LongitudinalGForce":-0.5,"FFBValue":0.35,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── F1 Manager (stub — no telemetry) ────────────────────────────────────────

#[test]
fn f1_manager_snapshot() -> TestResult {
    let adapter = F1ManagerAdapter::new();
    let normalized = adapter.normalize(&[])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── F1 Native (EA F1 2023 / EA F1 2024) ─────────────────────────────────────

/// Build a Car Telemetry (ID 6) packet for `packet_format` (2023 or 2024).
fn make_f1_native_telemetry_packet(packet_format: u16) -> Vec<u8> {
    build_car_telemetry_packet_native(
        packet_format,
        0,                        // player_index
        252,                      // speed_kmh (→ 70.0 m/s)
        4,                        // gear
        12_000,                   // engine_rpm
        0.85,                     // throttle
        0.0,                      // brake
        -0.15,                    // steer
        0,                        // drs = off
        [26.0, 26.0, 26.5, 26.5], // tyre pressures (PSI, RL/RR/FL/FR)
    )
}

#[test]
fn f1_native_f23_snapshot() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let normalized = adapter.normalize(&make_f1_native_telemetry_packet(2023))?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn f1_native_f24_snapshot() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let normalized = adapter.normalize(&make_f1_native_telemetry_packet(2024))?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Sébastien Loeb Rally EVO (stub — no telemetry) ──────────────────────────

#[test]
fn seb_loeb_rally_snapshot() -> TestResult {
    let adapter = SebLoebRallyAdapter::new();
    let normalized = adapter.normalize(&[])?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor 1 engine variants ────────────────────────────────────────────────

/// Build a full-size rFactor 1 UDP telemetry packet (1025 bytes).
fn make_rfactor1_variant_packet() -> Vec<u8> {
    let mut data = vec![0u8; 1025];
    write_f64(&mut data, 24, 0.0); // vel_x
    write_f64(&mut data, 32, 0.0); // vel_y
    write_f64(&mut data, 40, 45.0); // vel_z → speed = 45.0 m/s
    write_f64(&mut data, 312, 6800.0); // engine_rpm
    write_f64(&mut data, 992, 0.1); // steer_input
    write_f64(&mut data, 1000, 0.7); // throttle
    write_f64(&mut data, 1008, 0.0); // brake
    data[1024] = 4u8; // gear = 4
    data
}

#[test]
fn gtr2_snapshot() -> TestResult {
    let adapter = RFactor1Adapter::with_variant(RFactor1Variant::Gtr2);
    let normalized = adapter.normalize(&make_rfactor1_variant_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn race_07_snapshot() -> TestResult {
    let adapter = RFactor1Adapter::with_variant(RFactor1Variant::Race07);
    let normalized = adapter.normalize(&make_rfactor1_variant_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn gsc_snapshot() -> TestResult {
    let adapter = RFactor1Adapter::with_variant(RFactor1Variant::GameStockCar);
    let normalized = adapter.normalize(&make_rfactor1_variant_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Forza Horizon 4 & 5 ─────────────────────────────────────────────────────

/// Build a minimum valid Forza Sled packet (232 bytes, is_race_on=1).
fn make_forza_sled_packet() -> Vec<u8> {
    let mut data = vec![0u8; 232];
    data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    data[8..12].copy_from_slice(&8000.0f32.to_le_bytes()); // engine_max_rpm
    data[16..20].copy_from_slice(&5000.0f32.to_le_bytes()); // current_rpm
    data[32..36].copy_from_slice(&20.0f32.to_le_bytes()); // vel_x → speed 20 m/s
    data
}

#[test]
fn forza_horizon_4_snapshot() -> TestResult {
    let adapter = ForzaHorizon4Adapter::new();
    let normalized = adapter.normalize(&make_forza_sled_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn forza_horizon_5_snapshot() -> TestResult {
    let adapter = ForzaHorizon5Adapter::new();
    let normalized = adapter.normalize(&make_forza_sled_packet())?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
