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

use std::io::Write;
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

/// Ensure a manifest-declared asset name is a plain basename that stays inside
/// the bundle directory. The manifest is attacker-controllable input for
/// `--check` (it may come from a downloaded release artifact), so an absolute
/// path or one containing `..`/separators could otherwise make `dir.join(name)`
/// read files outside the bundle (CWE-22). Reject anything that is not a single
/// safe filename component.
fn safe_asset_name(name: &str) -> Result<()> {
    let is_safe = !name.is_empty()
        && Path::new(name).components().count() == 1
        && matches!(
            Path::new(name).components().next(),
            Some(std::path::Component::Normal(_))
        );
    if !is_safe {
        bail!("unsafe bundle asset name {name:?}: must be a plain file name");
    }
    Ok(())
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
    let write = |name: &str, bytes: &[u8]| -> Result<()> {
        let path = out.join(name);
        std::fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))
    };
    write(PROTO_NAME, &proto)?;
    write(FIXTURE_NAME, &fixture)?;
    write(CHECKSUMS_NAME, render_checksums(&manifest.files).as_bytes())?;

    let manifest_json =
        serde_json::to_string_pretty(&manifest).context("failed to serialize contract manifest")?;
    write(MANIFEST_NAME, format!("{manifest_json}\n").as_bytes())?;

    // Route status output through a locked stdout handle rather than the
    // print!/println! macros, which the governance lint forbids in non-test
    // production code (tools bins are not exempt).
    let _ = writeln!(
        std::io::stdout().lock(),
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

    // Cross-check the control-stream feature descriptor against the constants
    // this build advertises, so a bundle that drifted from the pinned feature
    // or capture schema is rejected rather than silently trusted.
    if manifest.control_stream.feature != CONTROL_STREAM_FEATURE {
        bail!(
            "unexpected feature {:?} (expected {CONTROL_STREAM_FEATURE:?})",
            manifest.control_stream.feature
        );
    }
    if manifest.control_stream.capture_schema_version != CONTROL_CAPTURE_SCHEMA_VERSION {
        bail!(
            "unexpected capture schema version {} (expected {CONTROL_CAPTURE_SCHEMA_VERSION})",
            manifest.control_stream.capture_schema_version
        );
    }

    // The wire descriptor must match the pinned package and name it a bundled
    // file whose recorded checksum agrees with the wire descriptor's own.
    if manifest.wire.package != WIRE_PACKAGE {
        bail!(
            "unexpected wire package {:?} (expected {WIRE_PACKAGE:?})",
            manifest.wire.package
        );
    }
    if manifest.wire.proto != PROTO_NAME {
        bail!(
            "unexpected wire proto file name {:?} (expected {PROTO_NAME:?})",
            manifest.wire.proto
        );
    }
    safe_asset_name(&manifest.wire.proto)?;
    let wire_file = manifest
        .files
        .iter()
        .find(|f| f.name == manifest.wire.proto)
        .with_context(|| {
            format!(
                "wire proto {:?} is not listed in the bundle files",
                manifest.wire.proto
            )
        })?;
    if wire_file.sha256 != manifest.wire.sha256 {
        bail!(
            "wire proto checksum disagreement: descriptor {} != file entry {}",
            manifest.wire.sha256,
            wire_file.sha256
        );
    }

    // Compatibility floor must match what this build guarantees.
    if manifest.compatibility.min_service_version != MIN_SERVICE_VERSION {
        bail!(
            "unexpected min_service_version {:?} (expected {MIN_SERVICE_VERSION:?})",
            manifest.compatibility.min_service_version
        );
    }
    if manifest.compatibility.min_client_version != MIN_CLIENT_VERSION {
        bail!(
            "unexpected min_client_version {:?} (expected {MIN_CLIENT_VERSION:?})",
            manifest.compatibility.min_client_version
        );
    }

    // Exactly the pinned control-capture fixture, cross-checked against its
    // bundled file entry.
    match manifest.fixtures.as_slice() {
        [fixture] => {
            if fixture.name != FIXTURE_NAME {
                bail!(
                    "unexpected fixture {:?} (expected {FIXTURE_NAME:?})",
                    fixture.name
                );
            }
            if fixture.schema_version != CONTROL_CAPTURE_SCHEMA_VERSION {
                bail!(
                    "unexpected fixture schema version {} (expected {CONTROL_CAPTURE_SCHEMA_VERSION})",
                    fixture.schema_version
                );
            }
            let fixture_file = manifest
                .files
                .iter()
                .find(|f| f.name == fixture.name)
                .with_context(|| {
                    format!(
                        "fixture {:?} is not listed in the bundle files",
                        fixture.name
                    )
                })?;
            if fixture_file.sha256 != fixture.sha256 {
                bail!(
                    "fixture {} checksum disagreement: descriptor {} != file entry {}",
                    fixture.name,
                    fixture.sha256,
                    fixture_file.sha256
                );
            }
        }
        other => bail!("expected exactly one fixture, found {}", other.len()),
    }

    // Every declared file must have a safe name, be present, and its on-disk
    // checksum must match. The name guard keeps a hostile manifest from making
    // us read outside the bundle directory (CWE-22).
    for file in &manifest.files {
        safe_asset_name(&file.name)?;
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

    // SHA256SUMS must agree with the manifest.
    let sums_path = dir.join(CHECKSUMS_NAME);
    let sums = std::fs::read_to_string(&sums_path)
        .with_context(|| format!("missing {}", sums_path.display()))?;
    if sums != render_checksums(&manifest.files) {
        bail!("{CHECKSUMS_NAME} does not match the manifest file list");
    }

    let _ = writeln!(
        std::io::stdout().lock(),
        "control-stream contract bundle at {} is coherent ({} files, feature {})",
        dir.display(),
        manifest.files.len(),
        manifest.control_stream.feature
    );
    Ok(())
}

fn usage() -> ! {
    let _ = writeln!(
        std::io::stderr().lock(),
        "usage: control-stream-contract --out <dir> | --check <dir>"
    );
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

    /// Load the on-disk manifest, mutate it, and write it back (pretty JSON,
    /// matching `generate`). SHA256SUMS is left untouched so tests can exercise
    /// either the cross-check or the SHA256SUMS agreement path.
    fn rewrite_manifest(dir: &Path, mutate: impl FnOnce(&mut ContractManifest)) -> Result<()> {
        let mut manifest: ContractManifest =
            serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST_NAME))?)?;
        mutate(&mut manifest);
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(dir.join(MANIFEST_NAME), format!("{json}\n"))?;
        Ok(())
    }

    #[test]
    fn safe_asset_name_rejects_traversal_and_absolute_paths() {
        assert!(safe_asset_name("wheel.proto").is_ok());
        for bad in ["../evil", "..", "/etc/passwd", "a/b", "sub/wheel.proto", ""] {
            assert!(safe_asset_name(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn check_rejects_a_path_traversal_asset_name() -> Result<()> {
        let dir = temp_dir("traversal");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        // Point the wire proto (and its file entry) at an out-of-bundle path.
        rewrite_manifest(&dir, |m| {
            m.wire.proto = "../wheel.proto".to_string();
            m.files[0].name = "../wheel.proto".to_string();
        })?;
        assert!(
            check(&dir).is_err(),
            "a manifest naming an out-of-bundle asset must fail validation"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_rejects_contradictory_wire_metadata() -> Result<()> {
        let dir = temp_dir("wire-meta");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        // The wire descriptor's checksum disagrees with its bundled file entry,
        // even though the file on disk is untouched.
        rewrite_manifest(&dir, |m| {
            m.wire.sha256 = "0".repeat(64);
        })?;
        assert!(
            check(&dir).is_err(),
            "contradictory wire checksum must fail validation"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_rejects_a_drifted_capture_schema_version() -> Result<()> {
        let dir = temp_dir("schema-drift");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        rewrite_manifest(&dir, |m| {
            m.control_stream.capture_schema_version = CONTROL_CAPTURE_SCHEMA_VERSION + 1;
        })?;
        assert!(
            check(&dir).is_err(),
            "a capture schema version that drifts from the build must fail"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_rejects_a_renamed_wire_proto() -> Result<()> {
        let dir = temp_dir("wire-rename");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        // Point the wire descriptor at the fixture (a safe basename with a real
        // file entry) instead of the pinned proto name. Coherent internally, but
        // the pinned wire asset drifted, so it must be rejected.
        rewrite_manifest(&dir, |m| {
            let fixture_sha = m.fixtures[0].sha256.clone();
            m.wire.proto = FIXTURE_NAME.to_string();
            m.wire.sha256 = fixture_sha;
        })?;
        assert!(
            check(&dir).is_err(),
            "a wire proto renamed away from the pinned name must fail"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn check_rejects_contradictory_fixture_metadata() -> Result<()> {
        let dir = temp_dir("fixture-meta");
        let _ = std::fs::remove_dir_all(&dir);
        generate(&dir)?;
        rewrite_manifest(&dir, |m| {
            m.fixtures[0].sha256 = "0".repeat(64);
        })?;
        assert!(
            check(&dir).is_err(),
            "a fixture checksum disagreeing with its file entry must fail"
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
