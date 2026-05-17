//! Wave 15 RC hardening integration tests.
//!
//! Five categories of cross-crate integration coverage:
//!
//! 1. **E2E telemetry pipeline**: raw bytes → adapter normalize → validate output
//! 2. **Multi-adapter**: all registered adapters normalize minimum-valid packets without panic
//! 3. **Cross-crate type consistency**: schema types match across crate boundaries
//! 4. **Configuration round-trip**: config → serialize → deserialize → validate
//! 5. **Error propagation**: error types propagate correctly across crate boundaries

use std::collections::BTreeMap;

use openracing_telemetry_adapters::{TelemetryAdapter, adapter_factories};
use openracing_telemetry_config::{
    ConfigDiff, DiffOperation, GameSupportMatrix, TelemetryConfig, config_writer_factories,
    load_default_matrix, matrix_game_id_set,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, NormalizedTelemetryBuilder, TelemetryFlags, TelemetrySnapshot,
    TelemetryValue,
};
use racing_wheel_service::system_config::SystemConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. End-to-end telemetry pipeline tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: write a little-endian f32 at byte offset.
fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Helper: write a little-endian i32 at byte offset.
fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Helper: look up an adapter by game_id.
fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

#[test]
fn e2e_forza_raw_bytes_normalize_validate_output() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 8500.0); // max_rpm
    write_f32_le(&mut packet, 16, 6200.0); // current_rpm
    write_f32_le(&mut packet, 32, 25.0); // vel_x
    write_f32_le(&mut packet, 36, 0.0); // vel_y
    write_f32_le(&mut packet, 40, 0.0); // vel_z

    let telem = adapter.normalize(&packet)?;

    // Validate normalized output is within expected ranges
    assert!(
        telem.speed_ms >= 0.0 && telem.speed_ms.is_finite(),
        "speed_ms must be non-negative and finite, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.rpm - 6200.0).abs() < 1.0,
        "RPM should be ~6200, got {}",
        telem.rpm
    );
    assert!(
        (telem.max_rpm - 8500.0).abs() < 1.0,
        "max RPM should be ~8500, got {}",
        telem.max_rpm
    );
    assert!(
        telem.throttle >= 0.0 && telem.throttle <= 1.0,
        "throttle must be in [0, 1], got {}",
        telem.throttle
    );
    assert!(
        telem.brake >= 0.0 && telem.brake <= 1.0,
        "brake must be in [0, 1], got {}",
        telem.brake
    );

    // Verify the output can be serialized (cross-crate serde compatibility)
    let json = serde_json::to_string(&telem)?;
    assert!(!json.is_empty(), "serialized telemetry must not be empty");

    Ok(())
}

#[test]
fn e2e_lfs_raw_bytes_normalize_validate_output() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;

    let mut packet = vec![0u8; 96];
    packet[10] = 3; // gear=3 → 2nd in OutGauge
    write_f32_le(&mut packet, 12, 33.0); // speed m/s
    write_f32_le(&mut packet, 16, 5500.0); // RPM
    write_f32_le(&mut packet, 48, 0.7); // throttle
    write_f32_le(&mut packet, 52, 0.1); // brake

    let telem = adapter.normalize(&packet)?;

    assert!(
        (telem.speed_ms - 33.0).abs() < 0.5,
        "LFS speed should be ~33.0, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.rpm - 5500.0).abs() < 1.0,
        "LFS RPM should be ~5500, got {}",
        telem.rpm
    );
    assert_eq!(telem.gear, 2, "OutGauge gear 3 maps to 2nd gear");

    // Validate output struct fields are finite
    assert!(telem.speed_ms.is_finite(), "speed_ms must be finite");
    assert!(telem.rpm.is_finite(), "rpm must be finite");
    assert!(telem.ffb_scalar.is_finite(), "ffb_scalar must be finite");

    Ok(())
}

#[test]
fn e2e_dirt_rally_2_raw_bytes_normalize_validate_output() -> Result<(), Box<dyn std::error::Error>>
{
    let adapter = get_adapter("dirt_rally_2")?;

    let mut packet = vec![0u8; 264];
    write_f32_le(&mut packet, 32, 28.0); // vel_x → speed
    write_f32_le(&mut packet, 116, 0.6); // throttle
    write_f32_le(&mut packet, 124, 0.0); // brake
    write_f32_le(&mut packet, 132, 3.0); // gear (3rd)
    write_f32_le(&mut packet, 148, 4800.0); // rpm
    write_f32_le(&mut packet, 252, 7500.0); // max_rpm

    let telem = adapter.normalize(&packet)?;

    assert!(
        telem.rpm > 4700.0 && telem.rpm < 4900.0,
        "DiRT Rally 2 RPM should be ~4800, got {}",
        telem.rpm
    );
    assert!(
        telem.throttle > 0.5 && telem.throttle < 0.7,
        "DiRT Rally 2 throttle should be ~0.6, got {}",
        telem.throttle
    );

    // Validate all numeric fields are finite
    let fields = [
        telem.speed_ms,
        telem.rpm,
        telem.max_rpm,
        telem.throttle,
        telem.brake,
        telem.ffb_scalar,
        telem.lateral_g,
        telem.longitudinal_g,
    ];
    for (i, f) in fields.iter().enumerate() {
        assert!(
            f.is_finite(),
            "numeric field index {i} must be finite, got {f}"
        );
    }

    Ok(())
}

#[test]
fn e2e_normalized_output_serializes_to_json_and_back() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1);
    write_f32_le(&mut packet, 8, 9000.0);
    write_f32_le(&mut packet, 16, 7000.0);
    write_f32_le(&mut packet, 32, 40.0);

    let telem = adapter.normalize(&packet)?;
    let json = serde_json::to_string(&telem)?;
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert!(
        (decoded.speed_ms - telem.speed_ms).abs() < 0.01,
        "speed_ms should survive JSON round-trip"
    );
    assert!(
        (decoded.rpm - telem.rpm).abs() < 0.01,
        "rpm should survive JSON round-trip"
    );
    assert_eq!(
        decoded.gear, telem.gear,
        "gear should survive JSON round-trip"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multi-adapter tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_adapters_normalize_without_panic_on_large_zero_buffer()
-> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    assert!(
        !factories.is_empty(),
        "adapter_factories must return at least one adapter"
    );

    // A large zero-filled buffer that exceeds the minimum for most protocols.
    // Some adapters will return Ok (zero-valued telemetry), others Err (invalid
    // magic bytes, is_race_on == 0, etc.).  Neither outcome must panic.
    let large_buf = vec![0u8; 2048];

    for (game_id, factory) in factories {
        let adapter = factory();
        // Ensure the adapter factory produces a working instance
        assert_eq!(
            adapter.game_id(),
            *game_id,
            "adapter game_id() must match factory registration"
        );
        // Attempt normalization — either Ok or Err is fine, panics are not.
        let _result = adapter.normalize(&large_buf);
    }

    Ok(())
}

#[test]
fn all_adapters_reject_empty_without_panic() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let empty: [u8; 0] = [];

    for (game_id, factory) in factories {
        let adapter = factory();
        let result = adapter.normalize(&empty);
        // Most adapters should reject empty packets; none should panic.
        // We don't assert Err because some adapters might handle 0-byte input
        // as a no-op. The key invariant is no panic.
        let _ = result;
        // Verify game_id consistency
        assert_eq!(adapter.game_id(), *game_id);
    }

    Ok(())
}

#[test]
fn all_adapters_reject_single_byte_without_panic() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let single = [0xFFu8; 1];

    for (game_id, factory) in factories {
        let adapter = factory();
        let result = adapter.normalize(&single);
        let _ = result;
        assert_eq!(adapter.game_id(), *game_id);
    }

    Ok(())
}

#[test]
fn all_adapters_have_positive_expected_update_rate() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();

    for (game_id, factory) in factories {
        let adapter = factory();
        let rate = adapter.expected_update_rate();
        assert!(
            !rate.is_zero(),
            "adapter '{game_id}' expected_update_rate must be non-zero"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Cross-crate type consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn schemas_normalized_telemetry_default_matches_builder_default()
-> Result<(), Box<dyn std::error::Error>> {
    let from_default = NormalizedTelemetry::default();
    let from_builder = NormalizedTelemetryBuilder::new().build();

    // Both defaults should produce equivalent zero-state telemetry.
    assert!(
        (from_default.speed_ms - from_builder.speed_ms).abs() < f32::EPSILON,
        "default speed_ms should match builder"
    );
    assert!(
        (from_default.rpm - from_builder.rpm).abs() < f32::EPSILON,
        "default rpm should match builder"
    );
    assert_eq!(
        from_default.gear, from_builder.gear,
        "default gear should match builder"
    );
    assert_eq!(
        from_default.flags, from_builder.flags,
        "default flags should match builder"
    );
    assert!(
        from_default.extended.is_empty() && from_builder.extended.is_empty(),
        "default extended maps should both be empty"
    );

    Ok(())
}

#[test]
fn adapter_output_type_matches_schemas_normalized_telemetry()
-> Result<(), Box<dyn std::error::Error>> {
    // Build a Forza packet and normalize
    let adapter = get_adapter("forza_motorsport")?;
    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1);
    write_f32_le(&mut packet, 8, 8000.0);
    write_f32_le(&mut packet, 16, 5000.0);
    write_f32_le(&mut packet, 32, 20.0);

    let telem: NormalizedTelemetry = adapter.normalize(&packet)?;

    // Verify the returned type is indeed schemas::NormalizedTelemetry by
    // accessing fields that only exist on the schemas version (not contracts).
    let _steering: f32 = telem.steering_angle;
    let _clutch: f32 = telem.clutch;
    let _lat_g: f32 = telem.lateral_g;
    let _long_g: f32 = telem.longitudinal_g;
    let _vert_g: f32 = telem.vertical_g;
    let _slip_fl: f32 = telem.slip_angle_fl;
    let _slip_fr: f32 = telem.slip_angle_fr;
    let _slip_rl: f32 = telem.slip_angle_rl;
    let _slip_rr: f32 = telem.slip_angle_rr;
    let _fuel: f32 = telem.fuel_percent;
    let _engine_temp: f32 = telem.engine_temp_c;
    let _seq: u64 = telem.sequence;
    let _position: u8 = telem.position;

    Ok(())
}

#[test]
fn telemetry_value_enum_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let values = vec![
        ("float_val", TelemetryValue::Float(42.5)),
        ("int_val", TelemetryValue::Integer(100)),
        ("bool_val", TelemetryValue::Boolean(true)),
        ("str_val", TelemetryValue::String("pit_lane".to_string())),
    ];

    for (label, original) in &values {
        let json = serde_json::to_string(original)?;
        let decoded: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(
            &decoded, original,
            "TelemetryValue::{label} should survive JSON round-trip"
        );
    }

    Ok(())
}

#[test]
fn telemetry_flags_default_has_green_flag_set() -> Result<(), Box<dyn std::error::Error>> {
    let flags = TelemetryFlags::default();
    assert!(
        flags.green_flag,
        "default TelemetryFlags should have green_flag set"
    );
    assert!(!flags.yellow_flag, "default should not have yellow_flag");
    assert!(!flags.red_flag, "default should not have red_flag");
    assert!(
        !flags.checkered_flag,
        "default should not have checkered_flag"
    );

    Ok(())
}

#[test]
fn telemetry_snapshot_serde_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = TelemetrySnapshot {
        timestamp_ns: 1_000_000,
        speed_ms: 45.0,
        steering_angle: 0.1,
        throttle: 0.8,
        brake: 0.0,
        clutch: 0.0,
        rpm: 6500.0,
        max_rpm: 8000.0,
        gear: 4,
        num_gears: 6,
        lateral_g: 0.0,
        longitudinal_g: 0.0,
        vertical_g: 0.0,
        slip_ratio: 0.0,
        slip_angle_fl: 0.0,
        slip_angle_fr: 0.0,
        slip_angle_rl: 0.0,
        slip_angle_rr: 0.0,
        ffb_scalar: 0.5,
        ffb_torque_nm: 0.0,
        flags: TelemetryFlags::default(),
        position: 1,
        lap: 3,
        current_lap_time_s: 0.0,
        fuel_percent: 0.8,
        sequence: 42,
    };

    let json = serde_json::to_string(&snapshot)?;
    let decoded: TelemetrySnapshot = serde_json::from_str(&json)?;

    assert_eq!(decoded.timestamp_ns, snapshot.timestamp_ns);
    assert!((decoded.speed_ms - snapshot.speed_ms).abs() < f32::EPSILON);
    assert!((decoded.rpm - snapshot.rpm).abs() < f32::EPSILON);
    assert_eq!(decoded.gear, snapshot.gear);
    assert_eq!(decoded.sequence, snapshot.sequence);

    Ok(())
}

#[test]
fn builder_produces_valid_telemetry_with_extended_data() -> Result<(), Box<dyn std::error::Error>> {
    let telem = NormalizedTelemetryBuilder::new()
        .speed_ms(55.0)
        .rpm(7200.0)
        .gear(5)
        .throttle(0.9)
        .brake(0.0)
        .steering_angle(0.05)
        .build();

    assert!((telem.speed_ms - 55.0).abs() < f32::EPSILON);
    assert!((telem.rpm - 7200.0).abs() < f32::EPSILON);
    assert_eq!(telem.gear, 5);
    assert!((telem.throttle - 0.9).abs() < f32::EPSILON);

    // Serialize and verify cross-crate compatibility
    let json = serde_json::to_string(&telem)?;
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
    assert_eq!(decoded.gear, 5);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Configuration round-trip tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_config_json_round_trip_preserves_all_fields() -> Result<(), Box<dyn std::error::Error>>
{
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 360,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec![
            "ffb_scalar".to_string(),
            "rpm".to_string(),
            "speed_ms".to_string(),
            "gear".to_string(),
            "slip_ratio".to_string(),
        ],
        enable_high_rate_iracing_360hz: true,
    };

    let json = serde_json::to_string_pretty(&config)?;
    let decoded: TelemetryConfig = serde_json::from_str(&json)?;

    assert_eq!(decoded.enabled, config.enabled);
    assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
    assert_eq!(decoded.output_method, config.output_method);
    assert_eq!(decoded.output_target, config.output_target);
    assert_eq!(decoded.fields, config.fields);
    assert_eq!(
        decoded.enable_high_rate_iracing_360hz,
        config.enable_high_rate_iracing_360hz
    );

    Ok(())
}

#[test]
fn config_diff_round_trip_all_operations() -> Result<(), Box<dyn std::error::Error>> {
    let operations = [
        DiffOperation::Add,
        DiffOperation::Modify,
        DiffOperation::Remove,
    ];

    for op in &operations {
        let diff = ConfigDiff {
            file_path: "game/config.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "udpEnabled".to_string(),
            old_value: Some("false".to_string()),
            new_value: "true".to_string(),
            operation: op.clone(),
        };

        let json = serde_json::to_string(&diff)?;
        let decoded: ConfigDiff = serde_json::from_str(&json)?;
        assert_eq!(decoded, diff, "ConfigDiff round-trip failed for {op:?}");
    }

    Ok(())
}

#[test]
fn game_support_matrix_loads_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;

    // Verify the matrix has games
    assert!(
        !matrix.games.is_empty(),
        "game support matrix must contain at least one game"
    );

    // Verify game_ids returns consistent data
    let ids = matrix.game_ids();
    assert!(
        !ids.is_empty(),
        "game_ids() must return at least one game ID"
    );

    // Verify game_id_set matches
    let id_set = matrix_game_id_set()?;
    for id in &ids {
        assert!(
            id_set.contains(id),
            "game ID '{id}' missing from matrix_game_id_set"
        );
    }

    // Verify serialization round-trip preserves game count
    let json = serde_json::to_string(&matrix)?;
    let decoded: GameSupportMatrix = serde_json::from_str(&json)?;
    assert_eq!(
        decoded.games.len(),
        matrix.games.len(),
        "game count must survive JSON round-trip"
    );

    Ok(())
}

#[test]
fn system_config_default_round_trips_via_json() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();

    let json = serde_json::to_string_pretty(&config)?;
    assert!(
        !json.is_empty(),
        "serialized SystemConfig must not be empty"
    );

    let decoded: SystemConfig = serde_json::from_str(&json)?;

    assert_eq!(
        decoded.schema_version, config.schema_version,
        "schema_version must survive round-trip"
    );
    assert_eq!(
        decoded.engine.tick_rate_hz, config.engine.tick_rate_hz,
        "engine.tick_rate_hz must survive round-trip"
    );
    assert_eq!(
        decoded.safety.default_safe_torque_nm, config.safety.default_safe_torque_nm,
        "safety.default_safe_torque_nm must survive round-trip"
    );
    assert_eq!(
        decoded.safety.max_torque_nm, config.safety.max_torque_nm,
        "safety.max_torque_nm must survive round-trip"
    );

    Ok(())
}

#[test]
fn config_writer_factory_ids_align_with_support_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;
    let factory_ids: std::collections::HashSet<&str> = config_writer_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect();

    for (game_id, game) in &matrix.games {
        assert!(
            factory_ids.contains(&*game.config_writer),
            "game '{game_id}' references config_writer '{}' with no factory",
            game.config_writer
        );
    }

    Ok(())
}

#[test]
fn normalized_telemetry_serde_round_trip_with_extended_data()
-> Result<(), Box<dyn std::error::Error>> {
    let mut extended = BTreeMap::new();
    extended.insert("tire_temp_fl".to_string(), TelemetryValue::Float(85.0));
    extended.insert("drs_active".to_string(), TelemetryValue::Boolean(true));
    extended.insert(
        "car_class".to_string(),
        TelemetryValue::String("GT3".to_string()),
    );
    extended.insert("position".to_string(), TelemetryValue::Integer(3));

    let telem = NormalizedTelemetryBuilder::new()
        .speed_ms(60.0)
        .rpm(8000.0)
        .gear(6)
        .build();
    // Builder doesn't set extended, so assign directly.
    let mut telem = telem;
    telem.extended = extended;

    let json = serde_json::to_string(&telem)?;
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(
        decoded.extended.len(),
        4,
        "extended map must preserve all 4 entries"
    );
    match decoded.extended.get("tire_temp_fl") {
        Some(TelemetryValue::Float(v)) => {
            assert!((*v - 85.0).abs() < f32::EPSILON);
        }
        other => {
            return Err(format!("expected Float(85.0), got {other:?}").into());
        }
    }
    match decoded.extended.get("drs_active") {
        Some(TelemetryValue::Boolean(true)) => {}
        other => {
            return Err(format!("expected Boolean(true), got {other:?}").into());
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Error propagation tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_error_propagates_as_boxed_std_error() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // Short packet should produce an error that propagates through Box<dyn Error>.
    let short = [0u8; 10];
    let result = adapter.normalize(&short);
    assert!(result.is_err(), "undersized packet must produce an error");

    let err = result.err().ok_or("expected error")?;
    let err_msg = format!("{err}");
    assert!(!err_msg.is_empty(), "error message must not be empty");

    // Verify the error can be converted to Box<dyn std::error::Error>
    let boxed: Box<dyn std::error::Error> = err.into();
    let display = format!("{boxed}");
    assert!(!display.is_empty(), "boxed error display must not be empty");

    Ok(())
}

#[test]
fn multiple_adapters_error_on_undersized_input() -> Result<(), Box<dyn std::error::Error>> {
    let games = [
        "forza_motorsport",
        "live_for_speed",
        "dirt_rally_2",
        "acc",
        "iracing",
        "f1_25",
    ];

    for game_id in &games {
        let adapter = get_adapter(game_id)?;
        let result = adapter.normalize(&[0u8; 4]);
        assert!(
            result.is_err(),
            "adapter '{game_id}' must reject a 4-byte packet"
        );

        // Verify the error has a non-empty message
        if let Err(e) = result {
            let msg = format!("{e}");
            assert!(
                !msg.is_empty(),
                "adapter '{game_id}' error must have a message"
            );
        }
    }

    Ok(())
}

#[test]
fn plugin_error_variants_implement_std_error() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_plugins::PluginError;

    let errors: Vec<PluginError> = vec![
        PluginError::ManifestValidation("bad manifest".to_string()),
        PluginError::LoadingFailed("file not found".to_string()),
        PluginError::ExecutionTimeout {
            duration: std::time::Duration::from_millis(500),
        },
        PluginError::BudgetViolation {
            used_us: 200,
            budget_us: 100,
        },
        PluginError::Crashed {
            reason: "segfault".to_string(),
        },
        PluginError::Quarantined {
            plugin_id: uuid::Uuid::nil(),
        },
        PluginError::CapabilityViolation {
            capability: "filesystem".to_string(),
        },
    ];

    for err in errors {
        // Verify Display trait works
        let display = format!("{err}");
        assert!(!display.is_empty(), "PluginError Display must not be empty");

        // Verify std::error::Error trait implementation
        let _source: Option<&dyn std::error::Error> = std::error::Error::source(&err);

        // Verify it can be boxed into Box<dyn Error>
        let boxed: Box<dyn std::error::Error> = Box::new(err);
        assert!(
            !format!("{boxed}").is_empty(),
            "boxed PluginError must display"
        );
    }

    Ok(())
}

#[test]
fn schema_error_propagates_from_invalid_json() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that schema validation errors propagate as std::error::Error.
    let bad_json = "{ not valid json }";
    let result: Result<NormalizedTelemetry, _> = serde_json::from_str(bad_json);
    assert!(result.is_err(), "invalid JSON must produce an error");

    if let Err(e) = result {
        let boxed: Box<dyn std::error::Error> = Box::new(e);
        assert!(
            !format!("{boxed}").is_empty(),
            "JSON parse error must have a message"
        );
    }

    Ok(())
}

#[test]
fn adapter_lookup_failure_propagates_descriptively() -> Result<(), Box<dyn std::error::Error>> {
    let result = get_adapter("__nonexistent_game__");
    assert!(result.is_err(), "missing adapter must return Err");

    if let Err(msg) = result {
        assert!(
            msg.contains("__nonexistent_game__"),
            "error message should contain the game ID, got: {msg}"
        );
    }

    Ok(())
}

#[test]
fn config_deserialization_error_propagates_on_bad_input() -> Result<(), Box<dyn std::error::Error>>
{
    // SystemConfig with wrong types should fail with a meaningful error.
    let bad_json = r#"{"schema_version": 42}"#;
    let result: Result<SystemConfig, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "SystemConfig from wrong-typed JSON must fail"
    );

    if let Err(e) = result {
        let msg = format!("{e}");
        assert!(!msg.is_empty(), "deserialization error must have a message");
    }

    Ok(())
}
