# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.


## Repo Source-of-Truth Stack

OpenRacing uses a linked source-of-truth stack:

```text
Roadmap → Proposal → Spec → ADR → Plan → Active goal → PR → Proof
```

Before making source-of-truth-managed changes, read:

1. `docs/reference/SPEC_SYSTEM.md`
2. `.openracing/goals/active.toml` when it exists
3. the linked implementation plan
4. the linked spec for the selected work item
5. any linked ADRs

Work on exactly one work item at a time. Do not create a new lane, mix proposal/spec/ADR/plan/runtime changes, hand-edit generated status, or claim success without proof unless the user or linked plan explicitly requires it. Stop and report when linked artifacts are missing, proof cannot run, generated status is dirty, unrelated staged files exist, or the requested work conflicts with an ADR.

## Project Overview

OpenRacing is a high-performance, safety-critical racing wheel and force feedback simulation software built in Rust. The real-time (RT) path runs at 1kHz with strict latency and allocation rules. Plugins support both WASM (safe, sandboxed) and native (fast, RT) implementations.

## Build and Test Commands

```bash
# Build
cargo build --workspace                    # Debug build
cargo build --release --workspace          # Release build
cargo build --profile rt --bin wheeld      # RT profile for real-time components

# Test
cargo test --all-features --workspace      # All tests
cargo test test_name --package racing-wheel-engine  # Single test
cargo nextest run --all-features --workspace  # Faster test runner (requires cargo-nextest)

# Lint and format
cargo fmt --all                            # Format code
cargo clippy --all-targets --all-features -- -D warnings  # Lint

# Benchmarks and performance
# Generate bench_results.json first, then validate:
BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing
python scripts/validate_performance.py bench_results.json --strict  # Performance gates

# Documentation
cargo doc --all-features --workspace       # Build docs
cargo run -p openracing-tools --bin validate-adr -- --verbose   # Validate ADRs
cargo run -p openracing-tools --bin generate-docs-index --      # Generate docs index

# Dependency checks
cargo deny check                           # Security and license compliance
cargo audit                                # Vulnerability scanning
```

## Architecture

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `schemas` | Shared data structures, protobuf definitions, JSON schemas (foundational, no deps) |
| `engine` | Core RT force feedback processing, device communication |
| `service` | Background daemon, IPC (gRPC/Unix socket), game integration, telemetry |
| `plugins` | Plugin loading: WASM runtime + native plugin ABI |
| `cli` | Command-line tool for user interaction (via IPC to service) |
| `ui` | UI components, safety displays |
| `compat` | Legacy API compatibility, migration helpers |
| `integration-tests` | End-to-end tests, performance gates, soak tests |

**Dependency Flow:** `schemas` → `engine`/`service`/`plugins`/`cli`/`ui`/`compat` → `integration-tests`

### Real-Time Processing Pipeline

```
Input → Filter Pipeline → Safety Checks → Output
 50μs    200μs (median)      50μs        100μs
         800μs (p99)
```

Total budget: 1000μs @ 1kHz. Jitter P99 must be ≤0.25ms.

### Plugin System

- **WASM plugins**: 60-200Hz, sandboxed, capability-based permissions, automatic crash recovery
- **Native plugins**: 1kHz (RT path), isolated helper process, microsecond timing budgets, Ed25519 code signing required

## Critical Engineering Rules

### Real-Time Path Constraints

- **No heap allocations** after initialization (no `Vec`, `HashMap`, `String` in RT path)
- **No blocking operations**: no I/O, locks, or syscalls in RT hot paths
- **Bounded execution**: all RT operations must have deterministic timing
- Use fixed-size arrays `[T; N]` and pre-allocated buffers `Box<[T; N]>`

### Memory Safety

- **No `static mut`**: use `OnceLock`, `LazyLock`, atomics, or other safe patterns
- All non-test crates must include `#![deny(static_mut_refs)]`

### Testing Rules

- **No `unwrap()`/`expect()` in tests**: test code must not rely on panics. Prefer `Result`-returning tests (e.g. `#[test] fn foo() -> Result<(), Error>`), use explicit assertions (`assert!(result.is_ok())`, `assert_eq!`), or test helper macros. Avoid `unwrap()` and `expect()` to ensure clearer failures and avoid masking errors.

### Performance Gates (CI enforced)

- P99 Jitter: ≤0.25ms at 1kHz
- Missed Ticks: ≤0.001% rate
- Processing Time: ≤50μs median, ≤200μs p99
- Memory: Zero heap allocations in RT path

### Safety Interlocks

- Fault detection time: ≤10ms
- Response time: ≤50ms (fault to safe state)
- Multi-layer system: physical interlock, software challenge-response, fault detection

## Architecture Decision Records

Significant architectural changes require an ADR. See `docs/adr/README.md` for the process.

Current ADRs:
- ADR-0001: Force Feedback Mode Matrix
- ADR-0002: IPC Transport Layer (gRPC + Unix sockets)
- ADR-0003: OWP-1 Protocol Specification
- ADR-0004: Real-Time Scheduling Architecture
- ADR-0005: Plugin Architecture
- ADR-0006: Safety Interlocks and Fault Management
- ADR-0007: Multi-Vendor HID Protocol Architecture
- ADR-0008: Game Auto-Configure and Telemetry Bridge

## Platform Considerations

- Targets Windows 10+, Linux kernel 4.0+, and macOS 10.15+
- Avoid OS-specific assumptions unless the module is platform-specific
- RT scheduling uses platform-specific APIs: MMCSS on Windows, SCHED_FIFO on Linux, thread_policy_set on macOS
- Linux requires udev rules: `packaging/linux/99-racing-wheel-suite.rules`

## Key Scripts

- `scripts/validate_performance.py` - Performance gate validation
- `cargo run -p openracing-tools --bin validate-adr --` - ADR format and reference validation
- `cargo run -p openracing-tools --bin generate-docs-index --` - Documentation index generation
- `scripts/analyze_compat_trend.py` - Track compatibility debt

## Dependency Management

- Use workspace dependencies (see root `Cargo.toml`)
- Update `Cargo.lock` when adding/updating dependencies
- Check `deny.toml` for allowed licenses (MIT, Apache-2.0, BSD) and advisories
