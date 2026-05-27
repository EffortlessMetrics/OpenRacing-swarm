# Passive USB Sniff Operator Notes

This template is non-claiming protocol research/support evidence.

Plan: `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-plan.json`
Family: `moza-r5`
Scenario: `pit-house-setting-change`
Lane: `ci/hardware/moza-r5/2026-05-13`
Operator: `Steven`
Device: `Moza R5 PID 0x0004 with KS/ES wheels, SR-P pedals, and HBP handbrake attached through the R5 hub`

## Required Notes

- [x] scenario performed: Pit House closed, Pit House opened, KS wheel top-left front LED changed from default teal to red, then changed back to default teal.
- [x] external app, simulator, or OS stack observed: MOZA Pit House.
- [x] capture duration or start/stop times: capinfos reports 113.446197 seconds, earliest packet 2026-05-27 12:11:53.989117, latest packet 2026-05-27 12:13:47.435314.
- [x] device stack attached: Moza R5 PID 0x0004 with KS wheel attached through the R5 hub.
- [x] whether firmware/update/DFU pages stayed closed: no firmware/update/DFU page or prompt was observed by the operator; no firmware/update/DFU interaction was performed.
- [x] whether raw pcapng was kept local or reviewed for bundling: raw pcapng kept local under target/sniff/pit-house-setting-change-repeat-02/capture.pcapng; not committed.
- [x] exact Pit House setting changed: KS wheel top-left front LED.
- [x] starting setting value: default teal.
- [x] ending setting value: red.
- [x] whether the setting value was restored: yes, changed back to default teal.

## Capture Tool Hints

Hardware doctor receipt: `target/moza-current/pre-setting-change-capture-hardware-doctor.json`

- [x] Hardware doctor no-HID/no-output/no-feature/no-serial-config/no-firmware flags stayed true: `true`
- [x] USBPcap interface used: `\\.\USBPcap2`
- [x] USBPcap device filter used: `--devices 4`
- [x] Bounded wheelctl USBPcapCMD capture helper was run for this scenario with `--duration-ms 120000`, `--usbpcap-interface "\\.\USBPcap2"`, and `--devices 4`. The helper returned `os error 32` while opening the finalized pcap for its local capture receipt; the pcap itself finalized and was validated with `capinfos` and `wheelctl hardware sniff-summary`.

```powershell
wheelctl hardware sniff-capture --usbpcapcmd "C:\Program Files\Wireshark\extcap\USBPcapCMD.exe" --usbpcap-interface "\\.\USBPcap2" --devices 4 --duration-ms 120000 --out "target\sniff\pit-house-setting-change-repeat-02\capture.pcapng" --confirm-external-passive-capture --json-out "target\sniff\pit-house-setting-change-repeat-02\sniff-capture-receipt.json"
```
- [ ] External USBPcapCMD capture command; run outside OpenRacing and stop it after the scenario:

```powershell
& "C:\Program Files\Wireshark\extcap\USBPcapCMD.exe" -d "\\.\USBPcap2" --devices 4 --inject-descriptors -o "target\sniff\pit-house-setting-change-repeat-02\capture.pcapng"
```
- [x] Suggested capture filter: `select \\.\USBPcap2 with USBPcap --devices 4`
- [x] Matched device stack: `USB Serial Device (COM4)`, `MOZA Windows Driver`, `MOZA WheelBase Virtual Device 2`, `MOZA WheelBase Virtual Device 3`, `MOZA WheelBase Virtual Device 4`
- [x] Hint boundary: hardware doctor is observe-only; these hints only identify the passive capture interface and device filter
- [x] Hint boundary: operator notes do not prove a pcap capture exists and do not authorize OpenRacing output

## Capture Safety Confirmations

- [x] Pre-capture: confirm the target device stack is attached before starting capture
- [x] Pre-capture: start USBPcap, Wireshark, tshark, or usbmon before launching or changing the external app
- [x] Pre-capture: keep OpenRacing hardware output commands stopped for this passive capture
- [x] Pre-capture: keep firmware, update, DFU, driver replacement, Zadig, and WinUSB conversion flows closed: no firmware/update/DFU page or prompt was observed by the operator; no firmware/update/DFU interaction was performed.
- [x] Post-capture: stop capture and save the pcapng in local scratch storage
- [x] Post-capture: record operator notes before bundling
- [x] Post-capture: run sniff-receipt, sniff-notes-template, and sniff-summary before treating the capture as lane evidence
- [x] Post-capture: do not commit raw pcapng unless it is separately reviewed for size, sensitivity, and operator consent
- [x] Raw pcapng commit default remained `false`

## Claim Boundaries

- [x] OpenRacing opened no HID device for this capture.
- [x] OpenRacing sent no output, feature, serial, firmware, or DFU commands.
- [x] This note does not claim native response, native visible, smoke, or release readiness.
