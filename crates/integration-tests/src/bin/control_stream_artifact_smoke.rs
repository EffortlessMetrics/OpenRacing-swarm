//! Release-artifact lifecycle smoke for the observe-only control stream.
//!
//! This binary deliberately launches only binaries and contract fixtures from
//! extracted package directories. The integration-test binary itself is the
//! harness; it is not the consumer under test. The smoke covers a deterministic
//! prior-lane fixture, replacement/rollback of installed files, persistence of
//! configuration and a profile sentinel, live feature negotiation, restart,
//! disabled-feature behavior, and the packaged input-only replay command.

use anyhow::{Context, Result, bail};
use clap::Parser;
use racing_wheel_schemas::generated::wheel::v1::{
    ControlSubscription, FeatureNegotiationRequest, control_stream_item::Item,
    wheel_service_client::WheelServiceClient,
};
use racing_wheel_service::SystemConfig;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use tonic::Request;

const CONTROL_STREAM_FEATURE: &str = "control_stream_v1";
const SERVICE_ENDPOINT: &str = "http://127.0.0.1:50051";

#[derive(Debug, Parser)]
#[command(about = "Smoke extracted OpenRacing control-stream package artifacts")]
struct Args {
    /// Extracted current package directory.
    #[arg(long)]
    current_package: PathBuf,

    /// Extracted prior package fixture directory.
    #[arg(long)]
    previous_package: PathBuf,
}

struct ServiceProcess {
    child: Child,
    log_path: PathBuf,
}

impl Drop for ServiceProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = request_graceful_shutdown(&mut self.child);
            let _ = self.child.wait();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamSnapshot {
    descriptor_device: String,
    descriptor_controls: usize,
    baseline_states: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    validate_package(&args.current_package).context("current package validation failed")?;
    validate_package(&args.previous_package).context("previous package validation failed")?;

    let sandbox = TempDir::new().context("failed to create artifact smoke sandbox")?;
    let home = sandbox.path().join("home");
    let prefix = sandbox.path().join("install");
    fs::create_dir_all(&home).context("failed to create isolated HOME")?;

    let config_path = home.join(".config/wheel/system.json");
    let mut config = SystemConfig::default();
    config.engine.disable_realtime = true;
    config.development.enable_virtual_devices = true;
    config
        .save_to_path(&config_path)
        .await
        .context("failed to write isolated service configuration")?;
    let config_before = fs::read(&config_path).context("failed to snapshot service config")?;

    let sentinel_path = home.join(".config/racing-wheel-suite/profiles/lifecycle-sentinel.json");
    write_sentinel(&sentinel_path)?;
    let sentinel_before =
        fs::read(&sentinel_path).context("failed to snapshot profile sentinel")?;

    install_package(&args.previous_package, &prefix, &home)
        .context("prior package install failed")?;
    let previous_snapshot = run_enabled_service(&prefix, &home, &config_path, "previous")
        .await
        .context("prior package live probe failed")?;
    assert_persistent_state(
        &config_path,
        &config_before,
        &sentinel_path,
        &sentinel_before,
    )?;

    install_package(&args.current_package, &prefix, &home)
        .context("current package upgrade failed")?;
    assert_persistent_state(
        &config_path,
        &config_before,
        &sentinel_path,
        &sentinel_before,
    )?;
    let current_snapshot = run_enabled_service(&prefix, &home, &config_path, "current").await?;
    if current_snapshot != previous_snapshot {
        bail!(
            "current package stream shape differs from prior fixture: previous={previous_snapshot:?}, current={current_snapshot:?}"
        );
    }

    run_disabled_feature_probe(&prefix, &home, &config_path).await?;
    run_packaged_replay(&prefix, &home)?;

    install_package(&args.previous_package, &prefix, &home)
        .context("rollback package install failed")?;
    assert_persistent_state(
        &config_path,
        &config_before,
        &sentinel_path,
        &sentinel_before,
    )?;
    let rollback_snapshot = run_enabled_service(&prefix, &home, &config_path, "rollback").await?;
    if rollback_snapshot != previous_snapshot {
        bail!(
            "rollback package stream shape differs from prior fixture: previous={previous_snapshot:?}, rollback={rollback_snapshot:?}"
        );
    }

    println!(
        "control-stream artifact smoke passed: prior/current/rollback lifecycle, restart, disabled feature, and packaged replay"
    );
    Ok(())
}

fn validate_package(package: &Path) -> Result<()> {
    if !package.is_dir() {
        bail!("package directory does not exist: {}", package.display());
    }
    for binary in ["wheeld", "wheelctl"] {
        let path = binary_in(package, binary);
        if !path.is_file() {
            bail!("package is missing required artifact {}", path.display());
        }
    }
    #[cfg(unix)]
    if !package.join("install.sh").is_file() {
        bail!(
            "package is missing shipped Linux installer: {}",
            package.join("install.sh").display()
        );
    }
    for relative in [
        "contract/control-stream/control-stream-contract.json",
        "contract/control-stream/wheel.proto",
        "contract/control-stream/sample-capture.json",
        "contract/control-stream/SHA256SUMS",
    ] {
        let path = package.join(relative);
        if !path.is_file() {
            bail!("package is missing required artifact {}", path.display());
        }
    }
    Ok(())
}

fn install_package(package: &Path, prefix: &Path, _home: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let installer = package.join("install.sh");
        if !installer.is_file() {
            bail!(
                "package is missing shipped Linux installer: {}",
                installer.display()
            );
        }
        let status = Command::new("bash")
            .arg(&installer)
            .current_dir(package)
            .env("HOME", _home)
            .env("INSTALL_PREFIX", prefix)
            .env("SKIP_SYSTEMD", "true")
            .env("SKIP_UDEV", "true")
            .env("SKIP_RTKIT", "true")
            .status()
            .context("failed to launch shipped Linux installer")?;
        if !status.success() {
            bail!("shipped Linux installer failed with {status}");
        }
        return Ok(());
    }

    #[cfg(windows)]
    {
        let bin_dir = prefix.join("bin");
        let contract_dir = prefix.join("share/openracing/contract/control-stream");
        fs::create_dir_all(&bin_dir).context("failed to create install bin directory")?;
        fs::create_dir_all(&contract_dir).context("failed to create install contract directory")?;

        for binary in ["wheeld", "wheelctl"] {
            fs::copy(binary_in(package, binary), binary_in(prefix, binary))
                .with_context(|| format!("failed to install {binary}"))?;
        }
        for asset in [
            "control-stream-contract.json",
            "wheel.proto",
            "sample-capture.json",
            "SHA256SUMS",
        ] {
            fs::copy(
                package.join("contract/control-stream").join(asset),
                contract_dir.join(asset),
            )
            .with_context(|| format!("failed to install contract asset {asset}"))?;
        }
        Ok(())
    }
}

fn write_sentinel(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create profile directory")?;
    }
    let mut file = File::create(path).context("failed to create profile sentinel")?;
    file.write_all(br#"{"schema":"lifecycle-sentinel/v1","safe":true}"#)
        .context("failed to write profile sentinel")?;
    Ok(())
}

fn assert_persistent_state(
    config_path: &Path,
    config_before: &[u8],
    sentinel_path: &Path,
    sentinel_before: &[u8],
) -> Result<()> {
    let config_after = fs::read(config_path).context("failed to read persistent service config")?;
    if config_after != config_before {
        bail!("package replacement changed the persistent service configuration");
    }
    let sentinel_after = fs::read(sentinel_path).context("failed to read profile sentinel")?;
    if sentinel_after != sentinel_before {
        bail!("package replacement changed the persistent profile sentinel");
    }
    Ok(())
}

async fn run_enabled_service(
    prefix: &Path,
    home: &Path,
    config_path: &Path,
    label: &str,
) -> Result<StreamSnapshot> {
    let mut service = start_service(prefix, home, config_path, label, false).await?;
    let probe_result = probe_enabled_stream().await;
    let stop_result = stop_service(&mut service);
    let snapshot = probe_result?;
    stop_result?;
    wait_for_port_closed().await?;
    Ok(snapshot)
}

async fn run_disabled_feature_probe(prefix: &Path, home: &Path, config_path: &Path) -> Result<()> {
    let mut service = start_service(prefix, home, config_path, "disabled", true).await?;
    let result = probe_disabled_stream().await;
    let stop_result = stop_service(&mut service);
    result?;
    stop_result?;
    wait_for_port_closed().await
}

async fn start_service(
    prefix: &Path,
    home: &Path,
    config_path: &Path,
    label: &str,
    disabled: bool,
) -> Result<ServiceProcess> {
    let log_path = prefix.join(format!("{label}.log"));
    let log = File::create(&log_path).context("failed to create service log")?;
    let stderr = log.try_clone().context("failed to clone service log")?;
    let mut command = Command::new(binary_in(prefix, "wheeld"));
    command
        .arg("--rt-off")
        .arg("--config")
        .arg(config_path)
        .env("HOME", home)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    if disabled {
        command.arg("--disable-control-stream");
    }
    #[cfg(windows)]
    command.env("LOCALAPPDATA", home.join("localappdata"));

    let child = command
        .spawn()
        .with_context(|| format!("failed to launch packaged wheeld for {label}"))?;
    let mut service = ServiceProcess { child, log_path };
    wait_for_port(&mut service)
        .await
        .with_context(|| format!("packaged wheeld did not become ready for {label}"))?;
    Ok(service)
}

async fn wait_for_port(service: &mut ServiceProcess) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = service
            .child
            .try_wait()
            .context("failed to inspect packaged wheeld")?
        {
            let log = fs::read_to_string(&service.log_path).unwrap_or_default();
            bail!("packaged wheeld exited before readiness ({status}); log:\n{log}");
        }
        if TcpStream::connect("127.0.0.1:50051").is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let log = fs::read_to_string(&service.log_path).unwrap_or_default();
            bail!("timed out waiting for packaged wheeld; log:\n{log}");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_port_closed() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect("127.0.0.1:50051").is_err() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("packaged wheeld port remained available after shutdown");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

fn stop_service(service: &mut ServiceProcess) -> Result<()> {
    if service
        .child
        .try_wait()
        .context("failed to inspect service before shutdown")?
        .is_none()
    {
        request_graceful_shutdown(&mut service.child)
            .context("failed to request packaged wheeld shutdown")?;

        let deadline = Instant::now() + Duration::from_secs(5);
        while service
            .child
            .try_wait()
            .context("failed to inspect packaged wheeld")?
            .is_none()
        {
            if Instant::now() >= deadline {
                service
                    .child
                    .kill()
                    .context("failed to force-stop packaged wheeld")?;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    service
        .child
        .wait()
        .context("failed to reap packaged wheeld")?;
    Ok(())
}

fn request_graceful_shutdown(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .context("failed to send SIGTERM to packaged wheeld")?;
        if !status.success() && child.try_wait()?.is_none() {
            child
                .kill()
                .context("failed to force-stop packaged wheeld")?;
        }
    }

    #[cfg(windows)]
    {
        child.kill().context("failed to stop packaged wheeld")?;
    }

    Ok(())
}

async fn probe_enabled_stream() -> Result<StreamSnapshot> {
    let mut client = connect_client().await?;
    let negotiation = timeout(
        Duration::from_secs(5),
        client.negotiate_features(Request::new(FeatureNegotiationRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec![CONTROL_STREAM_FEATURE.to_string()],
            namespace: "wheel.v1".to_string(),
        })),
    )
    .await
    .context("control-stream negotiation timed out")??
    .into_inner();
    if !negotiation.compatible
        || !negotiation
            .enabled_features
            .iter()
            .any(|feature| feature == CONTROL_STREAM_FEATURE)
    {
        bail!("packaged wheeld did not truthfully negotiate control_stream_v1: {negotiation:?}");
    }

    let response = timeout(
        Duration::from_secs(5),
        client.subscribe_control_stream(Request::new(ControlSubscription {
            device_id: String::new(),
            control_kinds: Vec::new(),
        })),
    )
    .await
    .context("control-stream subscription timed out")??;
    let mut stream = response.into_inner();
    let mut kinds = Vec::new();
    let mut descriptor_device = None;
    let mut descriptor_controls = None;
    let mut baseline_states = None;
    let mut sequences = Vec::new();
    let mut epochs = Vec::new();

    for _ in 0..8 {
        let item = timeout(Duration::from_secs(5), stream.message())
            .await
            .context("control-stream item timed out")??
            .ok_or_else(|| {
                anyhow::anyhow!("packaged control stream ended before expected items")
            })?;
        let metadata = item
            .metadata
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("control-stream item has no metadata"))?;
        let racing_wheel_schemas::generated::wheel::v1::ControlStreamMetadata {
            sequence: item_sequence,
            epoch: item_epoch,
            ..
        } = metadata;
        sequences.push(*item_sequence);
        epochs.push(*item_epoch);
        match item.item.as_ref() {
            Some(Item::Descriptor(descriptor)) => {
                kinds.push("descriptor");
                descriptor_device = descriptor
                    .device
                    .as_ref()
                    .map(|device| device.logical_id.clone());
                descriptor_controls = Some(descriptor.controls.len());
            }
            Some(Item::Baseline(baseline)) => {
                kinds.push("baseline");
                baseline_states = Some(baseline.states.len());
            }
            Some(Item::Event(_)) => kinds.push("event"),
            Some(Item::Reset(_)) => kinds.push("reset"),
            Some(Item::Disconnect(_)) => kinds.push("disconnect"),
            None => bail!("control-stream item has no oneof payload"),
        }
        if descriptor_device.is_some() && baseline_states.is_some() {
            break;
        }
    }

    if kinds.first() != Some(&"descriptor") || kinds.get(1) != Some(&"baseline") {
        bail!("packaged control stream did not start descriptor -> baseline: {kinds:?}");
    }
    let first_sequence = sequences
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("control stream produced no sequence"))?;
    let second_sequence = sequences
        .get(1)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("control stream produced no baseline sequence"))?;
    if second_sequence <= first_sequence {
        bail!("control-stream sequence did not increase: {sequences:?}");
    }
    let first_epoch = epochs
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("control stream produced no epoch"))?;
    let second_epoch = epochs
        .get(1)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("control stream produced no baseline epoch"))?;
    if second_epoch < first_epoch {
        bail!("control-stream epoch regressed: {epochs:?}");
    }

    Ok(StreamSnapshot {
        descriptor_device: descriptor_device
            .ok_or_else(|| anyhow::anyhow!("control stream produced no descriptor"))?,
        descriptor_controls: descriptor_controls
            .ok_or_else(|| anyhow::anyhow!("control stream descriptor had no controls"))?,
        baseline_states: baseline_states
            .ok_or_else(|| anyhow::anyhow!("control stream produced no baseline"))?,
    })
}

async fn probe_disabled_stream() -> Result<()> {
    let mut client = connect_client().await?;
    let negotiation = timeout(
        Duration::from_secs(5),
        client.negotiate_features(Request::new(FeatureNegotiationRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec![CONTROL_STREAM_FEATURE.to_string()],
            namespace: "wheel.v1".to_string(),
        })),
    )
    .await
    .context("disabled-feature negotiation timed out")?
    .context("disabled-feature negotiation failed")?
    .into_inner();
    if negotiation
        .enabled_features
        .iter()
        .any(|feature| feature == CONTROL_STREAM_FEATURE)
    {
        bail!("disabled packaged service advertised control_stream_v1");
    }
    let error = timeout(
        Duration::from_secs(5),
        client.subscribe_control_stream(Request::new(ControlSubscription {
            device_id: String::new(),
            control_kinds: Vec::new(),
        })),
    )
    .await
    .context("disabled-feature subscription timed out")?
    .err()
    .ok_or_else(|| anyhow::anyhow!("disabled packaged service accepted control stream"))?;
    if error.code() != tonic::Code::Unimplemented {
        bail!("disabled control stream returned unexpected status: {error}");
    }
    Ok(())
}

async fn connect_client() -> Result<WheelServiceClient<tonic::transport::Channel>> {
    timeout(
        Duration::from_secs(5),
        WheelServiceClient::connect(SERVICE_ENDPOINT.to_string()),
    )
    .await
    .context("gRPC connection timed out")?
    .context("failed to connect to packaged wheeld")
}

fn run_packaged_replay(prefix: &Path, home: &Path) -> Result<()> {
    let wheelctl = binary_in(prefix, "wheelctl");
    let capture = prefix.join("share/openracing/contract/control-stream/sample-capture.json");
    let output = prefix.join("packaged-replay.json");
    let status = Command::new(&wheelctl)
        .args([
            "controls",
            "replay",
            capture
                .to_str()
                .context("capture path is not valid UTF-8")?,
            "--json-out",
            output
                .to_str()
                .context("replay output path is not valid UTF-8")?,
        ])
        .env("HOME", home)
        .status()
        .context("failed to launch packaged wheelctl replay")?;
    if !status.success() {
        bail!("packaged wheelctl replay failed with {status}");
    }
    let value: Value = serde_json::from_slice(
        &fs::read(&output).context("packaged wheelctl did not write replay output")?,
    )
    .context("packaged replay output was not valid JSON")?;
    let entries = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("packaged replay output was not an array"))?;
    let has_event = entries.iter().any(|entry| entry.get("Event").is_some());
    let has_disconnect_reset = entries.iter().any(|entry| {
        entry
            .get("Reset")
            .and_then(|reset| reset.get("reason"))
            .and_then(Value::as_str)
            == Some("Disconnect")
    });
    if !has_event || !has_disconnect_reset {
        bail!("packaged replay output lacks Event/Reset Disconnect evidence: {entries:?}");
    }
    Ok(())
}

fn binary_in(root: &Path, name: &str) -> PathBuf {
    let unix_path = root.join("bin").join(name);
    if unix_path.is_file() {
        return unix_path;
    }
    root.join("bin").join(format!("{name}.exe"))
}
