# Unsafe-review closure

This lane makes unsafe Rust reviewability measurable before the repo claims
unsafe Rust soundness, UB-freedom, Miri-clean status, or unsafe removal.

Source of truth:

- Proposal: `docs/proposals/OR-PROP-0003-unsafe-review-closure-lane.md`
- Spec: `docs/specs/OR-SPEC-0004-unsafe-review-closure.md`
- Plan: `plans/unsafe-review-closure/implementation-plan.md`
- Exception ledger: `policy/unsafe-review-exceptions.toml`
- Receipt command: `cargo xtask unsafe-review-closure --check`
