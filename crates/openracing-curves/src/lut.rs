//! Pre-computed lookup table for RT-safe curve evaluation.

use serde::{Deserialize, Serialize};

use crate::bezier::BezierCurve;
use crate::error::CurveError;

/// Pre-computed lookup table for RT path (no allocation).
///
/// The LUT provides O(1) curve evaluation with linear interpolation
/// between entries. This is essential for the RT path where we cannot
/// perform iterative algorithms like Newton-Raphson.
///
/// # RT Safety
///
/// `CurveLut::lookup()` is RT-safe:
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
/// - No syscalls or I/O
///
/// # Example
///
/// ```
/// use openracing_curves::{BezierCurve, CurveLut};
///
/// // Create a curve and convert to LUT at profile load time
/// let curve = BezierCurve::ease_in_out();
/// let lut = curve.to_lut();
///
/// // RT-safe lookup (can be called at 1kHz)
/// let output = lut.lookup(0.5);
/// assert!(output >= 0.0 && output <= 1.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct CurveLut {
    /// 256-entry lookup table for fast interpolation.
    pub(crate) table: [f32; 256],
}

impl CurveLut {
    /// LUT size (256 entries provides good precision with minimal memory).
    pub const SIZE: usize = 256;

    /// Build LUT from Bezier curve (called at profile load, not RT).
    ///
    /// This pre-computes the curve mapping for 256 evenly-spaced input values,
    /// allowing fast lookup with linear interpolation in the RT path.
    ///
    /// # Arguments
    ///
    /// * `curve` - The Bezier curve to pre-compute
    ///
    /// # Returns
    ///
    /// A `CurveLut` ready for RT-safe lookups.
    pub fn from_bezier(curve: &BezierCurve) -> Self {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            let input = i as f32 / (Self::SIZE - 1) as f32;
            *entry = Self::sanitize_generated_value(curve.map(input));
        }

        Self { table }
    }

    /// Create a linear (identity) LUT.
    ///
    /// For a linear LUT, output equals input.
    pub fn linear() -> Self {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            *entry = i as f32 / (Self::SIZE - 1) as f32;
        }

        Self { table }
    }

    /// Create a LUT from a closure.
    ///
    /// # Arguments
    ///
    /// * `f` - Function that maps input `[0,1]` to output `[0,1]`
    ///
    /// # Returns
    ///
    /// A `CurveLut` with values computed from the function.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_curves::CurveLut;
    ///
    /// // Create a quadratic response curve: f(x) = x²
    /// let lut = CurveLut::from_fn(|x| x * x);
    ///
    /// assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
    /// assert!((lut.lookup(0.5) - 0.25).abs() < 0.02);
    /// assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    /// ```
    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn(f32) -> f32,
    {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            let input = i as f32 / (Self::SIZE - 1) as f32;
            *entry = Self::sanitize_generated_value(f(input));
        }

        Self { table }
    }

    /// Strictly construct a LUT from a raw 256-entry table.
    ///
    /// Unlike the compatibility `from_fn` generator, raw/user-supplied table
    /// data is never sanitized: the first non-finite entry is rejected with its
    /// index. Finite values outside `[0,1]` retain the existing raw-table
    /// acceptance policy; this method narrows only non-finite acceptance.
    pub fn try_from_table(table: [f32; 256]) -> Result<Self, CurveError> {
        Self::validate_table(&table)?;
        Ok(Self { table })
    }

    /// Strictly generate a LUT from a closure.
    ///
    /// Finite outputs keep the existing `[0,1]` clamp. The first non-finite
    /// generated output is rejected instead of being sanitized.
    pub fn try_from_fn<F>(f: F) -> Result<Self, CurveError>
    where
        F: Fn(f32) -> f32,
    {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            let input = i as f32 / (Self::SIZE - 1) as f32;
            let value = f(input);
            if !value.is_finite() {
                return Err(Self::non_finite_error(i));
            }
            *entry = value.clamp(0.0, 1.0);
        }

        Ok(Self { table })
    }

    /// Normalize generated values for the legacy infallible constructors.
    ///
    /// Finite values keep the historical `[0,1]` clamp. A non-finite generated
    /// value becomes `0.0`, a deterministic finite safe value. Callers that
    /// require rejection should use `try_from_fn` or `try_from_table`.
    #[inline]
    fn sanitize_generated_value(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn validate_table(table: &[f32; 256]) -> Result<(), CurveError> {
        for (index, value) in table.iter().enumerate() {
            if !value.is_finite() {
                return Err(Self::non_finite_error(index));
            }
        }
        Ok(())
    }

    fn non_finite_error(index: usize) -> CurveError {
        CurveError::InvalidConfiguration(format!(
            "CurveLut entry {index} must be finite"
        ))
    }

    /// Fast lookup with linear interpolation (RT-safe).
    ///
    /// This method is designed for the RT path:
    /// - No heap allocations
    /// - O(1) time complexity
    /// - Bounded execution time
    ///
    /// # Arguments
    ///
    /// * `input` - Input value (will be clamped to `[0,1]`)
    ///
    /// # Returns
    ///
    /// Interpolated output value in `[0,1]`.
    #[inline]
    pub fn lookup(&self, input: f32) -> f32 {
        let input = input.clamp(0.0, 1.0);

        let scaled = input * (Self::SIZE - 1) as f32;
        let index_low = (scaled as usize).min(Self::SIZE - 2);
        let index_high = index_low + 1;
        let fraction = scaled - index_low as f32;

        let low_value = self.table[index_low];
        let high_value = self.table[index_high];

        low_value + fraction * (high_value - low_value)
    }

    /// Get the raw table for inspection.
    ///
    /// This is primarily useful for testing and debugging.
    pub fn table(&self) -> &[f32; 256] {
        &self.table
    }

    /// Check if the LUT is monotonic (always increasing or staying the same).
    ///
    /// This is useful for validating that a curve produces a valid LUT.
    pub fn is_monotonic(&self) -> bool {
        for i in 1..Self::SIZE {
            if self.table[i] < self.table[i - 1] {
                return false;
            }
        }
        true
    }

    /// Get the minimum value in the LUT.
    pub fn min_value(&self) -> f32 {
        self.table.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// Get the maximum value in the LUT.
    pub fn max_value(&self) -> f32 {
        self.table.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }
}

impl Default for CurveLut {
    fn default() -> Self {
        Self::linear()
    }
}

impl Serialize for CurveLut {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.table.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CurveLut {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<f32> = Vec::deserialize(deserializer)?;
        if vec.len() != 256 {
            return Err(serde::de::Error::custom(format!(
                "Expected 256 entries in CurveLut, got {}",
                vec.len()
            )));
        }
        let mut table = [0.0f32; 256];
        table.copy_from_slice(&vec);
        CurveLut::try_from_table(table).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(v) => v,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_curve_lut_from_linear_bezier() {
        let curve = BezierCurve::linear();
        let lut = CurveLut::from_bezier(&curve);

        let tolerance = 0.02;
        assert!((lut.lookup(0.0) - 0.0).abs() < tolerance);
        assert!((lut.lookup(0.5) - 0.5).abs() < tolerance);
        assert!((lut.lookup(1.0) - 1.0).abs() < tolerance);
    }

    #[test]
    fn test_curve_lut_lookup_interpolation() {
        let lut = CurveLut::linear();

        for i in 0..100 {
            let input = i as f32 / 99.0;
            let output = lut.lookup(input);
            assert!(
                (output - input).abs() < 0.01,
                "Linear LUT failed at input {}: got {}",
                input,
                output
            );
        }
    }

    #[test]
    fn test_curve_lut_lookup_clamping() {
        let lut = CurveLut::linear();

        let below = lut.lookup(-0.5);
        let above = lut.lookup(1.5);

        assert!((0.0..=1.0).contains(&below));
        assert!((0.0..=1.0).contains(&above));
        assert!((below - 0.0).abs() < 0.01);
        assert!((above - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_output_range() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.1, 0.9),
            (0.9, 0.1),
            (1.0, 1.0),
        ]));
        let lut = CurveLut::from_bezier(&curve);

        for i in 0..=100 {
            let input = i as f32 / 100.0;
            let output = lut.lookup(input);
            assert!(
                (0.0..=1.0).contains(&output),
                "Output {} out of range for input {}",
                output,
                input
            );
        }
    }

    #[test]
    fn test_curve_lut_determinism() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));

        let lut1 = CurveLut::from_bezier(&curve);
        let lut2 = CurveLut::from_bezier(&curve);

        for i in 0..CurveLut::SIZE {
            assert_eq!(
                lut1.table()[i],
                lut2.table()[i],
                "LUT mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_curve_lut_no_allocation_in_lookup() {
        let lut = CurveLut::linear();

        for _ in 0..10000 {
            let input = 0.5;
            let output = lut.lookup(input);
            assert!(output.is_finite());
        }
    }

    #[test]
    fn test_curve_lut_s_curve() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.0, 0.5),
            (1.0, 0.5),
            (1.0, 1.0),
        ]));
        let lut = CurveLut::from_bezier(&curve);

        let low = lut.lookup(0.25);
        let mid = lut.lookup(0.5);
        let high = lut.lookup(0.75);

        assert!(low < mid);
        assert!(mid < high);

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_default() {
        let lut = CurveLut::default();
        assert!((lut.lookup(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_from_fn() {
        let lut = CurveLut::from_fn(|x| x * x);

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.25).abs() < 0.02);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_is_monotonic() {
        let linear_lut = CurveLut::linear();
        assert!(linear_lut.is_monotonic());

        let s_curve = BezierCurve::ease_in_out();
        let s_lut = s_curve.to_lut();
        assert!(s_lut.is_monotonic());
    }

    #[test]
    fn test_curve_lut_min_max() {
        let lut = CurveLut::linear();
        assert!((lut.min_value() - 0.0).abs() < 0.01);
        assert!((lut.max_value() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_serialization() {
        let lut = CurveLut::from_fn(|x| x.powf(2.0));

        let json = serde_json::to_string(&lut).expect("serialization failed");
        let deserialized: CurveLut = serde_json::from_str(&json).expect("deserialization failed");

        for i in 0..CurveLut::SIZE {
            assert!(
                (lut.table()[i] - deserialized.table()[i]).abs() < 1e-6,
                "Mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_curve_lut_serialization_wrong_size() {
        let bad_data = serde_json::to_string(&vec![0.0f32; 100]).expect("serialization failed");
        let result: Result<CurveLut, _> = serde_json::from_str(&bad_data);
        assert!(result.is_err());
    }
}
