//! Cross-crate normalization consistency integration tests.
//!
//! Validates multi-vendor force-feedback normalization, game telemetry pipeline
//! correctness, and device identification across all supported vendor protocols.

// ─── Multi-vendor normalization ───────────────────────────────────────────────

use racing_wheel_hid_fanatec_protocol::CONSTANT_FORCE_REPORT_LEN as FANATEC_REPORT_LEN;
use racing_wheel_hid_fanatec_protocol::FanatecConstantForceEncoder;

use racing_wheel_hid_logitech_protocol::CONSTANT_FORCE_REPORT_LEN as LOGITECH_REPORT_LEN;
use racing_wheel_hid_logitech_protocol::LogitechConstantForceEncoder;

use racing_wheel_hid_thrustmaster_protocol::EFFECT_REPORT_LEN as THRUSTMASTER_REPORT_LEN;
use racing_wheel_hid_thrustmaster_protocol::ThrustmasterConstantForceEncoder;

/// Extract signed 16-bit LE force value from bytes 2–3.
fn extract_force_i16(buf: &[u8]) -> i16 {
    i16::from_le_bytes([buf[2], buf[3]])
}

/// Write a little-endian f32 at the given offset in a byte buffer.
fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Write a little-endian i32 at the given offset in a byte buffer.
fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Encode a torque for all three vendors and return the normalized fraction
/// (force / full_scale) for each, allowing cross-vendor comparison.
fn encode_all_vendors_normalized(fraction: f32) -> (f32, f32, f32) {
    // Fanatec: 8 Nm max, i16 full-scale = 32767
    let fanatec = FanatecConstantForceEncoder::new(8.0);
    let mut fb = [0u8; FANATEC_REPORT_LEN];
    fanatec.encode(fraction * 8.0, 0, &mut fb);
    let f_force = extract_force_i16(&fb) as f32 / i16::MAX as f32;

    // Logitech: 2.2 Nm max, ±10000 scale
    let logitech = LogitechConstantForceEncoder::new(2.2);
    let mut lb = [0u8; LOGITECH_REPORT_LEN];
    logitech.encode(fraction * 2.2, &mut lb);
    let l_force = extract_force_i16(&lb) as f32 / 10_000.0;

    // Thrustmaster: 3.9 Nm max, ±10000 scale
    let thrustmaster = ThrustmasterConstantForceEncoder::new(3.9);
    let mut tb = [0u8; THRUSTMASTER_REPORT_LEN];
    thrustmaster.encode(fraction * 3.9, &mut tb);
    let t_force = extract_force_i16(&tb) as f32 / 10_000.0;

    (f_force, l_force, t_force)
}

#[test]
fn all_vendors_zero_torque_encodes_to_zero_force() -> Result<(), Box<dyn std::error::Error>> {
    let (fan, logi, tm) = encode_all_vendors_normalized(0.0);

    assert!(fan.abs() < f32::EPSILON, "Fanatec zero force: got {fan}");
    assert!(logi.abs() < f32::EPSILON, "Logitech zero force: got {logi}");
    assert!(tm.abs() < f32::EPSILON, "Thrustmaster zero force: got {tm}");

    Ok(())
}

#[test]
fn all_vendors_full_torque_encodes_to_full_scale() -> Result<(), Box<dyn std::error::Error>> {
    let (fan, logi, tm) = encode_all_vendors_normalized(1.0);

    assert!((fan - 1.0).abs() < 0.001, "Fanatec full scale: got {fan}");
    assert!(
        (logi - 1.0).abs() < 0.001,
        "Logitech full scale: got {logi}"
    );
    assert!(
        (tm - 1.0).abs() < 0.001,
        "Thrustmaster full scale: got {tm}"
    );

    Ok(())
}

#[test]
fn all_vendors_negative_full_torque_encodes_to_negative_full_scale()
-> Result<(), Box<dyn std::error::Error>> {
    let (fan, logi, tm) = encode_all_vendors_normalized(-1.0);

    assert!(
        (fan + 1.0).abs() < 0.002,
        "Fanatec neg full scale: got {fan}"
    );
    assert!(
        (logi + 1.0).abs() < 0.001,
        "Logitech neg full scale: got {logi}"
    );
    assert!(
        (tm + 1.0).abs() < 0.001,
        "Thrustmaster neg full scale: got {tm}"
    );

    Ok(())
}

#[test]
fn all_vendors_half_torque_produces_proportional_output() -> Result<(), Box<dyn std::error::Error>>
{
    let (fan, logi, tm) = encode_all_vendors_normalized(0.5);

    // All three should produce ≈0.5 within small quantization tolerance
    assert!(
        (fan - 0.5).abs() < 0.001,
        "Fanatec 50% normalized: got {fan}"
    );
    assert!(
        (logi - 0.5).abs() < 0.001,
        "Logitech 50% normalized: got {logi}"
    );
    assert!(
        (tm - 0.5).abs() < 0.001,
        "Thrustmaster 50% normalized: got {tm}"
    );

    // Cross-vendor: all three should agree within 0.002
    assert!(
        (fan - logi).abs() < 0.002,
        "Fanatec vs Logitech divergence: {fan} vs {logi}"
    );
    assert!(
        (logi - tm).abs() < 0.002,
        "Logitech vs Thrustmaster divergence: {logi} vs {tm}"
    );

    Ok(())
}

// ─── Game telemetry pipeline ──────────────────────────────────────────────────

use openracing_telemetry_adapters::adapter_factories;

/// Look up an adapter by game_id from the factory registry.
fn get_adapter(
    game_id: &str,
) -> Result<Box<dyn openracing_telemetry_adapters::TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

#[test]
fn forza_sled_packet_normalizes_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // Construct a valid 232-byte Sled packet
    let mut packet = vec![0u8; 232];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 8000.0); // engine_max_rpm
    write_f32_le(&mut packet, 16, 6000.0); // current_rpm
    write_f32_le(&mut packet, 32, 30.0); // vel_x
    write_f32_le(&mut packet, 36, 0.0); // vel_y
    write_f32_le(&mut packet, 40, 40.0); // vel_z

    let telem = adapter.normalize(&packet)?;

    // speed = sqrt(30² + 0² + 40²) = 50.0 m/s
    let expected_speed = (30.0f32.powi(2) + 40.0f32.powi(2)).sqrt();
    assert!(
        (telem.speed_ms - expected_speed).abs() < 1.0,
        "speed should be ~{expected_speed} m/s, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.rpm - 6000.0).abs() < 1.0,
        "RPM should be ~6000, got {}",
        telem.rpm
    );
    assert!(
        (telem.max_rpm - 8000.0).abs() < 1.0,
        "max RPM should be ~8000, got {}",
        telem.max_rpm
    );

    Ok(())
}

#[test]
fn forza_sled_rejects_truncated_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // 100 bytes is too short for any Forza format (min 232)
    let short = [0u8; 100];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "truncated Forza packet must fail normalization"
    );

    Ok(())
}

#[test]
fn forza_cardash_packet_normalizes_user_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;

    // Build a 311-byte CarDash packet
    let mut packet = vec![0u8; 311];
    write_i32_le(&mut packet, 0, 1); // is_race_on = 1
    write_f32_le(&mut packet, 8, 8000.0); // max_rpm
    write_f32_le(&mut packet, 16, 5500.0); // current_rpm
    write_f32_le(&mut packet, 32, 25.0); // vel_x
    write_f32_le(&mut packet, 244, 25.0); // CarDash speed field
    packet[303] = 255; // throttle: full
    packet[304] = 0; // brake: none
    packet[307] = 4; // gear byte (4 = 3rd gear in Forza's 0=R,1=N,2=1st scheme)

    let telem = adapter.normalize(&packet)?;

    assert!(
        telem.throttle > 0.9,
        "full throttle byte 255 should normalize to ~1.0, got {}",
        telem.throttle
    );
    assert!(
        telem.brake < 0.1,
        "zero brake byte should normalize to ~0.0, got {}",
        telem.brake
    );
    assert!(
        telem.gear > 0,
        "gear should be a positive forward gear, got {}",
        telem.gear
    );

    Ok(())
}

#[test]
fn dirt_rally_2_packet_normalizes_core_fields() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("dirt_rally_2")?;

    // Build a 264-byte Codemasters Mode 1 packet
    let mut packet = vec![0u8; 264];
    write_f32_le(&mut packet, 32, 20.0); // vel_x → speed
    write_f32_le(&mut packet, 116, 0.75); // throttle
    write_f32_le(&mut packet, 120, -0.3); // steer
    write_f32_le(&mut packet, 124, 0.5); // brake
    write_f32_le(&mut packet, 132, 3.0); // gear (3rd)
    write_f32_le(&mut packet, 148, 5500.0); // rpm
    write_f32_le(&mut packet, 252, 7500.0); // max_rpm

    let telem = adapter.normalize(&packet)?;

    assert!(
        telem.rpm > 5000.0 && telem.rpm < 6000.0,
        "RPM should be ~5500, got {}",
        telem.rpm
    );
    assert!(
        telem.throttle > 0.7 && telem.throttle < 0.8,
        "throttle should be ~0.75, got {}",
        telem.throttle
    );
    assert!(
        telem.brake > 0.4 && telem.brake < 0.6,
        "brake should be ~0.5, got {}",
        telem.brake
    );
    assert!(
        telem.gear >= 2 && telem.gear <= 4,
        "gear should map to ~3, got {}",
        telem.gear
    );

    Ok(())
}

#[test]
fn dirt_rally_2_rejects_truncated_packet() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("dirt_rally_2")?;

    let short = [0u8; 50];
    let result = adapter.normalize(&short);
    assert!(
        result.is_err(),
        "truncated DiRT Rally 2 packet must fail normalization"
    );

    Ok(())
}

#[test]
fn telemetry_registry_contains_major_game_adapters() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    let required = [
        "forza_motorsport",
        "acc",
        "iracing",
        "dirt_rally_2",
        "f1",
        "beamng_drive",
    ];

    for game in &required {
        assert!(
            ids.contains(game),
            "adapter registry must include '{game}', available: {ids:?}"
        );
    }

    // Registry should have a healthy number of adapters
    assert!(
        ids.len() >= 15,
        "expected at least 15 telemetry adapters, found {}",
        ids.len()
    );

    Ok(())
}

// ─── Device identification matrix ─────────────────────────────────────────────

use racing_wheel_hid_fanatec_protocol::{
    FanatecModel, is_pedal_product, is_wheelbase_product, product_ids as fanatec_pids,
};
use racing_wheel_hid_logitech_protocol::{
    LogitechModel, is_wheel_product, product_ids as logitech_pids,
};
use racing_wheel_hid_thrustmaster_protocol::{
    ThrustmasterDeviceCategory, identify_device, product_ids as tm_pids,
};

#[test]
fn fanatec_all_wheelbase_pids_identified() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_pids = [
        fanatec_pids::DD1,
        fanatec_pids::DD2,
        fanatec_pids::CSL_DD,
        fanatec_pids::GT_DD_PRO,
        fanatec_pids::CLUBSPORT_DD,
        fanatec_pids::CLUBSPORT_V2,
        fanatec_pids::CLUBSPORT_V2_5,
        fanatec_pids::CSL_ELITE,
        fanatec_pids::CSL_ELITE_PS4,
        fanatec_pids::CSR_ELITE,
    ];

    for pid in &wheelbase_pids {
        assert!(
            is_wheelbase_product(*pid),
            "PID 0x{pid:04X} must be identified as a Fanatec wheelbase"
        );
        assert!(
            !is_pedal_product(*pid),
            "Wheelbase PID 0x{pid:04X} must NOT be classified as pedals"
        );
    }

    Ok(())
}

#[test]
fn fanatec_pedal_pids_are_not_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let pedal_pids = [
        fanatec_pids::CLUBSPORT_PEDALS_V3,
        fanatec_pids::CSL_ELITE_PEDALS,
        fanatec_pids::CSL_PEDALS_LC,
    ];

    for pid in &pedal_pids {
        assert!(
            is_pedal_product(*pid),
            "PID 0x{pid:04X} must be identified as Fanatec pedals"
        );
        assert!(
            !is_wheelbase_product(*pid),
            "Pedal PID 0x{pid:04X} must NOT be classified as a wheelbase"
        );
    }

    Ok(())
}

#[test]
fn fanatec_model_torque_ratings_correct() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, f32)] = &[
        (fanatec_pids::DD1, 20.0),
        (fanatec_pids::DD2, 25.0),
        (fanatec_pids::CSL_DD, 8.0),
        (fanatec_pids::GT_DD_PRO, 8.0),
        (fanatec_pids::CLUBSPORT_DD, 12.0),
        (fanatec_pids::CSL_ELITE, 6.0),
    ];

    for (pid, expected_nm) in cases {
        let model = FanatecModel::from_product_id(*pid);
        let actual = model.max_torque_nm();
        assert!(
            (actual - expected_nm).abs() < 0.1,
            "Fanatec PID 0x{pid:04X}: expected {expected_nm} Nm, got {actual} Nm"
        );
    }

    Ok(())
}

#[test]
fn thrustmaster_device_identification_matrix() -> Result<(), Box<dyn std::error::Error>> {
    // FFB-capable wheelbases
    let ffb_wheelbases = [
        (tm_pids::T300_RS, "T300"),
        (tm_pids::T150, "T150"),
        (tm_pids::TX_RACING, "TX"),
        (tm_pids::T500_RS, "T500"),
        (tm_pids::T818, "T818"),
        (tm_pids::T248, "T248"),
        (tm_pids::TS_PC_RACER, "TS-PC"),
    ];

    for (pid, label) in &ffb_wheelbases {
        let ident = identify_device(*pid);
        assert_eq!(
            ident.category,
            ThrustmasterDeviceCategory::Wheelbase,
            "{label} (0x{pid:04X}) must be Wheelbase category"
        );
        assert!(ident.supports_ffb, "{label} (0x{pid:04X}) must support FFB");
        assert!(
            !ident.name.is_empty(),
            "{label} (0x{pid:04X}) must have a non-empty name"
        );
    }

    // T80: wheelbase but NO FFB
    let t80 = identify_device(tm_pids::T80);
    assert_eq!(
        t80.category,
        ThrustmasterDeviceCategory::Wheelbase,
        "T80 must be a Wheelbase"
    );
    assert!(!t80.supports_ffb, "T80 must NOT support FFB (rumble-only)");

    // Unknown PID
    let unknown = identify_device(0xFF00);
    assert_eq!(
        unknown.category,
        ThrustmasterDeviceCategory::Unknown,
        "unrecognised PID must be Unknown category"
    );
    assert!(
        !unknown.supports_ffb,
        "unknown PID must not claim FFB support"
    );

    Ok(())
}

#[test]
fn logitech_all_wheel_pids_identified() -> Result<(), Box<dyn std::error::Error>> {
    let wheel_pids = [
        logitech_pids::G920,
        logitech_pids::G923_PS,
        logitech_pids::G923_XBOX,
        logitech_pids::G_PRO,
        logitech_pids::G29_PS,
        logitech_pids::G25,
        logitech_pids::G27,
    ];

    for pid in &wheel_pids {
        assert!(
            is_wheel_product(*pid),
            "Logitech PID 0x{pid:04X} must be identified as a wheel product"
        );
    }

    // Unknown PID should not be a wheel
    assert!(
        !is_wheel_product(0xFF00),
        "unknown PID 0xFF00 must not be a wheel product"
    );

    Ok(())
}

#[test]
fn logitech_model_torque_ratings_correct() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, f32)] = &[
        (logitech_pids::G920, 2.2),
        (logitech_pids::G29_PS, 2.2),
        (logitech_pids::G923_PS, 2.2),
        (logitech_pids::G25, 2.5),
        (logitech_pids::G27, 2.5),
        (logitech_pids::G_PRO, 11.0),
    ];

    for (pid, expected_nm) in cases {
        let model = LogitechModel::from_product_id(*pid);
        let actual = model.max_torque_nm();
        assert!(
            (actual - expected_nm).abs() < 0.1,
            "Logitech PID 0x{pid:04X}: expected {expected_nm} Nm, got {actual} Nm"
        );
    }

    Ok(())
}
