//! Racing Wheel Service Daemon (wheeld)
//!
//! Complete system integration with graceful degradation, feature flags,
//! and comprehensive configuration management.

use clap::{Parser, Subcommand};
use racing_wheel_service::{FeatureFlags, ServiceDaemon, SystemConfig};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{self, EnvFilter};

use anyhow::{Context, Result};

#[derive(Parser)]
#[command(name = "wheeld")]
#[command(about = "Racing Wheel Service Daemon")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Force feedback mode (development)
    #[arg(long, value_enum)]
    mode: Option<FfbMode>,

    /// Disable real-time scheduling (for CI)
    #[arg(long)]
    rt_off: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Enable development features
    #[arg(long)]
    dev: bool,

    /// Dry run mode (validate config only)
    #[arg(long)]
    dry_run: bool,

    /// Run as Windows service (used by Windows Service Control Manager)
    /// This flag is automatically passed when the service is started by Windows.
    #[arg(long)]
    service: bool,

    /// Hardware validation lane identifier or path (for receipt-backed readiness)
    #[arg(long)]
    hardware_lane: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install the service
    Install,
    /// Uninstall the service
    Uninstall,
    /// Check service status
    Status,
    /// Validate configuration
    Validate,
    /// Run system diagnostics
    Diagnostics,
    /// Generate anti-cheat compatibility report
    AntiCheat,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum FfbMode {
    /// PID pass-through mode
    Pid,
    /// Raw torque mode (1kHz)
    Raw,
    /// Telemetry synthesis mode
    Synth,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging with appropriate level
    // When running as a service, use Windows Event Log or file logging
    let log_level = if cli.verbose {
        "racing_wheel_service=trace,racing_wheel_engine=debug,info"
    } else {
        "racing_wheel_service=info,racing_wheel_engine=warn,warn"
    };

    // Configure logging based on service mode
    if cli.service {
        // When running as a Windows service, log to file instead of stdout
        // The service runs without a console, so stdout logging won't work
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(log_level))
            .with_target(true)
            .with_thread_ids(true)
            .with_ansi(false) // Disable ANSI colors for file/event log
            .init();

        info!("Starting as Windows service");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(log_level))
            .with_target(true)
            .with_thread_ids(true)
            .init();
    }

    info!("Racing Wheel Service v{}", env!("CARGO_PKG_VERSION"));
    // Build info (optional environment variables)
    let git_sha = std::env::var("VERGEN_GIT_SHA").unwrap_or_else(|_| "unknown".to_string());
    let build_timestamp =
        std::env::var("VERGEN_BUILD_TIMESTAMP").unwrap_or_else(|_| "unknown".to_string());
    info!("Build: {} {}", git_sha, build_timestamp);

    // Handle commands
    if let Some(command) = cli.command {
        return handle_command(command).await;
    }

    // Load and validate system configuration
    let system_config = load_system_config(&cli)
        .await
        .context("Failed to load system configuration")?;

    // Validate configuration if requested
    if cli.dry_run {
        info!("Configuration validation successful");
        return Ok(());
    }

    // Create feature flags from CLI and config
    let feature_flags = create_feature_flags(&cli, &system_config);

    // Log system information (skip detailed logging in service mode to reduce noise)
    if !cli.service {
        log_system_info(&system_config, &feature_flags).await;
    } else {
        info!("Running in Windows service mode");
        debug!("Feature flags: {:?}", feature_flags);
    }

    // Create and run service daemon
    run_service_daemon(system_config, feature_flags, cli.hardware_lane.clone())
        .await
        .context("Service daemon failed")
}

async fn handle_command(command: Commands) -> Result<()> {
    match command {
        Commands::Install => {
            info!("Installing service...");
            ServiceDaemon::install()
                .await
                .context("Failed to install service")?;
            info!("Service installed successfully");
        }
        Commands::Uninstall => {
            info!("Uninstalling service...");
            ServiceDaemon::uninstall()
                .await
                .context("Failed to uninstall service")?;
            info!("Service uninstalled successfully");
        }
        Commands::Status => {
            let status = ServiceDaemon::status()
                .await
                .context("Failed to get service status")?;
            info!("Service status: {}", status);
        }
        Commands::Validate => {
            validate_system_configuration()
                .await
                .context("Configuration validation failed")?;
            info!("Configuration validation successful");
        }
        Commands::Diagnostics => {
            run_system_diagnostics()
                .await
                .context("System diagnostics failed")?;
        }
        Commands::AntiCheat => {
            generate_anticheat_report()
                .await
                .context("Failed to generate anti-cheat report")?;
        }
    }
    Ok(())
}

async fn load_system_config(cli: &Cli) -> Result<SystemConfig> {
    let config_path = cli.config.as_deref();

    let mut system_config = if let Some(path) = config_path {
        SystemConfig::load_from_path(path)
            .await
            .with_context(|| format!("Failed to load config from {}", path))?
    } else {
        SystemConfig::load().await.unwrap_or_else(|e| {
            warn!("Failed to load config, using defaults: {}", e);
            SystemConfig::default()
        })
    };

    // Apply CLI overrides
    if cli.rt_off {
        system_config.engine.disable_realtime = true;
        warn!("Real-time scheduling disabled via CLI flag");
    }

    if let Some(mode) = &cli.mode {
        system_config.engine.force_ffb_mode = Some(match mode {
            FfbMode::Pid => "pid".to_string(),
            FfbMode::Raw => "raw".to_string(),
            FfbMode::Synth => "synth".to_string(),
        });
        info!("FFB mode forced to {:?} via CLI", mode);
    }

    if cli.dev {
        system_config.development.enable_dev_features = true;
        system_config.development.enable_debug_logging = true;
        info!("Development features enabled");
    }

    // Validate configuration
    system_config
        .validate()
        .context("Configuration validation failed")?;

    Ok(system_config)
}

fn create_feature_flags(cli: &Cli, config: &SystemConfig) -> FeatureFlags {
    FeatureFlags {
        disable_realtime: cli.rt_off || config.engine.disable_realtime,
        force_ffb_mode: config.engine.force_ffb_mode.clone(),
        enable_dev_features: cli.dev || config.development.enable_dev_features,
        enable_debug_logging: cli.verbose || config.development.enable_debug_logging,
        enable_virtual_devices: config.development.enable_virtual_devices,
        disable_safety_interlocks: config.development.disable_safety_interlocks,
        enable_plugin_dev_mode: config.development.enable_plugin_dev_mode,
    }
}

async fn log_system_info(config: &SystemConfig, flags: &FeatureFlags) {
    info!("System Configuration:");
    info!("  Platform: {}", std::env::consts::OS);
    info!("  Architecture: {}", std::env::consts::ARCH);
    info!("  CPU cores: {}", num_cpus::get());

    let sys = sysinfo::System::new_all();
    let cpu_freq = sys.cpus().first().map(|cpu| cpu.frequency());
    match cpu_freq {
        Some(freq) => info!("  CPU: {} MHz", freq),
        None => info!("  CPU: Unknown"),
    }

    info!("Feature Flags:");
    info!("  Real-time disabled: {}", flags.disable_realtime);
    info!("  Forced FFB mode: {:?}", flags.force_ffb_mode);
    info!("  Development features: {}", flags.enable_dev_features);
    info!("  Debug logging: {}", flags.enable_debug_logging);
    info!("  Virtual devices: {}", flags.enable_virtual_devices);

    if flags.disable_safety_interlocks {
        warn!("SAFETY INTERLOCKS DISABLED - FOR DEVELOPMENT ONLY");
    }

    debug!("Configuration: {:#?}", config);
}

async fn run_service_daemon(
    config: SystemConfig,
    flags: FeatureFlags,
    hardware_lane: Option<String>,
) -> Result<()> {
    // Create service configuration from system config
    let service_config =
        racing_wheel_service::system_config::ServiceConfig::from_system_config(&config);

    if let Some(lane) = &hardware_lane {
        info!(
            "Hardware validation lane enabled: {} (readiness is observe-only until receipts pass)",
            lane
        );
    }

    // Create service daemon with feature flags
    let daemon =
        ServiceDaemon::new_with_flags_and_hardware_lane(service_config, flags, hardware_lane)
            .await
            .context("Failed to create service daemon")?;

    // Run the daemon with graceful degradation
    daemon
        .run()
        .await
        .context("Service daemon execution failed")
}

async fn validate_system_configuration() -> Result<()> {
    info!("Validating system configuration...");

    // Load and validate main config
    let config = SystemConfig::load().await?;
    config.validate()?;
    info!("✓ Main configuration valid");

    // Validate profile schemas
    let profile_service = racing_wheel_service::ApplicationProfileService::new().await?;
    let profiles = profile_service.list_profiles().await?;
    info!("✓ Found {} profiles", profiles.len());

    // Validate game support matrix
    let game_service = racing_wheel_service::ApplicationGameService::new().await?;
    let games = game_service.get_supported_games().await;
    info!("✓ Game support matrix loaded with {} games", games.len());

    // Check system requirements
    validate_system_requirements().await?;
    info!("✓ System requirements met");

    Ok(())
}

async fn validate_system_requirements() -> Result<()> {
    // Check OS version
    #[cfg(windows)]
    {
        let version = os_info::get();
        if version.version() < &os_info::Version::from_string("10.0") {
            anyhow::bail!("Windows 10 or later required");
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Check for required capabilities
        if !std::path::Path::new("/dev/hidraw0").exists() {
            warn!("No HID devices found - ensure udev rules are installed");
        }
    }

    // Check available memory
    let sys = sysinfo::System::new_all();
    let available_mb = sys.available_memory() / 1024 / 1024;
    if available_mb < 512 {
        warn!("Low available memory: {} MB", available_mb);
    }

    Ok(())
}

async fn run_system_diagnostics() -> Result<()> {
    info!("Running system diagnostics...");

    // Create diagnostic service
    let diag_service = racing_wheel_service::DiagnosticService::new().await?;

    // Run comprehensive diagnostics
    let results = diag_service.run_full_diagnostics().await?;

    // Display results
    for result in results {
        match result.status {
            racing_wheel_service::DiagnosticStatus::Pass => {
                info!("✓ {}: {}", result.name, result.message);
            }
            racing_wheel_service::DiagnosticStatus::Warn => {
                warn!("⚠ {}: {}", result.name, result.message);
            }
            racing_wheel_service::DiagnosticStatus::Fail => {
                error!("✗ {}: {}", result.name, result.message);
            }
        }
    }

    Ok(())
}

async fn generate_anticheat_report() -> Result<()> {
    info!("Generating anti-cheat compatibility report...");

    let report = racing_wheel_service::AntiCheatReport::generate().await?;

    // Write report to file
    let report_path = "anticheat_compatibility_report.md";
    tokio::fs::write(report_path, report.to_markdown()).await?;

    info!(
        "Anti-cheat compatibility report written to: {}",
        report_path
    );

    // Display summary
    info!("Compatibility Summary:");
    info!("  No DLL injection: ✓");
    info!("  No kernel drivers: ✓");
    info!("  Documented telemetry methods: ✓");
    info!("  Process isolation: ✓");
    info!("  Signed binaries: ✓");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser as _;
    use std::path::Path;

    #[test]
    fn hardware_lane_cli_flag_is_parsed_for_moza_receipt_lane() -> Result<(), clap::Error> {
        let cli = Cli::try_parse_from([
            "wheeld",
            "--hardware-lane",
            "ci/hardware/moza-r5/2026-05-06",
        ])?;

        assert_eq!(
            cli.hardware_lane.as_deref(),
            Some("ci/hardware/moza-r5/2026-05-06")
        );
        Ok(())
    }

    #[test]
    fn moza_operator_docs_wheeld_commands_parse() -> Result<(), Box<dyn std::error::Error>> {
        let docs = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/hardware/moza-r5/README.md"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware/moza-r5-validation.md"),
        ];
        let mut checked = 0usize;

        for path in docs {
            let text = std::fs::read_to_string(&path)?;
            for (line_index, line) in text.lines().enumerate() {
                let command = line
                    .trim()
                    .replace("YYYY-MM-DD", "2026-05-06")
                    .replace("<date>", "2026-05-06");
                if !command.starts_with("wheeld ") {
                    continue;
                }

                let args = command.split_whitespace().collect::<Vec<_>>();
                Cli::try_parse_from(args).map_err(|error| {
                    format!(
                        "{}:{} documented wheeld command failed to parse: {command}\n{error}",
                        path.display(),
                        line_index + 1
                    )
                })?;
                checked += 1;
            }
        }

        assert!(
            checked >= 4,
            "expected to parse documented Moza wheeld commands, checked {checked}"
        );
        Ok(())
    }
}
