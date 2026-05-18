# Moza R5 PIDFF Semantics Diagnosis

This note is a no-output diagnosis after the preserved controlled-angle retry. It is not a new authorization and it does not claim native visible motion.

## Evidence

| Artifact | PIDFF shape | Result |
| --- | --- | --- |
| `low-torque-proof.json` | Set Effect -> Set Constant Force -> Effect Start -> Stop All | Bounded low-torque proof passed |
| `native-actuator-visible-smoke.json` | Set Effect -> outbound force -> Start -> return/settle force -> Stop All | Measured `0.18127718013275285` degrees, below the 1 degree visible threshold |
| `native-controlled-angle-smoke.json` | One outbound controlled-angle PIDFF sequence plus Stop All | Timed out before target at `0.18127718013275285` degrees |
| `native-controlled-angle-retry-smoke.json` | Eight outbound micro-step repetitions plus final Stop All | Also timed out before target at `0.18127718013275285` degrees |

The retry changed write count, not motion. It recorded `writes_ok=33`, `write_errors=0`, live steering samples, final Stop All, and stable post-stop behavior, but it stayed in the same response band as the first attempt.

## Reading

The transport path is not the obvious blocker. The receipts show successful PIDFF writes, feedback samples, and cleanup. The blocker is more likely PIDFF effect semantics, wheelbase state, or profile authority inside the current 5 percent envelope.

Important questions before any third attempt:

- Does this R5 state require a different effect lifecycle, such as explicit effect allocation, block load validation, or block free handling?
- Does Stop All after every 250 ms micro-step prevent the wheel from accumulating visible movement?
- Does the device need actuator enable, device gain, or a different start/update order before the constant force takes authority?
- Are direction, gain, duration, or loop count interpreted differently from the descriptor-derived assumption?

## Guardrails

This diagnosis does not authorize:

- blind third run
- force escalation
- dwell extension
- 30 degree or 90 degree tests
- direct report `0x20`
- high torque
- serial config
- firmware or DFU
- Pit House or SimHub as native prerequisites

The next software step is the no-output [PIDFF lifecycle trace](moza-r5-pidff-lifecycle-trace.md). It reads the preserved receipts, decodes the set-effect / constant-force / effect-operation / Stop All sequence, and still carries no readiness claim. A later profile plan or hardware attempt still requires fresh review, command-bound bench clear, and exact authorization.
