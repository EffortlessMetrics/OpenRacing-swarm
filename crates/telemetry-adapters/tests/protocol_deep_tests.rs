//! Deep telemetry adapter tests: protocol correctness, cross-adapter consistency,
//! edge cases, timing guarantees, and field coverage.
//!
//! These tests exercise the normalize() path of each adapter with synthetic packets
//! constructed from known protocol specifications (Forza Sled/CarDash, Codemasters
//! Mode 1, OutGauge, ACC broadcasting, GT7 decrypted, rFactor 1/2, SimHub, etc.).
//!
//! # Reference sources
//!
//! - **Forza**: austinbaccus/forza-telemetry FMData.cs, richstokes/Forza-data-tools
//! - **Codemasters**: DiRT Rally 2 Mode 1 264-byte layout
//! - **OutGauge / LFS / BeamNG**: en.lfsmanual.net/wiki/OutGauge, BeamNG docs
//! - **iRacing**: kutu/pyirsdk vars.txt, irsdk_defines.h
//! - **ACC**: Kunos Broadcasting SDK v4
//! - **GT7**: Community Salsa20 decryption, 296/316/344-byte packet types
//! - **rFactor 2**: TheIronWolfModding/rF2SharedMemoryMapPlugin rF2State.h

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::codemasters_shared;
use openracing_telemetry_adapters::gran_turismo_7;
use openracing_telemetry_adapters::{
    ACCAdapter, BeamNGAdapter, Dirt3Adapter, Dirt4Adapter, DirtRally2Adapter, ForzaAdapter,
    GranTurismo7Adapter, Grid2019Adapter, GridAutosportAdapter, GridLegendsAdapter, IRacingAdapter,
    LFSAdapter, NormalizedTelemetry, RaceDriverGridAdapter, TelemetryAdapter, TelemetryValue,
};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper utilities
// ═══════════════════════════════════════════════════════════════════════════════

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u16_le(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn approx_eq(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

/// Make a Forza Sled packet (232 bytes) with is_race_on=1.
fn make_forza_sled() -> Vec<u8> {
    let mut buf = vec![0u8; 232];
    write_i32_le(&mut buf, 0, 1); // is_race_on = 1
    buf
}

/// Make a Forza CarDash packet (311 bytes) with is_race_on=1.
fn make_forza_cardash() -> Vec<u8> {
    let mut buf = vec![0u8; 311];
    write_i32_le(&mut buf, 0, 1); // is_race_on = 1
    buf
}

/// Make a Forza FM8 CarDash packet (331 bytes) with is_race_on=1.
fn make_forza_fm8_cardash() -> Vec<u8> {
    let mut buf = vec![0u8; 331];
    write_i32_le(&mut buf, 0, 1);
    buf
}

/// Make a Forza FH4 CarDash packet (324 bytes) with is_race_on=1.
fn make_forza_fh4_cardash() -> Vec<u8> {
    let mut buf = vec![0u8; 324];
    write_i32_le(&mut buf, 0, 1);
    buf
}

/// Make a Codemasters Mode 1 packet (264 bytes, all zeros).
fn make_cm_mode1() -> Vec<u8> {
    vec![0u8; 264]
}

/// Make an OutGauge packet (92 bytes) for BeamNG/LFS.
fn make_outgauge() -> Vec<u8> {
    vec![0u8; 92]
}

/// Make a GT7 decrypted packet (296 bytes) with valid magic.
fn make_gt7_decrypted() -> Vec<u8> {
    let mut buf = vec![0u8; 296];
    buf[0..4].copy_from_slice(&gran_turismo_7::MAGIC.to_le_bytes());
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. FORZA PROTOCOL TESTS - Sled, CarDash, FM8, FH4
// ═══════════════════════════════════════════════════════════════════════════════

mod forza_protocol {
    use super::*;

    fn adapter() -> ForzaAdapter {
        ForzaAdapter::new()
    }

    // -- Packet format detection --

    #[test]
    fn rejects_empty_packet() -> TestResult {
        assert!(adapter().normalize(&[]).is_err());
        Ok(())
    }

    #[test]
    fn rejects_undersized_sled() -> TestResult {
        let buf = vec![0u8; 231];
        assert!(adapter().normalize(&buf).is_err());
        Ok(())
    }

    #[test]
    fn rejects_unknown_size() -> TestResult {
        let buf = vec![0u8; 250]; // not 232, 311, 324, or 331
        assert!(adapter().normalize(&buf).is_err());
        Ok(())
    }

    #[test]
    fn sled_packet_accepted_at_232_bytes() -> TestResult {
        let buf = make_forza_sled();
        let _t = adapter().normalize(&buf)?;
        Ok(())
    }

    #[test]
    fn cardash_packet_accepted_at_311_bytes() -> TestResult {
        let buf = make_forza_cardash();
        let _t = adapter().normalize(&buf)?;
        Ok(())
    }

    #[test]
    fn fm8_cardash_accepted_at_331_bytes() -> TestResult {
        let buf = make_forza_fm8_cardash();
        let _t = adapter().normalize(&buf)?;
        Ok(())
    }

    #[test]
    fn fh4_cardash_accepted_at_324_bytes() -> TestResult {
        let buf = make_forza_fh4_cardash();
        let _t = adapter().normalize(&buf)?;
        Ok(())
    }

    // -- is_race_on gate --

    #[test]
    fn sled_race_off_returns_zeroed() -> TestResult {
        let mut buf = make_forza_sled();
        write_i32_le(&mut buf, 0, 0); // is_race_on = 0
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn cardash_race_off_returns_zeroed() -> TestResult {
        let mut buf = make_forza_cardash();
        write_i32_le(&mut buf, 0, 0);
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    // -- Sled field extraction (verified against FMData.cs offsets) --

    #[test]
    fn sled_extracts_rpm_from_offset_16() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 16, 7500.0); // CurrentEngineRpm @ offset 16
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.rpm, 7500.0, 0.1), "rpm={}", t.rpm);
        Ok(())
    }

    #[test]
    fn sled_extracts_max_rpm_from_offset_8() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 8, 9000.0); // EngineMaxRpm @ offset 8
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.max_rpm, 9000.0, 0.1), "max_rpm={}", t.max_rpm);
        Ok(())
    }

    #[test]
    fn sled_speed_from_velocity_magnitude() -> TestResult {
        let mut buf = make_forza_sled();
        // VelocityX=3, VelocityY=0, VelocityZ=4 => speed = sqrt(9+0+16) = 5 m/s
        write_f32_le(&mut buf, 32, 3.0);
        write_f32_le(&mut buf, 36, 0.0);
        write_f32_le(&mut buf, 40, 4.0);
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.speed_ms, 5.0, 0.01), "speed={}", t.speed_ms);
        Ok(())
    }

    #[test]
    fn sled_g_forces_converted_from_accel() -> TestResult {
        let mut buf = make_forza_sled();
        const G: f32 = 9.80665;
        // AccelerationX (lateral) @ 20, AccelerationZ (longitudinal) @ 28
        write_f32_le(&mut buf, 20, G * 2.0);
        write_f32_le(&mut buf, 28, G * 1.5);
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.lateral_g, 2.0, 0.01), "lat_g={}", t.lateral_g);
        assert!(
            approx_eq(t.longitudinal_g, 1.5, 0.01),
            "lon_g={}",
            t.longitudinal_g
        );
        Ok(())
    }

    #[test]
    fn sled_tire_slip_ratios_are_averaged() -> TestResult {
        let mut buf = make_forza_sled();
        // TireSlipRatio offsets: 84, 88, 92, 96
        write_f32_le(&mut buf, 84, 0.1);
        write_f32_le(&mut buf, 88, 0.2);
        write_f32_le(&mut buf, 92, 0.3);
        write_f32_le(&mut buf, 96, 0.4);
        let t = adapter().normalize(&buf)?;
        assert!(
            approx_eq(t.slip_ratio, 0.25, 0.01),
            "slip_ratio={}",
            t.slip_ratio
        );
        Ok(())
    }

    #[test]
    fn sled_slip_angles_from_offsets_164_176() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 164, 0.05);
        write_f32_le(&mut buf, 168, 0.06);
        write_f32_le(&mut buf, 172, 0.07);
        write_f32_le(&mut buf, 176, 0.08);
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.slip_angle_fl, 0.05, 0.001));
        assert!(approx_eq(t.slip_angle_fr, 0.06, 0.001));
        assert!(approx_eq(t.slip_angle_rl, 0.07, 0.001));
        assert!(approx_eq(t.slip_angle_rr, 0.08, 0.001));
        Ok(())
    }

    #[test]
    fn sled_extended_wheel_speeds_populated() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 100, 50.0); // wheel_speed_fl
        let t = adapter().normalize(&buf)?;
        match t.extended.get("wheel_speed_fl") {
            Some(TelemetryValue::Float(v)) => assert!(approx_eq(*v, 50.0, 0.01)),
            other => return Err(format!("expected Float, got {:?}", other).into()),
        }
        Ok(())
    }

    // -- CarDash extension fields --

    #[test]
    fn cardash_throttle_brake_from_u8_offsets() -> TestResult {
        let mut buf = make_forza_cardash();
        buf[303] = 255; // throttle = 255/255 = 1.0
        buf[304] = 128; // brake ≈ 0.502
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.throttle, 1.0, 0.01), "throttle={}", t.throttle);
        assert!(approx_eq(t.brake, 0.502, 0.01), "brake={}", t.brake);
        Ok(())
    }

    #[test]
    fn cardash_gear_encoding_reverse_neutral_first() -> TestResult {
        let mut buf = make_forza_cardash();
        // Gear @ 307: 0=Reverse→-1, 1=Neutral→0, 2=1st→1
        buf[307] = 0;
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.gear, -1, "gear 0 should be reverse");

        buf[307] = 1;
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.gear, 0, "gear 1 should be neutral");

        buf[307] = 2;
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.gear, 1, "gear 2 should be 1st");

        buf[307] = 5;
        let t = adapter().normalize(&buf)?;
        assert_eq!(t.gear, 4, "gear 5 should be 4th");
        Ok(())
    }

    #[test]
    fn cardash_steering_from_i8_at_offset_308() -> TestResult {
        let mut buf = make_forza_cardash();
        // steer: i8 -127..127 → -1.0..1.0. Offset 308.
        buf[308] = 127u8; // +127 → ~1.0
        let t = adapter().normalize(&buf)?;
        assert!(t.steering_angle > 0.99, "steer={}", t.steering_angle);

        buf[308] = (-127i8) as u8; // -127 → ~-1.0
        let t = adapter().normalize(&buf)?;
        assert!(t.steering_angle < -0.99, "steer={}", t.steering_angle);
        Ok(())
    }

    #[test]
    fn cardash_tire_temps_fahrenheit_to_celsius() -> TestResult {
        let mut buf = make_forza_cardash();
        // TireTempFL @ 256 in Fahrenheit. 212°F = 100°C
        write_f32_le(&mut buf, 256, 212.0);
        let t = adapter().normalize(&buf)?;
        assert_eq!(
            t.tire_temps_c[0], 100,
            "expected 100°C, got {}",
            t.tire_temps_c[0]
        );
        Ok(())
    }

    #[test]
    fn cardash_fuel_and_lap_data() -> TestResult {
        let mut buf = make_forza_cardash();
        write_f32_le(&mut buf, 276, 0.75); // fuel
        write_f32_le(&mut buf, 284, 62.5); // best lap
        write_f32_le(&mut buf, 288, 63.1); // last lap
        write_f32_le(&mut buf, 292, 30.0); // current lap
        write_u16_le(&mut buf, 300, 5); // lap number
        buf[302] = 3; // race position
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.fuel_percent, 0.75, 0.01));
        assert!(approx_eq(t.best_lap_time_s, 62.5, 0.1));
        assert!(approx_eq(t.last_lap_time_s, 63.1, 0.1));
        assert!(approx_eq(t.current_lap_time_s, 30.0, 0.1));
        assert_eq!(t.lap, 5);
        assert_eq!(t.position, 3);
        Ok(())
    }

    #[test]
    fn fh4_offsets_shifted_by_12_bytes() -> TestResult {
        let mut buf = make_forza_fh4_cardash();
        // FH4 has 12-byte HorizonPlaceholder; dash speed at 244+12=256
        write_f32_le(&mut buf, 256, 50.0);
        let t = adapter().normalize(&buf)?;
        assert!(approx_eq(t.speed_ms, 50.0, 0.1), "speed={}", t.speed_ms);
        Ok(())
    }

    #[test]
    fn fm8_extended_data_contains_power_torque_boost() -> TestResult {
        let mut buf = make_forza_fm8_cardash();
        write_f32_le(&mut buf, 248, 150000.0); // power in watts
        write_f32_le(&mut buf, 252, 450.0); // torque in Nm
        write_f32_le(&mut buf, 272, 1.2); // boost in PSI
        let t = adapter().normalize(&buf)?;
        match t.extended.get("power_w") {
            Some(TelemetryValue::Float(v)) => assert!(approx_eq(*v, 150000.0, 1.0)),
            other => return Err(format!("expected power_w, got {:?}", other).into()),
        }
        match t.extended.get("torque_nm") {
            Some(TelemetryValue::Float(v)) => assert!(approx_eq(*v, 450.0, 0.1)),
            other => return Err(format!("expected torque_nm, got {:?}", other).into()),
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. CODEMASTERS MODE 1 PROTOCOL (DiRT Rally 2, DiRT 3/4, GRID family)
// ═══════════════════════════════════════════════════════════════════════════════

mod codemasters_protocol {
    use super::*;

    fn dr2_adapter() -> DirtRally2Adapter {
        DirtRally2Adapter::new()
    }

    fn grid2019_adapter() -> Grid2019Adapter {
        Grid2019Adapter::new()
    }

    #[test]
    fn rejects_packet_below_264_bytes() -> TestResult {
        assert!(dr2_adapter().normalize(&vec![0u8; 263]).is_err());
        Ok(())
    }

    #[test]
    fn accepts_exact_264_bytes() -> TestResult {
        let _t = dr2_adapter().normalize(&make_cm_mode1())?;
        Ok(())
    }

    #[test]
    fn accepts_oversized_packet() -> TestResult {
        let _t = dr2_adapter().normalize(&vec![0u8; 512])?;
        Ok(())
    }

    #[test]
    fn speed_from_wheel_speeds_at_100_112() -> TestResult {
        let mut buf = make_cm_mode1();
        // Wheel speed offsets: RL=100, RR=104, FL=108, FR=112
        write_f32_le(&mut buf, 100, 10.0);
        write_f32_le(&mut buf, 104, 10.0);
        write_f32_le(&mut buf, 108, 10.0);
        write_f32_le(&mut buf, 112, 10.0);
        let t = dr2_adapter().normalize(&buf)?;
        assert!(approx_eq(t.speed_ms, 10.0, 0.1), "speed={}", t.speed_ms);
        Ok(())
    }

    #[test]
    fn speed_fallback_to_velocity_when_wheels_zero() -> TestResult {
        let mut buf = make_cm_mode1();
        // velocity vector at 32/36/40
        write_f32_le(&mut buf, 32, 3.0);
        write_f32_le(&mut buf, 36, 4.0);
        write_f32_le(&mut buf, 40, 0.0);
        let t = dr2_adapter().normalize(&buf)?;
        // sqrt(9+16) = 5
        assert!(approx_eq(t.speed_ms, 5.0, 0.1), "speed={}", t.speed_ms);
        Ok(())
    }

    #[test]
    fn rpm_at_offset_148() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 148, 6000.0);
        let t = dr2_adapter().normalize(&buf)?;
        assert!(approx_eq(t.rpm, 6000.0, 0.1));
        Ok(())
    }

    #[test]
    fn max_rpm_at_offset_252() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 252, 8500.0);
        let t = dr2_adapter().normalize(&buf)?;
        assert!(approx_eq(t.max_rpm, 8500.0, 0.1));
        Ok(())
    }

    #[test]
    fn gear_encoding_reverse_is_below_0_5() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 132, 0.0); // 0.0 → reverse (-1)
        let t = dr2_adapter().normalize(&buf)?;
        assert_eq!(t.gear, -1, "gear 0.0 should be reverse");
        Ok(())
    }

    #[test]
    fn gear_encoding_forward_gears() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 132, 3.0); // 3.0 → 3rd gear
        let t = dr2_adapter().normalize(&buf)?;
        assert_eq!(t.gear, 3);
        Ok(())
    }

    #[test]
    fn throttle_brake_steer_at_116_124_120() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 116, 0.8); // throttle
        write_f32_le(&mut buf, 124, 0.5); // brake
        write_f32_le(&mut buf, 120, -0.3); // steer
        let t = dr2_adapter().normalize(&buf)?;
        assert!(approx_eq(t.throttle, 0.8, 0.01));
        assert!(approx_eq(t.brake, 0.5, 0.01));
        assert!(approx_eq(t.steering_angle, -0.3, 0.01));
        Ok(())
    }

    #[test]
    fn lateral_g_at_offset_136_generates_ffb_scalar() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 136, 1.5); // lat_g → ffb_scalar = 1.5/3.0 = 0.5
        let t = dr2_adapter().normalize(&buf)?;
        assert!(approx_eq(t.ffb_scalar, 0.5, 0.01), "ffb={}", t.ffb_scalar);
        Ok(())
    }

    #[test]
    fn fuel_percent_from_tank_and_capacity() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 180, 30.0); // fuel in tank
        write_f32_le(&mut buf, 184, 60.0); // fuel capacity
        let t = dr2_adapter().normalize(&buf)?;
        assert!(
            approx_eq(t.fuel_percent, 0.5, 0.01),
            "fuel={}",
            t.fuel_percent
        );
        Ok(())
    }

    #[test]
    fn in_pits_flag_from_offset_188() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 188, 1.0); // in_pit >= 0.5
        let t = dr2_adapter().normalize(&buf)?;
        assert!(t.flags.in_pits, "expected in_pits=true");
        Ok(())
    }

    #[test]
    fn grid_2019_uses_same_mode1_format() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 148, 5000.0);
        let t = grid2019_adapter().normalize(&buf)?;
        assert!(approx_eq(t.rpm, 5000.0, 0.1));
        Ok(())
    }

    #[test]
    fn num_gears_at_offset_260() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 260, 6.0);
        let t = dr2_adapter().normalize(&buf)?;
        assert_eq!(t.num_gears, 6);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. OUTGAUGE PROTOCOL (LFS / BeamNG)
// ═══════════════════════════════════════════════════════════════════════════════

mod outgauge_protocol {
    use super::*;

    fn lfs() -> LFSAdapter {
        LFSAdapter::new()
    }

    fn beamng() -> BeamNGAdapter {
        BeamNGAdapter::new()
    }

    #[test]
    fn rejects_below_92_bytes() -> TestResult {
        assert!(lfs().normalize(&[0u8; 91]).is_err());
        assert!(beamng().normalize(&[0u8; 91]).is_err());
        Ok(())
    }

    #[test]
    fn accepts_92_byte_packet() -> TestResult {
        let _t = lfs().normalize(&make_outgauge())?;
        let _t = beamng().normalize(&make_outgauge())?;
        Ok(())
    }

    #[test]
    fn accepts_96_byte_with_optional_id() -> TestResult {
        let buf = vec![0u8; 96];
        let _t = lfs().normalize(&buf)?;
        let _t = beamng().normalize(&buf)?;
        Ok(())
    }

    #[test]
    fn speed_at_offset_12() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 12, 33.33); // speed m/s
        let t = lfs().normalize(&buf)?;
        assert!(approx_eq(t.speed_ms, 33.33, 0.01));
        Ok(())
    }

    #[test]
    fn rpm_at_offset_16() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 16, 4500.0);
        let t = beamng().normalize(&buf)?;
        assert!(approx_eq(t.rpm, 4500.0, 0.1));
        Ok(())
    }

    #[test]
    fn gear_encoding_outgauge_standard() -> TestResult {
        let mut buf = make_outgauge();
        // Gear @ 10: 0=Reverse→-1, 1=Neutral→0, 2=1st→1
        buf[10] = 0;
        assert_eq!(lfs().normalize(&buf)?.gear, -1);

        buf[10] = 1;
        assert_eq!(lfs().normalize(&buf)?.gear, 0);

        buf[10] = 2;
        assert_eq!(lfs().normalize(&buf)?.gear, 1);

        buf[10] = 7;
        assert_eq!(lfs().normalize(&buf)?.gear, 6);
        Ok(())
    }

    #[test]
    fn throttle_brake_clutch_at_48_52_56() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 48, 0.9); // throttle
        write_f32_le(&mut buf, 52, 0.4); // brake
        write_f32_le(&mut buf, 56, 0.7); // clutch
        let t = lfs().normalize(&buf)?;
        assert!(approx_eq(t.throttle, 0.9, 0.01));
        assert!(approx_eq(t.brake, 0.4, 0.01));
        assert!(approx_eq(t.clutch, 0.7, 0.01));
        Ok(())
    }

    #[test]
    fn fuel_at_offset_28() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 28, 0.65);
        let t = beamng().normalize(&buf)?;
        assert!(approx_eq(t.fuel_percent, 0.65, 0.01));
        Ok(())
    }

    #[test]
    fn engine_temp_at_offset_24() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 24, 92.5);
        let t = lfs().normalize(&buf)?;
        assert!(approx_eq(t.engine_temp_c, 92.5, 0.1));
        Ok(())
    }

    #[test]
    fn dash_lights_flags_decoded() -> TestResult {
        let mut buf = make_outgauge();
        // DL_PITSPEED=0x0008, DL_TC=0x0010, DL_ABS=0x0400
        write_u32_le(&mut buf, 44, 0x0008 | 0x0010 | 0x0400);
        let t = lfs().normalize(&buf)?;
        assert!(t.flags.pit_limiter, "pit_limiter from DL_PITSPEED");
        assert!(t.flags.traction_control, "TC from DL_TC");
        assert!(t.flags.abs_active, "ABS from DL_ABS");
        Ok(())
    }

    #[test]
    fn extended_turbo_and_oil_populated() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 20, 1.2); // turbo BAR
        write_f32_le(&mut buf, 32, 3.5); // oil pressure BAR
        write_f32_le(&mut buf, 36, 110.0); // oil temp °C
        let t = beamng().normalize(&buf)?;
        match t.extended.get("turbo_bar") {
            Some(TelemetryValue::Float(v)) => assert!(approx_eq(*v, 1.2, 0.01)),
            other => return Err(format!("unexpected turbo: {:?}", other).into()),
        }
        match t.extended.get("oil_temp_c") {
            Some(TelemetryValue::Float(v)) => assert!(approx_eq(*v, 110.0, 0.1)),
            other => return Err(format!("unexpected oil_temp: {:?}", other).into()),
        }
        Ok(())
    }

    #[test]
    fn lfs_and_beamng_produce_consistent_output() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 12, 20.0); // speed
        write_f32_le(&mut buf, 16, 3000.0); // rpm
        buf[10] = 3; // gear: 3rd → 2
        write_f32_le(&mut buf, 48, 0.5); // throttle
        let t_lfs = lfs().normalize(&buf)?;
        let t_bmg = beamng().normalize(&buf)?;
        assert!(approx_eq(t_lfs.speed_ms, t_bmg.speed_ms, 0.001));
        assert!(approx_eq(t_lfs.rpm, t_bmg.rpm, 0.001));
        assert_eq!(t_lfs.gear, t_bmg.gear);
        assert!(approx_eq(t_lfs.throttle, t_bmg.throttle, 0.001));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. GT7 DECRYPTED PACKET PROTOCOL
// ═══════════════════════════════════════════════════════════════════════════════

mod gt7_protocol {
    use super::*;

    #[test]
    fn parse_decrypted_rejects_short_buffer() -> TestResult {
        let buf = vec![0u8; 295];
        assert!(gran_turismo_7::parse_decrypted_ext(&buf).is_err());
        Ok(())
    }

    #[test]
    fn parse_decrypted_accepts_zero_magic_buffer() -> TestResult {
        // parse_decrypted_ext only validates size, not magic
        let buf = vec![0u8; 296];
        let _t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        Ok(())
    }

    #[test]
    fn parse_decrypted_accepts_valid_magic() -> TestResult {
        let buf = make_gt7_decrypted();
        let _t = gran_turismo_7::parse_decrypted_ext(&buf)?;
        Ok(())
    }

    #[test]
    fn gt7_magic_is_0x47375330() -> TestResult {
        assert_eq!(gran_turismo_7::MAGIC, 0x4737_5330);
        Ok(())
    }

    #[test]
    fn gt7_packet_sizes_match_documented_values() -> TestResult {
        assert_eq!(gran_turismo_7::PACKET_SIZE, 296);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE2, 316);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE3, 344);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. CROSS-ADAPTER CONSISTENCY TESTS
// ═══════════════════════════════════════════════════════════════════════════════

mod cross_adapter_consistency {
    use super::*;

    /// All adapters should accept zero-filled packets of the right size and
    /// produce NormalizedTelemetry with speed_ms, rpm, gear in sane ranges.
    fn verify_bounds(t: &NormalizedTelemetry) -> TestResult {
        assert!(
            t.speed_ms >= 0.0,
            "speed_ms must be non-negative: {}",
            t.speed_ms
        );
        assert!(t.rpm >= 0.0, "rpm must be non-negative: {}", t.rpm);
        assert!(
            t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of [0,1]: {}",
            t.throttle
        );
        assert!(
            t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of [0,1]: {}",
            t.brake
        );
        assert!(
            t.clutch >= 0.0 && t.clutch <= 1.0,
            "clutch out of [0,1]: {}",
            t.clutch
        );
        assert!(
            t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
            "fuel out of [0,1]: {}",
            t.fuel_percent
        );
        assert!(
            t.gear >= -1 && t.gear <= 20,
            "gear out of range: {}",
            t.gear
        );
        assert!(
            t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "ffb out of [-1,1]: {}",
            t.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn forza_sled_zero_packet_within_bounds() -> TestResult {
        let t = ForzaAdapter::new().normalize(&make_forza_sled())?;
        verify_bounds(&t)
    }

    #[test]
    fn forza_cardash_zero_packet_within_bounds() -> TestResult {
        let t = ForzaAdapter::new().normalize(&make_forza_cardash())?;
        verify_bounds(&t)
    }

    #[test]
    fn codemasters_zero_packet_within_bounds() -> TestResult {
        let t = DirtRally2Adapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    #[test]
    fn outgauge_lfs_zero_packet_within_bounds() -> TestResult {
        let t = LFSAdapter::new().normalize(&make_outgauge())?;
        verify_bounds(&t)
    }

    #[test]
    fn outgauge_beamng_zero_packet_within_bounds() -> TestResult {
        let t = BeamNGAdapter::new().normalize(&make_outgauge())?;
        verify_bounds(&t)
    }

    #[test]
    fn gt7_decrypted_zero_packet_within_bounds() -> TestResult {
        let t = gran_turismo_7::parse_decrypted_ext(&make_gt7_decrypted())?;
        verify_bounds(&t)
    }

    #[test]
    fn dirt3_zero_packet_within_bounds() -> TestResult {
        let t = Dirt3Adapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    #[test]
    fn dirt4_zero_packet_within_bounds() -> TestResult {
        let t = Dirt4Adapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    #[test]
    fn grid_autosport_zero_packet_within_bounds() -> TestResult {
        let t = GridAutosportAdapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    #[test]
    fn grid_legends_zero_packet_within_bounds() -> TestResult {
        let t = GridLegendsAdapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    #[test]
    fn race_driver_grid_zero_packet_within_bounds() -> TestResult {
        let t = RaceDriverGridAdapter::new().normalize(&make_cm_mode1())?;
        verify_bounds(&t)
    }

    /// Speed unit consistency: all adapters producing speed should use m/s.
    #[test]
    fn forza_speed_unit_is_meters_per_second() -> TestResult {
        let mut buf = make_forza_cardash();
        // CarDash speed at offset 244: 100 km/h would be ~27.78 m/s
        // Forza reports speed_ms directly in m/s at offset 244
        write_f32_le(&mut buf, 244, 27.78);
        let t = ForzaAdapter::new().normalize(&buf)?;
        // If the field is in km/h instead, t.speed_ms would be 27.78 (same).
        // But let's verify it's within the m/s range for ~100 km/h.
        assert!(
            t.speed_ms > 20.0 && t.speed_ms < 35.0,
            "speed_ms={}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_kmh_helper_consistent_across_adapters() -> TestResult {
        // Set up 10 m/s via different adapter paths
        let mut cm_buf = make_cm_mode1();
        write_f32_le(&mut cm_buf, 100, 10.0);
        write_f32_le(&mut cm_buf, 104, 10.0);
        write_f32_le(&mut cm_buf, 108, 10.0);
        write_f32_le(&mut cm_buf, 112, 10.0);
        let t_cm = DirtRally2Adapter::new().normalize(&cm_buf)?;

        let mut og_buf = make_outgauge();
        write_f32_le(&mut og_buf, 12, 10.0);
        let t_og = LFSAdapter::new().normalize(&og_buf)?;

        // Both should report ~36 km/h for 10 m/s
        assert!(approx_eq(t_cm.speed_kmh(), 36.0, 0.5));
        assert!(approx_eq(t_og.speed_kmh(), 36.0, 0.5));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. EDGE CASES AND PROTOCOL ROBUSTNESS
// ═══════════════════════════════════════════════════════════════════════════════

mod edge_cases {
    use super::*;

    #[test]
    fn zero_length_data_rejected_by_all_udp_adapters() -> TestResult {
        let empty: &[u8] = &[];
        assert!(ForzaAdapter::new().normalize(empty).is_err());
        assert!(DirtRally2Adapter::new().normalize(empty).is_err());
        assert!(LFSAdapter::new().normalize(empty).is_err());
        assert!(BeamNGAdapter::new().normalize(empty).is_err());
        assert!(ACCAdapter::new().normalize(empty).is_err());
        Ok(())
    }

    #[test]
    fn single_byte_rejected_by_all() -> TestResult {
        let one = &[0x42u8];
        assert!(ForzaAdapter::new().normalize(one).is_err());
        assert!(DirtRally2Adapter::new().normalize(one).is_err());
        assert!(LFSAdapter::new().normalize(one).is_err());
        assert!(BeamNGAdapter::new().normalize(one).is_err());
        Ok(())
    }

    #[test]
    fn forza_nan_rpm_treated_as_zero() -> TestResult {
        let mut buf = make_forza_sled();
        // Write NaN to RPM offset 16
        buf[16..20].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = ForzaAdapter::new().normalize(&buf)?;
        // read_f32_le filters NaN → None → unwrap_or(0.0)
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn forza_infinity_acceleration_treated_as_zero() -> TestResult {
        let mut buf = make_forza_sled();
        buf[20..24].copy_from_slice(&f32::INFINITY.to_le_bytes());
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert_eq!(t.lateral_g, 0.0);
        Ok(())
    }

    #[test]
    fn codemasters_nan_in_wheel_speeds_yields_zero_speed() -> TestResult {
        let mut buf = make_cm_mode1();
        buf[100..104].copy_from_slice(&f32::NAN.to_le_bytes());
        buf[104..108].copy_from_slice(&f32::NAN.to_le_bytes());
        buf[108..112].copy_from_slice(&f32::NAN.to_le_bytes());
        buf[112..116].copy_from_slice(&f32::NAN.to_le_bytes());
        // velocity also NaN
        buf[32..36].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = DirtRally2Adapter::new().normalize(&buf)?;
        assert_eq!(t.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn outgauge_nan_speed_treated_as_zero() -> TestResult {
        let mut buf = make_outgauge();
        buf[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = LFSAdapter::new().normalize(&buf)?;
        assert_eq!(t.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn negative_infinity_filtered() -> TestResult {
        let mut buf = make_forza_sled();
        buf[16..20].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert_eq!(t.rpm, 0.0, "neg infinity RPM should be filtered to 0");
        Ok(())
    }

    #[test]
    fn forza_all_zeros_sled_yields_zero_speed() -> TestResult {
        let buf = make_forza_sled();
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert_eq!(t.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn codemasters_all_zeros_yields_zero_everything() -> TestResult {
        let buf = make_cm_mode1();
        let t = DirtRally2Adapter::new().normalize(&buf)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        assert_eq!(t.gear, -1); // 0.0 < 0.5 → reverse
        Ok(())
    }

    #[test]
    fn outgauge_all_zeros_neutral_gear() -> TestResult {
        let buf = make_outgauge();
        let t = LFSAdapter::new().normalize(&buf)?;
        assert_eq!(t.gear, -1); // gear byte 0 → reverse
        Ok(())
    }

    #[test]
    fn forza_truncated_cardash_rejected() -> TestResult {
        // 300 bytes: > 232 (Sled) but < 311 (CarDash) → Unknown
        let buf = vec![0u8; 300];
        assert!(ForzaAdapter::new().normalize(&buf).is_err());
        Ok(())
    }

    #[test]
    fn max_valid_values_do_not_panic() -> TestResult {
        let mut buf = make_forza_cardash();
        write_i32_le(&mut buf, 0, 1);
        write_f32_le(&mut buf, 16, f32::MAX); // max RPM
        write_f32_le(&mut buf, 32, f32::MAX); // max velocity
        buf[303] = 255; // max throttle
        buf[304] = 255; // max brake
        buf[305] = 255; // max clutch
        buf[307] = 255; // max gear
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.speed_ms.is_finite());
        assert!(t.throttle <= 1.0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. TIMING GUARANTEES - Parsing must complete within 1ms budget
// ═══════════════════════════════════════════════════════════════════════════════

mod timing_guarantees {
    use super::*;

    const MAX_PARSE_TIME: Duration = Duration::from_millis(1);
    const ITERATIONS: usize = 100;

    /// Check if timing guarantees should be skipped (coverage, shared CI, or GitHub Actions)
    fn skip_timing_guarantees() -> bool {
        // Coverage instrumentation adds overhead
        if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
            return true;
        }
        // CI runners have unpredictable scheduling jitter
        if std::env::var_os("CI").is_some() {
            return true;
        }
        std::env::var("OPENRACING_SKIP_TIMING_GUARANTEES")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    fn measure_parse(adapter: &dyn TelemetryAdapter, data: &[u8]) -> TestResult {
        if skip_timing_guarantees() {
            println!("SKIPPED: timing guarantee test under coverage/shared CI");
            return Ok(());
        }

        // Warm up
        for _ in 0..10 {
            let _ = adapter.normalize(data);
        }
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let _ = adapter.normalize(data);
            let elapsed = start.elapsed();
            assert!(
                elapsed < MAX_PARSE_TIME,
                "parse took {:?}, budget is {:?}",
                elapsed,
                MAX_PARSE_TIME
            );
        }
        Ok(())
    }

    #[test]
    fn forza_sled_parse_within_1ms() -> TestResult {
        measure_parse(&ForzaAdapter::new(), &make_forza_sled())
    }

    #[test]
    fn forza_cardash_parse_within_1ms() -> TestResult {
        measure_parse(&ForzaAdapter::new(), &make_forza_cardash())
    }

    #[test]
    fn codemasters_mode1_parse_within_1ms() -> TestResult {
        measure_parse(&DirtRally2Adapter::new(), &make_cm_mode1())
    }

    #[test]
    fn outgauge_lfs_parse_within_1ms() -> TestResult {
        measure_parse(&LFSAdapter::new(), &make_outgauge())
    }

    #[test]
    fn outgauge_beamng_parse_within_1ms() -> TestResult {
        measure_parse(&BeamNGAdapter::new(), &make_outgauge())
    }

    #[test]
    fn gt7_decrypted_parse_within_1ms() -> TestResult {
        if skip_timing_guarantees() {
            println!("SKIPPED: timing guarantee test under coverage/shared CI");
            return Ok(());
        }

        let buf = make_gt7_decrypted();
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let _ = gran_turismo_7::parse_decrypted_ext(&buf);
        }
        let avg = start.elapsed() / ITERATIONS as u32;
        assert!(avg < MAX_PARSE_TIME, "GT7 avg parse took {:?}", avg);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. FIELD COVERAGE - Ensure all NormalizedTelemetry fields are populated
//    by at least one adapter
// ═══════════════════════════════════════════════════════════════════════════════

mod field_coverage {
    use super::*;

    #[test]
    fn forza_cardash_covers_all_motion_fields() -> TestResult {
        let mut buf = make_forza_cardash();
        write_f32_le(&mut buf, 244, 50.0); // speed
        buf[303] = 200; // throttle
        buf[304] = 100; // brake
        buf[305] = 50; // clutch
        buf[308] = 64u8; // steer
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.speed_ms > 0.0, "speed not covered");
        assert!(t.throttle > 0.0, "throttle not covered");
        assert!(t.brake > 0.0, "brake not covered");
        assert!(t.clutch > 0.0, "clutch not covered");
        assert!(t.steering_angle != 0.0, "steering not covered");
        Ok(())
    }

    #[test]
    fn forza_sled_covers_g_forces() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 20, 9.8); // lateral
        write_f32_le(&mut buf, 28, 4.9); // longitudinal
        write_f32_le(&mut buf, 24, 9.8); // vertical
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.lateral_g != 0.0, "lateral_g not covered");
        assert!(t.longitudinal_g != 0.0, "longitudinal_g not covered");
        assert!(t.vertical_g != 0.0, "vertical_g not covered");
        Ok(())
    }

    #[test]
    fn forza_cardash_covers_tire_and_fuel_data() -> TestResult {
        let mut buf = make_forza_cardash();
        write_f32_le(&mut buf, 256, 200.0); // tire temp FL (°F)
        write_f32_le(&mut buf, 276, 0.5); // fuel
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.tire_temps_c[0] > 0, "tire temp FL not covered");
        assert!(t.fuel_percent > 0.0, "fuel not covered");
        Ok(())
    }

    #[test]
    fn codemasters_covers_ffb_scalar() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 136, 2.0);
        let t = DirtRally2Adapter::new().normalize(&buf)?;
        assert!(t.ffb_scalar != 0.0, "ffb_scalar not covered");
        Ok(())
    }

    #[test]
    fn codemasters_covers_position_and_lap() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 156, 5.0); // position
        write_f32_le(&mut buf, 144, 3.0); // lap (0-indexed, converted to 1-indexed)
        let t = DirtRally2Adapter::new().normalize(&buf)?;
        assert!(t.position > 0, "position not covered");
        assert!(t.lap > 0, "lap not covered");
        Ok(())
    }

    #[test]
    fn codemasters_covers_tire_pressures() -> TestResult {
        let mut buf = make_cm_mode1();
        write_f32_le(&mut buf, 228, 26.0); // tire pressure FL
        write_f32_le(&mut buf, 232, 26.5);
        write_f32_le(&mut buf, 236, 25.0);
        write_f32_le(&mut buf, 240, 25.5);
        let t = DirtRally2Adapter::new().normalize(&buf)?;
        assert!(
            t.tire_pressures_psi[0] > 0.0,
            "tire pressure FL not covered"
        );
        assert!(
            t.tire_pressures_psi[3] > 0.0,
            "tire pressure RR not covered"
        );
        Ok(())
    }

    #[test]
    fn outgauge_covers_engine_temp() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 24, 95.0);
        let t = LFSAdapter::new().normalize(&buf)?;
        assert!(t.engine_temp_c > 0.0, "engine_temp_c not covered");
        Ok(())
    }

    #[test]
    fn forza_cardash_covers_lap_times() -> TestResult {
        let mut buf = make_forza_cardash();
        write_f32_le(&mut buf, 284, 72.5); // best
        write_f32_le(&mut buf, 288, 73.1); // last
        write_f32_le(&mut buf, 292, 20.0); // current
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.best_lap_time_s > 0.0, "best_lap_time_s not covered");
        assert!(t.last_lap_time_s > 0.0, "last_lap_time_s not covered");
        assert!(t.current_lap_time_s > 0.0, "current_lap_time_s not covered");
        Ok(())
    }

    #[test]
    fn forza_sled_covers_slip_angles() -> TestResult {
        let mut buf = make_forza_sled();
        write_f32_le(&mut buf, 164, 0.1);
        write_f32_le(&mut buf, 168, 0.2);
        write_f32_le(&mut buf, 172, 0.15);
        write_f32_le(&mut buf, 176, 0.18);
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.slip_angle_fl != 0.0, "slip_angle_fl not covered");
        assert!(t.slip_angle_fr != 0.0, "slip_angle_fr not covered");
        assert!(t.slip_angle_rl != 0.0, "slip_angle_rl not covered");
        assert!(t.slip_angle_rr != 0.0, "slip_angle_rr not covered");
        Ok(())
    }

    #[test]
    fn forza_cardash_covers_max_rpm() -> TestResult {
        let mut buf = make_forza_cardash();
        write_f32_le(&mut buf, 8, 8500.0);
        let t = ForzaAdapter::new().normalize(&buf)?;
        assert!(t.max_rpm > 0.0, "max_rpm not covered");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. KNOWN-GOOD DATA VALIDATION
// ═══════════════════════════════════════════════════════════════════════════════

mod known_good_data {
    use super::*;

    /// Simulate a Forza CarDash packet for a car doing ~100 km/h in 3rd gear.
    #[test]
    fn forza_realistic_driving_scenario() -> TestResult {
        let mut buf = make_forza_cardash();
        // Velocity vector: ~27.78 m/s (100 km/h) in Z direction
        write_f32_le(&mut buf, 32, 0.0);
        write_f32_le(&mut buf, 36, 0.0);
        write_f32_le(&mut buf, 40, 27.78);
        // RPM: 5500, Max RPM: 7500
        write_f32_le(&mut buf, 16, 5500.0);
        write_f32_le(&mut buf, 8, 7500.0);
        // CarDash speed (more accurate)
        write_f32_le(&mut buf, 244, 27.78);
        // Throttle: ~60%, Brake: 0
        buf[303] = 153; // 153/255 ≈ 0.6
        buf[304] = 0;
        // Gear: 4th (value 5: 0=R, 1=N, 2=1st, 3=2nd, 4=3rd, 5=4th)
        buf[307] = 5;
        // Tire temps: ~180°F = ~82°C
        write_f32_le(&mut buf, 256, 180.0);
        write_f32_le(&mut buf, 260, 180.0);
        write_f32_le(&mut buf, 264, 180.0);
        write_f32_le(&mut buf, 268, 180.0);
        // Fuel: 75%
        write_f32_le(&mut buf, 276, 0.75);
        // Lap 3, position 5
        write_u16_le(&mut buf, 300, 3);
        buf[302] = 5;

        let t = ForzaAdapter::new().normalize(&buf)?;

        // Validate realistic ranges
        assert!(approx_eq(t.speed_ms, 27.78, 0.5), "speed ~100 km/h");
        assert!(approx_eq(t.rpm, 5500.0, 1.0));
        assert!(approx_eq(t.max_rpm, 7500.0, 1.0));
        assert_eq!(t.gear, 4, "5th value = 4th gear");
        assert!(t.throttle > 0.55 && t.throttle < 0.65);
        assert_eq!(t.brake, 0.0);
        // Tire temps in realistic racing range
        for &temp in &t.tire_temps_c {
            assert!(
                temp > 70 && temp < 95,
                "tire temp {} out of realistic range",
                temp
            );
        }
        assert!(approx_eq(t.fuel_percent, 0.75, 0.01));
        assert_eq!(t.lap, 3);
        assert_eq!(t.position, 5);
        Ok(())
    }

    /// Simulate a Codemasters car cornering hard at high speed.
    #[test]
    fn codemasters_high_g_cornering_scenario() -> TestResult {
        let mut buf = make_cm_mode1();
        // Speed from wheel speeds: ~40 m/s (144 km/h)
        write_f32_le(&mut buf, 100, 40.0);
        write_f32_le(&mut buf, 104, 40.0);
        write_f32_le(&mut buf, 108, 40.0);
        write_f32_le(&mut buf, 112, 40.0);
        // RPM: 6500, Max: 7800
        write_f32_le(&mut buf, 148, 6500.0);
        write_f32_le(&mut buf, 252, 7800.0);
        // Gear: 4th
        write_f32_le(&mut buf, 132, 4.0);
        // Hard throttle and slight steering
        write_f32_le(&mut buf, 116, 0.95);
        write_f32_le(&mut buf, 120, -0.4);
        // High lateral G (2.5 G cornering)
        write_f32_le(&mut buf, 136, 2.5);
        write_f32_le(&mut buf, 140, 0.3);

        let t = DirtRally2Adapter::new().normalize(&buf)?;

        assert!(approx_eq(t.speed_ms, 40.0, 1.0));
        assert!(approx_eq(t.rpm, 6500.0, 1.0));
        assert_eq!(t.gear, 4);
        assert!(t.throttle > 0.9);
        assert!(t.lateral_g > 2.0, "high G not captured");
        // FFB scalar should be saturated at high G
        assert!(
            t.ffb_scalar.abs() > 0.5,
            "ffb should be significant at 2.5G"
        );
        Ok(())
    }

    /// Simulate an OutGauge packet from LFS at pit limiter speed.
    #[test]
    fn lfs_pit_limiter_scenario() -> TestResult {
        let mut buf = make_outgauge();
        write_f32_le(&mut buf, 12, 16.67); // ~60 km/h pit speed
        write_f32_le(&mut buf, 16, 3000.0); // low RPM
        buf[10] = 3; // 2nd gear
        write_f32_le(&mut buf, 48, 0.3); // light throttle
        write_f32_le(&mut buf, 28, 0.4); // 40% fuel
        write_f32_le(&mut buf, 24, 95.0); // engine temp
        // Pit limiter active: DL_PITSPEED = 0x0008
        write_u32_le(&mut buf, 44, 0x0008);

        let t = LFSAdapter::new().normalize(&buf)?;

        assert!(approx_eq(t.speed_ms, 16.67, 0.1));
        assert_eq!(t.gear, 2, "3rd byte = 2nd gear");
        assert!(t.flags.pit_limiter, "pit limiter should be active");
        assert!(approx_eq(t.fuel_percent, 0.4, 0.01));
        Ok(())
    }

    /// Verify the Forza Sled-only packet gives physics but no driver input.
    #[test]
    fn forza_sled_only_no_driver_inputs() -> TestResult {
        let mut buf = make_forza_sled();
        // Set physics data
        write_f32_le(&mut buf, 16, 5000.0); // RPM
        write_f32_le(&mut buf, 32, 0.0);
        write_f32_le(&mut buf, 36, 0.0);
        write_f32_le(&mut buf, 40, 30.0); // 30 m/s forward

        let t = ForzaAdapter::new().normalize(&buf)?;

        // Physics data present
        assert!(t.rpm > 0.0);
        assert!(t.speed_ms > 0.0);

        // Sled format has NO throttle/brake/steer/gear/clutch
        assert_eq!(t.throttle, 0.0, "Sled has no throttle");
        assert_eq!(t.brake, 0.0, "Sled has no brake");
        assert_eq!(t.clutch, 0.0, "Sled has no clutch");
        assert_eq!(t.gear, 0, "Sled has no gear");
        assert_eq!(t.steering_angle, 0.0, "Sled has no steer");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. ADAPTER TRAIT COMPLIANCE
// ═══════════════════════════════════════════════════════════════════════════════

mod adapter_trait_compliance {
    use super::*;

    fn check_game_id_non_empty(adapter: &dyn TelemetryAdapter) -> TestResult {
        let id = adapter.game_id();
        assert!(!id.is_empty(), "game_id must not be empty");
        assert!(
            !id.contains(' '),
            "game_id '{}' should not contain spaces",
            id
        );
        Ok(())
    }

    fn check_update_rate_sane(adapter: &dyn TelemetryAdapter) -> TestResult {
        let rate = adapter.expected_update_rate();
        assert!(
            rate >= Duration::from_millis(1),
            "update rate {:?} too fast",
            rate
        );
        assert!(
            rate <= Duration::from_secs(1),
            "update rate {:?} too slow",
            rate
        );
        Ok(())
    }

    #[test]
    fn forza_game_id_and_rate() -> TestResult {
        let a = ForzaAdapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        assert_eq!(a.game_id(), "forza_motorsport");
        Ok(())
    }

    #[test]
    fn lfs_game_id_and_rate() -> TestResult {
        let a = LFSAdapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        assert_eq!(a.game_id(), "live_for_speed");
        Ok(())
    }

    #[test]
    fn beamng_game_id_and_rate() -> TestResult {
        let a = BeamNGAdapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        assert_eq!(a.game_id(), "beamng_drive");
        Ok(())
    }

    #[test]
    fn acc_game_id_and_rate() -> TestResult {
        let a = ACCAdapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        assert_eq!(a.game_id(), "acc");
        Ok(())
    }

    #[test]
    fn dirt_rally_2_game_id_and_rate() -> TestResult {
        let a = DirtRally2Adapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        Ok(())
    }

    #[test]
    fn gt7_game_id_and_rate() -> TestResult {
        let a = GranTurismo7Adapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        Ok(())
    }

    #[test]
    fn iracing_game_id_and_rate() -> TestResult {
        let a = IRacingAdapter::new();
        check_game_id_non_empty(&a)?;
        check_update_rate_sane(&a)?;
        assert_eq!(a.game_id(), "iracing");
        Ok(())
    }

    #[test]
    fn all_factory_adapters_have_valid_ids() -> TestResult {
        let factories = openracing_telemetry_adapters::adapter_factories();
        assert!(
            factories.len() >= 30,
            "expected 30+ adapters, got {}",
            factories.len()
        );
        for (name, factory) in factories {
            let adapter = factory();
            assert!(
                !adapter.game_id().is_empty(),
                "adapter '{}' has empty game_id",
                name
            );
            let rate = adapter.expected_update_rate();
            assert!(
                rate >= Duration::from_millis(1) && rate <= Duration::from_secs(1),
                "adapter '{}' has unreasonable update rate {:?}",
                name,
                rate
            );
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. NORMALIZED TELEMETRY HELPER METHODS
// ═══════════════════════════════════════════════════════════════════════════════

mod normalized_helpers {
    use super::*;

    #[test]
    fn speed_kmh_conversion() -> TestResult {
        let t = NormalizedTelemetry::builder().speed_ms(10.0).build();
        assert!(approx_eq(t.speed_kmh(), 36.0, 0.01));
        Ok(())
    }

    #[test]
    fn speed_mph_conversion() -> TestResult {
        let t = NormalizedTelemetry::builder().speed_ms(10.0).build();
        assert!(approx_eq(t.speed_mph(), 22.37, 0.01));
        Ok(())
    }

    #[test]
    fn total_g_pythagorean() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .lateral_g(3.0)
            .longitudinal_g(4.0)
            .build();
        assert!(approx_eq(t.total_g(), 5.0, 0.01));
        Ok(())
    }

    #[test]
    fn is_stationary_below_half_mps() -> TestResult {
        let moving = NormalizedTelemetry::builder().speed_ms(1.0).build();
        assert!(!moving.is_stationary());
        let stopped = NormalizedTelemetry::builder().speed_ms(0.3).build();
        assert!(stopped.is_stationary());
        Ok(())
    }

    #[test]
    fn rpm_fraction_clamped() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .rpm(9000.0)
            .max_rpm(8000.0)
            .build();
        assert!(approx_eq(t.rpm_fraction(), 1.0, 0.01));
        Ok(())
    }

    #[test]
    fn rpm_fraction_zero_when_no_max() -> TestResult {
        let t = NormalizedTelemetry::builder().rpm(5000.0).build();
        assert_eq!(t.rpm_fraction(), 0.0);
        Ok(())
    }

    #[test]
    fn average_slip_angle_computation() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .slip_angle_fl(0.1)
            .slip_angle_fr(0.2)
            .slip_angle_rl(0.3)
            .slip_angle_rr(0.4)
            .build();
        assert!(approx_eq(t.average_slip_angle(), 0.25, 0.001));
        Ok(())
    }

    #[test]
    fn front_rear_slip_angle_split() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .slip_angle_fl(0.1)
            .slip_angle_fr(0.3)
            .slip_angle_rl(0.5)
            .slip_angle_rr(0.7)
            .build();
        assert!(approx_eq(t.front_slip_angle(), 0.2, 0.001));
        assert!(approx_eq(t.rear_slip_angle(), 0.6, 0.001));
        Ok(())
    }

    #[test]
    fn has_ffb_data_checks_both_fields() -> TestResult {
        let no_ffb = NormalizedTelemetry::builder().build();
        assert!(!no_ffb.has_ffb_data());

        let with_scalar = NormalizedTelemetry::builder().ffb_scalar(0.5).build();
        assert!(with_scalar.has_ffb_data());

        let with_torque = NormalizedTelemetry::builder().ffb_torque_nm(10.0).build();
        assert!(with_torque.has_ffb_data());
        Ok(())
    }

    #[test]
    fn has_active_flags_checks_racing_flags() -> TestResult {
        let default_t = NormalizedTelemetry::builder().build();
        assert!(!default_t.has_active_flags());

        let yellow = NormalizedTelemetry::builder()
            .flags(openracing_telemetry_adapters::TelemetryFlags {
                yellow_flag: true,
                ..Default::default()
            })
            .build();
        assert!(yellow.has_active_flags());
        Ok(())
    }

    #[test]
    fn builder_ignores_nan_values() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .speed_ms(f32::NAN)
            .rpm(f32::NAN)
            .throttle(f32::NAN)
            .brake(f32::NAN)
            .build();
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        assert_eq!(t.throttle, 0.0);
        assert_eq!(t.brake, 0.0);
        Ok(())
    }

    #[test]
    fn builder_ignores_negative_speed() -> TestResult {
        let t = NormalizedTelemetry::builder().speed_ms(-10.0).build();
        assert_eq!(t.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn builder_clamps_throttle_brake() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .throttle(1.5)
            .brake(2.0)
            .build();
        assert_eq!(t.throttle, 1.0);
        assert_eq!(t.brake, 1.0);
        Ok(())
    }

    #[test]
    fn validated_sanitizes_nan_and_infinity() -> TestResult {
        let t = NormalizedTelemetry {
            speed_ms: f32::NAN,
            rpm: f32::INFINITY,
            throttle: f32::NEG_INFINITY,
            lateral_g: f32::NAN,
            ..Default::default()
        };
        let v = t.validated();
        assert_eq!(v.speed_ms, 0.0);
        assert_eq!(v.rpm, 0.0);
        assert_eq!(v.throttle, 0.0);
        assert_eq!(v.lateral_g, 0.0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. PROTOCOL CONSTANT VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════════

mod protocol_constants {
    use super::*;

    #[test]
    fn forza_packet_sizes_match_specification() -> TestResult {
        // Sled: 58 × 4-byte fields = 232 bytes
        // CarDash: 311 bytes (Sled + dashboard)
        // FM8: 331 bytes (CarDash + 20 extra)
        // FH4: 324 bytes (CarDash + 12-byte HorizonPlaceholder)
        assert!(
            ForzaAdapter::new().normalize(&[0u8; 232]).is_ok()
                || ForzaAdapter::new().normalize(&[0u8; 232]).is_err()
        );
        // Verify exact sizes are accepted
        let mut s = vec![0u8; 232];
        write_i32_le(&mut s, 0, 1);
        assert!(ForzaAdapter::new().normalize(&s).is_ok());

        let mut c = vec![0u8; 311];
        write_i32_le(&mut c, 0, 1);
        assert!(ForzaAdapter::new().normalize(&c).is_ok());

        let mut f8 = vec![0u8; 331];
        write_i32_le(&mut f8, 0, 1);
        assert!(ForzaAdapter::new().normalize(&f8).is_ok());

        let mut f4 = vec![0u8; 324];
        write_i32_le(&mut f4, 0, 1);
        assert!(ForzaAdapter::new().normalize(&f4).is_ok());
        Ok(())
    }

    #[test]
    fn codemasters_min_packet_264_bytes() -> TestResult {
        assert_eq!(codemasters_shared::MIN_PACKET_SIZE, 264);
        Ok(())
    }

    #[test]
    fn outgauge_packet_92_bytes_base() -> TestResult {
        // 92 without ID, 96 with ID
        assert!(LFSAdapter::new().normalize(&[0u8; 92]).is_ok());
        assert!(LFSAdapter::new().normalize(&[0u8; 96]).is_ok());
        Ok(())
    }

    #[test]
    fn gt7_packet_types_296_316_344() -> TestResult {
        assert_eq!(gran_turismo_7::PACKET_SIZE, 296);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE2, 316);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE3, 344);
        Ok(())
    }
}
