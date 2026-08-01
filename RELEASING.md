# Releasing OpenRacing

This document describes the release process for OpenRacing.

## Release Types

OpenRacing follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html):

- **Alpha releases** (`v0.x.0-alpha`): Early development, API may change significantly
- **Beta releases** (`v0.x.0-beta`): Feature complete for the milestone, API stabilizing
- **Release candidates** (`v0.x.0-rc.N`): Final testing before stable release
- **Stable releases** (`v1.0.0+`): Production-ready, backward compatible within major version

> **Note**: OpenRacing remains in the v0.x.y range until the maintainer provides hardware sign-off on physical hardware. This is independent of feature completeness.

## Prerequisites

Before creating a release:

1. **All CI checks must pass** on the `main` branch
2. **CHANGELOG.md must be updated** with all changes for the release
3. **Version in Cargo.toml** must match the release version (base version for prereleases)
4. **Documentation must be current** for any new features

## Creating a Release

### 1. Update CHANGELOG.md

Ensure the `[Unreleased]` section is moved to a new version section:

```markdown
## [Unreleased]

## [0.1.0] - 2025-01-15

### Added
- New feature description

### Changed
- Changed behavior description

### Fixed
- Bug fix description
```

### 2. Update Version (if needed)

For major/minor releases, update the version in `Cargo.toml`:

```toml
[workspace.package]
version = "0.2.0"
```

### 3. Commit Changes

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "chore: prepare release v0.1.0-alpha"
```

### 4. Create Annotated Tag

Create an annotated git tag with a descriptive message:

```bash
# For alpha release
git tag -a v0.1.0-alpha -m "v0.1.0-alpha: First alpha release

Highlights:
- Core FFB engine with 1kHz real-time processing
- Linux HID support via hidraw/udev
- CLI tool (wheelctl) for device management
- Background service (wheeld) for continuous operation
- Safety system foundation with fault detection

See CHANGELOG.md for full details."

# For stable release
git tag -a v1.0.0 -m "v1.0.0: First stable release

This is the first production-ready release of OpenRacing.

See CHANGELOG.md for full details."
```

### 5. Push Tag

Push the tag to trigger the release workflow:

```bash
git push origin v0.1.0-alpha
```

### 6. Monitor Release Workflow

The GitHub Actions release workflow will automatically:

1. Validate the tag format and version consistency
2. Extract release notes from CHANGELOG.md
3. Build release binaries for Linux (x86_64) and Windows (x64)
4. Create platform-specific packages:
   - Linux: tarball with binaries, systemd service, and udev rules
   - Windows: portable ZIP with binaries
5. Run the Linux control-stream artifact lifecycle smoke against the current
   package and the deterministic prior-lane fixture
6. Generate SHA256 checksums for all artifacts
7. Create a GitHub Release with all artifacts attached

Monitor the workflow at: `https://github.com/EffortlessMetrics/OpenRacing/actions/workflows/release.yml`

## Release Artifacts

Each release includes:

### Linux (x86_64)
- `openracing-{version}-linux-amd64.tar.gz` - Tarball containing:
  - `wheelctl` - CLI tool
  - `wheeld` - Background service
  - `99-racing-wheel-suite.rules` - udev rules
  - `wheeld.service` - systemd service file
  - `install.sh` - Installation script
  - `contract/control-stream/` - External **control-stream contract** bundle:
    the pinned `wheel.proto`, a `control-stream-contract.json` compatibility
    manifest (`control_stream_v1` feature version, capture schema version,
    minimum compatible service/client versions, source checksum), `SHA256SUMS`,
    and a deterministic `sample-capture.json` replay fixture. Lets external
    consumers build a `control_stream_v1` client without depending on
    OpenRacing engine/HID/FFB crates. Generated and coherence-verified at
    package time by `openracing-tools --bin control-stream-contract`.
  - Documentation (README, LICENSE, CHANGELOG)

### Windows (x64)
- `openracing-{version}-windows-x64-portable.zip` - ZIP containing:
  - `wheelctl.exe` - CLI tool
  - `wheeld.exe` - Background service
  - Documentation (README, LICENSE, CHANGELOG)

### Checksums
- `SHA256SUMS.txt` - Combined checksums for all artifacts
- Individual `.sha256` files for each artifact

## Linux artifact lifecycle smoke

The release workflow runs `control-stream-artifact-smoke` before signing
release assets. The harness launches only the extracted package binaries and
contract assets, then proves:

- `wheeld` feature negotiation and descriptor-to-baseline sequencing with
  virtual input only;
- restart and disabled-feature capability behavior;
- replacement and rollback of installed binaries without changing the service
  configuration or a persistent profile sentinel; and
- `wheelctl controls replay` consuming the packaged input-only fixture with
  event, reset, and disconnect evidence.

The repository has no tagged prior release yet, so the workflow builds the
package-complete PR #196 commit (`e625296e`) as a deterministic prior-lane
fixture. This is lifecycle compatibility evidence for the package format, not
a claim that a released version has been tested. A future release lane should
replace that fixture with the previous supported release artifact.

To run the same harness locally after producing two Linux tarballs:

```bash
cargo build --release --locked -p racing-wheel-integration-tests \
  --bin control-stream-artifact-smoke
bash scripts/control_stream_artifact_smoke.sh \
  --current dist/openracing-current-linux-amd64.tar.gz \
  --previous dist/openracing-previous-linux-amd64.tar.gz \
  --harness target/release/control-stream-artifact-smoke
```

This receipt is limited to Linux tarball artifacts, virtual input, and the
input-only replay consumer. It does not establish hardware, output, FFB,
high-torque, named-control, Runbook product, or Windows/macOS support.

## Verifying a Release

Users can verify downloaded artifacts:

```bash
# Linux
sha256sum -c openracing-0.1.0-alpha-linux-amd64.tar.gz.sha256

# Windows (PowerShell)
$expected = (Get-Content openracing-0.1.0-alpha-windows-x64-portable.zip.sha256).Split()[0]
$actual = (Get-FileHash openracing-0.1.0-alpha-windows-x64-portable.zip -Algorithm SHA256).Hash.ToLower()
if ($expected -eq $actual) { "Checksum OK" } else { "Checksum FAILED" }
```

## Hotfix Releases

For critical bug fixes on a released version:

1. Create a release branch from the tag:
   ```bash
   git checkout -b release/v0.1.x v0.1.0
   ```

2. Cherry-pick or apply the fix

3. Update CHANGELOG.md with the fix

4. Create a new patch version tag:
   ```bash
   git tag -a v0.1.1 -m "v0.1.1: Hotfix release"
   git push origin v0.1.1
   ```

## Troubleshooting

### Release workflow fails

1. Check the workflow logs for specific errors
2. Common issues:
   - Tag version doesn't match Cargo.toml version
   - CHANGELOG.md missing entry for the version
   - Build failures (check CI status before tagging)

### Missing changelog entry

If the release notes are empty, ensure CHANGELOG.md has an entry matching the version:

```markdown
## [0.1.0] - 2025-01-15
```

The version in brackets must match the tag version (without the `v` prefix and prerelease suffix).

### Deleting a tag (if needed)

If you need to recreate a tag:

```bash
# Delete local tag
git tag -d v0.1.0-alpha

# Delete remote tag
git push origin :refs/tags/v0.1.0-alpha

# Recreate and push
git tag -a v0.1.0-alpha -m "..."
git push origin v0.1.0-alpha
```

**Note:** Only delete tags that haven't been widely distributed. Once users have downloaded a release, avoid changing it.

## Release Checklist

- [ ] All CI checks pass on `main`
- [ ] CHANGELOG.md updated with all changes
- [ ] Version in Cargo.toml matches release (base version)
- [ ] Documentation updated for new features
- [ ] Changes committed and pushed to `main`
- [ ] Annotated tag created with descriptive message
- [ ] Tag pushed to origin
- [ ] Release workflow completed successfully
- [ ] Linux artifact lifecycle smoke completed successfully
- [ ] GitHub Release created with all artifacts
- [ ] Release announcement prepared (if applicable)

## Future Enhancements

The following release features are planned for future milestones:

- **v0.2.0**: Windows MSI installer with service registration
- **v1.0.0**: 
  - Signed release artifacts (Ed25519)
  - macOS .dmg installer
  - Plugin registry integration

> **Note:** Linux .deb and .rpm packaging scripts already exist in `packaging/linux/build-packages.sh` and can be used for local builds.
> The GitHub Actions release workflow (`.github/workflows/release.yml`) is now in place
> and will be triggered automatically when a version tag is pushed.
