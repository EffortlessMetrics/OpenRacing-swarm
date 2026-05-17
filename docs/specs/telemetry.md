# Telemetry Integration Specification

This document defines:
- The **contract** between game-specific telemetry adapters and the OpenRacing service.
- The **normalized schema** we emit internally.
- The **wire-level requirements** we follow for each integration (UDP/shared memory), including configuration.

Where a vendor's primary documentation is not publicly accessible, this spec is written to:
1) match what we implement today, and
2) make the remaining unknowns explicit so they can be verified against the vendor-shipped SDK/header.

---

## 1) Architecture and contracts

### 1.1 Components

- **TelemetryService (service/daemon)** orchestrates polling/receiving and exposes telemetry to the rest of the system.
- **TelemetryAdapter (per game)** owns:
  - transport (UDP, shared memory),
  - parsing,
  - mapping into the normalized model,
  - detection ("is game running?").

Implementation touchpoints:
- `crates/telemetry-orchestrator/src/lib.rs` (adapter registration + matrix-driven startup)
- `crates/telemetry-adapters/src/*` (per-game adapter implementations)
- `crates/telemetry-contracts/src/lib.rs` (normalized telemetry schema)
- `crates/service/src/telemetry/mod.rs` (service-level compatibility re-exports)

### 1.2 Adapter runtime contract (MUST)

An adapter MUST:
- Emit `TelemetryFrame`s containing:
  - a `NormalizedTelemetry` payload,
  - a monotonically increasing `sequence` (local to adapter task),
  - a timestamp (ns), and
  - raw input length (bytes) where applicable.
- Never panic on malformed input.
- Treat parse failures as **drop + log**, not process termination.
- Be resilient to "game present but idle" (timeouts are expected).

An adapter SHOULD:
- Avoid unbounded allocations per packet.
- Emit updates at its `expected_update_rate()` (best effort).

### 1.3 Matrix integration governance

- The support matrix is the source of truth for active telemetry integrations.
- The runtime should validate registry parity between:
  - matrix-defined game IDs (`game_support_matrix.yaml`)
  - adapter factories (`openracing-telemetry-adapters`)
  - config writer factories (`racing-wheel-telemetry-config-writers`)
- Missing adapter/writer coverage must be surfaced through startup logs; missing writers are a hard failure path for game configuration services.

### 1.4 BDD matrix metrics (MUST)

OpenRacing MUST compute deterministic matrix metrics at runtime so parity checks are observable, testable, and enforceable in BDD scenarios.

Required metrics:
- `matrix_game_count`: total matrix-defined game IDs.
- `registry_game_count`: total IDs in the runtime registry being checked (adapter or writer).
- `missing_count`: matrix IDs missing in the runtime registry.
- `extra_count`: runtime IDs not present in the matrix.
- `matrix_coverage_ratio`: `(matrix_game_count - missing_count) / matrix_game_count`.
- `registry_coverage_ratio`: `(registry_game_count - extra_count) / registry_game_count`.
- `parity_ok`: true only when configured policies are satisfied.

Implementation touchpoints:
- `crates/telemetry-bdd-metrics/src/lib.rs` (`BddMatrixMetrics`, `RuntimeBddMatrixMetrics`, policy evaluation)
- `crates/telemetry-integration/src/lib.rs` (`RegistryCoverage::bdd_metrics`, `RuntimeCoverageReport::bdd_metrics`)
- `crates/telemetry-orchestrator/src/lib.rs` (`TelemetryService::runtime_bdd_metrics`)

BDD acceptance examples:
- Given the matrix includes `acc`, `iracing`, and `dirt5`, when adapter registry omits `dirt5`, then `missing_count > 0` and `parity_ok = false` under `MATRIX_COMPLETE`.
- Given the matrix is fully represented and registry includes extra experimental IDs, when policy is `MATRIX_COMPLETE`, then matrix coverage remains valid and extras are reported without blocking.

---

## 2) Normalized telemetry model

### 2.1 Core fields

| Field | Type | Units | Notes |
|---|---:|---|---|
| `speed_ms` | `f32` | m/s | MUST be meters per second. |
| `rpm` | `f32` | rpm | Engine RPM where available. |
| `gear` | `i8` | n/a | Conventional: -1 reverse, 0 neutral, 1..n forward. |
| `ffb_scalar` | `f32` | -1..1 | Best-effort normalized steering/FFB proxy. Not safety-critical. |
| `slip_ratio` | `f32` | 0..1 | Best-effort "how much wheel vs ground speed diverges." |
| `car_id` | `String` | n/a | Stable-ish identifier if available. |
| `track_id` | `String` | n/a | Stable-ish identifier if available. |
| `flags` | struct | n/a | In pits, flags, etc. |

### 2.2 Extended fields

Adapters MAY attach additional fields via `extended: HashMap<String, TelemetryValue>`.

Rules:
- Extended keys MUST be stable strings.
- Values MUST be typed (`Integer`, `Float`, `Bool`, `String` where supported).
- Extended fields MUST NOT be required for core functionality.

---

## 3) Game integrations

## 3.1 Telemetry support matrix (authoritative)

The following table is derived from the authoritative matrix in
`crates/telemetry-support/src/game_support_matrix.yaml`.
Each entry is now the source of truth for runtime wiring, including the `status`
and `config_writer` fields consumed by service startup.

| Game | `game_id` | Transport | Method | Default `output_target` | Config writer | Status |
|---|---|---|---|---|---|---|
| iRacing | `iracing` | Windows shared memory | `shared_memory` | `127.0.0.1:12345` | `iracing` | stable |
| Assetto Corsa Competizione | `acc` | UDP broadcast | `udp_broadcast` | `127.0.0.1:9000` | `acc` | stable |
| Assetto Corsa Rally | `ac_rally` | Probe discovery | `probe_discovery` | `127.0.0.1:9000` | `ac_rally` | experimental |
| Automobilista 2 | `ams2` | Shared memory | `shared_memory` | `127.0.0.1:12345` | `ams2` | experimental |
| rFactor 2 | `rfactor2` | Shared memory | `shared_memory` | `127.0.0.1:12345` | `rfactor2` | experimental |
| EA SPORTS WRC | `eawrc` | UDP schema | `udp_schema` | `127.0.0.1:20778` | `eawrc` | experimental |
| F1 2025 | `f1` | Codemasters UDP bridge | `udp_custom_codemasters` | `127.0.0.1:20777` | `f1` | experimental |
| Dirt 5 | `dirt5` | Codemasters UDP bridge | `udp_custom_codemasters` | `127.0.0.1:20777` | `dirt5` | experimental |

---

## 3.2 Assetto Corsa Competizione (ACC)

### 3.2.1 Transport and config

Transport: **UDP** "broadcasting" protocol.

Config file (Windows default):
- `Documents/Assetto Corsa Competizione/Config/broadcasting.json`

Keys OpenRacing expects to manage:
- `updListenerPort` (typo preserved by ACC)
- `broadcastingPort`
- `connectionId`
- `connectionPassword`
- `commandPassword`
- `updateRateHz`

OpenRacing default:
- `output_target = "127.0.0.1:9000"` for ACC unless overridden by configuration.

> NOTE: Some ACC clients configure separate "listener" and "broadcasting" ports. OpenRacing's default stance is to keep this simple: use a single port unless you have a known reason not to.

Implementation touchpoints:
- `crates/telemetry-adapters/src/acc.rs`
- `crates/telemetry-config-writers/src/lib.rs` (`ACCConfigWriter`, writes `broadcasting.json`)
- Fixture conformance: `crates/service/tests/fixtures/acc/*.bin`

### 3.2.2 Wire format (MUST)

Strings are encoded as:
- `u16` little-endian length (byte count),
- followed by UTF-8 bytes.

Registration request (client -> game) is encoded as:
- `u8` message type = REGISTER
- `u8` protocol version
- `string` display name
- `string` connection password
- `i32` update interval (ms), little-endian
- `string` command password

Inbound messages begin with:
- `u8` message type, followed by a type-specific payload.

OpenRacing parses (at minimum):
- Registration result
- Realtime update
- Realtime car update
- Track data
- Entry list / events (best-effort; primarily for enrichment)

### 3.2.3 Normalization mapping (SHOULD)

For realtime car update:
- `speed_ms = speed_kmh / 3.6`
- `gear = (gear_raw - 2)` (ACC commonly encodes as gear+2)
- Flags:
  - `in_pits` and `pit_limiter` derived from location enum where available.
- Extend:
  - positions, lap count, delta, temps/wetness where available

### 3.2.4 Conformance tests (MUST)

OpenRacing MUST maintain:
- Unit test for registration packet layout.
- Fixture-backed decode tests for a realistic message sequence:
  - track data -> realtime update -> realtime car update -> normalized output.
- Truncation test: truncated packets must fail cleanly (error, not panic).

(These are already represented by `include_bytes!` fixture tests in `acc.rs`.)

---

## 3.3 iRacing

### 3.3.1 Transport and platform constraints

Transport: **Windows shared memory (memory-mapped file)**.

Non-Windows behavior:
- Adapter SHOULD return "not running" and SHOULD NOT crash.

### 3.3.2 Win32 mapping rules (MUST)

The shared memory mapping MUST be opened read-only using:
- `OpenFileMappingW(dwDesiredAccess = FILE_MAP_READ, ...)`
- `MapViewOfFile(..., dwDesiredAccess = FILE_MAP_READ, ...)`

Do not confuse:
- FILE mapping access flags (`FILE_MAP_READ`) with
- file share flags (`FILE_SHARE_READ`).

### 3.3.3 IRSDK buffer semantics (MUST)

The mapping contains:
- a header at base,
- rotating telemetry buffers described by header metadata.

OpenRacing MUST:
- read the header,
- select the newest buffer by tick count,
- perform a "stable read" check (header/buffer consistent across two reads),
- decode variables using the variable header table rather than overlaying an invented struct.

OpenRacing SHOULD:
- tolerate missing variables (treat as 0 / empty) to avoid breaking across IRSDK revisions.

Implementation touchpoints:
- `crates/telemetry-adapters/src/iracing.rs`

### 3.3.4 Normalization mapping (SHOULD)

Variables of interest (names subject to IRSDK header verification on a dev machine):
- SessionTime, SessionFlags
- Speed, RPM, Gear
- Throttle, Brake
- SteeringWheelAngle, SteeringWheelTorque
- OnPitRoad, FuelLevel, Lap, LapBestLapTime
- CarPath, TrackName (when exposed)

Mapping:
- `speed_ms` MUST be m/s in normalized form.
- `ffb_scalar` is best-effort (not safety-critical). If torque is in Nm, normalize consistently (e.g., divide by a configured max).

### 3.3.5 Conformance tests (MUST)

OpenRacing MUST keep deterministic tests for:
- buffer selection (highest tick wins),
- rotated-buffer read behavior using a synthetic memory image.
- decoding resilience for variable iRacing payload sizes, including minimum legacy payload and full payload layouts.

---

## 3.4 Automobilista 2 (AMS2)

Status: **experimental / best-effort**.

Transport: PCARS-style shared memory (commonly referenced as `$pcars2$`).

Authoritative schema:
- The shipped `SharedMemory.h` in the AMS2 install directory is the source of truth.

Spec requirement:
- The Rust struct/layout MUST be generated from or manually matched to that header.
- Torn-read avoidance MUST follow the header's published sequencing/version fields (if present).

Implementation touchpoints:
- `crates/telemetry-adapters/src/ams2.rs`

---

## 3.5 rFactor 2

Status: **experimental / best-effort**.

Transport: shared memory, typically exposed by a plugin.

Spec requirements:
- Adapter MUST clearly distinguish:
  - "game running but plugin missing" vs
  - "plugin present but no frames."

Naming:
- Shared memory names may be fixed or PID-suffixed depending on plugin/environment.
- Prefer dedicated force feedback maps when available (do not infer torque from unrelated signals).

Implementation touchpoints:
- `crates/telemetry-adapters/src/rfactor2.rs`

---

## 3.6 Assetto Corsa Rally (AC Rally)

Transport: **probe/discovery** path for mixed shared-memory + UDP discovery.

Status: **experimental / best-effort**.

Config path:
- `Documents/Assetto Corsa Rally/Config/openracing_probe.json`

OpenRacing expects a probe profile with:
- `mode = "discovery"`
- `probeOrder`
- `udpCandidates`
- `sharedMemoryCandidates`
- `outputTarget`
- `updateRateHz`

Implementation touchpoints:
- `crates/telemetry-adapters/src/ac_rally.rs`
- `crates/telemetry-config-writers/src/lib.rs` (`ACRallyConfigWriter`)

### 3.6.1 Conformance expectations (MUST)

- `get_supported_games()` includes `ac_rally`.
- Config generation must be idempotent.
- Probe profile should be preserved when existing data is present.

## 3.7 EA SPORTS WRC

Transport: **UDP schema-driven JSON decode**.

Status: **experimental / best-effort**.

Config path:
- `Documents/My Games/WRC/telemetry/config.json`
- `Documents/My Games/WRC/telemetry/udp/openracing.json`

Implementation touchpoints:
- `crates/telemetry-adapters/src/eawrc.rs`
- `crates/telemetry-config-writers/src/lib.rs` (`EAWRCConfigWriter`)

### 3.7.1 Conformance expectations (MUST)

- Writer must emit packet/structure assignments that align with default game port and selected fields.
- Decoder must map schema channels into normalized telemetry and extended payload fields.

## 3.8 Dirt 5

Transport: **bridge-backed custom UDP protocol**.

Status: **experimental / best-effort**.

Config path:
- `Documents/OpenRacing/dirt5_bridge_contract.json`

Implementation touchpoints:
- `crates/telemetry-adapters/src/dirt5.rs`
- `crates/telemetry-config-writers/src/lib.rs` (`Dirt5ConfigWriter`)

### 3.8.1 Conformance expectations (MUST)

- Configuration writes should never assume native game config directories exist.
- Bridge contract should remain minimal and explicit (game id + protocol + UDP port).

## 3.9 F1 2025

Transport: **bridge-backed custom UDP protocol**.

Status: **experimental / best-effort**.

Config path:
- `Documents/OpenRacing/f1_bridge_contract.json`

Implementation touchpoints:
- `crates/telemetry-adapters/src/f1.rs`
- `crates/telemetry-config-writers/src/lib.rs` (`F1ConfigWriter`)

### 3.9.1 Conformance expectations (MUST)

- F1 bridge packets must be decoded through the shared Codemasters UDP path.
- Adapter normalization must preserve all decoded channels in `extended`.
- Adapter should map F1 flags when present (`drs_available`, `drs_active`, `ers_available`, pit flags).
- Configuration writes should not assume native game config files exist.

## 4) Config writers

Config writers MUST:
- Be idempotent.
- Preserve unknown fields where practical.
- Write the actual files (not just "diffs"), while still returning a diff summary.

### 4.1 ACC

- Write `broadcasting.json` with:
  - `updListenerPort` derived from `TelemetryConfig.output_target` (port),
  - `broadcastingPort` defaulted sensibly,
  - credentials fields present (possibly empty),
  - `updateRateHz` from config.

### 4.2 iRacing

- Edit `Documents/iRacing/app.ini`, ensuring:
  - `[Telemetry]` section exists
  - `telemetryDiskFile=1` when enabled.

Implementation touchpoint:
- `crates/telemetry-config-writers/src/lib.rs`

### 4.3 F1 (Bridge Contract)

- Write `Documents/OpenRacing/f1_bridge_contract.json` with:
  - `game_id = "f1"`
  - `telemetry_protocol = "codemasters_udp"`
  - `mode` defaulted for extended channel coverage
  - `udp_port` derived from `TelemetryConfig.output_target`
  - `update_rate_hz` and `enabled` from the requested config

---

## 5) Troubleshooting checklist

### ACC: no frames
- Verify `broadcasting.json` exists and has `updListenerPort`.
- Ensure the port matches the service configuration (default 9000).
- Restart ACC after edits (some setups only read config at startup).

### iRacing: not detected / no frames
- Windows only: confirm you're running on Windows.
- Confirm the mapping can be opened read-only with FILE_MAP_READ.
- Verify variable names against the installed IRSDK header if values stay zero.

### F1: no frames
- Verify `Documents/OpenRacing/f1_bridge_contract.json` exists and has `game_id = "f1"`.
- Confirm UDP port matches service config (default 20777).
- If using custom channel mapping, set `OPENRACING_F1_CUSTOM_UDP_XML` to a valid XML schema.

---

## References

R1 (Win32): File mapping access rights and `FILE_MAP_READ`.
- https://learn.microsoft.com/en-us/windows/win32/memory/file-mapping-security-and-access-rights

R2 (Win32): `MapViewOfFileEx` parameters (`dwDesiredAccess` uses `FILE_MAP_*`).
- https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffileex

R3 (Win32): `OpenFileMappingW` (`dwDesiredAccess` uses `FILE_MAP_*`).
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-openfilemappingw

R4 (ACC): Broadcasting protocol sample code (message layouts, string encoding).
- https://raw.githubusercontent.com/angel-git/acc-broadcasting/master/BroadcastingNetworkProtocol.cs

R5 (ACC): Broadcasting enums (message type enums).
- https://raw.githubusercontent.com/angel-git/acc-broadcasting/master/BroadcastingEnums.cs
