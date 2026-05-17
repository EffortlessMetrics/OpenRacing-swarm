# openracing

`openracing` is the start-here SDK facade for OpenRacing.

The crate is intentionally thin. It provides stable module names for public
OpenRacing packages while the workspace migrates from historical package names
to the final crate surface. It does not own hardware I/O, runtime execution, or
vendor protocol logic.

Enable only the families needed by your integration:

```toml
openracing = { version = "0.1", features = ["sdk"] }
```

The initial facade re-exports existing public crates behind feature flags:

- `calibration`
- `curves`
- `ffb`
- `profile`
- `plugin_abi`
- `engine`

Future migration PRs will attach `hid`, `pidff`, `moza`, and telemetry family
crates after those public packages exist.
