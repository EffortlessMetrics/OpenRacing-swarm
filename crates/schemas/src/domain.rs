//! Core domain types and value objects with unit safety
//!
//! This module contains the pure domain types that form the core of the racing wheel
//! software. These types enforce unit safety and business rules at the type level.

use openracing_errors::{ProfileError, ValidationError};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Domain errors for value object validation
#[derive(Error, Debug, Clone, PartialEq)]
pub enum DomainError {
    #[error("Invalid torque value: {0} Nm (must be >= 0 and <= {1} Nm)")]
    InvalidTorque(f32, f32),

    #[error("Invalid degrees value: {0}° (must be >= {1}° and <= {2}°)")]
    InvalidDegrees(f32, f32, f32),

    #[error("Invalid device ID: {0} (must be non-empty and alphanumeric with hyphens)")]
    InvalidDeviceId(String),

    #[error("Invalid profile ID: {0} (must be non-empty and valid identifier)")]
    InvalidProfileId(String),

    #[error("Invalid gain value: {0} (must be between 0.0 and 1.0)")]
    InvalidGain(f32),

    #[error("Invalid frequency: {0} Hz (must be > 0)")]
    InvalidFrequency(f32),

    #[error("Invalid curve points: {0}")]
    InvalidCurvePoints(String),

    #[error("Profile inheritance depth exceeded: {depth} levels (maximum is {max_depth})")]
    InheritanceDepthExceeded {
        /// The depth at which the limit was exceeded
        depth: usize,
        /// The maximum allowed depth
        max_depth: usize,
    },

    #[error(
        "Circular inheritance detected: profile '{profile_id}' appears multiple times in inheritance chain"
    )]
    CircularInheritance {
        /// The profile ID that was detected as circular
        profile_id: String,
    },

    #[error("Parent profile not found: '{profile_id}'")]
    ParentProfileNotFound {
        /// The profile ID that was not found
        profile_id: String,
    },
}

impl From<DomainError> for ValidationError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::InvalidTorque(v, max) => {
                ValidationError::out_of_range("torque", v, 0.0_f32, max)
            }
            DomainError::InvalidDegrees(v, min, max) => {
                ValidationError::out_of_range("degrees", v, min, max)
            }
            DomainError::InvalidDeviceId(id) => ValidationError::invalid_format(
                "device_id",
                format!(
                    "must be non-empty and alphanumeric with hyphens, got: {}",
                    id
                ),
            ),
            DomainError::InvalidProfileId(id) => ValidationError::invalid_format(
                "profile_id",
                format!("must be non-empty and valid identifier, got: {}", id),
            ),
            DomainError::InvalidGain(v) => {
                ValidationError::out_of_range("gain", v, 0.0_f32, 1.0_f32)
            }
            DomainError::InvalidFrequency(v) => {
                ValidationError::out_of_range("frequency", v, 0.0_f32, f32::MAX)
            }
            DomainError::InvalidCurvePoints(msg) => {
                ValidationError::invalid_format("curve_points", msg)
            }
            DomainError::InheritanceDepthExceeded { depth, max_depth } => ValidationError::custom(
                format!("inheritance depth exceeded: {} > {}", depth, max_depth),
            ),
            DomainError::CircularInheritance { profile_id } => {
                ValidationError::custom(format!("circular inheritance: {}", profile_id))
            }
            DomainError::ParentProfileNotFound { profile_id } => {
                ValidationError::custom(format!("parent not found: {}", profile_id))
            }
        }
    }
}

impl From<DomainError> for ProfileError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::InheritanceDepthExceeded { depth, max_depth } => {
                ProfileError::InheritanceDepthExceeded { depth, max_depth }
            }
            DomainError::CircularInheritance { profile_id } => {
                ProfileError::circular_inheritance(&profile_id)
            }
            DomainError::ParentProfileNotFound { profile_id } => ProfileError::ParentNotFound {
                parent_id: profile_id,
            },
            DomainError::InvalidProfileId(id) => ProfileError::InvalidId(id),
            other => ProfileError::ValidationFailed(other.to_string()),
        }
    }
}

/// Torque value in Newton-meters with validation
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::TorqueNm;
///
/// // Create a valid torque value
/// let torque = TorqueNm::new(12.5)?;
/// assert!((torque.value() - 12.5).abs() < f32::EPSILON);
///
/// // Convert to/from centi-Newton-meters (HID reports)
/// let cnm = torque.to_cnm();
/// assert_eq!(cnm, 1250);
///
/// // Invalid values are rejected
/// assert!(TorqueNm::new(-1.0).is_err());
/// assert!(TorqueNm::new(51.0).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TorqueNm(f32);

impl TorqueNm {
    /// Maximum reasonable torque for racing wheels (50 Nm)
    pub const MAX_TORQUE: f32 = 50.0;

    /// Create a new torque value with validation
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if !Self::is_valid_raw_value(value) {
            return Err(DomainError::InvalidTorque(value, Self::MAX_TORQUE));
        }
        Ok(TorqueNm(value))
    }

    fn is_valid_raw_value(value: f32) -> bool {
        (0.0..=Self::MAX_TORQUE).contains(&value) && value.is_finite()
    }

    /// Create a new torque value without validation
    ///
    /// # Examples
    ///
    /// ```
    /// use racing_wheel_schemas::domain::TorqueNm;
    ///
    /// // SAFETY: 25.0 is finite and in the allowed [0.0, MAX_TORQUE] range.
    /// let torque = unsafe { TorqueNm::new_unchecked(25.0) };
    /// assert_eq!(torque.value(), 25.0);
    /// ```
    ///
    /// # Safety
    ///
    /// The caller must ensure that the value is finite and within the range [0.0, MAX_TORQUE].
    pub unsafe fn new_unchecked(value: f32) -> Self {
        debug_assert!(
            Self::is_valid_raw_value(value),
            "TorqueNm::new_unchecked requires a finite value in [0.0, MAX_TORQUE]"
        );
        TorqueNm(value)
    }

    /// Create torque from centi-Newton-meters (used in HID reports)
    pub fn from_cnm(cnm: u16) -> Result<Self, DomainError> {
        let nm = (cnm as f32) / 100.0;
        Self::new(nm)
    }

    /// Convert to centi-Newton-meters for HID reports
    pub fn to_cnm(self) -> u16 {
        (self.0 * 100.0).round() as u16
    }

    /// Get the raw value in Newton-meters
    pub fn value(self) -> f32 {
        self.0
    }

    /// Zero torque constant
    pub const ZERO: TorqueNm = TorqueNm(0.0);
}

impl fmt::Display for TorqueNm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} Nm", self.0)
    }
}

impl std::ops::Add for TorqueNm {
    type Output = TorqueNm;

    fn add(self, rhs: Self) -> Self::Output {
        TorqueNm((self.0 + rhs.0).min(Self::MAX_TORQUE))
    }
}

impl std::ops::Sub for TorqueNm {
    type Output = TorqueNm;

    fn sub(self, rhs: Self) -> Self::Output {
        TorqueNm((self.0 - rhs.0).max(0.0))
    }
}

impl std::ops::Mul<f32> for TorqueNm {
    type Output = TorqueNm;

    fn mul(self, rhs: f32) -> Self::Output {
        TorqueNm((self.0 * rhs).clamp(0.0, Self::MAX_TORQUE))
    }
}

impl PartialOrd for TorqueNm {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TorqueNm {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Eq for TorqueNm {}

impl TorqueNm {
    /// Return the minimum of two torque values
    pub fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }

    /// Return the maximum of two torque values
    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }
}

/// Angle value in degrees with validation
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::Degrees;
///
/// // Create a Degrees of Rotation (DOR) value (180°–2160°)
/// let dor = Degrees::new_dor(900.0)?;
/// assert!((dor.value() - 900.0).abs() < f32::EPSILON);
///
/// // Create an unrestricted angle
/// let angle = Degrees::new_angle(45.0)?;
/// assert!((angle.to_radians() - std::f32::consts::FRAC_PI_4).abs() < 0.001);
///
/// // Normalize angles to [-180, 180]
/// let wide = Degrees::new_angle(270.0)?;
/// assert!((wide.normalize().value() - (-90.0)).abs() < f32::EPSILON);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Degrees(f32);

impl Degrees {
    /// Minimum degrees of rotation (typically 180°)
    pub const MIN_DOR: f32 = 180.0;

    /// Maximum degrees of rotation (typically 2160°)
    pub const MAX_DOR: f32 = 2160.0;

    /// Create a new degrees value with validation for DOR (Degrees of Rotation)
    pub fn new_dor(value: f32) -> Result<Self, DomainError> {
        if !(Self::MIN_DOR..=Self::MAX_DOR).contains(&value) || !value.is_finite() {
            return Err(DomainError::InvalidDegrees(
                value,
                Self::MIN_DOR,
                Self::MAX_DOR,
            ));
        }
        Ok(Degrees(value))
    }

    /// Create a new degrees value for angles (unrestricted range)
    pub fn new_angle(value: f32) -> Result<Self, DomainError> {
        if !value.is_finite() {
            return Err(DomainError::InvalidDegrees(
                value,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ));
        }
        Ok(Degrees(value))
    }

    /// Create degrees from millidegrees (used in HID reports)
    pub fn from_millidegrees(mdeg: i32) -> Self {
        Degrees((mdeg as f32) / 1000.0)
    }

    /// Convert to millidegrees for HID reports
    pub fn to_millidegrees(self) -> i32 {
        (self.0 * 1000.0).round() as i32
    }

    /// Get the raw value in degrees
    pub fn value(self) -> f32 {
        self.0
    }

    /// Convert to radians
    pub fn to_radians(self) -> f32 {
        self.0.to_radians()
    }

    /// Create from radians
    pub fn from_radians(rad: f32) -> Self {
        Degrees(rad.to_degrees())
    }

    /// Normalize angle to [-180, 180] range
    pub fn normalize(self) -> Self {
        let mut angle = self.0 % 360.0;
        if angle > 180.0 {
            angle -= 360.0;
        } else if angle < -180.0 {
            angle += 360.0;
        }
        Degrees(angle)
    }

    /// Zero degrees constant
    pub const ZERO: Degrees = Degrees(0.0);
}

impl fmt::Display for Degrees {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}°", self.0)
    }
}

impl std::ops::Add for Degrees {
    type Output = Degrees;

    fn add(self, rhs: Self) -> Self::Output {
        Degrees(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Degrees {
    type Output = Degrees;

    fn sub(self, rhs: Self) -> Self::Output {
        Degrees(self.0 - rhs.0)
    }
}

/// Device identifier with validation and normalization
///
/// DeviceId enforces safe construction through validation only.
/// All construction must go through FromStr or TryFrom to ensure
/// proper normalization (trim, lowercase) and validation.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::DeviceId;
///
/// // Parse and normalize a device ID
/// let id: DeviceId = "Moza-R9".parse()?;
/// assert_eq!(id.as_str(), "moza-r9");
///
/// // Also available via constructor
/// let id2 = DeviceId::new("SimuCube-2".to_string())?;
/// assert_eq!(id2.as_str(), "simucube-2");
///
/// // Invalid IDs are rejected
/// assert!("".parse::<DeviceId>().is_err());
/// assert!("has spaces".parse::<DeviceId>().is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    /// Create a new DeviceId with validation
    pub fn new(value: String) -> Result<Self, DomainError> {
        value.parse()
    }

    /// Get the raw string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for DeviceId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Normalize: trim whitespace and convert to lowercase
        let normalized = s.trim().to_lowercase();

        if normalized.is_empty() {
            return Err(DomainError::InvalidDeviceId(s.to_string()));
        }

        // Validate that ID contains only alphanumeric characters, hyphens, and underscores
        if !normalized
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(DomainError::InvalidDeviceId(s.to_string()));
        }

        Ok(DeviceId(normalized))
    }
}

impl TryFrom<String> for DeviceId {
    type Error = DomainError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for DeviceId {
    type Error = DomainError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<DeviceId> for String {
    fn from(id: DeviceId) -> String {
        id.0
    }
}

/// Profile identifier with validation and normalization
///
/// ProfileId enforces safe construction through validation only.
/// All construction must go through FromStr or TryFrom to ensure
/// proper normalization (trim, lowercase) and validation.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::ProfileId;
///
/// // Parse and normalize a profile ID
/// let id: ProfileId = "iRacing.GT3".parse()?;
/// assert_eq!(id.as_str(), "iracing.gt3");
///
/// // Dots, hyphens, and underscores are allowed
/// let id2: ProfileId = "profile-v2_test".parse()?;
/// assert_eq!(id2.as_str(), "profile-v2_test");
///
/// // Invalid IDs are rejected
/// assert!("".parse::<ProfileId>().is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(String);

impl ProfileId {
    /// Create a new ProfileId with validation
    pub fn new(value: String) -> Result<Self, DomainError> {
        value.parse()
    }

    /// Get the raw string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for ProfileId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Normalize: trim whitespace and convert to lowercase
        let normalized = s.trim().to_lowercase();

        if normalized.is_empty() {
            return Err(DomainError::InvalidProfileId(s.to_string()));
        }

        // Validate that ID is a reasonable identifier
        if !normalized
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(DomainError::InvalidProfileId(s.to_string()));
        }

        Ok(ProfileId(normalized))
    }
}

impl TryFrom<String> for ProfileId {
    type Error = DomainError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for ProfileId {
    type Error = DomainError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<ProfileId> for String {
    fn from(id: ProfileId) -> String {
        id.0
    }
}

/// Gain value (0.0 to 1.0) with validation
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::Gain;
///
/// let gain = Gain::new(0.8)?;
/// assert!((gain.value() - 0.8).abs() < f32::EPSILON);
///
/// // Gain can scale values via multiplication
/// let scaled = gain * 100.0;
/// assert!((scaled - 80.0).abs() < f32::EPSILON);
///
/// // Out-of-range values are rejected
/// assert!(Gain::new(1.5).is_err());
/// assert!(Gain::new(-0.1).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Gain(f32);

impl Gain {
    /// Create a new gain value with validation
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if !(0.0..=1.0).contains(&value) || !value.is_finite() {
            return Err(DomainError::InvalidGain(value));
        }
        Ok(Gain(value))
    }

    /// Get the raw value
    pub fn value(self) -> f32 {
        self.0
    }

    /// Zero gain constant
    pub const ZERO: Gain = Gain(0.0);

    /// Full gain constant
    pub const FULL: Gain = Gain(1.0);
}

impl fmt::Display for Gain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}%", self.0 * 100.0)
    }
}

impl std::ops::Mul<f32> for Gain {
    type Output = f32;

    fn mul(self, rhs: f32) -> Self::Output {
        self.0 * rhs
    }
}

impl std::ops::Mul<Gain> for f32 {
    type Output = f32;

    fn mul(self, rhs: Gain) -> Self::Output {
        self * rhs.0
    }
}

/// Frequency value in Hz with validation
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::FrequencyHz;
///
/// let freq = FrequencyHz::new(1000.0)?;
/// assert!((freq.value() - 1000.0).abs() < f32::EPSILON);
/// assert_eq!(freq.to_string(), "1000.0 Hz");
///
/// // Zero and negative frequencies are rejected
/// assert!(FrequencyHz::new(0.0).is_err());
/// assert!(FrequencyHz::new(-50.0).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FrequencyHz(f32);

impl FrequencyHz {
    /// Create a new frequency value with validation
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if value <= 0.0 || !value.is_finite() {
            return Err(DomainError::InvalidFrequency(value));
        }
        Ok(FrequencyHz(value))
    }

    /// Get the raw value in Hz
    pub fn value(self) -> f32 {
        self.0
    }
}

impl fmt::Display for FrequencyHz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} Hz", self.0)
    }
}

/// Curve point for force feedback curves
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::CurvePoint;
///
/// // Both input and output must be in [0.0, 1.0]
/// let point = CurvePoint::new(0.5, 0.7)?;
/// assert!((point.input - 0.5).abs() < f32::EPSILON);
/// assert!((point.output - 0.7).abs() < f32::EPSILON);
///
/// // Out-of-range values are rejected
/// assert!(CurvePoint::new(-0.1, 0.5).is_err());
/// assert!(CurvePoint::new(0.5, 1.1).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub input: f32,
    pub output: f32,
}

impl CurvePoint {
    /// Create a new curve point with validation
    pub fn new(input: f32, output: f32) -> Result<Self, DomainError> {
        if !input.is_finite() || !output.is_finite() {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Non-finite values: input={}, output={}",
                input, output
            )));
        }

        if !(0.0..=1.0).contains(&input) {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Input must be in [0,1] range: {}",
                input
            )));
        }

        if !(0.0..=1.0).contains(&output) {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Output must be in [0,1] range: {}",
                output
            )));
        }

        Ok(CurvePoint { input, output })
    }
}

/// Validate that curve points are monotonic
///
/// Ensures that the input values of successive curve points are strictly
/// increasing.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), racing_wheel_schemas::domain::DomainError> {
/// use racing_wheel_schemas::domain::{CurvePoint, validate_curve_monotonic};
///
/// let points = vec![
///     CurvePoint::new(0.0, 0.0)?,
///     CurvePoint::new(0.5, 0.6)?,
///     CurvePoint::new(1.0, 1.0)?,
/// ];
/// assert!(validate_curve_monotonic(&points).is_ok());
///
/// // Non-monotonic points are rejected
/// let bad = vec![
///     CurvePoint::new(0.0, 0.0)?,
///     CurvePoint::new(0.7, 0.6)?,
///     CurvePoint::new(0.5, 1.0)?,
/// ];
/// assert!(validate_curve_monotonic(&bad).is_err());
/// # Ok(())
/// # }
/// ```
pub fn validate_curve_monotonic(points: &[CurvePoint]) -> Result<(), DomainError> {
    if points.is_empty() {
        return Err(DomainError::InvalidCurvePoints("Empty curve".to_string()));
    }

    for window in points.windows(2) {
        if window[1].input <= window[0].input {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Non-monotonic input: {} <= {}",
                window[1].input, window[0].input
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn test_torque_nm_validation() {
        // Valid torque values
        assert!(TorqueNm::new(0.0).is_ok());
        assert!(TorqueNm::new(25.0).is_ok());
        assert!(TorqueNm::new(50.0).is_ok());

        // Invalid torque values
        assert!(TorqueNm::new(-1.0).is_err());
        assert!(TorqueNm::new(51.0).is_err());
        assert!(TorqueNm::new(f32::NAN).is_err());
        assert!(TorqueNm::new(f32::INFINITY).is_err());
    }

    #[test]
    fn test_torque_nm_raw_value_guard() {
        assert!(TorqueNm::is_valid_raw_value(0.0));
        assert!(TorqueNm::is_valid_raw_value(25.0));
        assert!(TorqueNm::is_valid_raw_value(TorqueNm::MAX_TORQUE));
        assert!(!TorqueNm::is_valid_raw_value(-1.0));
        assert!(!TorqueNm::is_valid_raw_value(TorqueNm::MAX_TORQUE + 1.0));
        assert!(!TorqueNm::is_valid_raw_value(f32::NAN));
        assert!(!TorqueNm::is_valid_raw_value(f32::INFINITY));
    }

    #[test]
    fn test_torque_nm_operations() -> Result<()> {
        let t1 = TorqueNm::new(10.0)?;
        let t2 = TorqueNm::new(15.0)?;

        // Addition with clamping
        let sum = t1 + t2;
        assert_eq!(sum.value(), 25.0);

        // Subtraction with clamping
        let diff = t2 - t1;
        assert_eq!(diff.value(), 5.0);

        // Multiplication with clamping
        let double = t1 * 2.0;
        assert_eq!(double.value(), 20.0);

        let clamped = t2 * 4.0;
        assert_eq!(clamped.value(), TorqueNm::MAX_TORQUE);
        Ok(())
    }

    #[test]
    fn test_torque_nm_ordering() -> Result<()> {
        let t1 = TorqueNm::new(10.0)?;
        let t2 = TorqueNm::new(20.0)?;

        assert!(t1 < t2);
        assert!(t2 > t1);
        assert_eq!(t1.min(t2), t1);
        assert_eq!(t1.max(t2), t2);
        Ok(())
    }

    #[test]
    fn test_torque_nm_cnm_conversion() -> Result<()> {
        let torque = TorqueNm::new(12.34)?;
        let cnm = torque.to_cnm();
        assert_eq!(cnm, 1234);

        let back = TorqueNm::from_cnm(cnm)?;
        assert!((back.value() - 12.34).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_degrees_validation() {
        // Valid DOR values
        assert!(Degrees::new_dor(180.0).is_ok());
        assert!(Degrees::new_dor(900.0).is_ok());
        assert!(Degrees::new_dor(2160.0).is_ok());

        // Invalid DOR values
        assert!(Degrees::new_dor(179.0).is_err());
        assert!(Degrees::new_dor(2161.0).is_err());
        assert!(Degrees::new_dor(f32::NAN).is_err());

        // Valid unrestricted angles
        assert!(Degrees::new_angle(0.0).is_ok());
        assert!(Degrees::new_angle(360.0).is_ok());
        assert!(Degrees::new_angle(-45.0).is_ok());

        // Invalid angle values
        assert!(Degrees::new_angle(f32::NAN).is_err());
    }

    #[test]
    fn test_degrees_operations() -> Result<()> {
        let d1 = Degrees::new_angle(45.0)?;
        let d2 = Degrees::new_angle(90.0)?;

        let sum = d1 + d2;
        assert_eq!(sum.value(), 135.0);

        let diff = d2 - d1;
        assert_eq!(diff.value(), 45.0);
        Ok(())
    }

    #[test]
    fn test_degrees_normalization() -> Result<()> {
        let d1 = Degrees::new_angle(270.0)?;
        let normalized = d1.normalize();
        assert_eq!(normalized.value(), -90.0);

        let d2 = Degrees::new_angle(-270.0)?;
        let normalized2 = d2.normalize();
        assert_eq!(normalized2.value(), 90.0);
        Ok(())
    }

    #[test]
    fn test_degrees_millidegrees_conversion() -> Result<()> {
        let degrees = Degrees::new_angle(123.456)?;
        let mdeg = degrees.to_millidegrees();
        assert_eq!(mdeg, 123456);

        let back = Degrees::from_millidegrees(mdeg);
        assert!((back.value() - 123.456).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_device_id_validation() -> Result<()> {
        // Valid device IDs
        assert!("device-123".parse::<DeviceId>().is_ok());
        assert!("wheel_base_1".parse::<DeviceId>().is_ok());
        assert!("ABC123".parse::<DeviceId>().is_ok());

        // Test normalization (trim and lowercase)
        let id: DeviceId = "  Device-123  ".parse()?;
        assert_eq!(id.as_str(), "device-123");

        let id2: DeviceId = "WHEEL_BASE_1".parse()?;
        assert_eq!(id2.as_str(), "wheel_base_1");

        // Invalid device IDs
        assert!("".parse::<DeviceId>().is_err());
        assert!("device with spaces".parse::<DeviceId>().is_err());
        assert!("device@special".parse::<DeviceId>().is_err());
        assert!("   ".parse::<DeviceId>().is_err()); // Only whitespace
        Ok(())
    }

    #[test]
    fn test_device_id_try_from() -> Result<()> {
        // Test TryFrom<String>
        let id = DeviceId::try_from("test-device".to_string())?;
        assert_eq!(id.as_str(), "test-device");

        // Test TryFrom<&str>
        let id2 = DeviceId::try_from("another-device")?;
        assert_eq!(id2.as_str(), "another-device");

        // Test AsRef<str>
        assert_eq!(id.as_ref(), "test-device");

        // Test Display
        assert_eq!(format!("{}", id), "test-device");
        Ok(())
    }

    #[test]
    fn test_profile_id_validation() -> Result<()> {
        // Valid profile IDs
        assert!("global".parse::<ProfileId>().is_ok());
        assert!("iracing.gt3".parse::<ProfileId>().is_ok());
        assert!("profile-123_v2".parse::<ProfileId>().is_ok());

        // Test normalization (trim and lowercase)
        let id: ProfileId = "  Global-Profile  ".parse()?;
        assert_eq!(id.as_str(), "global-profile");

        let id2: ProfileId = "IRACING.GT3".parse()?;
        assert_eq!(id2.as_str(), "iracing.gt3");

        // Invalid profile IDs
        assert!("".parse::<ProfileId>().is_err());
        assert!("profile with spaces".parse::<ProfileId>().is_err());
        assert!("profile@special".parse::<ProfileId>().is_err());
        assert!("   ".parse::<ProfileId>().is_err()); // Only whitespace
        Ok(())
    }

    #[test]
    fn test_profile_id_try_from() -> Result<()> {
        // Test TryFrom<String>
        let id = ProfileId::try_from("test-profile".to_string())?;
        assert_eq!(id.as_str(), "test-profile");

        // Test TryFrom<&str>
        let id2 = ProfileId::try_from("another.profile")?;
        assert_eq!(id2.as_str(), "another.profile");

        // Test AsRef<str>
        assert_eq!(id.as_ref(), "test-profile");

        // Test Display
        assert_eq!(format!("{}", id), "test-profile");
        Ok(())
    }

    #[test]
    fn test_gain_validation() {
        // Valid gain values
        assert!(Gain::new(0.0).is_ok());
        assert!(Gain::new(0.5).is_ok());
        assert!(Gain::new(1.0).is_ok());

        // Invalid gain values
        assert!(Gain::new(-0.1).is_err());
        assert!(Gain::new(1.1).is_err());
        assert!(Gain::new(f32::NAN).is_err());
    }

    #[test]
    fn test_gain_operations() -> Result<()> {
        let gain = Gain::new(0.8)?;

        let result1 = gain * 100.0;
        assert_eq!(result1, 80.0);

        let result2 = 50.0 * gain;
        assert_eq!(result2, 40.0);
        Ok(())
    }

    #[test]
    fn test_frequency_hz_validation() {
        // Valid frequencies
        assert!(FrequencyHz::new(1.0).is_ok());
        assert!(FrequencyHz::new(1000.0).is_ok());

        // Invalid frequencies
        assert!(FrequencyHz::new(0.0).is_err());
        assert!(FrequencyHz::new(-1.0).is_err());
        assert!(FrequencyHz::new(f32::NAN).is_err());
    }

    #[test]
    fn test_curve_point_validation() {
        // Valid curve points
        assert!(CurvePoint::new(0.0, 0.0).is_ok());
        assert!(CurvePoint::new(0.5, 0.7).is_ok());
        assert!(CurvePoint::new(1.0, 1.0).is_ok());

        // Invalid curve points
        assert!(CurvePoint::new(-0.1, 0.5).is_err());
        assert!(CurvePoint::new(0.5, 1.1).is_err());
        assert!(CurvePoint::new(f32::NAN, 0.5).is_err());
        assert!(CurvePoint::new(0.5, f32::NAN).is_err());
    }

    #[test]
    fn test_curve_monotonic_validation() -> Result<()> {
        // Valid monotonic curve
        let points = vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.6)?,
            CurvePoint::new(1.0, 1.0)?,
        ];
        assert!(validate_curve_monotonic(&points).is_ok());

        // Invalid non-monotonic curve
        let bad_points = vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.7, 0.6)?,
            CurvePoint::new(0.5, 1.0)?, // Input goes backwards
        ];
        assert!(validate_curve_monotonic(&bad_points).is_err());

        // Empty curve
        assert!(validate_curve_monotonic(&[]).is_err());
        Ok(())
    }
}
