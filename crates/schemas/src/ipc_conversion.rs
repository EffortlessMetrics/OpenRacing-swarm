//! IPC conversion layer between domain and wire types
//!
//! This module provides conversion implementations between domain types
//! and protobuf-generated wire types for IPC communication.

use crate::domain::{
    CurvePoint, Degrees, DeviceId, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm,
};
use crate::entities::{
    BaseSettings, Device, DeviceCapabilities, DeviceState, DeviceType, FilterConfig, HapticsConfig,
    LedConfig, NotchFilter, Profile, ProfileMetadata, ProfileScope,
};
use crate::telemetry::TelemetryData;
use std::collections::HashMap;
use thiserror::Error;

// Import generated protobuf types
use crate::generated::wheel::v1 as proto;

/// Conversion errors between domain and wire types
#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Domain validation error: {0}")]
    DomainError(#[from] DomainError),

    #[error("Invalid device type: {0}")]
    InvalidDeviceType(i32),

    #[error("Invalid device state: {0}")]
    InvalidDeviceState(i32),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unit conversion error: {0}")]
    UnitConversion(String),

    #[error("Range validation error: {field} value {value} is out of range [{min}, {max}]")]
    RangeValidation {
        field: String,
        value: f64,
        min: f64,
        max: f64,
    },
}

// Device conversions

impl TryFrom<proto::DeviceInfo> for Device {
    type Error = ConversionError;

    fn try_from(wire: proto::DeviceInfo) -> Result<Self, Self::Error> {
        let device_id: DeviceId = wire.id.parse().map_err(ConversionError::DomainError)?;

        let device_type = match wire.r#type {
            0 => DeviceType::Other,
            1 => DeviceType::WheelBase,
            2 => DeviceType::SteeringWheel,
            3 => DeviceType::Pedals,
            4 => DeviceType::Shifter,
            5 => DeviceType::Handbrake,
            6 => DeviceType::ButtonBox,
            _ => return Err(ConversionError::InvalidDeviceType(wire.r#type)),
        };

        let capabilities = wire
            .capabilities
            .ok_or_else(|| ConversionError::MissingField("capabilities".to_string()))?
            .try_into()?;

        let _state = match wire.state {
            0 => DeviceState::Disconnected,
            1 => DeviceState::Connected,
            2 => DeviceState::Active,
            3 => DeviceState::Faulted,
            4 => DeviceState::SafeMode,
            _ => return Err(ConversionError::InvalidDeviceState(wire.state)),
        };

        Ok(Device::new(device_id, wire.name, device_type, capabilities))
    }
}

impl From<Device> for proto::DeviceInfo {
    fn from(domain: Device) -> Self {
        Self {
            id: domain.id.to_string(),
            name: domain.name,
            r#type: match domain.device_type {
                DeviceType::Other => 0,
                DeviceType::WheelBase => 1,
                DeviceType::SteeringWheel => 2,
                DeviceType::Pedals => 3,
                DeviceType::Shifter => 4,
                DeviceType::Handbrake => 5,
                DeviceType::ButtonBox => 6,
            },
            capabilities: Some(domain.capabilities.into()),
            state: match domain.state {
                DeviceState::Disconnected => 0,
                DeviceState::Connected => 1,
                DeviceState::Active => 2,
                DeviceState::Faulted => 3,
                DeviceState::SafeMode => 4,
            },
            vendor_id: 0,
            product_id: 0,
        }
    }
}

impl TryFrom<proto::DeviceCapabilities> for DeviceCapabilities {
    type Error = ConversionError;

    fn try_from(wire: proto::DeviceCapabilities) -> Result<Self, Self::Error> {
        // Convert centi-Newton-meters to Newton-meters with validation
        let max_torque =
            TorqueNm::from_cnm(wire.max_torque_cnm as u16).map_err(ConversionError::DomainError)?;

        // Validate encoder CPR range (reasonable values: 1000-100000)
        if !(1000..=100000).contains(&wire.encoder_cpr) {
            return Err(ConversionError::RangeValidation {
                field: "encoder_cpr".to_string(),
                value: wire.encoder_cpr as f64,
                min: 1000.0,
                max: 100000.0,
            });
        }

        // Validate report period (1000us = 1kHz max, 100000us = 10Hz min)
        if !(1000..=100000).contains(&wire.min_report_period_us) {
            return Err(ConversionError::RangeValidation {
                field: "min_report_period_us".to_string(),
                value: wire.min_report_period_us as f64,
                min: 1000.0,
                max: 100000.0,
            });
        }

        Ok(DeviceCapabilities::new(
            wire.supports_pid,
            wire.supports_raw_torque_1khz,
            wire.supports_health_stream,
            wire.supports_led_bus,
            max_torque,
            wire.encoder_cpr as u16,
            wire.min_report_period_us as u16,
        ))
    }
}

impl From<DeviceCapabilities> for proto::DeviceCapabilities {
    fn from(domain: DeviceCapabilities) -> Self {
        Self {
            supports_pid: domain.supports_pid,
            supports_raw_torque_1khz: domain.supports_raw_torque_1khz,
            supports_health_stream: domain.supports_health_stream,
            supports_led_bus: domain.supports_led_bus,
            max_torque_cnm: domain.max_torque.to_cnm() as u32,
            encoder_cpr: domain.encoder_cpr as u32,
            min_report_period_us: domain.min_report_period_us as u32,
        }
    }
}

// Telemetry conversions with unit validation

impl TryFrom<proto::TelemetryData> for TelemetryData {
    type Error = ConversionError;

    fn try_from(wire: proto::TelemetryData) -> Result<Self, Self::Error> {
        // Convert millidegrees to degrees with validation
        let wheel_angle_deg = (wire.wheel_angle_mdeg as f32) / 1000.0;
        if !wheel_angle_deg.is_finite() {
            return Err(ConversionError::UnitConversion(format!(
                "Invalid wheel angle: {} mdeg",
                wire.wheel_angle_mdeg
            )));
        }

        // Convert milli-radians/s to radians/s with validation
        let wheel_speed_rad_s = (wire.wheel_speed_mrad_s as f32) / 1000.0;
        if !wheel_speed_rad_s.is_finite() {
            return Err(ConversionError::UnitConversion(format!(
                "Invalid wheel speed: {} mrad/s",
                wire.wheel_speed_mrad_s
            )));
        }

        // Validate temperature range (0-150°C for reasonable operation)
        if wire.temp_c > 150 {
            return Err(ConversionError::RangeValidation {
                field: "temperature_c".to_string(),
                value: wire.temp_c as f64,
                min: 0.0,
                max: 150.0,
            });
        }

        // Validate fault flags (8-bit value)
        if wire.faults > 255 {
            return Err(ConversionError::RangeValidation {
                field: "fault_flags".to_string(),
                value: wire.faults as f64,
                min: 0.0,
                max: 255.0,
            });
        }

        Ok(TelemetryData {
            wheel_angle_deg,
            wheel_speed_rad_s,
            temperature_c: wire.temp_c as u8,
            fault_flags: wire.faults as u8,
            hands_on: wire.hands_on,
            timestamp: 0, // Will be set by the service layer
        })
    }
}

impl From<TelemetryData> for proto::TelemetryData {
    fn from(domain: TelemetryData) -> Self {
        Self {
            wheel_angle_mdeg: (openracing_hid_common::math::safe_clamp(
                domain.wheel_angle_deg * 1000.0,
                i32::MIN as f32,
                i32::MAX as f32,
            ))
            .round() as i32,
            wheel_speed_mrad_s: (openracing_hid_common::math::safe_clamp(
                domain.wheel_speed_rad_s * 1000.0,
                i32::MIN as f32,
                i32::MAX as f32,
            ))
            .round() as i32,
            temp_c: domain.temperature_c as u32,
            faults: domain.fault_flags as u32,
            hands_on: domain.hands_on,
            sequence: 0, // Deprecated field, always 0
        }
    }
}

// Profile conversions

impl TryFrom<proto::Profile> for Profile {
    type Error = ConversionError;

    fn try_from(wire: proto::Profile) -> Result<Self, Self::Error> {
        let profile_id: ProfileId = "converted".parse().map_err(ConversionError::DomainError)?;

        let scope = wire
            .scope
            .ok_or_else(|| ConversionError::MissingField("scope".to_string()))?
            .try_into()?;

        let base_settings = wire
            .base
            .ok_or_else(|| ConversionError::MissingField("base".to_string()))?
            .try_into()?;

        let led_config = wire.leds.map(|led| led.try_into()).transpose()?;
        let haptics_config = wire.haptics.map(|haptics| haptics.try_into()).transpose()?;

        let metadata = ProfileMetadata {
            name: "Converted Profile".to_string(),
            description: None,
            author: None,
            version: "1.0.0".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            modified_at: chrono::Utc::now().to_rfc3339(),
            tags: Vec::new(),
        };

        Ok(Profile {
            id: profile_id,
            parent: None, // IPC profiles don't support inheritance yet
            scope,
            base_settings,
            led_config,
            haptics_config,
            metadata,
        })
    }
}

impl From<Profile> for proto::Profile {
    fn from(domain: Profile) -> Self {
        Self {
            schema_version: "wheel.profile/1".to_string(),
            scope: Some(domain.scope.into()),
            base: Some(domain.base_settings.into()),
            leds: domain.led_config.map(Into::into),
            haptics: domain.haptics_config.map(Into::into),
            signature: String::new(), // Will be computed by service layer
        }
    }
}

impl TryFrom<proto::ProfileScope> for ProfileScope {
    type Error = ConversionError;

    fn try_from(wire: proto::ProfileScope) -> Result<Self, Self::Error> {
        Ok(ProfileScope {
            game: if wire.game.is_empty() {
                None
            } else {
                Some(wire.game)
            },
            car: if wire.car.is_empty() {
                None
            } else {
                Some(wire.car)
            },
            track: if wire.track.is_empty() {
                None
            } else {
                Some(wire.track)
            },
        })
    }
}

impl From<ProfileScope> for proto::ProfileScope {
    fn from(domain: ProfileScope) -> Self {
        Self {
            game: domain.game.unwrap_or_default(),
            car: domain.car.unwrap_or_default(),
            track: domain.track.unwrap_or_default(),
        }
    }
}

impl TryFrom<proto::BaseSettings> for BaseSettings {
    type Error = ConversionError;

    fn try_from(wire: proto::BaseSettings) -> Result<Self, Self::Error> {
        let ffb_gain = Gain::new(wire.ffb_gain).map_err(ConversionError::DomainError)?;

        let degrees_of_rotation =
            Degrees::new_dor(wire.dor_deg as f32).map_err(ConversionError::DomainError)?;

        let torque_cap = TorqueNm::new(wire.torque_cap_nm).map_err(ConversionError::DomainError)?;

        let filters = wire
            .filters
            .ok_or_else(|| ConversionError::MissingField("filters".to_string()))?
            .try_into()?;

        Ok(BaseSettings::new(
            ffb_gain,
            degrees_of_rotation,
            torque_cap,
            filters,
        ))
    }
}

impl From<BaseSettings> for proto::BaseSettings {
    fn from(domain: BaseSettings) -> Self {
        Self {
            ffb_gain: domain.ffb_gain.value(),
            dor_deg: domain.degrees_of_rotation.value() as u32,
            torque_cap_nm: domain.torque_cap.value(),
            filters: Some(domain.filters.into()),
        }
    }
}

impl TryFrom<proto::FilterConfig> for FilterConfig {
    type Error = ConversionError;

    fn try_from(wire: proto::FilterConfig) -> Result<Self, Self::Error> {
        // Validate reconstruction level (0-8)
        if wire.reconstruction > 8 {
            return Err(ConversionError::RangeValidation {
                field: "reconstruction".to_string(),
                value: wire.reconstruction as f64,
                min: 0.0,
                max: 8.0,
            });
        }

        let friction = Gain::new(wire.friction).map_err(ConversionError::DomainError)?;
        let damper = Gain::new(wire.damper).map_err(ConversionError::DomainError)?;
        let inertia = Gain::new(wire.inertia).map_err(ConversionError::DomainError)?;
        let slew_rate = Gain::new(wire.slew_rate).map_err(ConversionError::DomainError)?;

        let notch_filters: Result<Vec<_>, _> = wire
            .notch_filters
            .into_iter()
            .map(|nf| nf.try_into())
            .collect();
        let notch_filters = notch_filters?;

        let curve_points: Result<Vec<_>, _> = wire
            .curve_points
            .into_iter()
            .map(|cp| cp.try_into())
            .collect();
        let curve_points = curve_points?;

        FilterConfig::new(
            wire.reconstruction as u8,
            friction,
            damper,
            inertia,
            notch_filters,
            slew_rate,
            curve_points,
        )
        .map_err(ConversionError::DomainError)
    }
}

impl From<FilterConfig> for proto::FilterConfig {
    fn from(domain: FilterConfig) -> Self {
        Self {
            reconstruction: domain.reconstruction as u32,
            friction: domain.friction.value(),
            damper: domain.damper.value(),
            inertia: domain.inertia.value(),
            notch_filters: domain.notch_filters.into_iter().map(Into::into).collect(),
            slew_rate: domain.slew_rate.value(),
            curve_points: domain.curve_points.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::NotchFilter> for NotchFilter {
    type Error = ConversionError;

    fn try_from(wire: proto::NotchFilter) -> Result<Self, Self::Error> {
        let frequency = FrequencyHz::new(wire.hz).map_err(ConversionError::DomainError)?;

        // Validate Q factor (0.1 to 100.0 is reasonable)
        if !(0.1..=100.0).contains(&wire.q) || !wire.q.is_finite() {
            return Err(ConversionError::RangeValidation {
                field: "q_factor".to_string(),
                value: wire.q as f64,
                min: 0.1,
                max: 100.0,
            });
        }

        // Validate gain_db (-60dB to +20dB is reasonable)
        if !(-60.0..=20.0).contains(&wire.gain_db) || !wire.gain_db.is_finite() {
            return Err(ConversionError::RangeValidation {
                field: "gain_db".to_string(),
                value: wire.gain_db as f64,
                min: -60.0,
                max: 20.0,
            });
        }

        NotchFilter::new(frequency, wire.q, wire.gain_db).map_err(ConversionError::DomainError)
    }
}

impl From<NotchFilter> for proto::NotchFilter {
    fn from(domain: NotchFilter) -> Self {
        Self {
            hz: domain.frequency.value(),
            q: domain.q_factor,
            gain_db: domain.gain_db,
        }
    }
}

impl TryFrom<proto::CurvePoint> for CurvePoint {
    type Error = ConversionError;

    fn try_from(wire: proto::CurvePoint) -> Result<Self, Self::Error> {
        CurvePoint::new(wire.input, wire.output).map_err(ConversionError::DomainError)
    }
}

impl From<CurvePoint> for proto::CurvePoint {
    fn from(domain: CurvePoint) -> Self {
        Self {
            input: domain.input,
            output: domain.output,
        }
    }
}

impl TryFrom<proto::LedConfig> for LedConfig {
    type Error = ConversionError;

    fn try_from(wire: proto::LedConfig) -> Result<Self, Self::Error> {
        // Validate RPM bands are in [0.0, 1.0] range and sorted
        for &band in &wire.rpm_bands {
            if !(0.0..=1.0).contains(&band) || !band.is_finite() {
                return Err(ConversionError::RangeValidation {
                    field: "rpm_bands".to_string(),
                    value: band as f64,
                    min: 0.0,
                    max: 1.0,
                });
            }
        }

        let brightness = Gain::new(wire.brightness).map_err(ConversionError::DomainError)?;

        // Create default colors since protobuf doesn't include them
        let mut colors = HashMap::new();
        colors.insert("green".to_string(), [0, 255, 0]);
        colors.insert("yellow".to_string(), [255, 255, 0]);
        colors.insert("red".to_string(), [255, 0, 0]);
        colors.insert("blue".to_string(), [0, 0, 255]);

        LedConfig::new(wire.rpm_bands, wire.pattern, brightness, colors)
            .map_err(ConversionError::DomainError)
    }
}

impl From<LedConfig> for proto::LedConfig {
    fn from(domain: LedConfig) -> Self {
        Self {
            rpm_bands: domain.rpm_bands,
            pattern: domain.pattern,
            brightness: domain.brightness.value(),
        }
    }
}

impl TryFrom<proto::HapticsConfig> for HapticsConfig {
    type Error = ConversionError;

    fn try_from(wire: proto::HapticsConfig) -> Result<Self, Self::Error> {
        let intensity = Gain::new(wire.intensity).map_err(ConversionError::DomainError)?;

        let frequency =
            FrequencyHz::new(wire.frequency_hz).map_err(ConversionError::DomainError)?;

        // Create default effects since protobuf doesn't include them
        let mut effects = HashMap::new();
        effects.insert("kerb".to_string(), true);
        effects.insert("slip".to_string(), true);
        effects.insert("gear_shift".to_string(), false);
        effects.insert("collision".to_string(), true);

        Ok(HapticsConfig::new(
            wire.enabled,
            intensity,
            frequency,
            effects,
        ))
    }
}

impl From<HapticsConfig> for proto::HapticsConfig {
    fn from(domain: HapticsConfig) -> Self {
        Self {
            enabled: domain.enabled,
            intensity: domain.intensity.value(),
            frequency_hz: domain.frequency.value(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must failed: {:?}", e),
        }
    }

    #[allow(dead_code)]
    fn must_some<T>(o: Option<T>, msg: &str) -> T {
        match o {
            Some(v) => v,
            None => panic!("must_some failed: {}", msg),
        }
    }

    #[test]
    fn test_device_capabilities_conversion() {
        let domain_caps = DeviceCapabilities::new(
            true,
            true,
            true,
            false,
            must(TorqueNm::new(25.0)),
            10000,
            1000,
        );

        let wire_caps: proto::DeviceCapabilities = domain_caps.clone().into();
        let back_to_domain: DeviceCapabilities = must(wire_caps.try_into());

        assert_eq!(domain_caps.supports_pid, back_to_domain.supports_pid);
        assert_eq!(
            domain_caps.max_torque.value(),
            back_to_domain.max_torque.value()
        );
        assert_eq!(domain_caps.encoder_cpr, back_to_domain.encoder_cpr);
    }

    #[test]
    fn test_telemetry_unit_conversion() {
        let domain_telemetry = TelemetryData {
            wheel_angle_deg: 123.456,
            wheel_speed_rad_s: 2.5,
            temperature_c: 45,
            fault_flags: 0b10101010,
            hands_on: true,
            timestamp: 1000,
        };

        let wire_telemetry: proto::TelemetryData = domain_telemetry.clone().into();

        // Check unit conversions
        assert_eq!(wire_telemetry.wheel_angle_mdeg, 123456); // degrees to millidegrees
        assert_eq!(wire_telemetry.wheel_speed_mrad_s, 2500); // rad/s to mrad/s
        assert_eq!(wire_telemetry.temp_c, 45);
        assert_eq!(wire_telemetry.faults, 0b10101010);

        let back_to_domain: TelemetryData = must(wire_telemetry.try_into());

        // Check conversion accuracy (within 0.001 for floating point)
        assert!((back_to_domain.wheel_angle_deg - 123.456).abs() < 0.001);
        assert!((back_to_domain.wheel_speed_rad_s - 2.5).abs() < 0.001);
        assert_eq!(back_to_domain.temperature_c, 45);
        assert_eq!(back_to_domain.fault_flags, 0b10101010);
    }

    #[test]
    fn test_range_validation() {
        // Test invalid temperature
        let invalid_temp = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 200, // Invalid: > 150°C
            faults: 0,
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = invalid_temp.try_into();
        assert!(result.is_err());

        // Test invalid fault flags
        let invalid_faults = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 45,
            faults: 300, // Invalid: > 255
            hands_on: false,
            sequence: 0,
        };

        let result: Result<TelemetryData, _> = invalid_faults.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_gain_validation() {
        let invalid_gain = proto::BaseSettings {
            ffb_gain: 1.5, // Invalid: > 1.0
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![],
            }),
        };

        let result: Result<BaseSettings, _> = invalid_gain.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_notch_filter_validation() {
        // Valid notch filter
        let valid_filter = proto::NotchFilter {
            hz: 60.0,
            q: 2.0,
            gain_db: -20.0,
        };

        let domain_filter: NotchFilter = must(valid_filter.try_into());
        assert_eq!(domain_filter.frequency.value(), 60.0);

        // Invalid Q factor
        let invalid_q = proto::NotchFilter {
            hz: 60.0,
            q: 0.05, // Invalid: < 0.1
            gain_db: -20.0,
        };

        let result: Result<NotchFilter, _> = invalid_q.try_into();
        assert!(result.is_err());

        // Invalid gain
        let invalid_gain = proto::NotchFilter {
            hz: 60.0,
            q: 2.0,
            gain_db: -100.0, // Invalid: < -60dB
        };

        let result: Result<NotchFilter, _> = invalid_gain.try_into();
        assert!(result.is_err());
    }
}
