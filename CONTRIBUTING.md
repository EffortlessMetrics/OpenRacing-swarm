# Contributing to OpenRacing

The full guide lives in [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md), with
environment and tooling detail in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
This page is the short version: what to do before your first pull request, and
the handful of rules that surprise people.

## Get it building

```bash
# Linux only: the workspace links against libudev
sudo apt-get install -y libudev-dev pkg-config   # Debian/Ubuntu

git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing
cargo build --workspace
cargo test --all-features --workspace
```

`rust-toolchain.toml` pins the nightly this repo builds with, so rustup picks it
up automatically. Before opening a pull request:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

## Rules that are easy to trip over

These are enforced, not stylistic preferences.

- **No `unwrap()` or `expect()` in tests.** Return `Result` from the test
  (`fn foo() -> Result<(), Error>`) or assert explicitly. A panicking test
  hides which assertion actually failed. See
  [docs/NO_PANIC_POLICY.md](docs/NO_PANIC_POLICY.md).
- **No heap allocation in the real-time path.** No `Vec`, `HashMap`, or `String`
  after initialization — use fixed-size arrays and pre-allocated buffers. No
  blocking I/O, locks, or syscalls in RT hot paths either.
- **No `static mut`.** Use `OnceLock`, `LazyLock`, or atomics. Non-test crates
  carry `#![deny(static_mut_refs)]`.
- **Performance gates are CI-enforced**: P99 jitter ≤0.25ms at 1kHz, processing
  ≤50µs median and ≤200µs p99, zero RT-path allocations. See
  [docs/PERFORMANCE_GATES.md](docs/PERFORMANCE_GATES.md).
- **Don't claim what you didn't verify.** If a command reports success, it must
  have done the thing. This repo has shipped commands that printed "applied"
  while sending nothing; that is treated as a bug, not a stub.

## Scope of a pull request

OpenRacing tracks work through a linked chain:

```text
Roadmap → Proposal → Spec → ADR → Plan → Active goal → PR → Proof
```

One work item per pull request. Don't mix a proposal, a spec, an ADR, and a
runtime change into one branch, and don't hand-edit generated status files.
[docs/reference/SPEC_SYSTEM.md](docs/reference/SPEC_SYSTEM.md) describes the
stack in full.

Architecturally significant changes need an ADR — see
[docs/adr/README.md](docs/adr/README.md) for the process, and run
`cargo run -p openracing-tools --bin validate-adr -- --verbose` before pushing.

If you hit a missing linked artifact, proof you cannot run, or a conflict with
an existing ADR, say so in the pull request rather than working around it.

## Reporting things

- **Bugs and feature requests**: [GitHub Issues](https://github.com/EffortlessMetrics/OpenRacing/issues)
- **Security vulnerabilities**: do not open a public issue — see
  [SECURITY.md](SECURITY.md)
- **Hardware validation**: this project is pre-validation, and reports from real
  hardware are the most valuable thing you can contribute. Include device
  make/model and firmware, OS version, simulator and version, connection mode,
  logs, and reproduction steps.
