//! Pipeline adapter tests — cross-cutting coverage for adapter behavior.
//!
//! Focuses on gaps:
//! - All adapter factories produce valid adapters with non-empty game_ids
//! - Forza adapter: empty, minimal, maximal, malformed payloads
//! - Adapter normalize rejects zero-length data
//! - telemetry_now_ns monotonicity in adapter context
//! - Serialization roundtrip for NormalizedTelemetry produced by adapters
//! - Property-based: random payloads never panic on normalize

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::{
    ForzaAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, adapter_factories,
    telemetry_now_ns,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter factory registry
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_adapter_factories_produce_valid_game_ids() -> TestResult {
    let factories = adapter_factories();
    assert!(!factories.is_empty(), "factory registry must not be empty");

    for (factory_id, factory_fn) in factories {
        let adapter = factory_fn();
        let game_id = adapter.game_id();
        assert!(
            !game_id.is_empty(),
            "factory '{factory_id}' produced adapter with empty game_id"
        );
    }
    Ok(())
}

#[test]
fn all_adapter_factories_have_unique_registry_ids() -> TestResult {
    let factories = adapter_factories();
    let mut seen = std::collections::HashSet::new();

    for (factory_id, _) in factories {
        assert!(
            seen.insert(*factory_id),
            "duplicate factory registry id: {factory_id}"
        );
    }
    Ok(())
}

#[test]
fn all_adapters_return_positive_update_rate() -> TestResult {
    let factories = adapter_factories();

    for (factory_id, factory_fn) in factories {
        let adapter = factory_fn();
        let rate = adapter.expected_update_rate();
        assert!(
            rate > Duration::ZERO,
            "factory '{factory_id}' returned zero update rate"
        );
        assert!(
            rate <= Duration::from_secs(1),
            "factory '{factory_id}' update rate {rate:?} seems too slow"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// telemetry_now_ns monotonicity (adapter crate)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_telemetry_now_ns_is_monotonic() -> TestResult {
    let mut prev = telemetry_now_ns();
    for _ in 0..100 {
        let curr = telemetry_now_ns();
        assert!(curr >= prev, "timestamps must be monotonic");
        prev = curr;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Forza adapter: payload edge cases
// ═══════════════════════════════════════════════════════════════════════════════

const FORZA_SLED_SIZE: usize = 232;
const FORZA_CARDASH_SIZE: usize = 311;
const FORZA_FM8_CARDASH_SIZE: usize = 331;
const FORZA_FH4_CARDASH_SIZE: usize = 324;

fn forza() -> ForzaAdapter {
    ForzaAdapter::new()
}

#[test]
fn forza_rejects_empty_packet() -> TestResult {
    let result = forza().normalize(&[]);
    assert!(result.is_err(), "empty packet must be rejected");
    Ok(())
}

#[test]
fn forza_rejects_one_byte_packet() -> TestResult {
    let result = forza().normalize(&[0x42]);
    assert!(result.is_err(), "single byte must be rejected");
    Ok(())
}

#[test]
fn forza_rejects_undersized_sled_packet() -> TestResult {
    let data = vec![0u8; FORZA_SLED_SIZE - 1];
    let result = forza().normalize(&data);
    assert!(
        result.is_err(),
        "231 bytes should be rejected (not a known format)"
    );
    Ok(())
}

#[test]
fn forza_accepts_exact_sled_packet() -> TestResult {
    let mut data = vec![0u8; FORZA_SLED_SIZE];
    // Set is_race_on to 1 (offset 0, i32 LE)
    data[0] = 1;
    let telemetry = forza().normalize(&data)?;
    assert_eq!(
        telemetry.speed_ms, 0.0,
        "zero-filled sled should give zero speed"
    );
    Ok(())
}

#[test]
fn forza_sled_race_not_on_returns_empty() -> TestResult {
    let data = vec![0u8; FORZA_SLED_SIZE]; // is_race_on = 0
    let telemetry = forza().normalize(&data)?;
    assert_eq!(telemetry.rpm, 0.0);
    assert_eq!(telemetry.speed_ms, 0.0);
    Ok(())
}

#[test]
fn forza_accepts_exact_cardash_packet() -> TestResult {
    let mut data = vec![0u8; FORZA_CARDASH_SIZE];
    data[0] = 1; // is_race_on
    let _telemetry = forza().normalize(&data)?;
    Ok(())
}

#[test]
fn forza_accepts_fm8_cardash_packet() -> TestResult {
    let mut data = vec![0u8; FORZA_FM8_CARDASH_SIZE];
    data[0] = 1;
    let _telemetry = forza().normalize(&data)?;
    Ok(())
}

#[test]
fn forza_accepts_fh4_cardash_packet() -> TestResult {
    let mut data = vec![0u8; FORZA_FH4_CARDASH_SIZE];
    data[0] = 1;
    let _telemetry = forza().normalize(&data)?;
    Ok(())
}

#[test]
fn forza_rejects_unknown_size() -> TestResult {
    let data = vec![0u8; 250]; // not a known format
    let result = forza().normalize(&data);
    assert!(result.is_err(), "unknown packet size should be rejected");
    Ok(())
}

#[test]
fn forza_cardash_parses_speed_and_inputs() -> TestResult {
    let mut data = vec![0u8; FORZA_CARDASH_SIZE];
    data[0] = 1; // is_race_on

    // Write speed at offset 244 (f32 LE)
    write_f32_le(&mut data, 244, 30.0);
    // Write RPM at offset 16
    write_f32_le(&mut data, 16, 7500.0);
    // Write throttle at offset 303 (u8, 0-255)
    data[303] = 200;
    // Write brake at offset 304
    data[304] = 50;
    // Write gear at offset 307 (2 = 1st gear)
    data[307] = 3; // 2nd gear

    let t = forza().normalize(&data)?;

    assert!(
        (t.speed_ms - 30.0).abs() < 0.01,
        "speed mismatch: {}",
        t.speed_ms
    );
    assert!((t.rpm - 7500.0).abs() < 0.01, "rpm mismatch: {}", t.rpm);
    assert!((t.throttle - (200.0 / 255.0)).abs() < 0.01);
    assert!((t.brake - (50.0 / 255.0)).abs() < 0.01);
    assert_eq!(t.gear, 2, "gear 3 raw → 2nd gear");
    Ok(())
}

#[test]
fn forza_cardash_gear_encoding() -> TestResult {
    // gear byte: 0=R, 1=N, 2=1st, ..., 9=8th
    let test_cases: Vec<(u8, i8)> = vec![
        (0, -1), // Reverse
        (1, 0),  // Neutral
        (2, 1),  // 1st
        (3, 2),  // 2nd
        (9, 8),  // 8th
    ];

    for (raw_gear, expected) in test_cases {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        data[0] = 1; // is_race_on
        data[307] = raw_gear;

        let t = forza().normalize(&data)?;
        assert_eq!(
            t.gear, expected,
            "raw gear {raw_gear} should map to {expected}, got {}",
            t.gear
        );
    }
    Ok(())
}

#[test]
fn forza_sled_g_forces_from_acceleration() -> TestResult {
    let mut data = vec![0u8; FORZA_SLED_SIZE];
    data[0] = 1; // is_race_on

    // Write acceleration values
    let g = 9.806_65f32;
    write_f32_le(&mut data, 20, 2.0 * g); // accel_x (lateral)
    write_f32_le(&mut data, 28, 1.0 * g); // accel_z (longitudinal)

    let t = forza().normalize(&data)?;
    assert!(
        (t.lateral_g - 2.0).abs() < 0.01,
        "lateral_g: {}",
        t.lateral_g
    );
    assert!(
        (t.longitudinal_g - 1.0).abs() < 0.01,
        "longitudinal_g: {}",
        t.longitudinal_g
    );
    Ok(())
}

#[test]
fn forza_cardash_extended_fields_present() -> TestResult {
    let mut data = vec![0u8; FORZA_CARDASH_SIZE];
    data[0] = 1; // is_race_on

    let t = forza().normalize(&data)?;

    // Verify extended fields are populated (even if zero)
    let expected_keys = [
        "wheel_speed_fl",
        "wheel_speed_fr",
        "wheel_speed_rl",
        "wheel_speed_rr",
        "suspension_travel_fl",
        "suspension_travel_fr",
        "suspension_travel_rl",
        "suspension_travel_rr",
    ];

    for key in &expected_keys {
        assert!(
            t.extended.contains_key(*key),
            "missing extended field: {key}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter normalize: serialization roundtrip
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_output_serializes_to_json() -> TestResult {
    let mut data = vec![0u8; FORZA_CARDASH_SIZE];
    data[0] = 1;
    write_f32_le(&mut data, 16, 5000.0); // RPM
    write_f32_le(&mut data, 244, 40.0); // speed

    let telemetry = forza().normalize(&data)?;
    let json = serde_json::to_string(&telemetry)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(deserialized.rpm, telemetry.rpm);
    assert_eq!(deserialized.speed_ms, telemetry.speed_ms);
    Ok(())
}

#[test]
fn adapter_output_wraps_into_frame_and_roundtrips() -> TestResult {
    let mut data = vec![0u8; FORZA_CARDASH_SIZE];
    data[0] = 1;
    write_f32_le(&mut data, 16, 6000.0);

    let telemetry = forza().normalize(&data)?;
    let frame = TelemetryFrame::new(telemetry, telemetry_now_ns(), 0, data.len());

    let json = serde_json::to_string(&frame)?;
    let deserialized: TelemetryFrame = serde_json::from_str(&json)?;

    assert_eq!(deserialized.raw_size, FORZA_CARDASH_SIZE);
    assert_eq!(deserialized.data.rpm, frame.data.rpm);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Property-based: random payloads never panic
// ═══════════════════════════════════════════════════════════════════════════════

mod proptest_adapters_pipeline {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn forza_normalize_never_panics_on_random_sled(data in proptest::collection::vec(any::<u8>(), 232..=232)) {
            // Should not panic; may return Ok or Err
            let _result = forza().normalize(&data);
        }

        #[test]
        fn forza_normalize_never_panics_on_random_cardash(data in proptest::collection::vec(any::<u8>(), 311..=311)) {
            let _result = forza().normalize(&data);
        }

        #[test]
        fn forza_normalize_never_panics_on_arbitrary_length(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _result = forza().normalize(&data);
        }

        #[test]
        fn adapter_game_id_is_non_empty_for_any_factory(idx in 0usize..50) {
            let factories = adapter_factories();
            if idx < factories.len() {
                let (_, factory_fn) = &factories[idx];
                let adapter = factory_fn();
                prop_assert!(!adapter.game_id().is_empty());
            }
        }

        #[test]
        fn telemetry_now_ns_pair_is_monotonic(sleep_us in 0u64..100) {
            let t1 = telemetry_now_ns();
            if sleep_us > 0 {
                std::thread::sleep(Duration::from_micros(sleep_us));
            }
            let t2 = telemetry_now_ns();
            prop_assert!(t2 >= t1, "monotonicity violation: {t1} > {t2}");
        }
    }
}
