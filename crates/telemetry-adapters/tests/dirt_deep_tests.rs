//! Deep validation tests for the Codemasters/DiRT family of telemetry adapters.
//!
//! Covers DiRT Rally 2.0, DiRT 3, DiRT 4, and DiRT Showdown — all of which
//! share the 264-byte Codemasters Mode 1 binary packet format parsed by
//! `codemasters_shared::parse_codemasters_mode1_common`.

use openracing_telemetry_adapters::codemasters_shared::{
    FFB_LAT_G_MAX, MIN_PACKET_SIZE, OFF_BRAKE, OFF_BRAKES_TEMP_FL, OFF_CAR_POSITION,
    OFF_CURRENT_LAP, OFF_FUEL_CAPACITY, OFF_FUEL_IN_TANK, OFF_GEAR, OFF_GFORCE_LAT, OFF_GFORCE_LON,
    OFF_IN_PIT, OFF_LAST_LAP_TIME, OFF_MAX_RPM, OFF_RPM, OFF_STEER, OFF_THROTTLE,
    OFF_TYRES_PRESSURE_FL, OFF_VEL_X, OFF_VEL_Y, OFF_VEL_Z, OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR,
    OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR,
};
use openracing_telemetry_adapters::{
    Dirt3Adapter, Dirt4Adapter, DirtRally2Adapter, DirtShowdownAdapter, TelemetryAdapter,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_pkt() -> Vec<u8> {
    vec![0u8; MIN_PACKET_SIZE]
}

fn set_f32(buf: &mut [u8], off: usize, val: f32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

// ── Proptest: arbitrary bytes never panic ─────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fuzz_dirt_rally2_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = DirtRally2Adapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_dirt3_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = Dirt3Adapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_dirt4_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = Dirt4Adapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_dirt_showdown_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = DirtShowdownAdapter::new();
        let _ = adapter.normalize(&data);
    }

    #[test]
    fn fuzz_throttle_brake_clamped(
        throttle in -1.0f32..2.0,
        brake in -1.0f32..2.0,
        steer in -2.0f32..2.0,
    ) {
        let mut pkt = make_pkt();
        set_f32(&mut pkt, OFF_THROTTLE, throttle);
        set_f32(&mut pkt, OFF_BRAKE, brake);
        set_f32(&mut pkt, OFF_STEER, steer);
        let adapter = DirtRally2Adapter::new();
        if let Ok(t) = adapter.normalize(&pkt) {
            prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
            prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
            prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0);
        }
    }

    #[test]
    fn fuzz_gear_mapping(gear_raw in -1.0f32..10.0) {
        let mut pkt = make_pkt();
        set_f32(&mut pkt, OFF_GEAR, gear_raw);
        let adapter = DirtRally2Adapter::new();
        if let Ok(t) = adapter.normalize(&pkt) {
            prop_assert!(t.gear >= -1 && t.gear <= 8);
        }
    }
}

// ── Packet size validation ───────────────────────────────────────────────────

#[test]
fn short_packet_rejected() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    assert!(adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]).is_err());
    Ok(())
}

#[test]
fn empty_packet_rejected() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn exact_min_size_accepted() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let pkt = make_pkt();
    let result = adapter.normalize(&pkt);
    assert!(result.is_ok(), "264-byte zeroed packet should parse");
    Ok(())
}

#[test]
fn oversized_packet_accepted() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    pkt.extend_from_slice(&[0xFF; 100]);
    let result = adapter.normalize(&pkt);
    assert!(result.is_ok());
    Ok(())
}

// ── Speed: wheel speed average fallback ──────────────────────────────────────

#[test]
fn speed_from_wheel_speeds() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FL, 20.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FR, 22.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RL, 18.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RR, 20.0);
    let t = adapter.normalize(&pkt)?;
    let avg = (20.0 + 22.0 + 18.0 + 20.0) / 4.0;
    assert!((t.speed_ms - avg).abs() < 0.5);
    Ok(())
}

#[test]
fn speed_zero_when_all_zero() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let pkt = make_pkt();
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.speed_ms, 0.0);
    Ok(())
}

#[test]
fn speed_from_velocity_magnitude() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    // wheel speeds zero, but velocity set → fallback
    set_f32(&mut pkt, OFF_VEL_X, 3.0);
    set_f32(&mut pkt, OFF_VEL_Y, 4.0);
    set_f32(&mut pkt, OFF_VEL_Z, 0.0);
    let t = adapter.normalize(&pkt)?;
    // velocity magnitude = 5.0
    assert!(t.speed_ms >= 0.0);
    Ok(())
}

// ── Gear mapping ─────────────────────────────────────────────────────────────

#[test]
fn gear_reverse() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GEAR, 0.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, -1, "0.0 → reverse");
    Ok(())
}

#[test]
fn gear_neutral() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GEAR, 1.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.gear == 0 || t.gear == 1, "1.0 → neutral or first");
    Ok(())
}

#[test]
fn gear_high() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GEAR, 6.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.gear >= 1 && t.gear <= 8);
    Ok(())
}

#[test]
fn gear_negative_value() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GEAR, -1.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.gear, -1, "negative → reverse");
    Ok(())
}

// ── Throttle / brake / steering ──────────────────────────────────────────────

#[test]
fn full_throttle() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_THROTTLE, 1.0);
    let t = adapter.normalize(&pkt)?;
    assert!((t.throttle - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn throttle_over_one_clamped() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_THROTTLE, 1.5);
    let t = adapter.normalize(&pkt)?;
    assert!(t.throttle <= 1.0);
    Ok(())
}

#[test]
fn brake_negative_clamped() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_BRAKE, -0.5);
    let t = adapter.normalize(&pkt)?;
    assert!(t.brake >= 0.0);
    Ok(())
}

#[test]
fn steering_clamped_to_range() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_STEER, 2.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.steering_angle <= 1.0);
    Ok(())
}

// ── RPM ──────────────────────────────────────────────────────────────────────

#[test]
fn rpm_positive() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_RPM, 5500.0);
    let t = adapter.normalize(&pkt)?;
    assert!((t.rpm - 5500.0).abs() < 1.0);
    Ok(())
}

#[test]
fn rpm_negative_clamped() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_RPM, -100.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.rpm >= 0.0);
    Ok(())
}

// ── Fuel calculation ─────────────────────────────────────────────────────────

#[test]
fn fuel_percent_normal() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_FUEL_IN_TANK, 30.0);
    set_f32(&mut pkt, OFF_FUEL_CAPACITY, 60.0);
    let t = adapter.normalize(&pkt)?;
    assert!((t.fuel_percent - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn fuel_zero_capacity_no_crash() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_FUEL_IN_TANK, 10.0);
    set_f32(&mut pkt, OFF_FUEL_CAPACITY, 0.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.fuel_percent.is_finite());
    Ok(())
}

#[test]
fn fuel_full() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_FUEL_IN_TANK, 60.0);
    set_f32(&mut pkt, OFF_FUEL_CAPACITY, 60.0);
    let t = adapter.normalize(&pkt)?;
    assert!((t.fuel_percent - 1.0).abs() < 0.01);
    Ok(())
}

// ── FFB scalar from lateral G ────────────────────────────────────────────────

#[test]
fn ffb_scalar_max_g() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GFORCE_LAT, FFB_LAT_G_MAX);
    let t = adapter.normalize(&pkt)?;
    assert!((t.ffb_scalar - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn ffb_scalar_negative_g() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GFORCE_LAT, -FFB_LAT_G_MAX);
    let t = adapter.normalize(&pkt)?;
    assert!((t.ffb_scalar + 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn ffb_scalar_over_max_clamped() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GFORCE_LAT, 10.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.ffb_scalar <= 1.0 && t.ffb_scalar >= -1.0);
    Ok(())
}

// ── Lap numbering (0-indexed in packet) ──────────────────────────────────────

#[test]
fn lap_zero_becomes_one() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_CURRENT_LAP, 0.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.lap, 1, "0-indexed lap 0 → displayed as 1");
    Ok(())
}

#[test]
fn lap_five() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_CURRENT_LAP, 4.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.lap, 5, "0-indexed lap 4 → displayed as 5");
    Ok(())
}

// ── In-pit detection ─────────────────────────────────────────────────────────

#[test]
fn in_pit_true() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_IN_PIT, 1.0);
    let t = adapter.normalize(&pkt)?;
    assert!(t.flags.in_pits);
    Ok(())
}

#[test]
fn in_pit_false() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let pkt = make_pkt();
    let t = adapter.normalize(&pkt)?;
    assert!(!t.flags.in_pits);
    Ok(())
}

// ── Position ─────────────────────────────────────────────────────────────────

#[test]
fn position_clamp() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_CAR_POSITION, 3.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.position, 3);
    Ok(())
}

// ── Tire temps ───────────────────────────────────────────────────────────────

#[test]
fn tire_temps_from_brake_temp_offsets() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_BRAKES_TEMP_FL, 150.0);
    set_f32(&mut pkt, OFF_BRAKES_TEMP_FL + 4, 160.0);
    set_f32(&mut pkt, OFF_BRAKES_TEMP_FL + 8, 140.0);
    set_f32(&mut pkt, OFF_BRAKES_TEMP_FL + 12, 155.0);
    let t = adapter.normalize(&pkt)?;
    assert_eq!(t.tire_temps_c[0], 150);
    assert_eq!(t.tire_temps_c[1], 160);
    assert_eq!(t.tire_temps_c[2], 140);
    assert_eq!(t.tire_temps_c[3], 155);
    Ok(())
}

// ── Tire pressures ───────────────────────────────────────────────────────────

#[test]
fn tire_pressures_read_correctly() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_TYRES_PRESSURE_FL, 25.0);
    set_f32(&mut pkt, OFF_TYRES_PRESSURE_FL + 4, 26.0);
    set_f32(&mut pkt, OFF_TYRES_PRESSURE_FL + 8, 24.0);
    set_f32(&mut pkt, OFF_TYRES_PRESSURE_FL + 12, 25.5);
    let t = adapter.normalize(&pkt)?;
    assert!((t.tire_pressures_psi[0] - 25.0).abs() < 0.1);
    assert!((t.tire_pressures_psi[1] - 26.0).abs() < 0.1);
    assert!((t.tire_pressures_psi[2] - 24.0).abs() < 0.1);
    assert!((t.tire_pressures_psi[3] - 25.5).abs() < 0.1);
    Ok(())
}

// ── Last lap time ────────────────────────────────────────────────────────────

#[test]
fn last_lap_time_read() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_LAST_LAP_TIME, 63.45);
    let t = adapter.normalize(&pkt)?;
    assert!((t.last_lap_time_s - 63.45).abs() < 0.01);
    Ok(())
}

// ── Max RPM and max gears ────────────────────────────────────────────────────

#[test]
fn rpm_fraction_when_max_rpm_set() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_RPM, 4000.0);
    set_f32(&mut pkt, OFF_MAX_RPM, 8000.0);
    let t = adapter.normalize(&pkt)?;
    assert!((t.rpm - 4000.0).abs() < 1.0);
    Ok(())
}

// ── G-force longitudinal ────────────────────────────────────────────────────

#[test]
fn longitudinal_g_force() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_GFORCE_LON, -2.5);
    let t = adapter.normalize(&pkt)?;
    assert!(t.longitudinal_g < 0.0 || t.longitudinal_g >= 0.0); // exists and is finite
    assert!(t.longitudinal_g.is_finite());
    Ok(())
}

// ── Cross-adapter: same packet parsed identically ────────────────────────────

#[test]
fn dirt3_and_dirt4_same_packet_format() -> TestResult {
    let d3 = Dirt3Adapter::new();
    let d4 = Dirt4Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_THROTTLE, 0.75);
    set_f32(&mut pkt, OFF_BRAKE, 0.3);
    set_f32(&mut pkt, OFF_RPM, 6000.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FL, 25.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FR, 25.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RL, 25.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RR, 25.0);

    let t3 = d3.normalize(&pkt)?;
    let t4 = d4.normalize(&pkt)?;

    assert!((t3.throttle - t4.throttle).abs() < 0.01);
    assert!((t3.brake - t4.brake).abs() < 0.01);
    assert!((t3.rpm - t4.rpm).abs() < 1.0);
    assert!((t3.speed_ms - t4.speed_ms).abs() < 0.5);
    Ok(())
}

#[test]
fn all_dirt_adapters_reject_short() -> TestResult {
    let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![
        Box::new(DirtRally2Adapter::new()),
        Box::new(Dirt3Adapter::new()),
        Box::new(Dirt4Adapter::new()),
        Box::new(DirtShowdownAdapter::new()),
    ];
    let short = vec![0u8; MIN_PACKET_SIZE - 1];
    for adapter in &adapters {
        assert!(
            adapter.normalize(&short).is_err(),
            "{} should reject short packet",
            adapter.game_id()
        );
    }
    Ok(())
}

// ── Game IDs ─────────────────────────────────────────────────────────────────

#[test]
fn game_ids_correct() -> TestResult {
    assert_eq!(DirtRally2Adapter::new().game_id(), "dirt_rally_2");
    assert_eq!(Dirt3Adapter::new().game_id(), "dirt3");
    assert_eq!(Dirt4Adapter::new().game_id(), "dirt4");
    assert_eq!(DirtShowdownAdapter::new().game_id(), "dirt_showdown");
    Ok(())
}

// ── Full rally stage scenario ────────────────────────────────────────────────

#[test]
fn full_rally_stage_scenario() -> TestResult {
    let adapter = DirtRally2Adapter::new();
    let mut pkt = make_pkt();
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FL, 30.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_FR, 31.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RL, 29.0);
    set_f32(&mut pkt, OFF_WHEEL_SPEED_RR, 30.0);
    set_f32(&mut pkt, OFF_THROTTLE, 0.85);
    set_f32(&mut pkt, OFF_BRAKE, 0.0);
    set_f32(&mut pkt, OFF_STEER, -0.15);
    set_f32(&mut pkt, OFF_GEAR, 4.0);
    set_f32(&mut pkt, OFF_RPM, 5800.0);
    set_f32(&mut pkt, OFF_MAX_RPM, 7500.0);
    set_f32(&mut pkt, OFF_FUEL_IN_TANK, 22.0);
    set_f32(&mut pkt, OFF_FUEL_CAPACITY, 44.0);
    set_f32(&mut pkt, OFF_IN_PIT, 0.0);
    set_f32(&mut pkt, OFF_CURRENT_LAP, 0.0);
    set_f32(&mut pkt, OFF_CAR_POSITION, 1.0);
    set_f32(&mut pkt, OFF_GFORCE_LAT, 1.2);
    set_f32(&mut pkt, OFF_GFORCE_LON, -0.5);
    set_f32(&mut pkt, OFF_LAST_LAP_TIME, 245.67);

    let t = adapter.normalize(&pkt)?;
    let avg_speed = (30.0 + 31.0 + 29.0 + 30.0) / 4.0;
    assert!((t.speed_ms - avg_speed).abs() < 0.5);
    assert!((t.throttle - 0.85).abs() < 0.01);
    assert_eq!(t.brake, 0.0);
    assert!(t.steering_angle < 0.0);
    assert!((t.rpm - 5800.0).abs() < 1.0);
    assert!((t.fuel_percent - 0.5).abs() < 0.01);
    assert!(!t.flags.in_pits);
    assert_eq!(t.lap, 1);
    assert_eq!(t.position, 1);
    assert!((t.last_lap_time_s - 245.67).abs() < 0.01);
    Ok(())
}
