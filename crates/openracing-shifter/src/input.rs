//! Shifter input parsing

use super::{GearPosition, MAX_GEARS, ShifterError, ShifterResult};

#[derive(Default)]
pub struct ShifterInput {
    pub gear: GearPosition,
    pub clutch: Option<u16>,
    pub paddle_up: bool,
    pub paddle_down: bool,
}

impl ShifterInput {
    pub fn parse_gamepad(data: &[u8]) -> ShifterResult<Self> {
        if data.len() < 4 {
            return Err(ShifterError::InvalidReport);
        }

        let gear_raw = data[2];
        let buttons = data[3];

        let gear = GearPosition::from_raw(gear_raw);
        let clutch = if data.len() >= 6 {
            Some(u16::from(data[4]) | (u16::from(data[5]) << 8))
        } else {
            None
        };

        let paddle_up = (buttons & 0x10) != 0;
        let paddle_down = (buttons & 0x20) != 0;

        Ok(Self {
            gear,
            clutch,
            paddle_up,
            paddle_down,
        })
    }

    pub fn from_sequential(up: bool, down: bool, current_gear: i32) -> Self {
        let gear = if up {
            GearPosition::new(current_gear.saturating_add(1).min(MAX_GEARS as i32))
        } else if down {
            GearPosition::new(current_gear.saturating_sub(1).max(1))
        } else {
            GearPosition::new(current_gear)
        };

        Self {
            gear,
            clutch: None,
            paddle_up: up,
            paddle_down: down,
        }
    }

    pub fn gear(&self) -> i32 {
        self.gear.gear
    }

    pub fn is_shifting(&self) -> bool {
        self.paddle_up || self.paddle_down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gamepad() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x03, 0x00, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(!input.paddle_up);
        assert!(!input.paddle_down);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_with_paddles() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x04, 0x30, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;

        assert_eq!(input.gear.gear, 4);
        assert!(input.paddle_up);
        assert!(input.paddle_down);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_with_clutch() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x03, 0x00, 0x34, 0x12];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(0x1234));
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_no_clutch_short_data() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x03, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, None);
        Ok(())
    }

    #[test]
    fn test_sequential_shifter() {
        let input = ShifterInput::from_sequential(true, false, 3);

        assert_eq!(input.gear.gear, 4);
        assert!(input.paddle_up);
        assert!(!input.paddle_down);
    }

    #[test]
    fn test_sequential_shifter_down() {
        let input = ShifterInput::from_sequential(false, true, 4);

        assert_eq!(input.gear.gear, 3);
        assert!(!input.paddle_up);
        assert!(input.paddle_down);
    }

    #[test]
    fn test_sequential_shifter_bounds() {
        let input_max = ShifterInput::from_sequential(true, false, 8);
        assert_eq!(input_max.gear.gear, 8);

        let input_min = ShifterInput::from_sequential(false, true, 1);
        assert_eq!(input_min.gear.gear, 1);
    }

    #[test]
    fn test_sequential_extreme_inputs_saturate_before_clamp() {
        let input_max = ShifterInput::from_sequential(true, false, i32::MAX);
        assert_eq!(input_max.gear(), MAX_GEARS as i32);

        let input_min = ShifterInput::from_sequential(false, true, i32::MIN);
        assert_eq!(input_min.gear(), 1);
    }

    #[test]
    fn test_sequential_no_shift() {
        let input = ShifterInput::from_sequential(false, false, 5);
        assert_eq!(input.gear.gear, 5);
        assert!(!input.paddle_up);
        assert!(!input.paddle_down);
    }

    #[test]
    fn test_is_shifting() {
        let idle = ShifterInput::default();
        assert!(!idle.is_shifting());

        let shifting = ShifterInput::from_sequential(true, false, 3);
        assert!(shifting.is_shifting());
    }

    #[test]
    fn test_gear_accessor() {
        let input = ShifterInput::from_sequential(true, false, 3);
        assert_eq!(input.gear(), 4);
    }

    #[test]
    fn test_invalid_report() {
        let data = vec![0x00];
        let result = ShifterInput::parse_gamepad(&data);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }

    #[test]
    fn test_invalid_report_empty() {
        let data: Vec<u8> = vec![];
        let result = ShifterInput::parse_gamepad(&data);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }

    #[test]
    fn test_invalid_report_3_bytes() {
        let data = vec![0x00, 0x00, 0x03];
        let result = ShifterInput::parse_gamepad(&data);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }

    #[test]
    fn test_shifter_input_default() {
        let input = ShifterInput::default();
        assert!(input.gear.is_neutral);
        assert_eq!(input.clutch, None);
        assert!(!input.paddle_up);
        assert!(!input.paddle_down);
        assert!(!input.is_shifting());
        assert_eq!(input.gear(), 0);
    }

    #[test]
    fn test_parse_gamepad_neutral_gear() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.gear.is_neutral);
        assert_eq!(input.gear(), 0);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_0xff_gear() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0xFF, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.gear.is_neutral);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_exactly_4_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x05, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.gear(), 5);
        assert_eq!(input.clutch, None);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_5_bytes_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x03, 0x00, 0xFF];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.gear(), 3);
        assert_eq!(input.clutch, None);
        Ok(())
    }

    #[test]
    fn test_sequential_both_paddles() {
        // When both up and down are true, up takes precedence
        let input = ShifterInput::from_sequential(true, true, 3);
        assert_eq!(input.gear(), 4);
        assert!(input.paddle_up);
        assert!(input.paddle_down);
        assert!(input.is_shifting());
    }

    #[test]
    fn test_sequential_at_max_gear_up() {
        let input = ShifterInput::from_sequential(true, false, MAX_GEARS as i32);
        assert_eq!(input.gear(), MAX_GEARS as i32);
    }

    #[test]
    fn test_sequential_at_min_gear_down() {
        let input = ShifterInput::from_sequential(false, true, 1);
        assert_eq!(input.gear(), 1);
    }

    #[test]
    fn test_parse_gamepad_paddle_up_only() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x01, 0x10];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.paddle_up);
        assert!(!input.paddle_down);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_paddle_down_only() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x01, 0x20];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(!input.paddle_up);
        assert!(input.paddle_down);
        Ok(())
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_parse_gamepad_succeeds_for_sufficient_data(
            data in proptest::collection::vec(any::<u8>(), 4..=64),
        ) {
            let result = ShifterInput::parse_gamepad(&data);
            prop_assert!(result.is_ok());
        }

        #[test]
        fn prop_parse_gamepad_fails_for_short_data(
            data in proptest::collection::vec(any::<u8>(), 0..4usize),
        ) {
            let result = ShifterInput::parse_gamepad(&data);
            prop_assert!(result.is_err());
        }

        #[test]
        fn prop_sequential_upshift_increases_gear(current in 1i32..=7i32) {
            let input = ShifterInput::from_sequential(true, false, current);
            prop_assert_eq!(input.gear(), current + 1);
        }

        #[test]
        fn prop_sequential_downshift_decreases_gear(current in 2i32..=8i32) {
            let input = ShifterInput::from_sequential(false, true, current);
            prop_assert_eq!(input.gear(), current - 1);
        }

        #[test]
        fn prop_sequential_gear_never_exceeds_max(current in 1i32..=100i32) {
            let input = ShifterInput::from_sequential(true, false, current);
            prop_assert!(input.gear() <= MAX_GEARS as i32);
        }

        #[test]
        fn prop_sequential_gear_never_below_min(current in -100i32..=10i32) {
            let input = ShifterInput::from_sequential(false, true, current);
            prop_assert!(input.gear() >= 1);
        }

        #[test]
        fn prop_no_shift_preserves_gear(current in 1i32..=8i32) {
            let input = ShifterInput::from_sequential(false, false, current);
            prop_assert_eq!(input.gear(), current);
        }
    }
}
