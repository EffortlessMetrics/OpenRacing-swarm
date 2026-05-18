# Moza R5 PIDFF Lifecycle Trace

`wheelctl moza pidff-lifecycle-trace` is a no-output diagnostic command for
controlled-angle receipts. It reads an existing JSON receipt and explains the
PIDFF lifecycle OpenRacing already sent.

Example:

```powershell
wheelctl moza pidff-lifecycle-trace `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --receipt ci/hardware/moza-r5/2026-05-13/native-controlled-angle-retry-smoke.json `
  --json-out ci/hardware/moza-r5/2026-05-13/native-pidff-lifecycle-trace.json `
  --md-out target/moza-pidff-lifecycle-trace.md `
  --json
```

The command decodes:

- Set Effect
- Set Constant Force
- Effect Operation Start and Stop
- Device Control Stop All
- effect block / effect index
- report id and payload length
- force magnitude
- direction / axis fields when present
- timing between lifecycle records
- profile phase and clear events

The trace is diagnosis evidence only. It does not open a HID device, does not
send HID output reports, does not create an authorization receipt, and does not
satisfy native-visible, smoke-ready, or release-ready gates.

The current Moza R5 frontier remains repeated safe undertravel: the first
controlled-angle attempt and the reviewed retry both stayed around 0.181277
degrees, even though the retry increased bounded PIDFF writes from 5 to 33. The
trace layer exists to determine whether those attempts changed the PIDFF
lifecycle materially before any new profile or output attempt is reviewed.

The `2026-05-13` lane now records the trace in
`native-pidff-lifecycle-trace.json`. It classifies the retry as repeated
setup/start/Stop-All cycles, compares the first attempt and retry as the same
delta band despite lifecycle replay, and still authorizes no output.
