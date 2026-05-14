# Device Capability Matrix

This document provides a comprehensive reference of every racing device vendor
supported by OpenRacing, the USB identifiers used to detect them, and the
force-feedback capabilities exposed through the corresponding
`crates/hid-*-protocol` microcrates.

All VID/PID values are sourced from `docs/protocols/SOURCES.md` (the golden
reference). Torque, encoder resolution, rotation, and FFB effect columns are
derived from the `ids.rs` and `types.rs` source files in each protocol crate.

---

## Table of Contents

1. [Logitech](#1-logitech--vid-0x046d)
2. [Thrustmaster](#2-thrustmaster--vid-0x044f)
3. [Fanatec](#3-fanatec--vid-0x0eb7)
4. [Moza Racing](#4-moza-racing--vid-0x346e)
5. [Simagic](#5-simagic--vid-0x3670-evo--0x0483-legacy)
6. [Simucube (Granite Devices)](#6-simucube-granite-devices--vid-0x16d0)
7. [Asetek SimSports](#7-asetek-simsports--vid-0x2433)
8. [Cammus](#8-cammus--vid-0x3416)
9. [VRS DirectForce](#9-vrs-directforce--vid-0x0483)
10. [OpenFFBoard](#10-openffboard--vid-0x1209)
11. [FFBeast](#11-ffbeast--vid-0x045b)
12. [Leo Bodnar](#12-leo-bodnar--vid-0x1dd2)
13. [SimXperience AccuForce](#13-simxperience-accuforce--vid-0x1fc9)
14. [Heusinkveld](#14-heusinkveld--vid-0x16d0--non-ffb-pedals)
15. [Cube Controls](#15-cube-controls--vid-0x0483--non-ffb-button-boxes)
16. [PXN](#16-pxn--vid-0x11ff)
17. [Granite Devices / OSW (SimpleMotion V2)](#17-granite-devices--osw-simplemotion-v2--vid-0x1d50)
18. [VID Collision Map](#vid-collision-map)
19. [Non-Wheelbase Peripherals](#non-wheelbase-peripherals)
20. [Force Feedback Protocol Types](#force-feedback-protocol-types)
21. [Tested Status](#tested-status)
22. [Adding New Devices](#adding-new-devices)
23. [Source Citations](#source-citations)

---

## Column Key

| Column | Description |
|---|---|
| **Model** | Commercial product name |
| **PID** | USB Product ID (hex) |
| **Torque (Nm)** | Peak rated torque; source: protocol crate `types.rs` |
| **Rotation (°)** | Maximum steering rotation in degrees |
| **Encoder (CPR)** | Angle-sensor counts-per-revolution or bit depth |
| **Protocol** | FFB wire protocol (see [Protocol Types](#force-feedback-protocol-types)) |

---

## 1. Logitech — VID `0x046D`

Source: `crates/hid-logitech-protocol`; status: **Verified** (Linux kernel hid-ids.h, new-lg4ff, oversteer).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| G25 | `0xC299` | 2.5 | 900 | Potentiometer | Logitech Native / HID PID |
| G27 | `0xC29B` | 2.5 | 900 | Potentiometer | Logitech Native / HID PID |
| G27 (compat mode) | `0xC294` | 2.5 | 900 | Potentiometer | Logitech Native / HID PID |
| G29 (PS/PC) | `0xC24F` | 2.2 | 900 | Potentiometer | Logitech Native / HID PID |
| G920 (Xbox/PC) | `0xC262` | 2.2 | 900 | Potentiometer | Logitech Native / HID PID |
| G923 (native) | `0xC266` | 2.2 | 900 | Potentiometer | Logitech TrueForce |
| G923 (PS compat) | `0xC267` | 2.2 | 900 | Potentiometer | Logitech TrueForce |
| G923 (Xbox/PC) | `0xC26E` | 2.2 | 900 | Potentiometer | Logitech TrueForce |
| G PRO (PS/PC) | `0xC268` | 11.0 | 1080 | N/A (direct drive) | Logitech Native / HID PID |
| G PRO (Xbox/PC) | `0xC272` | 11.0 | 1080 | N/A (direct drive) | Logitech Native / HID PID |

**FFB effects:** Constant, Spring, Damper, Friction. G923 adds **TrueForce** high-frequency haptic output.
G PRO is Logitech's first direct-drive wheelbase (11 Nm, 1080°).

---

## 2. Thrustmaster — VID `0x044F`

Source: `crates/hid-thrustmaster-protocol`; status: **Verified** (hid-tmff2, oversteer, linux-steering-wheels).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| Generic FFB Wheel (pre-init) | `0xB65D` | — | — | — | Thrustmaster Proprietary |
| T150 | `0xB677` | 2.5 | 1080 | N/A | Thrustmaster Proprietary |
| TMX | `0xB67F` | 2.5 | 900 | N/A | Thrustmaster Proprietary |
| T300 RS (PS3) | `0xB66E` | 4.0 | 1080 | N/A | Thrustmaster Proprietary |
| T300 RS (PS4 mode) | `0xB66D` | 4.0 | 1080 | N/A | Thrustmaster Proprietary |
| T300 RS GT | `0xB66F` | 4.0 | 1080 | N/A | Thrustmaster Proprietary |
| TX Racing (Xbox) | `0xB669` | 4.0 | 1080 | N/A | Thrustmaster Proprietary |
| T248 | `0xB696` | 4.0 | 900 | N/A | Thrustmaster Proprietary |
| T248X (Xbox/GIP) | `0xB69A` | 4.0 | 900 | N/A | Thrustmaster Proprietary |
| T500 RS | `0xB65E` | 4.0 | 1080 | N/A | Thrustmaster Proprietary |
| T-GT | *unknown* | 6.0 | 1080 | N/A | Thrustmaster Proprietary |
| T-GT II | *unknown* | 6.0 | 1080 | N/A | Thrustmaster Proprietary |
| TS-PC Racer | `0xB689` | 6.0 | 1080 | N/A | Thrustmaster Proprietary |
| TS-XW (USB/HID) | `0xB692` | 6.0 | 1080 | N/A | Thrustmaster Proprietary |
| TS-XW (GIP/Xbox) | `0xB691` | 6.0 | 1080 | N/A | Thrustmaster Proprietary |
| T818 | `0xB69B` | 10.0 | 1080 | N/A | Thrustmaster Proprietary |

**FFB effects:** Constant, Spring, Damper, Friction. Requires `hid-tminit`-style init sequence; devices enumerate as `0xB65D` before mode switch.

> **Note:** T-GT and T-GT II USB PIDs were previously listed as `0xB68E` and `0xB692` respectively, but those belong to other devices (TPR Rudder and TS-XW). The T-GT II reportedly reuses T300 RS USB PIDs. Real T-GT/T-GT II PIDs are **unverified**.

---

## 3. Fanatec — VID `0x0EB7`

Source: `crates/hid-fanatec-protocol`; status: **Verified** (gotzl/hid-fanatecff).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| CSR Elite (legacy) | `0x0011` | 5.0 | dynamic | 4 096 | Fanatec Proprietary |
| ClubSport V2 | `0x0001` | 8.0 | dynamic | 4 096 | Fanatec Proprietary |
| ClubSport V2.5 | `0x0004` | 8.0 | dynamic | 4 096 | Fanatec Proprietary |
| CSL Elite (PS4) | `0x0005` | 6.0 | dynamic | 4 096 | Fanatec Proprietary |
| CSL Elite (PC) | `0x0E03` | 6.0 | dynamic | 4 096 | Fanatec Proprietary |
| CSL DD | `0x0020` | 8.0 | dynamic | 16 384 | Fanatec Proprietary |
| GT DD Pro | `0x0024` | 8.0 | dynamic | 16 384 | Fanatec Proprietary |
| ClubSport DD+ | `0x01E9` | 12.0 | dynamic | 16 384 | Fanatec Proprietary |
| Podium DD1 | `0x0006` | 20.0 | dynamic | 16 384 | Fanatec Proprietary |
| Podium DD2 | `0x0007` | 25.0 | dynamic | 16 384 | Fanatec Proprietary |

**FFB effects:** Constant Force, Gain, Rumble (via rim). Rotation range set dynamically via `SET_ROTATION_RANGE` command.
**1 kHz FFB:** CSL DD, GT DD Pro, ClubSport DD+, Podium DD1, Podium DD2.

---

## 4. Moza Racing — VID `0x346E`

Source: `crates/hid-moza-protocol`; status: **Source-backed / receipt-gated** (universal-pidff, mozaracing.com; real-hardware claims require Moza lane receipts).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| R3 (V1) | `0x0005` | 3.9 | N/A | 16-bit (i16) | Moza Proprietary |
| R3 (V2) | `0x0015` | 3.9 | N/A | 16-bit (i16) | Moza Proprietary |
| R5 (V1) | `0x0004` | 5.5 | N/A | 16-bit (i16) | Moza Proprietary |
| R5 (V2) | `0x0014` | 5.5 | N/A | 16-bit (i16) | Moza Proprietary |
| R9 (V1) | `0x0002` | 9.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R9 (V2) | `0x0012` | 9.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R12 (V1) | `0x0006` | 12.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R12 (V2) | `0x0016` | 12.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R16 (V1) | `0x0000` | 16.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R16 (V2) | `0x0010` | 16.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R21 (V1) | `0x0000` | 21.0 | N/A | 16-bit (i16) | Moza Proprietary |
| R21 (V2) | `0x0010` | 21.0 | N/A | 16-bit (i16) | Moza Proprietary |

**FFB effects:** Constant, Spring, Damper, Friction. Torque output via report `0x20` (signed i16, percent-of-max).

> R16 and R21 share the same PIDs (`0x0000` V1, `0x0010` V2); the engine differentiates them by product string or handshake response.

---

## 5. Simagic — VID `0x3670` (EVO) / `0x0483` (legacy)

Source: `crates/hid-simagic-protocol`; status: **Verified** (EVO series) / **Estimated** (accessories).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| EVO Sport | `0x0500` | 9.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| EVO | `0x0501` | 12.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| EVO Pro | `0x0502` | 18.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| Alpha EVO | `0x0600` | 15.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| Neo | `0x0700` | 10.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| Neo Mini | `0x0701` | 7.0 | N/A | N/A (proprietary) | Simagic Proprietary |
| Alpha / Alpha Mini / Alpha U / M10 (legacy) | `0x0522` | N/A | N/A | N/A | Simagic Proprietary |

**VID note:** EVO-generation devices use VID `0x3670`. Legacy devices (Alpha, Alpha Mini, Alpha U, M10) use VID `0x0483` (STMicroelectronics) with shared PID `0x0522` — see [VID Collision Map](#vid-collision-map).

**FFB effects:** Constant (0x11), Spring (0x12), Damper (0x13), Friction (0x14), Sine (0x15), Square (0x16), Triangle (0x17).

> Legacy Alpha / Alpha Mini / M10 individual torque ratings are not encoded in the protocol crate; the engine disambiguates via USB `iProduct` string descriptor.

---

## 6. Simucube (Granite Devices) — VID `0x16D0`

Source: `crates/hid-simucube-protocol`; status: **Verified**.

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| Simucube 1 | `0x0D5A` | 25.0 | dynamic | 22-bit (4 194 303) | Simucube Proprietary |
| Simucube 2 Sport | `0x0D61` | 17.0 | dynamic | 22-bit (4 194 303) | Simucube Proprietary |
| Simucube 2 Pro | `0x0D60` | 25.0 | dynamic | 22-bit (4 194 303) | Simucube Proprietary |
| Simucube 2 Ultimate | `0x0D5F` | 32.0 | dynamic | 22-bit (4 194 303) | Simucube Proprietary |
| Wireless Wheel Adapter | `0x0D63` | — | — | — | Simucube Proprietary |
| ActivePedal (SC-Link Hub) | `0x0D66` | — | — | 16-bit | Simucube Proprietary |

**FFB effects:** Constant, Ramp, Square, Sine, Triangle, Sawtooth Up/Down, Spring, Damper, Friction. 360 Hz FFB update rate. SC2 Pro and Ultimate support wireless wheel modules.

---

## 7. Asetek SimSports — VID `0x2433`

Source: `crates/hid-asetek-protocol`; status: **Verified** (Invicta, Forte) / **Community** (La Prima, Tony Kanaan).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| La Prima | `0xF303` | 12.0 | N/A | N/A | Asetek Proprietary |
| Forte | `0xF301` | 18.0 | N/A | N/A | Asetek Proprietary |
| Tony Kanaan Edition | `0xF306` | 27.0 | N/A | N/A | Asetek Proprietary |
| Invicta | `0xF300` | 27.0 | N/A | N/A | Asetek Proprietary |

**FFB effects:** Constant, Spring, Damper. Quick-release system on all models. Torque output in centi-Newton-meters (cNm), 16-bit. Requires continuous HID polling (devices reboot without it).

---

## 8. Cammus — VID `0x3416`

Source: `crates/hid-cammus-protocol`; status: **Verified**.

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| C5 | `0x0301` | 5.0 | 1080 | 16-bit (i16) | Cammus Proprietary Direct |
| C12 | `0x0302` | 12.0 | 1080 | 16-bit (i16) | Cammus Proprietary Direct |

**FFB effects:** Constant (direct torque via report `0x01`). Two modes: configuration (`0x00`) and game (`0x01`).

> **Note:** The Cammus DDMAX is not yet represented in the protocol crate.

---

## 9. VRS DirectForce — VID `0x0483`

Source: `crates/hid-vrs-protocol`; status: **Verified** (DFP) / **Community** (V2, accessories).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| DirectForce Pro | `0xA355` | 20.0 | dynamic | N/A (PIDFF) | HID PIDFF |
| DirectForce Pro V2 | `0xA356` | 25.0 | dynamic | N/A (PIDFF) | HID PIDFF |

**FFB effects (PIDFF):** Constant, Ramp, Square, Sine, Triangle, Sawtooth Up/Down, Spring, Damper, Friction, Custom. Uses STMicroelectronics VID `0x0483` — see [VID Collision Map](#vid-collision-map).

---

## 10. OpenFFBoard — VID `0x1209`

Source: `crates/hid-openffboard-protocol`; status: **Verified** (pid.codes).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| OpenFFBoard | `0xFFB0` | ~20.0 (configurable) | dynamic | 16-bit | HID PIDFF + vendor reports |
| OpenFFBoard (alt) | `0xFFB1` | ~20.0 (configurable) | dynamic | 16-bit | HID PIDFF + vendor reports |

**FFB effects:** Constant (PIDFF). Actual torque depends on motor and PSU. Open-source firmware: [github.com/Ultrawipf/OpenFFBoard](https://github.com/Ultrawipf/OpenFFBoard).

---

## 11. FFBeast — VID `0x045B`

Source: `crates/hid-ffbeast-protocol`; status: **Verified**.

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| FFBeast Wheel | `0x59D7` | ~20.0 (configurable) | dynamic | 16-bit | HID PIDFF + vendor reports |
| FFBeast Joystick | `0x58F9` | ~20.0 (configurable) | dynamic | 16-bit | HID PIDFF + vendor reports |
| FFBeast Rudder | `0x5968` | ~20.0 (configurable) | dynamic | 16-bit | HID PIDFF + vendor reports |

**FFB effects:** Constant (PIDFF). Open-source DIY force feedback controller; actual torque depends on build.

---

## 12. Leo Bodnar — VID `0x1DD2`

Source: `crates/hid-leo-bodnar-protocol`; status: **Community**.

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| USB Joystick | `0x0001` | — | — | — | Input only |
| BU0836A | `0x000B` | — | — | 12-bit ADC | Input only |
| BBI-32 | `0x000C` | — | — | — | Input only (32-button box) |
| Sim Racing Wheel Interface | `0x000E` | ~10.0 (configurable) | dynamic | 16-bit (65 535) | HID PIDFF |
| FFB Joystick | `0x000F` | ~10.0 (configurable) | dynamic | 16-bit (65 535) | HID PIDFF |
| BU0836X | `0x0030` | — | — | 16-bit ADC | Input only |
| BU0836 16-bit | `0x0031` | — | — | 16-bit ADC | Input only |
| SLI-Pro | `0x1301` | — | — | — | Output/display + button inputs (estimated PID) |

**FFB effects (Wheel Interface / FFB Joystick):** Constant, Spring, Damper (PIDFF standard). Actual torque depends on motor/PSU.

---

## 13. SimXperience AccuForce — VID `0x1FC9`

Source: `crates/hid-accuforce-protocol`; status: **Community** (USB captures).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| AccuForce Pro | `0x804C` | 7.0 | dynamic | N/A (PIDFF) | HID PIDFF |

**FFB effects (PIDFF):** Constant, Spring, Damper, Friction. Uses NXP USB chip VID. V2 model reportedly produces ~13 Nm but shares the same PID.

---

## 14. Heusinkveld — VID `0x04D8` *(non-FFB pedals)*

Source: `crates/hid-heusinkveld-protocol`; status: **Community** (OpenFlight cross-reference).

> ⚠️ Heusinkveld products are **load-cell pedals** (input-only). They do **not** produce force feedback.

| Model | PID | Max Load (kg) | Pedal Axes | Protocol |
|-------|-----|---------------|------------|----------|
| Sim Pedals Sprint | `0xF6D0` | 55 | 2 | USB HID (load cell) |
| Sim Pedals Ultimate+ | `0xF6D2` | 140 | 3 | USB HID (load cell) |
| Sim Pedals Pro (discontinued) | `0xF6D3` | 200 | 3 | USB HID (load cell) |

**VID note:** Heusinkveld uses VID `0x04D8` (Microchip Technology) — a generic chip vendor VID. Disambiguation is by PID range `0xF6D0`–`0xF6D3`.

---

## 15. Cube Controls — VID `0x0483` *(non-FFB button boxes)*

Source: `crates/hid-cube-controls-protocol`; status: **Estimated** (provisional PIDs).

> ⚠️ **PROVISIONAL**: PIDs are **unconfirmed**. Cube Controls products are **steering wheel button boxes** (input-only), not wheelbases. They do not produce force feedback.

| Model | PID | Protocol | Notes |
|-------|-----|----------|-------|
| GT Pro | `0x0C73` (prov.) | Input-only (HID gamepad) | Steering wheel button box |
| Formula Pro | `0x0C74` (prov.) | Input-only (HID gamepad) | Steering wheel button box |
| CSX3 | `0x0C75` (prov.) | Input-only (HID gamepad) | Steering wheel with touchscreen |

**VID note:** Uses STMicroelectronics VID `0x0483` — see [VID Collision Map](#vid-collision-map).

---

## 16. PXN — VID `0x11FF`

Source: `crates/hid-pxn-protocol`; status: **Community** (JacKeTUs/linux-steering-wheels).

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| PXN V10 | `0x3245` | ~10.0 | N/A | N/A (PIDFF) | HID PIDFF |
| PXN V12 | `0x1212` | ~12.0 | N/A | N/A (PIDFF) | HID PIDFF |
| PXN V12 Lite | `0x1112` | ~12.0 | N/A | N/A (PIDFF) | HID PIDFF |
| PXN V12 Lite SE | `0x1211` | ~12.0 | N/A | N/A (PIDFF) | HID PIDFF |
| PXN GT987 FF | `0x2141` | ~5.0 | N/A | N/A (PIDFF) | HID PIDFF |

---

## 17. Granite Devices / OSW (SimpleMotion V2) — VID `0x1D50`

Source: `crates/simplemotion-v2`; status: **Community**.

| Model | PID | Torque (Nm) | Rotation (°) | Encoder (CPR) | Protocol |
|-------|-----|-------------|--------------|---------------|----------|
| Simucube 1 / IONI Drive | `0x6050` | 15.0 | dynamic | 17-bit (131 072) | SimpleMotion V2 (serial) |
| IONI Premium | `0x6051` | 35.0 | dynamic | 17-bit (131 072) | SimpleMotion V2 (serial) |
| ARGON Servo Drive | `0x6052` | 10.0 | dynamic | 17-bit (131 072) | SimpleMotion V2 (serial) |

**FFB effects:** Constant (direct torque at up to 1 kHz). Legacy OSW/Simucube 1 generation.

---

## VID Collision Map

Several vendors share the same USB Vendor ID. The engine resolves collisions using PID ranges and/or the USB `iProduct` string descriptor.

### `0x0483` — STMicroelectronics (shared)

| Vendor | PID(s) | Device Type | Disambiguation |
|--------|--------|-------------|----------------|
| **Simagic** (legacy) | `0x0522` | Wheelbase (Alpha, Alpha Mini, M10, Alpha U) | `iProduct` string |
| **VRS DirectForce** | `0xA355`–`0xA35A` | Wheelbase + peripherals | PID range `0xA3xx` |
| **Cube Controls** | `0x0C73`–`0x0C75` (prov.) | Button boxes (input-only) | PID range `0x0Cxx` |

### `0x04D8` — Microchip Technology (shared)

| Vendor | PID(s) | Device Type | Disambiguation |
|--------|--------|-------------|----------------|
| **Heusinkveld** | `0xF6D0`–`0xF6D3` | Pedals (non-FFB) | PID range `0xF6Dx` |

### `0x16D0` — MCS Electronics (shared)

| Vendor | PID(s) | Device Type | Disambiguation |
|--------|--------|-------------|----------------|
| **Simucube** (Granite Devices) | `0x0D5A`–`0x0D66` | Wheelbases + pedals | PID range `0x0Dxx` |

> When adding a device under a shared VID, update the disambiguation logic in `crates/engine/src/hid/vendor/mod.rs` → `get_vendor_protocol()`.

---

## Non-Wheelbase Peripherals

The following devices are supported for pedal, shifter, or handbrake input. They do not transmit force feedback.

| Device | Vendor | USB VID | Protocol | Axes | Notes |
|---|---|---|---|---|---|
| Moza SR-P Pedals | Moza Racing | `0x346E` | Moza Proprietary | 3 (T/B/C) | PID `0x0003`; standalone USB |
| Moza HBP Handbrake | Moza Racing | `0x346E` | Moza Proprietary | 1 | PID `0x0022` |
| Moza HGP / SGP Shifter | Moza Racing | `0x346E` | Moza Proprietary | — | PIDs `0x0020`, `0x0021` |
| Fanatec ClubSport Pedals V1/V2 | Fanatec | `0x0EB7` | Fanatec Proprietary | 2 | PID `0x1839` |
| Fanatec ClubSport Pedals V3 | Fanatec | `0x0EB7` | Fanatec Proprietary | 3 (load cell) | PID `0x183B` |
| Fanatec CSL Elite Pedals | Fanatec | `0x0EB7` | Fanatec Proprietary | 2 | PID `0x6204` |
| Fanatec CSL Pedals LC | Fanatec | `0x0EB7` | Fanatec Proprietary | 3 (load cell) | PID `0x6205` |
| Fanatec CSL Pedals V2 | Fanatec | `0x0EB7` | Fanatec Proprietary | 3 | PID `0x6206` |
| Simagic P1000 / P1000A Pedals | Simagic | `0x3670` | Simagic Proprietary | 3 | PIDs `0x1001`, `0x1003` (estimated) |
| Simagic P2000 Pedals | Simagic | `0x3670` | Simagic Proprietary | 3 | PID `0x1002` (estimated) |
| Simagic H-Pattern / Sequential Shifter | Simagic | `0x3670` | Simagic Proprietary | — | PIDs `0x2001`, `0x2002` (estimated) |
| Simagic Handbrake | Simagic | `0x3670` | Simagic Proprietary | 1 | PID `0x3001` (estimated) |
| VRS Pedals V1 / V2 | VRS | `0x0483` | USB HID | 3 | PIDs `0xA357`, `0xA358` |
| VRS Handbrake | VRS | `0x0483` | USB HID | 1 | PID `0xA359` |
| VRS Shifter | VRS | `0x0483` | USB HID | — | PID `0xA35A` |
| Cammus CP5 Pedals | Cammus | `0x3416` | USB HID | 3 | PID `0x1018`; community-sourced |
| Cammus LC100 Pedals | Cammus | `0x3416` | USB HID | 3 | PID `0x1019`; community-sourced |
| Simucube ActivePedal | Granite Devices | `0x16D0` | Simucube Proprietary | — | PID `0x0D66` (SC-Link Hub) |
| Heusinkveld Sprint | Heusinkveld | `0x04D8` | USB HID (load cell) | 2 | PID `0xF6D0`; 55 kg max |
| Heusinkveld Ultimate+ | Heusinkveld | `0x04D8` | USB HID (load cell) | 3 | PID `0xF6D2`; 140 kg max |
| Heusinkveld Pro | Heusinkveld | `0x04D8` | USB HID (load cell) | 3 | PID `0xF6D3`; 200 kg max (discontinued) |

---

## Force Feedback Protocol Types

| Protocol Type | Description | Devices Using This Protocol |
|---|---|---|
| **HID PIDFF** | Standard USB HID Physical Interface Device (PID) force feedback, Usage Page `0x000F`. Effects are managed through the OS HID driver via effect create/update/destroy reports. Supports a wide set of effect types defined by the USB HID spec. | VRS DirectForce Pro, AccuForce Pro, FFBeast, OpenFFBoard, Leo Bodnar Wheel Interface/FFB Joystick, PXN |
| **Moza Proprietary** | Custom HID vendor usage page. Torque output uses report `0x20` (direct torque, signed `i16`, percent-of-max). Handshake sequence required at connect. Rim identity, pedal axes, and KS control-surface snapshots multiplexed through the same USB endpoint. | Moza R3–R21 |
| **Fanatec Proprietary** | Endor AG / Fanatec vendor HID protocol. Supports constant-force, gain, LED, display, and mode-switch feature reports. DD models support 1 kHz output (1 ms USB bInterval). | Fanatec CSR Elite, CSL Elite, CSL DD, GT DD Pro, ClubSport DD+, Podium DD1/DD2 |
| **Logitech Native / HID PID** | Logitech wheels start in compatibility mode and must be switched to native mode via a vendor command (`0xF8`/`0x0A`) before exposing the full effect set. HID PID reports are used after mode switch. | Logitech G25, G27, G29, G920, G PRO |
| **Logitech TrueForce** | Extension of Logitech Native mode that adds high-frequency haptic output layered on top of standard effects. The G923 uses this for road-surface texture simulation. | Logitech G923 |
| **Thrustmaster Proprietary** | Thrustmaster vendor HID protocol. Uses proprietary HID reports for constant-force, spring, damper, friction, and device gain. Requires an initialization sequence (`hid-tminit`-style); devices enumerate as PID `0xB65D` before mode switch. | Thrustmaster T150–T818 |
| **Simagic Proprietary** | Simagic vendor HID protocol. Supports constant-force and conditional effects (spring, damper, friction) plus waveform effects (sine, square, triangle) via custom report IDs `0x11`–`0x17`. | Simagic EVO, Alpha, Neo families |
| **Simucube Proprietary** | Granite Devices proprietary protocol over USB HID, providing direct torque control at 360 Hz. Supports 22-bit angle sensor resolution and wireless wheel modules. | Simucube 1, Simucube 2 Sport/Pro/Ultimate |
| **Asetek Proprietary** | Asetek SimSports vendor HID protocol. Supports constant-force, spring, and damper effects. Quick-release system. Torque output in cNm (16-bit). | Asetek Invicta, Forte, La Prima, Tony Kanaan |
| **Cammus Proprietary Direct** | Cammus vendor HID protocol. Direct torque command via report `0x01` (i16 LE). Two modes: configuration (`0x00`) and game (`0x01`). | Cammus C5, C12 |
| **SimpleMotion V2 (Proprietary Serial)** | Granite Devices SimpleMotion V2 protocol over USB CDC/serial. Used for IONI/ARGON servo drives and legacy OSW builds. Direct torque command at up to 1 kHz. | Granite Devices IONI, ARGON, OSW / Simucube 1 |

---

## Tested Status

Devices are assigned one of three status levels based on available evidence.

| Status | Definition | How to Upgrade |
|---|---|---|
| **Verified** | VID/PID confirmed from an official USB descriptor dump, official SDK, or Linux kernel `hid-ids.h`. Protocol behaviour confirmed by capture or documentation. | Add a link to the capture or SDK source in `docs/protocols/SOURCES.md`. |
| **Community-reported** | VID/PID and protocol behaviour confirmed from a community-maintained compatibility table (e.g., [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels), iRacing forum captures, or SimHub issues). Not independently verified against hardware by the OpenRacing project. | Provide a USB descriptor capture or official source; escalate to Verified. |
| **Protocol documented / Estimated** | PID logically extrapolated from a known VID or a sibling model, or assigned by OpenRacing based on community discussion with no independent confirmation. Must be confirmed before production release. | Obtain a USB descriptor capture (`lsusb -v`, USBTreeView, or Zadig) from real hardware. |

### Current status summary

| Vendor | Status | Verification Detail |
|---|---|---|
| Logitech (G25–G PRO) | Verified | All PIDs from Linux kernel hid-ids.h, new-lg4ff, oversteer |
| Thrustmaster (T150, T300, T500, T248, TS-PC, TS-XW, T818) | Verified | PIDs from hid-tmff2, linux-steering-wheels; T-GT/T-GT II PIDs **unknown** |
| Fanatec (CSR Elite–DD2) | Verified (wheelbases) / Community (pedals) | PIDs from gotzl/hid-fanatecff |
| Moza Racing (R3–R21 V1/V2) | Source-backed / receipt-gated | VID/PID and protocol research present; real-hardware validation requires Moza lane receipts |
| Simagic EVO series | Verified (accessories Estimated) | EVO torques verified (9/12/18 Nm); legacy PID `0x0522` confirmed |
| Simucube 1/2 Sport/Pro/Ultimate | Verified | SC2 Sport 17 Nm, Pro 25 Nm, Ultimate 32 Nm; SC1 PID `0x0D5A` |
| Asetek Invicta / Forte | Verified (La Prima, TK: Community) | Torques from crate: 12/18/27 Nm; TK corrected 18→27 Nm (F-042) |
| Cammus C5 / C12 | Verified | Confirmed against hid-ids.h |
| VRS DirectForce Pro | Verified | PID `0xA355` confirmed via linux-steering-wheels; VID collision with Simagic documented |
| OpenFFBoard | Verified | Main PID `0xFFB0` confirmed (pid.codes); alt `0xFFB1` **unverified — absent from all sources** (F-037) |
| FFBeast | Verified | PIDs confirmed via hid-ids.h |
| Leo Bodnar | Community-reported | VID confirmed; SLI-Pro PID `0x1301` **estimated** — community reports, not hardware-verified (F-036) |
| AccuForce Pro | Community-reported | PID `0x804C` confirmed from USB captures |
| Heusinkveld (Sprint, Ultimate+, Pro) | Community (OpenFlight) | VID `0x04D8` (Microchip); PIDs from OpenFlight device manifests |
| Cube Controls | Estimated (**Provisional — PIDs unconfirmed, product pages 404; input-only, not wheelbases**) | Button boxes (non-FFB); see F-038 |
| PXN | Community-reported | PIDs from linux-steering-wheels |
| Granite Devices / OSW (SimpleMotion V2) | Community-reported | Legacy OSW generation |

---

## Adding New Devices

### Overview

Each vendor is implemented as a self-contained "microcrate" in `crates/hid-<vendor>-protocol/`. The crate is intentionally I/O-free and allocation-free so it can be tested and fuzzed without hardware.

### Step-by-step

1. **Obtain authoritative VID/PID values.**
   - Use `lsusb -v` (Linux), USBTreeView (Windows), or a Wireshark/Zadig capture.
   - Check the official vendor SDK or Linux kernel `hid-ids.h` if available.
   - Record the source in `docs/protocols/SOURCES.md` with a **Verified**, **Community**, or **Estimated** tag before writing any code.

2. **Create the protocol microcrate.**
   ```
   crates/hid-<vendor>-protocol/
     src/
       ids.rs      # VENDOR_ID, product_ids, is_<vendor>_product()
       types.rs    # DeviceIdentity, Model enum, max_torque_nm()
       input.rs    # report parsing, InputState struct
       output.rs   # FFB encoders, build_* functions
       lib.rs      # #![deny(static_mut_refs)]; flat re-exports
   ```
   - Add the new crate to the workspace `Cargo.toml`.
   - Use workspace dependencies where possible.
   - All hot-path code must be allocation-free (no `Vec`, `HashMap`, `String`).

3. **Register the vendor in the engine.**
   - Add a match arm in `crates/engine/src/hid/vendor/mod.rs` → `get_vendor_protocol()`.
   - Create `crates/engine/src/hid/vendor/<vendor>.rs` implementing `VendorProtocol`.
   - Register the `FfbConfig` (max torque, encoder CPR, vendor usage page flag).

4. **Write tests.**
   - Unit tests in `crates/hid-<vendor>-protocol/src/*.rs` (inline `#[test]`).
   - Vendor integration tests in `crates/engine/src/hid/vendor/<vendor>_tests.rs`.
   - Use `Result`-returning test functions; no `unwrap()`/`expect()` in test code.
   - Add snapshot/regression tests for report encoding golden values.

5. **Update documentation.**
   - Add a row to the relevant table(s) in this file (`docs/DEVICE_CAPABILITIES.md`).
   - Add a section to `docs/protocols/SOURCES.md` citing the VID/PID source.

6. **Run CI checks.**
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-features --workspace
   cargo deny check
   ```

### VID collision handling

Several vendors share a USB VID (see [VID Collision Map](#vid-collision-map) and `docs/protocols/SOURCES.md#vid-collision-map`). When adding a new device under a shared VID, update the disambiguation logic in `get_vendor_protocol()` and add a comment explaining the collision resolution strategy (PID range, `iProduct` string, or feature report probe).

---

## Source Citations

All VID/PID data in this document traces back to `docs/protocols/SOURCES.md`. Torque, encoder resolution, and effect-type data is derived directly from the Rust source; see the files listed below.

| Data | Source file |
|---|---|
| Logitech VID/PIDs, torque, rotation, TrueForce | `crates/hid-logitech-protocol/src/ids.rs`, `types.rs` |
| Thrustmaster VID/PIDs, torque, rotation | `crates/hid-thrustmaster-protocol/src/ids.rs` |
| Fanatec VID/PIDs, torque, encoder, 1 kHz | `crates/hid-fanatec-protocol/src/ids.rs`, `types.rs` |
| Moza VID/PIDs, torque | `crates/hid-moza-protocol/src/ids.rs`, `types.rs` |
| Simagic VID/PIDs, torque, effects | `crates/hid-simagic-protocol/src/ids.rs`, `types.rs` |
| Simucube VID/PIDs, torque, encoder | `crates/hid-simucube-protocol/src/lib.rs`, `types.rs` |
| Asetek VID/PIDs, torque | `crates/hid-asetek-protocol/src/lib.rs`, `types.rs` |
| Cammus VID/PIDs, torque, rotation | `crates/hid-cammus-protocol/src/ids.rs`, `types.rs` |
| VRS VID/PIDs, torque, effects | `crates/hid-vrs-protocol/src/ids.rs`, `types.rs` |
| OpenFFBoard VID/PIDs | `crates/hid-openffboard-protocol/src/lib.rs` |
| FFBeast VID/PIDs | `crates/hid-ffbeast-protocol/src/lib.rs` |
| Leo Bodnar VID/PIDs, encoder | `crates/hid-leo-bodnar-protocol/src/ids.rs`, `types.rs` |
| AccuForce VID/PIDs, torque | `crates/hid-accuforce-protocol/src/ids.rs`, `types.rs` |
| Heusinkveld VID/PIDs, load ratings | `crates/hid-heusinkveld-protocol/src/lib.rs` |
| Cube Controls (provisional) | `crates/hid-cube-controls-protocol/src/lib.rs` |
| PXN VID/PIDs | `crates/hid-pxn-protocol/src/ids.rs` |
| Granite Devices / OSW torque, encoder | `crates/simplemotion-v2/src/types.rs` |
| Supported vendor registry | `crates/engine/src/hid/vendor/mod.rs` |
| All VID/PID authoritative sources | `docs/protocols/SOURCES.md` |
