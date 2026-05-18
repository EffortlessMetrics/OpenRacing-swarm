//! wheelctl - Racing Wheel Control CLI
//!
//! A comprehensive command-line interface for managing racing wheel hardware,
//! profiles, diagnostics, and game integration.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

mod client;
mod commands;
mod completion;
mod error;
mod output;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::commands::*;
use crate::error::CliError;

#[derive(Parser)]
#[command(name = "wheelctl")]
#[command(
    about = "Racing Wheel Control CLI - Manage racing wheel hardware, profiles, and diagnostics"
)]
#[command(version)]
#[command(long_about = "
wheelctl is a command-line interface for the Racing Wheel Software Suite.
It provides comprehensive control over racing wheel hardware, profile management,
diagnostics, and game integration features.

All write operations available in the UI are also available through this CLI.
Use --json flag for machine-readable output suitable for scripting.
")]
struct Cli {
    /// Output format (human-readable or JSON)
    #[arg(
        long,
        global = true,
        help = "Output in JSON format for machine parsing"
    )]
    json: bool,

    /// Verbose logging
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Service endpoint (for testing)
    #[arg(long, global = true, env = "WHEELCTL_ENDPOINT", hide = true)]
    endpoint: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Device management commands
    #[command(subcommand)]
    Device(DeviceCommands),

    /// Profile management commands  
    #[command(subcommand)]
    Profile(ProfileCommands),

    /// Plugin management commands
    #[command(subcommand)]
    Plugin(PluginCommands),

    /// Diagnostic and monitoring commands
    #[command(subcommand)]
    Diag(DiagCommands),

    /// Game integration commands
    #[command(subcommand)]
    Game(GameCommands),

    /// Telemetry probe and capture commands
    #[command(subcommand)]
    Telemetry(TelemetryCommands),

    /// Hardware environment diagnostics
    #[command(subcommand)]
    Hardware(HardwareCommands),

    /// Safe Moza HID probe and capture commands
    #[command(subcommand)]
    Moza(MozaCommands),

    /// Safety and control commands
    #[command(subcommand)]
    Safety(SafetyCommands),

    /// Generate diagnostic support bundle
    SupportBundle(SupportBundleArgs),

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Service health and status
    Health {
        /// Watch health events in real-time
        #[arg(short, long)]
        watch: bool,
    },
}

#[derive(Args)]
struct SupportBundleArgs {
    /// Limit status snapshots to this device ID or name
    #[arg(long)]
    device: Option<String>,
    /// Include blackbox recording
    #[arg(short, long)]
    blackbox: bool,
    /// Include Moza lane receipt verification summaries from this directory
    #[arg(long)]
    moza_lane: Option<String>,
    /// Output file path
    #[arg(short, long)]
    output: Option<String>,
}

fn main() -> Result<()> {
    let handle = std::thread::Builder::new()
        .name("wheelctl-main".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("failed to build wheelctl async runtime")?;
            runtime.block_on(async_main())
        })
        .context("failed to start wheelctl main thread")?;

    match handle.join() {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("wheelctl={}", log_level).into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_writer(std::io::stderr),
        )
        .init();

    // Execute command
    let result = execute_command(&cli).await;

    // Handle errors with appropriate exit codes
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let receipt_failure_already_printed = cli.json
                && e.downcast_ref::<CliError>()
                    .is_some_and(|err| matches!(err, CliError::ReceiptFailure(_)));
            if cli.json {
                if !receipt_failure_already_printed {
                    output::print_error_json(&e);
                }
            } else {
                output::print_error_human(&e);
            }

            // Set appropriate exit code
            let exit_code = match e.downcast_ref::<CliError>() {
                Some(CliError::DeviceNotFound(_)) => 2,
                Some(CliError::ProfileNotFound(_)) => 3,
                Some(CliError::ValidationError(_))
                | Some(CliError::JsonError(_))
                | Some(CliError::SchemaError(_))
                | Some(CliError::ReceiptFailure(_)) => 4,
                Some(CliError::ServiceUnavailable(_)) => 5,
                Some(CliError::PermissionDenied(_)) => 6,
                _ => 1,
            };

            std::process::exit(exit_code);
        }
    }
}

async fn execute_command(cli: &Cli) -> Result<()> {
    match &cli.command {
        Commands::Device(cmd) => {
            commands::device::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Profile(cmd) => {
            commands::profile::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Plugin(cmd) => {
            commands::plugin::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Diag(cmd) => {
            commands::diag::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Game(cmd) => {
            commands::game::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Telemetry(cmd) => commands::telemetry::execute(cmd, cli.json).await,
        Commands::Hardware(cmd) => commands::hardware::execute(cmd, cli.json).await,
        Commands::Moza(cmd) => commands::moza::execute(cmd, cli.json).await,
        Commands::Safety(cmd) => {
            commands::safety::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::SupportBundle(args) => {
            let client = client::WheelClient::connect_or_mock(cli.endpoint.as_deref()).await?;
            commands::diag::generate_support_bundle(
                &client,
                "wheelctl support-bundle",
                args.blackbox,
                args.device.as_deref(),
                args.moza_lane.as_deref(),
                args.output.as_deref(),
                cli.json,
            )
            .await
        }
        Commands::Completion { shell } => {
            completion::generate_completion(*shell);
            Ok(())
        }
        Commands::Health { watch } => {
            commands::health::execute(*watch, cli.json, cli.endpoint.as_deref()).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn parse_cli<I, T>(args: I) -> Result<Cli, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = T> + Send + 'static,
        T: Into<std::ffi::OsString> + Clone + Send + 'static,
    {
        let handle = std::thread::Builder::new()
            .name("wheelctl-parse-test".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || Cli::try_parse_from(args))?;
        let parsed = handle
            .join()
            .map_err(|_| std::io::Error::other("CLI parse thread panicked"))?;
        Ok(parsed?)
    }

    // --- Global flag parsing ---

    #[test]
    fn parse_device_list_defaults() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "list"])?;
        assert!(!cli.json);
        assert_eq!(cli.verbose, 0);
        assert!(cli.endpoint.is_none());
        assert!(matches!(
            cli.command,
            Commands::Device(DeviceCommands::List {
                detailed: false,
                hid_observe_only: false,
                json_out: None
            })
        ));
        Ok(())
    }

    #[test]
    fn parse_global_json_flag_before_subcommand() -> TestResult {
        let cli = parse_cli(["wheelctl", "--json", "device", "list"])?;
        assert!(cli.json);
        Ok(())
    }

    #[test]
    fn parse_global_json_flag_after_subcommand() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "list", "--json"])?;
        assert!(cli.json);
        Ok(())
    }

    #[test]
    fn parse_verbose_levels() -> TestResult {
        let cli0 = parse_cli(["wheelctl", "device", "list"])?;
        assert_eq!(cli0.verbose, 0);

        let cli1 = parse_cli(["wheelctl", "-v", "device", "list"])?;
        assert_eq!(cli1.verbose, 1);

        let cli2 = parse_cli(["wheelctl", "-vv", "device", "list"])?;
        assert_eq!(cli2.verbose, 2);

        let cli3 = parse_cli(["wheelctl", "-vvv", "device", "list"])?;
        assert_eq!(cli3.verbose, 3);
        Ok(())
    }

    #[test]
    fn parse_endpoint_flag() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "--endpoint",
            "http://localhost:5000",
            "device",
            "list",
        ])?;
        assert_eq!(cli.endpoint.as_deref(), Some("http://localhost:5000"));
        Ok(())
    }

    // --- Device command parsing ---

    #[test]
    fn parse_device_list_detailed() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "list", "--detailed"])?;
        assert!(matches!(
            cli.command,
            Commands::Device(DeviceCommands::List {
                detailed: true,
                hid_observe_only: false,
                json_out: None
            })
        ));
        Ok(())
    }

    #[test]
    fn parse_device_list_hid_observe_only() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "list", "--hid-observe-only"])?;
        assert!(matches!(
            cli.command,
            Commands::Device(DeviceCommands::List {
                detailed: false,
                hid_observe_only: true,
                json_out: None
            })
        ));
        Ok(())
    }

    #[test]
    fn parse_device_list_json_out() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "device",
            "list",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-06/device-list.json",
        ])?;
        match &cli.command {
            Commands::Device(DeviceCommands::List { json_out, .. }) => {
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-06/device-list.json")
                );
            }
            _ => return Err("expected Device List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_doctor_json_out() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "doctor",
            "--json-out",
            "target/hardware-doctor.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::Doctor { json_out }) => {
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/hardware-doctor.json")
                );
            }
            _ => return Err("expected Hardware Doctor command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_bringup_rail() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "bringup-rail",
            "--family",
            "moza-r5",
            "--json-out",
            "target/hardware-bringup-rail.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::BringupRail { family, json_out }) => {
                assert_eq!(family, "moza-r5");
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/hardware-bringup-rail.json")
                );
            }
            _ => return Err("expected Hardware BringupRail command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_sniff_plan() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "sniff-plan",
            "--family",
            "moza",
            "--scenario",
            "pit-house-open-idle",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--operator",
            "Steven",
            "--device-note",
            "R5 + KS, SR-P and HBP through base",
            "--capture-tool",
            "wireshark",
            "--capture-tool",
            "usbpcap",
            "--platform-hint",
            "windows",
            "--json-out",
            "target/sniff/pit-house-open-idle/sniff-plan.json",
            "--md-out",
            "target/sniff/pit-house-open-idle/sniff-plan.md",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::SniffPlan {
                family,
                scenario,
                lane,
                operator,
                device_note,
                capture_tools,
                platform_hint,
                json_out,
                md_out,
            }) => {
                assert_eq!(family, "moza");
                assert_eq!(*scenario, HardwareSniffScenario::PitHouseOpenIdle);
                assert_eq!(lane.to_str(), Some("ci/hardware/moza-r5/2026-05-13"));
                assert_eq!(operator, "Steven");
                assert_eq!(device_note, "R5 + KS, SR-P and HBP through base");
                assert_eq!(
                    capture_tools,
                    &vec![
                        HardwareSniffCaptureTool::Wireshark,
                        HardwareSniffCaptureTool::UsbPcap,
                    ]
                );
                assert_eq!(*platform_hint, Some(HardwareSniffPlatformHint::Windows));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/sniff-plan.json")
                );
                assert_eq!(
                    md_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/sniff-plan.md")
                );
            }
            _ => return Err("expected Hardware SniffPlan command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_sniff_receipt() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "sniff-receipt",
            "--plan",
            "target/sniff/pit-house-open-idle/sniff-plan.json",
            "--pcapng",
            "target/sniff/pit-house-open-idle/capture.pcapng",
            "--operator",
            "Steven",
            "--app",
            "Pit House",
            "--scenario",
            "pit-house-open-idle",
            "--device-note",
            "R5 + KS, SR-P and HBP through base",
            "--evidence",
            "Pit House opened and left idle for 30 seconds.",
            "--json-out",
            "target/sniff/pit-house-open-idle/sniff-receipt.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::SniffReceipt {
                plan,
                pcapng,
                operator,
                app,
                scenario,
                device_note,
                evidence,
                json_out,
            }) => {
                assert_eq!(
                    plan.to_str(),
                    Some("target/sniff/pit-house-open-idle/sniff-plan.json")
                );
                assert_eq!(
                    pcapng.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/capture.pcapng")
                );
                assert_eq!(operator.as_deref(), Some("Steven"));
                assert_eq!(app, "Pit House");
                assert_eq!(*scenario, Some(HardwareSniffScenario::PitHouseOpenIdle));
                assert_eq!(
                    device_note.as_deref(),
                    Some("R5 + KS, SR-P and HBP through base")
                );
                assert_eq!(evidence, "Pit House opened and left idle for 30 seconds.");
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/sniff-receipt.json")
                );
            }
            _ => return Err("expected Hardware SniffReceipt command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_sniff_summary() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "sniff-summary",
            "--pcapng",
            "target/sniff/pit-house-open-idle/capture.pcapng",
            "--vendor",
            "0x346E",
            "--product",
            "0x0014",
            "--interface",
            "2",
            "--include-payload-samples",
            "--max-samples-per-report",
            "2",
            "--json-out",
            "target/sniff/pit-house-open-idle/sniff-summary.json",
            "--md-out",
            "target/sniff/pit-house-open-idle/sniff-summary.md",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::SniffSummary {
                pcapng,
                vendor,
                product,
                interface,
                include_payload_samples,
                max_samples_per_report,
                json_out,
                md_out,
            }) => {
                assert_eq!(
                    pcapng.to_str(),
                    Some("target/sniff/pit-house-open-idle/capture.pcapng")
                );
                assert_eq!(vendor.as_deref(), Some("0x346E"));
                assert_eq!(product.as_deref(), Some("0x0014"));
                assert_eq!(*interface, Some(2));
                assert!(*include_payload_samples);
                assert_eq!(*max_samples_per_report, Some(2));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/sniff-summary.json")
                );
                assert_eq!(
                    md_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/sniff-summary.md")
                );
            }
            _ => return Err("expected Hardware SniffSummary command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_sniff_bundle() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "--json",
            "hardware",
            "sniff-bundle",
            "--plan",
            "target/sniff/pit-house-open-idle/sniff-plan.json",
            "--receipt",
            "target/sniff/pit-house-open-idle/sniff-receipt.json",
            "--summary",
            "target/sniff/pit-house-open-idle/sniff-summary.json",
            "--operator-notes",
            "target/sniff/pit-house-open-idle/operator-notes.md",
            "--include-pcapng",
            "target/sniff/pit-house-open-idle/capture.pcapng",
            "--out",
            "target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip",
        ])?;
        assert!(cli.json);
        match &cli.command {
            Commands::Hardware(HardwareCommands::SniffBundle {
                plan,
                receipt,
                summary,
                operator_notes,
                include_pcapng,
                out,
            }) => {
                assert_eq!(
                    plan.to_str(),
                    Some("target/sniff/pit-house-open-idle/sniff-plan.json")
                );
                assert_eq!(
                    receipt.to_str(),
                    Some("target/sniff/pit-house-open-idle/sniff-receipt.json")
                );
                assert_eq!(
                    summary.to_str(),
                    Some("target/sniff/pit-house-open-idle/sniff-summary.json")
                );
                assert_eq!(
                    operator_notes.to_str(),
                    Some("target/sniff/pit-house-open-idle/operator-notes.md")
                );
                assert_eq!(
                    include_pcapng.as_ref().and_then(|p| p.to_str()),
                    Some("target/sniff/pit-house-open-idle/capture.pcapng")
                );
                assert_eq!(
                    out.to_str(),
                    Some("target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip")
                );
            }
            _ => return Err("expected Hardware SniffBundle command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_lane_init() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "lane",
            "init",
            "--family",
            "moza-r5",
            "--topology",
            "wheelbase-hub",
            "--lane",
            "target/hardware-lane",
            "--json-out",
            "target/hardware-lane/lane-init.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::Lane(command)) => match command.as_ref() {
                HardwareLaneCommands::Init {
                    family,
                    topology,
                    lane,
                    json_out,
                    ..
                } => {
                    assert_eq!(family, "moza-r5");
                    assert_eq!(topology, "wheelbase-hub");
                    assert_eq!(lane.to_str(), Some("target/hardware-lane"));
                    assert_eq!(
                        json_out.as_ref().and_then(|p| p.to_str()),
                        Some("target/hardware-lane/lane-init.json")
                    );
                }
                _ => return Err("expected Hardware Lane Init command".into()),
            },
            _ => return Err("expected Hardware Lane Init command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_lane_init_role_overrides() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "lane",
            "init",
            "--family",
            "moza-r5",
            "--lane",
            "target/hardware-lane",
            "--required-role",
            "handbrake",
            "--required-role",
            "ks_controls",
            "--role-artifact",
            "ks_controls=captures/ks-controls.jsonl",
            "--role-endpoint",
            "ks_controls=hid-0x346E-0x0004-if2-0x0001-0x0004",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::Lane(command)) => match command.as_ref() {
                HardwareLaneCommands::Init {
                    required_roles,
                    role_artifacts,
                    role_endpoints,
                    ..
                } => {
                    assert_eq!(required_roles, &vec!["handbrake", "ks_controls"]);
                    assert_eq!(
                        role_artifacts,
                        &vec!["ks_controls=captures/ks-controls.jsonl"]
                    );
                    assert_eq!(
                        role_endpoints,
                        &vec!["ks_controls=hid-0x346E-0x0004-if2-0x0001-0x0004"]
                    );
                }
                _ => return Err("expected Hardware Lane Init command".into()),
            },
            _ => return Err("expected Hardware Lane Init command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_lane_status() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "lane",
            "status",
            "--lane",
            "target/hardware-lane",
            "--json-out",
            "target/hardware-lane/lane-status.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::Lane(command)) => match command.as_ref() {
                HardwareLaneCommands::Status { lane, json_out } => {
                    assert_eq!(lane.to_str(), Some("target/hardware-lane"));
                    assert_eq!(
                        json_out.as_ref().and_then(|p| p.to_str()),
                        Some("target/hardware-lane/lane-status.json")
                    );
                }
                _ => return Err("expected Hardware Lane Status command".into()),
            },
            _ => return Err("expected Hardware Lane Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_hardware_lane_set_role_endpoint() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "hardware",
            "lane",
            "set-role-endpoint",
            "--lane",
            "target/hardware-lane",
            "--role",
            "button_box",
            "--endpoint",
            "hid-0x1234-0x5678-if0-0x0001-0x0004",
            "--json-out",
            "target/hardware-lane/role-endpoint-button_box.json",
        ])?;
        match &cli.command {
            Commands::Hardware(HardwareCommands::Lane(command)) => match command.as_ref() {
                HardwareLaneCommands::SetRoleEndpoint {
                    lane,
                    role,
                    endpoint,
                    json_out,
                } => {
                    assert_eq!(lane.to_str(), Some("target/hardware-lane"));
                    assert_eq!(role, "button_box");
                    assert_eq!(endpoint, "hid-0x1234-0x5678-if0-0x0001-0x0004");
                    assert_eq!(
                        json_out.as_ref().and_then(|p| p.to_str()),
                        Some("target/hardware-lane/role-endpoint-button_box.json")
                    );
                }
                _ => return Err("expected Hardware Lane SetRoleEndpoint command".into()),
            },
            _ => return Err("expected Hardware Lane SetRoleEndpoint command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_status() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "status", "wheel-001"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status {
                device,
                moza_lane,
                json_out,
                watch,
            }) => {
                assert_eq!(device, "wheel-001");
                assert!(moza_lane.is_none());
                assert!(json_out.is_none());
                assert!(!watch);
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_status_moza_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "device",
            "status",
            "moza-r5",
            "--moza-lane",
            "ci/hardware/moza-r5/2026-05-06",
        ])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status {
                device,
                moza_lane,
                json_out,
                watch,
            }) => {
                assert_eq!(device, "moza-r5");
                assert_eq!(
                    moza_lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert!(json_out.is_none());
                assert!(!watch);
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_status_watch() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "status", "wheel-001", "--watch"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status { watch, .. }) => {
                assert!(watch);
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_status_json_out() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "device",
            "status",
            "moza-r5",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-06/device-status.json",
        ])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status { json_out, .. }) => {
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-06/device-status.json")
                );
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_calibrate() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "device",
            "calibrate",
            "wheel-001",
            "center",
            "--yes",
        ])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Calibrate {
                device,
                calibration_type,
                yes,
            }) => {
                assert_eq!(device, "wheel-001");
                assert!(matches!(calibration_type, CalibrationType::Center));
                assert!(yes);
            }
            _ => return Err("expected Device Calibrate command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_calibrate_all_types() -> TestResult {
        for (arg, expected) in [
            ("center", CalibrationType::Center),
            ("dor", CalibrationType::Dor),
            ("pedals", CalibrationType::Pedals),
            ("all", CalibrationType::All),
        ] {
            let cli = parse_cli(["wheelctl", "device", "calibrate", "w1", arg])?;
            match &cli.command {
                Commands::Device(DeviceCommands::Calibrate {
                    calibration_type, ..
                }) => {
                    assert_eq!(
                        std::mem::discriminant(calibration_type),
                        std::mem::discriminant(&expected)
                    );
                }
                _ => return Err("expected Device Calibrate command".into()),
            }
        }
        Ok(())
    }

    #[test]
    fn parse_device_reset_force() -> TestResult {
        let cli = parse_cli(["wheelctl", "device", "reset", "dev-001", "--force"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Reset { device, force }) => {
                assert_eq!(device, "dev-001");
                assert!(force);
            }
            _ => return Err("expected Device Reset command".into()),
        }
        Ok(())
    }

    // --- Profile command parsing ---

    #[test]
    fn parse_profile_list_with_filters() -> TestResult {
        let cli = parse_cli([
            "wheelctl", "profile", "list", "--game", "iracing", "--car", "gt3",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::List { game, car }) => {
                assert_eq!(game.as_deref(), Some("iracing"));
                assert_eq!(car.as_deref(), Some("gt3"));
            }
            _ => return Err("expected Profile List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_list_no_filters() -> TestResult {
        let cli = parse_cli(["wheelctl", "profile", "list"])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::List { game, car }) => {
                assert!(game.is_none());
                assert!(car.is_none());
            }
            _ => return Err("expected Profile List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_apply_with_skip_validation() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "profile",
            "apply",
            "dev-001",
            "my_profile.json",
            "--skip-validation",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Apply {
                device,
                profile,
                skip_validation,
            }) => {
                assert_eq!(device, "dev-001");
                assert_eq!(profile, "my_profile.json");
                assert!(skip_validation);
            }
            _ => return Err("expected Profile Apply command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_create_with_options() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "profile",
            "create",
            "out.json",
            "--from",
            "base.json",
            "--game",
            "acc",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Create {
                path,
                from,
                game,
                car,
            }) => {
                assert_eq!(path, "out.json");
                assert_eq!(from.as_deref(), Some("base.json"));
                assert_eq!(game.as_deref(), Some("acc"));
                assert!(car.is_none());
            }
            _ => return Err("expected Profile Create command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_edit_with_field_value() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "profile",
            "edit",
            "p.json",
            "--field",
            "base.ffbGain",
            "--value",
            "0.9",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Edit {
                profile,
                field,
                value,
            }) => {
                assert_eq!(profile, "p.json");
                assert_eq!(field.as_deref(), Some("base.ffbGain"));
                assert_eq!(value.as_deref(), Some("0.9"));
            }
            _ => return Err("expected Profile Edit command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_validate() -> TestResult {
        let cli = parse_cli(["wheelctl", "profile", "validate", "test.json", "--detailed"])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Validate { path, detailed }) => {
                assert_eq!(path, "test.json");
                assert!(detailed);
            }
            _ => return Err("expected Profile Validate command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_export_signed() -> TestResult {
        let cli = parse_cli([
            "wheelctl", "profile", "export", "p.json", "--output", "out.json", "--signed",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Export {
                profile,
                output,
                signed,
            }) => {
                assert_eq!(profile, "p.json");
                assert_eq!(output.as_deref(), Some("out.json"));
                assert!(signed);
            }
            _ => return Err("expected Profile Export command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_import_with_verify() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "profile",
            "import",
            "in.json",
            "--target",
            "dest.json",
            "--verify",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Import {
                path,
                target,
                verify,
            }) => {
                assert_eq!(path, "in.json");
                assert_eq!(target.as_deref(), Some("dest.json"));
                assert!(verify);
            }
            _ => return Err("expected Profile Import command".into()),
        }
        Ok(())
    }

    // --- Plugin command parsing ---

    #[test]
    fn parse_plugin_install_with_version() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "plugin",
            "install",
            "ffb-smoothing",
            "--version",
            "1.2.0",
        ])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Install { plugin_id, version }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
                assert_eq!(version.as_deref(), Some("1.2.0"));
            }
            _ => return Err("expected Plugin Install command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_search() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "search", "smoothing"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Search { query }) => {
                assert_eq!(query, "smoothing");
            }
            _ => return Err("expected Plugin Search command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_uninstall_force() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "uninstall", "my-plugin", "--force"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Uninstall { plugin_id, force }) => {
                assert_eq!(plugin_id, "my-plugin");
                assert!(force);
            }
            _ => return Err("expected Plugin Uninstall command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_verify() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "verify", "ffb-smoothing"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Verify { plugin_id }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
            }
            _ => return Err("expected Plugin Verify command".into()),
        }
        Ok(())
    }

    // --- Safety command parsing ---

    #[test]
    fn parse_safety_limit_global() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "safety",
            "limit",
            "wheel-001",
            "5.5",
            "--global",
        ])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Limit {
                device,
                torque,
                global,
            }) => {
                assert_eq!(device, "wheel-001");
                assert!((torque - 5.5).abs() < f32::EPSILON);
                assert!(global);
            }
            _ => return Err("expected Safety Limit command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_stop_all() -> TestResult {
        let cli = parse_cli(["wheelctl", "safety", "stop"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Stop { device }) => {
                assert!(device.is_none());
            }
            _ => return Err("expected Safety Stop command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_stop_specific_device() -> TestResult {
        let cli = parse_cli(["wheelctl", "safety", "stop", "wheel-001"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Stop { device }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
            }
            _ => return Err("expected Safety Stop command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_enable_force() -> TestResult {
        let cli = parse_cli(["wheelctl", "safety", "enable", "wheel-001", "--force"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Enable { device, force }) => {
                assert_eq!(device, "wheel-001");
                assert!(force);
            }
            _ => return Err("expected Safety Enable command".into()),
        }
        Ok(())
    }

    // --- Diag command parsing ---

    #[test]
    fn parse_diag_record_with_defaults() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "record", "wheel-001"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Record {
                device, duration, ..
            }) => {
                assert_eq!(device, "wheel-001");
                assert_eq!(*duration, 120);
            }
            _ => return Err("expected Diag Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_record_custom_duration() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "60",
            "--output",
            "test.wbb",
        ])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Record {
                device,
                duration,
                output,
            }) => {
                assert_eq!(device, "wheel-001");
                assert_eq!(*duration, 60);
                assert_eq!(output.as_deref(), Some("test.wbb"));
            }
            _ => return Err("expected Diag Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_test_specific_type() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "test", "--device", "wheel-001", "motor"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Test { device, test_type }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
                assert!(matches!(test_type, Some(TestType::Motor)));
            }
            _ => return Err("expected Diag Test command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_metrics_watch() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "metrics", "--watch"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Metrics { watch, .. }) => {
                assert!(watch);
            }
            _ => return Err("expected Diag Metrics command".into()),
        }
        Ok(())
    }

    // --- Telemetry command parsing ---

    #[test]
    fn parse_telemetry_probe() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "probe",
            "--game",
            "acc",
            "--endpoint",
            "127.0.0.1:9001",
            "--timeout-ms",
            "200",
            "--attempts",
            "5",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Probe {
                game,
                endpoint,
                timeout_ms,
                attempts,
            }) => {
                assert_eq!(game, "acc");
                assert_eq!(endpoint, "127.0.0.1:9001");
                assert_eq!(*timeout_ms, 200);
                assert_eq!(*attempts, 5);
            }
            _ => return Err("expected Telemetry Probe command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_probe_defaults() -> TestResult {
        let cli = parse_cli(["wheelctl", "telemetry", "probe", "--game", "acc"])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Probe {
                endpoint,
                timeout_ms,
                attempts,
                ..
            }) => {
                assert_eq!(endpoint, "127.0.0.1:9000");
                assert_eq!(*timeout_ms, 400);
                assert_eq!(*attempts, 3);
            }
            _ => return Err("expected Telemetry Probe command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_capture() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "capture",
            "--game",
            "acc",
            "--port",
            "9001",
            "--duration",
            "30",
            "--out",
            "capture.bin",
            "--max-payload",
            "1024",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Capture {
                game,
                port,
                duration,
                out,
                max_payload,
            }) => {
                assert_eq!(game, "acc");
                assert_eq!(*port, 9001);
                assert_eq!(*duration, 30);
                assert_eq!(out, "capture.bin");
                assert_eq!(*max_payload, 1024);
            }
            _ => return Err("expected Telemetry Capture command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_record() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "record",
            "--game",
            "simhub-bridge",
            "--telemetry-source",
            "simhub_bridge",
            "--input",
            "normalized.jsonl",
            "--out",
            "simulator-telemetry-recording.jsonl",
            "--session-id",
            "session-001",
            "--duration-ms",
            "5000",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Record {
                game,
                telemetry_source,
                input,
                live_simhub,
                port,
                out,
                session_id,
                duration_ms,
            }) => {
                assert_eq!(game, "simhub-bridge");
                assert_eq!(telemetry_source, "simhub_bridge");
                assert_eq!(input.as_deref(), Some("normalized.jsonl"));
                assert!(!live_simhub);
                assert_eq!(*port, 5555);
                assert_eq!(out, "simulator-telemetry-recording.jsonl");
                assert_eq!(session_id.as_deref(), Some("session-001"));
                assert_eq!(*duration_ms, 5000);
            }
            _ => return Err("expected Telemetry Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_record_live_simhub() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "record",
            "--game",
            "simhub-bridge",
            "--telemetry-source",
            "simhub_bridge",
            "--live-simhub",
            "--port",
            "5556",
            "--out",
            "simulator-telemetry-recording.jsonl",
            "--session-id",
            "session-001",
            "--duration-ms",
            "5000",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Record {
                game,
                telemetry_source,
                input,
                live_simhub,
                port,
                out,
                session_id,
                duration_ms,
            }) => {
                assert_eq!(game, "simhub-bridge");
                assert_eq!(telemetry_source, "simhub_bridge");
                assert!(input.is_none());
                assert!(*live_simhub);
                assert_eq!(*port, 5556);
                assert_eq!(out, "simulator-telemetry-recording.jsonl");
                assert_eq!(session_id.as_deref(), Some("session-001"));
                assert_eq!(*duration_ms, 5000);
            }
            _ => return Err("expected Telemetry Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_virtual_ffb_log() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "virtual-ffb-log",
            "--input",
            "simulator-telemetry-recording.jsonl",
            "--out",
            "target/virtual/simulator-ffb-output.virtual.jsonl",
            "--session-id",
            "virtual-session-001",
            "--max-percent",
            "2",
            "--watchdog-timeout-ms",
            "100",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::VirtualFfbLog {
                input,
                out,
                session_id,
                max_percent,
                watchdog_timeout_ms,
            }) => {
                assert_eq!(input, "simulator-telemetry-recording.jsonl");
                assert_eq!(out, "target/virtual/simulator-ffb-output.virtual.jsonl");
                assert_eq!(session_id.as_deref(), Some("virtual-session-001"));
                assert_eq!(*max_percent, 2.0);
                assert_eq!(*watchdog_timeout_ms, 100);
            }
            _ => return Err("expected Telemetry VirtualFfbLog command".into()),
        }
        Ok(())
    }

    // --- Moza command parsing ---

    #[test]
    fn parse_moza_init_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "init-lane",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--wheelbase-pid",
            "0x0004",
            "--operator",
            "Steven",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::InitLane {
                lane,
                wheelbase_pid,
                operator,
                overwrite,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(wheelbase_pid, "0x0004");
                assert_eq!(operator, "Steven");
                assert!(*overwrite);
            }
            _ => return Err("expected Moza InitLane command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_probe_with_json_out() -> TestResult {
        let cli = parse_cli(["wheelctl", "moza", "probe", "--json-out", "moza-probe.json"])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Probe { json_out }) => {
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("moza-probe.json")
                );
            }
            _ => return Err("expected Moza Probe command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_status_with_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "status",
            "--device",
            "0x0014",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--json-out",
            "moza-status.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Status {
                device,
                lane,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x0014"));
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("moza-status.json")
                );
            }
            _ => return Err("expected Moza Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_descriptor_with_device() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "descriptor",
            "--device",
            "0x0014",
            "--descriptor-hex",
            "--report-descriptor-hex",
            "05010904",
            "--json-out",
            "descriptor.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Descriptor {
                device,
                descriptor_hex,
                report_descriptor_hex,
                report_descriptor_hex_file,
                report_descriptor_bin_file,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x0014"));
                assert!(*descriptor_hex);
                assert_eq!(report_descriptor_hex.as_deref(), Some("05010904"));
                assert!(report_descriptor_hex_file.is_none());
                assert!(report_descriptor_bin_file.is_none());
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("descriptor.json")
                );
            }
            _ => return Err("expected Moza Descriptor command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_descriptor_with_report_descriptor_hex_file() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "descriptor",
            "--device",
            "0x0004",
            "--report-descriptor-hex-file",
            "target/r5-report-descriptor.txt",
            "--json-out",
            "descriptor.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Descriptor {
                device,
                descriptor_hex,
                report_descriptor_hex,
                report_descriptor_hex_file,
                report_descriptor_bin_file,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x0004"));
                assert!(!*descriptor_hex);
                assert!(report_descriptor_hex.is_none());
                assert_eq!(
                    report_descriptor_hex_file.as_ref().and_then(|p| p.to_str()),
                    Some("target/r5-report-descriptor.txt")
                );
                assert!(report_descriptor_bin_file.is_none());
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("descriptor.json")
                );
            }
            _ => return Err("expected Moza Descriptor command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_descriptor_with_report_descriptor_bin_file() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "descriptor",
            "--device",
            "0x0004",
            "--report-descriptor-bin-file",
            "target/r5-report-descriptor.bin",
            "--json-out",
            "descriptor.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Descriptor {
                device,
                descriptor_hex,
                report_descriptor_hex,
                report_descriptor_hex_file,
                report_descriptor_bin_file,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x0004"));
                assert!(!*descriptor_hex);
                assert!(report_descriptor_hex.is_none());
                assert!(report_descriptor_hex_file.is_none());
                assert_eq!(
                    report_descriptor_bin_file.as_ref().and_then(|p| p.to_str()),
                    Some("target/r5-report-descriptor.bin")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("descriptor.json")
                );
            }
            _ => return Err("expected Moza Descriptor command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_capture_input() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "capture-input",
            "--device",
            "0x346E:0x0014",
            "--duration-ms",
            "250",
            "--read-timeout-ms",
            "20",
            "--json-out",
            "captures/r5-idle.jsonl",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::CaptureInput {
                device,
                duration_ms,
                read_timeout_ms,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(*duration_ms, 250);
                assert_eq!(*read_timeout_ms, 20);
                assert_eq!(json_out.as_path().to_str(), Some("captures/r5-idle.jsonl"));
            }
            _ => return Err("expected Moza CaptureInput command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_steering_stream_proof() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "steering-stream-proof",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--duration-ms",
            "5000",
            "--read-timeout-ms",
            "20",
            "--degrees-of-rotation",
            "1080",
            "--jsonl-out",
            "target/steering-angle-stream.jsonl",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::SteeringStreamProof {
                device,
                lane,
                duration_ms,
                read_timeout_ms,
                degrees_of_rotation,
                jsonl_out,
                json_out,
            }) => {
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(*duration_ms, 5000);
                assert_eq!(*read_timeout_ms, 20);
                assert_eq!(*degrees_of_rotation, 1080.0);
                assert_eq!(
                    jsonl_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/steering-angle-stream.jsonl")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json")
                );
            }
            _ => return Err("expected Moza SteeringStreamProof command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_validate_capture() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "validate-capture",
            "--capture",
            "captures/r5-idle.jsonl",
            "--pid",
            "0x0014",
            "--json-out",
            "parser-fixture-validation.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ValidateCapture {
                capture,
                pid,
                json_out,
            }) => {
                assert_eq!(capture.as_path().to_str(), Some("captures/r5-idle.jsonl"));
                assert_eq!(pid.as_deref(), Some("0x0014"));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("parser-fixture-validation.json")
                );
            }
            _ => return Err("expected Moza ValidateCapture command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_analyze_capture() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "analyze-capture",
            "--capture",
            "captures/r5-throttle-only-sweep.jsonl",
            "--json-out",
            "target/r5-throttle-analysis.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::AnalyzeCapture { capture, json_out }) => {
                assert_eq!(
                    capture.as_path().to_str(),
                    Some("captures/r5-throttle-only-sweep.jsonl")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/r5-throttle-analysis.json")
                );
            }
            _ => return Err("expected Moza AnalyzeCapture command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_analyze_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "analyze-lane",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--json-out",
            "target/lane-analysis.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::AnalyzeLane { lane, json_out }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/lane-analysis.json")
                );
            }
            _ => return Err("expected Moza AnalyzeLane command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_sync_role_status() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "sync-role-status",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--check",
            "--json-out",
            "target/role-status-sync.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::SyncRoleStatus {
                lane,
                check,
                json_out,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert!(*check);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("target/role-status-sync.json")
                );
            }
            _ => return Err("expected Moza SyncRoleStatus command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_validate_captures() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "validate-captures",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--json-out",
            "parser-fixture-validation.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ValidateCaptures { lane, json_out }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("parser-fixture-validation.json")
                );
            }
            _ => return Err("expected Moza ValidateCaptures command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pre_output_readiness() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pre-output-readiness",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--json-out",
            "pre-output-readiness.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PreOutputReadiness { lane, json_out }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("pre-output-readiness.json")
                );
            }
            _ => return Err("expected Moza PreOutputReadiness command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_promote_fixture() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "promote-fixture",
            "--capture",
            "captures/r5-idle.jsonl",
            "--fixture-id",
            "r5_v2_idle",
            "--fixture-out",
            "crates/moza-wheelbase-report/fixtures/r5_v2_idle.json",
            "--pid",
            "0x0014",
            "--max-reports",
            "32",
            "--overwrite",
            "--json-out",
            "fixture-promotion.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PromoteFixture {
                capture,
                fixture_id,
                fixture_out,
                pid,
                max_reports,
                overwrite,
                json_out,
            }) => {
                assert_eq!(capture.as_path().to_str(), Some("captures/r5-idle.jsonl"));
                assert_eq!(fixture_id, "r5_v2_idle");
                assert_eq!(
                    fixture_out.as_path().to_str(),
                    Some("crates/moza-wheelbase-report/fixtures/r5_v2_idle.json")
                );
                assert_eq!(pid.as_deref(), Some("0x0014"));
                assert_eq!(*max_reports, 32);
                assert!(*overwrite);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("fixture-promotion.json")
                );
            }
            _ => return Err("expected Moza PromoteFixture command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_promote_fixtures() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "promote-fixtures",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--fixture-dir",
            "crates/hid-moza-protocol/fixtures/moza-r5-2026-05-06",
            "--max-reports",
            "64",
            "--overwrite",
            "--json-out",
            "fixture-promotion.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PromoteFixtures {
                lane,
                fixture_dir,
                max_reports,
                overwrite,
                json_out,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    fixture_dir.as_path().to_str(),
                    Some("crates/hid-moza-protocol/fixtures/moza-r5-2026-05-06")
                );
                assert_eq!(*max_reports, 64);
                assert!(*overwrite);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("fixture-promotion.json")
                );
            }
            _ => return Err("expected Moza PromoteFixtures command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_zero() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "zero",
            "--device",
            "0x346E:0x0014",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--pid",
            "0x0014",
            "--dry-run",
            "--confirm-zero-torque",
            "--repeat",
            "250",
            "--hz",
            "1000",
            "--watchdog-timeout-ms",
            "50",
            "--json-out",
            "zero-torque-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Zero {
                device,
                lane,
                pid,
                strategy,
                dry_run,
                confirm_zero_torque,
                repeat,
                hz,
                watchdog_timeout_ms,
                json_out,
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(pid.as_deref(), Some("0x0014"));
                assert_eq!(
                    *strategy,
                    crate::commands::MozaZeroOutputStrategy::DirectReport0x20
                );
                assert!(*dry_run);
                assert!(*confirm_zero_torque);
                assert_eq!(*repeat, 250);
                assert_eq!(*hz, 1000);
                assert_eq!(*watchdog_timeout_ms, 50);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("zero-torque-proof.json")
                );
            }
            _ => return Err("expected Moza Zero command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_zero_pidff_stop_all_strategy() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "zero",
            "--device",
            "0x346E:0x0004",
            "--strategy",
            "pidff-stop-all",
            "--dry-run",
            "--json-out",
            "zero-torque-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Zero { strategy, .. }) => {
                assert_eq!(
                    *strategy,
                    crate::commands::MozaZeroOutputStrategy::PidffStopAll
                );
            }
            _ => return Err("expected Moza Zero command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_watchdog_proof() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "watchdog-proof",
            "--device",
            "0x346E:0x0014",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--confirm-watchdog-test",
            "--pre-zero-count",
            "5",
            "--hz",
            "1000",
            "--watchdog-timeout-ms",
            "100",
            "--json-out",
            "watchdog-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::WatchdogProof {
                device,
                lane,
                strategy,
                confirm_watchdog_test,
                pre_zero_count,
                hz,
                watchdog_timeout_ms,
                json_out,
                ..
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(
                    *strategy,
                    crate::commands::MozaZeroOutputStrategy::DirectReport0x20
                );
                assert!(*confirm_watchdog_test);
                assert_eq!(*pre_zero_count, 5);
                assert_eq!(*hz, 1000);
                assert_eq!(*watchdog_timeout_ms, 100);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("watchdog-proof.json")
                );
            }
            _ => return Err("expected Moza WatchdogProof command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_disconnect_proof() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "disconnect-proof",
            "--device",
            "0x346E:0x0014",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--confirm-disconnect-test",
            "--max-duration-ms",
            "5000",
            "--hz",
            "1000",
            "--json-out",
            "disconnect-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::DisconnectProof {
                device,
                lane,
                strategy,
                confirm_disconnect_test,
                max_duration_ms,
                hz,
                json_out,
                ..
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(
                    *strategy,
                    crate::commands::MozaZeroOutputStrategy::DirectReport0x20
                );
                assert!(*confirm_disconnect_test);
                assert_eq!(*max_duration_ms, 5000);
                assert_eq!(*hz, 1000);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("disconnect-proof.json")
                );
            }
            _ => return Err("expected Moza DisconnectProof command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_init() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "init",
            "--device",
            "0x346E:0x0014",
            "--mode",
            "standard",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--confirm-init",
            "--json-out",
            "init-standard.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::Init {
                device,
                lane,
                mode,
                confirm_init,
                json_out,
                ..
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert!(matches!(mode, MozaInitMode::Standard));
                assert!(*confirm_init);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("init-standard.json")
                );
            }
            _ => return Err("expected Moza Init command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_torque_test() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "torque-test",
            "--device",
            "0x346E:0x0014",
            "--zero-proof",
            "zero-torque-proof.json",
            "--descriptor",
            "descriptor.json",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--confirm-low-torque",
            "--explicit-operator-override",
            "--max-percent",
            "2",
            "--duration-ms",
            "250",
            "--hz",
            "1000",
            "--json-out",
            "low-torque-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::TorqueTest {
                device,
                zero_proof,
                descriptor,
                lane,
                strategy,
                confirm_low_torque,
                explicit_operator_override,
                max_percent,
                duration_ms,
                hz,
                json_out,
                ..
            }) => {
                assert_eq!(device.as_deref(), Some("0x346E:0x0014"));
                assert_eq!(
                    zero_proof.as_ref().and_then(|p| p.to_str()),
                    Some("zero-torque-proof.json")
                );
                assert_eq!(
                    descriptor.as_ref().and_then(|p| p.to_str()),
                    Some("descriptor.json")
                );
                assert_eq!(
                    lane.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    *strategy,
                    crate::commands::MozaLowTorqueStrategy::DirectReport0x20
                );
                assert!(*confirm_low_torque);
                assert!(*explicit_operator_override);
                assert!((*max_percent - 2.0).abs() < f32::EPSILON);
                assert_eq!(*duration_ms, 250);
                assert_eq!(*hz, 1000);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("low-torque-proof.json")
                );
            }
            _ => return Err("expected Moza TorqueTest command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_torque_test_pidff_strategy() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "torque-test",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--strategy",
            "pidff-bounded-effect",
            "--zero-proof",
            "ci/hardware/moza-r5/2026-05-13/zero-torque-proof.json",
            "--init-off",
            "ci/hardware/moza-r5/2026-05-13/init-off.json",
            "--init-standard",
            "ci/hardware/moza-r5/2026-05-13/init-standard.json",
            "--dry-run",
            "--json-out",
            "low-torque-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::TorqueTest {
                strategy, dry_run, ..
            }) => {
                assert_eq!(
                    *strategy,
                    crate::commands::MozaLowTorqueStrategy::PidffBoundedEffect
                );
                assert!(*dry_run);
            }
            _ => return Err("expected Moza TorqueTest command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_actuator_profile_smoke() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "actuator-profile-smoke",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--low-torque-proof",
            "ci/hardware/moza-r5/2026-05-13/low-torque-proof.json",
            "--steering-proof",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
            "--profile",
            "constant-low-force",
            "--strategy",
            "pidff-bounded-effect",
            "--max-percent",
            "1",
            "--duration-ms",
            "2000",
            "--confirm-actuator-profile",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ActuatorProfileSmoke {
                device,
                lane,
                low_torque_proof,
                steering_proof,
                profile,
                strategy,
                confirm_actuator_profile,
                max_percent,
                duration_ms,
                json_out,
                ..
            }) => {
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(
                    low_torque_proof.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/low-torque-proof.json")
                );
                assert_eq!(
                    steering_proof.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json")
                );
                assert_eq!(*profile, MozaActuatorProfile::ConstantLowForce);
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!(*confirm_actuator_profile);
                assert!((*max_percent - 1.0).abs() < f32::EPSILON);
                assert_eq!(*duration_ms, 2000);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json")
                );
            }
            _ => return Err("expected Moza ActuatorProfileSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_actuator_visible_smoke() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "actuator-visible-smoke",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--prior-actuator-proof",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json",
            "--steering-proof",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
            "--profile",
            "constant-low-force",
            "--strategy",
            "pidff-bounded-effect",
            "--max-percent",
            "5",
            "--duration-ms",
            "2000",
            "--confirm-actuator-visible",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-visible-smoke.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ActuatorVisibleSmoke {
                device,
                lane,
                prior_actuator_proof,
                steering_proof,
                profile,
                strategy,
                confirm_actuator_visible,
                max_percent,
                duration_ms,
                json_out,
                ..
            }) => {
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(
                    prior_actuator_proof.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json")
                );
                assert_eq!(
                    steering_proof.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json")
                );
                assert_eq!(*profile, MozaActuatorProfile::ConstantLowForce);
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!(*confirm_actuator_visible);
                assert!((*max_percent - 5.0).abs() < f32::EPSILON);
                assert_eq!(*duration_ms, 2000);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/native-actuator-visible-smoke.json")
                );
            }
            _ => return Err("expected Moza ActuatorVisibleSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_actuator_visible_shaped_micro_profile() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "actuator-visible-smoke",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--prior-actuator-proof",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json",
            "--steering-proof",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
            "--profile",
            "bounded-shaped-pidff-micro-profile",
            "--strategy",
            "pidff-bounded-effect",
            "--max-percent",
            "5",
            "--duration-ms",
            "2000",
            "--dry-run",
            "--json-out",
            "target/native-actuator-visible-shaped-plan.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ActuatorVisibleSmoke {
                profile,
                strategy,
                dry_run,
                confirm_actuator_visible,
                max_percent,
                duration_ms,
                ..
            }) => {
                assert_eq!(
                    *profile,
                    MozaActuatorProfile::BoundedShapedPidffMicroProfile
                );
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!(*dry_run);
                assert!(!*confirm_actuator_visible);
                assert!((*max_percent - 5.0).abs() < f32::EPSILON);
                assert_eq!(*duration_ms, 2000);
            }
            _ => return Err("expected Moza ActuatorVisibleSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_controlled_angle_smoke_dry_run() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "controlled-angle-smoke",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--prior-actuator-proof",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json",
            "--steering-proof",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
            "--target-degrees",
            "1",
            "--profile",
            "bounded-pidff-micro-step-v2",
            "--max-percent",
            "5",
            "--strategy",
            "pidff-bounded-effect",
            "--dry-run",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/native-controlled-angle-smoke.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ControlledAngleSmoke {
                device,
                lane,
                target_degrees,
                profile,
                max_percent,
                timeout_ms,
                strategy,
                dry_run,
                confirm_controlled_angle,
                json_out,
                ..
            }) => {
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert!((*target_degrees - 1.0).abs() < f64::EPSILON);
                assert_eq!(
                    *profile,
                    MozaControlledAngleProfile::BoundedPidffMicroStepV2
                );
                assert!((*max_percent - 5.0).abs() < f32::EPSILON);
                assert_eq!(*timeout_ms, 2000);
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!(*dry_run);
                assert!(!*confirm_controlled_angle);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("ci/hardware/moza-r5/2026-05-13/native-controlled-angle-smoke.json")
                );
            }
            _ => return Err("expected Moza ControlledAngleSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_authorize_visible_output() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "authorize-visible-output",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--operator",
            "Steven",
            "--bench-clear-evidence",
            "Bench clear for the exact shaped PIDFF command.",
            "--ffb-mode-evidence",
            "Wheelbase is in standard/PIDFF mode; no simulator FFB source active.",
            "--profile",
            "bounded-shaped-pidff-micro-profile",
            "--strategy",
            "pidff-bounded-effect",
            "--max-percent",
            "5",
            "--duration-ms",
            "2000",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-visible-follow-up-plan.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::AuthorizeVisibleOutput {
                lane,
                device,
                operator,
                profile,
                strategy,
                max_percent,
                duration_ms,
                json_out,
                ..
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(operator, "Steven");
                assert_eq!(
                    *profile,
                    MozaActuatorProfile::BoundedShapedPidffMicroProfile
                );
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!((*max_percent - 5.0).abs() < f32::EPSILON);
                assert_eq!(*duration_ms, 2000);
                assert_eq!(
                    json_out.as_ref().and_then(|path| path.to_str()),
                    Some(
                        "ci/hardware/moza-r5/2026-05-13/native-actuator-visible-follow-up-plan.json"
                    )
                );
            }
            _ => return Err("expected Moza AuthorizeVisibleOutput command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_authorize_controlled_angle_output() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "authorize-controlled-angle-output",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--device",
            "hid-0x346E-0x0004-if2-0x0001-0x0004",
            "--operator",
            "Steven",
            "--bench-clear-evidence",
            "Bench clear for exactly one 1 degree controlled-angle command.",
            "--prior-response-proof",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-visible-smoke.json",
            "--prior-actuator-proof",
            "ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json",
            "--steering-proof",
            "ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json",
            "--target-degrees",
            "1",
            "--profile",
            "bounded-pidff-micro-step-v2",
            "--strategy",
            "pidff-bounded-effect",
            "--max-percent",
            "5",
            "--timeout-ms",
            "2000",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-13/native-controlled-angle-authorization.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::AuthorizeControlledAngleOutput {
                lane,
                device,
                operator,
                target_degrees,
                profile,
                strategy,
                max_percent,
                timeout_ms,
                json_out,
                ..
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-13")
                );
                assert_eq!(device, "hid-0x346E-0x0004-if2-0x0001-0x0004");
                assert_eq!(operator, "Steven");
                assert!((*target_degrees - 1.0).abs() < f64::EPSILON);
                assert_eq!(
                    *profile,
                    MozaControlledAngleProfile::BoundedPidffMicroStepV2
                );
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
                assert!((*max_percent - 5.0).abs() < f32::EPSILON);
                assert_eq!(*timeout_ms, 2000);
                assert_eq!(
                    json_out.as_ref().and_then(|path| path.to_str()),
                    Some(
                        "ci/hardware/moza-r5/2026-05-13/native-controlled-angle-authorization.json"
                    )
                );
            }
            _ => return Err("expected Moza AuthorizeControlledAngleOutput command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_receipt_template() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "receipt-template",
            "--kind",
            "pit-house",
            "--json-out",
            "pit-house-coexistence.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::ReceiptTemplate {
                kind,
                json_out,
                overwrite,
            }) => {
                assert!(matches!(kind, MozaReceiptTemplateKind::PitHouse));
                assert_eq!(
                    json_out.as_path().to_str(),
                    Some("pit-house-coexistence.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza ReceiptTemplate command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pit_house_availability() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pit-house-availability",
            "--operator",
            "Steven",
            "--evidence",
            "Pit House is not installed on this host.",
            "--json-out",
            "pit-house-availability.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PitHouseAvailability {
                operator,
                evidence,
                json_out,
                overwrite,
            }) => {
                assert_eq!(operator, "Steven");
                assert_eq!(
                    evidence.as_deref(),
                    Some("Pit House is not installed on this host.")
                );
                assert_eq!(
                    json_out.as_path().to_str(),
                    Some("pit-house-availability.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza PitHouseAvailability command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pit_house_observation() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pit-house-observation",
            "--case",
            "open-standard",
            "--evidence-kind",
            "operator-screenshot",
            "--evidence-artifact",
            "pit-house-open-standard.png",
            "--operator",
            "Steven",
            "--evidence",
            "Pit House open and idle screenshot saved.",
            "--json-out",
            "pit-house-observation-open-standard.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PitHouseObservation {
                case,
                evidence_kind,
                evidence_artifact,
                operator,
                evidence,
                json_out,
                overwrite,
            }) => {
                assert!(matches!(case, MozaPitHouseObservationCase::OpenStandard));
                assert!(matches!(
                    evidence_kind,
                    MozaPitHouseEvidenceKind::OperatorScreenshot
                ));
                assert_eq!(
                    evidence_artifact.as_ref().and_then(|path| path.to_str()),
                    Some("pit-house-open-standard.png")
                );
                assert_eq!(operator, "Steven");
                assert_eq!(evidence, "Pit House open and idle screenshot saved.");
                assert_eq!(
                    json_out.as_path().to_str(),
                    Some("pit-house-observation-open-standard.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza PitHouseObservation command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pit_house_evidence() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pit-house-evidence",
            "--case",
            "open-standard",
            "--operator",
            "Steven",
            "--evidence",
            "Pit House process/window snapshot saved.",
            "--require-match",
            "--json-out",
            "pit-house-evidence-open-standard.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PitHouseEvidence {
                case,
                operator,
                evidence,
                require_match,
                json_out,
                overwrite,
            }) => {
                assert!(matches!(case, MozaPitHouseObservationCase::OpenStandard));
                assert_eq!(operator, "Steven");
                assert_eq!(
                    evidence.as_deref(),
                    Some("Pit House process/window snapshot saved.")
                );
                assert_eq!(
                    json_out.as_path().to_str(),
                    Some("pit-house-evidence-open-standard.json")
                );
                assert!(*require_match);
                assert!(*overwrite);
            }
            _ => return Err("expected Moza PitHouseEvidence command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pit_house_case() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pit-house-case",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--case",
            "open-standard",
            "--observation-artifact",
            "pit-house-observation-open-standard.json",
            "--evidence",
            "Pit House open idle case linked to standard init receipt.",
            "--json-out",
            "ci/hardware/moza-r5/2026-05-06/pit-house-open-standard.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PitHouseCase {
                lane,
                case,
                observation_artifact,
                evidence,
                json_out,
                overwrite,
            }) => {
                assert_eq!(lane.to_str(), Some("ci/hardware/moza-r5/2026-05-06"));
                assert!(matches!(case, MozaPitHouseObservationCase::OpenStandard));
                assert_eq!(
                    observation_artifact.to_str(),
                    Some("pit-house-observation-open-standard.json")
                );
                assert_eq!(
                    evidence,
                    "Pit House open idle case linked to standard init receipt."
                );
                assert_eq!(
                    json_out.to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06/pit-house-open-standard.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza PitHouseCase command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_pit_house_proof() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "pit-house-proof",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--closed-artifact",
            "pit-house-closed.json",
            "--open-standard-artifact",
            "pit-house-open-standard.json",
            "--direct-artifact",
            "pit-house-direct-blocked.json",
            "--mode-change-artifact",
            "pit-house-mode-change.json",
            "--firmware-page-artifact",
            "pit-house-firmware-page.json",
            "--shared-control-risk",
            "documented_limit",
            "--json-out",
            "pit-house-coexistence.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PitHouseProof {
                lane,
                closed_artifact,
                open_standard_artifact,
                direct_artifact,
                mode_change_artifact,
                firmware_page_artifact,
                shared_control_risk,
                json_out,
                overwrite,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(
                    closed_artifact.as_path().to_str(),
                    Some("pit-house-closed.json")
                );
                assert_eq!(
                    open_standard_artifact.as_path().to_str(),
                    Some("pit-house-open-standard.json")
                );
                assert_eq!(
                    direct_artifact.as_path().to_str(),
                    Some("pit-house-direct-blocked.json")
                );
                assert_eq!(
                    mode_change_artifact.as_path().to_str(),
                    Some("pit-house-mode-change.json")
                );
                assert_eq!(
                    firmware_page_artifact.as_path().to_str(),
                    Some("pit-house-firmware-page.json")
                );
                assert_eq!(shared_control_risk, "documented_limit");
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("pit-house-coexistence.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza PitHouseProof command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_simulator_telemetry_proof_defaults_source() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "simulator-telemetry-proof",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--game",
            "simhub-bridge",
            "--recorder-artifact",
            "simulator-telemetry-recording.jsonl",
            "--duration-ms",
            "5000",
            "--json-out",
            "simulator-telemetry-proof.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::SimulatorTelemetryProof {
                lane,
                game,
                telemetry_source,
                recorder_artifact,
                duration_ms,
                json_out,
                overwrite,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(game, "simhub-bridge");
                assert_eq!(telemetry_source, "simhub_bridge");
                assert_eq!(
                    recorder_artifact.as_path().to_str(),
                    Some("simulator-telemetry-recording.jsonl")
                );
                assert_eq!(*duration_ms, 5000);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("simulator-telemetry-proof.json")
                );
                assert!(!*overwrite);
            }
            _ => return Err("expected Moza SimulatorTelemetryProof command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_simulator_ffb_smoke() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "simulator-ffb-smoke",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--game",
            "iracing",
            "--telemetry-source",
            "real_game",
            "--output-log-artifact",
            "simulator-ffb-output.jsonl",
            "--descriptor-trusted",
            "--watchdog-timeout-ms",
            "100",
            "--stop-cleared-output",
            "--pause-cleared-output",
            "--game-exit-cleared-output",
            "--json-out",
            "simulator-ffb-smoke.json",
            "--overwrite",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::SimulatorFfbSmoke {
                lane,
                game,
                telemetry_source,
                output_log_artifact,
                strategy,
                descriptor_trusted,
                explicit_operator_override,
                watchdog_timeout_ms,
                stop_cleared_output,
                pause_cleared_output,
                game_exit_cleared_output,
                json_out,
                overwrite,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(game, "iracing");
                assert_eq!(telemetry_source, "real_game");
                assert_eq!(
                    output_log_artifact.as_path().to_str(),
                    Some("simulator-ffb-output.jsonl")
                );
                assert_eq!(*strategy, MozaLowTorqueStrategy::DirectReport0x20);
                assert!(*descriptor_trusted);
                assert!(!*explicit_operator_override);
                assert_eq!(*watchdog_timeout_ms, 100);
                assert!(*stop_cleared_output);
                assert!(*pause_cleared_output);
                assert!(*game_exit_cleared_output);
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("simulator-ffb-smoke.json")
                );
                assert!(*overwrite);
            }
            _ => return Err("expected Moza SimulatorFfbSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_simulator_ffb_smoke_pidff_strategy() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "simulator-ffb-smoke",
            "--lane",
            "ci/hardware/moza-r5/2026-05-13",
            "--game",
            "simhub-bridge",
            "--telemetry-source",
            "simhub_bridge",
            "--output-log-artifact",
            "simulator-ffb-output.jsonl",
            "--strategy",
            "pidff-bounded-effect",
            "--descriptor-trusted",
            "--watchdog-timeout-ms",
            "100",
            "--stop-cleared-output",
            "--pause-cleared-output",
            "--game-exit-cleared-output",
        ])?;

        match &cli.command {
            Commands::Moza(MozaCommands::SimulatorFfbSmoke { strategy, .. }) => {
                assert_eq!(*strategy, MozaLowTorqueStrategy::PidffBoundedEffect);
            }
            _ => return Err("expected Moza SimulatorFfbSmoke command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_promote_manifest() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "promote-manifest",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--stage",
            "zero",
            "--json-out",
            "manifest-promotion-zero.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::PromoteManifest {
                lane,
                stage,
                json_out,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert!(matches!(stage, MozaBundleStage::Zero));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("manifest-promotion-zero.json")
                );
            }
            _ => return Err("expected Moza PromoteManifest command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_verify_bundle() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "verify-bundle",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--stage",
            "smoke-ready",
            "--json-out",
            "bundle-verification.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::VerifyBundle {
                lane,
                stage,
                json_out,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert!(matches!(stage, MozaBundleStage::SmokeReady));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("bundle-verification.json")
                );
            }
            _ => return Err("expected Moza VerifyBundle command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_moza_audit_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "moza",
            "audit-lane",
            "--lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--stage",
            "smoke-ready",
            "--json-out",
            "lane-audit-smoke-ready.json",
        ])?;
        match &cli.command {
            Commands::Moza(MozaCommands::AuditLane {
                lane,
                stage,
                json_out,
            }) => {
                assert_eq!(
                    lane.as_path().to_str(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert!(matches!(stage, MozaBundleStage::SmokeReady));
                assert_eq!(
                    json_out.as_ref().and_then(|p| p.to_str()),
                    Some("lane-audit-smoke-ready.json")
                );
            }
            _ => return Err("expected Moza AuditLane command".into()),
        }
        Ok(())
    }

    // --- Game command parsing ---

    #[test]
    fn parse_game_configure() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "game",
            "configure",
            "iracing",
            "--path",
            "/games/iracing",
            "--auto",
        ])?;
        match &cli.command {
            Commands::Game(GameCommands::Configure { game, path, auto }) => {
                assert_eq!(game, "iracing");
                assert_eq!(path.as_deref(), Some("/games/iracing"));
                assert!(auto);
            }
            _ => return Err("expected Game Configure command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_test_custom_duration() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "test", "acc", "--duration", "30"])?;
        match &cli.command {
            Commands::Game(GameCommands::Test { game, duration }) => {
                assert_eq!(game, "acc");
                assert_eq!(*duration, 30);
            }
            _ => return Err("expected Game Test command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_test_default_duration() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "test", "acc"])?;
        match &cli.command {
            Commands::Game(GameCommands::Test { duration, .. }) => {
                assert_eq!(*duration, 10);
            }
            _ => return Err("expected Game Test command".into()),
        }
        Ok(())
    }

    // --- Completion and health ---

    #[test]
    fn parse_completion_bash() -> TestResult {
        let cli = parse_cli(["wheelctl", "completion", "bash"])?;
        assert!(matches!(cli.command, Commands::Completion { .. }));
        Ok(())
    }

    #[test]
    fn parse_health_no_watch() -> TestResult {
        let cli = parse_cli(["wheelctl", "health"])?;
        match &cli.command {
            Commands::Health { watch } => assert!(!watch),
            _ => return Err("expected Health command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_health_watch() -> TestResult {
        let cli = parse_cli(["wheelctl", "health", "--watch"])?;
        match &cli.command {
            Commands::Health { watch } => assert!(watch),
            _ => return Err("expected Health command".into()),
        }
        Ok(())
    }

    // --- Rejection / error cases ---

    #[test]
    fn reject_no_subcommand() {
        let result = parse_cli(["wheelctl"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_unknown_subcommand() {
        let result = parse_cli(["wheelctl", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_required_device_arg() {
        let result = parse_cli(["wheelctl", "device", "status"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_calibration_type() {
        let result = parse_cli(["wheelctl", "device", "calibrate", "w1", "invalid_type"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_test_type() {
        let result = parse_cli(["wheelctl", "diag", "test", "bad_type"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_plugin_search_query() {
        let result = parse_cli(["wheelctl", "plugin", "search"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_completion_shell() {
        let result = parse_cli(["wheelctl", "completion"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_safety_limit_missing_torque() {
        let result = parse_cli(["wheelctl", "safety", "limit", "wheel-001"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_safety_limit_non_numeric_torque() {
        let result = parse_cli(["wheelctl", "safety", "limit", "wheel-001", "abc"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_unknown_device_subcommand() {
        let result = parse_cli(["wheelctl", "device", "fly"]);
        assert!(result.is_err());
    }

    // --- Additional subcommand parsing ---

    #[test]
    fn parse_profile_show() -> TestResult {
        let cli = parse_cli(["wheelctl", "profile", "show", "my_profile.json"])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Show { profile }) => {
                assert_eq!(profile, "my_profile.json");
            }
            _ => return Err("expected Profile Show command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_list_no_filter() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "list"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::List { category }) => {
                assert!(category.is_none());
            }
            _ => return Err("expected Plugin List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_list_with_category() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "list", "--category", "ffb"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::List { category }) => {
                assert_eq!(category.as_deref(), Some("ffb"));
            }
            _ => return Err("expected Plugin List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_info() -> TestResult {
        let cli = parse_cli(["wheelctl", "plugin", "info", "ffb-smoothing"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Info { plugin_id, version }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
                assert!(version.is_none());
            }
            _ => return Err("expected Plugin Info command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_info_with_version() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "plugin",
            "info",
            "ffb-smoothing",
            "--version",
            "2.0",
        ])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Info { plugin_id, version }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
                assert_eq!(version.as_deref(), Some("2.0"));
            }
            _ => return Err("expected Plugin Info command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_replay() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "replay", "recording.wbb"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Replay { file, detailed }) => {
                assert_eq!(file, "recording.wbb");
                assert!(!detailed);
            }
            _ => return Err("expected Diag Replay command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_replay_detailed() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "replay", "recording.wbb", "--detailed"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Replay { detailed, .. }) => {
                assert!(detailed);
            }
            _ => return Err("expected Diag Replay command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_support_defaults() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "support"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Support {
                blackbox,
                moza_lane,
                output,
            }) => {
                assert!(!blackbox);
                assert!(moza_lane.is_none());
                assert!(output.is_none());
            }
            _ => return Err("expected Diag Support command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_support_with_options() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "diag",
            "support",
            "--blackbox",
            "--output",
            "bundle.zip",
        ])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Support {
                blackbox,
                moza_lane,
                output,
            }) => {
                assert!(blackbox);
                assert!(moza_lane.is_none());
                assert_eq!(output.as_deref(), Some("bundle.zip"));
            }
            _ => return Err("expected Diag Support command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_support_with_moza_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "diag",
            "support",
            "--moza-lane",
            "ci/hardware/moza-r5/2026-05-06",
        ])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Support { moza_lane, .. }) => {
                assert_eq!(moza_lane.as_deref(), Some("ci/hardware/moza-r5/2026-05-06"));
            }
            _ => return Err("expected Diag Support command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_support_bundle_alias_with_device_and_moza_lane() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "support-bundle",
            "--device",
            "r5",
            "--moza-lane",
            "ci/hardware/moza-r5/2026-05-06",
            "--output",
            "support.json",
        ])?;
        match &cli.command {
            Commands::SupportBundle(args) => {
                assert_eq!(args.device.as_deref(), Some("r5"));
                assert!(!args.blackbox);
                assert_eq!(
                    args.moza_lane.as_deref(),
                    Some("ci/hardware/moza-r5/2026-05-06")
                );
                assert_eq!(args.output.as_deref(), Some("support.json"));
            }
            _ => return Err("expected SupportBundle command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_list_defaults() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "list"])?;
        match &cli.command {
            Commands::Game(GameCommands::List { detailed }) => {
                assert!(!detailed);
            }
            _ => return Err("expected Game List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_list_detailed() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "list", "--detailed"])?;
        match &cli.command {
            Commands::Game(GameCommands::List { detailed }) => {
                assert!(detailed);
            }
            _ => return Err("expected Game List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_status_no_telemetry() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "status"])?;
        match &cli.command {
            Commands::Game(GameCommands::Status { telemetry }) => {
                assert!(!telemetry);
            }
            _ => return Err("expected Game Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_status_with_telemetry() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "status", "--telemetry"])?;
        match &cli.command {
            Commands::Game(GameCommands::Status { telemetry }) => {
                assert!(telemetry);
            }
            _ => return Err("expected Game Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_status_no_device() -> TestResult {
        let cli = parse_cli(["wheelctl", "safety", "status"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Status { device }) => {
                assert!(device.is_none());
            }
            _ => return Err("expected Safety Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_status_with_device() -> TestResult {
        let cli = parse_cli(["wheelctl", "safety", "status", "wheel-001"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Status { device }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
            }
            _ => return Err("expected Safety Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_metrics_default() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "metrics"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Metrics { device, watch }) => {
                assert!(device.is_none());
                assert!(!watch);
            }
            _ => return Err("expected Diag Metrics command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_metrics_with_device() -> TestResult {
        let cli = parse_cli(["wheelctl", "diag", "metrics", "wheel-001", "--watch"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Metrics { device, watch }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
                assert!(watch);
            }
            _ => return Err("expected Diag Metrics command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_capture_defaults() -> TestResult {
        let cli = parse_cli([
            "wheelctl",
            "telemetry",
            "capture",
            "--game",
            "acc",
            "--out",
            "cap.bin",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Capture {
                game,
                port,
                duration,
                out,
                max_payload,
            }) => {
                assert_eq!(game, "acc");
                assert_eq!(*port, 9000);
                assert_eq!(*duration, 10);
                assert_eq!(out, "cap.bin");
                assert_eq!(*max_payload, 2048);
            }
            _ => return Err("expected Telemetry Capture command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_configure_minimal() -> TestResult {
        let cli = parse_cli(["wheelctl", "game", "configure", "acc"])?;
        match &cli.command {
            Commands::Game(GameCommands::Configure { game, path, auto }) => {
                assert_eq!(game, "acc");
                assert!(path.is_none());
                assert!(!auto);
            }
            _ => return Err("expected Game Configure command".into()),
        }
        Ok(())
    }

    #[test]
    fn reject_missing_game_configure_id() {
        let result = parse_cli(["wheelctl", "game", "configure"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_diag_record_missing_device() {
        let result = parse_cli(["wheelctl", "diag", "record"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_telemetry_probe_missing_game() {
        let result = parse_cli(["wheelctl", "telemetry", "probe"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_telemetry_capture_missing_out() {
        let result = parse_cli(["wheelctl", "telemetry", "capture", "--game", "acc"]);
        assert!(result.is_err());
    }
}
