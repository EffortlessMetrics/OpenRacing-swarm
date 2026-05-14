# Device Protocol Knowledge Base

This directory contains detailed protocol documentation for all racing wheel manufacturers supported by OpenRacing. Each document serves as the reference for implementing device drivers in `crates/engine/src/hw/`.

## Supported Manufacturers

| Manufacturer | Status | Protocol Type | Documentation |
|--------------|--------|---------------|---------------|
| [Logitech](LOGITECH_PROTOCOL.md) | ✅ Supported | HID PIDFF + TrueForce | Well-documented |
| [Fanatec](FANATEC_PROTOCOL.md) | ✅ Supported | Custom HID | Community reverse-engineered |
| [Thrustmaster](THRUSTMASTER_PROTOCOL.md) | ✅ Supported | HID PIDFF | Full model coverage |
| [Moza](MOZA_PROTOCOL.md) | Source-backed / receipt-gated | Serial/HID direct torque research | Lane receipts required before real-hardware output claims |
| [Simagic](SIMAGIC_PROTOCOL.md) | ✅ Supported | HID PIDFF / Proprietary | Legacy + modern (0x2D5C) |
| [Simucube 2](SIMUCUBE_PROTOCOL.md) | ✅ Supported | HID PIDFF (plug-and-play) | Granite Devices |
| [VRS DirectForce Pro](VRS_PROTOCOL.md) | ✅ Supported | HID PIDFF | VRS (shares VID with Simagic) |
| [Heusinkveld](HEUSINKVELD_PROTOCOL.md) | ✅ Supported | HID Input (no FFB) | Pedal sets only |
| [Asetek SimSports](ASETEK_PROTOCOL.md) | ✅ Supported | HID PIDFF (plug-and-play) | Forte/Invicta/LaPrima |
| [OpenFFBoard](OPENFFBOARD_PROTOCOL.md) | ✅ Supported | HID PIDFF + feature init | Open-source DD controller |
| [FFBeast](FFBEAST_PROTOCOL.md) | ✅ Supported | HID PIDFF + feature reports | Open-source DD controller |
| [Granite Devices IONI/ARGON (SimpleMotion V2)](SIMPLEMOTION_PROTOCOL.md) | ✅ Supported | SimpleMotion V2 over USB HID | Simucube 1 / OSW builds |

## Protocol Overview

### Common Concepts

All racing wheel protocols share these fundamental concepts:

1. **Device Enumeration**: USB HID device discovery via Vendor ID (VID) and Product ID (PID)
2. **Initialization**: Mode switching from generic/compatibility mode to native/advanced mode
3. **Input Reports**: Steering angle, pedal positions, button states
4. **Output Reports**: Force feedback effects, LED control, display data
5. **Feature Reports**: Configuration, calibration, firmware queries

### Force Feedback Standards

| Standard | Description | Supported By |
|----------|-------------|--------------|
| USB HID PID | Physical Interface Device standard | Logitech, Thrustmaster, Moza, VRS, Simucube, Asetek, Simagic modern, OpenFFBoard, FFBeast |
| SimpleMotion V2 | Granite Devices binary protocol over USB HID | IONI, IONI Premium, ARGON (Simucube 1 / OSW) |
| Custom HID | Vendor-specific FFB protocol | Fanatec, Simagic (legacy) |
| TrueForce | High-frequency audio-based haptics | Logitech G923+ |

### Common Effect Types

| Effect | Description | Usage |
|--------|-------------|-------|
| Constant | Steady force in one direction | Steering resistance, weight transfer |
| Spring | Position-dependent centering force | Self-centering, road feel |
| Damper | Velocity-dependent resistance | Steering smoothness |
| Friction | Static resistance to movement | Tire grip simulation |
| Periodic | Oscillating forces (sine, square, etc.) | Engine vibration, curbs |
| Ramp | Linearly changing force | Acceleration effects |

## Implementation Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    OpenRacing Engine                         │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │  Logitech   │  │   Fanatec   │  │ Thrustmaster│  ...    │
│  │   Driver    │  │   Driver    │  │   Driver    │         │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
│         │                │                │                 │
│  ┌──────┴────────────────┴────────────────┴──────┐         │
│  │              HID Abstraction Layer             │         │
│  │         (hidapi / Windows HID / hidraw)        │         │
│  └────────────────────────┬──────────────────────┘         │
└───────────────────────────┼─────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │   USB Stack   │
                    └───────────────┘
```

## Adding New Device Support

### Prerequisites

1. Physical access to the device
2. USB traffic capture tools (Wireshark + USBPcap)
3. Manufacturer's official software (for protocol sniffing)

### Capture Process

1. **Install USBPcap**: Download from [USBPcap](https://desowin.org/usbpcap/)
2. **Start Capture**: Filter by device VID/PID
3. **Record Initialization**: Capture the official software's init sequence
4. **Document Effects**: Send each FFB effect type and record the packets
5. **Create Protocol Doc**: Follow the template in this directory

### Protocol Document Template

```markdown
# [Manufacturer] Protocol Documentation

**Status**: [Supported/Partial/Research]

## Device Identification
| Model | VID | PID | Notes |
|-------|-----|-----|-------|

## Initialization Sequence
[Document the mode switch commands]

## Input Reports
[Document input report structure]

## Output Reports (FFB)
[Document FFB report structure]

## Feature Reports
[Document configuration reports]

## Resources
[Links to community projects, drivers, etc.]
```

## USB Traffic Analysis Tips

### Wireshark Filters

```
# Filter by Vendor ID
usb.idVendor == 0x046d

# Filter by specific device
usb.device_address == 5

# Filter HID reports only
usbhid

# Filter output reports (FFB)
usb.endpoint_address.direction == OUT
```

### Common Pitfalls

1. **Descriptor Parsing**: Some devices have malformed HID descriptors
2. **Timing Sensitivity**: Init sequences may require specific delays
3. **Mode Dependencies**: Some features only work in specific modes
4. **Firmware Variations**: Protocol may change between firmware versions

## Safety Considerations

When implementing device protocols:

1. **Torque Limits**: Always respect device maximum torque ratings
2. **Watchdog**: Implement communication timeout handling
3. **Graceful Degradation**: Fall back to safe mode on protocol errors
4. **User Safety**: Never exceed safe force levels during development

## External Resources

### Community Projects

- [new-lg4ff](https://github.com/berarma/new-lg4ff) - Logitech Linux driver
- [hid-fanatecff](https://github.com/gotzl/hid-fanatecff) - Fanatec Linux driver
- [hid-tmff2](https://github.com/Kimplul/hid-tmff2) - Thrustmaster Linux driver
- [universal-pidff](https://github.com/JacKeTUs/universal-pidff) - Generic PIDFF driver
- [Boxflat](https://github.com/Lawstorant/boxflat) - Moza protocol documentation
- [OpenFFBoard firmware](https://github.com/Ultrawipf/OpenFFBoard) - Open-source DD wheel controller

### Specifications

- [USB HID Specification](https://www.usb.org/hid)
- [HID Usage Tables](https://usb.org/document-library/hid-usage-tables-14)
- [PID Usage Page (0x0F)](https://www.usb.org/sites/default/files/hut1_4.pdf)

## Version History

| Date | Change |
|------|--------|
| 2024-01 | Initial protocol documentation |
| 2024-06 | Added Moza and Simagic protocols |
| 2024-12 | Comprehensive update for v1.0.0 release |
| 2026-02 | Added Simucube 2, VRS, Heusinkveld, Asetek; corrected Thrustmaster PIDs; upgraded Simagic to cover modern VID 0x2D5C |
| 2026-03 | Added OpenFFBoard (VID 0x1209) and Granite Devices IONI/ARGON (VID 0x1D50/SimpleMotion V2) |
| 2026-Q3 | Added FFBeast (VID 0x045B) and SimpleMotion V2 dedicated protocol docs (1.0 RC) |
