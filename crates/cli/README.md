# wheelctl - Racing Wheel Control CLI

A comprehensive command-line interface for the Racing Wheel Software Suite, providing full parity with UI capabilities for managing racing wheel hardware, profiles, diagnostics, and game integration.

## Features

### ✅ Complete Implementation

- **Device Management**: List, status, calibration, and reset operations
- **Profile Management**: Create, edit, validate, apply, import/export profiles
- **Diagnostics**: System tests, blackbox recording/replay, support bundles, metrics
- **Game Integration**: Configure telemetry, test connections, manage game support
- **Safety Controls**: High torque mode, emergency stop, torque limits, safety status
- **Health Monitoring**: Real-time service and device health monitoring
- **JSON Output**: Machine-readable output for all commands (`--json` flag)
- **Shell Completion**: Bash, Zsh, Fish, and PowerShell completion scripts
- **Error Handling**: Proper exit codes for different error types
- **Verbose Logging**: Configurable logging levels (`-v`, `-vv`, `-vvv`)

### Command Structure

```
wheelctl [OPTIONS] <COMMAND>

Commands:
  device      Device management commands
  profile     Profile management commands  
  diag        Diagnostic and monitoring commands
  game        Game integration commands
  safety      Safety and control commands
  completion  Generate shell completion scripts
  health      Service health and status
```

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
- `wheelctl diag replay <file> [--verbose]` - Replay blackbox recording
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

This implementation satisfies all requirements from UX-02:

✅ **Command-line interface with device, profile, and diagnostic commands**
✅ **JSON output formatting (--json flag) for machine-readable responses**  
✅ **All write operations available in CLI match UI capabilities**
✅ **Bash/zsh completion scripts for CLI commands**
✅ **CLI integration tests covering all major command workflows with error code validation**

The CLI provides complete parity with UI functionality while offering additional automation and scripting capabilities through JSON output and proper exit codes.
