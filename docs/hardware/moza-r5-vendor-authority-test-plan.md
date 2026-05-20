# Moza R5 vendor authority test implementation plan

Status: proposed
Owner: hardware
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Active goal: .openracing/goals/active.toml

## Purpose

Provide a step-by-step implementation queue for Moza R5 vendor authority infrastructure, starting with docs/schema/fixture scaffolding and deferring hardware writes to exact-authorization stages.

## PR sequence

1. **Spec + schemas only**
   - Add OR-SPEC-0002.
   - Add five schemas for registry/status-plan/authorization/smoke receipts.
   - Add fixture skeleton for command registry.
   - No runtime implementation.

2. **Command registry + risk policy tests**
3. **Semantic-only serial codec + fixtures**
4. **Fake serial transport**
5. **No-output CLI tools**
6. **Read-only vendor status probe**
7. **Exact authorization support**
8. **Vendor authority smoke dry-run**
9. **First bounded hardware authority attempt**
10. **Post-authority PIDFF response comparison**
11+. **Closed-loop motion ladder**

## Required gating invariant

No stage may claim native-visible readiness without strict verifier acceptance of real movement evidence.

## Proof commands (PR1)

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```
