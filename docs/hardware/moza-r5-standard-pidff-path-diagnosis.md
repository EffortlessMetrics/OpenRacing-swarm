# Moza R5 Standard PIDFF Path Diagnosis

Status: complete no-output diagnosis
Lane: `ci/hardware/moza-r5/2026-05-13`
Artifact: `native-pidff-standard-path-diagnosis.json`
Claim ceiling: protocol architecture diagnosis only

## Verdict

Classify the standard PIDFF controlled-angle path as:

```text
standard_pidff_controlled_angle_path_ineffective_in_current_r5_mode
```

Three bounded standard-PIDFF-family controlled-angle attempts all landed in the
same undertravel band, about `0.181277` degrees, and all timed out before the
1 degree target. The result is strong enough to stop iterating standard PIDFF
profile variants until new protocol evidence exists.

## Evidence

| Attempt | Receipt | Profile | Writes | Result |
| --- | --- | --- | --- | --- |
| First controlled-angle attempt | `native-controlled-angle-smoke.json` | baseline `pidff-bounded-effect` | 5 ok, 0 errors | `0.18127718013275285` degrees, timeout |
| Reviewed retry | `native-controlled-angle-retry-smoke.json` | `bounded-pidff-micro-step-v2` | 33 ok, 0 errors | `0.18127718013275285` degrees, timeout |
| Attempt 03 | `native-controlled-angle-attempt-03-smoke.json` | `bounded-pidff-effect-lifecycle-v1` | 4 ok, 0 errors | `0.18127718013275285` degrees, timeout |

All three receipts preserve the same safety envelope:

- transport writes worked
- steering feedback worked
- cleanup worked
- post-stop stability worked
- standard PIDFF writes were accepted
- standard PIDFF effect lifecycle still did not produce visible controlled motion
- native-visible remains blocked
- no further output is authorized

Attempt 03 also consumed `native-controlled-angle-attempt-03-authorization.json`
and records final Stop All plus final zero sent.

## Boundary

This diagnosis does not authorize hardware output. It does not replace any
hardware receipt and does not claim `native_visible_ready`, `smoke_ready`, or
`release_ready`.

Forbidden next steps remain:

- blind rerun
- longer dwell
- force increase
- 3, 5, 30, or 90 degree attempts
- direct report `0x20`
- high torque
- serial config
- firmware or DFU

## Next Research Path

Move to no-output Moza vendor-specific enable/control path investigation:

```text
sniff Pit House / SimHub
decode vendor reports
map report IDs
identify enable/gain/mode handshakes
design reviewed vendor-control plan
only then consider another output attempt
```

Passive sniffing and vendor report decoding are protocol research only. They do
not satisfy native-visible or smoke-ready gates by themselves.
