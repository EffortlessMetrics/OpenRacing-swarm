# Unsafe-review policy

Unsafe-review is OpenRacing's unsafe-contract reviewability lane. It exists
because unsafe Rust is not only a Clippy or source-allowlist problem: unsafe
seams need contracts, guards, test reach, and witness routes that reviewers can
inspect.

## Tool-role split

| Tool or lane | Question |
|---|---|
| Source exception policy | Is this unsafe/source exception allowed, owned, and time-boxed? |
| unsafe-review | Is this unsafe seam reviewable: contract, guard, test reach, witness route? |
| Miri/sanitizers/tests | Did a concrete execution expose UB or memory misuse for the scenario that ran? |

## What unsafe-review answers

Unsafe-review asks whether changed or retained unsafe seams have reviewable
evidence:

- a local `SAFETY:` contract or equivalent documentation;
- a bounded unsafe block rather than an oversized unsafe region;
- a guard that checks the preconditions before the unsafe operation;
- a test, fake transport, verifier, Miri run, or other witness route;
- an owner and follow-up path for missing evidence.

## What unsafe-review does not answer

Unsafe-review does not prove that unsafe Rust is sound, memory-safe, UB-free, or
Miri-clean. Those are execution or formal-proof claims and require matching
witness receipts.

Missing Miri evidence may be `skipped` or `advisory`; it must not be reported as
a pass.

## PR usage

For unsafe seam changes, run the repo wrapper when available:

```bash
cargo xtask unsafe-review-closure --check
```

If the project later adds a PR-scoped unsafe-review wrapper, changed unsafe
surfaces should use that lane first and reserve broader closure checks for main,
nightly, manual, or release workflows.

## Receipt expectations

Unsafe-review receipts should identify:

- changed unsafe seams;
- missing or present safety contracts;
- witness routes and whether they actually ran;
- advisory, skipped, failed, and not-applicable states;
- owned exceptions with owner, reason, review date, and removal condition.

The claim boundary must stay narrow: reviewability evidence supports review. It
is not a blanket safety claim.
