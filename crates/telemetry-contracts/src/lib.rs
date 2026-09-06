//! Normalized telemetry domain contracts for OpenRacing.

#![deny(static_mut_refs)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normalized telemetry data structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NormalizedTelemetry {
    /// Force feedback scalar value (-1.0 to 1.0)
    /// Represents the force feedback strength requested by the game.
    pub ffb_scalar: Option<f32>,

    /// Engine RPM (revolutions per minute).
    pub rpm: Option<f32>,

    /// Vehicle speed in meters per second.
    pub speed_ms: Option<f32>,

    /// Tire slip ratio (0.0 = no slip, 1.0 = full slip).
    pub slip_ratio: Option<f32>,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears).
    pub gear: Option<i8>,

    /// Racing flags and status information.
    pub flags: TelemetryFlags,

    /// Car identifier (if available).
    pub car_id: Option<String>,

    /// Track identifier (if available).
    pub track_id: Option<String>,

    /// Additional game-specific data.
    pub extended: HashMap<String, TelemetryValue>,
}

/// Racing flags and status information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFlags {
    /// Yellow flag (caution).
    pub yellow_flag: bool,

    /// Red flag (session stopped).
    pub red_flag: bool,

    /// Blue flag (being lapped).
    pub blue_flag: bool,

    /// Checkered flag (race finished).
    pub checkered_flag: bool,

    /// Green flag (racing).
    pub green_flag: bool,

    /// Pit limiter active.
    pub pit_limiter: bool,

    /// In pit lane.
    pub in_pits: bool,

    /// DRS (Drag Reduction System) available.
    pub drs_available: bool,

    /// DRS currently active.
    pub drs_active: bool,

    /// ERS (Energy Recovery System) available.
    pub ers_available: bool,

    /// Launch control active.
    pub launch_control: bool,

    /// Traction control active.
    pub traction_control: bool,

    /// ABS active.
    pub abs_active: bool,
}

impl Default for TelemetryFlags {
    fn default() -> Self {
        Self {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        }
    }
}

/// Extended telemetry value for game-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TelemetryValue {
    Float(f32),
    Integer(i32),
    Boolean(bool),
    String(String),
}

impl NormalizedTelemetry {
    /// Create a default telemetry instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set FFB scalar value with finite-value validation and clamping.
    pub fn with_ffb_scalar(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.ffb_scalar = Some(value.clamp(-1.0, 1.0));
        }
        self
    }

    /// Set RPM value with validation.
    pub fn with_rpm(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.rpm = Some(value);
        }
        self
    }

    /// Set speed value with validation.
    pub fn with_speed_ms(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.speed_ms = Some(value);
        }
        self
    }

    /// Set slip ratio with validation.
    pub fn with_slip_ratio(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.slip_ratio = Some(value.clamp(0.0, 1.0));
        }
        self
    }

    /// Set gear value.
    pub fn with_gear(mut self, value: i8) -> Self {
        self.gear = Some(value);
        self
    }

    /// Set car ID.
    pub fn with_car_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.car_id = Some(id);
        }
        self
    }

    /// Set track ID.
    pub fn with_track_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.track_id = Some(id);
        }
        self
    }

    /// Set flags.
    pub fn with_flags(mut self, flags: TelemetryFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Add extended telemetry value.
    pub fn with_extended(mut self, key: String, value: TelemetryValue) -> Self {
        self.extended.insert(key, value);
        self
    }

    /// Check if telemetry has valid FFB data.
    pub fn has_ffb_data(&self) -> bool {
        self.ffb_scalar.is_some()
    }

    /// Check if telemetry has valid RPM data for LED display.
    pub fn has_rpm_data(&self) -> bool {
        self.rpm.is_some()
    }

    /// Get RPM as fraction of a positive finite redline (0.0-1.0).
    pub fn rpm_fraction(&self, redline_rpm: f32) -> Option<f32> {
        if !redline_rpm.is_finite() || redline_rpm <= 0.0 {
            return None;
        }
        self.rpm.map(|rpm| (rpm / redline_rpm).clamp(0.0, 1.0))
    }

    /// Check if any racing flags are active.
    pub fn has_active_flags(&self) -> bool {
        self.flags.yellow_flag
            || self.flags.red_flag
            || self.flags.blue_flag
            || self.flags.checkered_flag
    }

    /// Get speed in km/h.
    pub fn speed_kmh(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 3.6)
    }

    /// Get speed in mph.
    pub fn speed_mph(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 2.237)
    }
}

/// Telemetry field coverage information for documentation and docs generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFieldCoverage {
    pub game_id: String,
    pub game_version: String,
    pub ffb_scalar: bool,
    pub rpm: bool,
    pub speed: bool,
    pub slip_ratio: bool,
    pub gear: bool,
    pub flags: FlagCoverage,
    pub car_id: bool,
    pub track_id: bool,
    pub extended_fields: Vec<String>,
}

/// Flag coverage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagCoverage {
    pub yellow_flag: bool,
    pub red_flag: bool,
    pub blue_flag: bool,
    pub checkered_flag: bool,
    pub green_flag: bool,
    pub pit_limiter: bool,
    pub in_pits: bool,
    pub drs_available: bool,
    pub drs_active: bool,
    pub ers_available: bool,
    pub launch_control: bool,
    pub traction_control: bool,
    pub abs_active: bool,
}

/// Telemetry frame with timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Normalized telemetry data.
    pub data: NormalizedTelemetry,

    /// Timestamp when frame was received (monotonic).
    pub timestamp_ns: u64,

    /// Sequence number for ordering.
    pub sequence: u64,

    /// Raw data size for diagnostics.
    pub raw_size: usize,
}

impl TelemetryFrame {
    /// Create a new telemetry frame.
    pub fn new(
        data: NormalizedTelemetry,
        timestamp_ns: u64,
        sequence: u64,
        raw_size: usize,
    ) -> Self {
        Self {
            data,
            timestamp_ns,
            sequence,
            raw_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FlagCoverage, NormalizedTelemetry, TelemetryFieldCoverage, TelemetryFlags, TelemetryFrame,
        TelemetryValue,
    };

    // ── NormalizedTelemetry::new / Default ──────────────────────────────

    #[test]
    fn normalized_telemetry_new_returns_default() {
        let t = NormalizedTelemetry::new();
        assert!(t.ffb_scalar.is_none());
        assert!(t.rpm.is_none());
        assert!(t.speed_ms.is_none());
        assert!(t.slip_ratio.is_none());
        assert!(t.gear.is_none());
        assert!(t.car_id.is_none());
        assert!(t.track_id.is_none());
        assert!(t.extended.is_empty());
        assert!(t.flags.green_flag);
    }

    // ── with_ffb_scalar ─────────────────────────────────────────────────

    #[test]
    fn with_ffb_scalar_clamps_above_one() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(2.5);
        assert_eq!(t.ffb_scalar, Some(1.0));
    }

    #[test]
    fn with_ffb_scalar_clamps_below_negative_one() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(-3.0);
        assert_eq!(t.ffb_scalar, Some(-1.0));
    }

    #[test]
    fn with_ffb_scalar_preserves_valid_value() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(0.5);
        assert_eq!(t.ffb_scalar, Some(0.5));
    }

    #[test]
    fn with_ffb_scalar_boundary_values() {
        assert_eq!(
            NormalizedTelemetry::new().with_ffb_scalar(-1.0).ffb_scalar,
            Some(-1.0)
        );
        assert_eq!(
            NormalizedTelemetry::new().with_ffb_scalar(0.0).ffb_scalar,
            Some(0.0)
        );
        assert_eq!(
            NormalizedTelemetry::new().with_ffb_scalar(1.0).ffb_scalar,
            Some(1.0)
        );
    }

    #[test]
    fn with_ffb_scalar_rejects_non_finite_values() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let telemetry = NormalizedTelemetry::new().with_ffb_scalar(value);
            assert!(telemetry.ffb_scalar.is_none());
            assert!(!telemetry.has_ffb_data());
        }
    }

    // ── with_rpm ────────────────────────────────────────────────────────

    #[test]
    fn with_rpm_rejects_negative_value() {
        let t = NormalizedTelemetry::new().with_rpm(-100.0);
        assert!(t.rpm.is_none());
    }

    #[test]
    fn with_rpm_accepts_valid_value() {
        let t = NormalizedTelemetry::new().with_rpm(7000.0);
        assert_eq!(t.rpm, Some(7000.0));
    }

    #[test]
    fn with_rpm_accepts_zero() {
        let t = NormalizedTelemetry::new().with_rpm(0.0);
        assert_eq!(t.rpm, Some(0.0));
    }

    #[test]
    fn with_rpm_rejects_nan() {
        let t = NormalizedTelemetry::new().with_rpm(f32::NAN);
        assert!(t.rpm.is_none());
    }

    #[test]
    fn with_rpm_rejects_infinity() {
        let t = NormalizedTelemetry::new().with_rpm(f32::INFINITY);
        assert!(t.rpm.is_none());
    }

    // ── with_speed_ms ───────────────────────────────────────────────────

    #[test]
    fn with_speed_ms_rejects_negative() {
        let t = NormalizedTelemetry::new().with_speed_ms(-10.0);
        assert!(t.speed_ms.is_none());
    }

    #[test]
    fn with_speed_ms_accepts_valid() {
        let t = NormalizedTelemetry::new().with_speed_ms(50.0);
        assert_eq!(t.speed_ms, Some(50.0));
    }

    #[test]
    fn with_speed_ms_accepts_zero() {
        let t = NormalizedTelemetry::new().with_speed_ms(0.0);
        assert_eq!(t.speed_ms, Some(0.0));
    }

    #[test]
    fn with_speed_ms_rejects_nan() {
        let t = NormalizedTelemetry::new().with_speed_ms(f32::NAN);
        assert!(t.speed_ms.is_none());
    }

    #[test]
    fn with_speed_ms_rejects_infinity() {
        let t = NormalizedTelemetry::new().with_speed_ms(f32::INFINITY);
        assert!(t.speed_ms.is_none());
    }

    // ── with_slip_ratio ─────────────────────────────────────────────────

    #[test]
    fn with_slip_ratio_clamps_to_zero_one() {
        let t = NormalizedTelemetry::new().with_slip_ratio(1.5);
        assert_eq!(t.slip_ratio, Some(1.0));

        let t = NormalizedTelemetry::new().with_slip_ratio(-0.5);
        assert_eq!(t.slip_ratio, Some(0.0));
    }

    #[test]
    fn with_slip_ratio_preserves_valid() {
        let t = NormalizedTelemetry::new().with_slip_ratio(0.3);
        assert_eq!(t.slip_ratio, Some(0.3));
    }

    #[test]
    fn with_slip_ratio_boundary_values() {
        assert_eq!(
            NormalizedTelemetry::new().with_slip_ratio(0.0).slip_ratio,
            Some(0.0)
        );
        assert_eq!(
            NormalizedTelemetry::new().with_slip_ratio(1.0).slip_ratio,
            Some(1.0)
        );
    }

    #[test]
    fn with_slip_ratio_rejects_nan() {
        let t = NormalizedTelemetry::new().with_slip_ratio(f32::NAN);
        assert!(t.slip_ratio.is_none());
    }

    #[test]
    fn with_slip_ratio_rejects_infinity() {
        let t = NormalizedTelemetry::new().with_slip_ratio(f32::INFINITY);
        assert!(t.slip_ratio.is_none());
    }

    // ── with_gear ───────────────────────────────────────────────────────

    #[test]
    fn with_gear_reverse() {
        let t = NormalizedTelemetry::new().with_gear(-1);
        assert_eq!(t.gear, Some(-1));
    }

    #[test]
    fn with_gear_neutral() {
        let t = NormalizedTelemetry::new().with_gear(0);
        assert_eq!(t.gear, Some(0));
    }

    #[test]
    fn with_gear_forward() {
        let t = NormalizedTelemetry::new().with_gear(6);
        assert_eq!(t.gear, Some(6));
    }

    // ── with_car_id / with_track_id ─────────────────────────────────────

    #[test]
    fn with_car_id_rejects_empty_string() {
        let t = NormalizedTelemetry::new().with_car_id(String::new());
        assert!(t.car_id.is_none());
    }

    #[test]
    fn with_car_id_accepts_non_empty() {
        let t = NormalizedTelemetry::new().with_car_id("bmw_m3_gt2".to_string());
        assert_eq!(t.car_id.as_deref(), Some("bmw_m3_gt2"));
    }

    #[test]
    fn with_track_id_rejects_empty_string() {
        let t = NormalizedTelemetry::new().with_track_id(String::new());
        assert!(t.track_id.is_none());
    }

    #[test]
    fn with_track_id_accepts_non_empty() {
        let t = NormalizedTelemetry::new().with_track_id("spa_francorchamps".to_string());
        assert_eq!(t.track_id.as_deref(), Some("spa_francorchamps"));
    }

    // ── with_extended ───────────────────────────────────────────────────

    #[test]
    fn with_extended_inserts_values() {
        let t = NormalizedTelemetry::new()
            .with_extended("brake_temp".to_string(), TelemetryValue::Float(350.0))
            .with_extended("lap".to_string(), TelemetryValue::Integer(5));
        assert_eq!(t.extended.len(), 2);
        assert_eq!(
            t.extended.get("brake_temp"),
            Some(&TelemetryValue::Float(350.0))
        );
        assert_eq!(t.extended.get("lap"), Some(&TelemetryValue::Integer(5)));
    }

    #[test]
    fn with_extended_overwrites_same_key() {
        let t = NormalizedTelemetry::new()
            .with_extended("key".to_string(), TelemetryValue::Integer(1))
            .with_extended("key".to_string(), TelemetryValue::Integer(2));
        assert_eq!(t.extended.len(), 1);
        assert_eq!(t.extended.get("key"), Some(&TelemetryValue::Integer(2)));
    }

    // ── has_ffb_data / has_rpm_data ─────────────────────────────────────

    #[test]
    fn has_ffb_data_false_when_none() {
        assert!(!NormalizedTelemetry::new().has_ffb_data());
    }

    #[test]
    fn has_ffb_data_true_when_set() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(0.0);
        assert!(t.has_ffb_data());
    }

    #[test]
    fn has_rpm_data_false_when_none() {
        assert!(!NormalizedTelemetry::new().has_rpm_data());
    }

    #[test]
    fn has_rpm_data_true_when_set() {
        let t = NormalizedTelemetry::new().with_rpm(1000.0);
        assert!(t.has_rpm_data());
    }

    // ── rpm_fraction ────────────────────────────────────────────────────

    #[test]
    fn rpm_fraction_returns_none_without_rpm() {
        let t = NormalizedTelemetry::new();
        assert!(t.rpm_fraction(8000.0).is_none());
    }

    #[test]
    fn rpm_fraction_computes_correctly() {
        let t = NormalizedTelemetry::new().with_rpm(4000.0);
        let frac = t.rpm_fraction(8000.0);
        assert!(frac.is_some());
        assert!((frac.unwrap_or(0.0) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn rpm_fraction_clamps_to_one() {
        let t = NormalizedTelemetry::new().with_rpm(10000.0);
        let frac = t.rpm_fraction(8000.0);
        assert_eq!(frac, Some(1.0));
    }

    #[test]
    fn rpm_fraction_rejects_invalid_redlines() {
        let telemetry = NormalizedTelemetry::new().with_rpm(4000.0);
        for redline in [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert!(
                telemetry.rpm_fraction(redline).is_none(),
                "invalid redline {redline:?} produced a fraction"
            );
        }
    }

    #[test]
    fn rpm_fraction_preserves_positive_finite_boundaries() {
        let zero = NormalizedTelemetry::new().with_rpm(0.0);
        assert_eq!(zero.rpm_fraction(8000.0), Some(0.0));

        let at_redline = NormalizedTelemetry::new().with_rpm(8000.0);
        assert_eq!(at_redline.rpm_fraction(8000.0), Some(1.0));

        let above = NormalizedTelemetry::new().with_rpm(9000.0);
        assert_eq!(above.rpm_fraction(8000.0), Some(1.0));
    }

    // ── speed conversions ───────────────────────────────────────────────

    #[test]
    fn speed_kmh_and_mph_conversions() {
        let t = NormalizedTelemetry::new().with_speed_ms(10.0);
        let kmh = t.speed_kmh();
        let mph = t.speed_mph();
        assert!(kmh.is_some());
        assert!(mph.is_some());
        assert!((kmh.unwrap_or(0.0) - 36.0).abs() < 0.01, "expected 36 km/h");
        assert!(
            (mph.unwrap_or(0.0) - 22.37).abs() < 0.01,
            "expected ~22.37 mph"
        );
    }

    #[test]
    fn speed_kmh_returns_none_without_speed() {
        assert!(NormalizedTelemetry::new().speed_kmh().is_none());
    }

    #[test]
    fn speed_mph_returns_none_without_speed() {
        assert!(NormalizedTelemetry::new().speed_mph().is_none());
    }

    #[test]
    fn speed_zero_converts_to_zero() {
        let t = NormalizedTelemetry::new().with_speed_ms(0.0);
        assert_eq!(t.speed_kmh(), Some(0.0));
        assert_eq!(t.speed_mph(), Some(0.0));
    }

    // ── TelemetryFlags ──────────────────────────────────────────────────

    #[test]
    fn telemetry_flags_default_has_green_flag_true() {
        let flags = TelemetryFlags::default();
        assert!(flags.green_flag);
        assert!(!flags.yellow_flag);
        assert!(!flags.checkered_flag);
    }

    #[test]
    fn telemetry_flags_default_all_assist_flags_false() {
        let flags = TelemetryFlags::default();
        assert!(!flags.pit_limiter);
        assert!(!flags.in_pits);
        assert!(!flags.drs_available);
        assert!(!flags.drs_active);
        assert!(!flags.ers_available);
        assert!(!flags.launch_control);
        assert!(!flags.traction_control);
        assert!(!flags.abs_active);
    }

    // ── has_active_flags ────────────────────────────────────────────────

    #[test]
    fn has_active_flags_returns_false_for_default() {
        let t = NormalizedTelemetry::new();
        assert!(!t.has_active_flags());
    }

    #[test]
    fn has_active_flags_returns_true_for_yellow() {
        let flags = TelemetryFlags {
            yellow_flag: true,
            ..TelemetryFlags::default()
        };
        let t = NormalizedTelemetry::new().with_flags(flags);
        assert!(t.has_active_flags());
    }

    #[test]
    fn has_active_flags_returns_true_for_red() {
        let flags = TelemetryFlags {
            red_flag: true,
            ..TelemetryFlags::default()
        };
        assert!(
            NormalizedTelemetry::new()
                .with_flags(flags)
                .has_active_flags()
        );
    }

    #[test]
    fn has_active_flags_returns_true_for_blue() {
        let flags = TelemetryFlags {
            blue_flag: true,
            ..TelemetryFlags::default()
        };
        assert!(
            NormalizedTelemetry::new()
                .with_flags(flags)
                .has_active_flags()
        );
    }

    #[test]
    fn has_active_flags_returns_true_for_checkered() {
        let flags = TelemetryFlags {
            checkered_flag: true,
            ..TelemetryFlags::default()
        };
        assert!(
            NormalizedTelemetry::new()
                .with_flags(flags)
                .has_active_flags()
        );
    }

    #[test]
    fn has_active_flags_ignores_green_flag() {
        // green_flag=true by default but should not trigger has_active_flags
        let t = NormalizedTelemetry::new();
        assert!(t.flags.green_flag);
        assert!(!t.has_active_flags());
    }

    #[test]
    fn has_active_flags_ignores_assist_flags() {
        let flags = TelemetryFlags {
            pit_limiter: true,
            drs_active: true,
            abs_active: true,
            traction_control: true,
            ..TelemetryFlags::default()
        };
        assert!(
            !NormalizedTelemetry::new()
                .with_flags(flags)
                .has_active_flags()
        );
    }

    // ── TelemetryFrame ──────────────────────────────────────────────────

    #[test]
    fn telemetry_frame_new_stores_fields() {
        let data = NormalizedTelemetry::new().with_rpm(5000.0);
        let frame = TelemetryFrame::new(data.clone(), 123_456_789, 42, 64);
        assert_eq!(frame.timestamp_ns, 123_456_789);
        assert_eq!(frame.sequence, 42);
        assert_eq!(frame.raw_size, 64);
        assert_eq!(frame.data.rpm, Some(5000.0));
    }

    // ── TelemetryValue ──────────────────────────────────────────────────

    #[test]
    fn telemetry_value_variants_are_distinct() {
        let f = TelemetryValue::Float(1.0);
        let i = TelemetryValue::Integer(1);
        let b = TelemetryValue::Boolean(true);
        let s = TelemetryValue::String("x".to_string());
        assert_ne!(f, i);
        assert_ne!(b, s);
    }

    #[test]
    fn telemetry_value_equality() {
        assert_eq!(TelemetryValue::Float(1.0), TelemetryValue::Float(1.0));
        assert_eq!(TelemetryValue::Integer(42), TelemetryValue::Integer(42));
        assert_eq!(TelemetryValue::Boolean(true), TelemetryValue::Boolean(true));
        assert_eq!(
            TelemetryValue::String("abc".into()),
            TelemetryValue::String("abc".into())
        );
    }

    // ── Builder chaining ────────────────────────────────────────────────

    #[test]
    fn builder_chaining_produces_fully_populated_telemetry() {
        let t = NormalizedTelemetry::new()
            .with_ffb_scalar(0.8)
            .with_rpm(6000.0)
            .with_speed_ms(55.0)
            .with_slip_ratio(0.15)
            .with_gear(4)
            .with_car_id("porsche_911_gt3".to_string())
            .with_track_id("nurburgring_gp".to_string())
            .with_extended("fuel".to_string(), TelemetryValue::Float(42.5));
        assert_eq!(t.ffb_scalar, Some(0.8));
        assert_eq!(t.rpm, Some(6000.0));
        assert_eq!(t.speed_ms, Some(55.0));
        assert_eq!(t.slip_ratio, Some(0.15));
        assert_eq!(t.gear, Some(4));
        assert_eq!(t.car_id.as_deref(), Some("porsche_911_gt3"));
        assert_eq!(t.track_id.as_deref(), Some("nurburgring_gp"));
        assert_eq!(t.extended.len(), 1);
    }

    // ── Clone / PartialEq ───────────────────────────────────────────────

    #[test]
    fn normalized_telemetry_clone_equals_original() {
        let t = NormalizedTelemetry::new()
            .with_ffb_scalar(0.5)
            .with_rpm(3000.0)
            .with_gear(2);
        let cloned = t.clone();
        assert_eq!(t, cloned);
    }

    #[test]
    fn telemetry_flags_clone_equals_original() {
        let flags = TelemetryFlags {
            yellow_flag: true,
            drs_active: true,
            ..TelemetryFlags::default()
        };
        assert_eq!(flags, flags.clone());
    }

    // ── Serde round-trips ───────────────────────────────────────────────

    #[test]
    fn normalized_telemetry_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let t = NormalizedTelemetry::new()
            .with_ffb_scalar(0.75)
            .with_rpm(6500.0)
            .with_speed_ms(50.0)
            .with_gear(3);
        let json = serde_json::to_string(&t)?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
        assert_eq!(decoded.ffb_scalar, t.ffb_scalar);
        assert_eq!(decoded.rpm, t.rpm);
        assert_eq!(decoded.speed_ms, t.speed_ms);
        assert_eq!(decoded.gear, t.gear);
        Ok(())
    }

    #[test]
    fn normalized_telemetry_serde_full_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let t = NormalizedTelemetry::new()
            .with_ffb_scalar(-0.3)
            .with_rpm(4500.0)
            .with_speed_ms(30.0)
            .with_slip_ratio(0.2)
            .with_gear(3)
            .with_car_id("test_car".to_string())
            .with_track_id("test_track".to_string())
            .with_flags(TelemetryFlags {
                yellow_flag: true,
                drs_active: true,
                ..TelemetryFlags::default()
            })
            .with_extended("temp".to_string(), TelemetryValue::Float(95.0))
            .with_extended("valid".to_string(), TelemetryValue::Boolean(true));
        let json = serde_json::to_string(&t)?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
        assert_eq!(t, decoded);
        Ok(())
    }

    #[test]
    fn telemetry_flags_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let flags = TelemetryFlags {
            yellow_flag: true,
            blue_flag: true,
            pit_limiter: true,
            abs_active: true,
            ..TelemetryFlags::default()
        };
        let json = serde_json::to_string(&flags)?;
        let decoded: TelemetryFlags = serde_json::from_str(&json)?;
        assert_eq!(flags, decoded);
        Ok(())
    }

    #[test]
    fn telemetry_frame_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::new().with_rpm(3000.0).with_gear(2),
            1_000_000_000,
            99,
            128,
        );
        let json = serde_json::to_string(&frame)?;
        let decoded: TelemetryFrame = serde_json::from_str(&json)?;
        assert_eq!(decoded.timestamp_ns, frame.timestamp_ns);
        assert_eq!(decoded.sequence, frame.sequence);
        assert_eq!(decoded.raw_size, frame.raw_size);
        assert_eq!(decoded.data, frame.data);
        Ok(())
    }

    #[test]
    fn telemetry_value_serde_all_variants() -> Result<(), Box<dyn std::error::Error>> {
        let variants = vec![
            TelemetryValue::Float(3.125),
            TelemetryValue::Integer(-42),
            TelemetryValue::Boolean(false),
            TelemetryValue::String("hello".to_string()),
        ];
        for v in &variants {
            let json = serde_json::to_string(v)?;
            let decoded: TelemetryValue = serde_json::from_str(&json)?;
            assert_eq!(&decoded, v);
        }
        Ok(())
    }

    #[test]
    fn telemetry_field_coverage_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let coverage = TelemetryFieldCoverage {
            game_id: "test_game".to_string(),
            game_version: "1.0".to_string(),
            ffb_scalar: true,
            rpm: true,
            speed: true,
            slip_ratio: false,
            gear: true,
            flags: FlagCoverage {
                yellow_flag: true,
                red_flag: true,
                blue_flag: false,
                checkered_flag: true,
                green_flag: true,
                pit_limiter: false,
                in_pits: true,
                drs_available: false,
                drs_active: false,
                ers_available: false,
                launch_control: false,
                traction_control: false,
                abs_active: true,
            },
            car_id: true,
            track_id: false,
            extended_fields: vec!["fuel".to_string(), "tire_temp".to_string()],
        };
        let json = serde_json::to_string(&coverage)?;
        let decoded: TelemetryFieldCoverage = serde_json::from_str(&json)?;
        assert_eq!(decoded.game_id, coverage.game_id);
        assert_eq!(decoded.game_version, coverage.game_version);
        assert_eq!(decoded.ffb_scalar, coverage.ffb_scalar);
        assert_eq!(decoded.slip_ratio, coverage.slip_ratio);
        assert_eq!(decoded.flags.yellow_flag, coverage.flags.yellow_flag);
        assert_eq!(decoded.flags.abs_active, coverage.flags.abs_active);
        assert_eq!(decoded.extended_fields, coverage.extended_fields);
        Ok(())
    }

    // ── Default NormalizedTelemetry serde (empty fields) ────────────────

    #[test]
    fn default_telemetry_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let t = NormalizedTelemetry::default();
        let json = serde_json::to_string(&t)?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
        assert_eq!(t, decoded);
        Ok(())
    }
}
