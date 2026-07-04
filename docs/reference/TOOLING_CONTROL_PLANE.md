# Tooling control plane standard

OpenRacing standardizes on a small upstream substrate and exposes it through
repo-owned `xtask` commands. Upstream tools are the engine room; `xtask` is the
repo-facing control surface.

This standard is documentation-only until a linked plan promotes a wrapper,
workflow, or policy ledger from advisory to required.

## Control-surface doctrine

```text
Do not make upstream tools the repo's public control surface.
Make xtask the repo surface.
Make upstream tools the engine room.
```

Policy should be encoded, exceptions should be receipted, CI should optimize
proof per minute, and `xtask` should enforce the repository contract instead of
scattering policy through workflow YAML or one-off scripts.

## Core upstream substrate

| Plane | Standard upstream tools | Repo-facing role |
| --- | --- | --- |
| Syntax and codemods | `ast-grep`; rust-analyzer crates for Rust-specific authority | Find syntactic candidates, codemod opportunities, non-Rust policy patterns, and agent worklists. Rust-aware tools decide exact Rust identity. |
| Workspace graph | `cargo_metadata`, `guppy` | Inventory packages and targets, compute reverse dependency closures, route risk packs, and plan CI lanes. |
| Test execution | `cargo-nextest`, `cargo test --doc` | Run PR and risk-selected tests with CI-friendly output while keeping doctests on Cargo's native runner. |
| Coverage | `cargo-llvm-cov`, Codecov | Produce execution-surface artifacts and coverage receipts without treating coverage as correctness proof. |
| Mutation | `ripr`, `cargo-mutants` | Shift static mutation-exposure signal left with `ripr`; reserve runtime mutation for targeted PR, nightly, and release lanes. |
| Unsafe and UB evidence | `unsafe-review`, Miri | Make unsafe seams reviewable at PR time; use Miri as a concrete targeted/nightly/release UB witness where it actually runs. |
| Source exceptions | `cargo-allow` | Own source exception ledgers and evidence links. |
| Dependency trust | `cargo-deny`, `cargo-vet`, RustSec / `cargo-audit`, `cargo-auditable` | Gate licenses, advisories, sources, bans, audits, and shipped-binary dependency disclosure. |
| Public API and release | `cargo-semver-checks`, rustdoc JSON | Check release compatibility and generate custom public-surface facts when needed. |
| Workflow policy | `actionlint`, `zizmor` | Lint GitHub Actions correctness and security posture, with exceptions routed through policy receipts. |
| Text and config hygiene | `taplo`, `typos`, Markdown link/style tooling | Keep TOML, spelling, Markdown structure, and links stable. |
| Workspace hygiene | `cargo-udeps` scheduled; `cargo-hakari` only when duplicate-build pain is measured | Keep unused dependency and feature-unification work out of default PR tax unless the workspace needs it. |
| CI cache | `Swatinem/rust-cache` by default; `sccache` only when cache economics justify it | Prefer simple Rust/Cargo caching; add compiler caching only for large or self-hosted matrices. |

## Authority rules

- `ast-grep` finds candidates; Rust-aware tooling decides authoritative Rust
  identity.
- `git ls-files -z` is the default source inventory for file policy and source
  exception scans.
- `cargo_metadata` is the baseline workspace metadata source; `guppy` is the
  richer graph-query substrate for reverse dependencies, feature routing, and CI
  planning.
- `cargo-nextest` is the default serious Rust test runner, but doctests remain a
  separate `cargo test --doc` lane.
- `cargo-llvm-cov` measures execution surface only. It is not a test-adequacy,
  release-readiness, or correctness claim.
- `ripr` is static mutation-exposure analysis. It can route repair packets and
  targeted mutation, but it does not execute mutants or report killed/survived
  outcomes.
- `cargo-mutants` is the runtime mutation backstop. Full-workspace mutation is
  not a default ordinary PR gate.
- `unsafe-review` makes unsafe contracts reviewable. It does not prove memory
  safety, UB freedom, or Miri cleanliness.
- Miri is evidence about concrete executions and should be targeted, scheduled,
  or release-scoped unless a linked plan makes it cheap and required.
- `cargo-deny` is the normal dependency policy gate; `cargo-vet` is a maturity
  layer for high-risk release surfaces; RustSec advisories remain the advisory
  source.
- `cargo-semver-checks` owns ordinary public API compatibility gates; rustdoc
  JSON is for custom surface analysis.

## Stable `xtask` command surface

The repo should expose stable wrappers even when the upstream implementation
changes.

Implemented today (see `crates/tools/src/bin/xtask.rs`):

```bash
cargo xtask pr                        # aggregate PR gate
cargo xtask ripr-pr [--check]         # static mutation-exposure PR evidence
cargo xtask ripr-review-comments [--check]
cargo xtask mutants-pr [...]          # targeted runtime mutation for a PR
cargo xtask quality-closure [--check] [--json-out ..] [--md-out ..]
cargo xtask unsafe-review-closure [--check] [--json-out ..] [--md-out ..]
cargo xtask impacted-evidence         # map changed files to evidence lanes
cargo xtask check-file-policy         # wraps scripts/policy_file.py
cargo xtask docs-sync [--check]
cargo xtask badges [--check]          # regenerate badges/ endpoint JSON
```

Target surface, not yet implemented — running these today fails with
`unknown xtask command`. Each one is a candidate wrapper that a linked plan
may promote:

```bash
cargo xtask check-pr | fix-pr | pr-summary
cargo xtask allow-check | allow-diff | unsafe-review-pr
cargo xtask test-pr | test-risk-pack <pack> | test-docs
cargo xtask coverage | mutation-targeted | miri-targeted
cargo xtask check-deps | check-supply-chain | semver-check
cargo xtask check-workflows | check-toml | policy-report
```

A wrapper may initially be advisory, a placeholder, or absent. Adding or
promoting one to a required gate should happen through the linked
source-of-truth plan for that lane.

## Default lane policy

| Lane | Default posture |
| --- | --- |
| Ordinary PR with Rust behavior changes | Run formatting, Clippy, nextest-backed tests, doctests where relevant, dependency policy, `ripr` PR evidence, and targeted checks selected by risk. |
| Docs or policy-only PR | Run docs/policy validation and avoid heavyweight runtime mutation, full Miri, or hardware gates unless the touched plan requires them. |
| Unsafe-affecting PR | Include unsafe-review evidence and route Miri/property/fake-transport witnesses when the seam and plan justify them. |
| Nightly | Broader mutation, Miri, unused dependency, link, workflow-security, and long-running hygiene checks. |
| Release | Dependency audit/vet evidence, semver compatibility, coverage receipts, mutation/readiness backstops, shipped-binary auditability, and public-surface reports. |

## Install set

Baseline developer and CI tool availability should prefer pinned installers or
locked Cargo installs:

```bash
cargo install cargo-allow --locked
cargo install ripr --locked
cargo install unsafe-review --locked
cargo install cargo-nextest --locked
cargo install cargo-deny --locked
cargo install cargo-llvm-cov --locked
cargo install cargo-semver-checks --locked
cargo install cargo-mutants --locked
cargo install cargo-audit --locked
cargo install taplo-cli --locked
cargo install typos-cli --locked
```

External binaries used by wrappers or workflows include:

```text
ast-grep
actionlint
zizmor
markdownlint-cli2
lychee or markdown-link-check
```

Nightly-only or scheduled tools should stay out of default PR setup unless a
plan explicitly promotes them:

```bash
cargo +nightly install cargo-udeps --locked
rustup +nightly component add miri
```

## Non-standard defaults

Do not globally standardize these as ordinary PR defaults:

- Semgrep as the main repo scanner. Use `ast-grep` and Rust-first tools for the
  repo control plane; Semgrep can remain an external security tool.
- Nix for every repository. Make environment determinism opt-in and receipted.
- Docker for default Rust CI unless the product lane is a container.
- Full-workspace mutation on every PR.
- Full-workspace Miri on every PR.
- `cargo-hakari` or `sccache` without measured workspace or cache economics.

## Contract summary

```text
ast-grep finds syntactic candidates.
cargo_metadata/guppy understand the workspace.
cargo-nextest runs the tests.
cargo-llvm-cov measures execution.
cargo-allow owns exception receipts.
ripr shifts mutation signal left.
unsafe-review makes unsafe reviewable.
cargo-mutants and Miri provide runtime backstops.
cargo-deny/vet/audit own dependency trust.
cargo-semver-checks owns release compatibility.
xtask ties it all into one repo-shaped control plane.
```
