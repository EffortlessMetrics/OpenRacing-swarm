//! Packaging validation tests — verifies binary names, service files, installer
//! metadata, udev rules, packaging directory structure, and cross-references
//! VID/PID constants from HID protocol crates.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Parsed udev rules: (VID/PID pairs, vendor-wide VIDs).
type UdevRules = (HashSet<(String, String)>, HashSet<String>);

/// Return the repository root (two levels up from the integration-tests crate).
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| manifest.clone())
}

/// Read a file relative to the repo root, returning its contents.
fn read_packaging_file(rel_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = repo_root().join(rel_path);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(content)
}

/// Parse all VID/PID pairs from the udev rules file (hidraw section only).
/// Returns `(vid_pid_pairs, vendor_wide_vids)`.
fn parse_udev_rules(content: &str) -> Result<UdevRules, Box<dyn std::error::Error>> {
    let vid_pid_re = regex::Regex::new(
        r#"ATTRS\{idVendor\}=="([0-9a-fA-F]{4})".*?ATTRS\{idProduct\}=="([0-9a-fA-F]{4})""#,
    )?;
    let hidraw_vid_re =
        regex::Regex::new(r#"SUBSYSTEM=="hidraw".*ATTRS\{idVendor\}=="([0-9a-fA-F]{4})""#)?;

    let mut pairs = HashSet::new();
    let mut vendor_wide = HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(caps) = vid_pid_re.captures(line) {
            let vid = caps.get(1).map(|m| m.as_str().to_lowercase());
            let pid = caps.get(2).map(|m| m.as_str().to_lowercase());
            if let (Some(v), Some(p)) = (vid, pid) {
                pairs.insert((v, p));
            }
        } else if let Some(caps) = hidraw_vid_re.captures(line)
            && let Some(vid) = caps.get(1).map(|m| m.as_str().to_lowercase())
        {
            // hidraw line with VID but no PID match → vendor-wide rule
            vendor_wide.insert(vid);
        }
    }
    Ok((pairs, vendor_wide))
}

/// Known VID/PID pairs that MUST appear in udev rules (from HID protocol crates).
/// Each entry: (vid, pid, description).
fn known_vid_pids() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        // Logitech (0x046D)
        ("046d", "c24f", "Logitech G29 PS"),
        ("046d", "c29b", "Logitech G27"),
        ("046d", "c298", "Logitech Driving Force Pro"),
        ("046d", "c299", "Logitech G25"),
        ("046d", "c29a", "Logitech Driving Force GT"),
        ("046d", "c262", "Logitech G920"),
        ("046d", "c266", "Logitech G923"),
        ("046d", "c267", "Logitech G923 PS"),
        ("046d", "c26e", "Logitech G923 Xbox"),
        ("046d", "c26d", "Logitech G923 Xbox Alt"),
        ("046d", "c268", "Logitech G PRO"),
        ("046d", "c272", "Logitech G PRO Xbox"),
        ("046d", "c295", "Logitech MOMO"),
        ("046d", "c294", "Logitech Driving Force EX"),
        ("046d", "c29c", "Logitech Speed Force Wireless"),
        ("046d", "ca03", "Logitech MOMO 2"),
        ("046d", "c293", "Logitech WingMan Formula Force GP"),
        ("046d", "c291", "Logitech WingMan Formula Force"),
        ("046d", "ca04", "Logitech Vibration Wheel"),
        // Thrustmaster (0x044F)
        ("044f", "b65d", "Thrustmaster FFB Wheel Generic"),
        ("044f", "b677", "Thrustmaster T150"),
        ("044f", "b65e", "Thrustmaster T500 RS"),
        ("044f", "b66d", "Thrustmaster T300 RS PS4"),
        ("044f", "b66e", "Thrustmaster T300 RS"),
        ("044f", "b66f", "Thrustmaster T300 RS GT"),
        ("044f", "b669", "Thrustmaster TX Racing"),
        ("044f", "b67f", "Thrustmaster TMX"),
        ("044f", "b696", "Thrustmaster T248"),
        ("044f", "b69a", "Thrustmaster T248X"),
        ("044f", "b689", "Thrustmaster TS-PC Racer"),
        ("044f", "b692", "Thrustmaster TS-XW"),
        ("044f", "b691", "Thrustmaster TS-XW GIP"),
        ("044f", "b681", "Thrustmaster T-GT II GT"),
        ("044f", "b69b", "Thrustmaster T818"),
        ("044f", "b664", "Thrustmaster TX Racing Orig"),
        ("044f", "b668", "Thrustmaster T80"),
        ("044f", "b371", "Thrustmaster T-LCM"),
        ("044f", "b68f", "Thrustmaster TPR Pedals"),
        // Fanatec — vendor-wide rule
        // Moza — vendor-wide rule
        // Simagic legacy (0x0483)
        ("0483", "0522", "Simagic Alpha/M10 legacy"),
        // VRS (0x0483)
        ("0483", "a355", "VRS DirectForce Pro"),
        ("0483", "a356", "VRS DirectForce Pro V2"),
        ("0483", "a357", "VRS Pedals V1"),
        ("0483", "a358", "VRS Pedals V2"),
        ("0483", "a359", "VRS Handbrake"),
        ("0483", "a35a", "VRS Shifter"),
        ("0483", "a3be", "VRS Pedals"),
        ("0483", "a44c", "VRS R295"),
        // Cube Controls (0x0483, provisional)
        ("0483", "0c73", "Cube Controls GT Pro"),
        ("0483", "0c74", "Cube Controls Formula Pro"),
        ("0483", "0c75", "Cube Controls CSX3"),
        // Simucube (0x16D0)
        ("16d0", "0d5a", "Simucube 1"),
        ("16d0", "0d5f", "Simucube 2 Ultimate"),
        ("16d0", "0d60", "Simucube 2 Pro"),
        ("16d0", "0d61", "Simucube 2 Sport"),
        ("16d0", "0d66", "Simucube ActivePedal"),
        ("16d0", "0d63", "Simucube Wireless Wheel"),
        // Heusinkveld (0x30B7)
        ("30b7", "1001", "Heusinkveld Sprint"),
        ("30b7", "1002", "Heusinkveld Handbrake V2"),
        ("30b7", "1003", "Heusinkveld Ultimate"),
        // Heusinkveld legacy (0x04D8)
        ("04d8", "f6d0", "Heusinkveld Legacy Sprint"),
        ("04d8", "f6d2", "Heusinkveld Legacy Ultimate"),
        ("04d8", "f6d3", "Heusinkveld Pro"),
        // Heusinkveld Handbrake V1 (0x10C4)
        ("10c4", "8b82", "Heusinkveld Handbrake V1"),
        // Heusinkveld Shifter (0xA020)
        ("a020", "3142", "Heusinkveld Sequential Shifter"),
        // Cammus (0x3416)
        ("3416", "0301", "Cammus C5"),
        ("3416", "0302", "Cammus C12"),
        ("3416", "1018", "Cammus CP5 Pedals"),
        ("3416", "1019", "Cammus LC100 Pedals"),
        // Asetek (0x2433)
        ("2433", "f300", "Asetek Invicta"),
        ("2433", "f301", "Asetek Forte"),
        ("2433", "f303", "Asetek La Prima"),
        ("2433", "f306", "Asetek Tony Kanaan"),
        ("2433", "f100", "Asetek Invicta Pedals"),
        ("2433", "f101", "Asetek Forte Pedals"),
        ("2433", "f102", "Asetek La Prima Pedals"),
        // OpenFFBoard (0x1209)
        ("1209", "ffb0", "OpenFFBoard"),
        ("1209", "ffb1", "OpenFFBoard Alt"),
        ("1209", "1bbd", "Generic Button Box"),
        // FFBeast (0x045B)
        ("045b", "58f9", "FFBeast Joystick"),
        ("045b", "5968", "FFBeast Rudder"),
        ("045b", "59d7", "FFBeast Wheel"),
        // Granite Devices (0x1D50)
        ("1d50", "6050", "IONI Simucube 1"),
        ("1d50", "6051", "IONI Premium Simucube 2"),
        ("1d50", "6052", "ARGON Simucube Sport"),
        // Leo Bodnar (0x1DD2)
        ("1dd2", "000e", "Leo Bodnar Wheel Interface"),
        ("1dd2", "000c", "Leo Bodnar BBI-32"),
        ("1dd2", "0001", "Leo Bodnar USB Joystick"),
        ("1dd2", "000f", "Leo Bodnar FFB Joystick"),
        ("1dd2", "1301", "Leo Bodnar SLI-Pro"),
        ("1dd2", "100c", "Leo Bodnar Pedals"),
        ("1dd2", "22d0", "Leo Bodnar LC Pedals"),
        // PXN (0x11FF)
        ("11ff", "3245", "PXN V10"),
        ("11ff", "1212", "PXN V12"),
        ("11ff", "1112", "PXN V12 Lite"),
        ("11ff", "1211", "PXN V12 Lite SE"),
        ("11ff", "2141", "PXN GT987"),
        // AccuForce (0x1FC9)
        ("1fc9", "804c", "AccuForce Pro"),
        // Oddor (0x1021)
        ("1021", "1888", "Oddor Handbrake"),
        // MMOS (0xF055)
        ("f055", "0ffb", "MMOS FFB Controller"),
        // SHH (0x16C0, V-USB shared VID)
        ("16c0", "05e1", "SHH Shifter"),
        // SimGrade (0x1209, pid.codes shared VID)
        ("1209", "3115", "SimGrade VX-Pro Pedals"),
        // SimJack (0x2497)
        ("2497", "5757", "SimJack PRO Pedals"),
        // SimLab (0x04D8, Microchip shared VID)
        ("04d8", "e760", "SimLab Handbrake XB1"),
        // SimNet (0xCAFE)
        ("cafe", "a301", "SimNet SP Pedals"),
        // SimRuito (0x5487)
        ("5487", "5401", "SimRuito Pedals"),
        // SimSonn (0xDDFD)
        ("ddfd", "5008", "SimSonn Pedals"),
        ("ddfd", "6011", "SimSonn Pedals Plus X"),
        // SimTrecs (0x03EB, Atmel shared VID)
        ("03eb", "2406", "SimTrecs ProPedal GT"),
    ]
}

/// VIDs covered by vendor-wide udev rules (no PID filter).
fn vendor_wide_vids() -> Vec<&'static str> {
    vec![
        "0eb7", // Fanatec
        "346e", // Moza
        "3670", // Simagic EVO
    ]
}

// ============================================================================
// Binary name and Cargo.toml validation
// ============================================================================

#[test]
fn binary_name_wheeld_defined_in_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let service_cargo = read_packaging_file("crates/service/Cargo.toml")?;
    assert!(
        service_cargo.contains("wheeld"),
        "wheeld binary not defined in crates/service/Cargo.toml"
    );
    Ok(())
}

#[test]
fn binary_name_wheelctl_defined_in_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let cli_cargo = read_packaging_file("crates/cli/Cargo.toml")?;
    assert!(
        cli_cargo.contains("wheelctl"),
        "wheelctl binary not defined in crates/cli/Cargo.toml"
    );
    Ok(())
}

#[test]
fn workspace_version_is_semver_compliant() -> Result<(), Box<dyn std::error::Error>> {
    let cargo_toml = read_packaging_file("Cargo.toml")?;
    let version_re = regex::Regex::new(r#"version\s*=\s*"(\d+\.\d+\.\d+[^"]*)""#)?;
    let caps = version_re
        .captures(&cargo_toml)
        .ok_or("No version found in root Cargo.toml")?;
    let version_str = caps.get(1).ok_or("No version capture group")?.as_str();
    let _parsed: semver::Version = version_str
        .parse()
        .map_err(|e| format!("Workspace version '{version_str}' is not valid semver: {e}"))?;
    Ok(())
}

#[test]
fn version_consistent_across_workspace_crates() -> Result<(), Box<dyn std::error::Error>> {
    let root_toml = read_packaging_file("Cargo.toml")?;
    let ver_re = regex::Regex::new(r#"version\s*=\s*"(\d+\.\d+\.\d+)""#)?;
    let root_version = ver_re
        .captures(&root_toml)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or("No version in root Cargo.toml")?;

    // Crates that use workspace version inherit it; check key crates that
    // explicitly set version match or use `version.workspace = true`.
    let crate_tomls = [
        "crates/service/Cargo.toml",
        "crates/cli/Cargo.toml",
        "crates/engine/Cargo.toml",
    ];
    for path in &crate_tomls {
        let content = read_packaging_file(path)?;
        // Accept either `version.workspace = true` or matching explicit version
        let uses_workspace = content.contains("version.workspace = true");
        let has_matching = content.contains(&format!("version = \"{root_version}\""));
        assert!(
            uses_workspace || has_matching,
            "{path} version does not match workspace version {root_version}"
        );
    }
    Ok(())
}

// ============================================================================
// Systemd service file validation
// ============================================================================

#[test]
fn systemd_service_has_required_sections() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    for section in &["[Unit]", "[Service]", "[Install]"] {
        assert!(
            content.contains(section),
            "Systemd service template missing {section} section"
        );
    }
    Ok(())
}

#[test]
fn systemd_service_type_is_notify() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("Type=notify"),
        "Systemd service should use Type=notify for readiness signaling"
    );
    Ok(())
}

#[test]
fn systemd_service_execstart_references_wheeld() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    let re = regex::Regex::new(r"ExecStart=.*wheeld")?;
    assert!(
        re.is_match(&content),
        "ExecStart must reference the wheeld binary"
    );
    Ok(())
}

#[test]
fn systemd_service_has_security_hardening() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    let hardening_directives = [
        "NoNewPrivileges=true",
        "ProtectSystem=strict",
        "PrivateTmp=true",
        "ProtectKernelTunables=true",
        "ProtectKernelModules=true",
        "RestrictNamespaces=true",
    ];
    let mut missing = Vec::new();
    for directive in &hardening_directives {
        if !content.contains(directive) {
            missing.push(*directive);
        }
    }
    assert!(
        missing.is_empty(),
        "Systemd service missing security hardening: {missing:?}"
    );
    Ok(())
}

#[test]
fn systemd_service_has_restart_policy() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("Restart=on-failure"),
        "Systemd service should have Restart=on-failure"
    );
    let re = regex::Regex::new(r"RestartSec=\d+")?;
    assert!(
        re.is_match(&content),
        "Systemd service should have RestartSec"
    );
    Ok(())
}

#[test]
fn systemd_service_allows_realtime_scheduling() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("CAP_SYS_NICE"),
        "Systemd service needs CAP_SYS_NICE for RT scheduling"
    );
    assert!(
        content.contains("RestrictRealtime=false"),
        "RestrictRealtime must be false for 1kHz RT processing"
    );
    Ok(())
}

#[test]
fn systemd_service_state_directories_configured() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    for directive in &[
        "StateDirectory=",
        "ConfigurationDirectory=",
        "LogsDirectory=",
        "RuntimeDirectory=",
    ] {
        assert!(
            content.contains(directive),
            "Systemd service missing {directive}"
        );
    }
    Ok(())
}

#[test]
fn systemd_service_hup_reload() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("ExecReload=") && content.contains("HUP"),
        "Systemd service should support SIGHUP reload via ExecReload"
    );
    Ok(())
}

// ============================================================================
// Udev rules validation (existing + new)
// ============================================================================

#[test]
fn udev_rules_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    assert!(
        path.exists(),
        "udev rules file not found at: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn udev_rules_all_known_vids_present() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;
    let (pairs, wide_vids) = parse_udev_rules(&content)?;

    let mut missing = Vec::new();
    for (vid, pid, desc) in known_vid_pids() {
        if wide_vids.contains(vid) {
            continue;
        }
        if !pairs.contains(&(vid.to_string(), pid.to_string())) {
            missing.push(format!("  {vid}:{pid} ({desc})"));
        }
    }

    assert!(
        missing.is_empty(),
        "Missing VID/PID pairs in udev rules:\n{}",
        missing.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_vendor_wide_vids_present() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;
    let (_pairs, wide_vids) = parse_udev_rules(&content)?;

    for vid in vendor_wide_vids() {
        assert!(
            wide_vids.contains(vid),
            "Vendor-wide VID {vid} not found in udev rules"
        );
    }
    Ok(())
}

#[test]
fn udev_rules_syntax_valid() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;

    let vid_re = regex::Regex::new(r#"ATTRS\{idVendor\}=="([^"]*)""#)?;
    let pid_re = regex::Regex::new(r#"ATTRS\{idProduct\}=="([^"]*)""#)?;
    let hex4_re = regex::Regex::new(r"^[0-9a-fA-F]{4}$")?;

    let mut errors = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Every non-comment line must have an operator
        if !trimmed.contains("==") && !trimmed.contains("+=") && !trimmed.contains('=') {
            errors.push(format!("Line {}: no assignment operator", lineno + 1));
        }
        // Validate VID format
        for caps in vid_re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1)
                && !hex4_re.is_match(val.as_str())
            {
                errors.push(format!(
                    "Line {}: invalid VID '{}'",
                    lineno + 1,
                    val.as_str()
                ));
            }
        }
        // Validate PID format
        for caps in pid_re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1)
                && !hex4_re.is_match(val.as_str())
            {
                errors.push(format!(
                    "Line {}: invalid PID '{}'",
                    lineno + 1,
                    val.as_str()
                ));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "udev rules syntax errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_no_duplicate_hidraw_entries() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;

    let re = regex::Regex::new(
        r#"SUBSYSTEM=="hidraw".*ATTRS\{idVendor\}=="([0-9a-fA-F]{4})".*ATTRS\{idProduct\}=="([0-9a-fA-F]{4})""#,
    )?;

    let mut seen = HashSet::new();
    let mut dupes = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(caps) = re.captures(trimmed) {
            let vid = caps.get(1).map(|m| m.as_str().to_lowercase());
            let pid = caps.get(2).map(|m| m.as_str().to_lowercase());
            if let (Some(v), Some(p)) = (vid, pid) {
                let key = format!("{v}:{p}");
                if !seen.insert(key.clone()) {
                    dupes.push(key);
                }
            }
        }
    }

    assert!(
        dupes.is_empty(),
        "Duplicate hidraw VID:PID entries: {:?}",
        dupes
    );
    Ok(())
}

#[test]
fn udev_rules_has_power_management_section() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    assert!(
        content.contains("power/autosuspend"),
        "udev rules should disable USB autosuspend for racing wheels"
    );
    // At least the major vendors should have autosuspend disabled
    for vid in &["046d", "044f", "0eb7", "346e"] {
        let pattern = format!("ATTRS{{idVendor}}==\"{vid}\"");
        let has_power_rule = content
            .lines()
            .any(|line| line.contains("autosuspend") && line.contains(&pattern));
        assert!(has_power_rule, "Missing USB autosuspend rule for VID {vid}");
    }
    Ok(())
}

#[test]
fn udev_rules_has_service_restart_trigger() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    assert!(
        content.contains("try-restart") || content.contains("restart"),
        "udev rules should trigger service restart on device connect"
    );
    Ok(())
}

// ============================================================================
// HID quirks (modprobe.d) validation
// ============================================================================

#[test]
fn hid_quirks_conf_exists_and_valid() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/90-racing-wheel-quirks.conf")?;
    assert!(
        content.contains("usbhid"),
        "Quirks conf must reference the usbhid module"
    );
    assert!(
        content.contains("quirks="),
        "Quirks conf must set quirks parameter"
    );
    // Asetek VID 0x2433 should be present
    assert!(
        content.to_lowercase().contains("0x2433"),
        "Quirks conf must include Asetek VID 0x2433"
    );
    // Simagic EVO VID 0x3670 should be present (GT Neo / EVO reboot quirk)
    assert!(
        content.to_lowercase().contains("0x3670"),
        "Quirks conf must include Simagic EVO VID 0x3670"
    );
    Ok(())
}

// ============================================================================
// Packaging directory structure
// ============================================================================

#[test]
fn packaging_linux_directory_complete() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let linux_dir = root.join("packaging/linux");
    assert!(linux_dir.exists(), "packaging/linux/ directory missing");

    let required_files = [
        "99-racing-wheel-suite.rules",
        "90-racing-wheel-quirks.conf",
        "install.sh",
        "wheeld.service.template",
        "openracing.spec",
        "com.github.openracing.OpenRacing.yml",
        "debian/control",
    ];
    for f in &required_files {
        let path = linux_dir.join(f);
        assert!(path.exists(), "Missing Linux packaging file: {f}");
    }
    Ok(())
}

#[test]
fn packaging_windows_directory_exists() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let win_dir = root.join("packaging/windows");
    assert!(win_dir.exists(), "packaging/windows/ directory missing");

    // Check for MSI stub or build script
    let has_wix = win_dir.join("wheel-suite.wxs").exists();
    let has_build = win_dir.join("build-msi.ps1").exists();
    assert!(
        has_wix || has_build,
        "packaging/windows/ must contain wheel-suite.wxs or build-msi.ps1"
    );
    Ok(())
}

// ============================================================================
// Windows MSI installer metadata validation
// ============================================================================

#[test]
fn wix_manifest_has_valid_xml_structure() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.starts_with("<?xml"),
        "WXS file must start with XML declaration"
    );
    assert!(
        content.contains("<Wix"),
        "WXS file must contain <Wix root element"
    );
    assert!(
        content.contains("<Product"),
        "WXS file must contain <Product element"
    );
    Ok(())
}

#[test]
fn wix_manifest_references_correct_binaries() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("wheeld.exe"),
        "WXS must reference wheeld.exe"
    );
    assert!(
        content.contains("wheelctl.exe"),
        "WXS must reference wheelctl.exe"
    );
    Ok(())
}

#[test]
fn wix_manifest_has_upgrade_code() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    let re = regex::Regex::new(r"UpgradeCode=")?;
    assert!(
        re.is_match(&content),
        "WXS must define an UpgradeCode for clean upgrades"
    );
    assert!(
        content.contains("MajorUpgrade"),
        "WXS must contain MajorUpgrade element for upgrade handling"
    );
    Ok(())
}

#[test]
fn wix_manifest_product_name_correct() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("OpenRacing"),
        "WXS Product Name must reference OpenRacing"
    );
    Ok(())
}

#[test]
fn wix_manifest_service_registration() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("ServiceInstall"),
        "WXS must contain ServiceInstall element"
    );
    assert!(
        content.contains("ServiceControl"),
        "WXS must contain ServiceControl element"
    );
    assert!(
        content.contains("OpenRacingService"),
        "Windows service name must be OpenRacingService"
    );
    Ok(())
}

#[test]
fn wix_manifest_service_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("Start=\"auto\""),
        "Windows service must be configured to auto-start"
    );
    Ok(())
}

#[test]
fn wix_manifest_environment_path() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("Environment") && content.contains("PATH"),
        "WXS must add bin directory to system PATH"
    );
    Ok(())
}

#[test]
fn wix_manifest_uninstall_support() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    // ServiceControl with Remove="uninstall" handles service cleanup
    assert!(
        content.contains("Remove=\"uninstall\""),
        "WXS must remove service on uninstall"
    );
    // Stop on both install and uninstall
    assert!(
        content.contains("Stop=\"both\""),
        "WXS must stop service on both install and uninstall"
    );
    Ok(())
}

#[test]
fn wix_manifest_mmcss_registration() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("Multimedia\\SystemProfile\\Tasks"),
        "WXS must register MMCSS task for real-time thread priority"
    );
    assert!(
        content.contains("Scheduling Category"),
        "MMCSS task must set Scheduling Category"
    );
    Ok(())
}

// ============================================================================
// Linux build script validation
// ============================================================================

#[test]
fn build_packages_script_supports_deb_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("dpkg-deb") || content.contains("build_deb"),
        "Build script must support .deb packages"
    );
    assert!(
        content.contains("rpmbuild") || content.contains("build_rpm"),
        "Build script must support .rpm packages"
    );
    Ok(())
}

#[test]
fn build_packages_script_validates_binaries() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("wheeld") && content.contains("wheelctl"),
        "Build script must validate both wheeld and wheelctl binaries"
    );
    Ok(())
}

#[test]
fn deb_control_has_correct_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    // The build script creates the control file inline
    assert!(
        content.contains("Package: openracing"),
        "DEB control must use package name 'openracing'"
    );
    assert!(
        content.contains("Architecture:"),
        "DEB control must specify Architecture"
    );
    assert!(
        content.contains("Depends:"),
        "DEB control must specify Depends"
    );
    Ok(())
}

#[test]
fn rpm_spec_has_correct_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("Name:           openracing"),
        "RPM spec must use package name 'openracing'"
    );
    assert!(
        content.contains("License:"),
        "RPM spec must specify License"
    );
    assert!(
        content.contains("%files"),
        "RPM spec must have %files section"
    );
    Ok(())
}

#[test]
fn build_packages_generates_checksums() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("sha256sum") || content.contains("sha256"),
        "Build script must generate SHA-256 checksums for packages"
    );
    Ok(())
}

// ============================================================================
// Install script validation
// ============================================================================

#[test]
fn install_script_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    // mkdir -p is idempotent (won't fail if directory exists)
    assert!(
        content.contains("mkdir -p"),
        "Install script should use mkdir -p for idempotent directory creation"
    );
    // cp overwrites by default (idempotent)
    assert!(
        content.contains("cp "),
        "Install script should use cp for file installation"
    );
    Ok(())
}

#[test]
fn install_script_creates_log_directory() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("logs") || content.contains("log"),
        "Install script should create log directory"
    );
    Ok(())
}

#[test]
fn install_script_sets_permissions() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("chmod"),
        "Install script must set file permissions"
    );
    Ok(())
}

#[test]
fn install_script_handles_prefix_option() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("--prefix") || content.contains("INSTALL_PREFIX"),
        "Install script should support configurable install prefix"
    );
    Ok(())
}

// ============================================================================
// Config file defaults validation
// ============================================================================

#[test]
fn systemd_service_has_default_config_env() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("WHEEL_CONFIG_DIR=") || content.contains("ConfigurationDirectory="),
        "Service must define configuration directory"
    );
    assert!(
        content.contains("RUST_LOG="),
        "Service must set default RUST_LOG level"
    );
    Ok(())
}

// ============================================================================
// PID file and signal handling
// ============================================================================

#[test]
fn systemd_service_uses_mainpid_for_reload() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("$MAINPID"),
        "ExecReload should use $MAINPID for signal delivery"
    );
    Ok(())
}

#[test]
fn systemd_service_has_stop_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    let re = regex::Regex::new(r"TimeoutStopSec=\d+")?;
    assert!(
        re.is_match(&content),
        "Systemd service must set TimeoutStopSec for graceful shutdown"
    );
    Ok(())
}

// ============================================================================
// Windows portable build validation
// ============================================================================

#[test]
fn portable_build_script_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/windows/build-portable.ps1");
    assert!(
        path.exists(),
        "Windows portable build script missing: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn msi_build_script_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/windows/build-msi.ps1");
    assert!(
        path.exists(),
        "Windows MSI build script missing: {}",
        path.display()
    );
    Ok(())
}

// ============================================================================
// hwdb file validation
// ============================================================================

/// Parse VID/PID pairs from the hwdb file.
/// Returns uppercase hex pairs like ("046D", "C299").
fn parse_hwdb_vid_pids(
    content: &str,
) -> Result<HashSet<(String, String)>, Box<dyn std::error::Error>> {
    let re =
        regex::Regex::new(r"(?i)id-input:modalias:input:\*v([0-9A-Fa-f]{4})p([0-9A-Fa-f]{4})\*")?;
    let mut pairs = HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(caps) = re.captures(line)
            && let (Some(vid), Some(pid)) = (caps.get(1), caps.get(2))
        {
            pairs.insert((vid.as_str().to_uppercase(), pid.as_str().to_uppercase()));
        }
    }
    Ok(pairs)
}

/// Known wheelbase VID/PID pairs that MUST have hwdb entries.
fn known_wheelbase_vid_pids() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        // Logitech
        ("046D", "C299", "Logitech G25"),
        ("046D", "C298", "Logitech DFP"),
        ("046D", "C29A", "Logitech DFGT"),
        ("046D", "C29B", "Logitech G27"),
        ("046D", "C24F", "Logitech G29"),
        ("046D", "C262", "Logitech G920"),
        ("046D", "C266", "Logitech G923"),
        ("046D", "C267", "Logitech G923 PS"),
        ("046D", "C26D", "Logitech G923 Xbox HID++"),
        ("046D", "C26E", "Logitech G923 Xbox"),
        ("046D", "C268", "Logitech G Pro"),
        ("046D", "C272", "Logitech G Pro Xbox"),
        // Fanatec
        ("0EB7", "0006", "Fanatec DD1"),
        ("0EB7", "0007", "Fanatec DD2"),
        ("0EB7", "0020", "Fanatec CSL DD"),
        // Thrustmaster
        ("044F", "B65E", "Thrustmaster T500 RS"),
        ("044F", "B66D", "Thrustmaster T300RS PS4"),
        ("044F", "B696", "Thrustmaster T248"),
        ("044F", "B69B", "Thrustmaster T818"),
        // Moza
        ("346E", "0002", "Moza R9"),
        ("346E", "0004", "Moza R5"),
        ("346E", "0005", "Moza R3"),
        // Simucube
        ("16D0", "0D5A", "Simucube SC1"),
        ("16D0", "0D5F", "Simucube SC2 Ultimate"),
        ("16D0", "0D60", "Simucube SC2 Pro"),
        ("16D0", "0D61", "Simucube SC2 Sport"),
        // Asetek
        ("2433", "F300", "Asetek Invicta"),
        ("2433", "F301", "Asetek Forte"),
    ]
}

#[test]
fn hwdb_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.hwdb");
    assert!(path.exists(), "hwdb file missing: {}", path.display());
    Ok(())
}

#[test]
fn hwdb_file_has_header() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    assert!(
        content.contains("ID_INPUT_JOYSTICK"),
        "hwdb file must reference ID_INPUT_JOYSTICK"
    );
    assert!(
        content.contains("SPDX-License-Identifier"),
        "hwdb file must contain SPDX license identifier"
    );
    Ok(())
}

#[test]
fn hwdb_entries_use_single_space_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        // Skip comments and blank lines — only check property assignment lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if line.contains("ID_INPUT_") {
            assert!(
                line.starts_with(' ') && !line.starts_with("  "),
                "Line {} must start with exactly one space: {:?}",
                i + 1,
                line
            );
        }
    }
    Ok(())
}

#[test]
fn hwdb_vid_pid_values_are_uppercase() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let re = regex::Regex::new(r"id-input:modalias:input:\*v([0-9A-Fa-f]{4})p([0-9A-Fa-f]{4})\*")?;
    for (i, line) in content.lines().enumerate() {
        if let Some(caps) = re.captures(line) {
            let vid = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let pid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            assert!(
                vid == vid.to_uppercase() && pid == pid.to_uppercase(),
                "Line {}: VID/PID must be uppercase hex, got v{}p{}",
                i + 1,
                vid,
                pid
            );
        }
    }
    Ok(())
}

#[test]
fn hwdb_contains_all_known_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let hwdb_pairs = parse_hwdb_vid_pids(&content)?;
    let mut missing = Vec::new();

    for (vid, pid, desc) in known_wheelbase_vid_pids() {
        if !hwdb_pairs.contains(&(vid.to_string(), pid.to_string())) {
            missing.push(format!("{desc} ({vid}:{pid})"));
        }
    }

    assert!(
        missing.is_empty(),
        "hwdb file is missing entries for these wheelbases: {}",
        missing.join(", ")
    );
    Ok(())
}

#[test]
fn hwdb_sets_accelerometer_to_zero() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let accel_count = content.matches("ID_INPUT_ACCELEROMETER=0").count();
    let joystick_count = content.matches("ID_INPUT_JOYSTICK=1").count();
    assert!(
        accel_count > 0,
        "hwdb file must contain ID_INPUT_ACCELEROMETER=0 entries"
    );
    assert_eq!(
        accel_count, joystick_count,
        "Every entry must have both ACCELEROMETER=0 and JOYSTICK=1 (got {accel_count} vs {joystick_count})"
    );
    Ok(())
}

#[test]
fn hwdb_has_minimum_device_count() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let pairs = parse_hwdb_vid_pids(&content)?;
    assert!(
        pairs.len() >= 90,
        "hwdb file should have at least 90 device entries, found {}",
        pairs.len()
    );
    Ok(())
}

#[test]
fn install_script_references_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("99-racing-wheel-suite.hwdb"),
        "install.sh must reference the hwdb file"
    );
    assert!(
        content.contains("systemd-hwdb update"),
        "install.sh must run systemd-hwdb update after installing hwdb"
    );
    Ok(())
}

// ============================================================================
// Additional packaging infrastructure validation
// ============================================================================

#[test]
fn systemd_service_template_placeholder_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    // All %INSTALL_PATH% placeholders should be substitutable
    let placeholder_count = content.matches("%INSTALL_PATH%").count();
    assert!(
        placeholder_count >= 2,
        "Service template should reference %INSTALL_PATH% in ExecStart and WorkingDirectory, found {placeholder_count}"
    );
    // No stale/mixed placeholder styles
    assert!(
        !content.contains("${INSTALL_PATH}") && !content.contains("$INSTALL_PATH"),
        "Service template should use %INSTALL_PATH% consistently, not shell variable syntax"
    );
    Ok(())
}

#[test]
fn systemd_service_template_no_absolute_hardcoded_paths() -> Result<(), Box<dyn std::error::Error>>
{
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    // ExecStart should not hardcode /usr or /opt
    let exec_lines: Vec<&str> = content
        .lines()
        .filter(|l| l.starts_with("ExecStart="))
        .collect();
    for line in &exec_lines {
        assert!(
            !line.contains("/usr/") && !line.contains("/opt/"),
            "ExecStart should use %INSTALL_PATH% placeholder, not hardcoded paths: {line}"
        );
    }
    Ok(())
}

#[test]
fn systemd_service_memory_deny_write_execute() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("MemoryDenyWriteExecute=true"),
        "Service must enable MemoryDenyWriteExecute for security"
    );
    Ok(())
}

#[test]
fn systemd_service_protect_home() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("ProtectHome="),
        "Service must set ProtectHome directive"
    );
    Ok(())
}

#[test]
fn systemd_service_supplementary_groups() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("SupplementaryGroups=") && content.contains("input"),
        "Service must add 'input' supplementary group for HID access"
    );
    Ok(())
}

#[test]
fn udev_rules_all_lines_well_formed() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Each rule line must contain SUBSYSTEM or ACTION or ATTR
        let has_key = trimmed.contains("SUBSYSTEM")
            || trimmed.contains("ACTION")
            || trimmed.contains("ATTR")
            || trimmed.contains("ENV")
            || trimmed.contains("TAG")
            || trimmed.contains("RUN")
            || trimmed.contains("IMPORT")
            || trimmed.contains("KERNEL");
        if !has_key {
            errors.push(format!(
                "Line {}: no valid udev key found: {trimmed}",
                i + 1
            ));
        }
    }
    assert!(
        errors.is_empty(),
        "udev rules have malformed lines:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_mode_is_octal() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mode_re = regex::Regex::new(r#"MODE="([^"]*)""#)?;
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for caps in mode_re.captures_iter(trimmed) {
            if let Some(mode) = caps.get(1) {
                let val = mode.as_str();
                assert!(
                    val.len() == 4 && val.chars().all(|c| c.is_ascii_digit()),
                    "Line {}: MODE value must be 4-digit octal, got '{val}'",
                    i + 1
                );
            }
        }
    }
    Ok(())
}

#[test]
fn hwdb_quirks_conf_format_valid() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/90-racing-wheel-quirks.conf")?;
    // Should be a single 'options' line for usbhid
    let options_lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .collect();
    assert!(
        !options_lines.is_empty(),
        "Quirks conf must have at least one options line"
    );
    for line in &options_lines {
        assert!(
            line.starts_with("options "),
            "Non-comment lines must start with 'options': {line}"
        );
    }
    // Validate quirk format: VID:PID:QUIRK comma-separated
    let quirk_re = regex::Regex::new(r"0x[0-9A-Fa-f]{4}:0x[0-9A-Fa-f]{4}:0x[0-9A-Fa-f]{4}")?;
    let quirk_count = quirk_re.find_iter(&content).count();
    assert!(
        quirk_count >= 1,
        "Quirks conf must contain at least one VID:PID:QUIRK entry, found {quirk_count}"
    );
    Ok(())
}

#[test]
fn package_name_consistent_across_formats() -> Result<(), Box<dyn std::error::Error>> {
    let build_script = read_packaging_file("packaging/linux/build-packages.sh")?;
    let wxs = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    let service = read_packaging_file("packaging/linux/wheeld.service.template")?;

    // The package name "openracing" or "OpenRacing" should appear across all formats
    assert!(
        build_script.contains("openracing"),
        "Build script must use 'openracing' package name"
    );
    assert!(
        wxs.contains("OpenRacing"),
        "WXS must reference 'OpenRacing'"
    );
    assert!(
        service.contains("OpenRacing")
            || service.contains("openracing")
            || service.contains("racing-wheel-suite"),
        "Service template must reference project name"
    );
    Ok(())
}

#[test]
fn package_description_present_in_all_formats() -> Result<(), Box<dyn std::error::Error>> {
    let build_script = read_packaging_file("packaging/linux/build-packages.sh")?;
    let wxs = read_packaging_file("packaging/windows/wheel-suite.wxs")?;

    // Both should have a human-readable description
    assert!(
        build_script.contains("force feedback") || build_script.contains("racing wheel"),
        "Build script must include product description"
    );
    assert!(
        wxs.contains("force feedback") || wxs.contains("racing wheel"),
        "WXS must include product description"
    );
    Ok(())
}

#[test]
fn linux_file_paths_use_fhs_layout() -> Result<(), Box<dyn std::error::Error>> {
    let build_script = read_packaging_file("packaging/linux/build-packages.sh")?;
    // DEB packages should install to /usr/bin
    assert!(
        build_script.contains("/usr/bin"),
        "DEB package should install binaries to /usr/bin"
    );
    // Udev rules to standard location
    assert!(
        build_script.contains("udev/rules.d") || build_script.contains("udevrulesdir"),
        "Package must install udev rules to standard rules.d directory"
    );
    Ok(())
}

#[test]
fn windows_install_path_uses_program_files() -> Result<(), Box<dyn std::error::Error>> {
    let wxs = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        wxs.contains("ProgramFiles64Folder") || wxs.contains("ProgramFilesFolder"),
        "WXS must install to Program Files directory"
    );
    Ok(())
}

#[test]
fn install_script_has_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("set -e") || content.contains("set -euo"),
        "Install script must use 'set -e' for error handling"
    );
    Ok(())
}

#[test]
fn build_script_has_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("set -e") || content.contains("set -euo"),
        "Build script must use 'set -e' for error handling"
    );
    Ok(())
}

#[test]
fn wix_manifest_has_directory_structure() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    let required_dirs = ["bin", "config", "profiles", "plugins", "logs"];
    let mut missing = Vec::new();
    for dir in &required_dirs {
        if !content.to_lowercase().contains(&dir.to_lowercase()) {
            missing.push(*dir);
        }
    }
    assert!(
        missing.is_empty(),
        "WXS missing directory definitions: {missing:?}"
    );
    Ok(())
}

#[test]
fn wix_manifest_installer_version_minimum() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    let re = regex::Regex::new(r#"InstallerVersion="(\d+)""#)?;
    if let Some(caps) = re.captures(&content) {
        let version: u32 = caps.get(1).map(|m| m.as_str()).unwrap_or("0").parse()?;
        assert!(
            version >= 500,
            "WXS InstallerVersion should be >= 500 for Windows 10+ features, got {version}"
        );
    }
    Ok(())
}

#[test]
fn wix_manifest_per_machine_install() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/windows/wheel-suite.wxs")?;
    assert!(
        content.contains("perMachine"),
        "WXS must use perMachine install scope for service registration"
    );
    Ok(())
}

// ============================================================================
// RPM spec file validation
// ============================================================================

#[test]
fn rpm_spec_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/openracing.spec");
    assert!(path.exists(), "RPM spec file missing: {}", path.display());
    Ok(())
}

#[test]
fn rpm_spec_has_required_sections() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    let required = [
        "%prep",
        "%install",
        "%files",
        "%post",
        "%preun",
        "%changelog",
    ];
    let mut missing = Vec::new();
    for section in &required {
        if !content.contains(section) {
            missing.push(*section);
        }
    }
    assert!(missing.is_empty(), "RPM spec missing sections: {missing:?}");
    Ok(())
}

#[test]
fn rpm_spec_has_correct_package_name() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("Name:           openracing"),
        "RPM spec must use package name 'openracing'"
    );
    Ok(())
}

#[test]
fn rpm_spec_has_license() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("License:") && content.contains("MIT"),
        "RPM spec must declare MIT license"
    );
    Ok(())
}

#[test]
fn rpm_spec_installs_binaries() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("wheeld") && content.contains("wheelctl"),
        "RPM spec must install both wheeld and wheelctl"
    );
    Ok(())
}

#[test]
fn rpm_spec_installs_udev_rules() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("99-racing-wheel-suite.rules"),
        "RPM spec must install udev rules"
    );
    assert!(
        content.contains("_udevrulesdir"),
        "RPM spec must use %_udevrulesdir macro for udev rules path"
    );
    Ok(())
}

#[test]
fn rpm_spec_installs_systemd_service() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("openracing.service"),
        "RPM spec must install systemd service"
    );
    assert!(
        content.contains("_userunitdir"),
        "RPM spec must use %_userunitdir macro for user service path"
    );
    Ok(())
}

#[test]
fn rpm_spec_has_url() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("URL:") && content.contains("github.com"),
        "RPM spec must have project URL"
    );
    Ok(())
}

#[test]
fn rpm_spec_post_reloads_udev() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    // After %post, udev rules should be reloaded
    let post_idx = content.find("%post");
    let preun_idx = content.find("%preun");
    if let (Some(start), Some(end)) = (post_idx, preun_idx) {
        let post_section = &content[start..end];
        assert!(
            post_section.contains("udev"),
            "RPM %post must reload udev rules"
        );
    }
    Ok(())
}

// ============================================================================
// Flatpak manifest validation
// ============================================================================

#[test]
fn flatpak_manifest_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/com.github.openracing.OpenRacing.yml");
    assert!(
        path.exists(),
        "Flatpak manifest missing: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn flatpak_manifest_has_correct_app_id() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;
    assert!(
        content.contains("app-id: com.github.openracing.OpenRacing"),
        "Flatpak manifest must have correct reverse-DNS app-id"
    );
    Ok(())
}

#[test]
fn flatpak_manifest_has_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;
    assert!(
        content.contains("runtime: org.freedesktop.Platform"),
        "Flatpak manifest must specify freedesktop Platform runtime"
    );
    assert!(
        content.contains("sdk: org.freedesktop.Sdk"),
        "Flatpak manifest must specify freedesktop Sdk"
    );
    Ok(())
}

#[test]
fn flatpak_manifest_grants_device_access() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;
    assert!(
        content.contains("--device=all"),
        "Flatpak manifest must grant device access for HID racing wheels"
    );
    Ok(())
}

#[test]
fn flatpak_manifest_installs_binaries() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;
    assert!(
        content.contains("wheeld") && content.contains("wheelctl"),
        "Flatpak manifest must install wheeld and wheelctl"
    );
    Ok(())
}

#[test]
fn flatpak_manifest_has_rust_sdk_extension() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;
    assert!(
        content.contains("rust-stable"),
        "Flatpak manifest must include Rust SDK extension"
    );
    Ok(())
}

// ============================================================================
// Debian packaging validation
// ============================================================================

#[test]
fn debian_control_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/debian/control");
    assert!(
        path.exists(),
        "Debian control file missing: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn debian_control_has_correct_package_name() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/control")?;
    assert!(
        content.contains("Package: openracing"),
        "Debian control must use package name 'openracing'"
    );
    Ok(())
}

#[test]
fn debian_control_has_source_section() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/control")?;
    assert!(
        content.contains("Source: openracing"),
        "Debian control must have Source section"
    );
    assert!(
        content.contains("Build-Depends:"),
        "Debian control must specify Build-Depends"
    );
    Ok(())
}

#[test]
fn debian_control_has_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/control")?;
    assert!(
        content.contains("libudev"),
        "Debian control must depend on libudev"
    );
    assert!(
        content.contains("Recommends:") && content.contains("rtkit"),
        "Debian control should recommend rtkit for real-time scheduling"
    );
    Ok(())
}

#[test]
fn debian_control_has_homepage() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/control")?;
    assert!(
        content.contains("Homepage:") && content.contains("github.com"),
        "Debian control must have Homepage field"
    );
    Ok(())
}

#[test]
fn debian_control_has_description() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/control")?;
    assert!(
        content.contains("Description:"),
        "Debian control must have Description field"
    );
    assert!(
        content.contains("force feedback") || content.contains("racing wheel"),
        "Debian control description must mention the product"
    );
    Ok(())
}

#[test]
fn debian_rules_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/debian/rules");
    assert!(
        path.exists(),
        "Debian rules file missing: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn debian_rules_installs_service() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/debian/rules")?;
    assert!(
        content.contains("openracing.service"),
        "Debian rules must install systemd service"
    );
    Ok(())
}

#[test]
fn debian_copyright_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/debian/copyright");
    assert!(
        path.exists(),
        "Debian copyright file missing: {}",
        path.display()
    );
    let content = read_packaging_file("packaging/linux/debian/copyright")?;
    assert!(
        content.contains("MIT") && content.contains("Apache"),
        "Debian copyright must declare both MIT and Apache-2.0 licenses"
    );
    Ok(())
}

// ============================================================================
// Udev rule advanced syntax validation
// ============================================================================

#[test]
fn udev_rules_vid_values_are_lowercase() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let re = regex::Regex::new(r#"ATTRS\{idVendor\}=="([^"]*)""#)?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for caps in re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1) {
                let v = val.as_str();
                if v != v.to_lowercase() {
                    errors.push(format!("Line {}: VID '{}' must be lowercase", i + 1, v));
                }
            }
        }
    }
    assert!(
        errors.is_empty(),
        "udev rules VID case errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_pid_values_are_lowercase() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let re = regex::Regex::new(r#"ATTRS\{idProduct\}=="([^"]*)""#)?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for caps in re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1) {
                let v = val.as_str();
                if v != v.to_lowercase() {
                    errors.push(format!("Line {}: PID '{}' must be lowercase", i + 1, v));
                }
            }
        }
    }
    assert!(
        errors.is_empty(),
        "udev rules PID case errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_autosuspend_value_correct() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let re = regex::Regex::new(r#"ATTR\{power/autosuspend\}="([^"]*)""#)?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for caps in re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1)
                && val.as_str() != "-1"
            {
                errors.push(format!(
                    "Line {}: autosuspend should be '-1', got '{}'",
                    i + 1,
                    val.as_str()
                ));
            }
        }
    }
    assert!(
        errors.is_empty(),
        "udev rules autosuspend errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

// ============================================================================
// File permission expectations in build script
// ============================================================================

#[test]
fn build_script_sets_binary_permissions_755() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("755") && content.contains("wheeld"),
        "Build script must set binary permissions to 755"
    );
    Ok(())
}

#[test]
fn build_script_sets_config_permissions_644() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        content.contains("644"),
        "Build script must set config file permissions to 644"
    );
    Ok(())
}

#[test]
fn rpm_spec_sets_correct_file_permissions() -> Result<(), Box<dyn std::error::Error>> {
    let content = read_packaging_file("packaging/linux/openracing.spec")?;
    assert!(
        content.contains("install -m 755") && content.contains("wheeld"),
        "RPM spec must install binaries with mode 755"
    );
    assert!(
        content.contains("install -m 644") && content.contains("openracing.service"),
        "RPM spec must install service file with mode 644"
    );
    Ok(())
}

// ============================================================================
// Cross-format package metadata consistency
// ============================================================================

#[test]
fn all_package_formats_use_same_name() -> Result<(), Box<dyn std::error::Error>> {
    let rpm_spec = read_packaging_file("packaging/linux/openracing.spec")?;
    let deb_control = read_packaging_file("packaging/linux/debian/control")?;
    let flatpak = read_packaging_file("packaging/linux/com.github.openracing.OpenRacing.yml")?;

    assert!(
        rpm_spec.contains("Name:           openracing"),
        "RPM spec package name must be 'openracing'"
    );
    assert!(
        deb_control.contains("Package: openracing"),
        "Debian package name must be 'openracing'"
    );
    assert!(
        flatpak.contains("openracing"),
        "Flatpak manifest must reference 'openracing'"
    );
    Ok(())
}

#[test]
fn all_package_formats_have_license() -> Result<(), Box<dyn std::error::Error>> {
    let rpm_spec = read_packaging_file("packaging/linux/openracing.spec")?;
    let deb_copyright = read_packaging_file("packaging/linux/debian/copyright")?;

    assert!(
        rpm_spec.contains("MIT") && rpm_spec.contains("Apache"),
        "RPM spec must declare dual license"
    );
    assert!(
        deb_copyright.contains("MIT") && deb_copyright.contains("Apache"),
        "Debian copyright must declare dual license"
    );
    Ok(())
}

#[test]
fn all_package_formats_reference_homepage() -> Result<(), Box<dyn std::error::Error>> {
    let rpm_spec = read_packaging_file("packaging/linux/openracing.spec")?;
    let deb_control = read_packaging_file("packaging/linux/debian/control")?;

    let homepage = "github.com/EffortlessMetrics/OpenRacing";
    assert!(
        rpm_spec.contains(homepage),
        "RPM spec must reference project homepage"
    );
    assert!(
        deb_control.contains(homepage),
        "Debian control must reference project homepage"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// External control-stream contract bundle packaging (issue #179)
// ---------------------------------------------------------------------------

/// The Linux tarball build must package the versioned external control-stream
/// contract bundle so external `control_stream_v1` consumers ship with the
/// release artifacts.
#[test]
fn build_packages_wires_control_stream_contract() -> Result<(), Box<dyn std::error::Error>> {
    let script = read_packaging_file("packaging/linux/build-packages.sh")?;
    assert!(
        script.contains("control-stream-contract"),
        "build-packages.sh must generate the control-stream contract bundle"
    );
    assert!(
        script.contains("contract/control-stream"),
        "build-packages.sh must place the contract under contract/control-stream"
    );
    assert!(
        script.contains("--check"),
        "build-packages.sh must verify the packaged contract bundle is coherent"
    );
    Ok(())
}

/// The Linux installer must preserve the contract bundle next to installed
/// binaries so an input-only consumer can use the package without a source
/// checkout.
#[test]
fn linux_install_wires_control_stream_contract() -> Result<(), Box<dyn std::error::Error>> {
    let script = read_packaging_file("packaging/linux/install.sh")?;
    for asset in [
        "share/openracing/contract/control-stream",
        "control-stream-contract.json",
        "wheel.proto",
        "sample-capture.json",
        "SHA256SUMS",
    ] {
        assert!(
            script.contains(asset),
            "install.sh must preserve control-stream asset {asset}"
        );
    }
    Ok(())
}

/// The release workflow must run the artifact lifecycle proof before signing
/// or publishing release assets.
#[test]
fn release_workflow_wires_control_stream_artifact_smoke() -> Result<(), Box<dyn std::error::Error>>
{
    let workflow = read_packaging_file(".github/workflows/release.yml")?;
    assert!(
        workflow.contains("control-stream-artifact-smoke"),
        "release workflow must define the artifact lifecycle smoke job"
    );
    assert!(
        workflow.contains("control_stream_artifact_smoke.sh"),
        "release workflow must invoke the checked-in artifact smoke script"
    );
    assert!(
        workflow.contains("e625296e3020911655dc5c3df59d0a4898a04898"),
        "release workflow must identify the deterministic prior-lane fixture"
    );
    Ok(())
}

/// The contract generator/validator tool must exist and be registered as a
/// hyphenated bin so packaging and CI can invoke it.
#[test]
fn control_stream_contract_tool_is_registered() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        repo_root()
            .join("crates/tools/src/bin/control_stream_contract.rs")
            .exists(),
        "control-stream contract generator source must exist"
    );
    let tools_manifest = read_packaging_file("crates/tools/Cargo.toml")?;
    assert!(
        tools_manifest.contains("control-stream-contract"),
        "openracing-tools must register the control-stream-contract bin"
    );
    Ok(())
}

/// The release documentation must describe the published contract bundle so
/// external consumers know it exists and what it contains.
#[test]
fn releasing_doc_mentions_control_stream_contract() -> Result<(), Box<dyn std::error::Error>> {
    let releasing = read_packaging_file("RELEASING.md")?;
    assert!(
        releasing.to_lowercase().contains("control-stream contract")
            || releasing.contains("control-stream-contract"),
        "RELEASING.md must document the control-stream contract bundle artifact"
    );
    Ok(())
}
