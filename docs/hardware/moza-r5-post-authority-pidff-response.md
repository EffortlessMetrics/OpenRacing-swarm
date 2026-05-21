# Moza R5 Post-Authority PIDFF Response

Status: complete no-output diagnosis
Lane: `ci/hardware/moza-r5/2026-05-13`
Primary receipt: `vendor-post-authority-pidff-response.json`
Claim ceiling: post-authority PIDFF comparison only

## Verdict

The first consumed vendor-authority frame did not improve standard PIDFF
authority for the tested R5 lane.

The comparison receipt classifies the result as:

```text
post_authority_pidff_response_regressed
```

The preserved baseline response moved `0.18127718013275285` degrees. The
post-authority response moved `0.032959487296864154` degrees. The measured
change was `-0.1483176928358887` degrees, below the prior response band and
still below the 1 degree visible-motion threshold.

## Evidence

| Artifact | Role |
| --- | --- |
| `vendor-authority-attempt.json` | Consumed exact `estop_set_ffb` authority attempt |
| `native-actuator-visible-smoke-response-only.json` | Preserved baseline PIDFF response receipt |
| `vendor-post-authority-pidff-smoke.json` | Separately authorized post-authority PIDFF response capture |
| `vendor-post-authority-pidff-response.json` | No-output baseline-vs-post comparison |

The post-authority PIDFF response capture kept the same safety envelope as the
baseline comparison path:

- 5 percent maximum
- 2000 ms duration
- PIDFF bounded effect
- no direct HID report `0xaf`
- no high torque
- no serial configuration
- no firmware or DFU
- final zero and final Stop All recorded

## Boundary

This diagnosis does not authorize another vendor-authority frame, another PIDFF
response capture, a controlled-angle rerun, force escalation, a longer dwell, or
larger angle targets. It also does not claim native control, native visible
motion, smoke-ready, release-ready, Pit House coexistence, simulator telemetry,
or simulator FFB.

The current evidence says the tested `estop_set_ffb` frame is not a sufficient
native-visible unlock for the standard PIDFF path. Future output requires new
protocol evidence, a reviewed plan, fresh command-bound bench-clear evidence,
and a new exact authorization.

## Next Research Path

Return to no-output protocol review before planning any further motion ladder:

```text
review post-authority comparison
inspect Pit House / SimHub / simulator sniff summaries
identify defensible enable, gain, mode, or authority semantics
design a reviewed vendor-control plan
only then consider another exact-authorized output attempt
```

Passive sniffing and vendor report decoding remain protocol research only. They
do not satisfy native-visible or smoke-ready gates by themselves.
