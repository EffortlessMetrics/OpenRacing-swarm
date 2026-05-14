# Device Support Matrix

Full device support matrix for OpenRacing. 28 vendors with 150+ unique VID/PID pairs are listed below.

For detailed capabilities (torque, encoder CPR, rotation, FFB effects), see [`DEVICE_CAPABILITIES.md`](DEVICE_CAPABILITIES.md).

> **Pre-validation note:** Status values in this document describe source-level VID/PID confidence only. They do not mean OpenRacing has been validated on real hardware. Real-hardware claims require receipt bundles under `ci/hardware/` and validation docs under `docs/hardware/`.

---

## Status Definitions

| Status | Meaning |
|--------|---------|
| **Verified** | VID/PID source confirmed from official USB descriptor dump, SDK, or Linux kernel `hid-ids.h`; not a hardware compatibility claim |
| **Community** | VID/PID confirmed from community-maintained tables (linux-steering-wheels, iRacing forums, SimHub) |
| **Estimated** | PID extrapolated from sibling models or vendor discussion; needs hardware confirmation |

---

## Wheelbases

### 1. Logitech — VID `0x046D` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| G25 | `0xC299` | Verified | kernel hid-ids.h |
| G27 | `0xC29B` | Verified | kernel hid-ids.h |
| G27 (compat) | `0xC294` | Verified | kernel hid-ids.h |
| G29 (PS/PC) | `0xC24F` | Verified | kernel hid-ids.h |
| G920 (Xbox/PC) | `0xC262` | Verified | kernel hid-ids.h |
| G923 (native) | `0xC266` | Verified | kernel hid-ids.h |
| G923 (PS compat) | `0xC267` | Verified | kernel hid-ids.h |
| G923 (Xbox/PC) | `0xC26E` | Verified | kernel hid-ids.h |
| G PRO (PS/PC) | `0xC268` | Verified | kernel hid-ids.h |
| G PRO (Xbox/PC) | `0xC272` | Verified | kernel hid-ids.h |

### 2. Thrustmaster — VID `0x044F` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| Generic FFB (pre-init) | `0xB65D` | Verified | hid-tmff2 |
| T150 | `0xB677` | Verified | hid-tmff2, oversteer |
| TMX | `0xB67F` | Verified | hid-tmff2 |
| T300 RS (PS3) | `0xB66E` | Verified | hid-tmff2 |
| T300 RS (PS4) | `0xB66D` | Verified | hid-tmff2 |
| T300 RS GT | `0xB66F` | Verified | hid-tmff2 |
| TX Racing (Xbox) | `0xB669` | Verified | hid-tmff2 |
| T248 | `0xB696` | Verified | linux-steering-wheels |
| T248X (Xbox/GIP) | `0xB69A` | Verified | linux-steering-wheels |
| T500 RS | `0xB65E` | Verified | hid-tmff2 |
| TS-PC Racer | `0xB689` | Verified | hid-tmff2 |
| TS-XW (USB/HID) | `0xB692` | Verified | hid-tmff2 |
| TS-XW (GIP/Xbox) | `0xB691` | Verified | hid-tmff2 |
| T818 | `0xB69B` | Verified | linux-steering-wheels |
| T-GT / T-GT II | *unknown* | Estimated | PIDs unverified |

### 3. Fanatec — VID `0x0EB7` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| CSR Elite (legacy) | `0x0011` | Verified | hid-fanatecff |
| ClubSport V2 | `0x0001` | Verified | hid-fanatecff |
| ClubSport V2.5 | `0x0004` | Verified | hid-fanatecff |
| CSL Elite (PS4) | `0x0005` | Verified | hid-fanatecff |
| CSL Elite (PC) | `0x0E03` | Verified | hid-fanatecff |
| CSL DD | `0x0020` | Verified | hid-fanatecff |
| GT DD Pro | `0x0024` | Verified | hid-fanatecff |
| ClubSport DD+ | `0x01E9` | Verified | hid-fanatecff |
| Podium DD1 | `0x0006` | Verified | hid-fanatecff |
| Podium DD2 | `0x0007` | Verified | hid-fanatecff |

### 4. Moza Racing — VID `0x346E` · Status: **Source-backed / receipt-gated**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| R3 V1 | `0x0005` | Source-backed | universal-pidff; hardware receipts pending |
| R3 V2 | `0x0015` | Source-backed | universal-pidff; hardware receipts pending |
| R5 V1 | `0x0004` | Source-backed | universal-pidff; Steven lane receipts pending |
| R5 V2 | `0x0014` | Source-backed | universal-pidff; hardware receipts pending |
| R9 V1 | `0x0002` | Source-backed | universal-pidff; hardware receipts pending |
| R9 V2 | `0x0012` | Source-backed | universal-pidff; hardware receipts pending |
| R12 V1 | `0x0006` | Source-backed | universal-pidff; hardware receipts pending |
| R12 V2 | `0x0016` | Source-backed | universal-pidff; hardware receipts pending |
| R16 V1 | `0x0000` | Source-backed | universal-pidff; hardware receipts pending |
| R16 V2 | `0x0010` | Source-backed | universal-pidff; hardware receipts pending |
| R21 V1 | `0x0000` | Source-backed | universal-pidff; hardware receipts pending |
| R21 V2 | `0x0010` | Source-backed | universal-pidff; hardware receipts pending |

### 5. Simagic — VID `0x3670` (EVO) / `0x0483` (legacy) · Status: **Verified** (EVO) / **Estimated** (legacy)

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| EVO Sport | `0x0500` | Verified | simagic-ff kernel driver |
| EVO | `0x0501` | Verified | simagic-ff kernel driver |
| EVO Pro | `0x0502` | Verified | simagic-ff kernel driver |
| Alpha EVO | `0x0600` | Verified | simagic-ff kernel driver |
| Neo | `0x0700` | Verified | simagic-ff kernel driver |
| Neo Mini | `0x0701` | Verified | simagic-ff kernel driver |
| Alpha / Mini / M10 (legacy) | `0x0522` | Community | shared PID, iProduct disambiguation |

### 6. Simucube (Granite Devices) — VID `0x16D0` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| Simucube 1 | `0x0D5A` | Verified | official docs, pid.codes |
| Simucube 2 Sport | `0x0D61` | Verified | official docs |
| Simucube 2 Pro | `0x0D60` | Verified | official docs |
| Simucube 2 Ultimate | `0x0D5F` | Verified | official docs |
| Wireless Wheel Adapter | `0x0D63` | Verified | official docs |

### 7. Asetek SimSports — VID `0x2433` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| La Prima | `0xF303` | Community | community databases |
| Forte | `0xF301` | Verified | community + vendor |
| Tony Kanaan Edition | `0xF306` | Community | community databases |
| Invicta | `0xF300` | Verified | community + vendor |

### 8. Cammus — VID `0x3416` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| C5 | `0x0301` | Verified | hid-ids.h |
| C12 | `0x0302` | Verified | hid-ids.h |

### 9. VRS DirectForce — VID `0x0483` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| DirectForce Pro | `0xA355` | Verified | kernel mainline |
| DirectForce Pro V2 | `0xA356` | Estimated | no independent source confirms V2 PID |

### 10. OpenFFBoard — VID `0x1209` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| OpenFFBoard (main) | `0xFFB0` | Verified | pid.codes, firmware source |
| OpenFFBoard (alt) | `0xFFB1` | Estimated | zero evidence across 5 sources |

### 11. FFBeast — VID `0x045B` · Status: **Verified**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| FFBeast Wheel | `0x59D7` | Verified | hid-ids.h |
| FFBeast Joystick | `0x58F9` | Verified | hid-ids.h |
| FFBeast Rudder | `0x5968` | Verified | hid-ids.h |

### 12. Leo Bodnar — VID `0x1DD2` · Status: **Community**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| USB Joystick | `0x0001` | Community | vendor documentation |
| BU0836A | `0x000B` | Community | vendor documentation |
| BBI-32 | `0x000C` | Community | vendor documentation |
| Wheel Interface | `0x000E` | Community | vendor documentation |
| FFB Joystick | `0x000F` | Community | vendor documentation |
| BU0836X | `0x0030` | Community | vendor documentation |
| BU0836 16-bit | `0x0031` | Community | vendor documentation |
| SLI-Pro | `0x1301` | Estimated | community reports |

### 13. AccuForce — VID `0x1FC9` · Status: **Community**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| AccuForce Pro | `0x804C` | Community | USB captures |

### 14. PXN — VID `0x11FF` · Status: **Community**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| PXN V10 | `0x3245` | Community | linux-steering-wheels |
| PXN V12 | `0x1212` | Community | linux-steering-wheels |
| PXN V12 Lite | `0x1112` | Community | linux-steering-wheels |
| PXN V12 Lite SE | `0x1211` | Community | linux-steering-wheels |
| PXN GT987 FF | `0x2141` | Community | linux-steering-wheels |

### 15. Granite Devices / OSW (SimpleMotion V2) — VID `0x1D50` · Status: **Community**

| Model | PID | Status | Source |
|-------|-----|--------|--------|
| Simucube 1 / IONI Drive | `0x6050` | Community | community databases |
| IONI Premium | `0x6051` | Community | community databases |
| ARGON Servo Drive | `0x6052` | Community | community databases |

---

## Peripherals (Non-FFB)

### Pedals

| Device | Vendor | VID | PID | Status | Source |
|--------|--------|-----|-----|--------|--------|
| Heusinkveld Sprint | Heusinkveld | `0x04D8` | `0xF6D0` | Community | OpenFlight |
| Heusinkveld Ultimate+ | Heusinkveld | `0x04D8` | `0xF6D2` | Community | OpenFlight |
| Heusinkveld Pro | Heusinkveld | `0x04D8` | `0xF6D3` | Community | OpenFlight |
| Moza SR-P Pedals | Moza | `0x346E` | `0x0003` | Source-backed | universal-pidff; direct-plug receipts optional |
| Fanatec ClubSport V1/V2 | Fanatec | `0x0EB7` | `0x1839` | Community | hid-fanatecff |
| Fanatec ClubSport V3 | Fanatec | `0x0EB7` | `0x183B` | Community | hid-fanatecff |
| Fanatec CSL Elite Pedals | Fanatec | `0x0EB7` | `0x6204` | Community | hid-fanatecff |
| Fanatec CSL Pedals LC | Fanatec | `0x0EB7` | `0x6205` | Community | hid-fanatecff |
| Fanatec CSL Pedals V2 | Fanatec | `0x0EB7` | `0x6206` | Community | hid-fanatecff |
| Simagic P1000 | Simagic | `0x3670` | `0x1001` | Estimated | extrapolated |
| Simagic P1000A | Simagic | `0x3670` | `0x1003` | Estimated | extrapolated |
| Simagic P2000 | Simagic | `0x3670` | `0x1002` | Estimated | extrapolated |
| VRS Pedals V1 | VRS | `0x0483` | `0xA357` | Community | linux-steering-wheels |
| VRS Pedals V2 | VRS | `0x0483` | `0xA358` | Community | linux-steering-wheels |
| Cammus CP5 Pedals | Cammus | `0x3416` | `0x1018` | Community | community sources |
| Cammus LC100 Pedals | Cammus | `0x3416` | `0x1019` | Community | community sources |
| Simucube ActivePedal | Granite Devices | `0x16D0` | `0x0D66` | Verified | official docs |
| Asetek La Prima Pedals | Asetek | `0x2433` | `0xF102` | Community | community sources |

### Shifters & Handbrakes

| Device | Vendor | VID | PID | Status | Source |
|--------|--------|-----|-----|--------|--------|
| Moza HBP Handbrake | Moza | `0x346E` | `0x0022` | Source-backed | universal-pidff; direct-plug receipts optional |
| Moza HGP Shifter | Moza | `0x346E` | `0x0020` | Source-backed | universal-pidff; hardware receipts pending |
| Moza SGP Shifter | Moza | `0x346E` | `0x0021` | Source-backed | universal-pidff; hardware receipts pending |
| Simagic H-Pattern Shifter | Simagic | `0x3670` | `0x2001` | Estimated | extrapolated |
| Simagic Sequential Shifter | Simagic | `0x3670` | `0x2002` | Estimated | extrapolated |
| Simagic Handbrake | Simagic | `0x3670` | `0x3001` | Estimated | extrapolated |
| VRS Handbrake | VRS | `0x0483` | `0xA359` | Community | linux-steering-wheels |
| VRS Shifter | VRS | `0x0483` | `0xA35A` | Community | linux-steering-wheels |

### Button Boxes

| Device | Vendor | VID | PID | Status | Source |
|--------|--------|-----|-----|--------|--------|
| Cube Controls GT Pro | Cube Controls | `0x0483` | `0x0C73` | Estimated | provisional, unconfirmed |
| Cube Controls Formula Pro | Cube Controls | `0x0483` | `0x0C74` | Estimated | provisional, unconfirmed |
| Cube Controls CSX3 | Cube Controls | `0x0483` | `0x0C75` | Estimated | provisional, unconfirmed |
| Generic HID Button Box | DIY/Arduino | `0x1209` | `0x1BBD` | Community | pid.codes |

---

## Summary

| Metric | Value |
|--------|-------|
| Total vendors | 28 (15 wheelbase + 13 peripheral-only) |
| Total VID/PID pairs | 150+ |
| Verified PIDs | ~65 |
| Community PIDs | ~70 |
| Estimated PIDs | ~15 |
| Protocol crate test coverage | All 15 wheelbase vendors with advanced proptest + deep tests |

---

*See also: [`DEVICE_CAPABILITIES.md`](DEVICE_CAPABILITIES.md) for detailed FFB capabilities, torque specs, and protocol types.*
