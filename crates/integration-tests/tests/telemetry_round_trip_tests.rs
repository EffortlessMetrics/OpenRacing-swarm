//! Telemetry round-trip integration tests.
//!
//! Validates that game telemetry adapters correctly parse raw packets and
//! produce expected `NormalizedTelemetry` field values. Each test imports
//! both the adapter (via `openracing_telemetry_adapters`) and the schemas
//! crate, constructs a representative packet, normalizes it, and verifies
//! key fields.

use openracing_telemetry_adapters::{TelemetryAdapter, adapter_factories};

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Write a little-endian f32 at the given byte offset.
fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Write a little-endian i32 at the given byte offset.
fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Look up an adapter by game_id from the factory registry.
fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

// ─── Forza Motorsport (Sled 232-byte format) ─────────────────────────────────

#[test]
fn forza_sled_packet_round_trips_velocity_to_speed() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 9000.0); // engine_max_rpm
    write_f32_le(&mut packet, 16, 7200.0); // current_rpm
    // velocity components: sqrt(3^2 + 4^2) = 5.0 m/s
    write_f32_le(&mut packet, 32, 3.0); // vel_x
    write_f32_le(&mut packet, 36, 0.0); // vel_y
    write_f32_le(&mut packet, 40, 4.0); // vel_z

    let telem = adapter.normalize(&packet)?;

    let expected_speed = (3.0f32.powi(2) + 4.0f32.powi(2)).sqrt();
    assert!(
        (telem.speed_ms - expected_speed).abs() < 1.0,
        "speed should be ~{expected_speed} m/s, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.rpm - 7200.0).abs() < 1.0,
        "RPM should be ~7200, got {}",
        telem.rpm
    );
    assert!(
        (telem.max_rpm - 9000.0).abs() < 1.0,
        "max RPM should be ~9000, got {}",
        telem.max_rpm
    );

    Ok(())
}

#[test]
fn forza_cardash_packet_round_trips_controls() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // 311-byte CarDash packet
    let mut packet = vec![0u8; 311];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 8000.0); // max_rpm
    write_f32_le(&mut packet, 16, 4000.0); // current_rpm
    write_f32_le(&mut packet, 32, 10.0); // vel_x
    packet[303] = 200; // throttle (~78%)
    packet[304] = 128; // brake (~50%)

    let telem = adapter.normalize(&packet)?;

    assert!(
        telem.throttle > 0.5,
        "throttle byte 200 should normalize to >0.5, got {}",
        telem.throttle
    );
    assert!(
        telem.brake > 0.3 && telem.brake < 0.7,
        "brake byte 128 should normalize to ~0.5, got {}",
        telem.brake
    );
    assert!(
        (telem.rpm - 4000.0).abs() < 1.0,
        "RPM should be ~4000, got {}",
        telem.rpm
    );

    Ok(())
}

#[test]
fn forza_rejects_undersized_packets() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // 50 bytes is too short for any Forza format (min 232)
    let short = [0u8; 50];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "undersized Forza packet must fail normalization"
    );

    Ok(())
}

// ─── Forza Horizon 4 (FH4 324-byte format) ───────────────────────────────────

#[test]
fn forza_horizon_4_packet_round_trips_engine_data() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_horizon_4")?;

    // FH4 accepts 232-byte Sled packets (same as Forza Motorsport)
    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 7500.0); // engine_max_rpm
    write_f32_le(&mut packet, 16, 3500.0); // current_rpm
    write_f32_le(&mut packet, 32, 20.0); // vel_x
    write_f32_le(&mut packet, 36, 0.0); // vel_y
    write_f32_le(&mut packet, 40, 0.0); // vel_z

    let telem = adapter.normalize(&packet)?;

    assert!(
        (telem.rpm - 3500.0).abs() < 1.0,
        "RPM should be ~3500, got {}",
        telem.rpm
    );
    assert!(
        (telem.max_rpm - 7500.0).abs() < 1.0,
        "max RPM should be ~7500, got {}",
        telem.max_rpm
    );
    assert!(
        (telem.speed_ms - 20.0).abs() < 1.0,
        "speed should be ~20.0 m/s, got {}",
        telem.speed_ms
    );

    Ok(())
}

// ─── Live For Speed (LFS OutGauge 96-byte format) ────────────────────────────

#[test]
fn lfs_outgauge_packet_round_trips_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;

    let mut packet = vec![0u8; 96];
    packet[10] = 4; // gear=4 → 3rd gear in OutGauge (0=R, 1=N, 2=1st, 3=2nd, 4=3rd)
    write_f32_le(&mut packet, 12, 42.5); // speed m/s
    write_f32_le(&mut packet, 16, 6800.0); // RPM
    write_f32_le(&mut packet, 28, 0.65); // fuel
    write_f32_le(&mut packet, 48, 0.9); // throttle
    write_f32_le(&mut packet, 52, 0.0); // brake
    write_f32_le(&mut packet, 56, 0.0); // clutch

    let telem = adapter.normalize(&packet)?;

    assert!(
        (telem.speed_ms - 42.5).abs() < 0.1,
        "speed should be ~42.5 m/s, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.rpm - 6800.0).abs() < 1.0,
        "RPM should be ~6800, got {}",
        telem.rpm
    );
    // OutGauge gear 4 = 3rd gear normalized
    assert_eq!(telem.gear, 3, "gear 4 in OutGauge maps to 3rd gear");

    Ok(())
}

#[test]
fn lfs_outgauge_packet_round_trips_controls() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;

    let mut packet = vec![0u8; 96];
    packet[10] = 1; // gear=1 → Neutral
    write_f32_le(&mut packet, 12, 0.0); // speed
    write_f32_le(&mut packet, 16, 850.0); // idle RPM
    write_f32_le(&mut packet, 48, 0.0); // throttle
    write_f32_le(&mut packet, 52, 1.0); // brake = full
    write_f32_le(&mut packet, 56, 1.0); // clutch = full

    let telem = adapter.normalize(&packet)?;

    assert_eq!(telem.gear, 0, "OutGauge gear 1 should be neutral (0)");
    assert!(
        telem.brake > 0.9,
        "full brake should normalize to ~1.0, got {}",
        telem.brake
    );
    assert!(
        telem.clutch > 0.9,
        "full clutch should normalize to ~1.0, got {}",
        telem.clutch
    );
    assert!(
        telem.throttle < 0.1,
        "zero throttle should normalize to ~0.0, got {}",
        telem.throttle
    );

    Ok(())
}

#[test]
fn lfs_outgauge_gear_reverse_maps_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;

    let mut packet = vec![0u8; 96];
    packet[10] = 0; // gear=0 → Reverse in OutGauge
    write_f32_le(&mut packet, 12, 5.0); // speed
    write_f32_le(&mut packet, 16, 2000.0); // RPM

    let telem = adapter.normalize(&packet)?;
    assert_eq!(telem.gear, -1, "OutGauge gear 0 should map to reverse (-1)");

    Ok(())
}

#[test]
fn lfs_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;

    let short = [0u8; 50];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "undersized LFS OutGauge packet must fail normalization"
    );

    Ok(())
}

// ─── DiRT Rally 2.0 (Codemasters 264-byte format) ────────────────────────────

#[test]
fn dirt_rally_2_packet_round_trips_driving_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("dirt_rally_2")?;

    let mut packet = vec![0u8; 264];
    write_f32_le(&mut packet, 32, 35.0); // vel_x → speed
    write_f32_le(&mut packet, 116, 0.85); // throttle
    write_f32_le(&mut packet, 120, 0.0); // steer
    write_f32_le(&mut packet, 124, 0.25); // brake
    write_f32_le(&mut packet, 132, 5.0); // gear (5th)
    write_f32_le(&mut packet, 148, 7200.0); // rpm
    write_f32_le(&mut packet, 252, 8500.0); // max_rpm

    let telem = adapter.normalize(&packet)?;

    assert!(
        telem.rpm > 7000.0 && telem.rpm < 7400.0,
        "RPM should be ~7200, got {}",
        telem.rpm
    );
    assert!(
        telem.throttle > 0.8 && telem.throttle < 0.9,
        "throttle should be ~0.85, got {}",
        telem.throttle
    );
    assert!(
        telem.brake > 0.2 && telem.brake < 0.3,
        "brake should be ~0.25, got {}",
        telem.brake
    );

    Ok(())
}

#[test]
fn dirt_rally_2_rejects_undersized_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("dirt_rally_2")?;

    let short = [0u8; 40];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "undersized DiRT Rally 2 packet must fail normalization"
    );

    Ok(())
}

// ─── Forza Horizon 5 ─────────────────────────────────────────────────────────

#[test]
fn forza_horizon_5_packet_round_trips_core_fields() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_horizon_5")?;

    // FH5 uses 324-byte packets (same as FH4)
    let mut packet = vec![0u8; 324];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 8500.0); // max_rpm
    write_f32_le(&mut packet, 16, 6000.0); // current_rpm
    write_f32_le(&mut packet, 32, 50.0); // vel_x
    write_f32_le(&mut packet, 36, 0.0); // vel_y
    write_f32_le(&mut packet, 40, 0.0); // vel_z

    let telem = adapter.normalize(&packet)?;

    assert!(
        (telem.rpm - 6000.0).abs() < 1.0,
        "RPM should be ~6000, got {}",
        telem.rpm
    );
    assert!(
        (telem.max_rpm - 8500.0).abs() < 1.0,
        "max RPM should be ~8500, got {}",
        telem.max_rpm
    );

    Ok(())
}

// ─── Cross-adapter: all registered adapters have valid game IDs ──────────────

#[test]
fn all_registered_adapters_have_non_empty_game_ids() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    assert!(
        factories.len() >= 20,
        "expected at least 20 registered adapters, found {}",
        factories.len()
    );

    for (game_id, factory) in factories {
        assert!(
            !game_id.is_empty(),
            "adapter factory must have a non-empty game_id"
        );
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *game_id,
            "adapter game_id() must match factory registration"
        );
    }

    Ok(())
}

#[test]
fn udp_adapters_reject_empty_packets() -> Result<(), Box<dyn std::error::Error>> {
    // Test that UDP-based adapters with fixed packet formats reject empty input.
    // Shared-memory adapters may not validate raw byte input the same way.
    let udp_game_ids = [
        "forza_motorsport",
        "forza_horizon_4",
        "forza_horizon_5",
        "live_for_speed",
        "dirt_rally_2",
        "beamng_drive",
    ];

    let factories = adapter_factories();
    let empty: [u8; 0] = [];

    for game_id in &udp_game_ids {
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| id == game_id)
            .ok_or_else(|| format!("adapter '{game_id}' not in registry"))?;
        let adapter = factory();
        let result = adapter.normalize(&empty);
        assert!(
            result.is_err(),
            "UDP adapter '{game_id}' must reject empty packets"
        );
    }

    Ok(())
}

#[test]
fn adapter_registry_contains_key_titles() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    let required = [
        "forza_motorsport",
        "forza_horizon_4",
        "forza_horizon_5",
        "live_for_speed",
        "dirt_rally_2",
        "acc",
        "iracing",
        "beamng_drive",
        "f1",
    ];

    for game in &required {
        assert!(
            ids.contains(game),
            "adapter registry must include '{game}', available: {ids:?}"
        );
    }

    Ok(())
}
