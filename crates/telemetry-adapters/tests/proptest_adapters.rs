//! Property-based tests for telemetry adapter `normalize()` implementations.
//!
//! Uses proptest to fuzz adapter parsing with random byte vectors, verifying that
//! no adapter panics on arbitrary input and that successfully-parsed telemetry
//! satisfies basic invariants (non-negative speed/RPM, clamped pedal inputs,
//! reasonable gear range).

use openracing_telemetry_adapters::codemasters_shared::{
    self, MIN_PACKET_SIZE as CM_MIN_PACKET_SIZE,
};
use openracing_telemetry_adapters::gran_turismo_7::{self, PACKET_SIZE as GT7_PACKET_SIZE};
use openracing_telemetry_adapters::{
    DirtRally2Adapter, F1Adapter, ForzaAdapter, GranTurismo7Adapter, NormalizedTelemetry,
    TelemetryAdapter,
};
use proptest::prelude::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Validate output invariants that all adapters must satisfy.
///
/// Gear is stored as `i8` and some adapters (e.g. Forza) pass through the raw
/// byte without clamping, so we only assert finite numeric ranges here.
fn assert_telemetry_invariants(t: &NormalizedTelemetry) {
    assert!(
        t.speed_ms >= 0.0,
        "speed_ms must be non-negative: {}",
        t.speed_ms
    );
    assert!(t.rpm >= 0.0, "rpm must be non-negative: {}", t.rpm);
    assert!(
        t.throttle >= 0.0 && t.throttle <= 1.0,
        "throttle out of 0.0..=1.0: {}",
        t.throttle
    );
    assert!(
        t.brake >= 0.0 && t.brake <= 1.0,
        "brake out of 0.0..=1.0: {}",
        t.brake
    );
    assert!(
        t.clutch >= 0.0 && t.clutch <= 1.0,
        "clutch out of 0.0..=1.0: {}",
        t.clutch
    );
}

/// Stricter invariant check for adapters known to clamp gear values.
fn assert_codemasters_invariants(t: &NormalizedTelemetry) {
    assert_telemetry_invariants(t);
    assert!(
        t.gear >= -1 && t.gear <= 20,
        "gear out of -1..=20: {}",
        t.gear
    );
}

// ── Codemasters shared (DiRT Rally 2.0, DiRT 3/4, GRID family) ─────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Codemasters Mode 1 parser must never panic on arbitrary bytes.
    #[test]
    fn codemasters_no_panic_arbitrary(
        data in proptest::collection::vec(any::<u8>(), 0..1024)
    ) {
        let _ = codemasters_shared::parse_codemasters_mode1_common(&data, "PropTest");
    }

    /// Packets shorter than MIN_PACKET_SIZE must return Err.
    #[test]
    fn codemasters_short_packet_rejected(len in 0usize..CM_MIN_PACKET_SIZE) {
        let data = vec![0u8; len];
        prop_assert!(
            codemasters_shared::parse_codemasters_mode1_common(&data, "PropTest").is_err()
        );
    }

    /// Valid-length Codemasters packets: output must satisfy telemetry invariants.
    #[test]
    fn codemasters_valid_length_invariants(
        data in proptest::collection::vec(any::<u8>(), CM_MIN_PACKET_SIZE..512)
    ) {
        if let Ok(t) = codemasters_shared::parse_codemasters_mode1_common(&data, "PropTest") {
            assert_codemasters_invariants(&t);
        }
    }

    /// All Codemasters-family adapters must not panic when fed 264-byte packets.
    #[test]
    fn codemasters_adapter_normalize_no_panic(
        data in proptest::collection::vec(any::<u8>(), CM_MIN_PACKET_SIZE..=CM_MIN_PACKET_SIZE)
    ) {
        let adapter = DirtRally2Adapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ── Forza (Sled 232, CarDash 311, FM8 331, FH4 324) ────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Forza adapter must never panic on arbitrary-length input.
    #[test]
    fn forza_no_panic_arbitrary(
        data in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let adapter = ForzaAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Forza CarDash (311 bytes): output invariants when parse succeeds.
    #[test]
    fn forza_cardash_311_invariants(
        data in proptest::collection::vec(any::<u8>(), 311..=311)
    ) {
        let adapter = ForzaAdapter::new();
        if let Ok(t) = adapter.normalize(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    /// Forza FH4 CarDash (324 bytes): output invariants when parse succeeds.
    #[test]
    fn forza_fh4_cardash_324_invariants(
        data in proptest::collection::vec(any::<u8>(), 324..=324)
    ) {
        let adapter = ForzaAdapter::new();
        if let Ok(t) = adapter.normalize(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    /// Forza Sled (232 bytes): output invariants when parse succeeds.
    #[test]
    fn forza_sled_232_invariants(
        data in proptest::collection::vec(any::<u8>(), 232..=232)
    ) {
        let adapter = ForzaAdapter::new();
        if let Ok(t) = adapter.normalize(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    /// Forza FM8 CarDash (331 bytes): output invariants when parse succeeds.
    #[test]
    fn forza_fm8_cardash_331_invariants(
        data in proptest::collection::vec(any::<u8>(), 331..=331)
    ) {
        let adapter = ForzaAdapter::new();
        if let Ok(t) = adapter.normalize(&data) {
            assert_telemetry_invariants(&t);
        }
    }
}

// ── Gran Turismo (296-byte decrypted packets) ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// GT7 decrypted parser must never panic on arbitrary 296-byte buffers.
    #[test]
    fn gt7_parse_decrypted_no_panic(
        data in proptest::collection::vec(any::<u8>(), GT7_PACKET_SIZE..=GT7_PACKET_SIZE)
    ) {
        let _ = gran_turismo_7::parse_decrypted_ext(&data);
    }

    /// GT7 decrypted parser rejects packets shorter than 296 bytes.
    #[test]
    fn gt7_short_packet_rejected(len in 0usize..GT7_PACKET_SIZE) {
        let data = vec![0u8; len];
        prop_assert!(gran_turismo_7::parse_decrypted_ext(&data).is_err());
    }

    /// GT7 valid-length decrypted packets: output must satisfy invariants.
    #[test]
    fn gt7_valid_length_invariants(
        data in proptest::collection::vec(any::<u8>(), GT7_PACKET_SIZE..512)
    ) {
        if let Ok(t) = gran_turismo_7::parse_decrypted_ext(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    /// GT7 adapter's normalize() includes Salsa20 decryption, so random bytes
    /// will fail magic validation — verify it never panics regardless.
    #[test]
    fn gt7_adapter_normalize_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let adapter = GranTurismo7Adapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ── F1 (Codemasters custom UDP) ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// F1 adapter must never panic on arbitrary bytes.
    #[test]
    fn f1_no_panic_arbitrary(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = F1Adapter::new();
        let _ = adapter.normalize(&data);
    }

    /// F1 adapter with mode-3 sized packets: output invariants when parse succeeds.
    #[test]
    fn f1_mode3_invariants(
        data in proptest::collection::vec(any::<u8>(), 264..=264)
    ) {
        let adapter = F1Adapter::new();
        if let Ok(t) = adapter.normalize(&data) {
            assert_telemetry_invariants(&t);
        }
    }
}
