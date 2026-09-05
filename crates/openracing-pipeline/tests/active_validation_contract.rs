//! Public validation contracts for active pipeline controls.

use openracing_pipeline::{PipelineError, PipelineValidator};
use racing_wheel_schemas::entities::{BumpstopConfig, FilterConfig, HandsOffConfig};
use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};
use std::error::Error;
use std::io;

type TestResult = Result<(), Box<dyn Error>>;

fn valid_config() -> Result<FilterConfig, Box<dyn Error>> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.1)?,
        Gain::new(0.15)?,
        Gain::new(0.05)?,
        vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        Gain::new(0.8)?,
        vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.6)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        Gain::new(0.9)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

fn assert_invalid_parameters(
    result: Result<(), PipelineError>,
    expected_message: &str,
) -> TestResult {
    match result {
        Err(PipelineError::InvalidParameters(message)) => {
            assert!(
                message.contains(expected_message),
                "expected error message to contain '{expected_message}', got '{message}'"
            );
            Ok(())
        }
        other => Err(io::Error::other(format!(
            "expected InvalidParameters containing '{expected_message}', got {other:?}"
        ))
        .into()),
    }
}

#[test]
fn gain_boundaries_are_valid_for_active_pipeline_controls() -> TestResult {
    let validator = PipelineValidator::new();
    let mut config = valid_config()?;

    config.friction = Gain::ZERO;
    config.damper = Gain::FULL;
    config.inertia = Gain::ZERO;
    config.slew_rate = Gain::FULL;
    config.torque_cap = Gain::FULL;

    validator.validate_config(&config)?;
    Ok(())
}

#[test]
fn notch_filter_pipeline_limits_are_enforced() -> TestResult {
    let validator = PipelineValidator::new();

    let mut frequency = valid_config()?;
    frequency.notch_filters = vec![NotchFilter::new(
        FrequencyHz::new(600.0)?,
        2.0,
        -12.0,
    )?];
    assert_invalid_parameters(validator.validate_config(&frequency), "frequency")?;

    let mut q_factor = valid_config()?;
    q_factor.notch_filters = vec![NotchFilter::new(
        FrequencyHz::new(60.0)?,
        20.1,
        -12.0,
    )?];
    assert_invalid_parameters(validator.validate_config(&q_factor), "Q factor")
}

#[test]
fn enabled_bumpstop_requires_ordered_angles_and_bounded_gains() -> TestResult {
    let validator = PipelineValidator::new();

    let mut angles = valid_config()?;
    angles.bumpstop = BumpstopConfig {
        enabled: true,
        start_angle: 540.0,
        max_angle: 540.0,
        stiffness: 0.5,
        damping: 0.5,
    };
    assert_invalid_parameters(validator.validate_config(&angles), "max_angle")?;

    let mut stiffness = valid_config()?;
    stiffness.bumpstop = BumpstopConfig {
        enabled: true,
        start_angle: 450.0,
        max_angle: 540.0,
        stiffness: 1.1,
        damping: 0.5,
    };
    assert_invalid_parameters(validator.validate_config(&stiffness), "stiffness")?;

    let mut damping = valid_config()?;
    damping.bumpstop = BumpstopConfig {
        enabled: true,
        start_angle: 450.0,
        max_angle: 540.0,
        stiffness: 0.5,
        damping: -0.1,
    };
    assert_invalid_parameters(validator.validate_config(&damping), "damping")
}

#[test]
fn enabled_hands_off_requires_bounded_threshold_and_positive_timeout() -> TestResult {
    let validator = PipelineValidator::new();

    let mut threshold = valid_config()?;
    threshold.hands_off = HandsOffConfig {
        enabled: true,
        threshold: 1.1,
        timeout_seconds: 5.0,
    };
    assert_invalid_parameters(validator.validate_config(&threshold), "threshold")?;

    let mut timeout = valid_config()?;
    timeout.hands_off = HandsOffConfig {
        enabled: true,
        threshold: 0.05,
        timeout_seconds: 0.0,
    };
    assert_invalid_parameters(validator.validate_config(&timeout), "timeout")
}
