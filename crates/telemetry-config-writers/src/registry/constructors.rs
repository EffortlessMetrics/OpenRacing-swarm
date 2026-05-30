use crate::types::ConfigWriter;
use crate::writers::*;

macro_rules! new_writer_constructor {
    ($fn_name:ident, $writer_ty:ty) => {
        pub(super) fn $fn_name() -> Box<dyn ConfigWriter + Send + Sync> {
            Box::new(<$writer_ty>::default())
        }
    };
}

new_writer_constructor!(new_iracing_config_writer, IRacingConfigWriter);
new_writer_constructor!(new_acc_config_writer, ACCConfigWriter);
new_writer_constructor!(new_ac_rally_config_writer, ACRallyConfigWriter);
new_writer_constructor!(new_ams2_config_writer, AMS2ConfigWriter);
new_writer_constructor!(new_rfactor2_config_writer, RFactor2ConfigWriter);
new_writer_constructor!(new_eawrc_config_writer, EAWRCConfigWriter);
new_writer_constructor!(new_dirt5_config_writer, Dirt5ConfigWriter);
new_writer_constructor!(new_dirt_rally_2_config_writer, DirtRally2ConfigWriter);
new_writer_constructor!(new_rbr_config_writer, RBRConfigWriter);
new_writer_constructor!(new_gran_turismo_7_config_writer, GranTurismo7ConfigWriter);
new_writer_constructor!(
    new_gran_turismo_sport_config_writer,
    GranTurismo7SportsConfigWriter
);
new_writer_constructor!(new_f1_manager_config_writer, F1ManagerConfigWriter);
new_writer_constructor!(new_assetto_corsa_config_writer, AssettoCorsaConfigWriter);
new_writer_constructor!(
    new_forza_motorsport_config_writer,
    ForzaMotorsportConfigWriter
);
new_writer_constructor!(new_beamng_drive_config_writer, BeamNGDriveConfigWriter);
new_writer_constructor!(
    new_wrc_generations_config_writer,
    WrcGenerationsConfigWriter
);
new_writer_constructor!(new_dirt4_config_writer, Dirt4ConfigWriter);
new_writer_constructor!(new_f1_config_writer, F1ConfigWriter);
new_writer_constructor!(new_f1_25_config_writer, F1_25ConfigWriter);
new_writer_constructor!(new_f1_native_config_writer, F1NativeConfigWriter);
new_writer_constructor!(new_project_cars_2_config_writer, PCars2ConfigWriter);
new_writer_constructor!(new_project_cars_3_config_writer, PCars3ConfigWriter);
new_writer_constructor!(new_live_for_speed_config_writer, LFSConfigWriter);
new_writer_constructor!(new_ets2_config_writer, Ets2ConfigWriter);
new_writer_constructor!(new_ats_config_writer, AtsConfigWriter);
new_writer_constructor!(new_wreckfest_config_writer, WreckfestConfigWriter);
new_writer_constructor!(new_flatout_config_writer, FlatOutConfigWriter);
new_writer_constructor!(new_dakar_config_writer, DakarDesertRallyConfigWriter);
new_writer_constructor!(new_rennsport_config_writer, RennsportConfigWriter);
new_writer_constructor!(new_grid_autosport_config_writer, GridAutosportConfigWriter);
new_writer_constructor!(new_grid_2019_config_writer, Grid2019ConfigWriter);
new_writer_constructor!(new_grid_legends_config_writer, GridLegendsConfigWriter);
new_writer_constructor!(new_dirt3_config_writer, Dirt3ConfigWriter);
new_writer_constructor!(
    new_race_driver_grid_config_writer,
    RaceDriverGridConfigWriter
);
new_writer_constructor!(new_automobilista_config_writer, AutomobilistaConfigWriter);
new_writer_constructor!(new_kartkraft_config_writer, KartKraftConfigWriter);
new_writer_constructor!(new_raceroom_config_writer, RaceRoomConfigWriter);
new_writer_constructor!(new_nascar_config_writer, NascarConfigWriter);
new_writer_constructor!(new_nascar_21_config_writer, Nascar21ConfigWriter);
new_writer_constructor!(
    new_le_mans_ultimate_config_writer,
    LeMansUltimateConfigWriter
);
new_writer_constructor!(new_wtcr_config_writer, WtcrConfigWriter);
new_writer_constructor!(new_trackmania_config_writer, TrackmaniaConfigWriter);
new_writer_constructor!(new_simhub_config_writer, SimHubConfigWriter);
new_writer_constructor!(new_mudrunner_config_writer, MudRunnerConfigWriter);
new_writer_constructor!(new_snowrunner_config_writer, SnowRunnerConfigWriter);
new_writer_constructor!(new_motogp_config_writer, MotoGPConfigWriter);
new_writer_constructor!(new_ride5_config_writer, Ride5ConfigWriter);
new_writer_constructor!(new_forza_horizon_4_config_writer, ForzaHorizon4ConfigWriter);
new_writer_constructor!(new_forza_horizon_5_config_writer, ForzaHorizon5ConfigWriter);
new_writer_constructor!(new_v_rally_4_config_writer, VRally4ConfigWriter);
new_writer_constructor!(new_gravel_config_writer, GravelConfigWriter);
new_writer_constructor!(new_seb_loeb_rally_config_writer, SebLoebRallyConfigWriter);
new_writer_constructor!(new_acc2_config_writer, ACC2ConfigWriter);
new_writer_constructor!(new_ac_evo_config_writer, ACEvoConfigWriter);
new_writer_constructor!(new_dirt_showdown_config_writer, DirtShowdownConfigWriter);

pub(super) fn new_rfactor1_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter {
        game_id: "rfactor1",
    })
}

pub(super) fn new_gtr2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "gtr2" })
}

pub(super) fn new_race_07_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "race_07" })
}

pub(super) fn new_gsc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor1ConfigWriter { game_id: "gsc" })
}

pub(super) fn new_wrc_9_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcKylotonnConfigWriter {
        variant: WrcKylotonnVariant::Wrc9,
    })
}

pub(super) fn new_wrc_10_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcKylotonnConfigWriter {
        variant: WrcKylotonnVariant::Wrc10,
    })
}
