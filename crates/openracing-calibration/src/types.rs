//! Calibration type definitions

use serde::{Deserialize, Serialize};

/// Calibration point for axis
///
/// Maps a single raw sensor reading to a normalized `[0.0, 1.0]` value.
///
/// # Examples
///
/// ```
/// use openracing_calibration::CalibrationPoint;
///
/// let point = CalibrationPoint::new(32768, 0.5);
/// assert_eq!(point.raw, 32768);
/// assert!((point.normalized - 0.5).abs() < f32::EPSILON);
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CalibrationPoint {
    /// Raw sensor value (device units, typically 0–65535).
    pub raw: u16,
    /// Expected normalized output in `[0.0, 1.0]`.
    pub normalized: f32,
}

impl CalibrationPoint {
    /// Creates a calibration point from a raw sensor value and its expected normalized output.
    pub fn new(raw: u16, normalized: f32) -> Self {
        Self { raw, normalized }
    }
}

/// Axis calibration data
///
/// Stores the min/max range, optional center point, and dead-zone boundaries
/// for a single input axis. Use [`apply`](AxisCalibration::apply) to convert
/// raw sensor values to normalized `[0.0, 1.0]` output.
///
/// # Examples
///
/// ```
/// use openracing_calibration::AxisCalibration;
///
/// let calib = AxisCalibration::new(0, 65535)
///     .with_center(32768)
///     .with_deadzone(1000, 64535);
///
/// // Mid-range input produces roughly 0.5
/// let value = calib.apply(32768);
/// assert!((value - 0.5).abs() < 0.02);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisCalibration {
    /// Minimum raw value observed (full-left / pedal released).
    pub min: u16,
    /// Optional center position. Used for centering-type axes (e.g., steering).
    pub center: Option<u16>,
    /// Maximum raw value observed (full-right / pedal fully pressed).
    pub max: u16,
    /// Lower dead-zone boundary (raw values below this map to 0.0).
    ///
    /// Stored in the same raw coordinate space as `min`/`max`. The default
    /// (`0`) and any legacy-deserialized value at or below `min` are a no-op:
    /// [`apply`](Self::apply) clamps this bound into `[min, max]`.
    pub deadzone_min: u16,
    /// Upper dead-zone boundary (raw values above this map to 1.0).
    ///
    /// Stored in the same raw coordinate space as `min`/`max`. The default
    /// (`u16::MAX`) and any legacy-deserialized value at or above `max` are
    /// a no-op: [`apply`](Self::apply) clamps this bound into `[min, max]`.
    pub deadzone_max: u16,
}

impl Default for AxisCalibration {
    fn default() -> Self {
        Self {
            min: 0,
            center: None,
            max: 0xFFFF,
            deadzone_min: 0,
            deadzone_max: 0xFFFF,
        }
    }
}

impl AxisCalibration {
    /// Creates an axis calibration with the given raw min/max range.
    ///
    /// Center is unset and the dead-zone defaults to `0..=u16::MAX`, which
    /// [`apply`](Self::apply) treats as a no-op regardless of `min`/`max`.
    /// Use [`with_center`](Self::with_center) and
    /// [`with_deadzone`](Self::with_deadzone) to configure them explicitly.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::AxisCalibration;
    ///
    /// let cal = AxisCalibration::new(100, 900);
    /// assert_eq!(cal.min, 100);
    /// assert_eq!(cal.max, 900);
    /// assert!(cal.center.is_none());
    /// ```
    pub fn new(min: u16, max: u16) -> Self {
        Self {
            min,
            center: None,
            max,
            deadzone_min: 0,
            deadzone_max: 0xFFFF,
        }
    }

    /// Sets the center-point for this axis (e.g., steering wheel straight-ahead).
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::AxisCalibration;
    ///
    /// let cal = AxisCalibration::new(0, 1000).with_center(500);
    /// assert_eq!(cal.center, Some(500));
    /// ```
    pub fn with_center(mut self, center: u16) -> Self {
        self.center = Some(center);
        self
    }

    /// Sets the dead-zone boundaries in raw units, in the same coordinate
    /// space as the axis's `min`/`max`.
    ///
    /// Values at or below `min` map to `0.0`; values at or above `max` map
    /// to `1.0`. [`apply`](Self::apply) clamps both bounds into the axis's
    /// `[min, max]` range, so bounds outside that range are harmless.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::AxisCalibration;
    ///
    /// let cal = AxisCalibration::new(0, 1000).with_deadzone(50, 950);
    /// assert_eq!(cal.deadzone_min, 50);
    /// assert_eq!(cal.deadzone_max, 950);
    /// ```
    pub fn with_deadzone(mut self, min: u16, max: u16) -> Self {
        self.deadzone_min = min;
        self.deadzone_max = max;
        self
    }

    /// Converts a raw sensor value to a normalized `[0.0, 1.0]` output,
    /// applying the configured range and dead-zone.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::AxisCalibration;
    ///
    /// // With deadzone matching the range, output is normalized
    /// let cal = AxisCalibration::new(0, 1000).with_deadzone(0, 1000);
    /// assert!((cal.apply(0) - 0.0).abs() < 0.01);
    /// assert!((cal.apply(500) - 0.5).abs() < 0.01);
    /// assert!((cal.apply(1000) - 1.0).abs() < 0.01);
    ///
    /// // A non-zero-min axis with the default dead-zone still maps its
    /// // full calibrated span to [0.0, 1.0].
    /// let pedal = AxisCalibration::new(1000, 3000);
    /// assert!((pedal.apply(1000) - 0.0).abs() < 0.01);
    /// assert!((pedal.apply(2000) - 0.5).abs() < 0.01);
    /// assert!((pedal.apply(3000) - 1.0).abs() < 0.01);
    /// ```
    pub fn apply(&self, raw: u16) -> f32 {
        let range = self.max.saturating_sub(self.min) as f32;
        if range <= 0.0 {
            return 0.5;
        }

        let normalized = (raw.clamp(self.min, self.max) - self.min) as f32 / range;

        // Deadzone bounds are raw values in the same coordinate space as
        // `min`/`max`. Clamp them into `[min, max]` before normalizing so
        // legacy or default bounds (0..=u16::MAX) act as a no-op instead of
        // compressing the calibrated span, and so inverted/collapsed bounds
        // resolve to a deterministic, finite result rather than NaN.
        let dz_min_raw = self.deadzone_min.clamp(self.min, self.max);
        let dz_max_raw = self.deadzone_max.clamp(self.min, self.max).max(dz_min_raw);
        let dz_min = (dz_min_raw - self.min) as f32 / range;
        let dz_max = (dz_max_raw - self.min) as f32 / range;

        if normalized <= dz_min {
            return 0.0;
        }
        if normalized >= dz_max {
            return 1.0;
        }

        // Remap to 0-1
        (normalized - dz_min) / (dz_max - dz_min)
    }
}

/// Complete device calibration
///
/// Holds the per-axis calibration data for an entire device (steering wheel,
/// pedal set, etc.) along with a human-readable name and schema version.
///
/// # Examples
///
/// ```
/// use openracing_calibration::{DeviceCalibration, AxisCalibration};
///
/// let mut device = DeviceCalibration::new("Fanatec CSL DD", 3);
/// assert_eq!(device.axes.len(), 3);
/// assert_eq!(device.name, "Fanatec CSL DD");
/// assert_eq!(device.version, 1);
///
/// // Configure individual axes
/// if let Some(steering) = device.axis(0) {
///     *steering = AxisCalibration::new(0, 65535).with_center(32768);
/// }
///
/// // Out-of-bounds access returns None
/// assert!(device.axis(10).is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCalibration {
    /// Human-readable device name (e.g., `"Fanatec CSL DD"`).
    pub name: String,
    /// Per-axis calibration data, indexed by axis number.
    pub axes: Vec<AxisCalibration>,
    /// Schema version for forward-compatible serialization.
    pub version: u32,
}

impl Default for DeviceCalibration {
    fn default() -> Self {
        Self {
            name: String::new(),
            axes: Vec::new(),
            version: 1,
        }
    }
}

impl DeviceCalibration {
    /// Creates a new device calibration with the given name and number of axes.
    ///
    /// Each axis starts with a default full-range calibration.
    pub fn new(name: impl Into<String>, axis_count: usize) -> Self {
        Self {
            name: name.into(),
            axes: vec![AxisCalibration::default(); axis_count],
            version: 1,
        }
    }

    /// Returns a mutable reference to the axis at `index`, or `None` if out of bounds.
    pub fn axis(&mut self, index: usize) -> Option<&mut AxisCalibration> {
        self.axes.get_mut(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axis_calibration_basic() {
        let calib = AxisCalibration::new(0, 65535);

        assert!((calib.apply(0) - 0.0).abs() < 0.01);
        assert!((calib.apply(32768) - 0.5).abs() < 0.01);
        assert!((calib.apply(65535) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_axis_calibration_with_deadzone() {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(1000, 64535);

        assert!((calib.apply(0) - 0.0).abs() < 0.01);
        assert!((calib.apply(32768) - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_axis_calibration_centered() {
        let calib = AxisCalibration::new(0, 65535).with_center(32768);

        assert!(calib.center.is_some());
        assert_eq!(calib.center.expect("center should be set"), 32768);
    }

    #[test]
    fn test_axis_calibration_degenerate_range() {
        // When min == max, apply should return 0.5 as a safe fallback
        let calib = AxisCalibration::new(500, 500);
        assert!((calib.apply(500) - 0.5).abs() < f32::EPSILON);
        assert!((calib.apply(0) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_axis_calibration_deadzone_clamps_below() {
        let calib = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
        // Value below dead-zone min should return 0.0
        assert!((calib.apply(100) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_axis_calibration_deadzone_clamps_above() {
        let calib = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
        // Value above dead-zone max should return 1.0
        assert!((calib.apply(900) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_axis_calibration_default() {
        let calib = AxisCalibration::default();
        assert_eq!(calib.min, 0);
        assert_eq!(calib.max, 0xFFFF);
        assert!(calib.center.is_none());
        assert_eq!(calib.deadzone_min, 0);
        assert_eq!(calib.deadzone_max, 0xFFFF);
    }

    #[test]
    fn test_axis_calibration_serde_round_trip() {
        let calib = AxisCalibration::new(100, 900)
            .with_center(500)
            .with_deadzone(150, 850);

        let json = serde_json::to_string(&calib).expect("serialize should succeed");
        let restored: AxisCalibration =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(restored.min, calib.min);
        assert_eq!(restored.max, calib.max);
        assert_eq!(restored.center, calib.center);
        assert_eq!(restored.deadzone_min, calib.deadzone_min);
        assert_eq!(restored.deadzone_max, calib.deadzone_max);
    }

    #[test]
    fn test_device_calibration() {
        let mut calib = DeviceCalibration::new("Test Device", 2);

        assert_eq!(calib.axes.len(), 2);

        if let Some(a) = calib.axis(0) {
            *a = AxisCalibration::new(0, 1000);
        }

        if let Some(axis) = calib.axis(0) {
            assert_eq!(axis.max, 1000);
        }
    }

    #[test]
    fn test_device_calibration_out_of_bounds() {
        let mut calib = DeviceCalibration::new("Test", 2);
        assert!(calib.axis(5).is_none());
        assert!(calib.axis(100).is_none());
    }

    #[test]
    fn test_device_calibration_default() {
        let calib = DeviceCalibration::default();
        assert!(calib.name.is_empty());
        assert!(calib.axes.is_empty());
        assert_eq!(calib.version, 1);
    }

    #[test]
    fn test_device_calibration_serde_round_trip() {
        let calib = DeviceCalibration::new("Fanatec CSL DD", 3);

        let json = serde_json::to_string(&calib).expect("serialize should succeed");
        let restored: DeviceCalibration =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(restored.name, calib.name);
        assert_eq!(restored.axes.len(), calib.axes.len());
        assert_eq!(restored.version, calib.version);
    }

    #[test]
    fn test_calibration_point() {
        let point = CalibrationPoint::new(32768, 0.5);
        assert_eq!(point.raw, 32768);
        assert!((point.normalized - 0.5).abs() < f32::EPSILON);
    }
}
