pub use openracing_telemetry_config::{
    ACCConfigWriter, ACRallyConfigWriter, AMS2ConfigWriter, ConfigDiff, ConfigWriter,
    ConfigWriterFactory, DiffOperation, Dirt5ConfigWriter, EAWRCConfigWriter, F1_25ConfigWriter,
    F1ConfigWriter, IRacingConfigWriter, RFactor2ConfigWriter, TelemetryConfig,
    config_writer_factories,
};

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::collections::HashSet;

    #[test]
    fn test_config_writer_factories_non_empty() -> Result<()> {
        let factories = config_writer_factories();
        assert!(
            !factories.is_empty(),
            "Should have at least one config writer factory"
        );
        Ok(())
    }

    #[test]
    fn test_config_writer_factory_ids_unique() -> Result<()> {
        let factories = config_writer_factories();
        let mut seen = HashSet::new();
        for &(id, _) in factories {
            assert!(
                seen.insert(id),
                "Duplicate config writer factory id: {}",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn test_known_games_have_factories() -> Result<()> {
        let factories = config_writer_factories();
        let ids: HashSet<&str> = factories.iter().map(|&(id, _)| id).collect();

        for expected in &["iracing", "acc", "ams2", "rfactor2", "eawrc"] {
            assert!(
                ids.contains(expected),
                "Expected factory for game '{}'",
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn test_iracing_factory_constructs_writer() -> Result<()> {
        let factories = config_writer_factories();
        let (_, factory) = factories
            .iter()
            .find(|&&(id, _)| id == "iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing factory not found"))?;

        let writer = factory();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs = writer.get_expected_diffs(&config)?;
        assert!(!diffs.is_empty(), "iRacing writer should produce diffs");
        Ok(())
    }

    #[test]
    fn test_acc_factory_constructs_writer() -> Result<()> {
        let factories = config_writer_factories();
        let (_, factory) = factories
            .iter()
            .find(|&&(id, _)| id == "acc")
            .ok_or_else(|| anyhow::anyhow!("acc factory not found"))?;

        let writer = factory();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let diffs = writer.get_expected_diffs(&config)?;
        assert!(!diffs.is_empty(), "ACC writer should produce diffs");
        Ok(())
    }

    #[test]
    fn test_all_factories_construct_successfully() -> Result<()> {
        let factories = config_writer_factories();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "test".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        for &(id, factory) in factories {
            let writer = factory();
            // Just verify get_expected_diffs doesn't panic
            let result = writer.get_expected_diffs(&config);
            assert!(
                result.is_ok(),
                "Factory '{}' should produce valid diffs, got: {:?}",
                id,
                result.as_ref().err()
            );
        }
        Ok(())
    }
}
