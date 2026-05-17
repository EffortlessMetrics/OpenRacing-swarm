# racing-wheel-telemetry-forza

Forza Motorsport / Forza Horizon UDP telemetry adapter for the OpenRacing platform.

## Protocol

Forza exposes telemetry through the in-game "Data Out" feature over UDP. Two
packet formats are supported and detected automatically by packet size:

| Format  | Size (bytes) | Games             |
|---------|-------------|-------------------|
| Sled    | 232         | FM7 and earlier   |
| CarDash | 311         | FM8, FH5, FH4+    |

All packets are little-endian.

## Wheel Telemetry

Wheel rotation speeds (rad/s) and suspension travel (m) are stored in the
`extended` map of `NormalizedTelemetry`:

| Key                    | Description                   |
|------------------------|-------------------------------|
| `wheel_speed_fl`       | Front-left wheel speed (rad/s)|
| `wheel_speed_fr`       | Front-right wheel speed       |
| `wheel_speed_rl`       | Rear-left wheel speed         |
| `wheel_speed_rr`       | Rear-right wheel speed        |
| `suspension_travel_fl` | Front-left suspension (m)     |
| `suspension_travel_fr` | Front-right suspension (m)    |
| `suspension_travel_rl` | Rear-left suspension (m)      |
| `suspension_travel_rr` | Rear-right suspension (m)     |

## Setup

In Forza Motorsport / Forza Horizon:

1. **Settings → HUD and Gameplay → Data Out → On**
2. Data Out IP Address: `127.0.0.1`
3. Data Out IP Port: `5300`

## Usage

```rust,no_run
use racing_wheel_telemetry_forza::ForzaAdapter;
use openracing_telemetry_adapters::TelemetryAdapter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let adapter = ForzaAdapter::new().with_port(5300);
    let mut rx = adapter.start_monitoring().await?;
    while let Some(frame) = rx.recv().await {
        println!("RPM: {:.0}, Speed: {:.1} m/s, Gear: {}",
            frame.data.rpm,
            frame.data.speed_ms,
            frame.data.gear,
        );
    }
    Ok(())
}
```

## Reference

- [Forza Data Out documentation](https://support.forzamotorsport.net/hc/en-us/articles/21742934790291)
