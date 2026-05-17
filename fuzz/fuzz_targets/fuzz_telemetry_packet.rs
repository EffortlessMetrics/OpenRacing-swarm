//! Combined telemetry adapter fuzz target.
//!
//! Feeds the same arbitrary byte slice into every telemetry adapter's
//! `normalize()` entry point. This is a catch-all target that exercises all
//! game-specific parsers in a single run, maximising coverage per corpus entry.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_telemetry_packet

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_telemetry_adapters::{
    ACC2Adapter, ACCAdapter, ACEvoAdapter, ACRallyAdapter, AMS2Adapter, AssettoCorsaAdapter,
    Automobilista1Adapter, BeamNGAdapter, DakarDesertRallyAdapter, Dirt3Adapter, Dirt4Adapter,
    Dirt5Adapter, DirtRally2Adapter, DirtShowdownAdapter, EAWRCAdapter, Ets2Adapter, F1_25Adapter,
    F1Adapter, F1ManagerAdapter, F1NativeAdapter, FlatOutAdapter, ForzaAdapter,
    ForzaHorizon4Adapter, ForzaHorizon5Adapter, GranTurismo7Adapter, GranTurismo7SportsAdapter,
    GravelAdapter, Grid2019Adapter, GridAutosportAdapter, GridLegendsAdapter, IRacingAdapter,
    KartKraftAdapter, LFSAdapter, LeMansUltimateAdapter, MotoGPAdapter, MudRunnerAdapter,
    Nascar21Adapter, NascarAdapter, PCars2Adapter, PCars3Adapter, RBRAdapter, RFactor1Adapter,
    RFactor2Adapter, RaceDriverGridAdapter, RaceRoomAdapter, RennsportAdapter, Ride5Adapter,
    SebLoebRallyAdapter, SimHubAdapter, TelemetryAdapter, TrackmaniaAdapter, VRally4Adapter,
    WrcGenerationsAdapter, WrcKylotonnAdapter, WreckfestAdapter, WtcrAdapter,
};

fuzz_target!(|data: &[u8]| {
    // Adapters with simple `new()` constructors.
    let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![
        Box::new(ACCAdapter::new()),
        Box::new(ACC2Adapter::new()),
        Box::new(ACEvoAdapter::new()),
        Box::new(ACRallyAdapter::new()),
        Box::new(AMS2Adapter::new()),
        Box::new(AssettoCorsaAdapter::new()),
        Box::new(Automobilista1Adapter::new()),
        Box::new(BeamNGAdapter::new()),
        Box::new(DakarDesertRallyAdapter::new()),
        Box::new(Dirt3Adapter::new()),
        Box::new(Dirt4Adapter::new()),
        Box::new(Dirt5Adapter::new()),
        Box::new(DirtRally2Adapter::new()),
        Box::new(DirtShowdownAdapter::new()),
        Box::new(EAWRCAdapter::new()),
        Box::new(F1Adapter::new()),
        Box::new(F1_25Adapter::new()),
        Box::new(F1ManagerAdapter::new()),
        Box::new(F1NativeAdapter::new()),
        Box::new(FlatOutAdapter::new()),
        Box::new(ForzaAdapter::new()),
        Box::new(ForzaHorizon4Adapter::new()),
        Box::new(ForzaHorizon5Adapter::new()),
        Box::new(GranTurismo7Adapter::new()),
        Box::new(GranTurismo7SportsAdapter::new()),
        Box::new(GravelAdapter::new()),
        Box::new(Grid2019Adapter::new()),
        Box::new(GridAutosportAdapter::new()),
        Box::new(GridLegendsAdapter::new()),
        Box::new(IRacingAdapter::new()),
        Box::new(KartKraftAdapter::new()),
        Box::new(LeMansUltimateAdapter::new()),
        Box::new(LFSAdapter::new()),
        Box::new(MotoGPAdapter::new()),
        Box::new(Nascar21Adapter::new()),
        Box::new(NascarAdapter::new()),
        Box::new(PCars2Adapter::new()),
        Box::new(PCars3Adapter::new()),
        Box::new(RBRAdapter::new()),
        Box::new(RaceDriverGridAdapter::new()),
        Box::new(RaceRoomAdapter::new()),
        Box::new(RFactor2Adapter::new()),
        Box::new(RennsportAdapter::new()),
        Box::new(Ride5Adapter::new()),
        Box::new(SebLoebRallyAdapter::new()),
        Box::new(SimHubAdapter::new()),
        Box::new(TrackmaniaAdapter::new()),
        Box::new(VRally4Adapter::new()),
        Box::new(WrcGenerationsAdapter::new()),
        Box::new(WreckfestAdapter::new()),
        Box::new(WtcrAdapter::new()),
    ];

    // Variant-based adapters.
    let variant_adapters: Vec<Box<dyn TelemetryAdapter>> = vec![
        Box::new(Ets2Adapter::with_variant(
            openracing_telemetry_adapters::ets2::Ets2Variant::Ets2,
        )),
        Box::new(Ets2Adapter::with_variant(
            openracing_telemetry_adapters::ets2::Ets2Variant::Ats,
        )),
        Box::new(MudRunnerAdapter::with_variant(
            openracing_telemetry_adapters::mudrunner::MudRunnerVariant::MudRunner,
        )),
        Box::new(MudRunnerAdapter::with_variant(
            openracing_telemetry_adapters::mudrunner::MudRunnerVariant::SnowRunner,
        )),
        Box::new(RFactor1Adapter::with_variant(
            openracing_telemetry_adapters::rfactor1::RFactor1Variant::RFactor1,
        )),
        Box::new(RFactor1Adapter::with_variant(
            openracing_telemetry_adapters::rfactor1::RFactor1Variant::Gtr2,
        )),
        Box::new(WrcKylotonnAdapter::new(
            openracing_telemetry_adapters::wrc_kylotonn::WrcKylotonnVariant::Wrc9,
        )),
    ];

    // Every adapter must handle arbitrary bytes without panicking.
    for adapter in adapters.iter().chain(variant_adapters.iter()) {
        let _ = adapter.normalize(data);
    }
});
