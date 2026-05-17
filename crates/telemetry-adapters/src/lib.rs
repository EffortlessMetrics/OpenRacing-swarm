//! Game-specific telemetry adapters.
//!
//! This crate provides the protocol implementations that were formerly embedded inside
//! `racing_wheel_service` while preserving the external adapter trait and types used by
//! higher layers.

#![deny(static_mut_refs)]

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

pub use openracing_telemetry::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};

// Keep these protocol modules first so dependent implementations can import helpers
// via `crate::` paths unchanged from their service-side origins.
pub mod ac_evo;
pub mod ac_rally;
pub mod acc;
pub mod acc2;
pub mod ams2;
pub mod assetto_corsa;
pub mod automobilista;
pub mod beamng;
pub mod codemasters_shared;
pub mod codemasters_udp;
pub mod dakar;
pub mod dirt3;
pub mod dirt4;
pub mod dirt5;
pub mod dirt_rally_2;
pub mod dirt_showdown;
pub mod eawrc;
pub mod ets2;
pub mod f1;
pub mod f1_25;
pub mod f1_manager;
pub mod f1_native;
pub mod flatout;
pub mod forza;
pub mod forza_horizon;
pub mod gran_turismo_7;
pub mod gran_turismo_sport;
pub mod gravel;
pub mod grid_2019;
pub mod grid_autosport;
pub mod grid_legends;
pub mod iracing;
pub mod kartkraft;
pub mod le_mans_ultimate;
pub mod lfs;
pub mod motogp;
pub mod mudrunner;
pub mod nascar;
pub mod nascar_21;
pub mod pcars2;
pub mod pcars3;
pub mod race_driver_grid;
pub mod raceroom;
pub mod rbr;
pub mod rennsport;
pub mod rfactor1;
pub mod rfactor2;
pub mod ride5;
pub mod seb_loeb_rally;
pub mod simhub;
pub mod trackmania;
pub mod v_rally_4;
pub mod wrc_generations;
pub mod wrc_kylotonn;
pub mod wreckfest;
pub mod wtcr;

/// Stable namespace for game-specific telemetry adapters.
///
/// The root modules remain available during the transition, while new callers
/// should prefer these grouped paths.
pub mod games {
    pub use crate::ams2;
    pub use crate::kartkraft;
    pub use crate::lfs as live_for_speed;
    pub use crate::mudrunner;
    pub use crate::raceroom;
    pub use crate::rennsport;
    pub use crate::simhub;
    pub use crate::wrc_generations;

    pub mod f1 {
        pub use crate::f1::F1Adapter;
        pub use crate::f1_25::F1_25Adapter;
        pub use crate::f1_manager::F1ManagerAdapter;
        pub use crate::f1_native::F1NativeAdapter;
    }

    pub mod forza {
        pub use crate::forza::ForzaAdapter;
        pub use crate::forza_horizon::{ForzaHorizon4Adapter, ForzaHorizon5Adapter};
    }
}

#[cfg(test)]
mod normalization_tests;

/// Shared type alias for outbound telemetry streams.
pub type TelemetryReceiver = mpsc::Receiver<TelemetryFrame>;

static TELEMETRY_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Return a monotonic timestamp in nanoseconds using a process-wide epoch.
pub fn telemetry_now_ns() -> u64 {
    let epoch = TELEMETRY_EPOCH.get_or_init(Instant::now);
    Instant::now()
        .checked_duration_since(*epoch)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
}

/// Telemetry adapter trait for game-specific telemetry sources.
#[async_trait]
pub trait TelemetryAdapter: Send + Sync {
    /// Get the game identifier this adapter supports.
    fn game_id(&self) -> &str;

    /// Start monitoring telemetry from the game.
    async fn start_monitoring(&self) -> Result<TelemetryReceiver>;

    /// Stop monitoring telemetry.
    async fn stop_monitoring(&self) -> Result<()>;

    /// Normalize raw telemetry data to common format.
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry>;

    /// Expected update rate for this adapter.
    fn expected_update_rate(&self) -> Duration;

    /// Check if the game is currently running.
    async fn is_game_running(&self) -> Result<bool>;
}

/// Factory for constructing adapter instances.
pub type AdapterFactory = fn() -> Box<dyn TelemetryAdapter>;

fn new_ac_rally_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ACRallyAdapter::new())
}

fn new_acc_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ACCAdapter::new())
}

fn new_ams2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(AMS2Adapter::new())
}

fn new_assetto_corsa_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(AssettoCorsaAdapter::new())
}

fn new_beamng_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(BeamNGAdapter::new())
}

fn new_forza_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ForzaAdapter::new())
}

fn new_gran_turismo_7_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GranTurismo7Adapter::new())
}

fn new_gran_turismo_sport_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(gran_turismo_sport::GranTurismo7SportsAdapter::new())
}

fn new_iracing_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(IRacingAdapter::new())
}

fn new_kartkraft_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(KartKraftAdapter::new())
}

fn new_lfs_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(LFSAdapter::new())
}

fn new_pcars2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(PCars2Adapter::new())
}

fn new_pcars3_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(PCars3Adapter::new())
}

fn new_raceroom_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RaceRoomAdapter::new())
}

fn new_rbr_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RBRAdapter::new())
}

fn new_rfactor1_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor1Adapter::with_variant(
        rfactor1::RFactor1Variant::RFactor1,
    ))
}

fn new_gtr2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor1Adapter::with_variant(
        rfactor1::RFactor1Variant::Gtr2,
    ))
}

fn new_race07_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor1Adapter::with_variant(
        rfactor1::RFactor1Variant::Race07,
    ))
}

fn new_gsc_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor1Adapter::with_variant(
        rfactor1::RFactor1Variant::GameStockCar,
    ))
}

fn new_rfactor2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor2Adapter::new())
}

fn new_eawrc_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(EAWRCAdapter::new())
}

fn new_dirt5_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Dirt5Adapter::new())
}

fn new_dirt_rally_2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(DirtRally2Adapter::new())
}

fn new_f1_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(F1Adapter::new())
}

fn new_f1_25_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(F1_25Adapter::new())
}

fn new_wrc_generations_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WrcGenerationsAdapter::new())
}

fn new_dirt4_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Dirt4Adapter::new())
}

fn new_dirt3_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Dirt3Adapter::new())
}

fn new_ets2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Ets2Adapter::with_variant(ets2::Ets2Variant::Ets2))
}

fn new_ats_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Ets2Adapter::with_variant(ets2::Ets2Variant::Ats))
}

fn new_wreckfest_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WreckfestAdapter::new())
}

fn new_automobilista_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Automobilista1Adapter::new())
}

fn new_grid_autosport_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GridAutosportAdapter::new())
}

fn new_grid_2019_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Grid2019Adapter::new())
}

fn new_grid_legends_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GridLegendsAdapter::new())
}

fn new_race_driver_grid_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RaceDriverGridAdapter::new())
}

fn new_rennsport_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RennsportAdapter::new())
}

fn new_nascar_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(NascarAdapter::new())
}

fn new_nascar_21_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(nascar_21::Nascar21Adapter::new())
}

fn new_f1_manager_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(f1_manager::F1ManagerAdapter::new())
}

fn new_le_mans_ultimate_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(LeMansUltimateAdapter::new())
}

fn new_wtcr_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WtcrAdapter::new())
}

fn new_trackmania_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(TrackmaniaAdapter::new())
}

fn new_simhub_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(SimHubAdapter::new())
}

fn new_mudrunner_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(mudrunner::MudRunnerAdapter::with_variant(
        mudrunner::MudRunnerVariant::MudRunner,
    ))
}

fn new_snowrunner_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(mudrunner::MudRunnerAdapter::with_variant(
        mudrunner::MudRunnerVariant::SnowRunner,
    ))
}

fn new_dakar_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(DakarDesertRallyAdapter::new())
}

fn new_flatout_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(flatout::FlatOutAdapter::new())
}

fn new_motogp_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(motogp::MotoGPAdapter::new())
}

fn new_ride5_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ride5::Ride5Adapter::new())
}

fn new_f1_native_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(F1NativeAdapter::new())
}

fn new_acc2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(acc2::ACC2Adapter::new())
}

fn new_ac_evo_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ac_evo::ACEvoAdapter::new())
}

fn new_forza_horizon_4_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(forza_horizon::ForzaHorizon4Adapter::new())
}

fn new_forza_horizon_5_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(forza_horizon::ForzaHorizon5Adapter::new())
}

fn new_v_rally_4_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(v_rally_4::VRally4Adapter::new())
}

fn new_gravel_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(gravel::GravelAdapter::new())
}

fn new_seb_loeb_rally_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(seb_loeb_rally::SebLoebRallyAdapter::new())
}

fn new_dirt_showdown_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(dirt_showdown::DirtShowdownAdapter::new())
}

fn new_wrc_9_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(wrc_kylotonn::WrcKylotonnAdapter::new(
        wrc_kylotonn::WrcKylotonnVariant::Wrc9,
    ))
}

fn new_wrc_10_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(wrc_kylotonn::WrcKylotonnAdapter::new(
        wrc_kylotonn::WrcKylotonnVariant::Wrc10,
    ))
}

/// Returns the canonical adapter factory registry for all supported native adapters.
pub fn adapter_factories() -> &'static [(&'static str, AdapterFactory)] {
    &[
        ("acc", new_acc_adapter),
        ("acc2", new_acc2_adapter),
        ("ac_evo", new_ac_evo_adapter),
        ("ac_rally", new_ac_rally_adapter),
        ("ams2", new_ams2_adapter),
        ("assetto_corsa", new_assetto_corsa_adapter),
        ("ats", new_ats_adapter),
        ("beamng_drive", new_beamng_adapter),
        ("dirt5", new_dirt5_adapter),
        ("dirt_rally_2", new_dirt_rally_2_adapter),
        ("dirt4", new_dirt4_adapter),
        ("dirt3", new_dirt3_adapter),
        ("dirt_showdown", new_dirt_showdown_adapter),
        ("eawrc", new_eawrc_adapter),
        ("ets2", new_ets2_adapter),
        ("f1", new_f1_adapter),
        ("f1_25", new_f1_25_adapter),
        ("forza_motorsport", new_forza_adapter),
        ("forza_horizon_4", new_forza_horizon_4_adapter),
        ("forza_horizon_5", new_forza_horizon_5_adapter),
        ("gran_turismo_7", new_gran_turismo_7_adapter),
        ("gran_turismo_sport", new_gran_turismo_sport_adapter),
        ("f1_manager", new_f1_manager_adapter),
        ("iracing", new_iracing_adapter),
        ("kartkraft", new_kartkraft_adapter),
        ("live_for_speed", new_lfs_adapter),
        ("project_cars_2", new_pcars2_adapter),
        ("project_cars_3", new_pcars3_adapter),
        ("raceroom", new_raceroom_adapter),
        ("rbr", new_rbr_adapter),
        ("automobilista", new_automobilista_adapter),
        ("grid_autosport", new_grid_autosport_adapter),
        ("grid_2019", new_grid_2019_adapter),
        ("grid_legends", new_grid_legends_adapter),
        ("race_driver_grid", new_race_driver_grid_adapter),
        ("rennsport", new_rennsport_adapter),
        ("rfactor1", new_rfactor1_adapter),
        ("gtr2", new_gtr2_adapter),
        ("race_07", new_race07_adapter),
        ("gsc", new_gsc_adapter),
        ("rfactor2", new_rfactor2_adapter),
        ("wrc_generations", new_wrc_generations_adapter),
        ("wrc_9", new_wrc_9_adapter),
        ("wrc_10", new_wrc_10_adapter),
        ("v_rally_4", new_v_rally_4_adapter),
        ("gravel", new_gravel_adapter),
        ("seb_loeb_rally", new_seb_loeb_rally_adapter),
        ("wreckfest", new_wreckfest_adapter),
        ("nascar", new_nascar_adapter),
        ("nascar_21", new_nascar_21_adapter),
        ("le_mans_ultimate", new_le_mans_ultimate_adapter),
        ("wtcr", new_wtcr_adapter),
        ("trackmania", new_trackmania_adapter),
        ("dakar_desert_rally", new_dakar_adapter),
        ("flatout", new_flatout_adapter),
        ("simhub", new_simhub_adapter),
        ("mudrunner", new_mudrunner_adapter),
        ("snowrunner", new_snowrunner_adapter),
        ("motogp", new_motogp_adapter),
        ("ride5", new_ride5_adapter),
        ("f1_native", new_f1_native_adapter),
    ]
}

pub use ac_evo::ACEvoAdapter;
pub use ac_rally::ACRallyAdapter;
pub use acc::ACCAdapter;
pub use acc2::ACC2Adapter;
pub use ams2::AMS2Adapter;
pub use assetto_corsa::AssettoCorsaAdapter;
pub use automobilista::Automobilista1Adapter;
pub use beamng::BeamNGAdapter;
pub use codemasters_udp::{CustomUdpSpec, DecodedCodemastersPacket, FieldSpec};
pub use dakar::DakarDesertRallyAdapter;
pub use dirt_rally_2::DirtRally2Adapter;
pub use dirt_showdown::DirtShowdownAdapter;
pub use dirt3::Dirt3Adapter;
pub use dirt4::Dirt4Adapter;
pub use dirt5::Dirt5Adapter;
pub use eawrc::EAWRCAdapter;
pub use ets2::Ets2Adapter;
pub use f1::F1Adapter;
pub use f1_25::F1_25Adapter;
pub use f1_manager::F1ManagerAdapter;
pub use f1_native::F1NativeAdapter;
pub use flatout::FlatOutAdapter;
pub use forza::ForzaAdapter;
pub use forza_horizon::{ForzaHorizon4Adapter, ForzaHorizon5Adapter};
pub use gran_turismo_7::GranTurismo7Adapter;
pub use gran_turismo_sport::GranTurismo7SportsAdapter;
pub use gravel::GravelAdapter;
pub use grid_2019::Grid2019Adapter;
pub use grid_autosport::GridAutosportAdapter;
pub use grid_legends::GridLegendsAdapter;
pub use iracing::IRacingAdapter;
pub use kartkraft::KartKraftAdapter;
pub use le_mans_ultimate::LeMansUltimateAdapter;
pub use lfs::LFSAdapter;
pub use motogp::MotoGPAdapter;
pub use mudrunner::MudRunnerAdapter;
pub use nascar::NascarAdapter;
pub use nascar_21::Nascar21Adapter;
pub use pcars2::PCars2Adapter;
pub use pcars3::PCars3Adapter;
pub use race_driver_grid::RaceDriverGridAdapter;
pub use raceroom::RaceRoomAdapter;
pub use rbr::RBRAdapter;
pub use rennsport::RennsportAdapter;
pub use rfactor1::RFactor1Adapter;
pub use rfactor2::RFactor2Adapter;
pub use ride5::Ride5Adapter;
pub use seb_loeb_rally::SebLoebRallyAdapter;
pub use simhub::SimHubAdapter;
pub use trackmania::TrackmaniaAdapter;
pub use v_rally_4::VRally4Adapter;
pub use wrc_generations::WrcGenerationsAdapter;
pub use wrc_kylotonn::WrcKylotonnAdapter;
pub use wreckfest::WreckfestAdapter;
pub use wtcr::WtcrAdapter;

/// Mock adapter for testing and deterministic fixture generation.
pub struct MockAdapter {
    game_id: String,
    update_rate: Duration,
    is_running: bool,
}

impl MockAdapter {
    pub fn new(game_id: String) -> Self {
        Self {
            game_id,
            update_rate: Duration::from_millis(16),
            is_running: false,
        }
    }

    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }
}

#[async_trait]
impl TelemetryAdapter for MockAdapter {
    fn game_id(&self) -> &str {
        &self.game_id
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut frame_seq = 0u64;

            loop {
                let timestamp_ns = telemetry_now_ns();
                let elapsed = std::time::Duration::from_nanos(timestamp_ns);
                let progress = (elapsed.as_secs_f32() % 10.0) / 10.0;
                let telemetry = generate_mock_telemetry(progress);

                let frame = TelemetryFrame::new(telemetry, timestamp_ns, frame_seq, 64);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_seq += 1;
                tokio::time::sleep(update_rate).await;
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::builder().rpm(5000.0).build())
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_running)
    }
}

fn generate_mock_telemetry(progress: f32) -> NormalizedTelemetry {
    use std::f32::consts::PI;

    let rpm = 4000.0 + (progress * 2.0 * PI).sin() * 2000.0;
    let speed = 30.0 + progress * 40.0;
    let ffb_scalar = (progress * 4.0 * PI).sin() * 0.7;
    let slip_ratio = ((progress * 8.0 * PI).sin().abs() * 0.2).min(1.0);
    let gear = match speed {
        s if s < 20.0 => 2,
        s if s < 35.0 => 3,
        s if s < 50.0 => 4,
        s if s < 65.0 => 5,
        _ => 6,
    };

    NormalizedTelemetry::builder()
        .ffb_scalar(ffb_scalar)
        .rpm(rpm.max(0.0))
        .speed_ms(speed)
        .slip_ratio(slip_ratio)
        .gear(gear)
        .car_id("mock_car".to_string())
        .track_id("mock_track".to_string())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn test_mock_adapter() -> TestResult {
        let adapter = MockAdapter::new("test_game".to_string());

        assert_eq!(adapter.game_id(), "test_game");
        let is_running = adapter.is_game_running().await?;
        assert!(!is_running);

        let mut receiver = adapter.start_monitoring().await?;
        let frame = tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("expected telemetry frame")?;

        assert!(frame.data.rpm > 0.0);
        assert!(frame.data.speed_ms > 0.0);
        assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
        Ok(())
    }

    #[test]
    fn test_mock_telemetry_generation() -> TestResult {
        let telemetry = generate_mock_telemetry(0.5);

        assert!(telemetry.rpm > 0.0);
        assert!(telemetry.speed_ms > 0.0);
        assert!(telemetry.ffb_scalar > 0.0);
        assert!(telemetry.slip_ratio >= 0.0);
        Ok(())
    }

    #[test]
    fn test_mock_telemetry_at_zero_progress() -> TestResult {
        let telemetry = generate_mock_telemetry(0.0);
        assert!(telemetry.rpm >= 0.0);
        assert!(telemetry.speed_ms >= 0.0);
        assert!(telemetry.slip_ratio >= 0.0 && telemetry.slip_ratio <= 1.0);
        Ok(())
    }

    #[test]
    fn test_mock_telemetry_at_full_progress() -> TestResult {
        let telemetry = generate_mock_telemetry(1.0);
        assert!(telemetry.rpm >= 0.0);
        assert!(telemetry.speed_ms >= 0.0);
        Ok(())
    }

    #[test]
    fn test_mock_adapter_set_running() -> TestResult {
        let mut adapter = MockAdapter::new("test".to_string());
        assert!(!adapter.is_running);

        adapter.set_running(true);
        assert!(adapter.is_running);
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_adapter_is_game_running_when_set() -> TestResult {
        let mut adapter = MockAdapter::new("test".to_string());
        adapter.set_running(true);
        let running = adapter.is_game_running().await?;
        assert!(running);
        Ok(())
    }

    #[test]
    fn test_mock_adapter_normalize() -> TestResult {
        let adapter = MockAdapter::new("test".to_string());
        let result = adapter.normalize(&[])?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_mock_adapter_update_rate() {
        let adapter = MockAdapter::new("test".to_string());
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[tokio::test]
    async fn test_mock_adapter_stop_monitoring() -> TestResult {
        let adapter = MockAdapter::new("test".to_string());
        adapter.stop_monitoring().await?;
        Ok(())
    }

    // ── Adapter registry tests ────────────────────────────────────────────

    #[test]
    fn test_adapter_factories_non_empty() {
        let factories = adapter_factories();
        assert!(
            factories.len() >= 50,
            "Expected at least 50 adapters, got {}",
            factories.len()
        );
    }

    #[test]
    fn test_adapter_factories_unique_game_ids() -> TestResult {
        let factories = adapter_factories();
        let mut seen = HashSet::new();
        for (id, _) in factories {
            assert!(
                seen.insert(*id),
                "Duplicate game_id in adapter_factories: {id}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_adapter_factories_all_constructible() -> TestResult {
        let factories = adapter_factories();
        for (id, factory) in factories {
            let adapter = factory();
            assert_eq!(
                adapter.game_id(),
                *id,
                "Factory for '{id}' produced adapter with game_id '{}'",
                adapter.game_id()
            );
        }
        Ok(())
    }

    #[test]
    fn test_adapter_factories_known_games_present() -> TestResult {
        let factories = adapter_factories();
        let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();

        let expected = [
            "acc",
            "forza_motorsport",
            "iracing",
            "f1",
            "eawrc",
            "rfactor2",
            "raceroom",
            "beamng_drive",
        ];

        for game in &expected {
            assert!(
                ids.contains(game),
                "Expected game '{game}' not found in adapter_factories"
            );
        }
        Ok(())
    }

    #[test]
    fn test_adapter_factories_forza_horizon_variants() -> TestResult {
        let factories = adapter_factories();
        let ids: HashSet<&str> = factories.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains("forza_horizon_4"));
        assert!(ids.contains("forza_horizon_5"));
        Ok(())
    }

    #[test]
    fn test_adapter_factories_update_rates_valid() -> TestResult {
        let factories = adapter_factories();
        for (id, factory) in factories {
            let adapter = factory();
            let rate = adapter.expected_update_rate();
            assert!(
                rate.as_millis() > 0 && rate.as_millis() <= 1000,
                "Adapter '{id}' has suspicious update rate: {rate:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_telemetry_now_ns_returns_value() {
        let ts = telemetry_now_ns();
        // Should be a small value since epoch was just initialized
        assert!(ts < 60_000_000_000); // less than 60 seconds
    }
}
