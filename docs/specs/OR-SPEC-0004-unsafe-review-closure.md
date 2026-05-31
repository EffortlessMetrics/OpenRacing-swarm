# OR-SPEC-0004: Unsafe-review closure

Status: proposed
Owner: release/ci
Created: 2026-05-31
Linked proposal: docs/proposals/OR-PROP-0003-unsafe-review-closure-lane.md
Linked ADRs: n/a
Linked plan: plans/unsafe-review-closure/implementation-plan.md
Linked issues: n/a
Linked PRs: n/a
Support-tier impact: no public support claim
Policy impact: policy/unsafe-review-exceptions.toml

## Terms

`Unsafe-review closure` means every tracked Rust unsafe seam is reviewed,
owned, contracted, guarded, and witnessed, or is explicitly listed as an active
exception with owner, reason, review date, and removal condition.

`Unsafe-review zero` does not mean no unsafe Rust exists. It means there are no
unreviewed unsafe gaps and no missing contract, guard, witness, owner, or review
date evidence in the unsafe-review receipt.

`Miri status` is separate evidence. A Miri pass may support an unsafe-review
entry, but unsafe-review closure MUST NOT claim UB-freedom or Rust soundness.
Skipped or missing Miri evidence MUST NOT be treated as an unsafe-review pass.

## Required receipt

The unsafe-review command MUST emit a machine-readable receipt with at least:

```json
{
  "schema_version": 1,
  "lane": "unsafe-review-closure",
  "status": "pass|fail|advisory",
  "unsafe_site_count": 0,
  "changed_unsafe_site_count": 0,
  "unsafe_contract_missing_count": 0,
  "local_guard_missing_count": 0,
  "witness_missing_count": 0,
  "owner_missing_count": 0,
  "expired_review_count": 0,
  "unreviewed_unsafe_gap_count": 0,
  "unsafe_review_closure_satisfied": false,
  "miri_status": "pass|fail|advisory|skipped|not_applicable"
}
```

The receipt MUST distinguish these statuses:

- `pass`: the surface produced the required evidence.
- `fail`: the surface is required and not satisfied.
- `advisory`: the surface reported information that is not currently a hard
  gate.
- `skipped`: no evidence was produced, and the skip is not equivalent to pass.
- `not_applicable`: the surface is intentionally out of scope for this receipt.

## Unsafe-review requirements

- The denominator is tracked Rust unsafe keyword sites outside comments and
  string literals.
- Changed unsafe sites MUST be counted separately from repo-wide unsafe sites.
- Missing local contracts, local guards, and witnesses MUST be counted.
- Missing owners and expired reviews MUST be counted.
- Unsafe sites without a matching active ledger entry MUST increment
  `unreviewed_unsafe_gap_count`.
- `unsafe_review_closure_satisfied` MUST be false while any missing evidence,
  expired review, or unreviewed unsafe gap remains.

## Exception ledger

`policy/unsafe-review-exceptions.toml` MUST require each active exception to
include:

```toml
id = "stable-id"
owner = "team-or-area"
path = "path/module/crate"
kind = "ffi_raw_pointer_or_other_surface"
reason = "why this unsafe surface is not closure-satisfied"
test_surface = ["command or workflow that keeps it visible"]
review_after = "YYYY-MM-DD"
removal_condition = "what removes or narrows this exception"
safety_contract = "present|missing|not_applicable"
local_guard = "present|missing|not_applicable"
witness = "present|missing|not_applicable"
```

Exceptions may be active or retired. Empty owner, path, reason, test surface,
review date, or removal condition is invalid.

## Miri boundary

`miri_status` MAY be `skipped` or `advisory` in this scaffold. A later PR may
promote Miri to a required gate for supported crates or targets. Until then, the
receipt MUST NOT claim Miri-clean status unless Miri actually ran and passed.

## Follow-up priority

The receipt or plan MUST route follow-up work in this order:

1. changed unsafe seams;
2. FFI, raw pointer, and transmute-like seams;
3. shared-memory, concurrency, and RT boundaries;
4. HID, USB, and driver-facing unsafe boundaries;
5. generated or platform-specific unsafe surfaces;
6. Miri, property, and fake-transport witnesses.

## Non-goals

- No hardware work.
- No live capture.
- No broad refactor.
- No forced unsafe removal.
- No claim that tests prove unsafe Rust soundness.
- No UB-free claim.
- No Miri-clean claim without passing Miri evidence.

## Proof

The scaffold proof is:

```bash
cargo xtask unsafe-review-closure --check
```

Full validation PRs may add focused crate tests, Miri, properties, or fake
transports as they close specific unsafe-review gaps.
