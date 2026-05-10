# OpenRacing Documentation

Welcome to the OpenRacing documentation. This guide will help you navigate the available resources for using, developing, and extending OpenRacing.

## Quick Links

| I want to... | Go to |
|--------------|-------|
| Get started quickly | [User Guide](USER_GUIDE.md) |
| Set up a development environment | [Development Guide](DEVELOPMENT.md) |
| Contribute to the project | [Contributing Guide](CONTRIBUTING.md) |
| Understand the architecture | [System Integration](SYSTEM_INTEGRATION.md) |
| Create a plugin | [Plugin Development](PLUGIN_DEVELOPMENT.md) |
| Optimize system performance | [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) |

---

## Documentation Overview

### For Users

- **[User Guide](USER_GUIDE.md)** - Complete guide to installing, configuring, and using OpenRacing
- **[Power Management Guide](POWER_MANAGEMENT_GUIDE.md)** - Optimize your system for real-time performance
- **[Anticheat Compatibility](ANTICHEAT_COMPATIBILITY.md)** - Information about anti-cheat system compatibility

### For Developers

- **[Development Guide](DEVELOPMENT.md)** - Setting up your development environment, coding standards, and workflow
- **[Contributing Guide](CONTRIBUTING.md)** - How to contribute to OpenRacing
- **[Plugin Development](PLUGIN_DEVELOPMENT.md)** - Creating WASM and native plugins
- **[Schema Governance](SCHEMA_GOVERNANCE.md)** - Schema versioning and evolution policies
- **[Migration Patterns](MIGRATION_PATTERNS.md)** - Handling schema migrations and backward compatibility

### Architecture & Design

- **[System Integration](SYSTEM_INTEGRATION.md)** - Overall system architecture and integration patterns
- **[Architecture Decision Records](adr/INDEX.md)** - Design decisions and technical rationale

### Device Protocols

Detailed documentation for supported racing wheel protocols:

- **[Protocol Knowledge Base](protocols/README.md)** - Overview of supported devices
- **[Fanatec Protocol](protocols/FANATEC_PROTOCOL.md)** - Fanatec wheel communication
- **[Logitech Protocol](protocols/LOGITECH_PROTOCOL.md)** - Logitech G-series wheels
- **[Thrustmaster Protocol](protocols/THRUSTMASTER_PROTOCOL.md)** - Thrustmaster T-series wheels
- **[Moza Protocol](protocols/MOZA_PROTOCOL.md)** - Moza Racing wheels
- **[Simagic Protocol](protocols/SIMAGIC_PROTOCOL.md)** - Simagic wheels

### Hardware Validation

Receipt-backed hardware validation lanes:

- **[Moza R5 Validation Lane](hardware/moza-r5-validation.md)** - Steven's R5 + KS/ES + SR-P + HBP bring-up lane
- **[Moza Validation Matrix](hardware/moza-validation-matrix.md)** - separates source research from real hardware receipts
- **[Moza R5 Artifact Checklist](hardware/moza-r5-artifact-checklist.md)** - maps every Moza bring-up claim to receipts and verifier gates

### Architecture Decision Records (ADRs)

ADRs document significant architectural decisions:

| ADR | Title |
|-----|-------|
| [0001](adr/0001-ffb-mode-matrix.md) | Force Feedback Mode Matrix |
| [0002](adr/0002-ipc-transport.md) | IPC Transport Layer |
| [0003](adr/0003-owp1-protocol.md) | OWP-1 Protocol Specification |
| [0004](adr/0004-rt-scheduling-architecture.md) | Real-Time Scheduling Architecture |
| [0005](adr/0005-plugin-architecture.md) | Plugin Architecture |
| [0006](adr/0006-safety-interlocks.md) | Safety Interlocks and Fault Management |
| [0007](adr/0007-multi-vendor-hid-protocol-architecture.md) | Multi-Vendor HID Protocol Architecture |
| [0008](adr/0008-game-auto-configure-telemetry-bridge.md) | Game Auto-Configure and Telemetry Bridge |
| [0009](adr/0009-hardware-validation-evidence-state-machine.md) | Hardware Validation Evidence State Machine |

See the [ADR Index](adr/INDEX.md) for more details and the [ADR README](adr/README.md) for guidelines on creating new ADRs.

---

## Crate Documentation

Each workspace crate has its own documentation:

| Crate | Description | Documentation |
|-------|-------------|---------------|
| `cli` | Command-line interface (`wheelctl`) | [README](../crates/cli/README.md) |
| `engine` | Core force feedback engine | See [ADR-0004](adr/0004-rt-scheduling-architecture.md) |
| `plugins` | Plugin system (WASM + native) | See [ADR-0005](adr/0005-plugin-architecture.md), [Plugin Development](PLUGIN_DEVELOPMENT.md) |
| `schemas` | Protocol and schema definitions | [README](../crates/schemas/README.md) |
| `telemetry-core` | Telemetry domain models and disconnection state | [README](../crates/telemetry-core/README.md) |
| `telemetry-support` | Shared telemetry game support matrix | [README](../crates/telemetry-support/README.md) |
| `telemetry-contracts` | Shared normalized telemetry contracts | [README](../crates/telemetry-contracts/README.md) |
| `telemetry-adapters` | Game-specific telemetry protocol adapters | [README](../crates/telemetry-adapters/README.md) |
| `telemetry-config-writers` | Game telemetry configuration file writers | [README](../crates/telemetry-config-writers/README.md) |
| `telemetry-integration` | Matrix parity comparison and runtime coverage reporting | [README](../crates/telemetry-integration/README.md) |
| `telemetry-bdd-metrics` | Policy-aware BDD parity counters and ratios | [README](../crates/telemetry-bdd-metrics/README.md) |
| `telemetry-orchestrator` | Matrix-driven telemetry adapter orchestration | [README](../crates/telemetry-orchestrator/README.md) |
| `telemetry-rate-limiter` | Rate limiting primitives for telemetry stream ingestion | [README](../crates/telemetry-rate-limiter/README.md) |
| `telemetry-recorder` | Telemetry recording, replay, and fixture generation | [README](../crates/telemetry-recorder/README.md) |
| `service` | Background service and IPC | See [ADR-0002](adr/0002-ipc-transport.md), [System Integration](SYSTEM_INTEGRATION.md) |
| `ui` | User interface components | See [ADR-0006](adr/0006-safety-interlocks.md) |
| `compat` | Compatibility layer | [README](../crates/compat/README.md) |
| `integration-tests` | Integration test suite | [README](../crates/integration-tests/README.md) |

---

## Getting Help

- **Issues**: Report bugs at [GitHub Issues](https://github.com/EffortlessMetrics/OpenRacing/issues)
- **Discussions**: Join the community at [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions)
- **CLI Help**: Run `wheelctl --help` for command-line help
- **Support Bundles**: Generate diagnostic info with `wheelctl diag support`

---

## Document Maintenance

Documentation is validated as part of CI:

```bash
# Validate ADRs
cargo run -p openracing-tools --bin validate-adr -- --verbose

# Generate ADR index
cargo run -p openracing-tools --bin generate-docs-index --

# Build API documentation
cargo doc --all-features --workspace
```
