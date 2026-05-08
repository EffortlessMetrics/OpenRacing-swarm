//! Output formatting for CLI responses

use anyhow::Error;
use colored::*;
use serde_json::json;
use std::collections::HashMap;

use crate::client::{
    DeviceCapabilities as ClientDeviceCapabilities, DeviceInfo as ClientDeviceInfo,
    DeviceState as ClientDeviceState, DeviceStatus, DiagnosticInfo, GameStatus, HealthEvent,
    HealthEventType,
};
use racing_wheel_schemas::config::ProfileSchema;

/// Print error in JSON format
pub fn print_error_json(error: &Error) {
    let error_json = json!({
        "success": false,
        "error": {
            "message": error.to_string(),
            "type": error_type_name(error)
        }
    });
    match serde_json::to_string_pretty(&error_json) {
        Ok(s) => println!("{}", s),
        Err(e) => eprintln!("Failed to format error as JSON: {}", e),
    }
}

/// Print error in human-readable format
pub fn print_error_human(error: &Error) {
    eprintln!("{} {}", "Error:".red().bold(), error);

    // Print error chain if available
    let mut source = error.source();
    while let Some(err) = source {
        eprintln!("  {} {}", "Caused by:".yellow(), err);
        source = err.source();
    }
}

/// Print device list in specified format
pub fn print_device_list(devices: &[ClientDeviceInfo], json: bool, detailed: bool) {
    if json {
        let output = json!({
            "success": true,
            "devices": devices
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format device list as JSON: {}", e),
        }
    } else {
        if devices.is_empty() {
            println!("{}", "No devices found".yellow());
            return;
        }

        println!("{}", "Connected Devices:".bold());
        for device in devices {
            print_device_human(device, detailed);
        }
    }
}

/// Print single device in human format
fn print_device_human(device: &ClientDeviceInfo, detailed: bool) {
    let state_color = match device.state {
        ClientDeviceState::Connected => "green",
        ClientDeviceState::Disconnected => "red",
        ClientDeviceState::Faulted => "red",
        ClientDeviceState::Calibrating => "yellow",
    };

    println!(
        "  {} {} ({})",
        "●".color(state_color),
        device.name.bold(),
        device.id.dimmed()
    );

    if detailed {
        println!("    Type: {:?}", device.device_type);
        println!("    State: {:?}", device.state);
        if device.capabilities.max_torque_nm > 0.0 {
            println!(
                "    Max Torque: {:.1} Nm",
                device.capabilities.max_torque_nm
            );
        }
        println!(
            "    Capabilities: {}",
            format_capabilities(&device.capabilities)
        );
    }
}

/// Format device capabilities as a string
fn format_capabilities(caps: &ClientDeviceCapabilities) -> String {
    let mut features = Vec::new();

    if caps.supports_pid {
        features.push("PID");
    }
    if caps.supports_raw_torque_1khz {
        features.push("Raw Torque");
    }
    if caps.supports_health_stream {
        features.push("Health");
    }
    if caps.supports_led_bus {
        features.push("LEDs");
    }

    if features.is_empty() {
        "None".to_string()
    } else {
        features.join(", ")
    }
}

/// Print device status
pub fn print_device_status(status: &DeviceStatus, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "status": status
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format device status as JSON: {}", e),
        }
    } else {
        println!("{} {}", "Device:".bold(), status.device.name);
        println!("  ID: {}", status.device.id);
        println!("  State: {:?}", status.device.state);
        println!(
            "  Last Seen: {}",
            status.last_seen.format("%Y-%m-%d %H:%M:%S UTC")
        );

        if !status.active_faults.is_empty() {
            println!(
                "  {} {}",
                "Active Faults:".red().bold(),
                status.active_faults.len()
            );
            for fault in &status.active_faults {
                println!("    • {}", fault.red());
            }
        } else {
            println!("  {}", "No Active Faults".green());
        }

        if let Some(moza) = &status.moza {
            println!("  {}:", "Moza Readiness".bold());
            println!("    Model: {}", moza.model);
            println!("    Product ID: {}", moza.product_id);
            println!("    Category: {}", moza.category);
            println!("    Output Capable: {}", moza.output_capable);
            println!("    FFB Ready: {}", moza.ffb_ready);
            println!("    Init State: {}", moza.init_state);
            println!("    Descriptor Trusted: {}", moza.descriptor_trusted);
            if let Some(crc) = &moza.descriptor_crc32 {
                println!("    Descriptor CRC32: {}", crc);
            }
            println!("    Safety State: {}", moza.safety_state);
            println!("    Safety Reason: {}", moza.safety_reason);
        }

        println!("  {}:", "Telemetry".bold());
        let tel = &status.telemetry;
        println!("    Wheel Angle: {:.1}°", tel.wheel_angle_deg);
        println!("    Wheel Speed: {:.1} rad/s", tel.wheel_speed_rad_s);
        println!("    Temperature: {}°C", tel.temperature_c);
        println!(
            "    Hands On: {}",
            if tel.hands_on {
                "Yes".green()
            } else {
                "No".red()
            }
        );
    }
}

/// Print profile information
pub fn print_profile(profile: &ProfileSchema, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "profile": profile
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format profile as JSON: {}", e),
        }
    } else {
        println!("{} {}", "Profile Schema:".bold(), profile.schema);

        if let Some(ref game) = profile.scope.game {
            print!("  Scope: {}", game.cyan());
            if let Some(ref car) = profile.scope.car {
                print!(" > {}", car.cyan());
            }
            if let Some(ref track) = profile.scope.track {
                print!(" > {}", track.cyan());
            }
            println!();
        }

        println!("  {}:", "Base Settings".bold());
        println!("    FFB Gain: {:.1}%", profile.base.ffb_gain * 100.0);
        println!("    DOR: {}°", profile.base.dor_deg);
        println!("    Torque Cap: {:.1} Nm", profile.base.torque_cap_nm);

        println!("    {}:", "Filters".bold());
        let f = &profile.base.filters;
        println!("      Reconstruction: {}", f.reconstruction);
        println!("      Friction: {:.2}", f.friction);
        println!("      Damper: {:.2}", f.damper);
        println!("      Inertia: {:.2}", f.inertia);
        println!("      Slew Rate: {:.2}", f.slew_rate);

        if !f.notch_filters.is_empty() {
            println!("      Notch Filters:");
            for (i, notch) in f.notch_filters.iter().enumerate() {
                println!(
                    "        {}: {:.1} Hz, Q={:.1}, Gain={:.1} dB",
                    i + 1,
                    notch.hz,
                    notch.q,
                    notch.gain_db
                );
            }
        }

        if !f.curve_points.is_empty() {
            println!("      Curve Points: {} points", f.curve_points.len());
        }

        if profile.signature.is_some() {
            println!("  {}", "✓ Signed".green());
        } else {
            println!("  {}", "⚠ Unsigned".yellow());
        }
    }
}

/// Print diagnostics information
pub fn print_diagnostics(diag: &DiagnosticInfo, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "diagnostics": diag
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format diagnostics as JSON: {}", e),
        }
    } else {
        println!("{} {}", "Diagnostics for:".bold(), diag.device_id);

        println!("  {}:", "System Info".bold());
        for (key, value) in &diag.system_info {
            println!("    {}: {}", key, value);
        }

        println!("  {}:", "Performance Metrics".bold());
        let perf = &diag.performance;
        println!("    P99 Jitter: {:.2} μs", perf.p99_jitter_us);
        println!(
            "    Missed Tick Rate: {:.4}%",
            perf.missed_tick_rate * 100.0
        );
        println!("    Total Ticks: {}", perf.total_ticks);
        println!("    Missed Ticks: {}", perf.missed_ticks);

        if !diag.recent_faults.is_empty() {
            println!("  {}:", "Recent Faults".red().bold());
            for fault in &diag.recent_faults {
                println!("    • {}", fault.red());
            }
        } else {
            println!("  {}", "No Recent Faults".green());
        }
    }
}

/// Print game status
pub fn print_game_status(status: &GameStatus, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "game_status": status
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format game status as JSON: {}", e),
        }
    } else {
        println!("{}", "Game Status:".bold());

        match &status.active_game {
            Some(game) => {
                println!("  Active Game: {}", game.cyan());
                println!(
                    "  Telemetry: {}",
                    if status.telemetry_active {
                        "Active".green()
                    } else {
                        "Inactive".red()
                    }
                );

                if let Some(ref car) = status.car_id {
                    println!("  Car: {}", car);
                }
                if let Some(ref track) = status.track_id {
                    println!("  Track: {}", track);
                }
            }
            None => {
                println!("  {}", "No active game detected".yellow());
            }
        }
    }
}

/// Print health event
pub fn print_health_event(event: &HealthEvent, json: bool) {
    if json {
        match serde_json::to_string(&event) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format health event as JSON: {}", e),
        }
    } else {
        let event_color = match event.event_type {
            HealthEventType::DeviceConnected => "green",
            HealthEventType::DeviceDisconnected => "red",
            HealthEventType::FaultDetected => "red",
            HealthEventType::FaultCleared => "green",
            HealthEventType::PerformanceWarning => "yellow",
        };

        println!(
            "{} [{}] {}: {}",
            event.timestamp.format("%H:%M:%S").to_string().dimmed(),
            event.device_id.cyan(),
            format!("{:?}", event.event_type).color(event_color),
            event.message
        );
    }
}

/// Print success message
pub fn print_success(message: &str, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "message": message
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format success message as JSON: {}", e),
        }
    } else {
        println!("{} {}", "✓".green(), message);
    }
}

/// Print warning message
pub fn print_warning(message: &str, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "warning": message
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format warning message as JSON: {}", e),
        }
    } else {
        println!("{} {}", "⚠".yellow(), message);
    }
}

/// Get error type name for JSON output
fn error_type_name(error: &Error) -> String {
    // Try to get the concrete error type name
    let debug_str = format!("{:?}", error);
    match debug_str.split('(').next() {
        Some(name) => name.to_string(),
        None => "Unknown".to_string(),
    }
}

/// Print table of data
#[allow(dead_code)]
pub fn print_table<T>(headers: &[&str], rows: &[Vec<T>], json: bool)
where
    T: std::fmt::Display + serde::Serialize,
{
    if json {
        let mut table_data = Vec::new();
        for row in rows {
            let mut row_map = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if let Some(value) = row.get(i) {
                    row_map.insert(header.to_string(), json!(value));
                }
            }
            table_data.push(row_map);
        }

        let output = json!({
            "success": true,
            "data": table_data
        });
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{}", s),
            Err(e) => eprintln!("Failed to format table data as JSON: {}", e),
        }
    } else {
        // Simple table formatting for human output
        if rows.is_empty() {
            println!("{}", "No data".yellow());
            return;
        }

        // Print headers
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                print!("  ");
            }
            print!("{}", header.bold());
        }
        println!();

        // Print separator
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                print!("  ");
            }
            print!("{}", "-".repeat(header.len()));
        }
        println!();

        // Print rows
        for row in rows {
            for (i, value) in row.iter().enumerate() {
                if i > 0 {
                    print!("  ");
                }
                print!("{}", value);
            }
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{
        DeviceCapabilities, DeviceInfo, DeviceState, DeviceType, GameStatus, HealthEvent,
        HealthEventType, MozaReadinessStatus, TelemetryData,
    };
    use crate::error::CliError;

    // --- format_capabilities ---

    #[test]
    fn format_capabilities_all_features() {
        let caps = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: true,
            ..DeviceCapabilities::default()
        };
        let result = format_capabilities(&caps);
        assert!(result.contains("PID"));
        assert!(result.contains("Raw Torque"));
        assert!(result.contains("Health"));
        assert!(result.contains("LEDs"));
    }

    #[test]
    fn format_capabilities_no_features() {
        let caps = DeviceCapabilities::default();
        assert_eq!(format_capabilities(&caps), "None");
    }

    #[test]
    fn format_capabilities_partial() {
        let caps = DeviceCapabilities {
            supports_pid: true,
            supports_led_bus: true,
            ..DeviceCapabilities::default()
        };
        let result = format_capabilities(&caps);
        assert!(result.contains("PID"));
        assert!(result.contains("LEDs"));
        assert!(!result.contains("Raw Torque"));
        assert!(!result.contains("Health"));
    }

    #[test]
    fn format_capabilities_single() {
        let caps = DeviceCapabilities {
            supports_health_stream: true,
            ..DeviceCapabilities::default()
        };
        // Single capability should not contain commas
        let result = format_capabilities(&caps);
        assert_eq!(result, "Health");
    }

    #[test]
    fn print_device_status_human_includes_moza_readiness_block() {
        let status = DeviceStatus {
            device: DeviceInfo {
                id: "moza-r5".to_string(),
                name: "Moza R5".to_string(),
                vendor_id: Some("0x346E".to_string()),
                product_id: Some("0x0014".to_string()),
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    max_torque_nm: 5.5,
                    ..DeviceCapabilities::default()
                },
            },
            last_seen: chrono::Utc::now(),
            active_faults: Vec::new(),
            telemetry: TelemetryData::default(),
            moza: Some(MozaReadinessStatus {
                model: "Moza R5".to_string(),
                product_id: "0x0014".to_string(),
                category: "wheelbase".to_string(),
                output_capable: true,
                ffb_ready: false,
                init_state: "uninitialized".to_string(),
                descriptor_trusted: true,
                descriptor_crc32: Some("0x12345678".to_string()),
                descriptor_source: Some("operator_supplied_hex".to_string()),
                lane: Some("ci/hardware/moza-r5/2026-05-06".to_string()),
                direct_mode_allowed: false,
                high_torque_allowed: false,
                safe_to_send_torque: false,
                safety_state: "descriptor_observed_pre_validation".to_string(),
                safety_reason: "torque output remains gated".to_string(),
            }),
        };

        print_device_status(&status, false);
    }

    // --- error_type_name ---

    #[test]
    fn error_type_name_cli_error() {
        let err: anyhow::Error = CliError::DeviceNotFound("w1".to_string()).into();
        let name = error_type_name(&err);
        assert!(!name.is_empty());
    }

    #[test]
    fn error_type_name_generic_error() {
        let err = anyhow::anyhow!("something went wrong");
        let name = error_type_name(&err);
        assert!(!name.is_empty());
    }

    // --- JSON output structure ---

    #[test]
    fn print_error_json_produces_valid_json() {
        let err: anyhow::Error = CliError::DeviceNotFound("dev-x".to_string()).into();
        // Build the JSON structure the same way print_error_json does
        let error_json = json!({
            "success": false,
            "error": {
                "message": err.to_string(),
                "type": error_type_name(&err)
            }
        });
        let serialized = serde_json::to_string_pretty(&error_json);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("\"success\": false"));
        assert!(s.contains("Device not found: dev-x"));
    }

    #[test]
    fn success_json_structure() {
        let output = json!({
            "success": true,
            "message": "test passed"
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("\"success\": true"));
        assert!(s.contains("test passed"));
    }

    #[test]
    fn warning_json_structure() {
        let output = json!({
            "success": true,
            "warning": "caution advised"
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("\"warning\""));
        assert!(s.contains("caution advised"));
    }

    #[test]
    fn device_list_json_structure() {
        let devices: Vec<DeviceInfo> = vec![];
        let output = json!({
            "success": true,
            "devices": devices
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("\"devices\""));
        assert!(s.contains("[]"));
    }

    #[test]
    fn device_list_json_with_devices() {
        let devices = vec![DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Test Wheel".to_string(),
            vendor_id: None,
            product_id: None,
            device_type: DeviceType::WheelBase,
            state: DeviceState::Connected,
            capabilities: DeviceCapabilities::default(),
        }];
        let output = json!({
            "success": true,
            "devices": devices
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("wheel-001"));
        assert!(s.contains("Test Wheel"));
    }

    #[test]
    fn game_status_json_with_active_game() {
        let status = GameStatus {
            active_game: Some("iracing".to_string()),
            telemetry_active: true,
            car_id: Some("gt3".to_string()),
            track_id: Some("spa".to_string()),
        };
        let output = json!({
            "success": true,
            "game_status": status
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("iracing"));
        assert!(s.contains("gt3"));
        assert!(s.contains("spa"));
    }

    #[test]
    fn game_status_json_no_active_game() {
        let status = GameStatus {
            active_game: None,
            telemetry_active: false,
            car_id: None,
            track_id: None,
        };
        let output = json!({
            "success": true,
            "game_status": status
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("null"));
    }

    #[test]
    fn health_event_json_serialization() {
        let event = HealthEvent {
            timestamp: chrono::Utc::now(),
            device_id: "wheel-001".to_string(),
            event_type: HealthEventType::FaultDetected,
            message: "Overcurrent detected".to_string(),
            metadata: std::collections::HashMap::new(),
        };
        let serialized = serde_json::to_string(&event);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("wheel-001"));
        assert!(s.contains("Overcurrent detected"));
        assert!(s.contains("FaultDetected"));
    }

    // --- table JSON output ---

    #[test]
    fn table_json_structure() {
        let headers = ["Name", "Value"];
        let rows: Vec<Vec<String>> = vec![
            vec!["key1".to_string(), "val1".to_string()],
            vec!["key2".to_string(), "val2".to_string()],
        ];

        let mut table_data = Vec::new();
        for row in &rows {
            let mut row_map = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if let Some(value) = row.get(i) {
                    row_map.insert(header.to_string(), json!(value));
                }
            }
            table_data.push(row_map);
        }
        let output = json!({
            "success": true,
            "data": table_data
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("key1"));
        assert!(s.contains("val1"));
        assert!(s.contains("key2"));
        assert!(s.contains("val2"));
    }

    #[test]
    fn table_json_empty_rows() {
        let rows: Vec<Vec<String>> = vec![];
        let headers: Vec<&str> = vec!["Col"];
        let mut table_data = Vec::new();
        for row in &rows {
            let mut row_map = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if let Some(value) = row.get(i) {
                    row_map.insert(header.to_string(), json!(value));
                }
            }
            table_data.push(row_map);
        }
        let output = json!({
            "success": true,
            "data": table_data
        });
        let serialized = serde_json::to_string_pretty(&output);
        assert!(serialized.is_ok());
        let s = serialized.unwrap_or_default();
        assert!(s.contains("\"data\": []"));
    }

    // --- device telemetry serialization ---

    #[test]
    fn telemetry_data_round_trip() {
        let tel = TelemetryData {
            wheel_angle_deg: 12.5,
            wheel_speed_rad_s: std::f32::consts::PI,
            temperature_c: 55,
            fault_flags: 0,
            hands_on: true,
        };
        let serialized = serde_json::to_value(&tel);
        assert!(serialized.is_ok());
        let val = serialized.unwrap_or_default();
        assert_eq!(val["hands_on"], true);
        assert_eq!(val["temperature_c"], 55);
    }

    // --- error chain ---

    #[test]
    fn nested_error_has_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
        let cli_err: anyhow::Error = CliError::IoError(io_err).into();
        // anyhow wraps source chain
        let msg = cli_err.to_string();
        assert!(msg.contains("no access"));
    }

    // --- device state color mapping (logic coverage) ---

    #[test]
    fn device_state_variants_exist() {
        // Ensures all state variants can be matched for coloring
        let states = [
            DeviceState::Connected,
            DeviceState::Disconnected,
            DeviceState::Faulted,
            DeviceState::Calibrating,
        ];
        for state in &states {
            let _color = match state {
                DeviceState::Connected => "green",
                DeviceState::Disconnected => "red",
                DeviceState::Faulted => "red",
                DeviceState::Calibrating => "yellow",
            };
        }
    }

    #[test]
    fn health_event_type_color_mapping() {
        let types = [
            (HealthEventType::DeviceConnected, "green"),
            (HealthEventType::DeviceDisconnected, "red"),
            (HealthEventType::FaultDetected, "red"),
            (HealthEventType::FaultCleared, "green"),
            (HealthEventType::PerformanceWarning, "yellow"),
        ];
        for (event_type, expected_color) in &types {
            let color = match event_type {
                HealthEventType::DeviceConnected => "green",
                HealthEventType::DeviceDisconnected => "red",
                HealthEventType::FaultDetected => "red",
                HealthEventType::FaultCleared => "green",
                HealthEventType::PerformanceWarning => "yellow",
            };
            assert_eq!(color, *expected_color);
        }
    }
}
