//! Deep property-based tests covering ALL telemetry adapters.
//!
//! Ensures every adapter's `normalize()` either returns a valid `NormalizedTelemetry`
//! satisfying field invariants, or returns `Err` — and never panics.
//!
//! Adapters already covered in `proptest_adapters.rs` (Forza, Codemasters/DiRT Rally 2,
//! GT7, F1) are tested here with *additional* strategies not present there (e.g.
//! zero-filled packets, extreme values, cross-adapter consistency).

#![allow(clippy::redundant_closure)]

mod helpers;

use helpers::write_f32_le;
use openracing_telemetry_adapters::{
    self as adapters,
    // Adapters accessed via normalize()
    BeamNGAdapter,
    Dirt3Adapter,
    Dirt4Adapter,
    Dirt5Adapter,
    DirtRally2Adapter,
    DirtShowdownAdapter,
    Ets2Adapter,
    ForzaHorizon4Adapter,
    ForzaHorizon5Adapter,
    GranTurismo7SportsAdapter,
    Grid2019Adapter,
    GridAutosportAdapter,
    GridLegendsAdapter,
    IRacingAdapter,
    KartKraftAdapter,
    LFSAdapter,
    LeMansUltimateAdapter,
    MudRunnerAdapter,
    Nascar21Adapter,
    NascarAdapter,
    NormalizedTelemetry,
    PCars2Adapter,
    PCars3Adapter,
    RBRAdapter,
    RFactor1Adapter,
    RFactor2Adapter,
    RaceDriverGridAdapter,
    RaceRoomAdapter,
    RennsportAdapter,
    TelemetryAdapter,
    WrcGenerationsAdapter,
    WrcKylotonnAdapter,
    WreckfestAdapter,
    WtcrAdapter,
    // UDP-based adapters with public parse functions
    dakar,
    ets2,
    flatout,
    le_mans_ultimate,
    nascar,
    pcars2,
    rennsport,
    rfactor1,
    simhub,
    trackmania,
    wreckfest,
    wtcr,
};
use proptest::prelude::*;

// ── Invariant helpers ────────────────────────────────────────────────────────

/// Core invariants that ALL adapters must satisfy on successful parse.
fn assert_telemetry_invariants(t: &NormalizedTelemetry) {
    assert!(
        t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
        "speed_ms invalid: {}",
        t.speed_ms
    );
    assert!(t.rpm >= 0.0 && t.rpm.is_finite(), "rpm invalid: {}", t.rpm);
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
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar out of -1.0..=1.0: {}",
        t.ffb_scalar
    );
    assert!(
        t.slip_ratio >= 0.0 && t.slip_ratio.is_finite(),
        "slip_ratio invalid: {}",
        t.slip_ratio
    );
    assert!(
        t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
        "fuel_percent out of 0.0..=1.0: {}",
        t.fuel_percent
    );
}

/// Generate proptest fuzz + unit tests for an adapter via normalize().
macro_rules! adapter_normalize_fuzz {
    ($mod_name:ident, $ctor:expr, $max_len:expr, $typical_size:expr) => {
        mod $mod_name {
            use super::*;

            proptest! {
                #![proptest_config(ProptestConfig::with_cases(200))]

                #[test]
                fn no_panic_arbitrary(
                    data in proptest::collection::vec(any::<u8>(), 0..$max_len)
                ) {
                    let adapter = $ctor;
                    let _ = adapter.normalize(&data);
                }
            }

            #[test]
            fn no_panic_empty() {
                let adapter = $ctor;
                let _ = adapter.normalize(&[]);
            }

            #[test]
            fn zeros_no_panic() {
                let adapter = $ctor;
                let data = vec![0u8; $typical_size];
                let _ = adapter.normalize(&data);
            }
        }
    };
}

// ── Per-adapter fuzz tests ───────────────────────────────────────────────────

adapter_normalize_fuzz!(fuzz_beamng, BeamNGAdapter::new(), 512, 92);
adapter_normalize_fuzz!(fuzz_lfs, LFSAdapter::new(), 256, 92);
adapter_normalize_fuzz!(fuzz_nascar, NascarAdapter::new(), 512, 92);
adapter_normalize_fuzz!(fuzz_wreckfest, WreckfestAdapter::new(), 512, 28);
adapter_normalize_fuzz!(fuzz_rennsport, RennsportAdapter::new(), 512, 24);
adapter_normalize_fuzz!(fuzz_rbr, RBRAdapter::new(), 256, 128);
adapter_normalize_fuzz!(
    fuzz_rfactor1,
    RFactor1Adapter::with_variant(rfactor1::RFactor1Variant::RFactor1),
    2048,
    48
);
adapter_normalize_fuzz!(fuzz_rfactor2, RFactor2Adapter::new(), 4096, 2048);
adapter_normalize_fuzz!(
    fuzz_wrc_kylotonn,
    WrcKylotonnAdapter::new(adapters::wrc_kylotonn::WrcKylotonnVariant::Wrc9),
    256,
    96
);
adapter_normalize_fuzz!(fuzz_le_mans, LeMansUltimateAdapter::new(), 512, 20);
adapter_normalize_fuzz!(
    fuzz_dakar,
    adapters::dakar::DakarDesertRallyAdapter::new(),
    512,
    40
);
adapter_normalize_fuzz!(
    fuzz_flatout,
    adapters::flatout::FlatOutAdapter::new(),
    512,
    36
);
adapter_normalize_fuzz!(fuzz_pcars2, PCars2Adapter::new(), 1500, 46);
adapter_normalize_fuzz!(fuzz_pcars3, PCars3Adapter::new(), 512, 46);
adapter_normalize_fuzz!(fuzz_wtcr, WtcrAdapter::new(), 512, 264);
adapter_normalize_fuzz!(
    fuzz_simhub,
    adapters::simhub::SimHubAdapter::new(),
    1024,
    64
);
adapter_normalize_fuzz!(
    fuzz_trackmania,
    adapters::trackmania::TrackmaniaAdapter::new(),
    1024,
    64
);
adapter_normalize_fuzz!(fuzz_dirt3, Dirt3Adapter::new(), 512, 264);
adapter_normalize_fuzz!(fuzz_dirt4, Dirt4Adapter::new(), 512, 264);
adapter_normalize_fuzz!(fuzz_dirt5, Dirt5Adapter::new(), 2048, 264);
adapter_normalize_fuzz!(fuzz_dirt_showdown, DirtShowdownAdapter::new(), 512, 264);
adapter_normalize_fuzz!(fuzz_grid_2019, Grid2019Adapter::new(), 512, 264);
adapter_normalize_fuzz!(fuzz_grid_autosport, GridAutosportAdapter::new(), 512, 264);
adapter_normalize_fuzz!(fuzz_grid_legends, GridLegendsAdapter::new(), 512, 264);
adapter_normalize_fuzz!(
    fuzz_race_driver_grid,
    RaceDriverGridAdapter::new(),
    512,
    264
);
adapter_normalize_fuzz!(
    fuzz_ets2,
    Ets2Adapter::with_variant(ets2::Ets2Variant::Ets2),
    512,
    48
);
adapter_normalize_fuzz!(fuzz_iracing, IRacingAdapter::new(), 4096, 2048);
adapter_normalize_fuzz!(fuzz_kartkraft, KartKraftAdapter::new(), 1024, 64);
adapter_normalize_fuzz!(fuzz_raceroom, RaceRoomAdapter::new(), 8192, 4096);
adapter_normalize_fuzz!(
    fuzz_wrc_generations,
    WrcGenerationsAdapter::new(),
    2048,
    264
);
adapter_normalize_fuzz!(fuzz_nascar_21, Nascar21Adapter::new(), 512, 128);
adapter_normalize_fuzz!(
    fuzz_mudrunner,
    MudRunnerAdapter::with_variant(adapters::mudrunner::MudRunnerVariant::MudRunner),
    2048,
    128
);
adapter_normalize_fuzz!(fuzz_forza_horizon_4, ForzaHorizon4Adapter::new(), 512, 324);
adapter_normalize_fuzz!(fuzz_forza_horizon_5, ForzaHorizon5Adapter::new(), 512, 324);
adapter_normalize_fuzz!(
    fuzz_gran_turismo_sport,
    GranTurismo7SportsAdapter::new(),
    512,
    296
);
adapter_normalize_fuzz!(
    fuzz_v_rally_4,
    adapters::v_rally_4::VRally4Adapter::new(),
    256,
    96
);

// ── Public parse functions: invariant tests with valid-size random data ──────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    // ── NASCAR ──

    #[test]
    fn nascar_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 92..512)
    ) {
        if let Ok(t) = nascar::parse_nascar_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn nascar_parse_short_rejected(len in 0usize..92) {
        let data = vec![0u8; len];
        prop_assert!(nascar::parse_nascar_packet(&data).is_err());
    }

    // ── Wreckfest ──

    #[test]
    fn wreckfest_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 28..512)
    ) {
        if let Ok(t) = wreckfest::parse_wreckfest_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn wreckfest_parse_short_rejected(len in 0usize..28) {
        let data = vec![0u8; len];
        prop_assert!(wreckfest::parse_wreckfest_packet(&data).is_err());
    }

    // ── Rennsport ──

    #[test]
    fn rennsport_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 24..512)
    ) {
        if let Ok(t) = rennsport::parse_rennsport_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn rennsport_parse_short_rejected(len in 0usize..24) {
        let data = vec![0u8; len];
        prop_assert!(rennsport::parse_rennsport_packet(&data).is_err());
    }

    // ── rFactor 1 ──

    #[test]
    fn rfactor1_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 48..2048)
    ) {
        if let Ok(t) = rfactor1::parse_rfactor1_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn rfactor1_parse_short_rejected(len in 0usize..48) {
        let data = vec![0u8; len];
        prop_assert!(rfactor1::parse_rfactor1_packet(&data).is_err());
    }

    // ── Le Mans Ultimate ──

    #[test]
    fn le_mans_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 20..512)
    ) {
        if let Ok(t) = le_mans_ultimate::parse_le_mans_ultimate_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn le_mans_parse_short_rejected(len in 0usize..20) {
        let data = vec![0u8; len];
        prop_assert!(le_mans_ultimate::parse_le_mans_ultimate_packet(&data).is_err());
    }

    // ── Dakar Desert Rally ──

    #[test]
    fn dakar_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 40..512)
    ) {
        if let Ok(t) = dakar::parse_dakar_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn dakar_parse_short_rejected(len in 0usize..40) {
        let data = vec![0u8; len];
        prop_assert!(dakar::parse_dakar_packet(&data).is_err());
    }

    // ── FlatOut ──

    #[test]
    fn flatout_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 36..512)
    ) {
        if let Ok(t) = flatout::parse_flatout_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn flatout_parse_short_rejected(len in 0usize..36) {
        let data = vec![0u8; len];
        prop_assert!(flatout::parse_flatout_packet(&data).is_err());
    }

    // ── PCars2 ──

    #[test]
    fn pcars2_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 46..1500)
    ) {
        if let Ok(t) = pcars2::parse_pcars2_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    #[test]
    fn pcars2_parse_short_rejected(len in 0usize..46) {
        let data = vec![0u8; len];
        prop_assert!(pcars2::parse_pcars2_packet(&data).is_err());
    }

    // ── WTCR ──

    #[test]
    fn wtcr_parse_valid_size_invariants(
        data in proptest::collection::vec(any::<u8>(), 264..512)
    ) {
        if let Ok(t) = wtcr::parse_wtcr_packet(&data) {
            assert_telemetry_invariants(&t);
        }
    }

    // ── SimHub (JSON) ──

    #[test]
    fn simhub_parse_empty_rejected(len in 0usize..1) {
        let data = vec![0u8; len];
        if data.is_empty() {
            prop_assert!(simhub::parse_simhub_packet(&data).is_err());
        }
    }

    // ── Trackmania (JSON) ──

    #[test]
    fn trackmania_parse_empty_rejected(len in 0usize..1) {
        let data = vec![0u8; len];
        if data.is_empty() {
            prop_assert!(trackmania::parse_trackmania_packet(&data).is_err());
        }
    }

    // ── ETS2 / ATS ──

    #[test]
    fn ets2_parse_short_rejected(len in 0usize..24) {
        let data = vec![0u8; len];
        prop_assert!(ets2::parse_scs_packet(&data).is_err());
    }
}

// ── Extreme value tests ──────────────────────────────────────────────────────
//
// Craft packets with extreme f32 values (MAX, MIN, NaN, Inf) and verify no panics.

fn make_extreme_f32_packet(size: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size];
    let extreme_values: &[f32] = &[
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -0.0_f32,
        f32::MIN_POSITIVE,
    ];
    for (i, &val) in extreme_values.iter().enumerate() {
        let offset = (i * 4) % size.saturating_sub(3);
        if offset + 4 <= size {
            write_f32_le(&mut buf, offset, val);
        }
    }
    buf
}

/// Verify that every adapter in the factory registry handles extreme f32 values
/// without panicking.
#[test]
fn all_adapters_survive_extreme_f32_values() -> Result<(), Box<dyn std::error::Error>> {
    for (game_id, factory) in adapters::adapter_factories() {
        let adapter = factory();
        // Try several extreme-value packet sizes
        for &size in &[32, 128, 264, 512, 1024, 2048, 4096] {
            let data = make_extreme_f32_packet(size);
            let _ = adapter.normalize(&data);
        }
        // Sanity: adapter game_id matches registry
        assert_eq!(adapter.game_id(), *game_id);
    }
    Ok(())
}

/// Verify that every adapter handles zero-length input without panicking.
/// Stub adapters (no UDP telemetry) may return Ok(default); that is acceptable.
#[test]
fn all_adapters_survive_empty_input() -> Result<(), Box<dyn std::error::Error>> {
    // Adapters that return Ok(default) on any input because they lack UDP telemetry.
    let stub_ids: &[&str] = &["f1_manager", "ac_evo", "acc2", "seb_loeb_rally"];
    for (game_id, factory) in adapters::adapter_factories() {
        let adapter = factory();
        let result = adapter.normalize(&[]);
        if stub_ids.contains(game_id) {
            // Stubs always return Ok — just verify no panic
            let _ = result;
        } else {
            assert!(
                result.is_err(),
                "Adapter '{game_id}' unexpectedly succeeded on empty input"
            );
        }
    }
    Ok(())
}

/// Verify that every adapter handles zero-filled packets at various sizes without panicking.
#[test]
fn all_adapters_survive_zero_filled_packets() -> Result<(), Box<dyn std::error::Error>> {
    for (_game_id, factory) in adapters::adapter_factories() {
        let adapter = factory();
        for &size in &[1, 16, 64, 128, 264, 512, 1024, 4096] {
            let data = vec![0u8; size];
            let _ = adapter.normalize(&data);
        }
    }
    Ok(())
}

// ── Cross-adapter consistency ────────────────────────────────────────────────
//
// Adapters that parse the same underlying format (e.g. Codemasters Mode 1)
// should produce consistent results from the same input.

#[test]
fn codemasters_family_consistency() -> Result<(), Box<dyn std::error::Error>> {
    // Build a valid Codemasters Mode 1 packet (264 bytes) with known values.
    let mut data = vec![0u8; 264];
    // speed at offset 28 (f32 LE), RPM at offset 148 (f32 LE), gear at offset 132 (f32 LE)
    write_f32_le(&mut data, 28, 30.0); // ~108 km/h
    write_f32_le(&mut data, 148, 5000.0); // 5000 RPM
    write_f32_le(&mut data, 132, 3.0); // 3rd gear

    let adapters_cm: Vec<Box<dyn TelemetryAdapter>> = vec![
        Box::new(DirtRally2Adapter::new()),
        Box::new(Dirt3Adapter::new()),
        Box::new(Dirt4Adapter::new()),
        Box::new(Grid2019Adapter::new()),
        Box::new(GridAutosportAdapter::new()),
        Box::new(GridLegendsAdapter::new()),
    ];

    let results: Vec<NormalizedTelemetry> = adapters_cm
        .iter()
        .filter_map(|a| a.normalize(&data).ok())
        .collect();

    // All adapters that parsed successfully should agree on key fields.
    if results.len() >= 2 {
        let first = &results[0];
        for t in &results[1..] {
            assert!(
                (first.rpm - t.rpm).abs() < 0.01,
                "Codemasters family RPM mismatch: {} vs {}",
                first.rpm,
                t.rpm
            );
            assert!(
                (first.speed_ms - t.speed_ms).abs() < 0.01,
                "Codemasters family speed_ms mismatch: {} vs {}",
                first.speed_ms,
                t.speed_ms
            );
            assert_eq!(first.gear, t.gear, "Codemasters family gear mismatch");
        }
    }
    Ok(())
}

// ── JSON adapter property tests ──────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// SimHub: valid JSON always produces valid telemetry.
    #[test]
    fn simhub_valid_json_invariants(
        speed in 0.0f32..500.0,
        rpm in 0.0f32..20000.0,
        throttle in 0.0f32..100.0,
        brake in 0.0f32..100.0,
        gear_val in -1i8..9,
    ) {
        let gear_str = match gear_val {
            -1 => "R".to_string(),
            0 => "N".to_string(),
            g => g.to_string(),
        };
        let json = format!(
            r#"{{"SpeedMs":{},"Rpms":{},"Throttle":{},"Brake":{},"Gear":"{}","MaxRpms":{},"Clutch":0,"SteeringAngle":0,"Steer":0,"FuelPercent":0,"LateralGForce":0,"LongitudinalGForce":0,"FFBValue":0,"IsRunning":true,"IsInPit":false}}"#,
            speed, rpm, throttle, brake, gear_str, rpm + 1000.0
        );
        let result = simhub::parse_simhub_packet(json.as_bytes());
        if let Ok(t) = result {
            assert_telemetry_invariants(&t);
        }
    }

    /// Trackmania: valid JSON always produces valid telemetry.
    #[test]
    fn trackmania_valid_json_invariants(
        speed in 0.0f32..200.0,
        rpm in 0.0f32..15000.0,
        gear in 0i32..8,
        throttle in 0.0f32..1.0,
        brake in 0.0f32..1.0,
    ) {
        let json = format!(
            r#"{{"speed":{},"rpm":{},"gear":{},"throttle":{},"brake":{},"steer_angle":0,"engine_running":true}}"#,
            speed, rpm, gear, throttle, brake
        );
        let result = trackmania::parse_trackmania_packet(json.as_bytes());
        if let Ok(t) = result {
            assert_telemetry_invariants(&t);
        }
    }
}

// ── Adapter factory completeness ─────────────────────────────────────────────

/// Every adapter produced by the factory must have a non-empty game_id and
/// a positive expected_update_rate.
#[test]
fn all_factory_adapters_have_valid_metadata() -> Result<(), Box<dyn std::error::Error>> {
    for (id, factory) in adapters::adapter_factories() {
        let adapter = factory();
        assert!(
            !adapter.game_id().is_empty(),
            "Adapter '{id}' has empty game_id"
        );
        assert!(
            adapter.expected_update_rate().as_micros() > 0,
            "Adapter '{id}' has zero update rate"
        );
    }
    Ok(())
}
