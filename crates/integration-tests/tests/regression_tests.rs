//! Regression tests for known past bugs.
//!
//! Each test targets a specific bug or edge case that was discovered during
//! development or production use. These tests prevent the bug from recurring.
//!
//! Regressions covered:
//! 1. Zero-torque on any safety fault
//! 2. Proptest ranges (LUT fidelity, drop rate)
//! 3. Moza CRC/signature correctness
//! 4. Fanatec torque encoding boundary values
//! 5. BeamNG 92 vs 96 byte packet handling
//! 6. GT7 flag combinations

use anyhow::Result;

// ── Engine ───────────────────────────────────────────────────────────────────
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline};

// ── Filters ──────────────────────────────────────────────────────────────────
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};

// ── Telemetry adapters ───────────────────────────────────────────────────────
use openracing_telemetry_adapters::gran_turismo_7::{
    MAGIC as GT7_MAGIC, PACKET_SIZE as GT7_PACKET_SIZE, parse_decrypted,
};
use openracing_telemetry_adapters::{TelemetryAdapter, adapter_factories};

// ── Protocol crates ──────────────────────────────────────────────────────────
use racing_wheel_hid_fanatec_protocol::{CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder};
use racing_wheel_hid_moza_protocol::protocol::{FfbMode, effective_ffb_mode, signature_is_trusted};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Zero-torque on any safety fault (was a real bug)
// ═══════════════════════════════════════════════════════════════════════════════

/// Regression: a bug allowed torque to pass through when certain fault types
/// were reported. Every fault variant must result in zero torque output.
#[test]
fn regression_zero_torque_on_every_safety_fault() -> Result<(), Box<dyn std::error::Error>> {
    let all_faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];

    for fault in &all_faults {
        let mut safety = SafetyService::new(8.0, 25.0);

        // Torque passes through before fault
        let pre_fault = safety.clamp_torque_nm(5.0);
        assert!(
            pre_fault.abs() > 0.0,
            "{:?}: torque must be non-zero before fault",
            fault
        );

        // Report the fault
        safety.report_fault(*fault);

        // CRITICAL REGRESSION CHECK: torque MUST be zero in faulted state
        let torque_values = [0.1, 1.0, 5.0, 8.0, 15.0, 25.0, 100.0, -5.0, -25.0];
        for torque_nm in &torque_values {
            let clamped = safety.clamp_torque_nm(*torque_nm);
            assert!(
                clamped.abs() < 0.001,
                "{:?}: torque must be ZERO after fault for input {} Nm, got {}",
                fault,
                torque_nm,
                clamped
            );
        }

        // Verify state is actually Faulted
        assert!(
            matches!(safety.state(), SafetyState::Faulted { .. }),
            "{:?}: state must be Faulted, got {:?}",
            fault,
            safety.state()
        );
    }

    Ok(())
}

/// Regression: pipeline output combined with safety must also be zero.
#[test]
fn regression_pipeline_output_zeroed_after_fault() -> Result<(), Box<dyn std::error::Error>> {
    let mut safety = SafetyService::new(10.0, 20.0);
    let mut pipeline = EnginePipeline::new();

    // Process a high-torque frame
    let mut frame = EngineFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
        wheel_speed: 50.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    pipeline.process(&mut frame)?;

    // Pre-fault: torque should pass through
    let pre = safety.clamp_torque_nm(frame.torque_out * 10.0);
    assert!(
        pre.abs() > 0.0,
        "Pre-fault pipeline torque must be non-zero"
    );

    // Fault
    safety.report_fault(FaultType::Overcurrent);

    // Post-fault: pipeline output must be zeroed
    let post = safety.clamp_torque_nm(frame.torque_out * 10.0);
    assert!(
        post.abs() < 0.001,
        "Post-fault pipeline torque must be zero, got {}",
        post
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Proptest ranges (LUT fidelity, drop rate)
// ═══════════════════════════════════════════════════════════════════════════════

/// Regression: filter chain output must always be finite and bounded for
/// any input within the valid range. Previously, certain combinations of
/// extreme damper/friction values produced NaN or infinite output.
#[test]
fn regression_filter_chain_bounded_for_extreme_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let extreme_inputs: &[(f32, f32)] = &[
        (0.0, 0.0),                             // zero inputs
        (1.0, 0.0),                             // max torque, no speed
        (-1.0, 0.0),                            // max negative torque, no speed
        (1.0, 300.0),                           // max torque, extreme speed
        (-1.0, 300.0),                          // max negative torque, extreme speed
        (0.001, 0.001),                         // tiny values
        (0.999, 299.999),                       // near-boundary values
        (f32::MIN_POSITIVE, f32::MIN_POSITIVE), // smallest positive
    ];

    let damper_coeffs = [0.0f32, 0.01, 0.1, 0.5, 1.0];
    let friction_coeffs = [0.0f32, 0.01, 0.1, 0.5, 1.0];

    for &(ffb_in, wheel_speed) in extreme_inputs {
        for &damp in &damper_coeffs {
            for &fric in &friction_coeffs {
                let damper = DamperState::fixed(damp);
                let friction = FrictionState::fixed(fric);

                let mut frame = FilterFrame {
                    ffb_in,
                    torque_out: ffb_in,
                    wheel_speed,
                    hands_off: false,
                    ts_mono_ns: 0,
                    seq: 0,
                };

                damper_filter(&mut frame, &damper);
                friction_filter(&mut frame, &friction);
                torque_cap_filter(&mut frame, 1.0);

                assert!(
                    frame.torque_out.is_finite(),
                    "Filter output must be finite for ffb={}, speed={}, damp={}, fric={}; got {}",
                    ffb_in,
                    wheel_speed,
                    damp,
                    fric,
                    frame.torque_out
                );
                assert!(
                    frame.torque_out.abs() <= 1.0,
                    "Filter output must be in [-1,1] for ffb={}, speed={}, damp={}, fric={}; got {}",
                    ffb_in,
                    wheel_speed,
                    damp,
                    fric,
                    frame.torque_out
                );
            }
        }
    }

    Ok(())
}

/// Regression: torque cap must clamp all outputs to the specified maximum,
/// including edge cases around the cap boundary.
#[test]
fn regression_torque_cap_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let cap_values = [0.0f32, 0.001, 0.5, 0.999, 1.0];
    let inputs = [-2.0f32, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0];

    for &cap in &cap_values {
        for &input in &inputs {
            let mut frame = FilterFrame {
                ffb_in: input,
                torque_out: input,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: 0,
                seq: 0,
            };

            torque_cap_filter(&mut frame, cap);

            assert!(
                frame.torque_out.abs() <= cap + 0.001,
                "Torque cap {} must limit output {}: got {}",
                cap,
                input,
                frame.torque_out
            );
            assert!(
                frame.torque_out.is_finite(),
                "Output must be finite for cap={}, input={}",
                cap,
                input
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Moza CRC/signature correctness
// ═══════════════════════════════════════════════════════════════════════════════

/// Regression: Moza signature_is_trusted must return false for unknown CRC32
/// values, preventing untrusted devices from accessing high-risk paths.
#[test]
fn regression_moza_unknown_crc_is_untrusted() -> Result<(), Box<dyn std::error::Error>> {
    // Unknown CRC32 values must not be trusted
    let unknown_crcs: &[u32] = &[0x00000000, 0xDEADBEEF, 0x12345678, 0xFFFFFFFF, 0xCAFEBABE];
    for &crc in unknown_crcs {
        assert!(
            !signature_is_trusted(Some(crc)),
            "Unknown CRC32 0x{:08x} must not be trusted",
            crc
        );
    }

    // None descriptor CRC32 must not be trusted
    assert!(
        !signature_is_trusted(None),
        "None CRC32 must not be trusted"
    );

    Ok(())
}

/// Regression: Direct FFB mode must be downgraded to Standard when
/// the device signature is not trusted.
#[test]
fn regression_moza_untrusted_downgrades_direct_to_standard()
-> Result<(), Box<dyn std::error::Error>> {
    // Untrusted signature: Direct → Standard
    let mode = effective_ffb_mode(FfbMode::Direct, None);
    assert_eq!(
        mode,
        FfbMode::Standard,
        "Direct mode must downgrade to Standard for untrusted signature"
    );

    // Untrusted with unknown CRC: still Standard
    let mode2 = effective_ffb_mode(FfbMode::Direct, Some(0xDEADBEEF));
    assert_eq!(
        mode2,
        FfbMode::Standard,
        "Direct mode must downgrade for unknown CRC32"
    );

    // Standard mode passes through regardless
    let mode3 = effective_ffb_mode(FfbMode::Standard, None);
    assert_eq!(
        mode3,
        FfbMode::Standard,
        "Standard mode must pass through unchanged"
    );

    // Off mode passes through regardless
    let mode4 = effective_ffb_mode(FfbMode::Off, None);
    assert_eq!(mode4, FfbMode::Off, "Off mode must pass through unchanged");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Fanatec torque encoding boundary values
// ═══════════════════════════════════════════════════════════════════════════════

/// Regression: Fanatec constant force encoder must handle boundary values
/// correctly, including zero torque, max positive, max negative, and beyond-limit.
#[test]
fn regression_fanatec_torque_encoding_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(8.0); // 8 Nm max

    let test_cases: &[(f32, &str)] = &[
        (0.0, "zero torque"),
        (8.0, "max positive (1.0)"),
        (-8.0, "max negative (-1.0)"),
        (4.0, "half positive (0.5)"),
        (-4.0, "half negative (-0.5)"),
        (16.0, "beyond max (clamped to 1.0)"),
        (-16.0, "beyond min (clamped to -1.0)"),
        (0.001, "tiny positive"),
        (-0.001, "tiny negative"),
    ];

    for &(torque_nm, label) in test_cases {
        let mut report = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(torque_nm, 0, &mut report);

        assert_eq!(
            len, CONSTANT_FORCE_REPORT_LEN,
            "{label}: encoded length must be {CONSTANT_FORCE_REPORT_LEN}"
        );

        // Decode the force value from the report
        let force_raw = i16::from_le_bytes([report[2], report[3]]);

        match label {
            "zero torque" => {
                assert_eq!(force_raw, 0, "{label}: zero torque must encode to 0");
            }
            "max positive (1.0)" => {
                assert_eq!(
                    force_raw,
                    i16::MAX,
                    "{label}: max positive must encode to i16::MAX ({}), got {}",
                    i16::MAX,
                    force_raw
                );
            }
            "max negative (-1.0)" => {
                assert_eq!(
                    force_raw,
                    i16::MIN,
                    "{label}: max negative must encode to i16::MIN ({}), got {}",
                    i16::MIN,
                    force_raw
                );
            }
            "beyond max (clamped to 1.0)" => {
                assert_eq!(
                    force_raw,
                    i16::MAX,
                    "{label}: beyond-max must clamp to i16::MAX, got {}",
                    force_raw
                );
            }
            "beyond min (clamped to -1.0)" => {
                assert_eq!(
                    force_raw,
                    i16::MIN,
                    "{label}: beyond-min must clamp to i16::MIN, got {}",
                    force_raw
                );
            }
            _ => {
                // For intermediate values, just verify the sign is correct
                if torque_nm > 0.0 {
                    assert!(
                        force_raw > 0,
                        "{label}: positive torque must encode to positive raw, got {}",
                        force_raw
                    );
                } else if torque_nm < 0.0 {
                    assert!(
                        force_raw < 0,
                        "{label}: negative torque must encode to negative raw, got {}",
                        force_raw
                    );
                }
            }
        }
    }

    Ok(())
}

/// Regression: Zero max torque must produce zero output for any input.
#[test]
fn regression_fanatec_zero_max_torque_encodes_zero() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(0.0);

    let inputs = [0.0f32, 1.0, -1.0, 100.0, -100.0];
    for input in &inputs {
        let mut report = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(*input, 0, &mut report);

        let force_raw = i16::from_le_bytes([report[2], report[3]]);
        assert_eq!(
            force_raw, 0,
            "Zero max_torque must encode 0 for input {}, got {}",
            input, force_raw
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. BeamNG 92 vs 96 byte packet handling
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper to find an adapter by game ID.
fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

/// Helper: write f32 LE at offset.
fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Regression: BeamNG must accept both 92-byte (base OutGauge) and 96-byte
/// (OutGauge with optional id field) packets.
#[test]
fn regression_beamng_92_byte_packet_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("beamng_drive")?;

    // 92-byte OutGauge packet (minimum valid size)
    let mut packet_92 = vec![0u8; 92];
    write_f32_le(&mut packet_92, 12, 25.0); // speed m/s
    write_f32_le(&mut packet_92, 16, 5000.0); // RPM
    packet_92[10] = 3; // gear: OutGauge 3 = 2nd gear

    let result = adapter.normalize(&packet_92);
    assert!(
        result.is_ok(),
        "BeamNG must accept 92-byte OutGauge packet, got {:?}",
        result.as_ref().err()
    );

    let telem = result?;
    assert!(
        (telem.speed_ms - 25.0).abs() < 0.5,
        "Speed must be ~25 m/s, got {}",
        telem.speed_ms
    );

    Ok(())
}

#[test]
fn regression_beamng_96_byte_packet_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("beamng_drive")?;

    // 96-byte OutGauge packet (with optional id field)
    let mut packet_96 = vec![0u8; 96];
    write_f32_le(&mut packet_96, 12, 30.0); // speed m/s
    write_f32_le(&mut packet_96, 16, 6000.0); // RPM
    packet_96[10] = 4; // gear: OutGauge 4 = 3rd gear

    let result = adapter.normalize(&packet_96);
    assert!(
        result.is_ok(),
        "BeamNG must accept 96-byte OutGauge packet, got {:?}",
        result.as_ref().err()
    );

    let telem = result?;
    assert!(
        (telem.speed_ms - 30.0).abs() < 0.5,
        "Speed must be ~30 m/s, got {}",
        telem.speed_ms
    );

    Ok(())
}

/// Regression: packets shorter than 92 bytes must be rejected.
#[test]
fn regression_beamng_short_packet_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("beamng_drive")?;

    let short_sizes = [0, 1, 50, 91];
    for size in &short_sizes {
        let packet = vec![0u8; *size];
        let result = adapter.normalize(&packet);
        assert!(result.is_err(), "BeamNG must reject {}-byte packet", size);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. GT7 flag combinations
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: build a minimal valid GT7 decrypted packet with magic and specified flags.
fn build_gt7_packet_with_flags(flags: u16) -> [u8; GT7_PACKET_SIZE] {
    let mut buf = [0u8; GT7_PACKET_SIZE];
    // Write magic at offset 0
    buf[0..4].copy_from_slice(&GT7_MAGIC.to_le_bytes());
    // Write flags at offset 0x8E (142)
    buf[0x8E..0x90].copy_from_slice(&flags.to_le_bytes());
    // Write some RPM so the packet is "active"
    let rpm: f32 = 3000.0;
    buf[0x3C..0x40].copy_from_slice(&rpm.to_le_bytes());
    buf
}

// GT7 flag constants (matching the adapter's private constants)
const GT7_FLAG_PAUSED: u16 = 1 << 1;
const GT7_FLAG_REV_LIMIT: u16 = 1 << 5;
const GT7_FLAG_ASM_ACTIVE: u16 = 1 << 10;
const GT7_FLAG_TCS_ACTIVE: u16 = 1 << 11;

/// Regression: individual GT7 flags must map correctly to TelemetryFlags fields.
#[test]
fn regression_gt7_individual_flag_mapping() -> Result<(), Box<dyn std::error::Error>> {
    // TCS only
    let buf = build_gt7_packet_with_flags(GT7_FLAG_TCS_ACTIVE);
    let telem = parse_decrypted(&buf)?;
    assert!(telem.flags.traction_control, "TCS flag must be set");
    assert!(!telem.flags.abs_active, "ABS must not be set for TCS-only");
    assert!(
        !telem.flags.engine_limiter,
        "Rev limiter must not be set for TCS-only"
    );
    assert!(
        !telem.flags.session_paused,
        "Paused must not be set for TCS-only"
    );

    // ASM/ABS only
    let buf = build_gt7_packet_with_flags(GT7_FLAG_ASM_ACTIVE);
    let telem = parse_decrypted(&buf)?;
    assert!(telem.flags.abs_active, "ABS flag must be set");
    assert!(
        !telem.flags.traction_control,
        "TCS must not be set for ABS-only"
    );

    // Rev limiter only
    let buf = build_gt7_packet_with_flags(GT7_FLAG_REV_LIMIT);
    let telem = parse_decrypted(&buf)?;
    assert!(telem.flags.engine_limiter, "Rev limiter flag must be set");
    assert!(
        !telem.flags.traction_control,
        "TCS must not be set for rev-limit-only"
    );
    assert!(
        !telem.flags.abs_active,
        "ABS must not be set for rev-limit-only"
    );

    // Paused only
    let buf = build_gt7_packet_with_flags(GT7_FLAG_PAUSED);
    let telem = parse_decrypted(&buf)?;
    assert!(telem.flags.session_paused, "Paused flag must be set");
    assert!(
        !telem.flags.traction_control,
        "TCS must not be set for paused-only"
    );

    Ok(())
}

/// Regression: all GT7 flags set simultaneously must all be observable.
#[test]
fn regression_gt7_all_flags_simultaneously() -> Result<(), Box<dyn std::error::Error>> {
    let all_flags =
        GT7_FLAG_TCS_ACTIVE | GT7_FLAG_ASM_ACTIVE | GT7_FLAG_REV_LIMIT | GT7_FLAG_PAUSED;
    let buf = build_gt7_packet_with_flags(all_flags);
    let telem = parse_decrypted(&buf)?;

    assert!(
        telem.flags.traction_control,
        "TCS must be set with all flags"
    );
    assert!(telem.flags.abs_active, "ABS must be set with all flags");
    assert!(
        telem.flags.engine_limiter,
        "Rev limiter must be set with all flags"
    );
    assert!(
        telem.flags.session_paused,
        "Paused must be set with all flags"
    );

    Ok(())
}

/// Regression: zero flags means no assists/state flags active.
#[test]
fn regression_gt7_no_flags() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_gt7_packet_with_flags(0);
    let telem = parse_decrypted(&buf)?;

    assert!(
        !telem.flags.traction_control,
        "TCS must not be set with zero flags"
    );
    assert!(
        !telem.flags.abs_active,
        "ABS must not be set with zero flags"
    );
    assert!(
        !telem.flags.engine_limiter,
        "Rev limiter must not be set with zero flags"
    );
    assert!(
        !telem.flags.session_paused,
        "Paused must not be set with zero flags"
    );

    Ok(())
}

/// Regression: GT7 packet telemetry values must be normalized correctly.
#[test]
fn regression_gt7_telemetry_values_normalized() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; GT7_PACKET_SIZE];
    buf[0..4].copy_from_slice(&GT7_MAGIC.to_le_bytes());

    // RPM at offset 0x3C
    let rpm: f32 = 7200.0;
    buf[0x3C..0x40].copy_from_slice(&rpm.to_le_bytes());

    // Speed at offset 0x4C
    let speed: f32 = 45.0;
    buf[0x4C..0x50].copy_from_slice(&speed.to_le_bytes());

    // Throttle at offset 0x91 (u8, 0-255)
    buf[0x91] = 204; // ~80%

    // Brake at offset 0x92 (u8, 0-255)
    buf[0x92] = 0; // 0%

    // Gear at offset 0x90 (low nibble)
    buf[0x90] = 4; // 4th gear

    let telem = parse_decrypted(&buf)?;

    assert!(
        (telem.rpm - 7200.0).abs() < 1.0,
        "RPM must be ~7200, got {}",
        telem.rpm
    );
    assert!(
        (telem.speed_ms - 45.0).abs() < 0.5,
        "Speed must be ~45 m/s, got {}",
        telem.speed_ms
    );
    assert!(
        (telem.throttle - 0.8).abs() < 0.01,
        "Throttle must be ~0.8, got {}",
        telem.throttle
    );
    assert!(
        telem.brake.abs() < 0.01,
        "Brake must be ~0.0, got {}",
        telem.brake
    );
    assert_eq!(telem.gear, 4, "Gear must be 4, got {}", telem.gear);

    // All numeric fields must be finite
    assert!(telem.speed_ms.is_finite(), "speed_ms must be finite");
    assert!(telem.rpm.is_finite(), "rpm must be finite");
    assert!(telem.throttle.is_finite(), "throttle must be finite");
    assert!(telem.brake.is_finite(), "brake must be finite");

    Ok(())
}
