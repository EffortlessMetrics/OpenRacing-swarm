# Gaming Setup Guide

Quick reference for configuring racing simulators with OpenRacing hardware.

For initial installation and device detection, see [SETUP.md](SETUP.md). For in-depth CLI usage and profile editing, see [USER_GUIDE.md](USER_GUIDE.md).

---

## Table of Contents

1. [Linux / Steam Proton](#linux--steam-proton)
2. [Windows](#windows)
3. [Game-Specific Notes](#game-specific-notes)
4. [Troubleshooting](#troubleshooting)

---

## Linux / Steam Proton

### Device Detection

OpenRacing ships udev rules and hwdb entries that classify your device as a joystick/wheel. After package installation, devices should be automatically detected by the kernel.

If a device is not detected:

```bash
# 1. Verify udev rules are installed
ls /etc/udev/rules.d/99-racing-wheel-suite.rules

# 2. Verify hwdb is installed
ls /etc/udev/hwdb.d/99-racing-wheel-suite.hwdb

# 3. Reload rules and rebuild hwdb
sudo udevadm control --reload-rules && sudo udevadm trigger && sudo systemd-hwdb update

# 4. Check device classification
udevadm info /dev/input/eventX | grep ID_INPUT_JOYSTICK
```

Replace `eventX` with your device's event node (find it with `ls /dev/input/by-id/*wheel*` or `evtest`).

### SDL Wheel Hints

Some games running under Proton use SDL2 for joystick enumeration and may not recognize your wheel without an explicit hint. Set the `SDL_JOYSTICK_WHEEL_DEVICES` environment variable with comma-separated `VID/PID` pairs:

```bash
# In ~/.bashrc or as a Steam launch option prefix
export SDL_JOYSTICK_WHEEL_DEVICES=0x044F/0xB66E,0x046D/0xC266,0x0EB7/0x0020,0x346E/0x0002,0x0483/0x0522,0x3670/0x0501
```

| VID/PID | Device |
|---------|--------|
| `0x044F/0xB66E` | Thrustmaster T300RS |
| `0x044F/0xB696` | Thrustmaster T248 |
| `0x046D/0xC266` | Logitech G923 |
| `0x046D/0xC24F` | Logitech G29 |
| `0x0EB7/0x0020` | Fanatec CSL DD |
| `0x346E/0x0002` | Moza R9 |
| `0x0483/0x0522` | Simagic Alpha / Mini / M10 |
| `0x3670/0x0501` | Simagic EVO |

Add your device's VID/PID to the list. Find it with `lsusb` or `wheelctl device list --detailed`. The full VID/PID table is in [DEVICE_SUPPORT.md](DEVICE_SUPPORT.md).

For **Steam launch options**, prepend the variable to the command:

```
SDL_JOYSTICK_WHEEL_DEVICES=0x044F/0xB66E,0x046D/0xC266 %command%
```

### Steam Input Configuration

1. Open **Steam → Settings → Controller**.
2. Under **Desktop Configuration**, verify your wheel appears.
3. For Proton games that don't detect the wheel, try enabling **Steam Input** per-game:
   right-click game → Properties → Controller → "Enable Steam Input".
4. Some games work better with Steam Input **disabled** (native HID). Test both.

### Proton Troubleshooting

| Symptom | Fix |
|---------|-----|
| No FFB in Proton | Set `PROTON_LOG=1 %command%` in launch options and check `~/steam-*.log` for HID errors. |
| Device disconnects mid-session | Verify kernel quirks are installed: `ls /etc/modprobe.d/90-racing-wheel-quirks.conf` |
| Wheel detected as gamepad | Add VID/PID to `SDL_JOYSTICK_WHEEL_DEVICES` (see above). |
| Permission denied on `/dev/input/*` | Add your user to the `input` group: `sudo usermod -aG input $USER` then log out and back in. |

---

## Windows

### Device Detection

Windows detects most USB HID wheels automatically. The OpenRacing `wheeld` service handles force feedback communication once the device appears in Device Manager.

```bash
# Verify device is visible to OpenRacing
wheelctl device list
```

### Common Issues

| Symptom | Fix |
|---------|-----|
| Multiple devices (pedals + wheel) show as one | Check **Game Controllers** (`joy.cpl`) — they should appear as separate entries. Some vendors bundle axes; consult vendor docs. |
| FFB not working | Open **Device Manager** → expand "Human Interface Devices" → confirm "HID-compliant game controller" is present and has no warning icon. |
| Device not appearing | Try a different USB port. Avoid USB hubs for wheelbases. Check Device Manager for unknown devices. |
| Fanatec wheel not detected | Install the official Fanatec driver first — it is required before OpenRacing can open the device. |

---

## Game-Specific Notes

Use `wheelctl game list` to see all supported game IDs. Use `wheelctl game configure <game_id>` to auto-write telemetry configuration.

### iRacing

- **Integration:** shared memory (auto-configured).
- No special setup needed. OpenRacing writes `app.ini` changes on first run.
- If iRacing was already running, restart it after `wheelctl game configure iracing`.

### Assetto Corsa Competizione (ACC)

- **Integration:** UDP broadcast on port 9000.
- Enable **Broadcasting Mode** in ACC settings (Settings → Connection).
- OpenRacing writes `broadcasting.json` on first setup. Restart ACC afterward.
- `wheelctl game configure acc`

### rFactor 2

- **Integration:** shared memory on Windows, HID PID on Linux.
- FFB uses DirectInput on Windows. On Linux/Proton, HID PID force-feedback is used.
- `wheelctl game configure rfactor2`

### Dirt Rally 2.0

- **Integration:** Codemasters UDP mode 1.
- Enable UDP telemetry by editing `hardware_settings_config.xml` in the game's documents folder.
- Default port: `20777`.
- `wheelctl game configure dirt_rally_2`
- Record a real-game telemetry-only session with:
  `wheelctl telemetry record --game dirt_rally_2 --telemetry-source real_game --live-game --game-port 20777 --duration-ms 30000 --out simulator-telemetry-recording.jsonl`

### EA SPORTS WRC

- **Integration:** UDP telemetry on port 20778.
- Enable telemetry in the game's settings or config file.
- `wheelctl game configure eawrc`

### Forza Motorsport / Forza Horizon

- **Integration:** Forza Data Out UDP on port 5300.
- In-game: enable **Data Out** in HUD & Gameplay settings. Set IP to `127.0.0.1`, port to `5300`.
- `wheelctl game configure forza_motorsport`

### BeamNG.drive

- **Integration:** UDP OutGauge on port 4444.
- OpenRacing auto-configures on first detection.
- `wheelctl game configure beamng_drive`

### F1 24 / F1 25

- **Integration:** Codemasters UDP on port 20777.
- Enable UDP telemetry in game settings.
- `wheelctl game configure f1`

---

## Troubleshooting

### Device not recognized

```bash
# 1. List detected hardware
wheelctl device list

# 2. Check VID/PID
#    Linux:   lsusb
#    Windows: Device Manager → device properties → Hardware IDs

# 3. Verify VID/PID is supported
wheelctl device list --all    # shows every HID device with VID/PID
```

Cross-reference with [DEVICE_SUPPORT.md](DEVICE_SUPPORT.md). If your device is missing, open a GitHub issue with the VID, PID, and device name.

### FFB not working

```bash
# 1. Quick vibration test
wheelctl diag test

# 2. Check service health
wheelctl health

# 3. Review logs
#    Linux:   journalctl --user -u openracing
#    Windows: check %LOCALAPPDATA%\wheel\logs\
```

Ensure in-game FFB strength is set to a non-zero value. Some games require restarting after initial configuration.

### Game not detected

```bash
# 1. Check game status
wheelctl game status

# 2. Verify expected process names
wheelctl game list --detailed

# 3. Manually apply config and restart the game
wheelctl game configure <game_id>
```

---

## Further reading

- [SETUP.md](SETUP.md) — installation and first-run setup
- [USER_GUIDE.md](USER_GUIDE.md) — in-depth usage, profiles, and CLI reference
- [DEVICE_SUPPORT.md](DEVICE_SUPPORT.md) — full VID/PID matrix for all 28 vendors
- [GAME_SUPPORT.md](GAME_SUPPORT.md) — complete game compatibility list
- [SYSTEM_INTEGRATION.md](SYSTEM_INTEGRATION.md) — detailed integration architecture
