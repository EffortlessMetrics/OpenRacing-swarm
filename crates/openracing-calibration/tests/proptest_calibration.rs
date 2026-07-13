//! Property-based tests for calibration: min/max detection and dead zone behavior.

#[cfg(test)]
mod proptest_calibration {
    use openracing_calibration::AxisCalibration;
    use proptest::prelude::*;

    /// Helper: build an AxisCalibration with no deadzone distortion.
    /// Set deadzone bounds to match min/max exactly for clean mapping tests.
    fn no_deadzone_calib(min: u16, max: u16) -> AxisCalibration {
        AxisCalibration::new(min, max).with_deadzone(min, max)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // --- Min/max detection: output always within [0.0, 1.0] for in-range inputs ---

        #[test]
        fn apply_in_range_always_within_unit(
            min_val in 0u16..32000,
            spread in 1u16..32000,
            offset in 0.0f32..=1.0,
        ) {
            let max_val = min_val.saturating_add(spread);
            let raw = min_val + (offset * spread as f32) as u16;
            let calib = no_deadzone_calib(min_val, max_val);
            let output = calib.apply(raw);
            prop_assert!(output >= 0.0, "output {} must be >= 0.0", output);
            prop_assert!(output <= 1.0, "output {} must be <= 1.0", output);
            prop_assert!(output.is_finite(), "output must be finite");
        }

        // --- Min/max detection: full-range calibration maps correctly ---

        #[test]
        fn full_range_apply_bounded(raw in 0u16..=u16::MAX) {
            let calib = AxisCalibration::new(0, u16::MAX);
            let output = calib.apply(raw);
            prop_assert!(output >= 0.0, "output {} must be >= 0.0", output);
            prop_assert!(output <= 1.0, "output {} must be <= 1.0", output);
        }

        // --- Min/max detection: min maps to 0.0, max maps to 1.0 ---

        #[test]
        fn apply_min_maps_to_zero(
            min_val in 0u16..60000,
            spread in 1u16..5000,
        ) {
            let max_val = min_val.saturating_add(spread);
            let calib = no_deadzone_calib(min_val, max_val);
            let output = calib.apply(min_val);
            prop_assert!((output - 0.0).abs() < 0.01,
                "apply(min={}) should be ~0.0, got {}", min_val, output);
        }

        #[test]
        fn apply_max_maps_to_one(
            min_val in 0u16..60000,
            spread in 1u16..5000,
        ) {
            let max_val = min_val.saturating_add(spread);
            let calib = no_deadzone_calib(min_val, max_val);
            let output = calib.apply(max_val);
            prop_assert!((output - 1.0).abs() < 0.01,
                "apply(max={}) should be ~1.0, got {}", max_val, output);
        }

        // --- Dead zone: inputs below deadzone_min normalize to 0.0 ---

        #[test]
        fn deadzone_low_maps_to_zero(
            spread in 1000u16..60000,
            dz_pct in 5u16..30,
        ) {
            // Use min=0 so deadzone values align with apply()'s normalization
            let max_val = spread;
            let dz_min_raw = (spread as u32 * dz_pct as u32 / 100) as u16;
            let calib = AxisCalibration::new(0, max_val)
                .with_deadzone(dz_min_raw, spread);

            // Input at min (0) is below deadzone → should be 0.0
            let output = calib.apply(0);
            prop_assert!((output - 0.0).abs() < 0.01,
                "input 0 with dz_min={} should be ~0.0, got {}", dz_min_raw, output);
        }

        // --- Dead zone: inputs above deadzone_max normalize to 1.0 ---

        #[test]
        fn deadzone_high_maps_to_one(
            spread in 1000u16..60000,
            dz_pct in 5u16..30,
        ) {
            let max_val = spread;
            let dz_max_raw = spread - (spread as u32 * dz_pct as u32 / 100) as u16;
            let calib = AxisCalibration::new(0, max_val)
                .with_deadzone(0, dz_max_raw);

            // Input at max exceeds deadzone_max → should be 1.0
            let output = calib.apply(max_val);
            prop_assert!((output - 1.0).abs() < 0.01,
                "input {} with dz_max={} should be ~1.0, got {}", max_val, dz_max_raw, output);
        }

        // --- Monotonicity: output is non-decreasing for increasing input ---

        #[test]
        fn apply_monotonic(
            min_val in 0u16..30000,
            spread in 2u16..30000,
            frac_a in 0.0f32..=1.0,
            frac_b in 0.0f32..=1.0,
        ) {
            let max_val = min_val.saturating_add(spread);
            let calib = no_deadzone_calib(min_val, max_val);
            let raw_a = min_val + (frac_a * spread as f32) as u16;
            let raw_b = min_val + (frac_b * spread as f32) as u16;
            let (lo, hi) = if raw_a <= raw_b { (raw_a, raw_b) } else { (raw_b, raw_a) };
            let out_lo = calib.apply(lo);
            let out_hi = calib.apply(hi);
            prop_assert!(out_hi >= out_lo,
                "apply must be monotonic: apply({})={} > apply({})={}", lo, out_lo, hi, out_hi);
        }

        // --- Equal min/max returns 0.5 ---

        #[test]
        fn equal_min_max_returns_half(val in 0u16..=u16::MAX) {
            let calib = AxisCalibration::new(val, val);
            let output = calib.apply(val);
            prop_assert!((output - 0.5).abs() < 0.01,
                "equal min/max should return 0.5, got {}", output);
        }
    }
}
