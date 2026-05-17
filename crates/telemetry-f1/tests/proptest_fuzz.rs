//! Property-based fuzz tests for F1 telemetry packet parsing.
//!
//! Ensures the parser never panics on arbitrary or random input,
//! and that encode→decode round-trips preserve data.

use openracing_telemetry_adapters::f1_25::{
    CarTelemetryData, SessionData, parse_car_telemetry, parse_header,
};
use openracing_telemetry_adapters::f1_native::{
    F1NativeAdapter, F1NativeCarStatusData, F1NativeState, build_car_status_packet_f23,
    build_car_status_packet_f24, build_car_telemetry_packet_native, build_f1_native_header_bytes,
    normalize, parse_car_status_2023, parse_car_status_2024,
};
use proptest::prelude::*;
use racing_wheel_telemetry_f1::TelemetryAdapter;

const F1_PACKET_MAX: usize = 2048;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..F1_PACKET_MAX)
    ) {
        let adapter = F1NativeAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 1349 bytes (min car-telemetry packet size: header 29 + 22×60)
    /// filled with random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), 1349..=1349)
    ) {
        let adapter = F1NativeAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Round-trip: build header → parse header → verify fields match.
    #[test]
    fn prop_header_round_trip(
        format in prop_oneof![Just(2023u16), Just(2024u16)],
        packet_id in 0..=13u8,
        player_index in 0..=21u8,
    ) {
        let raw = build_f1_native_header_bytes(format, packet_id, player_index);
        let header = parse_header(&raw).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(header.packet_format, format);
        prop_assert_eq!(header.packet_id, packet_id);
        prop_assert_eq!(header.player_car_index, player_index);
    }

    /// Round-trip: build car telemetry packet → parse → verify key fields match.
    #[test]
    fn prop_car_telemetry_round_trip(
        format in prop_oneof![Just(2023u16), Just(2024u16)],
        player_index in 0..=21u8,
        speed in 0..=400u16,
        gear in -1..=8i8,
        rpm in 0..=16000u16,
        throttle in 0.0f32..=1.0f32,
        brake in 0.0f32..=1.0f32,
        steer in -1.0f32..=1.0f32,
        drs in 0..=1u8,
        pressure in 15.0f32..=35.0f32,
    ) {
        let raw = build_car_telemetry_packet_native(
            format,
            player_index,
            speed,
            gear,
            rpm,
            throttle,
            brake,
            steer,
            drs,
            [pressure; 4],
        );
        let telem = parse_car_telemetry(&raw, usize::from(player_index))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert_eq!(telem.speed_kmh, speed);
        prop_assert_eq!(telem.gear, gear);
        prop_assert_eq!(telem.engine_rpm, rpm);
        prop_assert_eq!(telem.drs, drs);
        prop_assert!((telem.throttle - throttle).abs() < 1e-5);
        prop_assert!((telem.brake - brake).abs() < 1e-5);
        prop_assert!((telem.steer - steer).abs() < 1e-5);
        for p in &telem.tyres_pressure {
            prop_assert!((*p - pressure).abs() < 1e-4);
        }
    }

    /// Round-trip: build F1 23 car status → parse → verify fields.
    #[test]
    fn prop_car_status_2023_round_trip(
        player_index in 0..=21u8,
        fuel in 0.0f32..=110.0f32,
        ers in 0.0f32..=4_000_000.0f32,
        drs_allowed in 0..=1u8,
        pit_limiter in 0..=1u8,
        compound in 7..=21u8,
        max_rpm in 8000..=16000u16,
    ) {
        let raw = build_car_status_packet_f23(
            player_index, fuel, ers, drs_allowed, pit_limiter, compound, max_rpm,
        );
        let status = parse_car_status_2023(&raw, usize::from(player_index))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert!((status.fuel_in_tank - fuel).abs() < 1e-4);
        prop_assert!((status.ers_store_energy - ers).abs() < 1.0);
        prop_assert_eq!(status.drs_allowed, drs_allowed);
        prop_assert_eq!(status.pit_limiter_status, pit_limiter);
        prop_assert_eq!(status.actual_tyre_compound, compound);
        prop_assert_eq!(status.max_rpm, max_rpm);
        // F1 23 should always have zero engine power
        prop_assert_eq!(status.engine_power_ice, 0.0);
        prop_assert_eq!(status.engine_power_mguk, 0.0);
    }

    /// Round-trip: build F1 24 car status → parse → verify fields.
    #[test]
    fn prop_car_status_2024_round_trip(
        player_index in 0..=21u8,
        fuel in 0.0f32..=110.0f32,
        ers in 0.0f32..=4_000_000.0f32,
        drs_allowed in 0..=1u8,
        pit_limiter in 0..=1u8,
        compound in 7..=21u8,
        max_rpm in 8000..=16000u16,
    ) {
        let raw = build_car_status_packet_f24(
            player_index, fuel, ers, drs_allowed, pit_limiter, compound, max_rpm,
        );
        let status = parse_car_status_2024(&raw, usize::from(player_index))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert!((status.fuel_in_tank - fuel).abs() < 1e-4);
        prop_assert!((status.ers_store_energy - ers).abs() < 1.0);
        prop_assert_eq!(status.drs_allowed, drs_allowed);
        prop_assert_eq!(status.pit_limiter_status, pit_limiter);
        prop_assert_eq!(status.actual_tyre_compound, compound);
        prop_assert_eq!(status.max_rpm, max_rpm);
    }

    /// Normalization invariants: speed_ms >= 0, rpm >= 0, ERS fraction in [0,1].
    #[test]
    fn prop_normalize_invariants(
        speed in 0..=400u16,
        gear in -1..=8i8,
        rpm in 0..=16000u16,
        throttle in 0.0f32..=1.0f32,
        brake in 0.0f32..=1.0f32,
        steer in -1.0f32..=1.0f32,
        ers in 0.0f32..=4_000_000.0f32,
        max_rpm in 0..=16000u16,
    ) {
        let telem = CarTelemetryData {
            speed_kmh: speed,
            throttle,
            steer,
            brake,
            gear,
            engine_rpm: rpm,
            drs: 0,
            brakes_temperature: [0; 4],
            tyres_surface_temperature: [0; 4],
            tyres_inner_temperature: [0; 4],
            engine_temperature: 0,
            tyres_pressure: [22.0; 4],
        };
        let status = F1NativeCarStatusData {
            ers_store_energy: ers,
            max_rpm,
            ..F1NativeCarStatusData::default()
        };
        let norm = normalize(&telem, &status, &SessionData::default());

        prop_assert!(norm.speed_ms >= 0.0, "speed must be non-negative");
        prop_assert!(norm.rpm >= 0.0, "rpm must be non-negative");

        if let Some(openracing_telemetry::TelemetryValue::Float(frac)) =
            norm.extended.get("ers_store_fraction")
        {
            prop_assert!(*frac >= 0.0 && *frac <= 1.0, "ERS fraction out of [0,1]: {}", frac);
        }
        if let Some(openracing_telemetry::TelemetryValue::Float(frac)) =
            norm.extended.get("rpm_fraction")
        {
            prop_assert!(*frac >= 0.0 && *frac <= 1.0, "RPM fraction out of [0,1]: {}", frac);
        }
    }

    /// process_packet never panics on random data with valid format bytes.
    #[test]
    fn prop_process_packet_no_panic(
        data in proptest::collection::vec(any::<u8>(), 29..F1_PACKET_MAX),
    ) {
        // Force a valid format into the first two bytes
        let mut buf = data;
        let format = if buf[0] % 2 == 0 { 2023u16 } else { 2024u16 };
        buf[0..2].copy_from_slice(&format.to_le_bytes());
        let mut state = F1NativeState::default();
        let _ = F1NativeAdapter::process_packet(&mut state, &buf);
    }
}
