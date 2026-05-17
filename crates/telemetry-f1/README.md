# racing-wheel-telemetry-f1

F1 2023 and F1 2024 native UDP telemetry adapter for OpenRacing.

Parses the EA/Codemasters binary UDP protocol directly (no bridge layer
or XML spec file required).  Supports packet format `2023` (F1 23) and
`2024` (F1 24) side by side; the format is auto-detected from each packet
header.

## Supported packet types

| Packet ID | Name          | Fields used                               |
|-----------|---------------|-------------------------------------------|
| 1         | Session       | track ID, session type, temperatures      |
| 6         | Car Telemetry | speed, gear, RPM, throttle, brake, steer, DRS, tyre pressures/temps |
| 7         | Car Status    | fuel, ERS, pit limiter, tyre compound     |

All other packet IDs are silently discarded.

## Protocol differences

| Version | CarStatusData per car | Engine power fields |
|---------|-----------------------|---------------------|
| F1 23   | 47 bytes              | Γ£ù (always 0)        |
| F1 24   | 55 bytes              | Γ£ô                   |

## Usage

```rust,no_run
use racing_wheel_telemetry_f1::F1NativeAdapter;
use openracing_telemetry_adapters::TelemetryAdapter;

# #[tokio::main]
# async fn main() -> anyhow::Result<()> {
let adapter = F1NativeAdapter::new();
assert_eq!(adapter.game_id(), "f1_native");
# Ok(())
# }
```

## UDP configuration

Configure the F1 game's UDP output to:

- **UDP Telemetry**: On
- **UDP Broadcast Mode**: Off
- **UDP IP Address**: `127.0.0.1`
- **UDP Port**: `20777` (or `OPENRACING_F1_NATIVE_UDP_PORT` env var)
- **UDP Send Rate**: 60 Hz
- **UDP Format**: `2023` or `2024`
