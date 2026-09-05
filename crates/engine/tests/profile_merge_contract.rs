//! Public hierarchy contracts for deterministic profile merging.

use racing_wheel_engine::profile_merge::ProfileMergeEngine;
use racing_wheel_schemas::prelude::*;
use std::error::Error;
use std::fmt::Debug;
use std::io;

type TestResult = Result<(), Box<dyn Error>>;

fn checked<T, E: Debug>(result: Result<T, E>) -> Result<T, Box<dyn Error>> {
    result.map_err(|error| io::Error::other(format!("invalid test fixture: {error:?}")).into())
}

fn profile(id: &str, scope: ProfileScope) -> Result<Profile, Box<dyn Error>> {
    Ok(Profile::new(
        checked(id.parse::<ProfileId>())?,
        scope,
        BaseSettings::default(),
        format!("Test profile {id}"),
    ))
}

#[test]
fn default_child_scalars_do_not_erase_parent_overrides() -> TestResult {
    let engine = ProfileMergeEngine::new();
    let mut global = profile("global", ProfileScope::global())?;
    let mut game = profile(
        "iracing",
        ProfileScope::for_game("iracing".to_string()),
    )?;
    let car = profile(
        "gt3",
        ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
    )?;

    global.base_settings.ffb_gain = checked(Gain::new(0.55))?;
    game.base_settings.ffb_gain = checked(Gain::new(0.82))?;
    game.base_settings.degrees_of_rotation = checked(Degrees::new_dor(540.0))?;
    game.base_settings.torque_cap = checked(TorqueNm::new(18.0))?;

    let merged = engine.merge_profiles(&global, Some(&game), Some(&car), None);

    assert_eq!(merged.profile.base_settings.ffb_gain.value(), 0.82);
    assert_eq!(
        merged.profile.base_settings.degrees_of_rotation.value(),
        540.0
    );
    assert_eq!(merged.profile.base_settings.torque_cap.value(), 18.0);
    Ok(())
}

#[test]
fn child_filter_config_replaces_parent_filter_config() -> TestResult {
    let engine = ProfileMergeEngine::new();
    let mut global = profile("global", ProfileScope::global())?;
    let mut game = profile(
        "iracing",
        ProfileScope::for_game("iracing".to_string()),
    )?;

    global.base_settings.filters.reconstruction = 8;
    global.base_settings.filters.friction = checked(Gain::new(0.4))?;
    global.base_settings.filters.damper = checked(Gain::new(0.5))?;
    global.base_settings.filters.inertia = checked(Gain::new(0.6))?;
    global.base_settings.filters.slew_rate = checked(Gain::new(0.7))?;
    global.base_settings.filters.curve_points = vec![
        checked(CurvePoint::new(0.0, 0.0))?,
        checked(CurvePoint::new(0.4, 0.2))?,
        checked(CurvePoint::new(1.0, 1.0))?,
    ];

    game.base_settings.filters.reconstruction = 2;
    game.base_settings.filters.friction = checked(Gain::new(0.1))?;
    game.base_settings.filters.damper = checked(Gain::new(0.2))?;
    game.base_settings.filters.inertia = checked(Gain::new(0.3))?;
    game.base_settings.filters.slew_rate = checked(Gain::new(0.9))?;
    game.base_settings.filters.curve_points = vec![
        checked(CurvePoint::new(0.0, 0.0))?,
        checked(CurvePoint::new(0.6, 0.8))?,
        checked(CurvePoint::new(1.0, 1.0))?,
    ];

    let merged = engine.merge_profiles(&global, Some(&game), None, None);
    let filters = &merged.profile.base_settings.filters;

    assert_eq!(filters.reconstruction, 2);
    assert_eq!(filters.friction.value(), 0.1);
    assert_eq!(filters.damper.value(), 0.2);
    assert_eq!(filters.inertia.value(), 0.3);
    assert_eq!(filters.slew_rate.value(), 0.9);
    assert_eq!(filters.curve_points, game.base_settings.filters.curve_points);
    Ok(())
}

#[test]
fn absent_led_and_haptics_configs_preserve_parent_values() -> TestResult {
    let engine = ProfileMergeEngine::new();
    let mut global = profile("global", ProfileScope::global())?;
    let mut game = profile(
        "iracing",
        ProfileScope::for_game("iracing".to_string()),
    )?;
    game.led_config = None;
    game.haptics_config = None;

    let led = LedConfig {
        pattern: "global-progressive".to_string(),
        ..LedConfig::default()
    };
    let haptics = HapticsConfig {
        enabled: false,
        ..HapticsConfig::default()
    };
    global.led_config = Some(led.clone());
    global.haptics_config = Some(haptics.clone());

    let merged = engine.merge_profiles(&global, Some(&game), None, None);

    assert_eq!(merged.stats.led_overrides, 0);
    assert_eq!(merged.stats.haptics_overrides, 0);
    assert_eq!(merged.profile.led_config, Some(led));
    assert_eq!(merged.profile.haptics_config, Some(haptics));
    Ok(())
}

#[test]
fn most_specific_led_and_haptics_configs_win() -> TestResult {
    let engine = ProfileMergeEngine::new();
    let mut global = profile("global", ProfileScope::global())?;
    let mut game = profile(
        "iracing",
        ProfileScope::for_game("iracing".to_string()),
    )?;
    let mut car = profile(
        "gt3",
        ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
    )?;

    global.led_config = Some(LedConfig {
        pattern: "global".to_string(),
        ..LedConfig::default()
    });
    game.led_config = Some(LedConfig {
        pattern: "game".to_string(),
        ..LedConfig::default()
    });
    let car_led = LedConfig {
        pattern: "car".to_string(),
        ..LedConfig::default()
    };
    car.led_config = Some(car_led.clone());

    global.haptics_config = Some(HapticsConfig {
        enabled: false,
        ..HapticsConfig::default()
    });
    game.haptics_config = Some(HapticsConfig {
        intensity: checked(Gain::new(0.25))?,
        ..HapticsConfig::default()
    });
    let car_haptics = HapticsConfig {
        intensity: checked(Gain::new(0.9))?,
        ..HapticsConfig::default()
    };
    car.haptics_config = Some(car_haptics.clone());

    let merged = engine.merge_profiles(&global, Some(&game), Some(&car), None);

    assert_eq!(merged.stats.led_overrides, 2);
    assert_eq!(merged.stats.haptics_overrides, 2);
    assert_eq!(merged.profile.led_config, Some(car_led));
    assert_eq!(merged.profile.haptics_config, Some(car_haptics));
    Ok(())
}
