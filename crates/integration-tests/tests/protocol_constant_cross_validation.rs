//! Cross-crate validation of protocol constants.
//!
//! Ensures that VID/PID constants defined in individual HID protocol crates
//! are consistent with the engine's device dispatch tables, that product IDs
//! are unique within each vendor, that telemetry adapter game IDs match the
//! config/registry, and that device names are non-empty and non-placeholder.

use std::collections::{HashMap, HashSet};

use openracing_telemetry_adapters::adapter_factories;
use racing_wheel_engine::hid::vendor::get_vendor_protocol;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═════════════════════════════════════════════════════════════════════════════
// 1. Protocol VID constants match engine dispatch tables
// ═════════════════════════════════════════════════════════════════════════════

/// Every VID/PID pair exported by protocol crates must be routable through
/// `get_vendor_protocol()`. A mismatch means a supported device would be
/// silently ignored at runtime.
#[test]
fn protocol_vid_pid_pairs_dispatch_through_engine() -> TestResult {
    // (VID, PID, label) — one representative per vendor + key products
    let cases: &[(u16, u16, &str)] = &[
        // Logitech
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G920,
            "Logitech G920",
        ),
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
            "Logitech G29",
        ),
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
            "Logitech G PRO",
        ),
        // Fanatec
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
            "Fanatec CSL DD",
        ),
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::DD1,
            "Fanatec DD1",
        ),
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::DD2,
            "Fanatec DD2",
        ),
        // Thrustmaster
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
            "Thrustmaster T300 RS",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T818,
            "Thrustmaster T818",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
            "Thrustmaster TS-PC Racer",
        ),
        // Simagic (modern VID)
        (
            racing_wheel_hid_simagic_protocol::SIMAGIC_VENDOR_ID,
            racing_wheel_hid_simagic_protocol::product_ids::EVO,
            "Simagic EVO",
        ),
        // Moza
        (
            racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID,
            racing_wheel_hid_moza_protocol::product_ids::R9_V1,
            "Moza R9",
        ),
        // PXN
        (
            racing_wheel_hid_pxn_protocol::VENDOR_ID,
            racing_wheel_hid_pxn_protocol::PRODUCT_V10,
            "PXN V10",
        ),
        (
            racing_wheel_hid_pxn_protocol::VENDOR_ID,
            racing_wheel_hid_pxn_protocol::PRODUCT_V12,
            "PXN V12",
        ),
        // Cammus
        (
            racing_wheel_hid_cammus_protocol::VENDOR_ID,
            racing_wheel_hid_cammus_protocol::PRODUCT_C5,
            "Cammus C5",
        ),
        (
            racing_wheel_hid_cammus_protocol::VENDOR_ID,
            racing_wheel_hid_cammus_protocol::PRODUCT_C12,
            "Cammus C12",
        ),
        // Simucube
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
            "Simucube 2 Sport",
        ),
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
            "Simucube 2 Pro",
        ),
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
            "Simucube 2 Ultimate",
        ),
        // VRS (shared STM VID)
        (
            racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID,
            racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID,
            "VRS DirectForce Pro",
        ),
        // Asetek
        (
            hid_asetek_protocol::VENDOR_ID,
            hid_asetek_protocol::PRODUCT_ID_FORTE,
            "Asetek Forte",
        ),
        (
            hid_asetek_protocol::VENDOR_ID,
            hid_asetek_protocol::PRODUCT_ID_INVICTA,
            "Asetek Invicta",
        ),
        // AccuForce
        (
            racing_wheel_hid_accuforce_protocol::VENDOR_ID,
            racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO,
            "AccuForce Pro",
        ),
        // Leo Bodnar
        (
            racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID,
            racing_wheel_hid_leo_bodnar_protocol::PID_WHEEL_INTERFACE,
            "Leo Bodnar Wheel Interface",
        ),
        // FFBeast
        (
            racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID,
            racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
            "FFBeast Wheel",
        ),
        // OpenFFBoard
        (
            racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID,
            racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
            "OpenFFBoard",
        ),
        // Cube Controls (shared STM VID)
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_VENDOR_ID,
            hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID,
            "Cube Controls GT Pro",
        ),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (vid, pid, label) in cases {
        if get_vendor_protocol(*vid, *pid).is_none() {
            failures.push(format!(
                "{label} (VID 0x{vid:04X}, PID 0x{pid:04X}) not dispatched by engine"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Protocol VID/PID constants not routable through engine dispatch:\n  {}",
        failures.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. PID uniqueness within each vendor (no collisions)
// ═════════════════════════════════════════════════════════════════════════════

/// All product IDs within a single vendor must be unique. Duplicate PIDs
/// would cause silent mis-identification of hardware at runtime.
#[test]
fn protocol_pids_unique_within_vendor() -> TestResult {
    // Collect (VID, PID, label) for all known products
    let all_products: &[(u16, u16, &str)] = &[
        // Logitech (VID 0x046D)
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::MOMO,
            "MOMO",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_PRO,
            "DFP",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_GT,
            "DFGT",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::SPEED_FORCE_WIRELESS,
            "SFW",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::MOMO_2,
            "MOMO 2",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE_GP,
            "WFFGP",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE,
            "WFF",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::VIBRATION_WHEEL,
            "Vibration",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G25,
            "G25",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_EX,
            "DF-EX",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G27,
            "G27",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
            "G29 PS",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G920,
            "G920",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G923,
            "G923",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G923_PS,
            "G923 PS",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX,
            "G923 Xbox",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX_ALT,
            "G923 Xbox Alt",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
            "G PRO",
        ),
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G_PRO_XBOX,
            "G PRO Xbox",
        ),
        // Fanatec (VID 0x0EB7)
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2,
            "CS V2",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2_5,
            "CS V2.5",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE_PS4,
            "CSL Elite PS4",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::DD1,
            "DD1",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::DD2,
            "DD2",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CSR_ELITE,
            "CSR Elite",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
            "CSL DD",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::GT_DD_PRO,
            "GT DD Pro",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE,
            "CSL Elite",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_DD,
            "CS DD",
        ),
        // Thrustmaster (VID 0x044F)
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T150,
            "T150",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
            "T300 RS",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_PS4,
            "T300 RS PS4",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_GT,
            "T300 RS GT",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TX_RACING,
            "TX Racing",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T500_RS,
            "T500 RS",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T248,
            "T248",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T818,
            "T818",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
            "TS-PC Racer",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TS_XW,
            "TS-XW",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TMX,
            "TMX",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T248X,
            "T248X",
        ),
        // PXN (VID 0x11FF)
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V10, "V10"),
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12, "V12"),
        (
            0x11FF,
            racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE,
            "V12 Lite",
        ),
        (
            0x11FF,
            racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE_2,
            "V12 Lite 2",
        ),
        (
            0x11FF,
            racing_wheel_hid_pxn_protocol::PRODUCT_GT987,
            "GT987",
        ),
        // Cammus (VID 0x3416)
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_C5, "C5"),
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_C12, "C12"),
        (
            0x3416,
            racing_wheel_hid_cammus_protocol::PRODUCT_CP5_PEDALS,
            "CP5 Pedals",
        ),
        (
            0x3416,
            racing_wheel_hid_cammus_protocol::PRODUCT_LC100_PEDALS,
            "LC100 Pedals",
        ),
        // Simucube (VID 0x16D0)
        (0x16D0, hid_simucube_protocol::SIMUCUBE_1_PID, "Simucube 1"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_SPORT_PID, "Sport"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_PRO_PID, "Pro"),
        (
            0x16D0,
            hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
            "Ultimate",
        ),
        (
            0x16D0,
            hid_simucube_protocol::SIMUCUBE_ACTIVE_PEDAL_PID,
            "ActivePedal",
        ),
        (
            0x16D0,
            hid_simucube_protocol::SIMUCUBE_WIRELESS_WHEEL_PID,
            "Wireless Wheel",
        ),
        // Simagic EVO (VID 0x3670)
        (
            0x3670,
            racing_wheel_hid_simagic_protocol::product_ids::EVO_SPORT,
            "EVO Sport",
        ),
        (
            0x3670,
            racing_wheel_hid_simagic_protocol::product_ids::EVO,
            "EVO",
        ),
        (
            0x3670,
            racing_wheel_hid_simagic_protocol::product_ids::EVO_PRO,
            "EVO Pro",
        ),
        // FFBeast (VID 0x045B)
        (
            0x045B,
            racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_JOYSTICK,
            "Joystick",
        ),
        (
            0x045B,
            racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_RUDDER,
            "Rudder",
        ),
        (
            0x045B,
            racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
            "Wheel",
        ),
        // Asetek (VID 0x2433)
        (0x2433, hid_asetek_protocol::ASETEK_INVICTA_PID, "Invicta"),
        (0x2433, hid_asetek_protocol::ASETEK_FORTE_PID, "Forte"),
        (0x2433, hid_asetek_protocol::ASETEK_LAPRIMA_PID, "La Prima"),
        (
            0x2433,
            hid_asetek_protocol::ASETEK_TONY_KANAAN_PID,
            "Tony Kanaan",
        ),
    ];

    // Group by VID and check for PID collisions
    let mut vid_pids: HashMap<u16, Vec<(u16, &str)>> = HashMap::new();
    for (vid, pid, label) in all_products {
        vid_pids.entry(*vid).or_default().push((*pid, label));
    }

    let mut collisions: Vec<String> = Vec::new();
    for (vid, entries) in &vid_pids {
        let mut seen: HashMap<u16, &str> = HashMap::new();
        for (pid, label) in entries {
            if let Some(existing) = seen.get(pid) {
                collisions.push(format!(
                    "VID 0x{vid:04X}: PID 0x{pid:04X} claimed by both '{existing}' and '{label}'"
                ));
            } else {
                seen.insert(*pid, label);
            }
        }
    }

    assert!(
        collisions.is_empty(),
        "PID collisions detected within vendor:\n  {}",
        collisions.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Telemetry adapter game IDs match config/registry (Superseded)
// ═════════════════════════════════════════════════════════════════════════════

// NOTE: The cross-crate consistency check between telemetry-adapters,
// telemetry-config-writers, and telemetry-support is now fully governed
// by the deterministic BDD metrics tests in `bdd_matrix_parity_tests.rs`.
//
// See `crates/integration-tests/tests/bdd_matrix_parity_tests.rs` for the
// exact coverage constraints.

// ═════════════════════════════════════════════════════════════════════════════
// 4. Device names: no empty strings or placeholder text
// ═════════════════════════════════════════════════════════════════════════════

/// Device display names must not be empty, whitespace-only, or contain
/// common placeholder text (TODO, FIXME, placeholder, TBD, unknown device).
#[test]
fn protocol_device_names_are_valid() -> TestResult {
    let placeholder_patterns: &[&str] = &["todo", "fixme", "placeholder", "tbd", "xxx", "n/a"];

    // Collect (name, source) pairs from all protocol crates that expose
    // display_name / product_name / name functions.
    let names: Vec<(&str, &str)> = vec![
        // Thrustmaster Model::name()
        (
            racing_wheel_hid_thrustmaster_protocol::Model::T150.name(),
            "Thrustmaster T150",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::Model::T300RS.name(),
            "Thrustmaster T300RS",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::Model::T818.name(),
            "Thrustmaster T818",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::Model::TSPCRacer.name(),
            "Thrustmaster TSPCRacer",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::Model::T80.name(),
            "Thrustmaster T80",
        ),
        // Simucube SimucubeModel::display_name()
        (
            hid_simucube_protocol::SimucubeModel::Simucube1.display_name(),
            "Simucube 1",
        ),
        (
            hid_simucube_protocol::SimucubeModel::Sport.display_name(),
            "Simucube Sport",
        ),
        (
            hid_simucube_protocol::SimucubeModel::Pro.display_name(),
            "Simucube Pro",
        ),
        (
            hid_simucube_protocol::SimucubeModel::Ultimate.display_name(),
            "Simucube Ultimate",
        ),
        // Asetek AsetekModel::display_name()
        (
            hid_asetek_protocol::AsetekModel::Forte.display_name(),
            "Asetek Forte",
        ),
        (
            hid_asetek_protocol::AsetekModel::Invicta.display_name(),
            "Asetek Invicta",
        ),
        (
            hid_asetek_protocol::AsetekModel::LaPrima.display_name(),
            "Asetek LaPrima",
        ),
        (
            hid_asetek_protocol::AsetekModel::TonyKanaan.display_name(),
            "Asetek TonyKanaan",
        ),
        // Heusinkveld HeusinkveldModel::display_name()
        (
            hid_heusinkveld_protocol::HeusinkveldModel::Sprint.display_name(),
            "Heusinkveld Sprint",
        ),
        (
            hid_heusinkveld_protocol::HeusinkveldModel::Ultimate.display_name(),
            "Heusinkveld Ultimate",
        ),
        (
            hid_heusinkveld_protocol::HeusinkveldModel::Pro.display_name(),
            "Heusinkveld Pro",
        ),
        // Cube Controls CubeControlsModel::display_name()
        (
            hid_cube_controls_protocol::CubeControlsModel::GtPro.display_name(),
            "CC GT Pro",
        ),
        (
            hid_cube_controls_protocol::CubeControlsModel::FormulaPro.display_name(),
            "CC Formula Pro",
        ),
        (
            hid_cube_controls_protocol::CubeControlsModel::Csx3.display_name(),
            "CC CSX3",
        ),
        // OpenFFBoard OpenFFBoardVariant::name()
        (
            racing_wheel_hid_openffboard_protocol::OpenFFBoardVariant::Main.name(),
            "OpenFFBoard Main",
        ),
        (
            racing_wheel_hid_openffboard_protocol::OpenFFBoardVariant::Alternate.name(),
            "OpenFFBoard Alt",
        ),
        // PXN product_name()
        (
            racing_wheel_hid_pxn_protocol::product_name(racing_wheel_hid_pxn_protocol::PRODUCT_V10)
                .unwrap_or(""),
            "PXN V10",
        ),
        (
            racing_wheel_hid_pxn_protocol::product_name(racing_wheel_hid_pxn_protocol::PRODUCT_V12)
                .unwrap_or(""),
            "PXN V12",
        ),
        // Cammus product_name()
        (
            racing_wheel_hid_cammus_protocol::product_name(
                racing_wheel_hid_cammus_protocol::PRODUCT_C5,
            )
            .unwrap_or(""),
            "Cammus C5",
        ),
        (
            racing_wheel_hid_cammus_protocol::product_name(
                racing_wheel_hid_cammus_protocol::PRODUCT_C12,
            )
            .unwrap_or(""),
            "Cammus C12",
        ),
    ];

    let mut issues: Vec<String> = Vec::new();
    for (name, source) in &names {
        if name.is_empty() || name.trim().is_empty() {
            issues.push(format!("{source}: device name is empty or whitespace-only"));
            continue;
        }
        let lower = name.to_lowercase();
        for pattern in placeholder_patterns {
            if lower.contains(pattern) {
                issues.push(format!(
                    "{source}: device name '{name}' contains placeholder text '{pattern}'"
                ));
            }
        }
    }

    assert!(
        issues.is_empty(),
        "Invalid device names detected:\n  {}",
        issues.join("\n  ")
    );

    Ok(())
}

/// VID constants themselves must be non-zero (0x0000 is not a valid USB VID).
#[test]
fn protocol_vendor_ids_are_non_zero() -> TestResult {
    let vids: &[(u16, &str)] = &[
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            "Logitech",
        ),
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            "Fanatec",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            "Thrustmaster",
        ),
        (
            racing_wheel_hid_simagic_protocol::SIMAGIC_VENDOR_ID,
            "Simagic",
        ),
        (racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID, "Moza"),
        (racing_wheel_hid_pxn_protocol::VENDOR_ID, "PXN"),
        (racing_wheel_hid_cammus_protocol::VENDOR_ID, "Cammus"),
        (hid_simucube_protocol::VENDOR_ID, "Simucube"),
        (racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID, "VRS"),
        (hid_asetek_protocol::VENDOR_ID, "Asetek"),
        (racing_wheel_hid_accuforce_protocol::VENDOR_ID, "AccuForce"),
        (
            racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID,
            "Leo Bodnar",
        ),
        (
            racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID,
            "FFBeast",
        ),
        (
            racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID,
            "OpenFFBoard",
        ),
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_VENDOR_ID,
            "Cube Controls",
        ),
        (
            hid_heusinkveld_protocol::HEUSINKVELD_VENDOR_ID,
            "Heusinkveld",
        ),
    ];

    let mut bad: Vec<&str> = Vec::new();
    for (vid, label) in vids {
        if *vid == 0 {
            bad.push(label);
        }
    }

    assert!(
        bad.is_empty(),
        "Vendor IDs must be non-zero. Invalid vendors: {:?}",
        bad
    );

    Ok(())
}

/// Cross-vendor PID collision guard: when multiple vendors share VID `0x0483`
/// (STMicroelectronics), their PIDs must not overlap.
#[test]
fn shared_stm_vid_pids_do_not_collide() -> TestResult {
    // VRS PIDs on shared STM VID
    let vrs_pids: &[(u16, &str)] = &[
        (racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID, "VRS DFP"),
        (
            racing_wheel_hid_vrs_protocol::product_ids::DIRECTFORCE_PRO_V2,
            "VRS DFP V2",
        ),
        (racing_wheel_hid_vrs_protocol::product_ids::R295, "VRS R295"),
        (
            racing_wheel_hid_vrs_protocol::product_ids::PEDALS,
            "VRS Pedals",
        ),
    ];

    // Cube Controls PIDs on shared STM VID
    let cc_pids: &[(u16, &str)] = &[
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID,
            "CC GT Pro",
        ),
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_FORMULA_PRO_PID,
            "CC Formula Pro",
        ),
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_CSX3_PID,
            "CC CSX3",
        ),
    ];

    // Simagic legacy PID on shared STM VID
    let simagic_pids: &[(u16, &str)] = &[(
        racing_wheel_hid_simagic_protocol::ids::SIMAGIC_LEGACY_PID,
        "Simagic Legacy",
    )];

    let mut all_pids: HashMap<u16, Vec<&str>> = HashMap::new();
    for group in [vrs_pids, cc_pids, simagic_pids] {
        for (pid, label) in group {
            all_pids.entry(*pid).or_default().push(label);
        }
    }

    let mut collisions: Vec<String> = Vec::new();
    for (pid, owners) in &all_pids {
        if owners.len() > 1 {
            collisions.push(format!("PID 0x{pid:04X} claimed by: {}", owners.join(", ")));
        }
    }

    assert!(
        collisions.is_empty(),
        "PID collisions on shared STM VID 0x0483:\n  {}",
        collisions.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. Authoritative PID cross-validation against kernel driver sources
// ═════════════════════════════════════════════════════════════════════════════

/// Cross-validate our VID/PID constants against hardcoded values extracted from
/// authoritative sources (Linux kernel drivers, community driver repos).
///
/// Each entry encodes: (crate_constant_value, expected_hex, source_description).
/// If any crate constant drifts from the kernel-sourced value, this test fails.
///
/// Sources:
/// - `gotzl/hid-fanatecff` `hid-ftec.h` (Fanatec)
/// - `JacKeTUs/simagic-ff` `hid-simagic.h` (Simagic)
/// - `JacKeTUs/simracing-hwdb` (Heusinkveld, Leo Bodnar, Asetek, Cammus, VRS, Simagic)
/// - `JacKeTUs/linux-steering-wheels` README (Asetek, Cammus, Moza, PXN, Simucube,
///   AccuForce, FFBeast, VRS, Logitech)
/// - Linux kernel `drivers/hid/hid-ids.h` (Logitech, Thrustmaster)
/// - `Kimplul/hid-tmff2` (Thrustmaster)
/// - `berarma/oversteer` `wheel_ids.py` (OpenFFBoard, Logitech)
#[test]
fn authoritative_pid_cross_validation() -> TestResult {
    let mut failures: Vec<String> = Vec::new();

    // Helper macro: checks constant == expected and records failures
    macro_rules! check_pid {
        ($constant:expr, $expected:expr, $label:expr, $source:expr) => {
            if $constant != $expected {
                failures.push(format!(
                    "{}: got 0x{:04X}, expected 0x{:04X} (source: {})",
                    $label, $constant, $expected, $source
                ));
            }
        };
    }

    // ── Fanatec (source: gotzl/hid-fanatecff hid-ftec.h) ──────────────────
    check_pid!(
        racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
        0x0EB7_u16,
        "Fanatec VID",
        "hid-ftec.h: FANATEC_VENDOR_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2,
        0x0001_u16,
        "Fanatec CS V2",
        "hid-ftec.h: CLUBSPORT_V2_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2_5,
        0x0004_u16,
        "Fanatec CS V2.5",
        "hid-ftec.h: CLUBSPORT_V25_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE_PS4,
        0x0005_u16,
        "Fanatec CSL Elite PS4",
        "hid-ftec.h: CSL_ELITE_PS4_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::DD1,
        0x0006_u16,
        "Fanatec DD1",
        "hid-ftec.h: PODIUM_WHEELBASE_DD1_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::DD2,
        0x0007_u16,
        "Fanatec DD2",
        "hid-ftec.h: PODIUM_WHEELBASE_DD2_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSR_ELITE,
        0x0011_u16,
        "Fanatec CSR Elite",
        "hid-ftec.h: CSR_ELITE_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
        0x0020_u16,
        "Fanatec CSL DD",
        "hid-ftec.h: CSL_DD_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE,
        0x0E03_u16,
        "Fanatec CSL Elite",
        "hid-ftec.h: CSL_ELITE_WHEELBASE_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_PEDALS_V3,
        0x183B_u16,
        "Fanatec CS Pedals V3",
        "hid-ftec.h: CLUBSPORT_PEDALS_V3_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE_PEDALS,
        0x6204_u16,
        "Fanatec CSL Elite Pedals",
        "hid-ftec.h: CSL_ELITE_PEDALS_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_PEDALS_LC,
        0x6205_u16,
        "Fanatec CSL Pedals LC",
        "hid-ftec.h: CSL_LC_PEDALS_DEVICE_ID"
    );
    check_pid!(
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_PEDALS_V2,
        0x6206_u16,
        "Fanatec CSL Pedals V2",
        "hid-ftec.h: CSL_LC_V2_PEDALS_DEVICE_ID"
    );

    // ── Simagic (source: JacKeTUs/simagic-ff hid-simagic.h) ───────────────
    check_pid!(
        racing_wheel_hid_simagic_protocol::SIMAGIC_VENDOR_ID,
        0x3670_u16,
        "Simagic VID",
        "simagic-ff README: VID 0x3670"
    );
    check_pid!(
        racing_wheel_hid_simagic_protocol::ids::SIMAGIC_LEGACY_PID,
        0x0522_u16,
        "Simagic Alpha/M10",
        "hid-simagic.h: SIMAGIC_ALPHA 0x0522"
    );
    check_pid!(
        racing_wheel_hid_simagic_protocol::product_ids::EVO_SPORT,
        0x0500_u16,
        "Simagic EVO Sport",
        "hid-simagic.h: SIMAGIC_EVO 0x0500"
    );
    check_pid!(
        racing_wheel_hid_simagic_protocol::product_ids::EVO,
        0x0501_u16,
        "Simagic EVO",
        "hid-simagic.h: SIMAGIC_EVO_1 0x0501"
    );
    check_pid!(
        racing_wheel_hid_simagic_protocol::product_ids::EVO_PRO,
        0x0502_u16,
        "Simagic EVO Pro",
        "hid-simagic.h: SIMAGIC_EVO_2 0x0502"
    );

    // ── Leo Bodnar (source: JacKeTUs/simracing-hwdb 90-leo-bodnar.hwdb) ───
    check_pid!(
        racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID,
        0x1DD2_u16,
        "Leo Bodnar VID",
        "simracing-hwdb: Leo Bodnar VID"
    );
    check_pid!(
        racing_wheel_hid_leo_bodnar_protocol::ids::PID_PEDALS,
        0x100C_u16,
        "Leo Bodnar Pedals",
        "simracing-hwdb 90-leo-bodnar.hwdb"
    );
    check_pid!(
        racing_wheel_hid_leo_bodnar_protocol::ids::PID_LC_PEDALS,
        0x22D0_u16,
        "Leo Bodnar LC Pedals",
        "simracing-hwdb 90-leo-bodnar.hwdb"
    );

    // ── Moza (source: linux-steering-wheels compatibility table) ───────────
    check_pid!(
        racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID,
        0x346E_u16,
        "Moza VID",
        "linux-steering-wheels: VID 346E"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R9_V1,
        0x0002_u16,
        "Moza R9",
        "linux-steering-wheels: R9 PID 0002"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R5_V1,
        0x0004_u16,
        "Moza R5",
        "linux-steering-wheels: R5 PID 0004"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R3_V1,
        0x0005_u16,
        "Moza R3",
        "linux-steering-wheels: R3 PID 0005"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R12_V1,
        0x0006_u16,
        "Moza R12",
        "linux-steering-wheels: R12 PID 0006"
    );

    // ── OpenFFBoard (source: berarma/oversteer wheel_ids.py) ──────────────
    check_pid!(
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID,
        0x1209_u16,
        "OpenFFBoard VID",
        "oversteer wheel_ids.py + pid.codes"
    );
    check_pid!(
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
        0xFFB0_u16,
        "OpenFFBoard PID",
        "oversteer wheel_ids.py: 0xFFB0"
    );

    // ── Logitech (source: Linux kernel drivers/hid/hid-ids.h) ─────────────
    check_pid!(
        racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
        0x046D_u16,
        "Logitech VID",
        "kernel hid-ids.h: USB_VENDOR_ID_LOGITECH"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G27,
        0xC29B_u16,
        "Logitech G27",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G27_WHEEL"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
        0xC24F_u16,
        "Logitech G29",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G29_WHEEL"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G920,
        0xC262_u16,
        "Logitech G920",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G920_WHEEL"
    );

    // ── Thrustmaster (source: Kimplul/hid-tmff2 + kernel hid-ids.h) ──────
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
        0x044F_u16,
        "Thrustmaster VID",
        "kernel hid-ids.h: USB_VENDOR_ID_THRUSTMASTER"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
        0xB66E_u16,
        "Thrustmaster T300 RS",
        "hid-tmff2.h: TMT300RS_PS3_NORM_ID"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_GT,
        0xB66F_u16,
        "Thrustmaster T300 RS GT",
        "hid-tmff2.h: TMT300RS_PS3_ADV_ID"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_PS4,
        0xB66D_u16,
        "Thrustmaster T300 RS PS4",
        "hid-tmff2.h: TMT300RS_PS4_NORM_ID"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::T248,
        0xB696_u16,
        "Thrustmaster T248",
        "hid-tmff2.h: TMT248_PC_ID"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::TX_RACING,
        0xB669_u16,
        "Thrustmaster TX",
        "hid-tmff2.h: TX_ACTIVE"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::TS_XW,
        0xB692_u16,
        "Thrustmaster TS-XW",
        "hid-tmff2.h: TSXW_ACTIVE"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
        0xB689_u16,
        "Thrustmaster TS-PC Racer",
        "hid-tmff2.h: TMTS_PC_RACER_ID"
    );
    check_pid!(
        racing_wheel_hid_thrustmaster_protocol::product_ids::TMX,
        0xB67F_u16,
        "Thrustmaster TMX",
        "linux-steering-wheels: TMX PID b67f"
    );

    // ── Heusinkveld (source: JacKeTUs/simracing-hwdb 90-heusinkveld.hwdb) ─
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_VENDOR_ID,
        0x30B7_u16,
        "Heusinkveld VID",
        "simracing-hwdb 90-heusinkveld.hwdb"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_SPRINT_PID,
        0x1001_u16,
        "Heusinkveld Sprint",
        "simracing-hwdb 90-heusinkveld.hwdb: v30B7p1001"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_HANDBRAKE_V2_PID,
        0x1002_u16,
        "Heusinkveld Handbrake V2",
        "simracing-hwdb 90-heusinkveld.hwdb: v30B7p1002"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_ULTIMATE_PID,
        0x1003_u16,
        "Heusinkveld Ultimate",
        "simracing-hwdb 90-heusinkveld.hwdb: v30B7p1003"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        0x10C4_u16,
        "Heusinkveld Handbrake V1 VID (Silicon Labs)",
        "simracing-hwdb 90-heusinkveld.hwdb: v10C4p8B82"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_HANDBRAKE_V1_PID,
        0x8B82_u16,
        "Heusinkveld Handbrake V1",
        "simracing-hwdb 90-heusinkveld.hwdb: v10C4p8B82"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_SHIFTER_VENDOR_ID,
        0xA020_u16,
        "Heusinkveld Shifter VID",
        "simracing-hwdb 90-heusinkveld.hwdb: vA020p3142"
    );
    check_pid!(
        hid_heusinkveld_protocol::HEUSINKVELD_SHIFTER_PID,
        0x3142_u16,
        "Heusinkveld Shifter",
        "simracing-hwdb 90-heusinkveld.hwdb: vA020p3142"
    );

    // ── Asetek (source: linux-steering-wheels + simracing-hwdb) ───────────
    check_pid!(
        hid_asetek_protocol::ASETEK_VENDOR_ID,
        0x2433_u16,
        "Asetek VID",
        "linux-steering-wheels + kernel: VID 2433"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_INVICTA_PID,
        0xF300_u16,
        "Asetek Invicta",
        "linux-steering-wheels: Invicta PID f300"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_FORTE_PID,
        0xF301_u16,
        "Asetek Forte",
        "linux-steering-wheels: Forte PID f301"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_LAPRIMA_PID,
        0xF303_u16,
        "Asetek La Prima",
        "linux-steering-wheels: La Prima PID f303"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_TONY_KANAAN_PID,
        0xF306_u16,
        "Asetek Tony Kanaan",
        "linux-steering-wheels: Tony Kanaan PID f306"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_INVICTA_PEDALS_PID,
        0xF100_u16,
        "Asetek Invicta Pedals",
        "simracing-hwdb 90-asetek.hwdb: v2433pF100"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_FORTE_PEDALS_PID,
        0xF101_u16,
        "Asetek Forte Pedals",
        "simracing-hwdb 90-asetek.hwdb: v2433pF101"
    );

    // ── Cammus (source: linux-steering-wheels + simracing-hwdb) ───────────
    check_pid!(
        racing_wheel_hid_cammus_protocol::VENDOR_ID,
        0x3416_u16,
        "Cammus VID",
        "linux-steering-wheels + kernel >=6.15: VID 3416"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_C5,
        0x0301_u16,
        "Cammus C5",
        "linux-steering-wheels: C5 PID 0301"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_C12,
        0x0302_u16,
        "Cammus C12",
        "linux-steering-wheels: C12 PID 0302"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_CP5_PEDALS,
        0x1018_u16,
        "Cammus CP5 Pedals",
        "simracing-hwdb 90-cammus.hwdb: v3416p1018"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_LC100_PEDALS,
        0x1019_u16,
        "Cammus LC100 Pedals",
        "simracing-hwdb 90-cammus.hwdb: v3416p1019"
    );

    // ── VRS (source: linux-steering-wheels + simracing-hwdb) ──────────────
    check_pid!(
        racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID,
        0x0483_u16,
        "VRS VID (STM shared)",
        "linux-steering-wheels: VRS VID 0483"
    );
    check_pid!(
        racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID,
        0xA355_u16,
        "VRS DirectForce Pro",
        "linux-steering-wheels + simracing-hwdb: PID a355"
    );
    check_pid!(
        racing_wheel_hid_vrs_protocol::product_ids::PEDALS,
        0xA3BE_u16,
        "VRS DirectForce Pro Pedals",
        "simracing-hwdb 90-vrs.hwdb: v0483pA3BE"
    );

    // ── Simagic additional (source: simracing-hwdb 90-simagic.hwdb) ──────
    check_pid!(
        racing_wheel_hid_simagic_protocol::product_ids::HANDBRAKE,
        0x0A04_u16,
        "Simagic TB-RS Handbrake",
        "simracing-hwdb 90-simagic.hwdb: v3670p0A04"
    );

    // ── Simucube (source: linux-steering-wheels compatibility table) ──────
    check_pid!(
        hid_simucube_protocol::VENDOR_ID,
        0x16D0_u16,
        "Simucube VID",
        "linux-steering-wheels: VID 16d0"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_1_PID,
        0x0D5A_u16,
        "Simucube 1",
        "linux-steering-wheels: Simucube 1 PID 0d5a"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
        0x0D61_u16,
        "Simucube 2 Sport",
        "linux-steering-wheels: SC2 Sport PID 0d61"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
        0x0D60_u16,
        "Simucube 2 Pro",
        "linux-steering-wheels: SC2 Pro PID 0d60"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
        0x0D5F_u16,
        "Simucube 2 Ultimate",
        "linux-steering-wheels: SC2 Ultimate PID 0d5f"
    );

    // ── AccuForce (source: linux-steering-wheels compatibility table) ─────
    check_pid!(
        racing_wheel_hid_accuforce_protocol::VENDOR_ID,
        0x1FC9_u16,
        "AccuForce VID",
        "linux-steering-wheels: AccuForce VID 1fc9"
    );
    check_pid!(
        racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO,
        0x804C_u16,
        "AccuForce Pro",
        "linux-steering-wheels: AccuForce Pro PID 804c"
    );

    // ── FFBeast (source: linux-steering-wheels compatibility table) ───────
    check_pid!(
        racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID,
        0x045B_u16,
        "FFBeast VID",
        "linux-steering-wheels: FFBeast VID 045b"
    );
    check_pid!(
        racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
        0x59D7_u16,
        "FFBeast Wheel",
        "linux-steering-wheels: FFBeast Wheel PID 59d7"
    );

    // ── PXN (source: linux-steering-wheels compatibility table) ───────────
    check_pid!(
        racing_wheel_hid_pxn_protocol::VENDOR_ID,
        0x11FF_u16,
        "PXN VID",
        "linux-steering-wheels: PXN/Lite Star VID 11ff"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V10,
        0x3245_u16,
        "PXN V10",
        "linux-steering-wheels: PXN V10 PID 3245"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V12,
        0x1212_u16,
        "PXN V12",
        "linux-steering-wheels: PXN V12 PID 1212"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE,
        0x1112_u16,
        "PXN V12 Lite",
        "linux-steering-wheels: PXN V12 Lite PID 1112"
    );

    // ── Logitech additional (source: linux-steering-wheels) ──────────────
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::MOMO,
        0xC295_u16,
        "Logitech MOMO",
        "linux-steering-wheels: MOMO PID c295"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_PRO,
        0xC298_u16,
        "Logitech DFP",
        "linux-steering-wheels: DFP PID c298"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_GT,
        0xC29A_u16,
        "Logitech DFGT",
        "linux-steering-wheels: DFGT PID c29a"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G25,
        0xC299_u16,
        "Logitech G25",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G25_WHEEL"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G923_PS,
        0xC267_u16,
        "Logitech G923 PS",
        "linux-steering-wheels: G923 PS PID c267"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX_ALT,
        0xC26D_u16,
        "Logitech G923 Xbox (alt)",
        "linux-steering-wheels: G923 Xbox PID c26d"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G_PRO_XBOX,
        0xC272_u16,
        "Logitech G Pro Xbox",
        "linux-steering-wheels: G Pro PID c272 (uses hid-logitech-hidpp)"
    );

    // ── Simucube (source: linux-steering-wheels compat table) ────────────
    check_pid!(
        hid_simucube_protocol::VENDOR_ID,
        0x16D0_u16,
        "Simucube VID",
        "linux-steering-wheels: VID 16D0"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_1_PID,
        0x0D5A_u16,
        "Simucube 1",
        "linux-steering-wheels: PID 0d5a"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
        0x0D61_u16,
        "Simucube 2 Sport",
        "linux-steering-wheels: PID 0d61"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
        0x0D60_u16,
        "Simucube 2 Pro",
        "linux-steering-wheels: PID 0d60"
    );
    check_pid!(
        hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
        0x0D5F_u16,
        "Simucube 2 Ultimate",
        "linux-steering-wheels: PID 0d5f"
    );

    // ── AccuForce (source: linux-steering-wheels compat table) ────────────
    check_pid!(
        racing_wheel_hid_accuforce_protocol::VENDOR_ID,
        0x1FC9_u16,
        "AccuForce VID",
        "linux-steering-wheels: VID 1fc9"
    );
    check_pid!(
        racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO,
        0x804C_u16,
        "AccuForce Pro",
        "linux-steering-wheels: PID 804c"
    );

    // ── FFBeast (source: linux-steering-wheels compat table) ──────────────
    check_pid!(
        racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID,
        0x045B_u16,
        "FFBeast VID",
        "linux-steering-wheels: VID 045b"
    );
    check_pid!(
        racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
        0x59D7_u16,
        "FFBeast Wheel",
        "linux-steering-wheels: PID 59d7"
    );

    // ── Asetek (source: linux-steering-wheels + simracing-hwdb) ───────────
    check_pid!(
        hid_asetek_protocol::VENDOR_ID,
        0x2433_u16,
        "Asetek VID",
        "linux-steering-wheels: VID 2433"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_INVICTA_PID,
        0xF300_u16,
        "Asetek Invicta",
        "linux-steering-wheels: PID f300"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_FORTE_PID,
        0xF301_u16,
        "Asetek Forte",
        "linux-steering-wheels: PID f301"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_LAPRIMA_PID,
        0xF303_u16,
        "Asetek La Prima",
        "linux-steering-wheels: PID f303"
    );
    check_pid!(
        hid_asetek_protocol::ASETEK_TONY_KANAAN_PID,
        0xF306_u16,
        "Asetek Tony Kanaan",
        "linux-steering-wheels: PID f306"
    );

    // ── Cammus (source: simracing-hwdb 90-cammus.hwdb) ───────────────────
    check_pid!(
        racing_wheel_hid_cammus_protocol::VENDOR_ID,
        0x3416_u16,
        "Cammus VID",
        "simracing-hwdb: VID 3416"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_C5,
        0x0301_u16,
        "Cammus C5",
        "simracing-hwdb + linux-steering-wheels: PID 0301"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_C12,
        0x0302_u16,
        "Cammus C12",
        "simracing-hwdb + linux-steering-wheels: PID 0302"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_CP5_PEDALS,
        0x1018_u16,
        "Cammus CP5 Pedals",
        "simracing-hwdb: PID 1018"
    );
    check_pid!(
        racing_wheel_hid_cammus_protocol::PRODUCT_LC100_PEDALS,
        0x1019_u16,
        "Cammus LC100 Pedals",
        "simracing-hwdb: PID 1019"
    );

    // ── PXN (source: Linux kernel hid-ids.h) ─────────────────────────────
    check_pid!(
        racing_wheel_hid_pxn_protocol::VENDOR_ID,
        0x11FF_u16,
        "PXN VID",
        "kernel hid-ids.h: USB_VENDOR_ID_LITE_STAR 0x11ff"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V10,
        0x3245_u16,
        "PXN V10",
        "kernel hid-ids.h: USB_DEVICE_ID_PXN_V10 0x3245"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V12,
        0x1212_u16,
        "PXN V12",
        "kernel hid-ids.h: USB_DEVICE_ID_PXN_V12 0x1212"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE,
        0x1112_u16,
        "PXN V12 Lite",
        "kernel hid-ids.h: USB_DEVICE_ID_PXN_V12_LITE 0x1112"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE_2,
        0x1211_u16,
        "PXN V12 Lite 2",
        "kernel hid-ids.h: USB_DEVICE_ID_PXN_V12_LITE_2 0x1211"
    );
    check_pid!(
        racing_wheel_hid_pxn_protocol::PRODUCT_GT987,
        0x2141_u16,
        "Lite Star GT987",
        "kernel hid-ids.h: USB_DEVICE_ID_LITE_STAR_GT987 0x2141"
    );

    // ── Logitech (source: Linux kernel hid-ids.h + hid-lg4ff.c) ──────────
    check_pid!(
        racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
        0x046D_u16,
        "Logitech VID",
        "kernel hid-ids.h: USB_VENDOR_ID_LOGITECH 0x046d"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
        0xC24F_u16,
        "Logitech G29 (PS)",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G29_WHEEL 0xc24f"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G920,
        0xC262_u16,
        "Logitech G920",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G920_WHEEL 0xc262"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX,
        0xC26E_u16,
        "Logitech G923 (Xbox)",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL 0xc26e"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_EX,
        0xC294_u16,
        "Logitech Driving Force / Formula EX",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_WHEEL 0xc294"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::MOMO,
        0xC295_u16,
        "Logitech MOMO",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_MOMO_WHEEL 0xc295"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_PRO,
        0xC298_u16,
        "Logitech Driving Force Pro",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_DFP_WHEEL 0xc298"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G25,
        0xC299_u16,
        "Logitech G25",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G25_WHEEL 0xc299"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_GT,
        0xC29A_u16,
        "Logitech Driving Force GT",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_DFGT_WHEEL 0xc29a"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G27,
        0xC29B_u16,
        "Logitech G27",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_G27_WHEEL 0xc29b"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::SPEED_FORCE_WIRELESS,
        0xC29C_u16,
        "Logitech Speed Force Wireless",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_WII_WHEEL 0xc29c"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::MOMO_2,
        0xCA03_u16,
        "Logitech MOMO Racing",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_MOMO_WHEEL2 0xca03"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G923,
        0xC266_u16,
        "Logitech G923 (PS/Trueforce)",
        "oversteer: LG_G923P 046d:c266"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G923_PS,
        0xC267_u16,
        "Logitech G923 (PS alt)",
        "linux-steering-wheels: PID c267"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
        0xC268_u16,
        "Logitech G Pro (PS)",
        "oversteer: LG_GPRO_PS 046d:c268"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::G_PRO_XBOX,
        0xC272_u16,
        "Logitech G Pro (Xbox)",
        "oversteer: LG_GPRO_XBOX 046d:c272"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE,
        0xC291_u16,
        "Logitech Wingman Formula Force",
        "oversteer: LG_WFF 046d:c291"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE_GP,
        0xC293_u16,
        "Logitech Wingman FFG",
        "oversteer: LG_WFFG 046d:c293 + kernel hid-ids.h: LOGITECH_WINGMAN_FFG"
    );
    check_pid!(
        racing_wheel_hid_logitech_protocol::product_ids::VIBRATION_WHEEL,
        0xCA04_u16,
        "Logitech Vibration Wheel",
        "kernel hid-ids.h: USB_DEVICE_ID_LOGITECH_VIBRATION_WHEEL 0xca04"
    );

    // ── Moza (source: linux-steering-wheels compat table) ─────────────────
    check_pid!(
        racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID,
        0x346E_u16,
        "Moza VID",
        "linux-steering-wheels: VID 346e"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R16_R21_V1,
        0x0000_u16,
        "Moza R16/R21 V1",
        "linux-steering-wheels: PID 0000"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R9_V1,
        0x0002_u16,
        "Moza R9 V1",
        "linux-steering-wheels: PID 0002"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R5_V1,
        0x0004_u16,
        "Moza R5 V1",
        "linux-steering-wheels: PID 0004"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R3_V1,
        0x0005_u16,
        "Moza R3 V1",
        "linux-steering-wheels: PID 0005"
    );
    check_pid!(
        racing_wheel_hid_moza_protocol::product_ids::R12_V1,
        0x0006_u16,
        "Moza R12 V1",
        "linux-steering-wheels: PID 0006"
    );

    // ── VRS (source: linux-steering-wheels compat table) ──────────────────
    check_pid!(
        racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID,
        0x0483_u16,
        "VRS VID (shared STM VID)",
        "linux-steering-wheels: VID 0483"
    );
    check_pid!(
        racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID,
        0xA355_u16,
        "VRS DirectForce Pro",
        "linux-steering-wheels: PID a355"
    );

    // ── OpenFFBoard (source: berarma/oversteer wheel_ids.py) ──────────────
    check_pid!(
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID,
        0x1209_u16,
        "OpenFFBoard VID",
        "oversteer wheel_ids.py + pid.codes registry"
    );
    check_pid!(
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
        0xFFB0_u16,
        "OpenFFBoard Main",
        "oversteer wheel_ids.py: Open FFBoard PID 0xFFB0"
    );

    assert!(
        failures.is_empty(),
        "PID cross-validation failures against authoritative sources:\n  {}",
        failures.join("\n  ")
    );

    Ok(())
}

/// Telemetry adapter game IDs must be unique (no two adapters claim the
/// same game ID).
#[test]
fn telemetry_adapter_game_ids_are_unique() -> TestResult {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut duplicates: Vec<&str> = Vec::new();

    for (game_id, _) in adapter_factories() {
        if !seen.insert(game_id) {
            duplicates.push(game_id);
        }
    }

    assert!(
        duplicates.is_empty(),
        "Duplicate telemetry adapter game IDs: {:?}",
        duplicates
    );

    Ok(())
}
