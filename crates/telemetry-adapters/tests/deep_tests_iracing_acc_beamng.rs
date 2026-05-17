//! Deep tests for iRacing, ACC, and BeamNG telemetry adapters.
//!
//! Tests exercise parsing, normalization, boundary values, and flag handling
//! through the public `TelemetryAdapter::normalize()` API.

use openracing_telemetry_adapters::{
    ACCAdapter, BeamNGAdapter, IRacingAdapter, TelemetryAdapter, TelemetryValue,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ══════════════════════════════════════════════════════════════════════════════
// iRacing deep tests
// ══════════════════════════════════════════════════════════════════════════════

mod iracing_deep {
    use super::*;

    // iRacing session flag constants (from irsdk_defines.h)
    const FLAG_CHECKERED: u32 = 0x0000_0001;
    const FLAG_GREEN: u32 = 0x0000_0004;
    const FLAG_YELLOW: u32 = 0x0000_0008;
    const FLAG_RED: u32 = 0x0000_0010;
    const FLAG_BLUE: u32 = 0x0000_0020;

    // IRacingData repr(C) layout offsets (verified against source struct)
    const OFF_SESSION_TIME: usize = 0;
    const OFF_SESSION_FLAGS: usize = 4;
    const OFF_SPEED: usize = 8;
    const OFF_RPM: usize = 12;
    const OFF_GEAR: usize = 16; // i8 (+ 3 padding bytes)
    const OFF_THROTTLE: usize = 20;
    const OFF_BRAKE: usize = 24;
    const OFF_STEERING_ANGLE: usize = 28;
    const OFF_FUEL_LEVEL: usize = 88;
    const OFF_FUEL_LEVEL_PCT: usize = 92;
    const OFF_ON_PIT_ROAD: usize = 96;
    const OFF_CLUTCH: usize = 100;
    const OFF_POSITION: usize = 104;
    const OFF_LAP_CURRENT: usize = 80;
    const OFF_LAP_BEST_TIME: usize = 84;
    const OFF_LAP_LAST_TIME: usize = 108;
    const OFF_LAP_CURRENT_TIME: usize = 112;
    const OFF_LF_TEMP: usize = 116;
    const OFF_RF_TEMP: usize = 120;
    const OFF_LR_TEMP: usize = 124;
    const OFF_RR_TEMP: usize = 128;
    const OFF_LF_PRESSURE: usize = 132;
    const OFF_RF_PRESSURE: usize = 136;
    const OFF_LR_PRESSURE: usize = 140;
    const OFF_RR_PRESSURE: usize = 144;
    const OFF_LAT_ACCEL: usize = 148;
    const OFF_LONG_ACCEL: usize = 152;
    const OFF_VERT_ACCEL: usize = 156;
    const OFF_WATER_TEMP: usize = 160;
    const OFF_CAR_PATH: usize = 164;
    const OFF_TRACK_NAME: usize = 228;
    const IRACING_DATA_SIZE: usize = 292;

    // Conversion factor: kPa → PSI
    const KPA_TO_PSI: f32 = 0.145_038;

    fn make_iracing_data() -> Vec<u8> {
        vec![0u8; IRACING_DATA_SIZE]
    }

    fn set_f32(buf: &mut [u8], offset: usize, val: f32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn set_u32(buf: &mut [u8], offset: usize, val: u32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn set_i32(buf: &mut [u8], offset: usize, val: i32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn set_string(buf: &mut [u8], offset: usize, s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len().min(63);
        buf[offset..offset + len].copy_from_slice(&bytes[..len]);
        buf[offset + len] = 0;
    }

    #[test]
    fn iracing_normalize_speed() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_SPEED, 55.5);
        let result = adapter.normalize(&data)?;
        assert!((result.speed_ms - 55.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn iracing_normalize_rpm() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_RPM, 7200.0);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 7200.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn iracing_gear_forward() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        data[OFF_GEAR] = 4_i8 as u8;
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 4);
        Ok(())
    }

    #[test]
    fn iracing_gear_reverse() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        data[OFF_GEAR] = (-1_i8) as u8;
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn iracing_gear_neutral() -> TestResult {
        let adapter = IRacingAdapter::new();
        let data = make_iracing_data();
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn iracing_throttle_brake_clutch() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_THROTTLE, 0.75);
        set_f32(&mut data, OFF_BRAKE, 0.3);
        set_f32(&mut data, OFF_CLUTCH, 0.5);
        let result = adapter.normalize(&data)?;
        assert!((result.throttle - 0.75).abs() < 0.001);
        assert!((result.brake - 0.3).abs() < 0.001);
        assert!((result.clutch - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn iracing_steering_angle() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_STEERING_ANGLE, 1.57);
        let result = adapter.normalize(&data)?;
        assert!((result.steering_angle - 1.57).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn iracing_flag_yellow() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(&mut data, OFF_SESSION_FLAGS, FLAG_YELLOW);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.yellow_flag);
        assert!(!result.flags.green_flag);
        assert!(!result.flags.blue_flag);
        assert!(!result.flags.checkered_flag);
        Ok(())
    }

    #[test]
    fn iracing_flag_blue() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(&mut data, OFF_SESSION_FLAGS, FLAG_BLUE);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.blue_flag);
        assert!(!result.flags.yellow_flag);
        Ok(())
    }

    #[test]
    fn iracing_flag_green() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(&mut data, OFF_SESSION_FLAGS, FLAG_GREEN);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.green_flag);
        Ok(())
    }

    #[test]
    fn iracing_flag_checkered() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(&mut data, OFF_SESSION_FLAGS, FLAG_CHECKERED);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.checkered_flag);
        Ok(())
    }

    #[test]
    fn iracing_flag_red() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(&mut data, OFF_SESSION_FLAGS, FLAG_RED);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.red_flag);
        Ok(())
    }

    #[test]
    fn iracing_combined_flags() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_u32(
            &mut data,
            OFF_SESSION_FLAGS,
            FLAG_YELLOW | FLAG_GREEN | FLAG_CHECKERED,
        );
        let result = adapter.normalize(&data)?;
        assert!(result.flags.yellow_flag);
        assert!(result.flags.green_flag);
        assert!(result.flags.checkered_flag);
        assert!(!result.flags.red_flag);
        assert!(!result.flags.blue_flag);
        Ok(())
    }

    #[test]
    fn iracing_fuel_and_engine_temp() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_FUEL_LEVEL, 42.5);
        set_f32(&mut data, OFF_FUEL_LEVEL_PCT, 0.65);
        set_f32(&mut data, OFF_WATER_TEMP, 92.0);
        let result = adapter.normalize(&data)?;
        assert!((result.fuel_percent - 0.65).abs() < 0.001);
        assert!((result.engine_temp_c - 92.0).abs() < 0.1);
        assert_eq!(
            result.extended.get("fuel_level"),
            Some(&TelemetryValue::Float(42.5))
        );
        Ok(())
    }

    #[test]
    fn iracing_tire_temps() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_LF_TEMP, 85.0);
        set_f32(&mut data, OFF_RF_TEMP, 90.0);
        set_f32(&mut data, OFF_LR_TEMP, 78.0);
        set_f32(&mut data, OFF_RR_TEMP, 82.0);
        let result = adapter.normalize(&data)?;
        assert_eq!(result.tire_temps_c, [85, 90, 78, 82]);
        Ok(())
    }

    #[test]
    fn iracing_tire_pressures_kpa_to_psi() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        let kpa_25psi = 25.0_f32 / KPA_TO_PSI;
        set_f32(&mut data, OFF_LF_PRESSURE, kpa_25psi);
        set_f32(&mut data, OFF_RF_PRESSURE, kpa_25psi);
        set_f32(&mut data, OFF_LR_PRESSURE, kpa_25psi);
        set_f32(&mut data, OFF_RR_PRESSURE, kpa_25psi);
        let result = adapter.normalize(&data)?;
        for p in &result.tire_pressures_psi {
            assert!((*p - 25.0).abs() < 0.1, "pressure {p} not ≈ 25 PSI");
        }
        Ok(())
    }

    #[test]
    fn iracing_position_and_laps() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_i32(&mut data, OFF_POSITION, 3);
        set_i32(&mut data, OFF_LAP_CURRENT, 12);
        set_f32(&mut data, OFF_LAP_BEST_TIME, 91.234);
        set_f32(&mut data, OFF_LAP_LAST_TIME, 92.1);
        set_f32(&mut data, OFF_LAP_CURRENT_TIME, 45.6);
        let result = adapter.normalize(&data)?;
        assert_eq!(result.position, 3);
        assert_eq!(result.lap, 12);
        assert!((result.best_lap_time_s - 91.234).abs() < 0.01);
        assert!((result.last_lap_time_s - 92.1).abs() < 0.01);
        assert!((result.current_lap_time_s - 45.6).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn iracing_g_forces_mps2_to_g() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_LAT_ACCEL, 9.806_65);
        set_f32(&mut data, OFF_LONG_ACCEL, -9.806_65);
        set_f32(&mut data, OFF_VERT_ACCEL, 19.613_3);
        let result = adapter.normalize(&data)?;
        assert!((result.lateral_g - 1.0).abs() < 0.01);
        assert!((result.longitudinal_g + 1.0).abs() < 0.01);
        assert!((result.vertical_g - 2.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn iracing_car_and_track_strings() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_string(&mut data, OFF_CAR_PATH, "gt3_ferrari_296");
        set_string(&mut data, OFF_TRACK_NAME, "spa");
        let result = adapter.normalize(&data)?;
        assert_eq!(result.car_id, Some("gt3_ferrari_296".to_string()));
        assert_eq!(result.track_id, Some("spa".to_string()));
        Ok(())
    }

    #[test]
    fn iracing_pit_road_flag() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_i32(&mut data, OFF_ON_PIT_ROAD, 1);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.in_pits);
        Ok(())
    }

    #[test]
    fn iracing_too_short_buffer_rejected() -> TestResult {
        let adapter = IRacingAdapter::new();
        let short = vec![0u8; 32];
        assert!(adapter.normalize(&short).is_err());
        Ok(())
    }

    #[test]
    fn iracing_zero_data_normalizes() -> TestResult {
        let adapter = IRacingAdapter::new();
        let data = make_iracing_data();
        let result = adapter.normalize(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn iracing_session_time_in_extended() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_SESSION_TIME, 1234.5);
        let result = adapter.normalize(&data)?;
        assert_eq!(
            result.extended.get("session_time"),
            Some(&TelemetryValue::Float(1234.5))
        );
        Ok(())
    }

    #[test]
    fn iracing_trailing_bytes_accepted() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = make_iracing_data();
        set_f32(&mut data, OFF_RPM, 4000.0);
        data.extend_from_slice(&[0xFF; 32]);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 4000.0).abs() < 0.1);
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ACC deep tests
// ══════════════════════════════════════════════════════════════════════════════

mod acc_deep {
    use super::*;

    const MSG_REGISTRATION_RESULT: u8 = 1;
    const MSG_REALTIME_CAR_UPDATE: u8 = 3;
    const MSG_TRACK_DATA: u8 = 5;

    fn push_acc_string(buf: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(bytes);
    }

    fn push_lap(buf: &mut Vec<u8>, lap_time_ms: i32) {
        buf.extend_from_slice(&lap_time_ms.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // car index
        buf.extend_from_slice(&0u16.to_le_bytes()); // driver index
        buf.push(0); // split count
        buf.push(0); // is invalid
        buf.push(1); // valid for best
        buf.push(0); // outlap
        buf.push(0); // inlap
    }

    fn build_car_update(
        car_index: u16,
        gear_raw: u8,
        speed_kmh: u16,
        position: u16,
        laps: u16,
        car_location: u8,
    ) -> Vec<u8> {
        let mut pkt = vec![MSG_REALTIME_CAR_UPDATE];
        pkt.extend_from_slice(&car_index.to_le_bytes());
        pkt.extend_from_slice(&0u16.to_le_bytes()); // driver index
        pkt.push(1); // driver count
        pkt.push(gear_raw);
        pkt.extend_from_slice(&0.0f32.to_le_bytes()); // world pos x
        pkt.extend_from_slice(&0.0f32.to_le_bytes()); // world pos y
        pkt.extend_from_slice(&0.0f32.to_le_bytes()); // yaw
        pkt.push(car_location);
        pkt.extend_from_slice(&speed_kmh.to_le_bytes());
        pkt.extend_from_slice(&position.to_le_bytes());
        pkt.extend_from_slice(&position.to_le_bytes()); // cup position
        pkt.extend_from_slice(&position.to_le_bytes()); // track position
        pkt.extend_from_slice(&0.5f32.to_le_bytes()); // spline position
        pkt.extend_from_slice(&laps.to_le_bytes());
        pkt.extend_from_slice(&0i32.to_le_bytes()); // delta
        push_lap(&mut pkt, 91_000); // best session lap
        push_lap(&mut pkt, 92_000); // last lap
        push_lap(&mut pkt, 45_000); // current lap
        pkt
    }

    fn build_track_data(track_name: &str) -> Vec<u8> {
        let mut pkt = vec![MSG_TRACK_DATA];
        pkt.extend_from_slice(&0i32.to_le_bytes()); // connection id
        push_acc_string(&mut pkt, track_name);
        pkt.extend_from_slice(&1i32.to_le_bytes()); // track id
        pkt.extend_from_slice(&5793i32.to_le_bytes()); // track meters
        pkt.push(0); // camera sets
        pkt.push(0); // hud pages
        pkt
    }

    fn build_registration_result(connection_id: i32, success: bool, readonly_byte: u8) -> Vec<u8> {
        let mut pkt = vec![MSG_REGISTRATION_RESULT];
        pkt.extend_from_slice(&connection_id.to_le_bytes());
        pkt.push(u8::from(success));
        pkt.push(readonly_byte);
        push_acc_string(&mut pkt, "");
        pkt
    }

    #[test]
    fn acc_speed_conversion_kmh_to_ms() -> TestResult {
        let adapter = ACCAdapter::new();
        // 180 km/h = 50 m/s
        let pkt = build_car_update(1, 6, 180, 1, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert!((result.speed_ms - 50.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn acc_gear_reverse() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 0, 50, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn acc_gear_neutral() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 1, 0, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn acc_gear_fifth() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 6, 200, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, 5);
        Ok(())
    }

    #[test]
    fn acc_position_and_laps() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 3, 12, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.position, 3);
        assert_eq!(result.lap, 12);
        Ok(())
    }

    #[test]
    fn acc_lap_times_ms_to_seconds() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 1, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert!((result.best_lap_time_s - 91.0).abs() < 0.01);
        assert!((result.last_lap_time_s - 92.0).abs() < 0.01);
        assert!((result.current_lap_time_s - 45.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn acc_pit_flags_car_location_2() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 2, 60, 1, 5, 2);
        let result = adapter.normalize(&pkt)?;
        assert!(result.flags.in_pits);
        assert!(result.flags.pit_limiter);
        Ok(())
    }

    #[test]
    fn acc_on_track_no_pit_flags() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 200, 1, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert!(!result.flags.in_pits);
        assert!(!result.flags.pit_limiter);
        Ok(())
    }

    #[test]
    fn acc_car_id_format() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(7, 4, 120, 1, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.car_id, Some("car_7".to_string()));
        Ok(())
    }

    #[test]
    fn acc_extended_fields_present() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 2, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert!(result.extended.contains_key("cup_position"));
        assert!(result.extended.contains_key("track_position"));
        assert!(result.extended.contains_key("delta_ms"));
        assert!(result.extended.contains_key("spline_position"));
        Ok(())
    }

    #[test]
    fn acc_empty_packet_rejected() -> TestResult {
        let adapter = ACCAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
        Ok(())
    }

    #[test]
    fn acc_truncated_car_update_rejected() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 1, 5, 1);
        let truncated = &pkt[..pkt.len() - 1];
        assert!(adapter.normalize(truncated).is_err());
        Ok(())
    }

    #[test]
    fn acc_registration_result_no_telemetry() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_registration_result(42, true, 1);
        assert!(adapter.normalize(&pkt).is_err());
        Ok(())
    }

    #[test]
    fn acc_track_data_no_telemetry() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_track_data("monza");
        assert!(adapter.normalize(&pkt).is_err());
        Ok(())
    }

    #[test]
    fn acc_speed_zero() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 1, 0, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn acc_high_speed_boundary() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 6, u16::MAX, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert!(result.speed_ms > 0.0);
        assert!(result.speed_ms.is_finite());
        Ok(())
    }

    #[test]
    fn acc_position_clamped_to_u8() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 300, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.position, 255);
        Ok(())
    }

    #[test]
    fn acc_gear_boundary_max_byte() -> TestResult {
        let adapter = ACCAdapter::new();
        // gear_raw=255 → (255−1)=254, clamped to i8::MAX=127
        let pkt = build_car_update(1, 255, 120, 1, 1, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, 127);
        Ok(())
    }

    #[test]
    fn acc_unknown_message_type() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = vec![255u8, 0, 0, 0];
        assert!(adapter.normalize(&pkt).is_err());
        Ok(())
    }

    #[test]
    fn acc_adapter_game_id_and_rate() -> TestResult {
        let adapter = ACCAdapter::new();
        assert_eq!(adapter.game_id(), "acc");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn acc_spline_position_in_extended() -> TestResult {
        let adapter = ACCAdapter::new();
        let pkt = build_car_update(1, 4, 120, 1, 5, 1);
        let result = adapter.normalize(&pkt)?;
        assert_eq!(
            result.extended.get("spline_position"),
            Some(&TelemetryValue::Float(0.5))
        );
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// BeamNG deep tests
// ══════════════════════════════════════════════════════════════════════════════

mod beamng_deep {
    use super::*;

    const OUTGAUGE_PACKET_SIZE: usize = 92;

    // OutGauge byte offsets
    const OFF_GEAR: usize = 10;
    const OFF_SPEED: usize = 12;
    const OFF_RPM: usize = 16;
    const OFF_TURBO: usize = 20;
    const OFF_ENG_TEMP: usize = 24;
    const OFF_FUEL: usize = 28;
    const OFF_OIL_PRESSURE: usize = 32;
    const OFF_OIL_TEMP: usize = 36;
    const OFF_SHOW_LIGHTS: usize = 44;
    const OFF_THROTTLE: usize = 48;
    const OFF_BRAKE: usize = 52;
    const OFF_CLUTCH: usize = 56;

    // Dashboard light flags
    const DL_SHIFT: u32 = 0x0001;
    const DL_PITSPEED: u32 = 0x0008;
    const DL_TC: u32 = 0x0010;
    const DL_ABS: u32 = 0x0400;

    fn make_outgauge() -> Vec<u8> {
        vec![0u8; OUTGAUGE_PACKET_SIZE]
    }

    fn set_f32(buf: &mut [u8], offset: usize, val: f32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn set_u32(buf: &mut [u8], offset: usize, val: u32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    }

    #[test]
    fn beamng_adapter_game_id_and_rate() -> TestResult {
        let adapter = BeamNGAdapter::new();
        assert_eq!(adapter.game_id(), "beamng_drive");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn beamng_custom_port() -> TestResult {
        let adapter = BeamNGAdapter::new().with_port(5555);
        assert_eq!(adapter.game_id(), "beamng_drive");
        Ok(())
    }

    #[test]
    fn beamng_speed_normalization() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_SPEED, 27.78);
        let result = adapter.normalize(&data)?;
        assert!((result.speed_ms - 27.78).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn beamng_rpm_normalization() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_RPM, 6500.0);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 6500.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn beamng_gear_reverse() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        data[OFF_GEAR] = 0;
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn beamng_gear_neutral() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        data[OFF_GEAR] = 1;
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn beamng_gear_forward() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        data[OFF_GEAR] = 4; // OutGauge 4 = 3rd gear
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 3);
        Ok(())
    }

    #[test]
    fn beamng_high_gear() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        data[OFF_GEAR] = 9; // OutGauge 9 = 8th gear
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 8);
        Ok(())
    }

    #[test]
    fn beamng_throttle_brake_clutch() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_THROTTLE, 0.8);
        set_f32(&mut data, OFF_BRAKE, 0.6);
        set_f32(&mut data, OFF_CLUTCH, 0.3);
        let result = adapter.normalize(&data)?;
        assert!((result.throttle - 0.8).abs() < 0.001);
        assert!((result.brake - 0.6).abs() < 0.001);
        assert!((result.clutch - 0.3).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn beamng_throttle_clamped_above_one() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_THROTTLE, 1.5);
        set_f32(&mut data, OFF_BRAKE, 2.0);
        let result = adapter.normalize(&data)?;
        assert!((result.throttle - 1.0).abs() < 0.001);
        assert!((result.brake - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn beamng_fuel_and_engine_temp() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_FUEL, 0.75);
        set_f32(&mut data, OFF_ENG_TEMP, 92.5);
        let result = adapter.normalize(&data)?;
        assert!((result.fuel_percent - 0.75).abs() < 0.001);
        assert!((result.engine_temp_c - 92.5).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn beamng_turbo_oil_in_extended() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_f32(&mut data, OFF_TURBO, 1.2);
        set_f32(&mut data, OFF_OIL_PRESSURE, 3.5);
        set_f32(&mut data, OFF_OIL_TEMP, 105.0);
        let result = adapter.normalize(&data)?;
        assert_eq!(
            result.extended.get("turbo_bar"),
            Some(&TelemetryValue::Float(1.2))
        );
        assert_eq!(
            result.extended.get("oil_pressure_bar"),
            Some(&TelemetryValue::Float(3.5))
        );
        assert_eq!(
            result.extended.get("oil_temp_c"),
            Some(&TelemetryValue::Float(105.0))
        );
        Ok(())
    }

    #[test]
    fn beamng_dash_light_pit_limiter() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_u32(&mut data, OFF_SHOW_LIGHTS, DL_PITSPEED);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.pit_limiter);
        assert!(!result.flags.traction_control);
        assert!(!result.flags.abs_active);
        Ok(())
    }

    #[test]
    fn beamng_dash_light_tc_and_abs() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_u32(&mut data, OFF_SHOW_LIGHTS, DL_TC | DL_ABS);
        let result = adapter.normalize(&data)?;
        assert!(result.flags.traction_control);
        assert!(result.flags.abs_active);
        assert!(!result.flags.pit_limiter);
        Ok(())
    }

    #[test]
    fn beamng_shift_light_in_extended() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_u32(&mut data, OFF_SHOW_LIGHTS, DL_SHIFT);
        let result = adapter.normalize(&data)?;
        assert_eq!(
            result.extended.get("shift_light"),
            Some(&TelemetryValue::Boolean(true))
        );
        assert!(result.flags.engine_limiter);
        Ok(())
    }

    #[test]
    fn beamng_dash_lights_raw_in_extended() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_u32(&mut data, OFF_SHOW_LIGHTS, DL_SHIFT | DL_ABS);
        let result = adapter.normalize(&data)?;
        let expected_raw = (DL_SHIFT | DL_ABS) as i32;
        assert_eq!(
            result.extended.get("dash_lights_raw"),
            Some(&TelemetryValue::Integer(expected_raw))
        );
        Ok(())
    }

    #[test]
    fn beamng_no_dash_lights_raw_when_zero() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let data = make_outgauge();
        let result = adapter.normalize(&data)?;
        assert!(!result.extended.contains_key("dash_lights_raw"));
        Ok(())
    }

    #[test]
    fn beamng_packet_too_short_rejected() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let short = vec![0u8; 50];
        assert!(adapter.normalize(&short).is_err());
        Ok(())
    }

    #[test]
    fn beamng_92_byte_packet_accepted() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let data = vec![0u8; 92];
        assert!(adapter.normalize(&data).is_ok());
        Ok(())
    }

    #[test]
    fn beamng_96_byte_packet_with_id_accepted() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let data = vec![0u8; 96];
        assert!(adapter.normalize(&data).is_ok());
        Ok(())
    }

    #[test]
    fn beamng_all_combined_lights() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut data = make_outgauge();
        set_u32(
            &mut data,
            OFF_SHOW_LIGHTS,
            DL_SHIFT | DL_PITSPEED | DL_TC | DL_ABS,
        );
        let result = adapter.normalize(&data)?;
        assert!(result.flags.pit_limiter);
        assert!(result.flags.traction_control);
        assert!(result.flags.abs_active);
        assert!(result.flags.engine_limiter);
        Ok(())
    }

    #[test]
    fn beamng_zero_data_normalizes() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let data = make_outgauge();
        let result = adapter.normalize(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        Ok(())
    }
}
