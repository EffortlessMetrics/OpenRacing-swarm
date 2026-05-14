# Staged Device Bring-Up Rail

OpenRacing hardware support moves through a common staged rail. Device-family
adapters provide VID/PID identity, endpoint roles, capture requirements, parser
fixtures, and output eligibility; they do not change the stage order or bypass
safety gates.

```text
discovery
passive
descriptor_trust
fixture_promotion
pre_output_readiness
zero_torque
watchdog
disconnect
bounded_ffb
ffb_extended
```

The rail is intentionally slower than "device works, try FFB". Input evidence
can be green while force output remains hard-blocked.

## Rail Receipt

Use the read-only hardware command to inspect the rail for a family adapter:

```powershell
wheelctl hardware bringup-rail `
  --family moza-r5 `
  --json-out target\hardware-bringup-rail-moza-r5.json `
  --json
```

The command opens no HID device and sends no output, feature, serial, firmware,
or DFU commands.

## Lane Scaffold

Use the generic lane scaffold before collecting device-family receipts:

```powershell
wheelctl hardware lane init `
  --family moza-r5 `
  --topology wheelbase-hub `
  --lane ci\hardware\moza-r5\2026-05-13 `
  --json
```

The scaffold creates a lane manifest, capture plan, artifact checklist, stage
gates, and a lane-init receipt. It creates planning files only; it does not
create fake pass/fail receipts and it does not open HID devices.

Inventory a scaffolded lane without validating hardware claims:

```powershell
wheelctl hardware lane status `
  --lane ci\hardware\moza-r5\2026-05-13 `
  --json-out ci\hardware\moza-r5\2026-05-13\hardware-lane-status.json `
  --json
```

The status receipt reports scaffold files, planned role evidence, stage artifact
presence, the next blocked stage, and safe next commands for observe-only or
passive evidence. It deliberately keeps `evidence_claims_validated`,
`ready_for_zero_torque`, and `ready_for_ffb` false; family verifiers remain
authoritative for actual hardware claims. The inventory command withholds
fixture-promotion, zero-torque, and FFB commands because it does not validate
descriptor trust or later-stage prerequisites.

## Adapter Contract

Each adapter declares:

```text
known VID/PIDs
known endpoint roles
default logical controls
report descriptor expectations
passive capture requirements
parser fixture requirements
output capability
zero-torque eligibility
FFB eligibility
known unsafe surfaces
```

For Moza R5, the adapter identifies the R5 wheelbase hub path and the known
R5 V1/V2 PIDs. It keeps clutch and handbrake optional unless the lane profile
declares them, and it keeps generic auxiliary signals generic until isolated
captures prove semantics.

## Stage Boundaries

Discovery and passive stages are observe-only. Descriptor trust requires raw
report descriptor bytes and CRC before output-adjacent work. Fixture promotion
waits for descriptor trust. Pre-output readiness separates
`ready_for_zero_torque` from `ready_for_ffb`; zero torque is the next stage,
not FFB.

Zero-torque, watchdog, disconnect, bounded FFB, and extended FFB are later
stages. They require explicit endpoint selection and lane receipts. Nonzero
torque, direct mode, feature reports, serial config, and firmware/DFU remain
forbidden until the relevant stage explicitly permits them.
