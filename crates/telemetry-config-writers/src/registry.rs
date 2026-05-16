use crate::types::ConfigWriter;
use crate::writers::*;

/// Factory for constructing config writer instances.
pub type ConfigWriterFactory = fn() -> Box<dyn ConfigWriter + Send + Sync>;

fn new_iracing_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(IRacingConfigWriter)
}

fn new_acc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACCConfigWriter)
}

fn new_ac_rally_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACRallyConfigWriter)
}

fn new_ams2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AMS2ConfigWriter)
}

fn new_rfactor2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor2ConfigWriter)
}

fn new_eawrc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(EAWRCConfigWriter)
}

fn new_dirt5_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Dirt5ConfigWriter)
}

fn new_dirt_rally_2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(DirtRally2ConfigWriter)
}

fn new_rbr_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RBRConfigWriter)
}

fn new_gran_turismo_7_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GranTurismo7ConfigWriter)
}

fn new_gran_turismo_sport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GranTurismo7SportsConfigWriter)
}

fn new_f1_manager_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1ManagerConfigWriter)
}

fn new_assetto_corsa_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AssettoCorsaConfigWriter)
}

fn new_forza_motorsport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ForzaMotorsportConfigWriter)
}

fn new_beamng_drive_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(BeamNGDriveConfigWriter)
}

fn new_wrc_generations_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcGenerationsConfigWriter)
}

fn new_dirt4_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Dirt4ConfigWriter)
}

fn new_f1_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1ConfigWriter)
}

fn new_f1_25_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1_25ConfigWriter)
}

fn new_f1_native_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1NativeConfigWriter)
}

fn new_project_cars_2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(PCars2ConfigWriter)
}

fn new_project_cars_3_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(PCars3ConfigWriter)
}

fn new_live_for_speed_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(LFSConfigWriter)
}

fn new_ets2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Ets2ConfigWriter)
}

fn new_ats_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AtsConfigWriter)
}

fn new_wreckfest_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WreckfestConfigWriter)
}

fn new_flatout_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(FlatOutConfigWriter)
}

fn new_dakar_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(DakarDesertRallyConfigWriter)
}

fn new_rennsport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RennsportConfigWriter)
}

fn new_grid_autosport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GridAutosportConfigWriter)
}

fn new_grid_2019_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Grid2019ConfigWriter)
}

fn new_grid_legends_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GridLegendsConfigWriter)
}

fn new_dirt3_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Dirt3ConfigWriter)
}

fn new_race_driver_grid_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RaceDriverGridConfigWriter)
}

fn new_automobilista_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AutomobilistaConfigWriter)
}

fn new_kartkraft_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(KartKraftConfigWriter)
}

fn new_raceroom_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RaceRoomConfigWriter)
}

fn new_nascar_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(NascarConfigWriter)
}

fn new_nascar_21_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Nascar21ConfigWriter)
}

fn new_rfactor1_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter {
        game_id: "rfactor1",
    })
}

fn new_gtr2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "gtr2" })
}

fn new_race_07_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "race_07" })
}

fn new_gsc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "gsc" })
}

fn new_le_mans_ultimate_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(LeMansUltimateConfigWriter)
}

fn new_wtcr_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WtcrConfigWriter)
}

fn new_trackmania_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(TrackmaniaConfigWriter)
}

fn new_simhub_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(SimHubConfigWriter)
}

fn new_mudrunner_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(MudRunnerConfigWriter)
}

fn new_snowrunner_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(SnowRunnerConfigWriter)
}

fn new_motogp_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(MotoGPConfigWriter)
}

fn new_ride5_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Ride5ConfigWriter)
}

fn new_forza_horizon_4_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ForzaHorizon4ConfigWriter)
}

fn new_forza_horizon_5_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ForzaHorizon5ConfigWriter)
}

fn new_wrc_9_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcKylotonnConfigWriter {
        variant: WrcKylotonnVariant::Wrc9,
    })
}

fn new_wrc_10_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcKylotonnConfigWriter {
        variant: WrcKylotonnVariant::Wrc10,
    })
}

fn new_v_rally_4_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(VRally4ConfigWriter)
}

fn new_gravel_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GravelConfigWriter)
}

fn new_seb_loeb_rally_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(SebLoebRallyConfigWriter)
}

fn new_acc2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACC2ConfigWriter)
}

fn new_ac_evo_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACEvoConfigWriter)
}

fn new_dirt_showdown_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(DirtShowdownConfigWriter)
}

/// Returns the canonical config writer registry for all supported integrations.
pub fn config_writer_factories() -> &'static [(&'static str, ConfigWriterFactory)] {
    &[
        ("iracing", new_iracing_config_writer),
        ("acc", new_acc_config_writer),
        ("acc2", new_acc2_config_writer),
        ("ac_evo", new_ac_evo_config_writer),
        ("ac_rally", new_ac_rally_config_writer),
        ("ams2", new_ams2_config_writer),
        ("rfactor2", new_rfactor2_config_writer),
        ("eawrc", new_eawrc_config_writer),
        ("f1", new_f1_config_writer),
        ("f1_25", new_f1_25_config_writer),
        ("f1_native", new_f1_native_config_writer),
        ("dirt5", new_dirt5_config_writer),
        ("dirt_rally_2", new_dirt_rally_2_config_writer),
        ("rbr", new_rbr_config_writer),
        ("gran_turismo_7", new_gran_turismo_7_config_writer),
        ("gran_turismo_sport", new_gran_turismo_sport_config_writer),
        ("f1_manager", new_f1_manager_config_writer),
        ("assetto_corsa", new_assetto_corsa_config_writer),
        ("forza_motorsport", new_forza_motorsport_config_writer),
        ("forza_horizon_4", new_forza_horizon_4_config_writer),
        ("forza_horizon_5", new_forza_horizon_5_config_writer),
        ("beamng_drive", new_beamng_drive_config_writer),
        ("project_cars_2", new_project_cars_2_config_writer),
        ("project_cars_3", new_project_cars_3_config_writer),
        ("live_for_speed", new_live_for_speed_config_writer),
        ("wrc_generations", new_wrc_generations_config_writer),
        ("wrc_9", new_wrc_9_config_writer),
        ("wrc_10", new_wrc_10_config_writer),
        ("v_rally_4", new_v_rally_4_config_writer),
        ("dirt_showdown", new_dirt_showdown_config_writer),
        ("dirt4", new_dirt4_config_writer),
        ("ets2", new_ets2_config_writer),
        ("ats", new_ats_config_writer),
        ("wreckfest", new_wreckfest_config_writer),
        ("flatout", new_flatout_config_writer),
        ("dakar_desert_rally", new_dakar_config_writer),
        ("rennsport", new_rennsport_config_writer),
        ("raceroom", new_raceroom_config_writer),
        ("kartkraft", new_kartkraft_config_writer),
        ("grid_autosport", new_grid_autosport_config_writer),
        ("grid_2019", new_grid_2019_config_writer),
        ("grid_legends", new_grid_legends_config_writer),
        ("dirt3", new_dirt3_config_writer),
        ("race_driver_grid", new_race_driver_grid_config_writer),
        ("automobilista", new_automobilista_config_writer),
        ("nascar", new_nascar_config_writer),
        ("nascar_21", new_nascar_21_config_writer),
        ("le_mans_ultimate", new_le_mans_ultimate_config_writer),
        ("wtcr", new_wtcr_config_writer),
        ("trackmania", new_trackmania_config_writer),
        ("simhub", new_simhub_config_writer),
        ("gravel", new_gravel_config_writer),
        ("seb_loeb_rally", new_seb_loeb_rally_config_writer),
        ("mudrunner", new_mudrunner_config_writer),
        ("snowrunner", new_snowrunner_config_writer),
        ("motogp", new_motogp_config_writer),
        ("ride5", new_ride5_config_writer),
        ("rfactor1", new_rfactor1_config_writer),
        ("gtr2", new_gtr2_config_writer),
        ("race_07", new_race_07_config_writer),
        ("gsc", new_gsc_config_writer),
    ]
}
