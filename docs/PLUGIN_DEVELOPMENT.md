# Plugin Development Guide

## Table of Contents

1. [Introduction](#introduction)
2. [Plugin Architecture](#plugin-architecture)
3. [Safe Plugins (WASM)](#safe-plugins-wasm)
4. [Fast Plugins (Native)](#fast-plugins-native)
   - [ABI Requirements and Version Compatibility](#abi-requirements-and-version-compatibility)
   - [Code Signing and Trust Store](#code-signing-and-trust-store)
5. [Plugin Manifest](#plugin-manifest)
6. [Development Setup](#development-setup)
7. [Deployment](#deployment)
8. [Best Practices](#best-practices)
9. [Troubleshooting](#troubleshooting)
10. [References](#references)
11. [Appendix](#appendix)
    - [A. Plugin Manifest Schema](#a-plugin-manifest-schema)
    - [B. ABI Version History](#b-abi-version-history)
    - [C. Quick Reference](#c-quick-reference)

---

## Introduction

The OpenRacing plugin system provides a flexible, secure, and high-performance extensibility framework for racing wheel software. Plugins enable community developers to extend functionality without compromising system stability or real-time (RT) guarantees.

### Why Plugins Exist

- **Extensibility**: Community contributions can add custom telemetry processing, LED patterns, DSP filters, and force feedback effects
- **Safety**: Sandboxed execution prevents plugins from compromising system stability
- **Performance**: Two-tier architecture balances safety with RT-critical performance requirements
- **Isolation**: Plugin crashes don't affect the main service
- **Security**: Capability-based permissions restrict plugin access to system resources

### Key Features

- **Two-tier architecture**: Safe WASM plugins for general use, Fast native plugins for RT operations
- **Capability-based security**: Plugins must declare required permissions
- **Automatic budget enforcement**: CPU and memory limits prevent performance degradation
- **Quarantine system**: Repeatedly failing plugins are automatically disabled
- **ABI versioning**: Ensures compatibility across plugin ecosystem evolution

---

## Plugin Architecture

OpenRacing implements a two-tier plugin architecture designed to balance safety, performance, and flexibility.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     OpenRacing Service                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────────────┐         ┌──────────────────────┐     │
│  │   WASM Plugin Host   │         │  Native Plugin Host  │     │
│  │   (Safe Plugins)     │         │   (Fast Plugins)     │     │
│  ├──────────────────────┤         ├──────────────────────┤     │
│  │ - Sandboxed          │         │ - Isolated Process   │     │
│  │ - 60-200Hz           │         │ - 1kHz               │     │
│  │ - Capability-based   │         │ - Shared Memory IPC  │     │
│  │ - Auto-restart       │         │ - Watchdog Timer     │     │
│  └──────────────────────┘         └──────────────────────┘     │
│           │                                │                    │
│           │ 60-200Hz                       │ 1kHz               │
│           ▼                                ▼                    │
│  ┌──────────────────────┐         ┌──────────────────────┐     │
│  │   Telemetry          │         │   DSP Pipeline       │     │
│  │   Processing         │         │   (RT Thread)        │     │
│  │   LED Mapping        │         │   FFB Effects        │     │
│  └──────────────────────┘         └──────────────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Safe Plugins (WASM)

| Characteristic | Value |
|----------------|-------|
| Execution Environment | Sandboxed WASM/WASI runtime |
| Update Rate | 60-200 Hz |
| Memory Limit | 16 MB (configurable) |
| Execution Budget | 5 ms per tick |
| Capabilities | ReadTelemetry, ModifyTelemetry, ControlLeds, InterPluginComm |
| Isolation | Process-level with automatic restart |
| Crash Recovery | Automatic with exponential backoff |

### Fast Plugins (Native)

| Characteristic | Value |
|----------------|-------|
| Execution Environment | Isolated `wheel-dsp` helper process |
| Update Rate | 1 kHz |
| Memory Limit | 4 MB |
| Execution Budget | 200 μs per tick |
| Capabilities | All Safe capabilities + ProcessDsp |
| Isolation | Separate process with SPSC shared memory |
| Crash Recovery | Quarantine policy with escalating timeouts |

### When to Use Each Type

**Use Safe Plugins (WASM) when:**
- Processing telemetry data (e.g., calculating derived metrics)
- Implementing custom LED patterns and mappings
- Adding game-specific telemetry enhancements
- Developing community-contributed features
- Security and stability are priorities

**Use Fast Plugins (Native) when:**
- Implementing DSP filters for force feedback
- Creating custom FFB effects requiring microsecond timing
- Processing high-frequency sensor data
- Performance is critical and RT guarantees are required
- You have experience with real-time programming

### Performance Characteristics

| Metric | Safe Plugin | Fast Plugin |
|--------|-------------|-------------|
| Latency | 5-16 ms (60-200Hz) | < 1 ms (1kHz) |
| Throughput | 60-200 ops/sec | 1000 ops/sec |
| Memory Overhead | ~2 MB (WASM runtime) | ~1 MB (helper process) |
| IPC Overhead | Minimal (in-process) | ~10 μs (shared memory) |
| Crash Impact | Auto-restart < 1s | Quarantine (60 min+) |

---

## Safe Plugins (WASM)

Safe plugins run in a WebAssembly (WASM) sandbox with capability-based permissions, providing a secure environment for community contributions.

### Overview and Sandboxing Model

WASM plugins execute within the Wasmtime runtime with the following security features:

- **Capability-based permissions**: Plugins must declare required capabilities in their manifest
- **WASI sandboxing**: File system and network access are restricted by default
- **Fuel consumption**: CPU time is tracked and limited per execution
- **Epoch interruption**: Long-running operations can be interrupted
- **Memory isolation**: Plugin memory is separate from host memory
- **No direct hardware access**: All hardware interaction goes through host functions

### Capability-Based Permissions

Safe plugins can request the following capabilities:

| Capability | Description | Use Case |
|------------|-------------|----------|
| `ReadTelemetry` | Read incoming telemetry data | Telemetry processing, analytics |
| `ModifyTelemetry` | Modify telemetry before it reaches other systems | Custom FFB scaling, signal conditioning |
| `ControlLeds` | Control wheel LED patterns | Custom RPM displays, flag indicators |
| `FileSystem` | Access specific file paths | Logging, configuration persistence |
| `Network` | Access specific network hosts | Telemetry streaming, cloud services |
| `InterPluginComm` | Communicate with other plugins | Plugin chaining, shared state |

### Update Rate and Budgets

Safe plugins operate at 60-200 Hz with the following constraints:

```rust
// Maximum execution time per tick
max_execution_time_us: 5000  // 5 milliseconds

// Maximum memory allocation
max_memory_bytes: 16 * 1024 * 1024  // 16 MB

// Supported update rates
update_rate_hz: 60, 100, 200  // Configurable
```

Budget violations result in:
1. Warning logged
2. Plugin throttled (update rate reduced)
3. Repeated violations → quarantine

### Use Cases

#### Telemetry Processing

- Calculate derived metrics (e.g., slip angle, g-forces)
- Implement custom FFB scaling based on vehicle state
- Add game-specific telemetry enhancements
- Filter noise from sensor data

#### LED Mapping

- Create custom RPM displays
- Implement shift indicators
- Show race flags and warnings
- Display custom patterns for events

### Development Workflow

#### 1. Project Setup

Create a new Rust project with WASM target:

```bash
cargo new my_safe_plugin --lib
cd my_safe_plugin

# Add WASM target
rustup target add wasm32-wasi
```

#### 2. Configure Cargo.toml

```toml
[package]
name = "my_safe_plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
racing_wheel_plugins = { path = "../../crates/plugins" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

#### 3. Implement the Plugin

```rust
use racing_wheel_plugins::sdk::*;
use serde_json::Value;

#[derive(Default)]
pub struct MySafePlugin {
    // Plugin state
}

impl WasmPlugin for MySafePlugin {
    fn initialize(&mut self, config: Value) -> SdkResult<()> {
        // Parse configuration
        Ok(())
    }
    
    fn process_telemetry(&mut self, input: SdkTelemetry, context: SdkContext) -> SdkResult<SdkOutput> {
        // Process telemetry
        Ok(SdkOutput::Telemetry {
            telemetry: input,
            custom_data: Value::Null,
        })
    }
    
    fn process_led_mapping(&mut self, input: SdkLedInput, context: SdkContext) -> SdkResult<SdkOutput> {
        // Process LED mapping
        Ok(SdkOutput::Led {
            led_pattern: vec![],
            brightness: 1.0,
            duration_ms: 50,
        })
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        // Cleanup
        Ok(())
    }
}

// Export the plugin
racing_wheel_plugins::export_wasm_plugin!(MySafePlugin);
```

#### 4. Build for WASM

```bash
cargo build --release --target wasm32-wasi
```

#### 5. Create Manifest

Create `plugin.yaml`:

```yaml
id: "550e8400-e29b-41d4-a716-446655440000"
name: "My Safe Plugin"
version: "0.1.0"
description: "A sample safe plugin"
author: "Your Name"
license: "MIT"
homepage: "https://github.com/yourname/my-safe-plugin"
class: Safe
capabilities:
  - ReadTelemetry
  - ControlLeds
operations:
  - TelemetryProcessor
  - LedMapper
constraints:
  max_execution_time_us: 5000
  max_memory_bytes: 16777216
  update_rate_hz: 60
entry_points:
  wasm_module: "target/wasm32-wasi/release/my_safe_plugin.wasm"
  main_function: "process"
  init_function: "initialize"
  cleanup_function: "shutdown"
```

### Code Examples

#### Example 1: RPM-Based LED Plugin

Reference: [`crates/plugins/examples/sample_led_plugin.rs`](../crates/plugins/examples/sample_led_plugin.rs)

```rust
use racing_wheel_plugins::sdk::*;
use serde_json::Value;

#[derive(Default)]
pub struct SampleLedPlugin {
    max_rpm: f32,
    shift_point: f32,
}

impl WasmPlugin for SampleLedPlugin {
    fn initialize(&mut self, config: Value) -> SdkResult<()> {
        self.max_rpm = config
            .get("max_rpm")
            .and_then(|v| v.as_f64())
            .unwrap_or(8000.0) as f32;
        
        self.shift_point = config
            .get("shift_point")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.9) as f32;
        
        Ok(())
    }
    
    fn process_led_mapping(&mut self, input: SdkLedInput, _context: SdkContext) -> SdkResult<SdkOutput> {
        let leds = if input.telemetry.flags.red_flag || input.telemetry.flags.yellow_flag {
            // Show flag colors
            vec![SdkLedColor { r: 255, g: 0, b: 0 }; input.led_count as usize]
        } else {
            // Show RPM pattern
            let normalized_rpm = (input.telemetry.rpm / self.max_rpm).clamp(0.0, 1.0);
            let active_leds = (normalized_rpm * input.led_count as f32) as u32;
            
            (0..input.led_count)
                .map(|i| {
                    if i < active_leds {
                        if normalized_rpm > self.shift_point {
                            SdkLedColor { r: 255, g: 0, b: 0 } // Red at shift point
                        } else {
                            SdkLedColor { r: 0, g: 255, b: 0 } // Green
                        }
                    } else {
                        SdkLedColor { r: 0, g: 0, b: 0 } // Off
                    }
                })
                .collect()
        };
        
        Ok(SdkOutput::Led {
            led_pattern: leds,
            brightness: 1.0,
            duration_ms: 50,
        })
    }
    
    fn process_telemetry(&mut self, _input: SdkTelemetry, _context: SdkContext) -> SdkResult<SdkOutput> {
        Err(SdkError::CapabilityRequired("ReadTelemetry".to_string()))
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        Ok(())
    }
}
```

#### Example 2: Telemetry Processing Plugin

Reference: [`crates/plugins/examples/sample_telemetry_plugin.rs`](../crates/plugins/examples/sample_telemetry_plugin.rs)

```rust
use racing_wheel_plugins::sdk::*;
use serde_json::Value;

#[derive(Default)]
pub struct SampleTelemetryPlugin {
    frame_count: u64,
}

impl WasmPlugin for SampleTelemetryPlugin {
    fn initialize(&mut self, _config: Value) -> SdkResult<()> {
        self.frame_count = 0;
        Ok(())
    }
    
    fn process_telemetry(&mut self, mut input: SdkTelemetry, _context: SdkContext) -> SdkResult<SdkOutput> {
        self.frame_count += 1;
        
        // Add custom data
        input.custom_data.insert(
            "frame_count".to_string(),
            Value::Number(self.frame_count.into()),
        );
        
        // Modify FFB based on slip ratio
        if input.slip_ratio > 0.1 {
            input.ffb_scalar *= 1.1; // Increase FFB when slipping
        }
        
        Ok(SdkOutput::Telemetry {
            telemetry: input,
            custom_data: Value::Object(serde_json::Map::new()),
        })
    }
    
    fn process_led_mapping(&mut self, _input: SdkLedInput, _context: SdkContext) -> SdkResult<SdkOutput> {
        Err(SdkError::CapabilityRequired("ControlLeds".to_string()))
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        Ok(())
    }
}
```

---

## Fast Plugins (Native)

Fast plugins provide high-performance, real-time processing for DSP filters and custom FFB effects. They run in an isolated helper process with strict timing budgets.

### Overview and Isolation Model

Native plugins are loaded as shared libraries (`.dll` on Windows, `.so` on Linux, `.dylib` on macOS) and execute in a dedicated `wheel-dsp` helper process. This architecture provides:

- **Process isolation**: Plugin crashes don't affect the main service
- **RT scheduling**: Helper process runs with real-time priority
- **Shared memory IPC**: SPSC ring buffer for minimal latency
- **Watchdog enforcement**: Automatic termination on budget violations
- **ABI versioning**: Ensures compatibility across versions

### Helper Process Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Main Service                              │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              RT Thread (1kHz)                         │  │
│  │                                                        │  │
│  │  ┌──────────────┐         ┌──────────────┐           │  │
│  │  │  Ring Buffer │◄────────┤  Ring Buffer │           │  │
│  │  │  (Producer)  │ Shared  │  (Consumer)  │           │  │
│  │  └──────────────┘ Memory  └──────────────┘           │  │
│  │         │                       ▲                      │  │
│  │         │                       │                      │  │
│  └─────────┼───────────────────────┼──────────────────────┘  │
│            │                       │                          │
│            │ IPC                   │ IPC                     │
│            │                       │                          │
│  ┌─────────▼───────────────────────▼──────────────────────┐  │
│  │         wheel-dsp Helper Process                         │  │
│  │                                                           │  │
│  │  ┌─────────────────────────────────────────────────┐   │  │
│  │  │         Native Plugin (.dll/.so)                 │   │  │
│  │  │                                                   │   │  │
│  │  │  ┌──────────────┐                                │   │  │
│  │  │  │ plugin_create │──► Initialize state           │   │  │
│  │  │  ├──────────────┤                                │   │  │
│  │  │  │ plugin_process│──► Process frame (RT-safe)     │   │  │
│  │  │  ├──────────────┤                                │   │  │
│  │  │  │ plugin_destroy│──► Cleanup                     │   │  │
│  │  │  └──────────────┘                                │   │  │
│  │  └─────────────────────────────────────────────────┘   │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Update Rate and Budgets

Fast plugins operate at 1 kHz with strict timing constraints:

```c
// Maximum execution time per tick
#define MAX_BUDGET_US 200  // 200 microseconds

// Shared memory ring buffer
#define RING_BUFFER_SIZE 1024  // Frames
#define FRAME_SIZE sizeof(PluginFrame)  // 32 bytes
```

Budget violations result in:
1. Immediate frame rejection
2. Warning logged
3. Repeated violations → quarantine (escalating duration)

### Use Cases

#### DSP Filters

- Low-pass/high-pass filters for force feedback
- Notch filters to remove resonance frequencies
- Custom signal processing algorithms
- Adaptive filters based on wheel speed

#### Custom FFB Effects

- Road surface effects
- Suspension simulation
- Tire slip effects
- Custom game-specific effects

### ABI Requirements and Version Compatibility

Native plugins must adhere to strict ABI (Application Binary Interface) requirements to ensure compatibility with the OpenRacing plugin loader. The ABI defines the binary contract between plugins and the host system.

#### Current ABI Version

```c
// Current ABI version - plugins MUST match this exactly
#define CURRENT_ABI_VERSION 1
```

The current ABI version is **1**. Plugins with any other ABI version will be rejected at load time with an `AbiMismatch` error.

#### ABI Compatibility Rules

1. **Exact Version Match Required**: The plugin's `abi_version` field in `PluginInfo` must exactly match `CURRENT_ABI_VERSION`. There is no backward or forward compatibility—mismatched versions are always rejected.

2. **Breaking Changes Increment Version**: When the host makes breaking changes to:
   - Function signatures in `PluginVTable`
   - Structure layouts (`PluginFrame`, `PluginInfo`)
   - Calling conventions or memory layout
   
   The `CURRENT_ABI_VERSION` is incremented, requiring plugin recompilation.

3. **Version Check Timing**: ABI version is checked immediately after loading the shared library, before any plugin code executes.

#### Required Structures

All native plugins must define these structures with exact memory layouts:

```c
// Plugin frame for RT communication (32 bytes, packed)
#[repr(C)]
typedef struct {
    float ffb_in;           // Input force feedback value
    float torque_out;       // Output torque value
    float wheel_speed;      // Current wheel speed (rad/s)
    uint64_t timestamp_ns;  // Frame timestamp in nanoseconds
    uint32_t budget_us;     // Execution budget in microseconds
    uint32_t sequence;      // Frame sequence number
} PluginFrame;

// Plugin metadata
typedef struct {
    const char* name;        // Plugin display name
    const char* version;     // Semantic version string
    const char* author;      // Author name
    const char* description; // Brief description
    uint32_t abi_version;    // MUST equal CURRENT_ABI_VERSION
} PluginInfo;

// Plugin function table (vtable)
typedef struct {
    void* (*create)(const uint8_t* config, size_t config_len);
    int (*process)(void* state, float ffb_in, float wheel_speed, 
                   float wheel_angle, float dt, float* ffb_out);
    void (*destroy)(void* state);
    PluginInfo (*get_info)(void);
} PluginVTable;
```

#### Required Export Function

Every native plugin must export a single function:

```c
// This function MUST be exported with C linkage
extern "C" PluginVTable get_plugin_vtable(void);
```

The loader calls this function to obtain the plugin's vtable. The `abi_version` field in the returned `PluginInfo` is checked before any other plugin functions are called.

#### ABI Mismatch Error

When a plugin's ABI version doesn't match, the loader returns:

```rust
NativePluginLoadError::AbiMismatch {
    expected: CURRENT_ABI_VERSION,  // What the host expects
    actual: plugin_abi_version,      // What the plugin reported
}
```

**Resolution**: Recompile the plugin against the current OpenRacing SDK headers.

#### ABI Version History

| Version | Date | Changes |
|---------|------|---------|
| 1 | 2024-01-15 | Initial ABI release |

### Code Signing and Trust Store

Native plugins require Ed25519 code signing for security. The signing system uses a trust store to manage trusted public keys and verify plugin authenticity.

#### Signature Verification Modes

The plugin loader supports different security configurations:

| `require_signatures` | `allow_unsigned` | Behavior |
|---------------------|------------------|----------|
| `true` | `false` | **Strict mode** (Production): Only signed plugins with valid signatures are loaded |
| `true` | `true` | **Permissive mode**: Signed plugins verified, unsigned allowed with warning |
| `false` | `true` | **Development mode**: No signature verification |
| `false` | `false` | Same as strict mode |

**Recommendation**: Use strict mode (`require_signatures: true`, `allow_unsigned: false`) in production.

#### Trust Store Management

The trust store manages public keys and their trust levels:

```rust
// Trust levels
pub enum TrustLevel {
    Trusted,     // Key is explicitly trusted
    Unknown,     // Key is not in trust store
    Distrusted,  // Key is explicitly distrusted (blocked)
}
```

**Trust Store Operations**:

```bash
# Add a trusted key
wheelctl trust add --key plugin_public.pem --level trusted --reason "Verified developer"

# List trusted keys
wheelctl trust list

# Remove a key (user keys only, system keys are protected)
wheelctl trust remove --fingerprint <key-fingerprint>

# Import keys from file
wheelctl trust import --file trusted_keys.json

# Export keys for sharing
wheelctl trust export --file my_keys.json --include-system false
```

**Trust Store Location**: `~/.config/openracing/trust_store.json`

#### Signature Metadata

Each signed plugin includes metadata in a detached `.sig` file:

```json
{
  "signature": "base64-encoded-ed25519-signature",
  "key_fingerprint": "sha256-hash-of-public-key-in-hex",
  "signer": "Developer Name",
  "timestamp": "2024-01-15T10:30:00Z",
  "content_type": "Plugin",
  "comment": "Optional description"
}
```

#### Signing Workflow

1. **Generate signing key pair**:
   ```bash
   # Generate Ed25519 private key (keep this secret!)
   openssl genpkey -algorithm ED25519 -out plugin_private.pem
   
   # Extract public key for distribution
   openssl pkey -in plugin_private.pem -pubout -out plugin_public.pem
   
   # Get key fingerprint (SHA256 of public key bytes)
   wheelctl crypto fingerprint --key plugin_public.pem
   ```

2. **Sign the plugin binary**:
   ```bash
   # Sign the shared library
   wheelctl plugin sign --library libmy_plugin.so --key plugin_private.pem --signer "Your Name"
   
   # This creates libmy_plugin.so.sig alongside the library
   ```

3. **Verify signature** (optional):
   ```bash
   wheelctl plugin verify --library libmy_plugin.so --key plugin_public.pem
   ```

4. **Distribute your public key** to users who want to trust your plugins.

#### Unsigned Plugin Configuration

For development, you can allow unsigned plugins:

```yaml
# ~/.config/openracing/config.yaml
plugins:
  native:
    allow_unsigned: true        # Allow unsigned plugins (development only!)
    require_signatures: false   # Skip signature verification
```

**Security Warning**: Never enable `allow_unsigned` in production environments.

### Development Workflow

#### 1. Project Setup

Create a new C project:

```bash
mkdir my_fast_plugin
cd my_fast_plugin
```

#### 2. Create Header File

Create `plugin.h`:

```c
#ifndef PLUGIN_H
#define PLUGIN_H

#include <stdint.h>

#define PLUGIN_ABI_VERSION 1

typedef struct {
    float ffb_in;
    float torque_out;
    float wheel_speed;
    uint64_t timestamp_ns;
    uint32_t budget_us;
    uint32_t sequence;
} PluginFrame;

typedef struct {
    const char* name;
    const char* version;
    const char* author;
    const char* description;
    uint32_t abi_version;
} PluginInfo;

typedef struct {
    void* (*create)(const uint8_t* config, size_t len);
    int (*process)(void* state, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out);
    void (*destroy)(void* state);
    PluginInfo (*get_info)(void);
} PluginVTable;

#endif // PLUGIN_H
```

#### 3. Implement the Plugin

Create `plugin.c`:

```c
#include "plugin.h"
#include <stdlib.h>
#include <string.h>
#include <math.h>

typedef struct {
    float cutoff_freq;
    float sample_rate;
    float previous_output;
    uint64_t frame_count;
} PluginState;

void* plugin_create(const uint8_t* config, size_t config_len) {
    PluginState* state = malloc(sizeof(PluginState));
    if (!state) return NULL;
    
    state->cutoff_freq = 50.0f;
    state->sample_rate = 1000.0f;
    state->previous_output = 0.0f;
    state->frame_count = 0;
    
    return state;
}

int plugin_process(void* state_ptr, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out) {
    PluginState* state = (PluginState*)state_ptr;
    if (!state || !ffb_out) return -1;
    
    // Simple low-pass filter
    float rc = 1.0f / (2.0f * M_PI * state->cutoff_freq);
    float alpha = dt / (rc + dt);
    
    float output = alpha * ffb_in + (1.0f - alpha) * state->previous_output;
    state->previous_output = output;
    
    *ffb_out = output;
    state->frame_count++;
    
    return 0;
}

void plugin_destroy(void* state) {
    if (state) {
        free(state);
    }
}

PluginInfo plugin_get_info(void) {
    PluginInfo info = {
        .name = "My Fast Plugin",
        .version = "1.0.0",
        .author = "Your Name",
        .description = "A sample DSP filter",
        .abi_version = PLUGIN_ABI_VERSION
    };
    return info;
}

PluginVTable get_plugin_vtable(void) {
    PluginVTable vtable = {
        .create = plugin_create,
        .process = plugin_process,
        .destroy = plugin_destroy,
        .get_info = plugin_get_info
    };
    return vtable;
}
```

#### 4. Build the Plugin

**Linux:**
```bash
gcc -shared -fPIC -o libmy_fast_plugin.so plugin.c -lm
```

**Windows (MSVC):**
```cmd
cl /LD plugin.c /Fe:my_fast_plugin.dll
```

**Windows (MinGW):**
```bash
gcc -shared -o my_fast_plugin.dll plugin.c -lm
```

#### 5. Create Manifest

Create `plugin.yaml`:

```yaml
id: "550e8400-e29b-41d4-a716-446655440000"
name: "My Fast Plugin"
version: "1.0.0"
description: "A sample DSP filter plugin"
author: "Your Name"
license: "MIT"
homepage: "https://github.com/yourname/my-fast-plugin"
class: Fast
capabilities:
  - ReadTelemetry
  - ProcessDsp
operations:
  - DspFilter
constraints:
  max_execution_time_us: 200
  max_memory_bytes: 4194304
  update_rate_hz: 1000
entry_points:
  native_library: "libmy_fast_plugin.so"  # or .dll on Windows
  main_function: "plugin_process"
  init_function: "plugin_create"
  cleanup_function: "plugin_destroy"
signature: "base64-encoded-ed25519-signature"
```

### Code Examples

#### Example: Low-Pass DSP Filter

Reference: [`crates/plugins/examples/sample_dsp_plugin.c`](../crates/plugins/examples/sample_dsp_plugin.c)

```c
#include <stdint.h>
#include <stdlib.h>
#include <math.h>

#define PLUGIN_ABI_VERSION 1

typedef struct {
    float cutoff_freq;
    float sample_rate;
    float previous_output;
    uint64_t frame_count;
} PluginState;

typedef struct {
    const char* name;
    const char* version;
    const char* author;
    const char* description;
    uint32_t abi_version;
} PluginInfo;

typedef struct {
    void* (*create)(const uint8_t* config, size_t len);
    int (*process)(void* state, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out);
    void (*destroy)(void* state);
    PluginInfo (*get_info)(void);
} PluginVTable;

void* plugin_create(const uint8_t* config, size_t config_len) {
    PluginState* state = malloc(sizeof(PluginState));
    if (!state) return NULL;
    
    state->cutoff_freq = 50.0f;  // 50 Hz cutoff
    state->sample_rate = 1000.0f; // 1 kHz
    state->previous_output = 0.0f;
    state->frame_count = 0;
    
    return state;
}

int plugin_process(void* state_ptr, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out) {
    PluginState* state = (PluginState*)state_ptr;
    if (!state || !ffb_out) return -1;
    
    // Simple low-pass filter: y[n] = α·x[n] + (1-α)·y[n-1]
    float rc = 1.0f / (2.0f * M_PI * state->cutoff_freq);
    float alpha = dt / (rc + dt);
    
    float output = alpha * ffb_in + (1.0f - alpha) * state->previous_output;
    state->previous_output = output;
    
    *ffb_out = output;
    state->frame_count++;
    
    return 0; // Success
}

void plugin_destroy(void* state) {
    if (state) {
        free(state);
    }
}

PluginInfo plugin_get_info(void) {
    PluginInfo info = {
        .name = "Sample DSP Filter",
        .version = "1.0.0",
        .author = "Racing Wheel Suite",
        .description = "Simple low-pass filter for force feedback",
        .abi_version = PLUGIN_ABI_VERSION
    };
    return info;
}

PluginVTable get_plugin_vtable(void) {
    PluginVTable vtable = {
        .create = plugin_create,
        .process = plugin_process,
        .destroy = plugin_destroy,
        .get_info = plugin_get_info
    };
    return vtable;
}
```

---

## Plugin Manifest

The plugin manifest is a YAML file that describes the plugin's metadata, capabilities, constraints, and entry points.

### Manifest Structure

```yaml
# Plugin identification
id: "550e8400-e29b-41d4-a716-446655440000"  # UUID v4
name: "My Plugin"
version: "1.0.0"  # Semantic versioning
description: "A brief description of the plugin"
author: "Your Name"
license: "MIT"  # SPDX identifier
homepage: "https://github.com/yourname/my-plugin"

# Plugin class: Safe (WASM) or Fast (Native)
class: Safe

# Required capabilities
capabilities:
  - ReadTelemetry
  - ControlLeds

# Supported operations
operations:
  - TelemetryProcessor
  - LedMapper

# Performance constraints
constraints:
  max_execution_time_us: 5000  # Maximum time per tick
  max_memory_bytes: 16777216   # 16 MB
  update_rate_hz: 60           # 60, 100, or 200 for Safe; 1000 for Fast
  cpu_affinity: null           # Optional CPU core mask

# Entry points
entry_points:
  wasm_module: "path/to/plugin.wasm"        # For Safe plugins
  native_library: "path/to/plugin.so"       # For Fast plugins
  main_function: "process"                   # Main entry point
  init_function: "initialize"                # Optional initialization
  cleanup_function: "shutdown"               # Optional cleanup

# Configuration schema (optional)
config_schema:
  type: object
  properties:
    max_rpm:
      type: number
      default: 8000
    shift_point:
      type: number
      default: 0.9

# Code signature (required for Fast plugins)
signature: "base64-encoded-ed25519-signature"
```

### Capability Declarations

Capabilities define what operations a plugin is allowed to perform:

| Capability | Safe Plugin | Fast Plugin | Description |
|------------|------------|-------------|-------------|
| `ReadTelemetry` | ✓ | ✓ | Read telemetry data |
| `ModifyTelemetry` | ✓ | ✓ | Modify telemetry before use |
| `ControlLeds` | ✓ | ✓ | Control wheel LEDs |
| `ProcessDsp` | ✗ | ✓ | Process DSP filters |
| `FileSystem` | ✓ | ✓ | Access specific file paths |
| `Network` | ✓ | ✓ | Access specific network hosts |
| `InterPluginComm` | ✓ | ✓ | Communicate with other plugins |

### Budget Enforcement

The plugin system enforces the constraints declared in the manifest:

```rust
// Safe plugin budgets
PluginConstraints {
    max_execution_time_us: 5000,  // 5 ms
    max_memory_bytes: 16 * 1024 * 1024,  // 16 MB
    update_rate_hz: 60,  // 60-200 Hz
    cpu_affinity: None,
}

// Fast plugin budgets
PluginConstraints {
    max_execution_time_us: 200,  // 200 μs
    max_memory_bytes: 4 * 1024 * 1024,  // 4 MB
    update_rate_hz: 1000,  // 1 kHz
    cpu_affinity: Some(0xFE),  // All cores except first
}
```

Budget violation policy:
1. **First violation**: Warning logged
2. **Second violation**: Plugin throttled (update rate halved)
3. **Third violation**: Plugin quarantined for 1 hour
4. **Escalation**: Quarantine duration doubles on each subsequent violation

---

## Development Setup

### Prerequisites

#### For Safe Plugins (WASM)

- **Rust** 1.95.0 or later (nightly toolchain required)
- **WASI target**: `rustup target add wasm32-wasi`
- **Cargo** (included with Rust)

#### For Fast Plugins (Native)

- **C compiler**: GCC, Clang, or MSVC
- **OpenSSL** (for code signing)
- **Platform-specific build tools**

#### Common Requirements

- **Git** (for version control)
- **YAML parser** (for manifest validation)
- **Text editor** or IDE

### Build Tools

#### Safe Plugin Build

```bash
# Add WASM target
rustup target add wasm32-wasi

# Build plugin
cargo build --release --target wasm32-wasi

# Optimize WASM (optional)
wasm-opt -Oz target/wasm32-wasi/release/my_plugin.wasm -o my_plugin_opt.wasm
```

#### Fast Plugin Build

**Linux:**
```bash
# Compile shared library
gcc -shared -fPIC -o libmy_plugin.so plugin.c -lm

# Strip symbols for smaller size
strip --strip-unneeded libmy_plugin.so
```

**Windows (MSVC):**
```cmd
REM Compile DLL
cl /LD plugin.c /Fe:my_plugin.dll

REM Optional: link with optimizations
link /DLL /OPT:REF /OPT:ICF plugin.obj /OUT:my_plugin.dll
```

**Windows (MinGW):**
```bash
gcc -shared -o my_plugin.dll plugin.c -lm
strip --strip-unneeded my_plugin.dll
```

### Testing Procedures

#### Unit Testing

**Rust (WASM):**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_initialization() {
        let mut plugin = MyPlugin::default();
        let config = serde_json::json!({"max_rpm": 8000});
        assert!(plugin.initialize(config).is_ok());
    }

    #[test]
    fn test_led_mapping() {
        let plugin = MyPlugin::default();
        let input = SdkLedInput {
            telemetry: SdkTelemetry { rpm: 4000.0, ..Default::default() },
            led_count: 10,
            current_leds: vec![],
        };
        let result = plugin.process_led_mapping(input, SdkContext::default());
        assert!(result.is_ok());
    }
}
```

**C (Native):**
```c
#include <assert.h>

void test_plugin_process() {
    void* state = plugin_create(NULL, 0);
    assert(state != NULL);
    
    float ffb_out;
    int result = plugin_process(state, 0.5f, 1.0f, 0.0f, 0.001f, &ffb_out);
    assert(result == 0);
    
    plugin_destroy(state);
}
```

#### Integration Testing

Use the plugin system's test framework:

```bash
# Run plugin system tests
cargo test --package racing-wheel-plugins

# Run with sample plugins
cargo test --test plugin_system_tests
```

#### Performance Testing

```bash
# Measure execution time
cargo test --release -- --nocapture --test-threads=1

# Check for memory leaks
valgrind --leak-check=full ./target/release/my_plugin_test
```

---

## Deployment

### Plugin Installation

#### Directory Structure

```
~/.config/openracing/plugins/
├── my_safe_plugin/
│   ├── plugin.yaml
│   ├── my_safe_plugin.wasm
│   └── config.json
└── my_fast_plugin/
    ├── plugin.yaml
    ├── my_fast_plugin.so
    ├── my_fast_plugin.sig
    └── config.json
```

#### Installation Steps

1. **Create plugin directory**:
   ```bash
   mkdir -p ~/.config/openracing/plugins/my_plugin
   ```

2. **Copy plugin files**:
   ```bash
   cp plugin.yaml ~/.config/openracing/plugins/my_plugin/
   cp my_plugin.wasm ~/.config/openracing/plugins/my_plugin/
   ```

3. **Verify installation**:
   ```bash
   wheelctl plugin list
   ```

### Configuration

#### Plugin Configuration

Create `config.json` in the plugin directory:

```json
{
  "max_rpm": 8000,
  "shift_point": 0.9,
  "led_brightness": 1.0,
  "update_rate": 60
}
```

#### Service Configuration

Add plugin to service configuration:

```yaml
# ~/.config/openracing/config.yaml
plugins:
  enabled:
    - my_safe_plugin
    - my_fast_plugin
  auto_load: true
  quarantine_policy:
    max_crashes: 3
    max_budget_violations: 10
    quarantine_duration_minutes: 60
```

### Quarantine Policy

Plugins that repeatedly fail are automatically quarantined:

| Violation | Threshold | Action |
|-----------|-----------|--------|
| Crashes | 3 in 60 minutes | Quarantine |
| Budget violations | 10 in 60 minutes | Quarantine |
| Timeout violations | 5 in 60 minutes | Quarantine |

Quarantine duration escalates:
- Level 1: 1 hour
- Level 2: 2 hours
- Level 3: 4 hours
- Level 4: 8 hours
- Level 5: 16 hours (maximum)

To manually release a plugin from quarantine:

```bash
wheelctl plugin release my_plugin
```

---

## Best Practices

### Performance Considerations

#### Safe Plugins (WASM)

1. **Minimize allocations**:
   ```rust
   // Bad: allocates on every call
   fn process(&mut self) -> SdkResult<SdkOutput> {
       let mut leds = vec![SdkLedColor::default(); 100];  // Allocation!
       // ...
   }
   
   // Good: pre-allocate
   struct MyPlugin {
       led_buffer: Vec<SdkLedColor>,
   }
   
   impl MyPlugin {
       fn new() -> Self {
           Self {
               led_buffer: vec![SdkLedColor::default(); 100],
           }
       }
   }
   ```

2. **Avoid expensive operations**:
   - No floating-point trigonometry in hot paths
   - Cache frequently used values
   - Use integer math where possible

3. **Profile your code**:
   ```bash
   cargo flamegraph --bin my_plugin
   ```

#### Fast Plugins (Native)

1. **RT-safe operations only**:
   ```c
   // Bad: malloc in process() - not RT-safe!
   int plugin_process(void* state, ...) {
       float* buffer = malloc(1024);  // Don't do this!
       // ...
   }
   
   // Good: pre-allocate in create()
   typedef struct {
       float* buffer;
   } PluginState;
   
   void* plugin_create(...) {
       PluginState* state = malloc(sizeof(PluginState));
       state->buffer = malloc(1024);  // Allocate once
       return state;
   }
   ```

2. **Avoid system calls**:
   - No I/O in `process()` function
   - No locks or mutexes
   - No dynamic memory allocation

3. **Use SIMD when appropriate**:
   ```c
   #include <immintrin.h>
   
   // Vectorized processing
   __m128 vec = _mm_load_ps(input);
   __m128 result = _mm_mul_ps(vec, scale);
   _mm_store_ps(output, result);
   ```

### Safety Guidelines

1. **Validate all inputs**:
   ```rust
   fn process(&mut self, input: SdkTelemetry) -> SdkResult<SdkOutput> {
       if input.led_count > 1000 {
           return Err(SdkError::InvalidInput("Too many LEDs".to_string()));
       }
       // ...
   }
   ```

2. **Handle errors gracefully**:
   ```rust
   fn process(&mut self, input: SdkTelemetry) -> SdkResult<SdkOutput> {
       self.process_telemetry(input)
           .unwrap_or_else(|e| {
               tracing::error!("Processing failed: {}", e);
               SdkOutput::default()
           })
   }
   ```

3. **Don't block**:
   - Use async/await for I/O
   - Set timeouts on all operations
   - Avoid busy loops

### Error Handling

#### WASM Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Processing failed: {0}")]
    ProcessingFailed(String),
    
    #[error("Capability denied: {0}")]
    CapabilityDenied(String),
}

impl From<PluginError> for SdkError {
    fn from(err: PluginError) -> Self {
        SdkError::ProcessingError(err.to_string())
    }
}
```

#### Native Error Handling

```c
// Return 0 for success, non-zero for error
int plugin_process(void* state, float ffb_in, float wheel_speed, 
                   float wheel_angle, float dt, float* ffb_out) {
    if (!state || !ffb_out) {
        return -1;  // Invalid arguments
    }
    
    PluginState* s = (PluginState*)state;
    
    // Check for invalid state
    if (s->sample_rate <= 0.0f) {
        return -2;  // Invalid configuration
    }
    
    // Process...
    return 0;  // Success
}
```

---

## Troubleshooting

### Common Issues

#### Plugin Won't Load

**Symptoms**: Plugin not appearing in list, error messages about loading

**Solutions**:
1. Check manifest syntax:
   ```bash
   wheelctl plugin validate my_plugin/plugin.yaml
   ```

2. Verify file permissions:
   ```bash
   ls -la ~/.config/openracing/plugins/my_plugin/
   ```

3. Check logs:
   ```bash
   journalctl --user -u wheeld -f | grep plugin
   ```

#### Budget Violations

**Symptoms**: Plugin throttled or quarantined, warnings in logs

**Solutions**:
1. Profile your code:
   ```rust
   use std::time::Instant;
   
   let start = Instant::now();
   // ... your code ...
   let elapsed = start.elapsed();
   tracing::debug!("Processing took: {:?}", elapsed);
   ```

2. Optimize hot paths
3. Increase budget in manifest (if justified)

#### Capability Violations

**Symptoms**: Operations fail with permission errors

**Solutions**:
1. Check manifest capabilities:
   ```yaml
   capabilities:
     - ReadTelemetry    # Required for telemetry access
     - ControlLeds      # Required for LED control
   ```

2. Verify capability checks in code:
   ```rust
   fn process_led_mapping(&mut self, input: SdkLedInput, context: SdkContext) -> SdkResult<SdkOutput> {
       // Ensure ControlLeds capability is granted
       // ...
   }
   ```

#### WASM Runtime Errors

**Symptoms**: Plugin crashes with WASM runtime errors

**Solutions**:
1. Check WASM compatibility:
   ```bash
   wasm-validate my_plugin.wasm
   ```

2. Verify WASI imports:
   ```bash
   wasm-objdump -x my_plugin.wasm | grep import
   ```

3. Check for unsupported features:
   - No threads
   - No SIMD (by default)
   - Limited memory

#### Native Plugin Crashes

**Symptoms**: Helper process crashes, plugin quarantined

**Solutions**:
1. Check for RT violations:
   - No malloc in process()
   - No system calls
   - No blocking operations

2. Verify ABI version:
   ```c
   #define PLUGIN_ABI_VERSION 1  // Must match host
   ```

3. Test with debug build:
   ```bash
   gdb --args wheeld --debug-plugins
   ```

### Debugging Tips

#### Enable Debug Logging

```yaml
# ~/.config/openracing/config.yaml
logging:
  level: debug
  plugins:
    level: trace
```

#### Use Plugin Inspector

```bash
# Inspect plugin details
wheelctl plugin inspect my_plugin

# View plugin statistics
wheelctl plugin stats my_plugin

# View quarantine status
wheelctl plugin quarantine list
```

#### Test in Isolation

```bash
# Run plugin test harness
cargo test --package my_plugin -- --nocapture

# Run with sample data
wheelctl plugin test my_plugin --input sample_data.json
```

#### Monitor Performance

```bash
# Real-time monitoring
wheelctl plugin monitor my_plugin

# Performance report
wheelctl plugin report my_plugin --format json
```

---

## References

### Documentation

- **ADR-0005: Plugin Architecture**: [`docs/adr/0005-plugin-architecture.md`](adr/0005-plugin-architecture.md)
- **Development Guide**: [`docs/DEVELOPMENT.md`](DEVELOPMENT.md)
- **System Integration**: [`docs/SYSTEM_INTEGRATION.md`](SYSTEM_INTEGRATION.md)

### SDK Documentation

- **Plugin SDK**: [`crates/plugins/src/sdk.rs`](../crates/plugins/src/sdk.rs)
- **ABI Definitions**: [`crates/plugins/src/abi.rs`](../crates/plugins/src/abi.rs)
- **Capability System**: [`crates/plugins/src/capability.rs`](../crates/plugins/src/capability.rs)
- **Manifest System**: [`crates/plugins/src/manifest.rs`](../crates/plugins/src/manifest.rs)

### Example Plugins

- **Sample LED Plugin**: [`crates/plugins/examples/sample_led_plugin.rs`](../crates/plugins/examples/sample_led_plugin.rs)
- **Sample Telemetry Plugin**: [`crates/plugins/examples/sample_telemetry_plugin.rs`](../crates/plugins/examples/sample_telemetry_plugin.rs)
- **Sample DSP Plugin**: [`crates/plugins/examples/sample_dsp_plugin.c`](../crates/plugins/examples/sample_dsp_plugin.c)

### External Resources

- **WASI Specification**: https://wasi.dev/
- **Wasmtime Documentation**: https://docs.wasmtime.dev/
- **Ed25519 Signatures**: https://ed25519.cr.yp.to/
- **Real-Time Programming**: https://wiki.linuxfoundation.org/realtime/start

### Community

- **GitHub Issues**: https://github.com/EffortlessMetrics/OpenRacing/issues
- **Discussions**: https://github.com/EffortlessMetrics/OpenRacing/discussions
- **Contributing Guide**: [`CONTRIBUTING.md`](CONTRIBUTING.md)

---

## Appendix

### A. Plugin Manifest Schema

```yaml
$schema: http://json-schema.org/draft-07/schema#
title: Plugin Manifest
type: object
required:
  - id
  - name
  - version
  - description
  - author
  - license
  - class
  - capabilities
  - operations
  - constraints
  - entry_points

properties:
  id:
    type: string
    format: uuid
    description: Unique plugin identifier (UUID v4)

  name:
    type: string
    description: Human-readable plugin name

  version:
    type: string
    pattern: '^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$'
    description: Semantic version

  description:
    type: string
    description: Brief plugin description

  author:
    type: string
    description: Plugin author name

  license:
    type: string
    description: SPDX license identifier

  homepage:
    type: string
    format: uri
    description: Plugin homepage URL

  class:
    type: string
    enum: [Safe, Fast]
    description: Plugin execution class

  capabilities:
    type: array
    items:
      type: string
      enum:
        - ReadTelemetry
        - ModifyTelemetry
        - ControlLeds
        - ProcessDsp
        - FileSystem
        - Network
        - InterPluginComm

  operations:
    type: array
    items:
      type: string
      enum:
        - TelemetryProcessor
        - LedMapper
        - DspFilter
        - TelemetrySource

  constraints:
    type: object
    required:
      - max_execution_time_us
      - max_memory_bytes
      - update_rate_hz
    properties:
      max_execution_time_us:
        type: integer
        minimum: 1
      max_memory_bytes:
        type: integer
        minimum: 4096
      update_rate_hz:
        type: integer
        enum: [60, 100, 200, 1000]
      cpu_affinity:
        type: integer

  entry_points:
    type: object
    properties:
      wasm_module:
        type: string
      native_library:
        type: string
      main_function:
        type: string
      init_function:
        type: string
      cleanup_function:
        type: string

  config_schema:
    type: object
    description: JSON Schema for plugin configuration

  signature:
    type: string
    description: Base64-encoded Ed25519 signature (required for Fast plugins)
```

### B. ABI Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-01-15 | Initial release |

### C. Quick Reference

#### Safe Plugin Template

```rust
use racing_wheel_plugins::sdk::*;
use serde_json::Value;

#[derive(Default)]
pub struct MyPlugin;

impl WasmPlugin for MyPlugin {
    fn initialize(&mut self, config: Value) -> SdkResult<()> {
        Ok(())
    }
    
    fn process_telemetry(&mut self, input: SdkTelemetry, context: SdkContext) -> SdkResult<SdkOutput> {
        Ok(SdkOutput::Telemetry {
            telemetry: input,
            custom_data: Value::Null,
        })
    }
    
    fn process_led_mapping(&mut self, input: SdkLedInput, context: SdkContext) -> SdkResult<SdkOutput> {
        Ok(SdkOutput::Led {
            led_pattern: vec![],
            brightness: 1.0,
            duration_ms: 50,
        })
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        Ok(())
    }
}

racing_wheel_plugins::export_wasm_plugin!(MyPlugin);
```

#### Fast Plugin Template

```c
#include <stdint.h>
#include <stdlib.h>

#define PLUGIN_ABI_VERSION 1

typedef struct {
    // Plugin state
} PluginState;

void* plugin_create(const uint8_t* config, size_t len) {
    PluginState* state = malloc(sizeof(PluginState));
    // Initialize...
    return state;
}

int plugin_process(void* state, float ffb_in, float wheel_speed, 
                   float wheel_angle, float dt, float* ffb_out) {
    // Process...
    *ffb_out = ffb_in;
    return 0;
}

void plugin_destroy(void* state) {
    free(state);
}

typedef struct {
    const char* name;
    const char* version;
    const char* author;
    const char* description;
    uint32_t abi_version;
} PluginInfo;

PluginInfo plugin_get_info(void) {
    PluginInfo info = {
        .name = "My Plugin",
        .version = "1.0.0",
        .author = "Your Name",
        .description = "Description",
        .abi_version = PLUGIN_ABI_VERSION
    };
    return info;
}

typedef struct {
    void* (*create)(const uint8_t*, size_t);
    int (*process)(void*, float, float, float, float, float*);
    void (*destroy)(void*);
    PluginInfo (*get_info)(void);
} PluginVTable;

PluginVTable get_plugin_vtable(void) {
    PluginVTable vtable = {
        .create = plugin_create,
        .process = plugin_process,
        .destroy = plugin_destroy,
        .get_info = plugin_get_info
    };
    return vtable;
}
```

---

*Last updated: 2026-01-23*
