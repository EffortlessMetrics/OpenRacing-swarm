# wheelctl - Racing Wheel Control CLI

The command-line interface for the Racing Wheel Software Suite: managing racing wheel hardware, profiles, diagnostics, and game integration.

## Features

- **Device Management**: List, status, calibration, and reset operations
- **Profile Management**: Create, edit, validate, import/export profiles
- **Diagnostics**: System tests, blackbox recording/replay, support bundles, metrics
- **Game Integration**: Configure telemetry, test connections, manage game support
- **Safety Controls**: High torque mode, emergency stop, safety status
- **Health Monitoring**: Service and device health monitoring
- **JSON Output**: Machine-readable output for all commands (`--json` flag)
- **Shell Completion**: Bash, Zsh, Fish, and PowerShell completion scripts
- **Error Handling**: Distinct exit codes per error type
- **Verbose Logging**: Configurable logging levels (`-v`, `-vv`, `-vvv`)

### Known gaps

The project is pre-validation and some commands are scaffolding. Listed here
rather than under a "complete implementation" heading, because a command that
reports success without acting is worse than one that is missing:

- `wheelctl safety limit` has no torque-limit write on the IPC surface. It
  fails rather than claiming success; use the wheelbase's physical limit.
- `wheelctl profile apply` transmits only what the IPC contract represents.
  Schema-only fields — `base.filters.torqueCap`, `bumpstop`, `handsOff`, LED
  colours, haptic effects, signatures — are rejected with an explicit error
  rather than silently dropped, so a profile using them will not apply until
  the contract covers them.
- Device I/O is not implemented on macOS.

### Command Structure

```
wheelctl [OPTIONS] <COMMAND>

Commands:
  device          Device management commands
  controls        Observe-only control-stream diagnostics, capture, and replay
  profile         Profile management commands
  plugin          Plugin management commands
  diag            Diagnostic and monitoring commands
  game            Game integration commands
  telemetry       Telemetry probe and capture commands
  hardware        Hardware environment diagnostics
  safety          Safety and control commands
  support-bundle  Generate diagnostic support bundle
  completion      Generate shell completion scripts
  health          Service health and status
```

`wheelctl` is a client. It talks to the `wheeld` service over IPC, so `wheeld`
must be running for anything that touches real hardware.

#### Hidden namespaces

`moza` is not listed in `wheelctl --help`. It holds 55 subcommands for the
receipt-gated Moza validation lane — vendor-status framing diagnosis, authority
attempts, fixture promotion, pit-house evidence — which are tooling for that
lane rather than commands a wheel owner runs.

It is hidden, not gated: `wheelctl moza ...` works exactly as before and
`wheelctl moza --help` lists every subcommand. See
[docs/hardware/moza-r5-validation.md](../../docs/hardware/moza-r5-validation.md).

### Key Capabilities

#### Device Management
- `wheelctl device list [--detailed] [--hid-observe-only] [--json]` - List connected devices
- `wheelctl device status <device> [--watch] [--json]` - Show device status
- `wheelctl device calibrate <device> <type> [--yes]` - Calibrate device
- `wheelctl device reset <device> [--force]` - Reset to safe state

#### Profile Management
- `wheelctl profile list [--game <game>] [--car <car>]` - List profiles
- `wheelctl profile show <profile>` - Show profile details
- `wheelctl profile create <path> [--from <base>] [--game <game>]` - Create profile
- `wheelctl profile apply <device> <profile>` - Apply profile to device
- `wheelctl profile edit <profile> [--field <field>] [--value <value>]` - Edit profile
- `wheelctl profile validate <path>` - Validate profile schema
- `wheelctl profile export <profile> [--output <file>] [--signed]` - Export profile
- `wheelctl profile import <path> [--target <dir>] [--verify]` - Import profile

#### Diagnostics
- `wheelctl diag test [--device <device>] [<test-type>]` - Run diagnostics
- `wheelctl diag record <device> [--duration <secs>] [--output <file>]` - Record blackbox
- `wheelctl diag replay <file> [--detailed]` - Replay blackbox recording (`-d`/`--detailed` gives frame-by-frame output; `-v`/`--verbose` is the global logging flag)
- `wheelctl diag support [--blackbox] [--output <file>]` - Generate support bundle
- `wheelctl diag metrics [--device <device>] [--watch]` - Show performance metrics

#### Game Integration
- `wheelctl game list [--detailed]` - List supported games
- `wheelctl game configure <game> [--path <path>] [--auto]` - Configure telemetry
- `wheelctl game status [--telemetry]` - Show game status
- `wheelctl game test <game> [--duration <secs>]` - Test telemetry connection

#### Safety Controls
- `wheelctl safety enable <device> [--force]` - Enable high torque mode
- `wheelctl safety stop [<device>]` - Emergency stop
- `wheelctl safety status [<device>]` - Show safety status
- `wheelctl safety limit <device> <torque> [--global]` - Set torque limits

#### Health Monitoring
- `wheelctl health [--watch]` - Show service health status

#### Plugins
- `wheelctl plugin list` - List available plugins from the registry
- `wheelctl plugin search <query>` - Search plugins by name or description
- `wheelctl plugin install <plugin>` - Install a plugin from the registry
- `wheelctl plugin uninstall <plugin>` - Uninstall a plugin
- `wheelctl plugin info <plugin>` - Show detailed plugin information
- `wheelctl plugin verify <plugin>` - Verify an installed plugin's integrity and signature

#### Telemetry
- `wheelctl telemetry probe <game>` - Probe the telemetry transport for a game
- `wheelctl telemetry capture` - Capture raw UDP telemetry packets to a binary file
- `wheelctl telemetry record` - Record normalized telemetry snapshots to JSONL with safety provenance
- `wheelctl telemetry virtual-ffb-log` - Replay normalized telemetry into a virtual FFB output log (no hardware writes)

#### Control Stream (observe-only)
- `wheelctl controls list` - List the stable control descriptors a profile may bind for a surface
- `wheelctl controls monitor` - Replay a capture as a human-readable stream, reporting resets and epoch changes
- `wheelctl controls capture` - Write a deterministic sample capture (virtual input; no hardware)
- `wheelctl controls replay` - Replay a capture's inputs through the real projection without hardware

#### Hardware Diagnostics
- `wheelctl hardware doctor` - Inspect local hardware/tooling readiness without opening devices or sending writes
- `wheelctl hardware bringup-rail [--family <family>]` - Print the staged bring-up rail for a device family
- `wheelctl hardware lane` - Scaffold a hardware validation lane from a device-family rail adapter
- `wheelctl hardware sniff-*` - Passive USB capture planning, receipts, and evidence bundles (nine subcommands; none send output)

#### Support Bundles
- `wheelctl support-bundle [--device <device>] [--blackbox] [--output <file>]` - Generate a diagnostic support bundle

### Error Codes

The CLI uses specific exit codes for different error types:

- `0` - Success
- `1` - General error
- `2` - Device not found
- `3` - Profile not found
- `4` - Validation error
- `5` - Service unavailable
- `6` - Permission denied

### JSON Output

All commands support JSON output via the `--json` flag for machine-readable responses:

```bash
wheelctl --json device list
wheelctl --json profile show my-profile.json
wheelctl --json diag test --device wheel-001
```

### Shell Completion

Generate completion scripts for your shell:

```bash
# Bash
wheelctl completion bash > ~/.wheelctl-completion.bash
source ~/.wheelctl-completion.bash

# Zsh
wheelctl completion zsh > ~/.zsh/completions/_wheelctl

# Fish
wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish

# PowerShell
wheelctl completion powershell | Out-String | Invoke-Expression
```

### Configuration

The CLI connects to the wheel service via IPC. Configuration options:

- `WHEELCTL_ENDPOINT` - Override service endpoint (for testing)
- Verbose logging with `-v`, `-vv`, `-vvv` flags
- JSON output with `--json` flag

### Integration Testing

Comprehensive integration tests cover:

- All major command workflows
- Error code validation
- JSON output validation
- Profile creation/validation workflows
- Diagnostic workflows
- Safety command workflows
- End-to-end user scenarios

Run tests with:
```bash
cargo test --test integration_tests
```

### Architecture

The CLI is built with:

- **clap** - Command-line argument parsing with derive macros
- **tokio** - Async runtime for IPC communication
- **serde_json** - JSON serialization/deserialization
- **colored** - Terminal color output
- **indicatif** - Progress bars and spinners
- **dialoguer** - Interactive prompts
- **anyhow/thiserror** - Error handling

The CLI communicates with the wheel service via IPC (gRPC over named pipes/UDS) using generated protobuf contracts for type safety and versioning.

### Requirements Compliance

Against UX-02:

- ✅ **Command-line interface with device, profile, and diagnostic commands**
- ✅ **JSON output formatting (`--json`) for machine-readable responses**
- ✅ **Bash/zsh completion scripts for CLI commands**
- ✅ **CLI integration tests covering all major command workflows with error code validation**
- ⚠️ **All write operations available in CLI match UI capabilities** — the
  commands exist and parse, but `safety limit` has no IPC write behind it and
  `profile apply` covers only the fields the wire contract represents (see
  Known gaps). Command surface parity is met; write-path parity is not.
