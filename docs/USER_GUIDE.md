# OpenRacing User Guide

Welcome to OpenRacing, a high-performance racing wheel and force feedback simulation software designed for sim-racing enthusiasts and professionals. This guide will help you get started with OpenRacing and make the most of its features.

> [!IMPORTANT]
> **Project status: pre-validation** — OpenRacing has not been end-to-end validated on real hardware or simulators. This guide describes intended functionality. See [Project Status](PROJECT_STATUS.md) for details.

## Table of Contents

1. [Introduction](#introduction)
2. [System Requirements](#system-requirements)
3. [Installation](#installation)
4. [Getting Started](#getting-started)
5. [CLI Reference](#cli-reference)
6. [Game Integration](#game-integration)
7. [Profiles](#profiles)
8. [Safety Features](#safety-features)
9. [Troubleshooting](#troubleshooting)
10. [Advanced Topics](#advanced-topics)
11. [FAQ](#faq)
12. [Glossary](#glossary)

---

## Introduction

OpenRacing is a safety-critical racing wheel and force feedback simulation software built in Rust. It targets real-time force feedback processing at 1kHz with deterministic latency and comprehensive safety interlocks.

### Who is OpenRacing for?

- **Sim-racing enthusiasts** who want authentic force feedback
- **Competitive racers** requiring consistent, low-latency performance
- **Hardware developers** working with racing wheel hardware
- **Modders** creating custom FFB profiles and effects

### Key Features

- **Real-time Force Feedback at 1kHz** - Deterministic processing pipeline with sub-millisecond latency
- **Multi-Game Integration** - Telemetry adapters for 61 racing simulators including iRacing, ACC, Forza, BeamNG, and more (see [Supported Games](SETUP.md#4-game-setup))
- **Safety-Critical Design** - Comprehensive fault detection and hardware watchdog integration
- **Cross-Platform Support** - Windows 10+, Linux kernel 4.0+; macOS compiles but device I/O is not yet implemented
- **Profile Management** - JSON-based force feedback profiles with schema validation
- **Comprehensive Diagnostics** - Black box recording and support bundle generation

---

## System Requirements

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | Multi-core processor (x64) | Intel Core i5 / AMD Ryzen 5 or better |
| RAM | 4 GB | 8 GB or more |
| Storage | 500 MB available space | SSD for best performance |
| USB | USB 2.0 port | USB 3.0 port, direct motherboard connection |

### Supported Operating Systems

- **Windows**: Windows 10 or later (x64)
- **Linux**: Modern distribution with kernel 4.0+ (x64)
- **macOS**: macOS 10.15 (Catalina) or later

### Supported Racing Wheels

OpenRacing contains protocol implementations for 28 vendors and their product lines through HID (Human Interface Device) communication:

- Moza Racing (R3, R5, R9, R12, R16, R21; protocol-known, real-hardware output requires lane receipts)
- Fanatec CSL DD, GT DD Pro, Podium DD1/DD2, CSW v2.5
- Logitech G27, G29, G923, G Pro
- Thrustmaster T-series and TX/T300/T818 series
- Simagic Alpha, Alpha Mini, M10, Neo
- VRS DirectForce Pro
- Simucube 2 Sport/Pro/Ultimate
- Asetek SimSports Forte, Invicta, LaPrima
- Cammus C5, C12
- OpenFFBoard, FFBeast, AccuForce
- Heusinkveld and Leo Bodnar (input only)
- PXN (V10, V12, V12 Lite)
- Granite Devices IONI/ARGON (Simucube 1, SimpleMotion V2)
- Most other HID-compliant racing wheels

> **Note**: For the complete vendor table with VID/PID details, see [SETUP.md — Supported Devices](SETUP.md#supported-devices).

---

## Installation

This section provides detailed installation instructions for all supported platforms. Choose the method that best suits your needs.

> [!NOTE]
> **Packaged installers are not published yet.** The platform-specific installer sections below describe planned packaging targets. Currently, OpenRacing must be [built from source](#building-from-source).

### Windows Installation

OpenRacing supports Windows 10 and later (x64). Multiple installation methods are available.

#### Packaged Installers (Planned)

> [!NOTE]
> **MSI installer, silent installation, and portable ZIP are planned packaging targets — not yet available.**

The planned Windows installers will:
- Register `wheeld` as a Windows service
- Configure system PATH
- Support configurable installation directory and optional power optimization
- Provide silent installation mode for automated deployments
- Offer a portable ZIP alternative for users who prefer no installer

#### Windows Service Management

The `wheeld` service runs in the background and manages device communication:

```cmd
# Check service status
sc query wheeld

# Start the service
sc start wheeld

# Stop the service
sc stop wheeld

# Restart the service
sc stop wheeld && sc start wheeld

# Remove the service (run as Administrator)
wheeld.exe uninstall
```

**Service Configuration:**
- **Service Name**: `wheeld`
- **Display Name**: OpenRacing Wheel Daemon
- **Startup Type**: Automatic
- **Account**: Local System

### Linux Installation

OpenRacing supports modern Linux distributions with kernel 4.0+. Multiple package formats are available.

#### Packaged Installers (Planned)

> [!NOTE]
> **Debian/Ubuntu (.deb), Fedora/RHEL (.rpm), and APT/DNF repository hosting are planned packaging targets — not yet available.**

The planned Linux packages will:
- Install binaries and udev rules (`packaging/linux/99-racing-wheel-suite.rules`)
- Register `wheeld` as a systemd user service
- Handle dependency installation
- Provide APT and DNF repository hosting for automatic updates

#### Generic Linux (Tarball)

For any Linux distribution or manual installation:

> [!NOTE]
> **Pre-built tarballs are not yet published.** The commands below show the intended installation steps once a release tarball is available. Currently, [build from source](#building-from-source-linux) instead.

```bash
# Extract to /opt (or your preferred location)
sudo tar -xzf openracing-<version>-linux-x86_64.tar.gz -C /opt

# Create symlinks for CLI access
sudo ln -s /opt/openracing/bin/wheelctl /usr/local/bin/wheelctl
sudo ln -s /opt/openracing/bin/wheeld /usr/local/bin/wheeld

# Install udev rules
sudo cp /opt/openracing/share/udev/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger

# Install systemd service
mkdir -p ~/.config/systemd/user
cp /opt/openracing/share/systemd/wheeld.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now wheeld
```

#### Building from Source (Linux)

For developers or users who want the latest features:

```bash
# Install Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Build release binaries
cargo build --release

# Install binaries
sudo cp target/release/wheelctl /usr/local/bin/
sudo cp target/release/wheeld /usr/local/bin/

# Install udev rules
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger

# Install systemd service
mkdir -p ~/.config/systemd/user
cp packaging/linux/wheeld.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now wheeld
```

#### Udev Rules Explained

The udev rules file (`99-racing-wheel-suite.rules`) ensures proper permissions for racing wheel devices:

```bash
# /etc/udev/rules.d/99-racing-wheel-suite.rules

# Logitech wheels
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", MODE="0666"

# Fanatec wheels
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="0eb7", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0eb7", MODE="0666"

# Thrustmaster wheels
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="044f", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="044f", MODE="0666"

# Moza wheels
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="346e", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="346e", MODE="0666"

# Simagic wheels
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="0483", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0483", MODE="0666"

# Generic HID racing wheels
ACTION=="add", SUBSYSTEM=="hidraw", MODE="0666"
```

After modifying udev rules, reload them:
```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

#### Linux Service Management

```bash
# Check service status
systemctl --user status wheeld

# Start the service
systemctl --user start wheeld

# Stop the service
systemctl --user stop wheeld

# Restart the service
systemctl --user restart wheeld

# View service logs
journalctl --user -u wheeld -f

# Disable service autostart
systemctl --user disable wheeld
```

### macOS Installation

OpenRacing compiles on macOS 10.15 (Catalina) and later, but the IOKit HID driver is not yet implemented — device I/O is not functional.

> **Note**: macOS support is compile-only. The IOKit HID driver and macOS-specific packaging (DMG, Homebrew, notarization) are planned but not yet available. You can build from source to experiment with non-device features.

#### Packaged Installers (Planned)

> [!NOTE]
> **Homebrew tap, DMG installer, and pre-built tarballs are planned packaging targets — not yet available.** macOS support is currently compile-only (see note above).

The planned macOS packages will:
- Provide a Homebrew tap (`brew install openracing`)
- Install a launchd service (`com.openracing.wheeld`)
- Offer a pre-built tarball for manual installation

#### Building from Source (macOS)

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing
cargo build --release

# Install binaries
sudo cp target/release/wheelctl /usr/local/bin/
sudo cp target/release/wheeld /usr/local/bin/

# Install launchd service
cp packaging/macos/com.openracing.wheeld.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.openracing.wheeld.plist
```

#### macOS Service Management

```bash
# Check service status
launchctl list | grep openracing

# Start the service
launchctl load ~/Library/LaunchAgents/com.openracing.wheeld.plist

# Stop the service
launchctl unload ~/Library/LaunchAgents/com.openracing.wheeld.plist

# View logs
log show --predicate 'subsystem == "com.openracing.wheeld"' --last 1h
```

### Verification Steps

After installation on any platform, verify that OpenRacing is working correctly:

```bash
# Check CLI installation
wheelctl --version

# Check service status
wheelctl health

# List connected devices
wheelctl device list
```

**Expected output:**
```
OpenRacing CLI version 0.1.0

Service Health Status
  Service: Running
  Overall: Healthy
  Devices: 1
    ✓ Logitech G29 (046d:c29f)
```

### Uninstallation

#### Windows

```cmd
# Using Control Panel
# Go to Settings > Apps > OpenRacing > Uninstall

# Using MSI (silent)
msiexec /x OpenRacing-0.1.0-x64.msi /quiet /norestart

# Manual cleanup (if needed)
sc stop wheeld
sc delete wheeld
rmdir /s /q "C:\Program Files\OpenRacing"
```

#### Linux (Debian/Ubuntu)

```bash
# Remove package
sudo apt remove openracing

# Remove package and configuration
sudo apt purge openracing

# Remove udev rules (if not removed automatically)
sudo rm /etc/udev/rules.d/99-racing-wheel-suite.rules
sudo udevadm control --reload-rules
```

#### Linux (Fedora/RHEL)

```bash
# Remove package
sudo dnf remove openracing

# Remove udev rules (if not removed automatically)
sudo rm /etc/udev/rules.d/99-racing-wheel-suite.rules
sudo udevadm control --reload-rules
```

#### macOS

```bash
# Using Homebrew
brew services stop openracing
brew uninstall openracing

# Manual removal
launchctl unload ~/Library/LaunchAgents/com.openracing.wheeld.plist
rm ~/Library/LaunchAgents/com.openracing.wheeld.plist
sudo rm -rf /usr/local/openracing
sudo rm /usr/local/bin/wheelctl /usr/local/bin/wheeld
```

### Building from Source

Until packaged installers are available, build from source on any platform:

```bash
# Install Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Build release binaries
cargo build --release --workspace

# The binaries will be in target/release/
# - wheelctl (CLI tool)
# - wheeld   (service daemon)
```

See the platform-specific sections above for instructions on installing udev rules (Linux) or launchd services (macOS).

---

## Getting Started

### First-Time Setup Wizard

When you first run OpenRacing, the setup wizard will guide you through initial configuration:

```bash
# Launch the setup wizard
wheelctl setup
```

The wizard will:
1. Detect your racing wheel hardware
2. Configure device permissions
3. Set up game integration
4. Create a default profile
5. Configure safety settings

### Device Detection and Calibration

#### Detect Connected Devices

```bash
# List all connected devices
wheelctl device list

# Show detailed device information
wheelctl device list --detailed
```

#### Calibrate Your Wheel

Calibration ensures accurate force feedback and proper device behavior:

```bash
# Full calibration (center, DOR, pedals)
wheelctl device calibrate <device-id> --type all

# Calibrate only center position
wheelctl device calibrate <device-id> --type center

# Calibrate degrees of rotation
wheelctl device calibrate <device-id> --type dor

# Calibrate pedals
wheelctl device calibrate <device-id> --type pedals

# Skip confirmation prompts
wheelctl device calibrate <device-id> --type all --yes
```

**Calibration Tips:**
- Ensure the wheel is centered before starting
- Remove hands from the wheel during DOR calibration
- Press each pedal fully and release during pedal calibration
- Keep the wheel in a stable position throughout

### Basic Configuration

#### Check Device Status

```bash
# Show current device status
wheelctl device status <device-id>

# Watch status in real-time
wheelctl device status <device-id> --watch
```

#### Create Your First Profile

```bash
# Create a default profile
wheelctl profile create my-profile.json

# Create a profile for a specific game
wheelctl profile create profiles/iracing/gt3.json --game iracing --car gt3

# Create from an existing profile
wheelctl profile create profiles/iracing/formula.json --from my-profile.json --game iracing --car formula
```

### Testing Force Feedback

After setting up your device and profile, test the force feedback:

```bash
# Run diagnostic tests
wheelctl diag test

# Run specific test
wheelctl diag test --device <device-id> --type motor

# Watch performance metrics
wheelctl diag metrics --watch
```

You should feel smooth, responsive force feedback when the tests pass.

---

## Configuration Guide

This section covers all aspects of configuring OpenRacing for optimal performance and personalized force feedback.

### Configuration Files

OpenRacing uses several configuration files stored in platform-specific locations:

| File | Purpose | Location (Windows) | Location (Linux/macOS) |
|------|---------|-------------------|------------------------|
| `config.toml` | Global settings | `%LOCALAPPDATA%\Wheel\config.toml` | `~/.config/wheel/config.toml` |
| `profiles/*.json` | FFB profiles | `%LOCALAPPDATA%\Wheel\profiles\` | `~/.config/wheel/profiles/` |
| `trust_store.json` | Plugin signatures | `%LOCALAPPDATA%\Wheel\trust_store.json` | `~/.config/wheel/trust_store.json` |
| `devices.json` | Device settings | `%LOCALAPPDATA%\Wheel\devices.json` | `~/.config/wheel/devices.json` |

### Global Configuration (config.toml)

The main configuration file controls service behavior and default settings:

```toml
# OpenRacing Configuration File

[service]
# Log level: trace, debug, info, warn, error
log_level = "info"

# IPC socket path (Linux/macOS only)
socket_path = "/tmp/wheeld.sock"

# Enable telemetry collection
telemetry_enabled = true

[safety]
# Global maximum torque limit (Nm)
max_torque_nm = 20.0

# Require physical confirmation for high torque
require_physical_interlock = true

# Watchdog timeout (ms)
watchdog_timeout_ms = 100

# Communication loss timeout (ms)
comm_loss_timeout_ms = 50

[performance]
# Target tick rate (Hz)
tick_rate_hz = 1000

# Enable MMCSS (Windows only)
enable_mmcss = true

# CPU affinity (comma-separated core IDs, empty for auto)
cpu_affinity = ""

[plugins]
# Allow unsigned plugins (security risk!)
allow_unsigned = false

# Plugin directory
plugin_dir = "~/.config/wheel/plugins"

# Maximum WASM memory (bytes)
wasm_max_memory = 16777216

[telemetry_adapters]
# Enable specific game adapters
iracing_enabled = true
acc_enabled = true
ams2_enabled = true
rf2_enabled = false

# iRacing shared memory name
iracing_shm_name = "Local\\IRSDKMemMapFileName"

# ACC UDP port
acc_udp_port = 9996
```

### Profile Management

Profiles define force feedback settings for your racing wheel. They support inheritance, allowing you to create base profiles and override specific settings for different games or cars.

#### Profile Directory Structure

Organize your profiles for easy management:

```
~/.config/wheel/profiles/
├── base/
│   ├── default.json          # Default base profile
│   └── high-torque.json      # High torque base profile
├── iracing/
│   ├── gt3.json              # iRacing GT3 cars
│   ├── formula.json          # iRacing Formula cars
│   └── oval.json             # iRacing Oval racing
├── acc/
│   ├── gt3.json              # ACC GT3 cars
│   └── gt4.json              # ACC GT4 cars
└── community/
    └── imported-profile.json # Community profiles
```

#### Creating Profiles

```bash
# Create a new profile with default settings
wheelctl profile create profiles/my-profile.json

# Create a profile inheriting from a base profile
wheelctl profile create profiles/iracing/gt3.json --from profiles/base/default.json

# Create a profile with game/car scope
wheelctl profile create profiles/iracing/gt3.json --game iracing --car gt3

# Create from template with all options
wheelctl profile create profiles/acc/gt3.json \
  --from profiles/base/high-torque.json \
  --game acc \
  --car gt3 \
  --name "ACC GT3 Profile"
```

#### Profile Inheritance

Profiles can inherit from parent profiles, allowing you to maintain a base configuration with game-specific overrides:

```json
{
  "schema": "wheel.profile/1",
  "name": "iRacing GT3",
  "parent": "base/default.json",
  "scope": {
    "game": "iracing",
    "car": "gt3"
  },
  "base": {
    "ffbGain": 0.85,
    "filters": {
      "damper": 0.15
    }
  }
}
```

In this example:
- The profile inherits all settings from `base/default.json`
- Only `ffbGain` and `damper` are overridden
- All other settings come from the parent profile

**Inheritance Rules:**
- Child values override parent values
- Unspecified values inherit from parent
- Maximum inheritance depth: 5 levels
- Circular inheritance is detected and rejected

#### Editing Profiles

```bash
# Interactive edit (opens in default editor)
wheelctl profile edit profiles/my-profile.json

# Edit specific fields directly
wheelctl profile edit profiles/my-profile.json --field base.ffbGain --value 0.8
wheelctl profile edit profiles/my-profile.json --field base.dorDeg --value 900
wheelctl profile edit profiles/my-profile.json --field base.filters.damper --value 0.2

# Batch edit multiple fields
wheelctl profile edit profiles/my-profile.json \
  --field base.ffbGain --value 0.8 \
  --field base.filters.damper --value 0.15 \
  --field base.filters.friction --value 0.1
```

### FFB Settings Reference

#### Core FFB Parameters

| Parameter | Description | Range | Recommended |
|-----------|-------------|-------|-------------|
| `ffbGain` | Overall force feedback strength | 0.0 - 1.0 | 0.7 - 0.9 |
| `dorDeg` | Degrees of rotation | 0 - 3600 | Match car's real steering |
| `torqueCapNm` | Maximum torque output | 0.1 - device max | Based on wheel capability |

#### Filter Parameters

| Parameter | Description | Range | Effect |
|-----------|-------------|-------|--------|
| `reconstruction` | Signal reconstruction level | 0 - 8 | Higher = smoother, more latency |
| `friction` | Static friction simulation | 0.0 - 1.0 | Adds resistance at center |
| `damper` | Dynamic damping | 0.0 - 1.0 | Reduces oscillation |
| `inertia` | Wheel inertia simulation | 0.0 - 1.0 | Adds weight to steering |
| `slewRate` | Torque change rate limit | 0.0 - 2.0 | Lower = smoother transitions |

#### Response Curves

Response curves modify how input torque maps to output torque:

```json
{
  "base": {
    "filters": {
      "curvePoints": [
        {"input": 0.0, "output": 0.0},
        {"input": 0.25, "output": 0.2},
        {"input": 0.5, "output": 0.45},
        {"input": 0.75, "output": 0.7},
        {"input": 1.0, "output": 1.0}
      ]
    }
  }
}
```

**Curve Types:**
- **Linear**: 1:1 input to output mapping
- **Exponential**: More detail at low forces, compressed at high forces
- **Logarithmic**: More detail at high forces, compressed at low forces
- **Custom Bezier**: Full control over the response curve

#### Bumpstop Configuration

```json
{
  "base": {
    "filters": {
      "bumpstop": {
        "enabled": true,
        "strength": 0.5,
        "range_deg": 5.0
      }
    }
  }
}
```

| Parameter | Description | Range |
|-----------|-------------|-------|
| `enabled` | Enable bumpstop effect | true/false |
| `strength` | Bumpstop force intensity | 0.0 - 1.0 |
| `range_deg` | Degrees before full stop | 1.0 - 30.0 |

#### Hands-Off Detection

```json
{
  "base": {
    "filters": {
      "handsOff": {
        "enabled": true,
        "sensitivity": 0.3,
        "timeout_ms": 500,
        "reduction": 0.5
      }
    }
  }
}
```

| Parameter | Description | Range |
|-----------|-------------|-------|
| `enabled` | Enable hands-off detection | true/false |
| `sensitivity` | Detection sensitivity | 0.0 - 1.0 |
| `timeout_ms` | Time before torque reduction | 100 - 5000 |
| `reduction` | Torque reduction factor | 0.0 - 1.0 |

### Device-Specific Configuration

Each device can have specific settings stored in `devices.json`:

```json
{
  "devices": {
    "046d:c29f": {
      "name": "Logitech G29",
      "max_torque_nm": 2.5,
      "default_dor_deg": 900,
      "calibration": {
        "center_offset": 0,
        "dor_actual": 900
      },
      "default_profile": "profiles/base/default.json"
    }
  }
}
```

### Auto Profile Switching

Configure automatic profile switching based on game and car:

```bash
# Enable auto-switching
wheelctl config set auto_profile_switch true

# Set profile priorities
wheelctl config set profile_priority "game,car,track"
```

**Profile Matching Order:**
1. Exact match: game + car + track
2. Game + car match
3. Game match only
4. Default profile

### Performance Tuning

#### Windows Performance Settings

```cmd
# Set high performance power plan
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c

# Disable USB selective suspend (Device Manager)
# Or via registry:
reg add "HKLM\SYSTEM\CurrentControlSet\Services\USB\DisableSelectiveSuspend" /v DisableSelectiveSuspend /t REG_DWORD /d 1 /f

# Enable MMCSS for OpenRacing (automatic if enabled in config)
```

#### Linux Performance Settings

```bash
# Set CPU governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable USB autosuspend
echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms

# Set real-time priority for wheeld (if using systemd)
# Add to wheeld.service:
# [Service]
# CPUSchedulingPolicy=fifo
# CPUSchedulingPriority=50
```

#### Configuration Validation

Validate your configuration before applying:

```bash
# Validate global config
wheelctl config validate

# Validate a specific profile
wheelctl profile validate profiles/my-profile.json --detailed

# Check for configuration issues
wheelctl diag config
```

---

## CLI Reference

The `wheelctl` command-line interface provides comprehensive control over OpenRacing. All commands support the `--json` flag for machine-readable output.

### Global Options

| Option | Description |
|--------|-------------|
| `--json` | Output in JSON format for machine parsing |
| `-v`, `-vv`, `-vvv` | Increase verbosity (info, debug, trace) |
| `--help` | Show help information |
| `--version` | Show version information |

### Device Command

Manage racing wheel hardware.

#### `device list`

List all connected devices.

```bash
wheelctl device list
wheelctl device list --detailed
```

**Output Example:**
```
Available Profiles:
  ● Logitech G29 (046d:c29f)
    Product ID: c29f
    Vendor ID: 046d
    Max Torque: 2.5 Nm
    DOR: 900°
```

#### `device status`

Show device status and telemetry.

```bash
wheelctl device status <device-id>
wheelctl device status <device-id> --watch
```

**Output Example:**
```
Device Status: Logitech G29
  State: Connected
  Angle: 0°
  Speed: 0.0 rad/s
  Temperature: 42°C
  Hands On: ✓
  Faults: None
```

#### `device calibrate`

Calibrate device (center, DOR, pedals).

```bash
wheelctl device calibrate <device-id> --type <center|dor|pedals|all>
wheelctl device calibrate <device-id> --type all --yes
```

**Calibration Types:**
- `center` - Center the wheel position
- `dor` - Calibrate degrees of rotation
- `pedals` - Calibrate pedal ranges
- `all` - Full calibration sequence

#### `device reset`

Reset device to safe state.

```bash
wheelctl device reset <device-id>
wheelctl device reset <device-id> --force
```

> **Warning**: Reset stops all force feedback and returns to default settings.

### Profile Command

Manage force feedback profiles.

#### `profile list`

List available profiles.

```bash
wheelctl profile list
wheelctl profile list --game iracing
wheelctl profile list --game iracing --car gt3
```

#### `profile show`

Show profile details.

```bash
wheelctl profile show <profile-path>
```

#### `profile apply`

Apply profile to device.

```bash
wheelctl profile apply <device-id> <profile-path>
wheelctl profile apply <device-id> <profile-path> --skip-validation
```

#### `profile create`

Create new profile.

```bash
wheelctl profile create <path>
wheelctl profile create <path> --from <base-profile> --game <game> --car <car>
```

#### `profile edit`

Edit profile interactively or with specific field/value.

```bash
# Interactive edit
wheelctl profile edit <profile-path>

# Direct field edit
wheelctl profile edit <profile-path> --field base.ffbGain --value 0.8
wheelctl profile edit <profile-path> --field base.dorDeg --value 900
wheelctl profile edit <profile-path> --field base.torqueCapNm --value 10.0
```

**Editable Fields:**
- `base.ffbGain` - Force feedback gain (0.0-1.0)
- `base.dorDeg` - Degrees of rotation (0-3600)
- `base.torqueCapNm` - Torque cap in Nm
- `scope.game` - Game scope
- `scope.car` - Car scope

#### `profile validate`

Validate profile.

```bash
wheelctl profile validate <path>
wheelctl profile validate <path> --detailed
```

#### `profile export`

Export profile.

```bash
wheelctl profile export <profile-path>
wheelctl profile export <profile-path> --output <output-path>
wheelctl profile export <profile-path> --signed
```

#### `profile import`

Import profile.

```bash
wheelctl profile import <path>
wheelctl profile import <path> --target <target-directory>
wheelctl profile import <path> --verify
```

### Game Command

Manage game integration.

#### `game list`

List supported games.

```bash
wheelctl game list
wheelctl game list --detailed
```

**Supported Games:**
| Game | ID | Status | Features |
|------|-----|--------|----------|
| iRacing | `iracing` | Full Support | FFB Scalar, RPM, Car ID |
| Assetto Corsa Competizione | `acc` | Full Support | FFB Scalar, RPM, Car ID, DRS |
| Assetto Corsa | `assetto_corsa` | Full Support | FFB Scalar, RPM |
| DiRT Rally 2.0 | `dirt_rally_2` | Full Support | FFB Scalar, RPM |
| Forza Motorsport / Horizon | `forza_motorsport` | Full Support | FFB Scalar, RPM |
| BeamNG.drive | `beamng_drive` | Full Support | FFB Scalar, RPM |
| Project CARS 2 | `project_cars_2` | Full Support | FFB Scalar, RPM |
| Automobilista 2 | `ams2` | Experimental | FFB Scalar, RPM |
| rFactor 2 | `rfactor2` | Experimental | FFB Scalar, RPM, Telemetry |
| F1 24 / F1 25 | `f1` | Experimental | FFB Scalar, RPM |
| EA SPORTS WRC | `eawrc` | Experimental | FFB Scalar, RPM |
| Gran Turismo 7 | `gran_turismo_7` | Experimental | FFB Scalar, RPM |

> For the full list of 28+ supported games, see [SETUP.md — Supported Games](SETUP.md#supported-games).

#### `game configure`

Configure game for telemetry.

```bash
wheelctl game configure <game-id>
wheelctl game configure <game-id> --path <install-path>
wheelctl game configure <game-id> --auto
```

**Example:**
```bash
# Auto-configure iRacing
wheelctl game configure iracing --auto

# Configure ACC with custom path
wheelctl game configure acc --path "C:\Games\ACC"
```

#### `game status`

Show game status.

```bash
wheelctl game status
wheelctl game status --telemetry
```

#### `game test`

Test telemetry connection.

```bash
wheelctl game test <game-id>
wheelctl game test <game-id> --duration 30
```

### Safety Command

Manage safety features and controls.

#### `safety enable`

Enable high torque mode.

```bash
wheelctl safety enable <device-id>
wheelctl safety enable <device-id> --force
```

> **Warning**: High torque mode requires physical confirmation (hold both clutch paddles for 3 seconds).

#### `safety stop`

Emergency stop all devices.

```bash
# Stop all devices
wheelctl safety stop

# Stop specific device
wheelctl safety stop <device-id>
```

#### `safety status`

Show safety status.

```bash
# Show all devices
wheelctl safety status

# Show specific device
wheelctl safety status <device-id>
```

**Output Example:**
```
Safety Status:
  ● Logitech G29 (046d:c29f)
    High Torque: Disabled
    Torque Limit: 2.5 Nm
    Hands On: ✓
    Temperature: ✓ (42°C)
    No Faults: ✓
    ✓ Ready for high torque
```

#### `safety limit`

Set torque limits.

```bash
wheelctl safety limit <device-id> <torque-nm>
wheelctl safety limit <device-id> 8.0 --global
```

### Diag Command

Diagnostic and monitoring commands.

#### `diag test`

Run system diagnostics.

```bash
wheelctl diag test
wheelctl diag test --device <device-id> --type <motor|encoder|usb|thermal|all>
```

**Test Types:**
- `motor` - Motor phase testing
- `encoder` - Encoder integrity testing
- `usb` - USB communication testing
- `thermal` - Thermal management testing
- `all` - Run all tests

#### `diag record`

Record blackbox data.

```bash
wheelctl diag record <device-id>
wheelctl diag record <device-id> --duration 60 --output my-blackbox.wbb
```

#### `diag replay`

Replay blackbox recording.

```bash
wheelctl diag replay <file>
wheelctl diag replay <file> --verbose
```

#### `diag support`

Generate support bundle.

```bash
wheelctl diag support
wheelctl diag support --blackbox --output support-bundle.zip
```

The support bundle includes:
- System information
- Device diagnostics
- Performance metrics
- Fault history
- Optional blackbox recordings

#### `diag metrics`

Show performance metrics.

```bash
wheelctl diag metrics
wheelctl diag metrics --device <device-id> --watch
```

### Health Command

Service health and status monitoring.

```bash
# Show health snapshot
wheelctl health

# Watch health events in real-time
wheelctl health --watch
```

### Shell Completion Setup

Enable tab completion for your shell:

#### Bash

```bash
# Generate completion script
wheelctl completion bash > ~/.local/share/bash-completion/completions/wheelctl

# Source it in your .bashrc
echo 'source ~/.local/share/bash-completion/completions/wheelctl' >> ~/.bashrc
```

#### Zsh

```bash
# Generate completion script
wheelctl completion zsh > ~/.zsh/completion/_wheelctl

# Add to .zshrc
echo 'fpath=(~/.zsh/completion $fpath)' >> ~/.zshrc
echo 'autoload -U compinit && compinit' >> ~/.zshrc
```

#### PowerShell

```powershell
# Generate and source completion script
wheelctl completion powershell | Out-File -Encoding UTF8 wheelctl.ps1
. ./wheelctl.ps1
```

#### Fish

```bash
# Generate completion script
wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish
```

---

## Game Integration

OpenRacing integrates with popular racing simulators to provide enhanced force feedback and telemetry features.

### Supported Games

#### iRacing

**Status**: Full Support

**Configuration Method**: Shared Memory (`app.ini`)

**Features**:
- FFB Scalar
- RPM
- Car ID
- Track ID

**Setup**:
```bash
# Auto-configure iRacing
wheelctl game configure iracing --auto
```

**Manual Configuration**:
1. Open `Documents\iRacing\app.ini`
2. Find or add the `[Telemetry]` section
3. Set `enableTelemetry=1`
4. Set `telemetryPort=9999`
5. Restart iRacing

#### Assetto Corsa Competizione (ACC)

**Status**: Full Support

**Configuration Method**: UDP Broadcast (`broadcasting.json`)

**Features**:
- FFB Scalar
- RPM
- Car ID
- DRS status

**Setup**:
```bash
# Auto-configure ACC
wheelctl game configure acc --auto
```

**Manual Configuration**:
1. Open `Documents\Assetto Corsa Competizione\Setup\broadcasting.json`
2. Set `active` to `true`
3. Set `port` to `9996`
4. Set `connectionIp` to your local IP
5. Restart ACC

#### Automobilista 2 (AMS2)

**Status**: Read-Only

**Configuration Method**: Shared Memory

**Features**:
- FFB Scalar
- RPM

**Setup**:
```bash
# Auto-configure AMS2
wheelctl game configure ams2 --auto
```

No manual configuration required - AMS2 exposes shared memory automatically.

#### rFactor 2

**Status**: Experimental

**Configuration Method**: Shared Memory

**Features**:
- FFB Scalar
- RPM
- Full Telemetry

**Note**: rFactor 2 support is experimental. Some features may have limited functionality.

### Auto Profile Switching

OpenRacing can automatically switch profiles based on the game and car you're driving:

1. Create profiles with game and car scope:
   ```bash
   wheelctl profile create profiles/iracing/gt3.json --game iracing --car gt3
   wheelctl profile create profiles/iracing/formula.json --game iracing --car formula
   ```

2. The service will detect the game and car from telemetry
3. The matching profile will be automatically applied

### Troubleshooting Game Integration

#### Telemetry Not Received

1. **Check game configuration**: Ensure telemetry is enabled in game settings
2. **Verify firewall**: Allow UDP traffic on the telemetry port
3. **Check game is running**: Telemetry is only available during sessions
4. **Test connection**: Run `wheelctl game test <game-id>`

#### Profile Not Switching

1. **Verify profile scope**: Ensure profiles have correct game/car tags
2. **Check telemetry**: Verify car ID is being received
3. **Manual apply**: Use `wheelctl profile apply` to test profile manually

#### Anti-Cheat Concerns

OpenRacing is designed to avoid common anti-cheat concerns (not yet validated — see [Anti-Cheat Compatibility](ANTICHEAT_COMPATIBILITY.md)):

- No process injection
- No kernel drivers
- Uses only documented, legitimate APIs
- All binaries will be digitally signed (planned)

For more details, see [ANTICHEAT_COMPATIBILITY.md](ANTICHEAT_COMPATIBILITY.md).

---

## Profiles

Profiles define force feedback settings for your racing wheel. They are stored as JSON files with schema validation.

### Profile Structure

A profile consists of the following sections:

```json
{
  "schema": "wheel.profile/1",
  "scope": {
    "game": "iracing",
    "car": "gt3",
    "track": null
  },
  "base": {
    "ffbGain": 0.75,
    "dorDeg": 900,
    "torqueCapNm": 8.0,
    "filters": {
      "reconstruction": 0,
      "friction": 0.0,
      "damper": 0.0,
      "inertia": 0.0,
      "bumpstop": {
        "enabled": true,
        "strength": 0.5
      },
      "handsOff": {
        "enabled": true,
        "sensitivity": 0.3
      },
      "torqueCap": 10.0,
      "notchFilters": [],
      "slewRate": 1.0,
      "curvePoints": [
        {"input": 0.0, "output": 0.0},
        {"input": 1.0, "output": 1.0}
      ]
    }
  },
  "leds": {
    "rpmBands": [6000, 8000, 9000],
    "pattern": "sequential",
    "brightness": 0.8,
    "colors": {
      "low": [0, 255, 0],
      "mid": [255, 255, 0],
      "high": [255, 0, 0]
    }
  },
  "haptics": {
    "enabled": true,
    "intensity": 0.5,
    "frequencyHz": 100,
    "effects": {
      "engineVibration": true,
      "gearShift": true,
      "lockup": true
    }
  },
  "signature": null
}
```

### Profile Settings

#### Base Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `ffbGain` | Force feedback gain | 0.0 - 1.0 | 0.75 |
| `dorDeg` | Degrees of rotation | 0 - 3600 | 900 |
| `torqueCapNm` | Maximum torque output | 0.1 - device max | device max |

#### Filter Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `reconstruction` | Reconstruction filter level | 0 - 8 | 0 |
| `friction` | Friction effect | 0.0 - 1.0 | 0.0 |
| `damper` | Damper effect | 0.0 - 1.0 | 0.0 |
| `inertia` | Inertia effect | 0.0 - 1.0 | 0.0 |
| `slewRate` | Torque slew rate limiting | 0.0 - 2.0 | 1.0 |

#### Bumpstop Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable bumpstop effect | true/false | true |
| `strength` | Bumpstop strength | 0.0 - 1.0 | 0.5 |

#### Hands-Off Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable hands-off detection | true/false | true |
| `sensitivity` | Detection sensitivity | 0.0 - 1.0 | 0.3 |

#### LED Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `rpmBands` | RPM shift light thresholds | array of floats | [6000, 8000, 9000] |
| `pattern` | LED pattern | "sequential", "bar", "center-out" | "sequential" |
| `brightness` | LED brightness | 0.0 - 1.0 | 0.8 |

#### Haptics Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable haptic effects | true/false | true |
| `intensity` | Haptic intensity | 0.0 - 1.0 | 0.5 |
| `frequencyHz` | Vibration frequency | 10 - 500 | 100 |

### Creating and Editing Profiles

#### Create a New Profile

```bash
# Create default profile
wheelctl profile create my-profile.json

# Create from template
wheelctl profile create profiles/iracing/gt3.json --from my-profile.json --game iracing --car gt3
```

#### Edit a Profile

```bash
# Interactive edit (opens in default editor)
wheelctl profile edit my-profile.json

# Direct field edit
wheelctl profile edit my-profile.json --field base.ffbGain --value 0.8
```

### Importing/Exporting Profiles

#### Export a Profile

```bash
# Export to file
wheelctl profile export my-profile.json --output shared-profile.json

# Export with signature
wheelctl profile export my-profile.json --signed
```

#### Import a Profile

```bash
# Import to default location
wheelctl profile import shared-profile.json

# Import to specific location
wheelctl profile import shared-profile.json --target profiles/community/

# Verify signature on import
wheelctl profile import shared-profile.json --verify
```

### Profile Validation

Profiles are validated against a JSON schema to ensure correctness:

```bash
# Validate profile
wheelctl profile validate my-profile.json

# Validate with detailed output
wheelctl profile validate my-profile.json --detailed
```

**Validation Checks:**
- Schema version compatibility
- Required fields present
- Value ranges within limits
- Curve points are monotonic
- RPM bands are sorted

---

## Safety Features

OpenRacing includes comprehensive safety features to protect you and your equipment.

### Safety Interlocks Explained

Safety interlocks prevent accidental high-torque operation:

1. **Physical Interlock**: Requires holding both clutch paddles for 3 seconds
2. **UI Consent**: Explicit user acknowledgment in the interface
3. **Session Persistence**: Safety state persists until power cycle
4. **Fault Detection**: Automatic torque reduction on fault detection

### Enabling High-Torque Mode

High-torque mode provides maximum force feedback output but requires explicit confirmation:

```bash
# Enable high torque (with confirmation)
wheelctl safety enable <device-id>

# Force enable (skips safety checks - use with caution)
wheelctl safety enable <device-id> --force
```

**Requirements for High-Torque Mode:**
- No active faults
- Device temperature below 80°C
- Hands detected on wheel
- Physical challenge completed (hold both clutch paddles for 3 seconds)

### Emergency Stop Procedures

In case of emergency, you can immediately stop all force feedback:

```bash
# Emergency stop all devices
wheelctl safety stop

# Emergency stop specific device
wheelctl safety stop <device-id>
```

**Physical Emergency Stop:**
- Most wheels have a physical button for emergency stop
- Press and hold to immediately disable force feedback

### Safety Limits Configuration

Set torque limits to protect yourself and your equipment:

```bash
# Set torque limit for current session
wheelctl safety limit <device-id> 8.0

# Set global torque limit (applies to all profiles)
wheelctl safety limit <device-id> 8.0 --global
```

**Recommended Limits:**
- **Beginners**: 3-5 Nm
- **Intermediate**: 5-8 Nm
- **Advanced**: 8-12 Nm
- **Professional**: Up to device maximum

### Fault Detection

OpenRacing continuously monitors for fault conditions:

| Fault | Detection | Response |
|-------|-----------|----------|
| USB Timeout | No device response within 3 frames | Torque ramp-down within 50ms |
| Encoder Error | NaN or out-of-range values | Immediate torque stop |
| Thermal Limit | Temperature > 80°C | Reduced torque mode |
| Overcurrent | Current exceeds limits | Emergency stop |
| Hands Off | No hands detected | Reduced torque |

---

## Troubleshooting

This section provides solutions for common issues and guidance on diagnosing problems with OpenRacing.

### Quick Diagnostics

Before diving into specific issues, run these diagnostic commands:

```bash
# Check overall system health
wheelctl health

# Run comprehensive diagnostics
wheelctl diag test

# Check service status
wheelctl health --watch

# View recent logs
wheelctl diag logs --lines 100
```

### Common Issues and Solutions

#### Device Not Detected

**Symptoms**: `wheelctl device list` shows no devices or missing devices

**Diagnostic Steps:**
```bash
# Check if device is recognized by the OS
# Windows:
Get-PnpDevice | Where-Object { $_.Class -eq "HIDClass" }

# Linux:
lsusb | grep -i "logitech\|fanatec\|thrustmaster"
ls -la /dev/hidraw*

# macOS:
system_profiler SPUSBDataType | grep -A 10 "Wheel\|Racing"
```

**Solutions:**

1. **Check USB connection**
   - Try a different USB port (preferably USB 2.0)
   - Connect directly to motherboard, avoid USB hubs
   - Use a high-quality, short USB cable

2. **Verify device power**
   - Ensure the wheel is powered on
   - Check power supply connections
   - Some wheels require external power

3. **Check permissions (Linux)**
   ```bash
   # Verify udev rules are installed
   ls -la /etc/udev/rules.d/99-racing-wheel-suite.rules
   
   # Reload udev rules
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   
   # Check device permissions
   ls -la /dev/hidraw*
   # Should show mode 0666 or your user should have access
   ```

4. **Check driver status (Windows)**
   - Open Device Manager
   - Look for "Human Interface Devices"
   - Check for yellow warning icons
   - Try "Update driver" or "Uninstall device" and reconnect

5. **Restart the service**
   ```bash
   # Linux
   systemctl --user restart wheeld
   
   # Windows (as Administrator)
   sc stop wheeld && sc start wheeld
   
   # macOS
   launchctl unload ~/Library/LaunchAgents/com.openracing.wheeld.plist
   launchctl load ~/Library/LaunchAgents/com.openracing.wheeld.plist
   ```

#### Service Not Running

**Symptoms**: `wheelctl health` shows "Service: Not Running" or connection errors

**Diagnostic Steps:**
```bash
# Check service status
# Linux
systemctl --user status wheeld
journalctl --user -u wheeld -n 50

# Windows
sc query wheeld
Get-EventLog -LogName Application -Source wheeld -Newest 20

# macOS
launchctl list | grep openracing
log show --predicate 'subsystem == "com.openracing.wheeld"' --last 30m
```

**Solutions:**

1. **Start the service**
   ```bash
   # Linux
   systemctl --user start wheeld
   
   # Windows (as Administrator)
   sc start wheeld
   
   # macOS
   launchctl load ~/Library/LaunchAgents/com.openracing.wheeld.plist
   ```

2. **Check for port conflicts**
   ```bash
   # Linux/macOS
   lsof -i :9999  # Default IPC port
   
   # Windows
   netstat -ano | findstr :9999
   ```

3. **Verify installation**
   ```bash
   # Check binary exists
   which wheeld
   wheeld --version
   ```

4. **Check configuration**
   ```bash
   wheelctl config validate
   ```

5. **Reinstall service**
   ```bash
   # Windows (as Administrator)
   wheeld uninstall
   wheeld install
   sc start wheeld
   ```

#### High Jitter/Latency

**Symptoms**: Inconsistent force feedback, stuttering, delayed response

**Diagnostic Steps:**
```bash
# Monitor performance metrics
wheelctl diag metrics --watch

# Check for missed ticks
wheelctl diag test --type timing

# Record performance data
wheelctl diag record <device-id> --duration 60
```

**Solutions:**

1. **Optimize power settings**

   **Windows:**
   ```cmd
   # Set high performance power plan
   powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
   
   # Disable USB selective suspend
   powercfg /setacvalueindex SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bebba308a3 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 0
   powercfg /setactive SCHEME_CURRENT
   ```

   **Linux:**
   ```bash
   # Set CPU governor to performance
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   
   # Disable USB autosuspend
   echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms
   
   # Disable CPU frequency scaling
   sudo systemctl disable ondemand
   ```

2. **Use USB 2.0 ports**
   - USB 3.0 can introduce additional latency for HID devices
   - Connect to USB 2.0 ports on the motherboard

3. **Close background applications**
   - Disable unnecessary startup programs
   - Close browser tabs and streaming software
   - Disable antivirus real-time scanning during racing

4. **Check CPU thermal throttling**
   ```bash
   # Linux
   cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq
   sensors  # If lm-sensors is installed
   
   # Windows
   # Use Task Manager > Performance > CPU
   ```

5. **Adjust reconstruction filter**
   ```bash
   # Lower reconstruction filter reduces latency
   wheelctl profile edit <profile> --field base.filters.reconstruction --value 0
   ```

#### Device Disconnects Randomly

**Symptoms**: Wheel disconnects during use, requires reconnection

**Diagnostic Steps:**
```bash
# Check for disconnect events
wheelctl diag logs --filter "disconnect"

# Monitor device status
wheelctl device status <device-id> --watch
```

**Solutions:**

1. **Check USB cable**
   - Use a high-quality, shielded USB cable
   - Keep cable length under 2 meters
   - Avoid cable stress and sharp bends

2. **Disable USB power saving**

   **Windows:**
   - Device Manager > USB Root Hub > Properties > Power Management
   - Uncheck "Allow the computer to turn off this device"

   **Linux:**
   ```bash
   # Disable autosuspend for all USB devices
   echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms
   
   # Or add to /etc/udev/rules.d/99-usb-power.rules:
   ACTION=="add", SUBSYSTEM=="usb", ATTR{power/autosuspend_delay_ms}="-1"
   ```

3. **Check for EMI interference**
   - Move wheel away from other electronics
   - Use a ferrite core on the USB cable

4. **Update USB drivers (Windows)**
   - Download latest chipset drivers from motherboard manufacturer
   - Update USB controller drivers

#### Force Feedback Issues

##### FFB Too Weak

**Solutions:**
1. Increase `ffbGain` in profile (0.0 - 1.0)
2. Check game FFB settings (often separate from OpenRacing)
3. Verify `torqueCapNm` isn't limiting output
4. Check safety torque limits: `wheelctl safety status`
5. Recalibrate: `wheelctl device calibrate <device-id> --type all`

##### FFB Too Strong

**Solutions:**
1. Decrease `ffbGain` in profile
2. Lower `torqueCapNm` in profile
3. Set safety limit: `wheelctl safety limit <device-id> 5.0`
4. Increase `damper` filter for smoother feel
5. Check game FFB multiplier settings

##### FFB Oscillation/Vibration

**Solutions:**
1. Increase `damper` filter (0.1 - 0.3 recommended)
2. Add `friction` filter (0.05 - 0.15)
3. Lower `reconstruction` filter
4. Reduce `slewRate` for smoother transitions
5. Check for mechanical issues with the wheel

##### FFB Clipping

**Symptoms**: FFB feels flat at high forces, loss of detail

**Solutions:**
1. Reduce `ffbGain` to prevent saturation
2. Lower game FFB strength
3. Use a response curve to compress high forces:
   ```json
   {
     "curvePoints": [
       {"input": 0.0, "output": 0.0},
       {"input": 0.5, "output": 0.6},
       {"input": 1.0, "output": 1.0}
     ]
   }
   ```

#### Game Integration Issues

##### Telemetry Not Received

**Diagnostic Steps:**
```bash
# Test game connection
wheelctl game test <game-id> --duration 30

# Check telemetry status
wheelctl game status --telemetry
```

**Solutions:**

1. **Verify game configuration**
   ```bash
   # Auto-configure game
   wheelctl game configure <game-id> --auto
   ```

2. **Check firewall settings**
   - Allow UDP traffic on telemetry ports
   - iRacing: Port 9999
   - ACC: Port 9996

3. **Verify game is running**
   - Telemetry only available during active sessions
   - Some games require being on track

4. **Manual game configuration**
   - See [Game Integration](#game-integration) section for game-specific setup

##### Profile Not Auto-Switching

**Solutions:**
1. Verify profile has correct scope:
   ```bash
   wheelctl profile show <profile> | grep scope
   ```
2. Check auto-switch is enabled:
   ```bash
   wheelctl config get auto_profile_switch
   ```
3. Verify telemetry is providing car ID:
   ```bash
   wheelctl game status --telemetry
   ```

#### Profile Issues

##### Profile Won't Load

**Diagnostic Steps:**
```bash
# Validate profile
wheelctl profile validate <profile> --detailed
```

**Solutions:**
1. Check JSON syntax errors
2. Verify schema version compatibility
3. Check parent profile exists (if using inheritance)
4. Ensure all required fields are present

##### Circular Inheritance Error

**Solution:**
Review profile inheritance chain and remove circular references:
```bash
# Check inheritance chain
wheelctl profile show <profile> --inheritance
```

### Error Messages Reference

| Error | Cause | Solution |
|-------|-------|----------|
| `DeviceNotFound` | Device not connected or not recognized | Check USB connection, reload udev rules |
| `ServiceUnavailable` | wheeld service not running | Start the service |
| `PermissionDenied` | Insufficient permissions | Check udev rules (Linux), run as admin (Windows) |
| `ProfileValidationFailed` | Invalid profile JSON | Run `wheelctl profile validate` |
| `TorqueLimitExceeded` | Requested torque above safety limit | Adjust safety limits or profile settings |
| `CommunicationTimeout` | USB communication failed | Check cable, try different port |
| `WatchdogTimeout` | Safety watchdog triggered | Check for system performance issues |
| `PluginLoadFailed` | Plugin couldn't be loaded | Check plugin signature and compatibility |

### Diagnostic Tools

#### Generate Support Bundle

When reporting issues, generate a support bundle:

```bash
# Basic support bundle
wheelctl diag support

# Include blackbox recording
wheelctl diag support --blackbox

# Include extended logs
wheelctl diag support --blackbox --logs-days 7

# Specify output location
wheelctl diag support --output ~/Desktop/openracing-support.zip
```

**Support bundle contents:**
- System information (OS, CPU, RAM, USB controllers)
- OpenRacing version and configuration
- Device information and calibration data
- Service logs (last 24 hours)
- Diagnostic test results
- Performance metrics
- Fault history
- Optional: Blackbox recordings

#### Blackbox Recording

Record detailed telemetry for analysis:

```bash
# Record 2 minutes of data
wheelctl diag record <device-id> --duration 120

# Record with high detail
wheelctl diag record <device-id> --duration 60 --detail high

# Replay recording
wheelctl diag replay recording.wbb --verbose

# Export to CSV for analysis
wheelctl diag replay recording.wbb --export csv --output data.csv
```

#### Performance Analysis

```bash
# Real-time performance monitoring
wheelctl diag metrics --watch

# Detailed timing analysis
wheelctl diag test --type timing --duration 60

# Generate performance report
wheelctl diag metrics --report --output perf-report.json
```

### Getting Help

If you're still experiencing issues after trying the solutions above:

1. **Search existing issues**: [GitHub Issues](https://github.com/EffortlessMetrics/OpenRacing/issues)
2. **Check discussions**: [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions)
3. **Generate support bundle**: `wheelctl diag support --blackbox`
4. **Open a new issue** with:
   - OpenRacing version (`wheelctl --version`)
   - Operating system and version
   - Racing wheel model
   - Steps to reproduce the issue
   - Support bundle attachment
5. **Join the community**: Discord server (link in README)

---

## Advanced Topics

### Power Management

Optimizing power management settings is crucial for consistent performance. See [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) for detailed guidance.

**Quick Tips:**

**Windows:**
```cmd
# Set high performance power plan
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c

# Disable USB selective suspend
# Use Device Manager or registry settings
```

**Linux:**
```bash
# Set CPU governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable USB autosuspend
echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms
```

### Anti-Cheat Compatibility

OpenRacing is designed to avoid common anti-cheat concerns (not yet validated — see [Anti-Cheat Compatibility](ANTICHEAT_COMPATIBILITY.md)):

- **No Process Injection**: External communication only
- **No Kernel Drivers**: User-space operation only
- **Documented Methods**: Official APIs and documented interfaces
- **Signed Binaries**: All executables will be digitally signed (planned)

For detailed information, see [ANTICHEAT_COMPATIBILITY.md](ANTICHEAT_COMPATIBILITY.md).

### Plugin Installation

OpenRacing supports plugins for extending functionality. Plugins can provide:

- Custom DSP (Digital Signal Processing) effects
- Additional telemetry adapters
- Custom LED patterns
- Haptic effects

For plugin development information, see [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md).

**Installing a Plugin:**

```bash
# Copy plugin to plugins directory
cp my-plugin.wasm ~/.wheel/plugins/

# Enable plugin
wheelctl plugin enable my-plugin

# List installed plugins
wheelctl plugin list
```

### Performance Tuning

#### Monitor Performance

```bash
# Watch performance metrics in real-time
wheelctl diag metrics --watch

# Check service health
wheelctl health --watch
```

#### Optimize Settings

1. **Reduce filter complexity**: Lower `reconstruction` and `notchFilters` count
2. **Adjust slew rate**: Set `slewRate` to 1.0 for maximum responsiveness
3. **Disable unnecessary effects**: Turn off `haptics` if not needed
4. **Profile-specific tuning**: Create profiles optimized for different scenarios

#### Performance Targets

| Metric | Target | Acceptable |
|--------|--------|------------|
| Tick Rate | 1000 Hz | > 900 Hz |
| Jitter (p99) | ≤ 0.25ms | ≤ 0.5ms |
| Processing Time | ≤ 200μs | ≤ 500μs |
| HID Write Latency (p99) | ≤ 300μs | ≤ 500μs |
| Total Added Latency | ≤ 2ms | ≤ 5ms |

---

## FAQ

### General

**Q: Is OpenRacing free?**  
A: Yes, OpenRacing is open-source and free to use. It is dual-licensed under MIT and Apache-2.0.

**Q: What racing wheels are supported?**  
A: OpenRacing supports most HID-compliant racing wheels. Commonly tested devices include Logitech G-series, Fanatec CSL/ClubSport, Thrustmaster T-series, and direct drive wheels.

**Q: Can I use OpenRacing with multiple wheels?**  
A: Yes, OpenRacing supports multiple connected devices simultaneously.

**Q: Does OpenRacing work on macOS?**  
A: OpenRacing compiles on macOS 10.15+, but the IOKit HID driver is not yet implemented. Device I/O (wheels, pedals) does not work on macOS yet. See the [ROADMAP](../ROADMAP.md) for planned macOS support.

### Installation

**Q: Do I need to install drivers?**  
A: No, OpenRacing uses standard HID drivers provided by your operating system.

**Q: Can I install OpenRacing without admin rights?**  
A: On Windows and macOS, admin rights are not required. On Linux, you may need sudo for udev rules and service installation.

**Q: How do I uninstall OpenRacing?**  
A: Run the uninstaller (Windows) or remove the installed files and service (Linux/macOS).

### Configuration

**Q: Where are profiles stored?**  
A: Profiles are stored in:
- Windows: `%LOCALAPPDATA%\Wheel\profiles\`
- Linux/macOS: `~/.wheel/profiles/`

**Q: Can I share my profiles with others?**  
A: Yes, profiles are JSON files that can be exported and imported. Use `wheelctl profile export` to share.

**Q: How do I reset to default settings?**  
A: Use `wheelctl device reset <device-id>` to reset the device to safe state, or delete/recreate profiles.

### Performance

**Q: Why is my FFB jittery?**  
A: Jitter is usually caused by power management settings. See [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) for optimization tips.

**Q: What is the ideal tick rate?**  
A: OpenRacing targets 1000 Hz (1ms intervals) for optimal force feedback quality.

**Q: Can I reduce latency further?**  
A: Ensure you're using USB 2.0 ports, disable power saving, and close background applications.

### Safety

**Q: Why does high torque mode require confirmation?**  
A: High torque mode can be dangerous. The physical interlock prevents accidental activation.

**Q: What happens if a fault is detected?**  
A: OpenRacing immediately ramps down torque within 50ms and logs the fault for diagnostics.

**Q: Can I disable safety interlocks?**  
A: Safety interlocks cannot be disabled. They are essential for safe operation.

### Games

**Q: Will OpenRacing get me banned?**  
A: OpenRacing uses only legitimate, documented methods and is designed to avoid common anti-cheat concerns, though this has not yet been validated in live game environments.

**Q: Can I use OpenRacing with games not officially supported?**  
A: OpenRacing provides basic FFB support for any game. Full telemetry integration requires game-specific adapters.

**Q: How do I add support for a new game?**  
A: See [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) for information on creating telemetry adapters.

---

## Glossary

| Term | Definition |
|------|------------|
| **FFB** | Force Feedback - the tactile feedback from the racing wheel that simulates road surface, tire grip, and vehicle physics |
| **DOR** | Degrees of Rotation - the total angle the wheel can turn from lock to lock |
| **HID** | Human Interface Device - the standard protocol for input devices like racing wheels |
| **IPC** | Inter-Process Communication - how OpenRacing components communicate with each other |
| **Jitter** | Variability in timing - lower jitter means smoother, more consistent force feedback |
| **Nm** | Newton-meter - the unit of torque used to measure force feedback strength |
| **p99** | 99th percentile - a statistical measure indicating that 99% of values fall below this threshold |
| **Profile** | A configuration file containing force feedback settings for a specific game/car combination |
| **RT** | Real-Time - processing that guarantees deterministic timing with minimal latency |
| **Telemetry** | Data from the racing game including speed, RPM, gear, and vehicle physics |
| **Torque Cap** | The maximum torque output allowed by the device or profile |
| **UDP** | User Datagram Protocol - a network protocol used for game telemetry |
| **Udev** | Linux device manager that handles device permissions and hot-plug events |

---

## Additional Resources

- [Development Guide](DEVELOPMENT.md) - Contributing to OpenRacing
- [System Integration](SYSTEM_INTEGRATION.md) - Technical integration details
- [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) - Optimizing system performance
- [Anti-Cheat Compatibility](ANTICHEAT_COMPATIBILITY.md) - Anti-cheat information
- [Plugin Development](PLUGIN_DEVELOPMENT.md) - Creating custom plugins
- [GitHub Repository](https://github.com/EffortlessMetrics/OpenRacing) - Source code and issues
- [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions) - Community discussions

---

**Document Version**: 2.0
**Last Updated**: 2026-02-15
**OpenRacing Version**: 0.1.0 (v0.x.y - pre-hardware sign-off)
