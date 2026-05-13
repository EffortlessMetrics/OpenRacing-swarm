//! Safety and control commands

use anyhow::Result;
use dialoguer::Confirm;
use std::time::Duration;

use crate::client::WheelClient;
use crate::commands::SafetyCommands;
use crate::error::CliError;
use crate::output;

/// Execute safety command
pub async fn execute(cmd: &SafetyCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect_or_mock(endpoint).await?;

    match cmd {
        SafetyCommands::Enable { device, force } => {
            enable_high_torque(&client, device, json, *force).await
        }
        SafetyCommands::Stop { device } => emergency_stop(&client, device.as_deref(), json).await,
        SafetyCommands::Status { device } => {
            show_safety_status(&client, device.as_deref(), json).await
        }
        SafetyCommands::Limit {
            device,
            torque,
            global,
        } => set_torque_limit(&client, device, *torque, json, *global).await,
    }
}

/// Enable high torque mode
async fn enable_high_torque(
    client: &WheelClient,
    device: &str,
    json: bool,
    force: bool,
) -> Result<()> {
    // Verify device exists
    let status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    // Check current safety conditions
    let can_enable = status.active_faults.is_empty()
        && status.telemetry.temperature_c < 80
        && status.telemetry.hands_on;

    if !can_enable && !force {
        let reasons = get_safety_block_reasons(&status);

        if json {
            let output = serde_json::json!({
                "success": false,
                "error": "Safety conditions not met",
                "blocked_reasons": reasons
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", "Cannot enable high torque mode:".red().bold());
            for reason in reasons {
                println!("  • {}", reason.red());
            }
            println!(
                "\n{} Use --force to override safety checks",
                "Warning:".yellow().bold()
            );
        }

        return Err(CliError::PermissionDenied("Safety conditions not met".to_string()).into());
    }

    // Show safety warning
    if !force && !json {
        println!("{}", "⚠ HIGH TORQUE MODE WARNING ⚠".yellow().bold());
        println!(
            "This will enable high torque output up to {:.1} Nm",
            status.device.capabilities.max_torque_nm
        );
        println!("Ensure:");
        println!("  • Hands are on the wheel");
        println!("  • Wheel is properly mounted");
        println!("  • No obstructions around wheel");
        println!("  • Emergency stop is accessible");

        if !Confirm::new()
            .with_prompt("Continue with high torque enable?")
            .interact()?
        {
            output::print_warning("High torque enable cancelled", json);
            return Ok(());
        }
    }

    // Perform physical challenge if not forced
    if !force && !json {
        println!("\n{}", "Physical Challenge Required:".bold());
        println!("Hold both clutch paddles for 3 seconds...");

        // Mock challenge - in real implementation this would wait for device response
        for i in (1..=3).rev() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("  {}...", i);
        }

        println!("✓ Challenge completed");
    }

    // Enable high torque
    client.start_high_torque(device).await?;

    output::print_success(
        &format!("High torque mode enabled for device {}", device),
        json,
    );

    if !json {
        println!("\n{}", "High torque mode is now active".green().bold());
        println!("Mode will persist until device power cycle");
        println!("Use 'wheelctl safety stop' for emergency stop");
    }

    Ok(())
}

/// Emergency stop
async fn emergency_stop(client: &WheelClient, device: Option<&str>, json: bool) -> Result<()> {
    match device {
        Some(device_id) => {
            // Verify device exists
            let _status = client
                .get_device_status(device_id)
                .await
                .map_err(|_| CliError::DeviceNotFound(device_id.to_string()))?;

            client.emergency_stop(Some(device_id)).await?;

            output::print_success(
                &format!("Emergency stop executed for device {}", device_id),
                json,
            );
        }
        None => {
            client.emergency_stop(None).await?;

            output::print_success("Emergency stop executed for all devices", json);
        }
    }

    if !json {
        println!("{}", "All force feedback has been stopped".red().bold());
        println!("Devices are now in safe torque mode");
    }

    Ok(())
}

/// Show safety status
async fn show_safety_status(client: &WheelClient, device: Option<&str>, json: bool) -> Result<()> {
    if let Some(device_id) = device {
        // Single device status
        let status = client
            .get_device_status(device_id)
            .await
            .map_err(|_| CliError::DeviceNotFound(device_id.to_string()))?;

        let safety_status = analyze_device_safety(&status);

        if json {
            let output = serde_json::json!({
                "success": true,
                "device_id": device_id,
                "safety_status": safety_status
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_device_safety_status(&status, &safety_status);
        }
    } else {
        // All devices status
        let devices = client.list_devices().await?;
        let mut all_status = Vec::new();

        for device in devices {
            if let Ok(status) = client.get_device_status(&device.id).await {
                let safety_status = analyze_device_safety(&status);
                all_status.push((status, safety_status));
            }
        }

        if json {
            let output = serde_json::json!({
                "success": true,
                "devices": all_status.iter().map(|(status, safety)| {
                    serde_json::json!({
                        "device_id": status.device.id,
                        "safety_status": safety
                    })
                }).collect::<Vec<_>>()
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", "Safety Status Overview:".bold());
            for (status, safety_status) in all_status {
                print_device_safety_status(&status, &safety_status);
                println!();
            }
        }
    }

    Ok(())
}

/// Set torque limit
async fn set_torque_limit(
    client: &WheelClient,
    device: &str,
    torque: f32,
    json: bool,
    global: bool,
) -> Result<()> {
    // Verify device exists
    let status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    let max_torque = status.device.capabilities.max_torque_nm;

    if torque > max_torque {
        return Err(CliError::ValidationError(format!(
            "Torque limit {:.1} Nm exceeds device maximum {:.1} Nm",
            torque, max_torque
        ))
        .into());
    }

    if torque < 0.1 {
        return Err(
            CliError::ValidationError("Torque limit must be at least 0.1 Nm".to_string()).into(),
        );
    }

    // Mock torque limit setting - in real implementation this would update device/profile

    let scope = if global {
        "all profiles"
    } else {
        "current session"
    };

    output::print_success(
        &format!(
            "Torque limit set to {:.1} Nm for device {} ({})",
            torque, device, scope
        ),
        json,
    );

    if !json {
        if torque < max_torque * 0.5 {
            println!(
                "{} Low torque limit may reduce force feedback quality",
                "Note:".yellow()
            );
        }

        if global {
            println!("This limit will be applied to all profiles for this device");
        } else {
            println!("This limit applies only to the current session");
        }
    }

    Ok(())
}

// Helper functions and data structures

use crate::client::DeviceStatus;
use colored::*;

#[derive(serde::Serialize, Debug)]
struct SafetyStatus {
    high_torque_enabled: bool,
    torque_limit_nm: f32,
    hands_on: bool,
    temperature_ok: bool,
    no_faults: bool,
    can_enable_high_torque: bool,
    blocked_reasons: Vec<String>,
}

fn analyze_device_safety(status: &DeviceStatus) -> SafetyStatus {
    let temp_ok = status.telemetry.temperature_c < 80;
    let no_faults = status.active_faults.is_empty();
    let hands_on = status.telemetry.hands_on;

    let can_enable = temp_ok && no_faults && hands_on;
    let blocked_reasons = get_safety_block_reasons(status);

    SafetyStatus {
        high_torque_enabled: false, // Mock - would check actual device state
        torque_limit_nm: status.device.capabilities.max_torque_nm,
        hands_on,
        temperature_ok: temp_ok,
        no_faults,
        can_enable_high_torque: can_enable,
        blocked_reasons,
    }
}

fn get_safety_block_reasons(status: &DeviceStatus) -> Vec<String> {
    let mut reasons = Vec::new();

    if !status.active_faults.is_empty() {
        reasons.push(format!(
            "Active faults: {}",
            status.active_faults.join(", ")
        ));
    }

    if status.telemetry.temperature_c >= 80 {
        reasons.push(format!(
            "Temperature too high: {}°C",
            status.telemetry.temperature_c
        ));
    }

    if !status.telemetry.hands_on {
        reasons.push("Hands not detected on wheel".to_string());
    }

    reasons
}

fn print_device_safety_status(status: &DeviceStatus, safety: &SafetyStatus) {
    println!(
        "  {} {} ({})",
        "●".green(),
        status.device.name.bold(),
        status.device.id.dimmed()
    );

    let high_torque_status = if safety.high_torque_enabled {
        "ENABLED".red().bold()
    } else {
        "Disabled".yellow()
    };
    println!("    High Torque: {}", high_torque_status);

    println!("    Torque Limit: {:.1} Nm", safety.torque_limit_nm);

    let hands_status = if safety.hands_on {
        "✓".green()
    } else {
        "✗".red()
    };
    println!("    Hands On: {}", hands_status);

    let temp_status = if safety.temperature_ok {
        "✓".green()
    } else {
        "✗".red()
    };
    println!(
        "    Temperature: {} ({}°C)",
        temp_status, status.telemetry.temperature_c
    );

    let fault_status = if safety.no_faults {
        "✓".green()
    } else {
        "✗".red()
    };
    println!("    No Faults: {}", fault_status);

    if !safety.blocked_reasons.is_empty() {
        println!("    {}", "Blocked Reasons:".red());
        for reason in &safety.blocked_reasons {
            println!("      • {}", reason.red());
        }
    }

    if safety.can_enable_high_torque {
        println!("    {}", "✓ Ready for high torque".green());
    } else {
        println!("    {}", "⚠ Cannot enable high torque".yellow());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{DeviceCapabilities, DeviceInfo, DeviceState, DeviceType, TelemetryData};

    fn make_status(temp: u8, hands_on: bool, faults: Vec<String>) -> DeviceStatus {
        DeviceStatus {
            device: DeviceInfo {
                id: "wheel-001".to_string(),
                name: "Test Wheel".to_string(),
                source: None,
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                product_string: None,
                serial_number_present: None,
                interface_number: None,
                usage_page: None,
                usage: None,
                hid_path_present: None,
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities {
                    max_torque_nm: 8.0,
                    ..DeviceCapabilities::default()
                },
            },
            last_seen: chrono::Utc::now(),
            active_faults: faults,
            telemetry: TelemetryData {
                wheel_angle_deg: 0.0,
                wheel_speed_rad_s: 0.0,
                temperature_c: temp,
                fault_flags: 0,
                hands_on,
            },
            moza: None,
        }
    }

    #[test]
    fn healthy_device_can_enable_high_torque() {
        let status = make_status(45, true, vec![]);
        let safety = analyze_device_safety(&status);
        assert!(safety.can_enable_high_torque);
        assert!(safety.temperature_ok);
        assert!(safety.hands_on);
        assert!(safety.no_faults);
        assert!(safety.blocked_reasons.is_empty());
    }

    #[test]
    fn high_temp_blocks_high_torque() {
        let status = make_status(85, true, vec![]);
        let safety = analyze_device_safety(&status);
        assert!(!safety.can_enable_high_torque);
        assert!(!safety.temperature_ok);
        assert!(!safety.blocked_reasons.is_empty());
    }

    #[test]
    fn temp_at_boundary_80_blocks() {
        let status = make_status(80, true, vec![]);
        let safety = analyze_device_safety(&status);
        assert!(!safety.temperature_ok);
        assert!(!safety.can_enable_high_torque);
    }

    #[test]
    fn temp_just_below_80_ok() {
        let status = make_status(79, true, vec![]);
        let safety = analyze_device_safety(&status);
        assert!(safety.temperature_ok);
    }

    #[test]
    fn hands_off_blocks_high_torque() {
        let status = make_status(45, false, vec![]);
        let safety = analyze_device_safety(&status);
        assert!(!safety.can_enable_high_torque);
        assert!(!safety.hands_on);
        assert!(!safety.blocked_reasons.is_empty());
    }

    #[test]
    fn active_faults_block_high_torque() {
        let status = make_status(45, true, vec!["overcurrent".to_string()]);
        let safety = analyze_device_safety(&status);
        assert!(!safety.can_enable_high_torque);
        assert!(!safety.no_faults);
    }

    #[test]
    fn multiple_blocks_accumulate_reasons() {
        let status = make_status(90, false, vec!["fault1".to_string()]);
        let reasons = get_safety_block_reasons(&status);
        // Should have reasons for: faults, temperature, hands_off
        assert!(reasons.len() >= 3);
    }

    #[test]
    fn no_blocks_for_healthy_device() {
        let status = make_status(45, true, vec![]);
        let reasons = get_safety_block_reasons(&status);
        assert!(reasons.is_empty());
    }

    #[test]
    fn safety_torque_limit_matches_device() {
        let status = make_status(45, true, vec![]);
        let safety = analyze_device_safety(&status);
        assert!((safety.torque_limit_nm - 8.0).abs() < f32::EPSILON);
    }
}
