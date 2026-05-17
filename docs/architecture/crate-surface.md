# OpenRacing Crate Surface

## Doctrine

OpenRacing uses one rule for package boundaries:

```text
Cargo package boundary = public support promise
Rust module boundary   = SRP / ownership / agent-context boundary
```

A separate `Cargo.toml` is not just an organization tool. It creates a semver surface, a docs.rs page, package metadata, a feature matrix, a release-order node, and a support promise. The durable states are therefore:

- a real public package with a durable external contract;
- an internal package with `publish = false` for repository tools, tests, examples, and dev-only support;
- a module family under an owner crate.

Production-path implementation seams should not stay as separate `publish = false` microcrates. Instead, design seams like microcrates, implement most seams as module families, and publish only durable public contracts.

## Target Public Packages

The target public support surface contains 18 packages:

| Package | Public contract |
| --- | --- |
| `openracing` | Facade SDK and stable start-here crate. |
| `openracing-engine` | Runtime engine, RT loop, device orchestration, and safety execution. |
| `openracing-ffb` | Force-feedback effects, force models, filters, and compiled output plans. |
| `openracing-calibration` | Reusable axis, deadzone, and normalization kernel. |
| `openracing-curves` | Reusable LUT, Bezier, and remap math. |
| `openracing-profile` | Profile, tuning, input-map, preset, and serialization contract. |
| `openracing-hid` | Generic HID transport, descriptor/capture/replay, and vendor protocol family. |
| `openracing-pidff` | Cross-vendor HID PIDFF report and safety layer. |
| `openracing-moza` | Receipt-backed Moza family: R5, KS, ES, SR-P, HBP, and native PIDFF support. |
| `openracing-firmware-update` | Firmware and update safety boundary. |
| `openracing-plugin-abi` | Hard plugin ABI contract. |
| `openracing-plugin-sdk` | Plugin-author SDK with no host/runtime internals. |
| `openracing-telemetry` | Normalized telemetry model, traits, streams, and orchestration. |
| `openracing-telemetry-adapters` | Game adapter family with its own cadence. |
| `openracing-telemetry-config` | Support matrix and config writers. |
| `openracing-telemetry-recorder` | Recording, playback, fixtures, and replay. |
| `openracing-service` | `wheeld` product package. |
| `wheelctl` | Operator CLI product package. |

`openracing-curves` remains public because it is reusable math. `openracing-moza` remains public because the Moza family is receipt-backed and validated enough to be a hardware-family contract rather than a generic HID leaf.

## Facade and Naming Spine

`openracing` is the start-here SDK facade. It owns stable public module names,
not implementation placement. The facade may re-export durable public crates
behind product-oriented features, but it must stay thin and must not become a
dumping ground for runtime, HID, service, simulator, or vendor-specific
implementation details.

The initial facade introduces these feature families:

| Feature | Module | Backing package today |
| --- | --- | --- |
| `calibration` | `openracing::calibration` | `openracing-calibration` |
| `curves` | `openracing::curves` | `openracing-curves` |
| `ffb` | `openracing::ffb` | `openracing-ffb` |
| `profile` | `openracing::profile` | `openracing-profile` |
| `plugin-abi` | `openracing::plugin_abi` | `openracing-plugin-abi` |
| `engine` | `openracing::engine` | `racing-wheel-engine` during transition |

The facade deliberately does not create `hid`, `pidff`, `moza`, or telemetry
modules until those owner packages exist. Later migration PRs should add those
facade modules only after the corresponding public package has landed or been
promoted.

Naming rules during the transition:

- prefer the final public names in new docs and examples;
- keep historical package names only where they describe current Cargo reality;
- add facade modules as thin re-exports, not as code movement;
- do not add migration-only feature names that match old microcrate seams;
- do not use the facade to make Pit House, SimHub, simulator, or hardware-output
  paths prerequisites for native OpenRacing control.

## Internal Packages

Internal packages are limited to tools, test crates, compatibility fixtures, examples, workspace machinery, and development-only support packages. They must have `publish = false` and should have a description that makes their internal role clear.

The initial internal set is declared in `policy/crate-boundaries.toml` and includes repository tools, integration tests, UI/dev packages, compatibility tests, plugin examples, test helpers, and `workspace-hack`.

## Collapse Map

The collapse map is the machine-readable list of current workspace packages that should stop being public package seams in later PRs. Each entry records:

- `from`: the current package name;
- `to`: the target module-family path;
- `owner`: the final public owner package;
- `reason`: why the seam belongs under that owner.

The first policy PR does not move or rename code. It only freezes the map so later telemetry, HID, Moza, FFB, engine-helper, profile, plugin, and app-boundary PRs have a shared pass/fail target.

## Module-Family Standard

Every collapsed seam keeps a crate-grade folder boundary:

```text
src/<family>/
  mod.rs
  error.rs
  types.rs
  state.rs
  validate.rs
  encode.rs
  decode.rs
  tests.rs
  fixtures/
  BOUNDARY.md
```

`BOUNDARY.md` uses this template:

```text
# Boundary: <family>

Owner:
Purpose:
Public façade:
Internal modules:
Allowed dependencies:
Forbidden dependencies:
Invariants:
Tests:
Non-goals:
Migration source:
```

Rules:

- `mod.rs` is the family façade.
- Siblings import through the façade.
- Internals are private or `pub(crate)`.
- Do not use `pub use *` from implementation modules.
- Do not take cross-family deep imports.
- Do not create a new `Cargo.toml` for a family unless policy marks it public.

## Dependency Layering

The target dependency layers are:

```text
openracing
  may depend on public library crates only

openracing-service / wheelctl
  may depend on public libraries and platform/system crates
  may not be dependencies of public library crates

openracing-engine
  may depend on openracing-ffb, openracing-hid, openracing-pidff,
  openracing-moza, openracing-profile, telemetry, plugin-abi
  may not depend on wheelctl or service

openracing-ffb
  may depend on calibration, curves, profile
  may not depend on engine, service, wheelctl, HID transport

openracing-hid
  may depend on pidff only for generic PIDFF helpers if needed
  may not depend on engine, service, wheelctl, telemetry

openracing-moza
  may depend on hid, pidff, curves/calibration if needed
  may not depend on engine, service, wheelctl

openracing-pidff
  may not depend on Moza, engine, service, wheelctl

openracing-plugin-abi
  must stay low-level and host-independent

openracing-plugin-sdk
  may depend on plugin-abi and public model crates
  may not depend on native/WASM host runtime internals
```

The first checker encodes package classification and obvious dependency hazards. Later PRs can tighten this into full layer-specific enforcement.

## Feature Policy

Features are public API. They must be additive, product-oriented, and stable enough to support. Use product features such as:

```toml
[features]
default = ["std"]
std = []
serde = ["dep:serde"]
moza = ["dep:openracing-moza"]
telemetry = ["dep:openracing-telemetry"]
plugins = ["dep:openracing-plugin-sdk"]
```

Avoid extraction and migration features such as:

```toml
openracing-scheduler = []
openracing-pipeline = []
telemetry-lfs-crate = []
hid-moza-protocol = []
```

The checker warns when feature names match former microcrate names so accidental public seams do not reappear as feature surfaces.

## Migration Sequence

The planned ladder is:

1. `policy-crate-surface`: add this document, `policy/crate-boundaries.toml`, the workspace publish allowlist, `default-members`, and `package-surface` enforcement.
2. `facade-naming-spine`: introduce the `openracing` facade and naming transition.
3. `telemetry-finish`: finish telemetry consolidation into four public telemetry packages.
4. `pidff-promote`: rename and stabilize PIDFF.
5. `hid-core-collapse`: create `openracing-hid` and move generic HID support.
6. `moza-family-collapse`: create `openracing-moza` and move Moza leaves.
7. `hid-vendor-collapse`: move remaining vendor protocol leaves under `openracing-hid::protocol::*`.
8. `ffb-collapse`: collapse FFB implementation satellites into `openracing-ffb`.
9. `engine-helper-collapse`: collapse engine-owned RT, observability, safety, hardware-evidence, diagnostics, and error helpers.
10. `profile-collapse`: move input maps and profile repository under `openracing-profile`.
11. `plugin-sdk-split`: keep the ABI/SDK public and move host runtimes under service plugins.
12. `app-boundary`: classify or move app/tool/UI packages.
13. `delete-old-packages`: remove old package directories after imports are clean.
14. `package-proof`: run package proof for every final public package.

## Packaging Proof

Each final public package must eventually pass:

```text
cargo test -p <crate> --all-features --locked
cargo clippy -p <crate> --all-targets --all-features --locked -- -D warnings
cargo doc -p <crate> --all-features --no-deps --locked
cargo package -p <crate> --list
cargo publish -p <crate> --dry-run --locked
```

Each final public package must also have complete package metadata: workspace version, edition, rust-version, license, authors, repository, homepage, documentation, README, description, keywords, categories, and `publish = true`.

## Non-Goals

The first policy PR intentionally does not:

- move code;
- rename packages;
- alter Moza hardware artifacts;
- run hardware-output commands;
- collapse telemetry, HID, FFB, Moza, profile, engine-helper, or plugin crates;
- combine this with CI optimization, Clippy-policy changes, or hardware-lane work;
- use `publish = false` as a substitute for collapsing production-path crates.
