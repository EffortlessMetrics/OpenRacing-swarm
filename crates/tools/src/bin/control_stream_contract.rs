//! Generate and verify the versioned external control-stream contract bundle
//! (issue #179).
//!
//! External consumers of the observe-only `control_stream_v1` stream must be
//! able to build a client without depending on OpenRacing engine/HID/FFB
//! implementation crates. This tool publishes a small, pinned, checksummed
//! bundle they can consume from a release artifact:
//!
//! ```text
//! wheel.proto                     # pinned wire contract (the #171 RPC)
//! sample-capture.json             # deterministic replay fixture (#172)
//! control-stream-contract.json    # compatibility manifest + checksums
//! SHA256SUMS                      # sha256 over the data files
//! ```
//!
//! The manifest records the OpenRacing release, the wire package/namespace, the
//! `control_stream_v1` feature version, the capture schema version, minimum
//! compatible service/client versions, and the source `wheel.proto` checksum,
//! so a consumer can verify coherence and compatibility offline.
//!
//! Usage:
//!
//! ```text
//! control-stream-contract --out <dir>    # write the bundle
//! control-stream-contract --check <dir>  # verify an existing bundle is coherent
//! ```
//!
//! `--check` is the mechanism package validation uses to catch missing or
//! mismatched binary/schema/fixture assets: it fails if any listed file is
//! absent or its checksum does not match the manifest.
//!
//! This is release *composition* only (issue #179): it packages the existing
//! observe-only contract. It opens no device, changes no output/FFB/support
//! behavior, and broadens no claim.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Negotiated control-stream feature name (mirrors
/// `racing_wheel_service::control_stream_wire::CONTROL_STREAM_FEATURE`).
const CONTROL_STREAM_FEATURE: &str = "control_stream_v1";
/// Control-capture schema version (mirrors
/// `wheelctl`'s `CONTROL_CAPTURE_SCHEMA_VERSION`).
const CONTROL_CAPTURE_SCHEMA_VERSION: u32 = 1;
/// Wire protobuf package / service namespace.
const WIRE_PACKAGE: &str = "wheel.v1";
/// Minimum compatible service/client versions (mirrors the service's
/// feature-negotiation `server_version`/`min_client_version`).
const MIN_SERVICE_VERSION: &str = "1.0.0";
const MIN_CLIENT_VERSION: &str = "1.0.0";
/// Contract identifier stamped into the manifest.
const CONTRACT_ID: &str = "openracing-control-stream";

/// Repo-relative source paths the bundle is composed from.
const PROTO_SOURCE: &str = "crates/schemas/proto/wheel.proto";
const FIXTURE_SOURCE: &str = "crates/cli/tests/fixtures/controls/sample.json";

/// Bundle file names.
const PROTO_NAME: &str = "wheel.proto";
const FIXTURE_NAME: &str = "sample-capture.json";
const MANIFEST_NAME: &str = "control-stream-contract.json";
const CHECKSUMS_NAME: &str = "SHA256SUMS";

/// A checksummed file listed in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleFile {
    name: String,
    sha256: String,
}

/// Wire contract descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireContract {
    /// Protobuf package / service namespace, e.g. `wheel.v1`.
    package: String,
    /// Bundled proto file name.
    proto: String,
    /// sha256 of the pinned proto.
    sha256: String,
}

/// Control-stream feature descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlStreamContract {
    /// Negotiated feature name.
    feature: String,
    /// Capture/replay schema version.
    capture_schema_version: u32,
}

/// Minimum-compatible-version descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Compatibility {
    min_service_version: String,
    min_client_version: String,
}

/// A packaged replay fixture descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureEntry {
    name: String,
    kind: String,
    schema_version: u32,
    sha256: String,
}

/// The full contract manifest written to `control-stream-contract.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractManifest {
    contract: String,
    openracing_release: String,
    wire: WireContract,
    control_stream: ControlStreamContract,
    compatibility: Compatibility,
    fixtures: Vec<FixtureEntry>,
    files: Vec<BundleFile>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Locate the workspace root from this crate's manifest dir (…/crates/tools).
fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to locate workspace root from CARGO_MANIFEST_DIR")
}

/// Build the manifest and the two data-file payloads from the repo sources.
fn compose_bundle(root: &Path) -> Result<(ContractManifest, Vec<u8>, Vec<u8>)> {
    let proto_path = root.join(PROTO_SOURCE);
    let fixture_path = root.join(FIXTURE_SOURCE);

    let proto = std::fs::read(&proto_path)
        .with_context(|| format!("failed to read proto source {}", proto_path.display()))?;
    let fixture = std::fs::read(&fixture_path)
        .with_context(|| format!("failed to read fixture source {}", fixture_path.display()))?;

    let proto_sha = sha256_hex(&proto);
    let fixture_sha = sha256_hex(&fixture);

    let manifest = ContractManifest {
        contract: CONTRACT_ID.to_string(),
        openracing_release: env!("CARGO_PKG_VERSION").to_string(),
        wire: WireContract {
            package: WIRE_PACKAGE.to_string(),
            proto: PROTO_NAME.to_string(),
            sha256: proto_sha.clone(),
        },
        control_stream: ControlStreamContract {
            feature: CONTROL_STREAM_FEATURE.to_string(),
            capture_schema_version: CONTROL_CAPTURE_SCHEMA_VERSION,
        },
        compatibility: Compatibility {
            min_service_version: MIN_SERVICE_VERSION.to_string(),
            min_client_version: MIN_CLIENT_VERSION.to_string(),
        },
        fixtures: vec![FixtureEntry {
            name: FIXTURE_NAME.to_string(),
            kind: "control-capture".to_string(),
            schema_version: CONTROL_CAPTURE_SCHEMA_VERSION,
            sha256: fixture_sha.clone(),
        }],
        files: vec![
            BundleFile {
                name: PROTO_NAME.to_string(),
                sha256: proto_sha,
            },
            BundleFile {
                name: FIXTURE_NAME.to_string(),
                sha256: fixture_sha,
            },
        ],
    };

    Ok((manifest, proto, fixture))
}

/// Render the `SHA256SUMS` body (`<hash>  <name>` per line, sorted by name).
fn render_checksums(files: &[BundleFile]) -> String {
    let mut lines: Vec<String> = files
        .iter()
        .map(|f| format!("{}  {}", f.sha256, f.name))
        .collect();
    lines.sort();
    let mut body = lines.join("\n");
    body.push('\n');
    body
}

/// Write the bundle to `out`.
fn generate(out: &Path) -> Result<()> {
    let root = workspace_root()?;
    let (manifest, proto, fixture) = compose_bundle(&root)?;

    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create bundle dir {}", out.display()))?;
    std::fs::write(out.join(PROTO_NAME), &proto)?;
    std::fs::write(out.join(FIXTURE_NAME), &fixture)?;
    std::fs::write(out.join(CHECKSUMS_NAME), render_checksums(&manifest.files))?;

    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(out.join(MANIFEST_NAME), format!("{manifest_json}\n"))?;

    println!(
        "wrote control-stream contract bundle to {} (feature {}, capture schema v{})",
        out.display(),
        manifest.control_stream.feature,
        manifest.control_stream.capture_schema_version
    );
    Ok(())
}

/// Verify that the bundle at `dir` is internally coherent: the manifest parses,
/// every listed file exists, and its on-disk checksum matches. Returns an error
/// describing the first mismatch so package validation can surface it.
fn check(dir: &Path) -> Result<()> {
    let manifest_path = dir.join(MANIFEST_NAME);
    let manifest_raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("missing contract manifest {}", manifest_path.display()))?;
    let manifest: ContractManifest = serde_json::from_str(&manifest_raw)
        .with_context(|| format!("invalid contract manifest {}", manifest_path.display()))?;

    if manifest.contract != CONTRACT_ID {
        bail!(
            "unexpected contract id {:?} (expected {CONTRACT_ID:?})",
            manifest.contract
        );
    }
    if manifest.control_stream.feature != CONTROL_STREAM_FEATURE {
        bail!(
            "unexpected feature {:?} (expected {CONTROL_STREAM_FEATURE:?})",
            manifest.control_stream.feature
        );
    }

    // Every declared file must be present with a matching checksum.
    for file in &manifest.files {
        let path = dir.join(&file.name);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("bundle asset missing: {}", path.display()))?;
        let actual = sha256_hex(&bytes);
        if actual != file.sha256 {
            bail!(
                "checksum mismatch for {}: manifest {} != actual {}",
                file.name,
                file.sha256,
                actual
            );
        }
    }

    // The proto referenced by the wire contract must be one of the files.
    if !manifest.files.iter().any(|f| f.name == manifest.wire.proto) {
        bail!(
            "wire proto {:?} is not listed in the bundle files",
            manifest.wire.proto
        );
    }

    // SHA256SUMS must agree with the manifest.
    let sums_path = dir.join(CHECKSUMS_NAME);
    let sums = std::fs::read_to_string(&sums_path)
        .with_context(|| format!("missing {}", sums_path.display()))?;
    if sums != render_checksums(&manifest.files) {
        bail!("{CHECKSUMS_NAME} does not match the manifest file list");
    }

    println!(
        "control-stream contract bundle at {} is coherent ({} files, feature {})",
        dir.display(),
        manifest.files.len(),
        manifest.control_stream.feature
    );
    Ok(())
}

fn usage() -> ! {
    eprintln!("usage: control-stream-contract --out <dir> | --check <dir>");
    std::process::exit(2);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--out") => {
            let dir = args.get(2).unwrap_or_else(|| usage());
            generate(Path::new(dir))
        }
        Some("--check") => {
            let dir = args.get(2).unwrap_or_else(|| usage());
            check(Path::new(dir))
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("or-contract-{tag}-{}", std::process::id()));
        dir
    }

    #[test]
    fn generated_bundle_is_coherent() -> Result<()> {
        let dir = temp_dir("gen");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;

        // All four files are written.
        for name in [PROTO_NAME, FIXTURE_NAME, MANIFEST_NAME, CHECKSUMS_NAME] {
            assert!(dir.join(name).exists(), "missing {name}");
        }
        // The manifest advertises the feature and capture schema version.
        let manifest: ContractManifest =
            serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST_NAME))?)?;
        assert_eq!(manifest.control_stream.feature, CONTROL_STREAM_FEATURE);
        assert_eq!(
            manifest.control_stream.capture_schema_version,
            CONTROL_CAPTURE_SCHEMA_VERSION
        );
        assert_eq!(manifest.wire.package, WIRE_PACKAGE);
        assert_eq!(
            manifest.compatibility.min_client_version,
            MIN_CLIENT_VERSION
        );
        assert_eq!(manifest.fixtures.len(), 1);

        // check() passes on a freshly generated bundle.
        check(&dir)?;

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_detects_a_missing_asset() -> Result<()> {
        let dir = temp_dir("missing");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        std::fs::remove_file(dir.join(PROTO_NAME))?;
        assert!(check(&dir).is_err(), "missing proto must fail validation");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_detects_a_mismatched_asset() -> Result<()> {
        let dir = temp_dir("mismatch");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        // Corrupt the fixture so its checksum no longer matches the manifest.
        std::fs::write(dir.join(FIXTURE_NAME), b"{\"tampered\":true}")?;
        assert!(
            check(&dir).is_err(),
            "mismatched checksum must fail validation"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn bundle_generation_is_deterministic() -> Result<()> {
        let a = temp_dir("det-a");
        let b = temp_dir("det-b");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        generate(&a)?;
        generate(&b)?;
        for name in [PROTO_NAME, FIXTURE_NAME, MANIFEST_NAME, CHECKSUMS_NAME] {
            assert_eq!(
                std::fs::read(a.join(name))?,
                std::fs::read(b.join(name))?,
                "{name} differs between runs"
            );
        }
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        Ok(())
    }
}
