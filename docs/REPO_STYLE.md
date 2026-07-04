# Repo style

OpenRacing is operated as an evidence machine: strict defaults, owned
exceptions, static signal first, runtime proof where it pays, receipts
everywhere, and one review-fast PR at a time.

Rust and repo-local `xtask` commands are the default construction material.
Non-Rust files, unsafe seams, panic paths, lint suppressions, generated files,
workflow behavior, process/network access, expensive CI lanes, and release
claims must be owned and receipted through the source-of-truth stack or the
matching policy ledger.

## Evidence order

Static evidence runs first because it is cheap, deterministic, and reviewable:

- `cargo-allow` or the repo's allowlist-policy checker for source exceptions;
- RIPR for static mutation-exposure and weak-oracle review signals;
- unsafe-review or the repo unsafe-review closure receipt for unsafe-contract
  reviewability;
- rustc and Clippy for code-shape policy.

Runtime evidence runs where it pays:

- focused tests on ordinary PRs;
- targeted mutation, Miri, fuzzing, and coverage for risk PRs;
- broader mutation, Miri, fuzzing, coverage, and release-readiness lanes on
  main, nightly, manual, or release workflows.

Coverage, mutation, and Miri are backstops. They do not replace ownership of
source exceptions, unsafe contracts, or weak-oracle findings.

## Tool-role split

| Tool or lane | Repo role | Claim boundary |
|---|---|---|
| `cargo-allow` / file-policy ledgers | Durable source-exception ownership | Records that an exception is allowed, owned, and reviewable; it does not prove behavior correct. |
| RIPR | Static mutation-exposure analysis | Surfaces weak test/oracle exposure early; it does not run mutants or replace runtime mutation testing. |
| unsafe-review | Unsafe-contract reviewability | Checks for reviewable contracts, guards, test reach, and witness routes; it does not prove UB-free execution. |
| `xtask` | Repo control plane | Wraps tools, emits receipts, and enforces repo glue; it should not reimplement upstream analyzers. |
| cargo-mutants, Miri, fuzzing, coverage | Runtime and execution backstops | Provide concrete execution evidence only for the scenarios that actually ran. |
| Codecov | Execution-surface telemetry | Reports coverage status; informational coverage is not release readiness. |

## Review-fast PRs

Agents and humans work one review-fast PR at a time. Review-fast does not mean
small for its own sake. It means the seam is coherent, proof is nearby,
verification is efficient, and claim boundaries are honest.

A review-fast PR:

1. implements one plan work item or one explicitly requested scaffold slice;
2. keeps docs-only artifact changes separate from runtime/code changes unless a
   linked plan says otherwise;
3. adds no invisible source exceptions, broad suppressions, or unowned policy
   debt;
4. does not broaden public support claims without support-tier proof or an
   equivalent receipt pointer;
5. runs the proof commands named by the plan, or records why a command is only
   discovery/advisory evidence.

Do not broaden scope merely to satisfy CI. Do not add shell, Python, or
JavaScript repo automation when Rust/`xtask` is the durable home. If non-Rust
must remain, it needs an owner, reason, coverage surface, and review date.

## CI economics

OpenRacing optimizes for proof per Linux-equivalent minute (LEM), not fewer
checks. Default PR lanes should be cheap, deterministic, and high-signal. Deep
validation is preserved, but routed by changed surface, risk pack, label, main,
nightly, manual dispatch, or release lane.

A skipped optional lane is not a pass. It is a policy decision that must remain
visible in the PR summary, CI receipt, or release receipt.
