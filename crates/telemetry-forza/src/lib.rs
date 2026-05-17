//! Forza Motorsport and Forza Horizon telemetry adapter.
#![deny(static_mut_refs)]

pub use openracing_telemetry::{NormalizedTelemetry, TelemetryFrame, TelemetryValue};
pub use openracing_telemetry_adapters::TelemetryAdapter;
pub use openracing_telemetry_adapters::games::forza::ForzaAdapter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_reexported_forza_adapter_game_id() -> TestResult {
        let adapter = ForzaAdapter::new();
        assert_eq!(adapter.game_id(), "forza_motorsport");
        Ok(())
    }

    #[test]
    fn test_reexported_forza_adapter_update_rate() -> TestResult {
        let adapter = ForzaAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn test_reexported_forza_adapter_normalize_sled() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut data = vec![0u8; 232];
        // is_race_on = 1
        data[0..4].copy_from_slice(&1i32.to_le_bytes());
        // engine_max_rpm
        data[8..12].copy_from_slice(&8000.0f32.to_le_bytes());
        // current_rpm
        data[16..20].copy_from_slice(&5000.0f32.to_le_bytes());
        // vel_x = 20 m/s
        data[32..36].copy_from_slice(&20.0f32.to_le_bytes());

        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        assert!((result.speed_ms - 20.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_reexported_forza_adapter_rejects_bad_data() -> TestResult {
        let adapter = ForzaAdapter::new();
        assert!(adapter.normalize(&[0u8; 10]).is_err());
        Ok(())
    }

    #[test]
    fn test_reexported_normalized_telemetry_default() -> TestResult {
        let t = NormalizedTelemetry::default();
        assert_eq!(t.rpm, 0.0);
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.gear, 0);
        Ok(())
    }

    #[test]
    fn test_reexported_telemetry_value_variants() -> TestResult {
        let f = TelemetryValue::Float(1.5);
        let i = TelemetryValue::Integer(42);
        let b = TelemetryValue::Boolean(true);
        let s = TelemetryValue::String("test".to_string());

        assert_eq!(f, TelemetryValue::Float(1.5));
        assert_eq!(i, TelemetryValue::Integer(42));
        assert_eq!(b, TelemetryValue::Boolean(true));
        assert_eq!(s, TelemetryValue::String("test".to_string()));
        Ok(())
    }

    #[test]
    fn test_reexported_telemetry_frame_creation() -> TestResult {
        let telemetry = NormalizedTelemetry::builder().rpm(3000.0).build();
        let frame = TelemetryFrame::new(telemetry, 12345, 1, 232);
        assert_eq!(frame.data.rpm, 3000.0);
        assert_eq!(frame.timestamp_ns, 12345);
        assert_eq!(frame.sequence, 1);
        assert_eq!(frame.raw_size, 232);
        Ok(())
    }

    // Confirm the adapter is usable as a trait object
    #[test]
    fn test_forza_adapter_as_trait_object() -> TestResult {
        let adapter: Box<dyn TelemetryAdapter> = Box::new(ForzaAdapter::new());
        assert_eq!(adapter.game_id(), "forza_motorsport");
        Ok(())
    }
}
