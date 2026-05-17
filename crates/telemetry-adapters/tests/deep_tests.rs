//! Deep integration tests for DiRT Rally 2.0, ETS2/ATS, and Gran Turismo 7
//! telemetry adapters.
//!
//! Each adapter section contains 15–25 tests exercising packet parsing,
//! normalisation, edge cases, and extended data fields.

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::codemasters_shared;
use openracing_telemetry_adapters::ets2;
use openracing_telemetry_adapters::gran_turismo_7;
use openracing_telemetry_adapters::{
    DirtRally2Adapter, Ets2Adapter, GranTurismo7Adapter, TelemetryAdapter, TelemetryValue,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// DiRT Rally 2.0 deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod dirt_rally_2_deep {
    use super::*;

    const MIN: usize = codemasters_shared::MIN_PACKET_SIZE;

    fn adapter() -> DirtRally2Adapter {
        DirtRally2Adapter::new()
    }

    fn make_packet() -> Vec<u8> {
        vec![0u8; MIN]
    }

    // -- Mode 1 packet parsing -----------------------------------------------

    #[test]
    fn rejects_empty_packet() -> TestResult {
        let result = adapter().normalize(&[]);
        assert!(result.is_err(), "empty packet must fail");
        Ok(())
    }

    #[test]
    fn rejects_packet_one_byte_short() -> TestResult {
        let data = vec![0u8; MIN - 1];
        let result = adapter().normalize(&data);
        assert!(result.is_err(), "short packet must fail");
        Ok(())
    }

    #[test]
    fn accepts_exact_min_packet() -> TestResult {
        let data = make_packet();
        let _t = adapter().normalize(&data)?;
        Ok(())
    }

    #[test]
    fn accepts_oversized_packet() -> TestResult {
        let data = vec![0u8; MIN * 2];
        let _t = adapter().normalize(&data)?;
        Ok(())
    }

    #[test]
    fn zero_packet_yields_zero_speed() -> TestResult {
        let t = adapter().normalize(&make_packet())?;
        assert_eq!(t.speed_ms, 0.0, "zero buffer must give zero speed");
        Ok(())
    }

    // -- Speed calculation from wheel speeds ---------------------------------

    #[test]
    fn speed_from_equal_wheel_speeds() -> TestResult {
        let mut buf = make_packet();
        for off in [
            codemasters_shared::OFF_WHEEL_SPEED_FL,
            codemasters_shared::OFF_WHEEL_SPEED_FR,
            codemasters_shared::OFF_WHEEL_SPEED_RL,
            codemasters_shared::OFF_WHEEL_SPEED_RR,
        ] {
            write_f32_le(&mut buf, off, 30.0);
        }
        let t = adapter().normalize(&buf)?;
        assert!(
            (t.speed_ms - 30.0).abs() < 0.01,
            "speed_ms should be 30.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_from_unequal_wheel_speeds() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_WHEEL_SPEED_FL, 10.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_WHEEL_SPEED_FR, 20.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_WHEEL_SPEED_RL, 30.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_WHEEL_SPEED_RR, 40.0);
        let t = adapter().normalize(&buf)?;
        let expected = (10.0 + 20.0 + 30.0 + 40.0) / 4.0;
        assert!(
            (t.speed_ms - expected).abs() < 0.01,
            "speed_ms should be {expected}, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_fallback_to_velocity_vector() -> TestResult {
        let mut buf = make_packet();
        // All wheel speeds are 0 — should fall back to velocity magnitude.
        write_f32_le(&mut buf, codemasters_shared::OFF_VEL_X, 3.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_VEL_Z, 4.0);
        let t = adapter().normalize(&buf)?;
        assert!(
            (t.speed_ms - 5.0).abs() < 0.01,
            "speed_ms should be 5.0 (3-4-5), got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn negative_wheel_speeds_use_absolute_values() -> TestResult {
        let mut buf = make_packet();
        for off in [
            codemasters_shared::OFF_WHEEL_SPEED_FL,
            codemasters_shared::OFF_WHEEL_SPEED_FR,
            codemasters_shared::OFF_WHEEL_SPEED_RL,
            codemasters_shared::OFF_WHEEL_SPEED_RR,
        ] {
            write_f32_le(&mut buf, off, -15.0);
        }
        let t = adapter().normalize(&buf)?;
        assert!(
            (t.speed_ms - 15.0).abs() < 0.01,
            "speed_ms should be 15.0 (abs), got {}",
            t.speed_ms
        );
        Ok(())
    }

    // -- FFB scalar from lateral G -------------------------------------------

    #[test]
    fn ffb_scalar_zero_at_no_lateral_g() -> TestResult {
        let t = adapter().normalize(&make_packet())?;
        assert!(
            t.ffb_scalar.abs() < 0.001,
            "no lateral G should give ~0 FFB scalar"
        );
        Ok(())
    }

    #[test]
    fn ffb_scalar_positive_from_positive_lateral_g() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_GFORCE_LAT, 1.5);
        let t = adapter().normalize(&buf)?;
        assert!(t.ffb_scalar > 0.0, "positive lat G → positive FFB scalar");
        assert!(
            t.ffb_scalar <= 1.0,
            "FFB scalar must not exceed 1.0, got {}",
            t.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn ffb_scalar_clamped_at_extreme_lateral_g() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_GFORCE_LAT, 100.0);
        let t = adapter().normalize(&buf)?;
        assert!(
            t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "extreme lat G must be clamped, got {}",
            t.ffb_scalar
        );
        Ok(())
    }

    // -- Gear encoding -------------------------------------------------------

    #[test]
    fn gear_zero_maps_to_reverse() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_GEAR, 0.0);
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.gear, -1, "gear 0.0 in packet should map to -1 (reverse)");
        Ok(())
    }

    #[test]
    fn gear_forward_1_through_6() -> TestResult {
        for g in 1i8..=6 {
            let mut buf = make_packet();
            write_f32_le(&mut buf, codemasters_shared::OFF_GEAR, f32::from(g));
            let t = adapter().normalize(&buf)?;
            assert_eq!(t.gear, g, "gear {g} mismatch");
        }
        Ok(())
    }

    #[test]
    fn gear_clamped_at_upper_bound() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_GEAR, 15.0);
        let t = adapter().normalize(&buf)?;
        assert!(t.gear <= 8, "gear must be clamped to ≤ 8, got {}", t.gear);
        Ok(())
    }

    // -- Extended data fields ------------------------------------------------

    #[test]
    fn extended_wheel_speeds_present() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_WHEEL_SPEED_FL, 12.0);
        let t = adapter().normalize(&buf)?;
        match t.extended.get("wheel_speed_fl") {
            Some(TelemetryValue::Float(v)) => {
                assert!(
                    (*v - 12.0).abs() < 0.01,
                    "wheel_speed_fl should be 12.0, got {v}"
                );
            }
            other => return Err(format!("expected Float, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn rpm_fraction_in_extended() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_RPM, 4000.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_MAX_RPM, 8000.0);
        let t = adapter().normalize(&buf)?;
        match t.extended.get("rpm_fraction") {
            Some(TelemetryValue::Float(v)) => {
                assert!(
                    (*v - 0.5).abs() < 0.001,
                    "rpm_fraction should be 0.5, got {v}"
                );
            }
            other => return Err(format!("expected Float for rpm_fraction, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn throttle_and_brake_clamped() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_THROTTLE, 2.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_BRAKE, -0.5);
        let t = adapter().normalize(&buf)?;
        assert!(
            t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of range: {}",
            t.throttle
        );
        assert!(
            t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of range: {}",
            t.brake
        );
        Ok(())
    }

    #[test]
    fn steering_angle_clamped() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_STEER, 5.0);
        let t = adapter().normalize(&buf)?;
        assert!(
            t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering out of range: {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn fuel_percent_calculated_correctly() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_FUEL_IN_TANK, 25.0);
        write_f32_le(&mut buf, codemasters_shared::OFF_FUEL_CAPACITY, 50.0);
        let t = adapter().normalize(&buf)?;
        assert!(
            (t.fuel_percent - 0.5).abs() < 0.001,
            "fuel_percent should be 0.5, got {}",
            t.fuel_percent
        );
        Ok(())
    }

    #[test]
    fn in_pit_flag_true_when_set() -> TestResult {
        let mut buf = make_packet();
        write_f32_le(&mut buf, codemasters_shared::OFF_IN_PIT, 1.0);
        let t = adapter().normalize(&buf)?;
        assert!(t.flags.in_pits, "in_pits should be true when pit = 1.0");
        Ok(())
    }

    #[test]
    fn game_id_is_dirt_rally_2() {
        assert_eq!(adapter().game_id(), "dirt_rally_2");
    }

    #[test]
    fn update_rate_is_16ms() {
        assert_eq!(adapter().expected_update_rate(), Duration::from_millis(16));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ETS2 / ATS deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod ets2_deep {
    use super::*;

    const SCS_SIZE: usize = 512;
    // Byte offsets matching the adapter source.
    const OFF_VERSION: usize = 0;
    const OFF_SPEED_MS: usize = 4;
    const OFF_ENGINE_RPM: usize = 8;
    const OFF_GEAR: usize = 12;
    const OFF_FUEL_RATIO: usize = 16;
    const OFF_ENGINE_LOAD: usize = 20;
    const OFF_THROTTLE: usize = 24;
    const OFF_BRAKE: usize = 28;
    const OFF_CLUTCH: usize = 32;
    const OFF_STEERING: usize = 36;
    const OFF_ENGINE_TEMP_C: usize = 40;
    const OFF_MAX_RPM: usize = 44;

    fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    /// Build a valid SCS packet with version = 1 and the given core fields.
    fn make_scs(speed: f32, rpm: f32, gear: i32, fuel: f32, load: f32) -> Vec<u8> {
        let mut data = vec![0u8; SCS_SIZE];
        write_u32(&mut data, OFF_VERSION, 1);
        write_f32_le(&mut data, OFF_SPEED_MS, speed);
        write_f32_le(&mut data, OFF_ENGINE_RPM, rpm);
        write_i32(&mut data, OFF_GEAR, gear);
        write_f32_le(&mut data, OFF_FUEL_RATIO, fuel);
        write_f32_le(&mut data, OFF_ENGINE_LOAD, load);
        data
    }

    // -- SCS telemetry format ------------------------------------------------

    #[test]
    fn rejects_empty_buffer() -> TestResult {
        assert!(ets2::parse_scs_packet(&[]).is_err());
        Ok(())
    }

    #[test]
    fn rejects_short_buffer() -> TestResult {
        let data = vec![0u8; 10];
        assert!(ets2::parse_scs_packet(&data).is_err());
        Ok(())
    }

    // -- Version validation --------------------------------------------------

    #[test]
    fn rejects_version_zero() -> TestResult {
        let mut data = make_scs(10.0, 1000.0, 3, 0.5, 0.3);
        write_u32(&mut data, OFF_VERSION, 0);
        assert!(ets2::parse_scs_packet(&data).is_err());
        Ok(())
    }

    #[test]
    fn rejects_version_two() -> TestResult {
        let mut data = make_scs(10.0, 1000.0, 3, 0.5, 0.3);
        write_u32(&mut data, OFF_VERSION, 2);
        assert!(ets2::parse_scs_packet(&data).is_err());
        Ok(())
    }

    #[test]
    fn accepts_version_one() -> TestResult {
        let data = make_scs(10.0, 1000.0, 3, 0.5, 0.3);
        let _t = ets2::parse_scs_packet(&data)?;
        Ok(())
    }

    // -- Truck-specific data -------------------------------------------------

    #[test]
    fn engine_rpm_parsed() -> TestResult {
        let data = make_scs(0.0, 1800.0, 0, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            (t.rpm - 1800.0).abs() < 0.1,
            "rpm should be 1800.0, got {}",
            t.rpm
        );
        Ok(())
    }

    #[test]
    fn engine_rpm_negative_clamped_to_zero() -> TestResult {
        let data = make_scs(0.0, -500.0, 0, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(t.rpm >= 0.0, "rpm must be non-negative, got {}", t.rpm);
        Ok(())
    }

    #[test]
    fn gear_forward() -> TestResult {
        let data = make_scs(20.0, 1500.0, 6, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert_eq!(t.gear, 6);
        Ok(())
    }

    #[test]
    fn gear_reverse() -> TestResult {
        let data = make_scs(2.0, 800.0, -1, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn gear_neutral() -> TestResult {
        let data = make_scs(0.0, 700.0, 0, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert_eq!(t.gear, 0);
        Ok(())
    }

    #[test]
    fn fuel_ratio_passed_through() -> TestResult {
        let data = make_scs(20.0, 1500.0, 4, 0.75, 0.5);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            (t.fuel_percent - 0.75).abs() < 0.001,
            "fuel_percent should be 0.75, got {}",
            t.fuel_percent
        );
        Ok(())
    }

    #[test]
    fn engine_temp_parsed() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_ENGINE_TEMP_C, 95.0);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            (t.engine_temp_c - 95.0).abs() < 0.1,
            "engine_temp_c should be 95.0, got {}",
            t.engine_temp_c
        );
        Ok(())
    }

    // -- Steering angle conversion -------------------------------------------

    #[test]
    fn steering_full_left() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_STEERING, -1.0);
        let t = ets2::parse_scs_packet(&data)?;
        // -1.0 * 0.6109 ≈ -0.6109 rad
        assert!(
            (t.steering_angle - (-0.6109)).abs() < 0.01,
            "full left steering should be ~-0.6109 rad, got {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn steering_full_right() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_STEERING, 1.0);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            (t.steering_angle - 0.6109).abs() < 0.01,
            "full right steering should be ~0.6109 rad, got {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn steering_centre() -> TestResult {
        let data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            t.steering_angle.abs() < 0.001,
            "centre steering should be ~0, got {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn steering_overclamped() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_STEERING, 5.0);
        let t = ets2::parse_scs_packet(&data)?;
        // Clamped to 1.0 * 0.6109
        assert!(
            (t.steering_angle - 0.6109).abs() < 0.01,
            "over-range steering must be clamped, got {}",
            t.steering_angle
        );
        Ok(())
    }

    // -- Extended fields and adapter traits -----------------------------------

    #[test]
    fn throttle_brake_clutch_parsed() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_THROTTLE, 0.9);
        write_f32_le(&mut data, OFF_BRAKE, 0.4);
        write_f32_le(&mut data, OFF_CLUTCH, 0.2);
        let t = ets2::parse_scs_packet(&data)?;
        assert!((t.throttle - 0.9).abs() < 0.001);
        assert!((t.brake - 0.4).abs() < 0.001);
        assert!((t.clutch - 0.2).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn max_rpm_parsed() -> TestResult {
        let mut data = make_scs(20.0, 1500.0, 4, 0.5, 0.3);
        write_f32_le(&mut data, OFF_MAX_RPM, 2500.0);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            (t.max_rpm - 2500.0).abs() < 0.1,
            "max_rpm should be 2500.0, got {}",
            t.max_rpm
        );
        Ok(())
    }

    #[test]
    fn engine_load_in_extended() -> TestResult {
        let data = make_scs(20.0, 1500.0, 4, 0.5, 0.8);
        let t = ets2::parse_scs_packet(&data)?;
        match t.extended.get("engine_load") {
            Some(TelemetryValue::Float(v)) => {
                assert!(
                    (*v - 0.8).abs() < 0.001,
                    "engine_load should be 0.8, got {v}"
                );
            }
            other => return Err(format!("expected Float for engine_load, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn speed_nonnegative() -> TestResult {
        let data = make_scs(-5.0, 1000.0, 1, 0.5, 0.3);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            t.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn ffb_scalar_within_range() -> TestResult {
        let data = make_scs(100.0, 2000.0, 8, 0.3, 1.0);
        let t = ets2::parse_scs_packet(&data)?;
        assert!(
            t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            t.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn adapter_game_id_ets2() {
        let adapter = Ets2Adapter::with_variant(ets2::Ets2Variant::Ets2);
        assert_eq!(adapter.game_id(), "ets2");
    }

    #[test]
    fn adapter_game_id_ats() {
        let adapter = Ets2Adapter::with_variant(ets2::Ets2Variant::Ats);
        assert_eq!(adapter.game_id(), "ats");
    }

    #[test]
    fn adapter_update_rate_50ms() {
        let adapter = Ets2Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(50));
    }

    #[test]
    fn normalize_delegates_to_parse_scs() -> TestResult {
        let adapter = Ets2Adapter::new();
        let data = make_scs(30.0, 2000.0, 5, 0.6, 0.4);
        let t = adapter.normalize(&data)?;
        assert!((t.speed_ms - 30.0).abs() < 0.01);
        assert!((t.rpm - 2000.0).abs() < 0.1);
        assert_eq!(t.gear, 5);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Gran Turismo 7 deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod gt7_deep {
    use super::*;

    const PACKET_SIZE: usize = gran_turismo_7::PACKET_SIZE;
    const PACKET_SIZE_TYPE2: usize = gran_turismo_7::PACKET_SIZE_TYPE2;
    const PACKET_SIZE_TYPE3: usize = gran_turismo_7::PACKET_SIZE_TYPE3;
    const MAGIC: u32 = gran_turismo_7::MAGIC;
    const OFF_MAGIC: usize = gran_turismo_7::OFF_MAGIC;

    // Field offsets — from the adapter's public/crate-visible constants.
    // We replicate them here because integration tests can't access non-pub consts.
    const OFF_ENGINE_RPM: usize = 0x3C;
    const OFF_FUEL_LEVEL: usize = 0x44;
    const OFF_FUEL_CAPACITY: usize = 0x48;
    const OFF_SPEED_MS: usize = 0x4C;
    const OFF_WATER_TEMP: usize = 0x58;
    const OFF_TIRE_TEMP_FL: usize = 0x60;
    const OFF_TIRE_TEMP_FR: usize = 0x64;
    const OFF_TIRE_TEMP_RL: usize = 0x68;
    const OFF_TIRE_TEMP_RR: usize = 0x6C;
    const OFF_LAP_COUNT: usize = 0x74;
    const OFF_BEST_LAP_MS: usize = 0x78;
    const OFF_LAST_LAP_MS: usize = 0x7C;
    const OFF_CURRENT_LAP_MS: usize = 0x80;
    const OFF_POSITION: usize = 0x84;
    const OFF_MAX_ALERT_RPM: usize = 0x8A;
    const OFF_FLAGS: usize = 0x8E;
    const OFF_GEAR_BYTE: usize = 0x90;
    const OFF_THROTTLE: usize = 0x91;
    const OFF_BRAKE: usize = 0x92;
    const OFF_CAR_CODE: usize = 0x124;

    // Extended field offsets (Type2)
    const OFF_WHEEL_ROTATION: usize = 0x128;
    const OFF_SWAY: usize = 0x130;
    const OFF_HEAVE: usize = 0x134;
    const OFF_SURGE: usize = 0x138;

    // Extended field offsets (Type3)
    const OFF_CAR_TYPE_BYTE3: usize = 0x13E;
    const OFF_ENERGY_RECOVERY: usize = 0x150;

    // GT7 flag bits
    const FLAG_PAUSED: u16 = 1 << 1;
    const FLAG_REV_LIMIT: u16 = 1 << 5;
    const FLAG_ASM_ACTIVE: u16 = 1 << 10;
    const FLAG_TCS_ACTIVE: u16 = 1 << 11;

    fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
        buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
    }

    fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn write_i16(buf: &mut [u8], offset: usize, val: i16) {
        buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
    }

    /// Build a standard 296-byte decrypted buffer with GT7 magic set.
    fn make_buf() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    /// Build a 316-byte (Type2) decrypted buffer with GT7 magic set.
    fn make_type2() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    /// Build a 344-byte (Type3) decrypted buffer with GT7 magic set.
    fn make_type3() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    // -- Encrypted UDP format (Salsa20) --------------------------------------

    #[test]
    fn rejects_short_packet() -> TestResult {
        let short = vec![0u8; 100];
        assert!(
            gran_turismo_7::parse_decrypted_ext(&short).is_err(),
            "packet shorter than 296 bytes must fail"
        );
        Ok(())
    }

    #[test]
    fn wrong_magic_via_normalize_returns_err() -> TestResult {
        // Adapter.normalize() calls decrypt_and_parse which checks magic after
        // Salsa20 decryption. A zero-filled buffer will have wrong magic.
        let adapter = GranTurismo7Adapter::new();
        let buf = vec![0u8; PACKET_SIZE];
        assert!(adapter.normalize(&buf).is_err());
        Ok(())
    }

    #[test]
    fn accepts_valid_magic() -> TestResult {
        let buf = make_buf();
        let _t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        Ok(())
    }

    // -- Position / speed / RPM extraction -----------------------------------

    #[test]
    fn speed_extracted_correctly() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_SPEED_MS, 44.44);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.speed_ms - 44.44).abs() < 0.01,
            "speed_ms should be 44.44, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_negative_clamped_to_zero() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_SPEED_MS, -10.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            t.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn rpm_extracted() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_ENGINE_RPM, 7500.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.rpm - 7500.0).abs() < 0.1,
            "rpm should be 7500.0, got {}",
            t.rpm
        );
        Ok(())
    }

    #[test]
    fn max_rpm_from_alert_rpm() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_MAX_ALERT_RPM, 9000);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.max_rpm - 9000.0).abs() < 0.1,
            "max_rpm should be 9000.0, got {}",
            t.max_rpm
        );
        Ok(())
    }

    #[test]
    fn position_extracted() -> TestResult {
        let mut buf = make_buf();
        write_i16(&mut buf, OFF_POSITION, 3);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.position, 3, "position should be 3");
        Ok(())
    }

    // -- Flag interpretation -------------------------------------------------

    #[test]
    fn flag_tcs_active() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_FLAGS, FLAG_TCS_ACTIVE);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.flags.traction_control, "TCS flag should be set");
        assert!(!t.flags.abs_active, "ASM should NOT be set");
        Ok(())
    }

    #[test]
    fn flag_asm_active() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_FLAGS, FLAG_ASM_ACTIVE);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.flags.abs_active, "ASM flag should map to abs_active");
        Ok(())
    }

    #[test]
    fn flag_rev_limiter() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_FLAGS, FLAG_REV_LIMIT);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.flags.engine_limiter, "REV_LIMIT flag should be set");
        Ok(())
    }

    #[test]
    fn flag_paused() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_FLAGS, FLAG_PAUSED);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.flags.session_paused, "PAUSED flag should be set");
        Ok(())
    }

    #[test]
    fn multiple_flags_combined() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_FLAGS, FLAG_TCS_ACTIVE | FLAG_PAUSED);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.flags.traction_control);
        assert!(t.flags.session_paused);
        assert!(!t.flags.abs_active);
        assert!(!t.flags.engine_limiter);
        Ok(())
    }

    // -- Gear encoding -------------------------------------------------------

    #[test]
    fn gear_neutral() -> TestResult {
        let mut buf = make_buf();
        buf[OFF_GEAR_BYTE] = 0x00;
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.gear, 0, "gear nibble 0 = neutral");
        Ok(())
    }

    #[test]
    fn gear_forward_low_nibble() -> TestResult {
        let mut buf = make_buf();
        // Low nibble = 5 (current gear), high nibble = 6 (suggested gear)
        buf[OFF_GEAR_BYTE] = (6 << 4) | 5;
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.gear, 5, "low nibble should be the current gear");
        Ok(())
    }

    #[test]
    fn gear_max_valid() -> TestResult {
        let mut buf = make_buf();
        buf[OFF_GEAR_BYTE] = 0x08; // low nibble = 8
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.gear, 8);
        Ok(())
    }

    #[test]
    fn gear_out_of_range_nibble_maps_to_neutral() -> TestResult {
        let mut buf = make_buf();
        buf[OFF_GEAR_BYTE] = 0x0F; // low nibble = 15, out of valid 0–8 range
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.gear, 0, "out-of-range nibble should map to neutral (0)");
        Ok(())
    }

    // -- Throttle / brake / fuel / laps / temps ------------------------------

    #[test]
    fn throttle_and_brake_normalised() -> TestResult {
        let mut buf = make_buf();
        buf[OFF_THROTTLE] = 255;
        buf[OFF_BRAKE] = 128;
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!((t.throttle - 1.0).abs() < 0.001, "255 → 1.0");
        assert!((t.brake - 128.0 / 255.0).abs() < 0.001, "128 → ~0.502");
        Ok(())
    }

    #[test]
    fn fuel_percent_from_level_and_capacity() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_FUEL_LEVEL, 30.0);
        write_f32_le(&mut buf, OFF_FUEL_CAPACITY, 60.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.fuel_percent - 0.5).abs() < 0.001,
            "fuel_percent should be 0.5, got {}",
            t.fuel_percent
        );
        Ok(())
    }

    #[test]
    fn fuel_percent_zero_when_no_capacity() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_FUEL_LEVEL, 10.0);
        write_f32_le(&mut buf, OFF_FUEL_CAPACITY, 0.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.fuel_percent, 0.0, "zero capacity → 0.0 fuel_percent");
        Ok(())
    }

    #[test]
    fn lap_count_extracted() -> TestResult {
        let mut buf = make_buf();
        write_u16(&mut buf, OFF_LAP_COUNT, 7);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.lap, 7);
        Ok(())
    }

    #[test]
    fn best_lap_time_conversion() -> TestResult {
        let mut buf = make_buf();
        // 1:23.456 = 83456 ms → 83.456 s
        write_i32(&mut buf, OFF_BEST_LAP_MS, 83_456);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.best_lap_time_s - 83.456).abs() < 0.001,
            "best_lap_time_s should be 83.456, got {}",
            t.best_lap_time_s
        );
        Ok(())
    }

    #[test]
    fn negative_best_lap_yields_zero() -> TestResult {
        let mut buf = make_buf();
        write_i32(&mut buf, OFF_BEST_LAP_MS, -1);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.best_lap_time_s, 0.0);
        Ok(())
    }

    #[test]
    fn tire_temps_clamped_to_u8() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_TIRE_TEMP_FL, 85.5);
        write_f32_le(&mut buf, OFF_TIRE_TEMP_FR, 300.0); // above u8 max
        write_f32_le(&mut buf, OFF_TIRE_TEMP_RL, -10.0); // below zero
        write_f32_le(&mut buf, OFF_TIRE_TEMP_RR, 100.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.tire_temps_c[0], 85);
        assert_eq!(t.tire_temps_c[1], 255, "300 → clamped to 255");
        assert_eq!(t.tire_temps_c[2], 0, "-10 → clamped to 0");
        assert_eq!(t.tire_temps_c[3], 100);
        Ok(())
    }

    #[test]
    fn water_temp_extracted() -> TestResult {
        let mut buf = make_buf();
        write_f32_le(&mut buf, OFF_WATER_TEMP, 88.0);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.engine_temp_c - 88.0).abs() < 0.1,
            "engine_temp_c should be 88.0, got {}",
            t.engine_temp_c
        );
        Ok(())
    }

    #[test]
    fn last_lap_time_conversion() -> TestResult {
        let mut buf = make_buf();
        write_i32(&mut buf, OFF_LAST_LAP_MS, 65_432);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.last_lap_time_s - 65.432).abs() < 0.001,
            "last_lap_time_s should be 65.432, got {}",
            t.last_lap_time_s
        );
        Ok(())
    }

    #[test]
    fn current_lap_time_conversion() -> TestResult {
        let mut buf = make_buf();
        write_i32(&mut buf, OFF_CURRENT_LAP_MS, 12_345);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.current_lap_time_s - 12.345).abs() < 0.001,
            "current_lap_time_s should be 12.345, got {}",
            t.current_lap_time_s
        );
        Ok(())
    }

    #[test]
    fn car_code_to_car_id() -> TestResult {
        let mut buf = make_buf();
        write_i32(&mut buf, OFF_CAR_CODE, 123);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(t.car_id.as_deref(), Some("gt7_123"));
        Ok(())
    }

    #[test]
    fn zero_car_code_no_car_id() -> TestResult {
        let buf = make_buf();
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.car_id.is_none());
        Ok(())
    }

    // -- Extended telemetry (PacketType2 / Type3) ----------------------------

    #[test]
    fn type2_wheel_rotation_as_steering_angle() -> TestResult {
        let mut buf = make_type2();
        write_f32_le(&mut buf, OFF_WHEEL_ROTATION, 2.5);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(
            (t.steering_angle - 2.5).abs() < 0.001,
            "steering_angle should be wheel rotation, got {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn type2_motion_data_mapped() -> TestResult {
        let mut buf = make_type2();
        write_f32_le(&mut buf, OFF_SWAY, 0.3);
        write_f32_le(&mut buf, OFF_HEAVE, -0.2);
        write_f32_le(&mut buf, OFF_SURGE, 0.8);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!((t.lateral_g - 0.3).abs() < 0.001);
        assert!((t.vertical_g - (-0.2)).abs() < 0.001);
        assert!((t.longitudinal_g - 0.8).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn type2_motion_in_extended_data() -> TestResult {
        let mut buf = make_type2();
        write_f32_le(&mut buf, OFF_SWAY, 1.1);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_sway"),
            Some(&TelemetryValue::Float(1.1))
        );
        Ok(())
    }

    #[test]
    fn type3_energy_recovery() -> TestResult {
        let mut buf = make_type3();
        write_f32_le(&mut buf, OFF_ENERGY_RECOVERY, 55.5);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_energy_recovery"),
            Some(&TelemetryValue::Float(55.5))
        );
        Ok(())
    }

    #[test]
    fn type3_car_type_electric() -> TestResult {
        let mut buf = make_type3();
        buf[OFF_CAR_TYPE_BYTE3] = 4;
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_car_type"),
            Some(&TelemetryValue::Integer(4))
        );
        Ok(())
    }

    #[test]
    fn type3_also_includes_type2_fields() -> TestResult {
        let mut buf = make_type3();
        write_f32_le(&mut buf, OFF_WHEEL_ROTATION, -1.0);
        write_f32_le(&mut buf, OFF_SURGE, 0.5);
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!((t.steering_angle - (-1.0)).abs() < 0.001);
        assert!((t.longitudinal_g - 0.5).abs() < 0.001);
        assert!(t.get_extended("gt7_energy_recovery").is_some());
        assert!(t.get_extended("gt7_car_type").is_some());
        Ok(())
    }

    #[test]
    fn standard_packet_has_no_extended_fields() -> TestResult {
        let buf = make_buf();
        let t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!(t.extended.is_empty(), "296-byte packet → no extended data");
        Ok(())
    }

    // -- Adapter trait -------------------------------------------------------

    #[test]
    fn adapter_game_id() {
        let adapter = GranTurismo7Adapter::new();
        assert_eq!(adapter.game_id(), "gran_turismo_7");
    }

    #[test]
    fn adapter_update_rate_17ms() {
        let adapter = GranTurismo7Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
    }
}
