# Hardware Readiness Prep — Research Report

Pre-hardware desk research for Phase 5 → Phase 6 transition. Covers the Moza R5 stack.

**Date:** 2026-03-16  
**Reference hardware:** R5 (wheelbase), KS (wheel), ES (wheel), SR-P (pedals), HBP (handbrake)

---

## 1. Software Build Verification ✅

| Binary | Package | Compile Status |
|--------|---------|---------------|
| `wheelctl` | `wheelctl` | ✅ Clean |
| `hid-capture` | `racing-wheel-hid-capture` | ✅ Clean |
| `wheeld` | `racing-wheel-service` | ✅ Clean |

All three tools compile on Windows without errors.

## 2. Device Identity & Enumeration

### VID/PID Table

| Device | VID | PID V1 | PID V2 | Category | FFB | Topology |
|--------|-----|--------|--------|----------|-----|----------|
| R5 | `0x346E` | `0x0004` | `0x0014` | `Wheelbase` | ✅ | Aggregated |
| SR-P | `0x346E` | `0x0003` | — | `Pedals` | ❌ | Standalone |
| HBP | `0x346E` | `0x0022` | — | `Handbrake` | ❌ | Standalone |
| HGP | `0x346E` | `0x0020` | — | `Shifter` | ❌ | Standalone |
| SGP | `0x346E` | `0x0021` | — | `Shifter` | ❌ | Standalone |

### V2 PID Pattern

V2 PIDs follow the pattern `V1 | 0x0010`. Confirmed in Linux kernel `hid-ids.h`:

```
R5  V1=0x0004 → V2=0x0014  (0x0004 | 0x0010)
R3  V1=0x0005 → V2=0x0015
R9  V1=0x0002 → V2=0x0012
R12 V1=0x0006 → V2=0x0016
```

### Rim IDs (via wheelbase transport, not USB PIDs)

| Rim | ID | Status |
|-----|----|--------|
| CS V2 | `0x01` | Parser ready |
| GS V2 | `0x02` | Parser ready |
| RS V2 | `0x03` | Parser ready |
| FSR | `0x04` | Parser ready |
| **KS** | **`0x05`** | Parser ready |
| **ES** | **`0x06`** | Parser ready |

### ES Compatibility

- R5 V1 (`0x0004`) → `Supported`
- R5 V2 (`0x0014`) → `Supported`
- R9 V1 (`0x0002`) → `UnsupportedHardwareRevision`
- R9 V2 (`0x0012`) → `Supported`

## 3. Device Quirks

Code reference: [quirks.rs](../crates/engine/src/hid/quirks.rs)

| Quirk | R5 (wheelbase) | SR-P (pedals) | HBP (handbrake) |
|-------|---------------|---------------|-----------------|
| `fix_conditional_direction` | ✅ `true` | ❌ `false` | ❌ `false` |
| `uses_vendor_usage_page` | ✅ `true` | ✅ `true` | ✅ `true` |
| `required_b_interval` | `Some(1)` (1ms) | `Some(1)` | `Some(1)` |
| `requires_init_handshake` | ✅ `true` | ❌ `false` | ❌ `false` |
| `aggregates_peripherals` | V2 only | ❌ `false` | ❌ `false` |

### Linux Kernel Confirmation

`hid-universal-pidff.c` applies `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION` to **all** Moza wheelbases (R3/R5/R9/R12/R16_R21, both V1 and V2). Our `fix_conditional_direction: true` matches exactly.

## 4. HID Report Layouts

### Wheelbase Aggregated Input (report ID `0x01`)

```
Offset  Size  Field
0       1     Report ID (0x01)
1-2     2     Steering (u16 LE)
3-4     2     Throttle (u16 LE)
5-6     2     Brake (u16 LE)
7-8     2     Clutch (u16 LE, optional)
9-10    2     Handbrake (u16 LE, optional)
11-26   16    Buttons (16 bytes)
27      1     Hat (8-way D-pad)
28      1     Funky (rim ID discriminator)
29-30   2     Rotary (2 bytes)
───────────────────────────────
Total: 31 bytes (full), 7 bytes (minimum: steering + throttle + brake)
```

### SR-P Standalone Report

```
Offset  Size  Field
0       1     Report ID
1-2     2     Throttle (u16 LE)
3-4     2     Brake (u16 LE)
───────────────────────────────
Total: 5 bytes minimum
```

> Clutch is not exposed in standalone mode — it is aggregated through the wheelbase when connected via the pedal port.

### HBP Standalone Report (3 layouts)

| Layout | Format | Size |
|--------|--------|------|
| Prefixed | `[report_id, axis_lo, axis_hi, button]` | 4 bytes |
| Raw + button | `[axis_lo, axis_hi, button]` | 3 bytes |
| Raw minimal | `[axis_lo, axis_hi]` | 2 bytes |

## 5. Direct Torque Encoder — Safety Review

Code reference: [direct.rs](../crates/hid-moza-protocol/src/direct.rs)

### Wire Format (report `0x20`, 8 bytes)

```
Byte  Field                  R5 Example (10%)
0     Report ID              0x20
1-2   Torque (i16 LE)        0xCD 0x0C  (3277 ≈ i16::MAX × 0.1)
3     Flags                  0x01       (bit0=motor enable)
4-5   Slew rate (u16 LE)     0x00 0x00  (disabled)
6-7   Reserved               0x00 0x00
```

### Safety Properties

| Property | Verified |
|----------|----------|
| `encode_zero()` output | `[0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` ✅ |
| Motor enable only on non-zero torque | `flags \|= 0x01` only when `torque_raw != 0` ✅ |
| Torque clamp | `(torque_nm / max_torque_nm).clamp(-1.0, 1.0)` ✅ |
| Zero-max returns 0 | `max_torque_nm ≤ ε → return 0i16` ✅ |
| R5 max torque | 5.5 Nm (`MozaModel::R5.max_torque_nm()`) ✅ |
| Slew rate gated | `use_slew_rate` must be explicitly enabled ✅ |

### Torque Values for R5 (5.5 Nm max)

| Percent | Nm | Raw i16 |
|---------|-----|---------|
| 0% | 0 | 0 |
| 10% | 0.55 | 3277 |
| 25% | 1.375 | 8192 |
| 50% | 2.75 | 16384 |
| 75% | 4.125 | 24576 |
| 100% | 5.5 | 32767 |

## 6. Safety Interlock & Watchdog

### Watchdog

| Property | Value |
|----------|-------|
| Default timeout | **100ms** (`SoftwareWatchdog::with_default_timeout()`) |
| Timeout response | Zero torque, `SafeMode` state |
| Emergency stop latency | < 1ms (tested) |
| Fault-to-zero-torque | < 10ms (tested) |
| Process tick latency | < 1ms in normal (tested over 100 iterations) |

### State Machine

```
Normal → Warning (threshold breach)
Normal → SafeMode (watchdog timeout, communication loss, fault)
Normal → EmergencyStop (explicit trigger)
SafeMode → Normal (reset after minimum duration ≥100ms)
EmergencyStop → Normal (reset)
```

All transitions produce zero torque in `SafeMode` and `EmergencyStop`. Property-based tests (200 cases) verify invariant: any fault immediately clamps torque to zero.

## 7. Protocol Notes

### Moza Pit House Interaction & Windows HID Sharing

- **Interface Model**: Moza devices expose both HID (FFB/Input) and CDC ACM (Serial Configuration) interfaces. Pit House primarily uses the Serial interface for settings but holds a shared HID handle for FFB and input monitoring.
- **Windows HID Access**: OpenRacing uses `hidapi` on Windows, which defaults to `FILE_SHARE_READ | FILE_SHARE_WRITE`. This allows OpenRacing and Pit House to share the same HID device simultaneously.
- **Protocol Concurrency**: Both applications may attempt to send the 3-step feature report handshake (0x02, 0x03, 0x11). 
    - **Conflict Risk**: If OpenRacing requests `Direct` FFB mode (0x02) while Pit House requests `Standard` (0x00), the device state may flip-flop.
    - **Stability Strategy**: Simulation (Phase 3) will verify OpenRacing’s behavior under handshake race conditions.
- **Fingerprinting Parity**: Windows hidapi enumeration does not expose raw HID report descriptor bytes. For the receipt-backed Moza lane, export the descriptor with USBTreeView, USBPcap/Wireshark, Linux sysfs, or an equivalent tool and import it with `wheelctl moza descriptor --report-descriptor-hex-file` or `--report-descriptor-bin-file` so the lane stores descriptor hex, parsed report metadata, and CRC evidence. If direct report `0x20` remains unproven, generated direct-report guidance must stay read-only. Bounded output bring-up may proceed only through an explicit strategy with descriptor-proven report metadata, such as the live R5 V1 `pidff-bounded-effect` path, and high torque stays disabled.

### HID vs Serial/CDC ACM

Moza devices expose **two** USB interfaces:
1. **HID interface** — used for input reports, FFB output (report `0x20`), feature reports (handshake, rotation range, FFB mode)
2. **CDC ACM (serial)** — used by boxflat/Moza Pit House for device configuration at 115200 baud

OpenRacing uses only the HID interface. The serial interface is not needed for FFB or input.

### boxflat Name Patterns

boxflat identifies Moza peripherals by HID device name, not PID:
- Pedals: `"gudsen moza (srp|sr-p|crp)[0-9]? pedals"`
- H-pattern shifter: `"hgp shifter"`
- Sequential shifter: `"sgp shifter"`
- Handbrake: `"hbp handbrake"`

### DFU Mode Risk

Some Moza devices support firmware update via Moza Pit House. During testing:
- **Do not** send firmware update commands
- **Avoid** holding specific button combinations during boot that might trigger DFU/bootloader mode
- OpenRacing does not implement any DFU commands; this is a manual-only risk

---

## 8. Known Gaps

| Item | Status | Risk |
|------|--------|------|
| CRP pedals PID | May share `0x0003` with SR-P or use distinct PID | Low — not in test hardware |
| Universal Hub PID | Not catalogued | Low — not in test hardware |
| E-Stop/Dashboard/Stalks | Serial-only device IDs (no USB PID) | None — serial not used |
| SR-P clutch in standalone mode | Not exposed (requires wheelbase aggregation) | **Test this** — verify in Phase 7 |

---

*Source files reviewed:*
- [ids.rs](../crates/hid-moza-protocol/src/ids.rs) — VID/PID constants
- [types.rs](../crates/hid-moza-protocol/src/types.rs) — device identity, models, ES compatibility
- [direct.rs](../crates/hid-moza-protocol/src/direct.rs) — torque encoder
- [quirks.rs](../crates/engine/src/hid/quirks.rs) — device quirks
- [moza.rs](../crates/engine/src/hid/vendor/moza.rs) — vendor dispatch
- [hardware_watchdog.rs](../crates/engine/src/safety/hardware_watchdog.rs) — watchdog + interlock
- [lib.rs (moza-wheelbase-report)](../crates/moza-wheelbase-report/src/lib.rs) — report layout
- [lib.rs (srp)](../crates/srp/src/lib.rs) — SR-P standalone report
- [lib.rs (hbp)](../crates/hbp/src/lib.rs) — HBP standalone report
- Linux kernel [hid-universal-pidff.c](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-universal-pidff.c)
