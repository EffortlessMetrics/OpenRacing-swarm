//! Handbrake input parsing

use super::{HandbrakeResult, MAX_ANALOG_VALUE};

pub struct HandbrakeInput {
    pub raw_value: u16,
    pub is_engaged: bool,
    pub calibration_min: u16,
    pub calibration_max: u16,
}

impl HandbrakeInput {
    pub fn parse_gamepad(data: &[u8]) -> HandbrakeResult<Self> {
        if data.len() < 4 {
            return Err(super::HandbrakeError::Disconnected);
        }

        let raw_value = u16::from(data[2]) | (u16::from(data[3]) << 8);

        Ok(Self {
            raw_value,
            is_engaged: raw_value > 100,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        })
    }

    pub fn normalized(&self) -> f32 {
        let min = self.calibration_min.min(self.calibration_max);
        let max = self.calibration_min.max(self.calibration_max);
        let range = max.saturating_sub(min);

        if range == 0 {
            return 0.0;
        }

        let offset = self.raw_value.saturating_sub(min);
        (f32::from(offset) / f32::from(range)).clamp(0.0, 1.0)
    }

    pub fn with_calibration(mut self, min: u16, max: u16) -> Self {
        self.calibration_min = min;
        self.calibration_max = max;
        self
    }

    pub fn calibrate(&mut self, min: u16, max: u16) {
        self.calibration_min = min;
        self.calibration_max = max;
    }
}

impl Default for HandbrakeInput {
    fn default() -> Self {
        Self {
            raw_value: 0,
            is_engaged: false,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        }
    }
}

pub struct HandbrakeCalibration {
    pub min: u16,
    pub max: u16,
    pub center: Option<u16>,
    samples: u32,
}

impl HandbrakeCalibration {
    pub fn new() -> Self {
        Self {
            min: 0,
            max: MAX_ANALOG_VALUE,
            center: None,
            samples: 0,
        }
    }

    pub fn sample(&mut self, value: u16) {
        if self.samples == 0 {
            self.min = value;
            self.max = value;
        } else {
            if value < self.min {
                self.min = value;
            }
            if value > self.max {
                self.max = value;
            }
        }
        self.samples = self.samples.saturating_add(1);
    }

    pub fn apply(&self, input: &mut HandbrakeInput) {
        input.calibrate(self.min, self.max);
    }
}

impl Default for HandbrakeCalibration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gamepad() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0xFF, 0xFF];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;

        assert_eq!(input.raw_value, 0xFFFF);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_zero() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0);
        assert!(!input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_engagement_threshold() -> Result<(), Box<dyn std::error::Error>> {
        // Value of 100 should not be engaged (threshold is > 100)
        let data = vec![0x00, 0x00, 100, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 100);
        assert!(!input.is_engaged);

        // Value of 101 should be engaged
        let data = vec![0x00, 0x00, 101, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 101);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_normalized_full() {
        let input = HandbrakeInput {
            raw_value: MAX_ANALOG_VALUE,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };

        assert!((input.normalized() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalized_half() {
        let input = HandbrakeInput {
            raw_value: MAX_ANALOG_VALUE / 2,
            is_engaged: false,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };

        assert!((input.normalized() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_normalized_zero_range() {
        let input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: false,
            calibration_min: 5000,
            calibration_max: 5000,
        };
        assert!((input.normalized()).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized_clamped_above_max() {
        let input = HandbrakeInput {
            raw_value: 10000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized_clamped_below_min() {
        let input = HandbrakeInput {
            raw_value: 500,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized()).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized_handles_inverted_calibration_range() {
        let input = HandbrakeInput {
            raw_value: 2500,
            is_engaged: true,
            calibration_min: 5000,
            calibration_max: 1000,
        };
        assert!((input.normalized() - 0.375).abs() < 0.001);
    }

    #[test]
    fn test_with_calibration() {
        let input = HandbrakeInput::default().with_calibration(1000, 9000);

        assert_eq!(input.calibration_min, 1000);
        assert_eq!(input.calibration_max, 9000);
    }

    #[test]
    fn test_calibration() {
        let mut calibration = HandbrakeCalibration::new();

        calibration.sample(100);
        calibration.sample(50);
        calibration.sample(200);

        assert_eq!(calibration.min, 50);
        assert_eq!(calibration.max, 200);
    }

    #[test]
    fn test_calibration_apply() {
        let mut calibration = HandbrakeCalibration::new();
        calibration.sample(100);
        calibration.sample(9000);

        let mut input = HandbrakeInput::default();
        calibration.apply(&mut input);

        assert_eq!(input.calibration_min, 100);
        assert_eq!(input.calibration_max, 9000);
    }

    #[test]
    fn test_calibration_default() {
        let calibration = HandbrakeCalibration::default();
        assert_eq!(calibration.min, 0);
        assert_eq!(calibration.max, MAX_ANALOG_VALUE);
        assert_eq!(calibration.center, None);
    }

    #[test]
    fn test_disconnected() {
        let data = vec![0x00];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_handbrake_input_default() {
        let input = HandbrakeInput::default();
        assert_eq!(input.raw_value, 0);
        assert!(!input.is_engaged);
        assert_eq!(input.calibration_min, 0);
        assert_eq!(input.calibration_max, MAX_ANALOG_VALUE);
    }

    #[test]
    fn test_calibrate_method() {
        let mut input = HandbrakeInput::default();
        input.calibrate(500, 8000);
        assert_eq!(input.calibration_min, 500);
        assert_eq!(input.calibration_max, 8000);
    }

    #[test]
    fn test_parse_gamepad_mid_value() -> Result<(), Box<dyn std::error::Error>> {
        // raw_value = 0x0180 (384 in decimal)
        let data = vec![0x00, 0x00, 0x80, 0x01];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x0180);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_exactly_4_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0xAA, 0xBB, 0x10, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x0010);
        assert!(!input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_large_data() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[2] = 0xFF;
        data[3] = 0x7F;
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0x7FFF);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_empty_data() {
        let data: Vec<u8> = vec![];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_gamepad_3_bytes() {
        let data = vec![0x00, 0x00, 0xFF];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalized_with_custom_calibration() {
        let input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: true,
            calibration_min: 2000,
            calibration_max: 8000,
        };
        // (5000-2000)/(8000-2000) = 3000/6000 = 0.5
        assert!((input.normalized() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_normalized_at_calibration_min() {
        let input = HandbrakeInput {
            raw_value: 1000,
            is_engaged: false,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized()).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized_at_calibration_max() {
        let input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_calibration_single_sample() {
        let mut cal = HandbrakeCalibration::new();
        cal.sample(500);
        assert_eq!(cal.min, 500);
        assert_eq!(cal.max, 500);
    }

    #[test]
    fn test_calibration_preserves_max_after_sentinel_observation() {
        // Regression: previously, observing MAX_ANALOG_VALUE then a smaller
        // sample would clobber max back down because of an OR-sentinel check
        // (`self.max == MAX_ANALOG_VALUE`).
        let mut cal = HandbrakeCalibration::new();
        cal.sample(MAX_ANALOG_VALUE);
        cal.sample(3000);
        assert_eq!(cal.min, 3000);
        assert_eq!(cal.max, MAX_ANALOG_VALUE);
    }

    #[test]
    fn test_calibration_preserves_min_after_zero_observation() {
        // Regression: observing 0 then a larger sample previously clobbered
        // min upward because of an OR-sentinel check (`self.min == 0`).
        let mut cal = HandbrakeCalibration::new();
        cal.sample(0);
        cal.sample(5000);
        assert_eq!(cal.min, 0);
        assert_eq!(cal.max, 5000);
    }

    #[test]
    fn test_calibration_center_stays_none() {
        let mut cal = HandbrakeCalibration::new();
        cal.sample(100);
        cal.sample(200);
        assert_eq!(cal.center, None);
    }

    #[test]
    fn test_with_calibration_chain() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0xE8, 0x03]; // raw_value = 1000
        let input = HandbrakeInput::parse_gamepad(&data)
            .map_err(|e| e.to_string())?
            .with_calibration(500, 2000);
        assert_eq!(input.calibration_min, 500);
        assert_eq!(input.calibration_max, 2000);
        // (1000-500)/(2000-500) = 500/1500 ≈ 0.333
        assert!((input.normalized() - 0.333).abs() < 0.01);
        Ok(())
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_normalized_within_unit_range(raw_value in any::<u16>(), min in any::<u16>(), max in any::<u16>()) {
            let input = HandbrakeInput {
                raw_value,
                is_engaged: raw_value > 100,
                calibration_min: min,
                calibration_max: max,
            };
            let norm = input.normalized();
            prop_assert!(norm >= 0.0, "normalized must be >= 0, got {}", norm);
            prop_assert!(norm <= 1.0, "normalized must be <= 1, got {}", norm);
        }

        #[test]
        fn prop_parse_gamepad_succeeds_for_sufficient_data(
            data in proptest::collection::vec(any::<u8>(), 4..=64),
        ) {
            let result = HandbrakeInput::parse_gamepad(&data);
            prop_assert!(result.is_ok());
        }

        #[test]
        fn prop_parse_gamepad_fails_for_short_data(
            data in proptest::collection::vec(any::<u8>(), 0..4usize),
        ) {
            let result = HandbrakeInput::parse_gamepad(&data);
            prop_assert!(result.is_err());
        }

        #[test]
        fn prop_engagement_threshold_consistent(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let data = vec![0x00, 0x00, lo, hi];
            if let Ok(input) = HandbrakeInput::parse_gamepad(&data) {
                let expected_engaged = input.raw_value > 100;
                prop_assert_eq!(input.is_engaged, expected_engaged);
            }
        }

        #[test]
        fn prop_calibration_sample_tracks_extremes(samples in proptest::collection::vec(any::<u16>(), 1..50)) {
            let mut calibration = HandbrakeCalibration::new();
            for &s in &samples {
                calibration.sample(s);
            }
            let expected_min = samples.iter().copied().fold(u16::MAX, u16::min);
            let expected_max = samples.iter().copied().fold(u16::MIN, u16::max);
            prop_assert_eq!(calibration.min, expected_min);
            prop_assert_eq!(calibration.max, expected_max);
        }
    }
}
