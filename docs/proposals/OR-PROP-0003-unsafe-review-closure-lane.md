# OR-PROP-0003: Unsafe-review closure lane

Status: proposed
Owner: release/ci
Created: 2026-05-31
Target milestone: n/a
Linked specs: docs/specs/OR-SPEC-0004-unsafe-review-closure.md
Linked ADRs: n/a
Linked plan: plans/unsafe-review-closure/implementation-plan.md
Support/status impact: no public support claim
Policy impact: policy/unsafe-review-exceptions.toml

## Problem

Unsafe Rust needs a separate quality rail from RIPR+ and coverage. Coverage can
show that code executed, and RIPR can show thin evidence around changed code, but
neither proves that every unsafe seam has a local contract, local guard, and
witness.

The project should make unsafe reviewability measurable before claiming unsafe
Rust soundness, UB-freedom, or Miri-clean status. The first useful target is not
removing every unsafe block. It is defining the denominator and making missing
review evidence explicit.

## Users and surfaces

This lane is for maintainers and PR reviewers. It covers:

- tracked Rust unsafe keyword sites;
- changed unsafe sites in the PR diff;
- local safety contract coverage;
- local guard and invariant coverage;
- witness coverage through tests, fake transports, receipts, properties, or
  verifiers;
- owner and review-date coverage;
- Miri status as separate evidence, not as the unsafe-review result.

## Success criteria

- Unsafe-review closure has a repo-owned definition.
- The receipt reports missing contracts, guards, witnesses, owners, expired
  reviews, and unreviewed unsafe sites.
- Miri status is visible but not conflated with unsafe-review closure.
- Missing unsafe-review evidence is not treated as a pass.
- Every initial unsafe exception is explicit, owned, reviewable, and removable.

## Proposed shape

Add a source-of-truth spec, implementation plan, machine-readable exception
ledger, and `cargo xtask unsafe-review-closure` receipt generator. The command
scans tracked Rust source for unsafe keyword sites and validates the ledger
without running hardware, opening devices, or mutating runtime state.

## Alternatives considered

- Claim unsafe Rust is safe because tests pass. Rejected because execution is
  not a soundness proof.
- Require zero unsafe immediately. Rejected because legitimate FFI, RT,
  shared-memory, and platform seams exist.
- Treat Miri as the unsafe-review gate. Rejected because Miri is useful evidence
  for supported targets but does not replace local contracts or review.

## Specs to create or update

- `docs/specs/OR-SPEC-0004-unsafe-review-closure.md`

## ADRs needed

n/a

## Implementation campaign shape

1. Define unsafe-review closure and emit a receipt.
2. Close changed unsafe seams first.
3. Add contracts and witnesses for FFI, raw pointer, and transmute-like seams.
4. Add fake-memory and fake-transport witnesses for shared-memory, HID, USB, and
   driver-facing unsafe boundaries.
5. Promote Miri or property checks only after they are cheap and supported.

## Evidence plan

The first PR proves only the measurement rail:

```bash
cargo xtask unsafe-review-closure --check
```

Later PRs can ratchet missing contract, guard, witness, owner, and expired
review counts toward zero.

## Risks

- Broad exception entries could hide real seams if they are not narrowed.
- Review dates could become stale unless CI and reviewers watch the receipt.
- Miri could be over-claimed as a proof of soundness.

## Non-goals

- No hardware work.
- No live capture.
- No broad refactor.
- No forced unsafe removal.
- No claim that unsafe Rust is sound.
- No UB-free or Miri-clean claim unless that evidence actually ran and passed.

## Exit criteria

The lane is complete when the unsafe-review receipt reports:

```text
unsafe_contract_missing_count = 0
local_guard_missing_count = 0
witness_missing_count = 0
owner_missing_count = 0
expired_review_count = 0
unreviewed_unsafe_gap_count = 0
unsafe_review_closure_satisfied = true
```

## Claim boundary

This proposal makes unsafe seams reviewable. It does not prove unsafe Rust
soundness, UB-freedom, Miri-clean status, release readiness, or hardware
readiness.
