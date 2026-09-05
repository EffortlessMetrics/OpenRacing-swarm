//! Field-sensitivity contracts for pipeline configuration hashes.

use openracing_curves::CurveType;
use openracing_pipeline::{calculate_config_hash, calculate_config_hash_with_curve};
use racing_wheel_schemas::entities::{BumpstopConfig, FilterConfig, HandsOffConfig};
use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn config() -> Result<FilterConfig, Box<dyn Error>> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.10)?,
        Gain::new(0.15)?,
        Gain::new(0.05)?,
        vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        Gain::new(0.80)?,
        vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.6)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        Gain::new(0.90)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

fn assert_hash_changes(base_hash: u64, changed: &FilterConfig, field: &str) {
    let changed_hash = calculate_config_hash(changed);
    assert_ne!(base_hash, changed_hash, "changing {field} did not change hash");
    assert_eq!(
        changed_hash,
        calculate_config_hash(changed),
        "changed {field} hash was not deterministic"
    );
}

#[test]
fn scalar_filter_fields_change_the_hash() -> TestResult {
    let base = config()?;
    let base_hash = calculate_config_hash(&base);

    let mut reconstruction = base.clone();
    reconstruction.reconstruction = 5;
    assert_hash_changes(base_hash, &reconstruction, "reconstruction");

    let mut friction = base.clone();
    friction.friction = Gain::new(0.11)?;
    assert_hash_changes(base_hash, &friction, "friction");

    let mut damper = base.clone();
    damper.damper = Gain::new(0.16)?;
    assert_hash_changes(base_hash, &damper, "damper");

    let mut inertia = base.clone();
    inertia.inertia = Gain::new(0.06)?;
    assert_hash_changes(base_hash, &inertia, "inertia");

    let mut slew_rate = base.clone();
    slew_rate.slew_rate = Gain::new(0.70)?;
    assert_hash_changes(base_hash, &slew_rate, "slew rate");

    let mut torque_cap = base;
    torque_cap.torque_cap = Gain::new(0.85)?;
    assert_hash_changes(base_hash, &torque_cap, "torque cap");
    Ok(())
}

#[test]
fn curve_and_notch_fields_change_the_hash() -> TestResult {
    let base = config()?;
    let base_hash = calculate_config_hash(&base);

    let mut curve = base.clone();
    curve.curve_points[1] = CurvePoint::new(0.5, 0.7)?;
    assert_hash_changes(base_hash, &curve, "curve points");

    let mut frequency = base.clone();
    frequency.notch_filters[0].frequency = FrequencyHz::new(61.0)?;
    assert_hash_changes(base_hash, &frequency, "notch frequency");

    let mut q_factor = base.clone();
    q_factor.notch_filters[0].q_factor = 2.5;
    assert_hash_changes(base_hash, &q_factor, "notch Q factor");

    let mut gain = base;
    gain.notch_filters[0].gain_db = -10.0;
    assert_hash_changes(base_hash, &gain, "notch gain");
    Ok(())
}

#[test]
fn safety_envelope_fields_change_the_hash() -> TestResult {
    let base = config()?;
    let base_hash = calculate_config_hash(&base);

    let mut bumpstop_enabled = base.clone();
    bumpstop_enabled.bumpstop.enabled = false;
    assert_hash_changes(base_hash, &bumpstop_enabled, "bumpstop enabled");

    let mut bumpstop_start = base.clone();
    bumpstop_start.bumpstop.start_angle = 455.0;
    assert_hash_changes(base_hash, &bumpstop_start, "bumpstop start angle");

    let mut bumpstop_max = base.clone();
    bumpstop_max.bumpstop.max_angle = 545.0;
    assert_hash_changes(base_hash, &bumpstop_max, "bumpstop max angle");

    let mut bumpstop_stiffness = base.clone();
    bumpstop_stiffness.bumpstop.stiffness = 0.7;
    assert_hash_changes(base_hash, &bumpstop_stiffness, "bumpstop stiffness");

    let mut bumpstop_damping = base.clone();
    bumpstop_damping.bumpstop.damping = 0.4;
    assert_hash_changes(base_hash, &bumpstop_damping, "bumpstop damping");

    let mut hands_off_enabled = base.clone();
    hands_off_enabled.hands_off.enabled = false;
    assert_hash_changes(base_hash, &hands_off_enabled, "hands-off enabled");

    let mut hands_off_threshold = base.clone();
    hands_off_threshold.hands_off.threshold = 0.07;
    assert_hash_changes(base_hash, &hands_off_threshold, "hands-off threshold");

    let mut hands_off_timeout = base;
    hands_off_timeout.hands_off.timeout_seconds = 4.0;
    assert_hash_changes(base_hash, &hands_off_timeout, "hands-off timeout");
    Ok(())
}

#[test]
fn response_curve_presence_and_parameters_change_the_hash() -> TestResult {
    let config = config()?;
    let exponential = CurveType::exponential(2.0)?;

    let without_curve = calculate_config_hash_with_curve(&config, None);
    let linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
    let exponential_hash = calculate_config_hash_with_curve(&config, Some(&exponential));

    assert_ne!(without_curve, linear);
    assert_ne!(without_curve, exponential_hash);
    assert_ne!(linear, exponential_hash);
    assert_eq!(
        exponential_hash,
        calculate_config_hash_with_curve(&config, Some(&exponential))
    );
    Ok(())
}
