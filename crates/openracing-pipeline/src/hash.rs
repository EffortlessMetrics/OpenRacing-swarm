//! Pipeline hash calculation for deterministic comparison
//!
//! This module provides deterministic hash calculation for filter configurations,
//! enabling change detection and cache invalidation.

use openracing_curves::CurveLut;
use openracing_curves::CurveType;
use racing_wheel_schemas::entities::FilterConfig;
use racing_wheel_schemas::prelude::CurvePoint;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const CUSTOM_LUT_HASH_DOMAIN: &[u8] = b"openracing:curve-lut:v1";

/// Calculate deterministic hash of filter configuration.
///
/// This hash is used to detect configuration changes and enable efficient
/// pipeline swap decisions. It is an in-process change-detection fingerprint,
/// not a cryptographic identifier or a persisted cross-version format.
///
/// # Arguments
///
/// * `config` - The filter configuration to hash
///
/// # Returns
///
/// A 64-bit change-detection fingerprint for the configuration.
#[must_use]
pub fn calculate_config_hash(config: &FilterConfig) -> u64 {
    let mut hasher = DefaultHasher::new();

    config.reconstruction.hash(&mut hasher);
    config.friction.value().to_bits().hash(&mut hasher);
    config.damper.value().to_bits().hash(&mut hasher);
    config.inertia.value().to_bits().hash(&mut hasher);
    config.slew_rate.value().to_bits().hash(&mut hasher);
    config.torque_cap.value().to_bits().hash(&mut hasher);

    hash_curve_points(&config.curve_points, &mut hasher);
    hash_notch_filters(&config.notch_filters, &mut hasher);
    hash_bumpstop_config(&config.bumpstop, &mut hasher);
    hash_hands_off_config(&config.hands_off, &mut hasher);

    hasher.finish()
}

/// Calculate deterministic hash including response curve.
///
/// Extends [`calculate_config_hash`] to include the response-curve type and
/// parameters in the change-detection fingerprint. Custom LUTs contribute all
/// 256 raw table entries; hashing remains a compile-time/off-thread operation
/// and does not add work to response-curve evaluation on the RT path.
///
/// This value is not a cryptographic identifier or a persisted cross-version
/// format. Floating-point values are represented by their raw bit patterns, so
/// `-0.0` and `0.0` are intentionally distinct here.
///
/// # Arguments
///
/// * `config` - The filter configuration to hash
/// * `response_curve` - Optional response curve type to include in hash
///
/// # Returns
///
/// A 64-bit change-detection fingerprint for the configuration and response curve.
#[must_use]
pub fn calculate_config_hash_with_curve(
    config: &FilterConfig,
    response_curve: Option<&CurveType>,
) -> u64 {
    let mut hasher = DefaultHasher::new();

    config.reconstruction.hash(&mut hasher);
    config.friction.value().to_bits().hash(&mut hasher);
    config.damper.value().to_bits().hash(&mut hasher);
    config.inertia.value().to_bits().hash(&mut hasher);
    config.slew_rate.value().to_bits().hash(&mut hasher);
    config.torque_cap.value().to_bits().hash(&mut hasher);

    hash_curve_points(&config.curve_points, &mut hasher);
    hash_notch_filters(&config.notch_filters, &mut hasher);
    hash_bumpstop_config(&config.bumpstop, &mut hasher);
    hash_hands_off_config(&config.hands_off, &mut hasher);

    hash_curve_type(response_curve, &mut hasher);

    hasher.finish()
}

/// Hash curve points into the hasher.
fn hash_curve_points(curve_points: &[CurvePoint], hasher: &mut DefaultHasher) {
    for point in curve_points {
        point.input.to_bits().hash(hasher);
        point.output.to_bits().hash(hasher);
    }
}

/// Hash notch filters into the hasher.
fn hash_notch_filters(
    notch_filters: &[racing_wheel_schemas::entities::NotchFilter],
    hasher: &mut DefaultHasher,
) {
    for filter in notch_filters {
        filter.frequency.value().to_bits().hash(hasher);
        filter.q_factor.to_bits().hash(hasher);
        filter.gain_db.to_bits().hash(hasher);
    }
}

/// Hash bumpstop configuration into the hasher.
fn hash_bumpstop_config(
    config: &racing_wheel_schemas::entities::BumpstopConfig,
    hasher: &mut DefaultHasher,
) {
    config.enabled.hash(hasher);
    config.start_angle.to_bits().hash(hasher);
    config.max_angle.to_bits().hash(hasher);
    config.stiffness.to_bits().hash(hasher);
    config.damping.to_bits().hash(hasher);
}

/// Hash hands-off configuration into the hasher.
fn hash_hands_off_config(
    config: &racing_wheel_schemas::entities::HandsOffConfig,
    hasher: &mut DefaultHasher,
) {
    config.enabled.hash(hasher);
    config.threshold.to_bits().hash(hasher);
    config.timeout_seconds.to_bits().hash(hasher);
}

/// Hash curve type into the hasher.
fn hash_curve_type(curve: Option<&CurveType>, hasher: &mut DefaultHasher) {
    if let Some(curve) = curve {
        match curve {
            CurveType::Linear => {
                0u8.hash(hasher);
            }
            CurveType::Exponential { exponent } => {
                1u8.hash(hasher);
                exponent.to_bits().hash(hasher);
            }
            CurveType::Logarithmic { base } => {
                2u8.hash(hasher);
                base.to_bits().hash(hasher);
            }
            CurveType::Bezier(bezier) => {
                3u8.hash(hasher);
                for (x, y) in &bezier.control_points {
                    x.to_bits().hash(hasher);
                    y.to_bits().hash(hasher);
                }
            }
            CurveType::Custom(lut) => {
                4u8.hash(hasher);
                CUSTOM_LUT_HASH_DOMAIN.hash(hasher);
                CurveLut::SIZE.hash(hasher);
                hash_lut(lut, hasher);
            }
        }
    } else {
        255u8.hash(hasher);
    }
}

/// Hash every raw LUT entry in table order.
///
/// `to_bits` matches the representation used for the other floating-point hash
/// inputs. This intentionally preserves signed-zero distinctions. Non-finite
/// accepted-value policy belongs at the curve validation boundary (see #312),
/// rather than being silently normalized by change detection.
fn hash_lut(lut: &CurveLut, hasher: &mut DefaultHasher) {
    for value in lut.table() {
        value.to_bits().hash(hasher);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{FrequencyHz, Gain, NotchFilter};

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must() failed: {:?}", e),
        }
    }

    fn create_test_config() -> FilterConfig {
        FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )
        .unwrap()
    }

    #[test]
    fn test_config_hash_deterministic() {
        let config = create_test_config();

        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);

        assert_eq!(hash1, hash2, "Same config should produce same hash");
    }

    #[test]
    fn test_config_hash_different_configs() {
        let config1 = create_test_config();
        let config2 = FilterConfig::default();

        let hash1 = calculate_config_hash(&config1);
        let hash2 = calculate_config_hash(&config2);

        assert_ne!(
            hash1, hash2,
            "Different configs should produce different hashes"
        );
    }

    #[test]
    fn test_config_hash_with_curve_different() {
        let config = create_test_config();

        let hash_no_curve = calculate_config_hash_with_curve(&config, None);
        let hash_linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
        let hash_exp =
            calculate_config_hash_with_curve(&config, Some(&CurveType::exponential(2.0).unwrap()));

        assert_ne!(hash_no_curve, hash_linear);
        assert_ne!(hash_linear, hash_exp);
        assert_ne!(hash_no_curve, hash_exp);
    }

    #[test]
    fn test_custom_lut_hash_covers_unsampled_interior_entries() {
        let config = create_test_config();
        let baseline = CurveLut::linear();
        let changed_index = 17usize;
        let changed = CurveLut::from_fn(|input| {
            let index = (input * (CurveLut::SIZE - 1) as f32).round() as usize;
            if index == changed_index {
                (input + 0.01).min(1.0)
            } else {
                input
            }
        });

        for sampled_index in [0usize, 64, 128, 192, 255] {
            assert_eq!(
                baseline.table()[sampled_index].to_bits(),
                changed.table()[sampled_index].to_bits(),
                "legacy sample index {sampled_index} unexpectedly differs"
            );
        }
        assert_ne!(
            baseline.table()[changed_index].to_bits(),
            changed.table()[changed_index].to_bits(),
            "fixture must differ at an entry omitted by the legacy sampler"
        );

        let baseline_curve = CurveType::Custom(Box::new(baseline));
        let changed_curve = CurveType::Custom(Box::new(changed));
        let baseline_hash = calculate_config_hash_with_curve(&config, Some(&baseline_curve));
        let changed_hash = calculate_config_hash_with_curve(&config, Some(&changed_curve));

        assert_ne!(
            baseline_hash, changed_hash,
            "an unsampled custom-LUT change must affect the configuration hash"
        );
        assert_eq!(
            baseline_hash,
            calculate_config_hash_with_curve(&config, Some(&baseline_curve))
        );
        assert_eq!(
            changed_hash,
            calculate_config_hash_with_curve(&config, Some(&changed_curve))
        );
    }

    #[test]
    fn test_custom_lut_hash_preserves_signed_zero_bits() {
        let config = create_test_config();
        let positive_zero = CurveLut::from_fn(|_| 0.0);
        let negative_zero = CurveLut::from_fn(|_| -0.0);

        assert_ne!(positive_zero.table()[0].to_bits(), negative_zero.table()[0].to_bits());

        let positive_curve = CurveType::Custom(Box::new(positive_zero));
        let negative_curve = CurveType::Custom(Box::new(negative_zero));

        assert_ne!(
            calculate_config_hash_with_curve(&config, Some(&positive_curve)),
            calculate_config_hash_with_curve(&config, Some(&negative_curve)),
            "custom-LUT hashing is bitwise, so signed zero must remain distinct"
        );
    }

    #[test]
    fn test_config_hash_stable_under_ordering() {
        let config = create_test_config();
        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);
        let hash3 = calculate_config_hash(&config);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_empty_config_hash() {
        let config = FilterConfig::default();
        let hash = calculate_config_hash(&config);
        assert_ne!(hash, 0, "Default config should have non-zero hash");
    }
}
