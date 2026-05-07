# No-panic policy

OpenRacing's RT, safety, and HID surfaces are panic-sensitive: an unwrapped
`Option` on the 1 kHz hot path, in a safety interlock, or in an HID
parser is a real defect, not a stylistic blemish. This policy defines what
"panic-free" means for OpenRacing and how exceptions are tracked.

## Definition

> **No unreceipted panic-family behavior in production or tests.**

Panic-family covers:

* `unwrap`
* `expect`
* `panic!`
* `todo!`
* `unimplemented!`
* `unreachable!`
* `[]` indexing on slices/vectors that can panic
* `&s[a..b]` string slicing on byte boundaries that can panic
* `Option::get(...).unwrap()` (sometimes hidden behind helpers)
* time subtraction that can panic
* `unwrap` calls inside `Result`-returning functions

The definition explicitly **excludes** assertion macros (`assert!`,
`assert_eq!`, `assert_ne!`, `debug_assert!`, …). They are test oracles
and are out of scope for the v1 panic-free rollout. A later v2 may
introduce fallible assertion helpers (`ensure_eq!`, `require_some!`,
…) for tests that return `Result<()>`.

## Two-rail design

* **Rail A — Clippy.** `[workspace.lints]` in root `Cargo.toml` carries
  the relevant Clippy lints. During Stage A they are `warn`; they are
  promoted to `deny` after the semantic checker reports zero
  unreceipted findings.
* **Rail B — semantic checker.** `scripts/policy_no_panic.py`
  walks the workspace, enumerates findings, and matches them against
  `policy/no-panic-allowlist.toml`. This is the **authoritative** source
  for intentional exceptions.

Clippy gives fast IDE/CI feedback. The semantic checker carries
exception metadata (owner, reason, classification, expiry, selector)
that Clippy alone cannot represent.

## Allowlist schema

`policy/no-panic-allowlist.toml` uses schema `0.3`:

```toml
[[allow]]
id = "panic-0001"
path = "crates/openracing-curves/src/lut.rs"
family = "indexing"
classification = "static_invariant"
owner = "ffb"
explanation = "LUT index is masked to LUT_SIZE-1 at the call site."
expires = "2026-12-01"

[allow.selector]
kind = "index_expr"
container = "fn lookup"
callee = "[]"
receiver_fingerprint = "self.table"

[allow.last_seen]
line = 78
column = 19
```

Identity for matching is `path + family + selector`. The `last_seen`
line and column are advisory only — they help reviewers locate the
finding but are never used to match an entry, so unrelated edits do
not break the allowlist.

Recognized `family` values are listed in the allowlist file itself.
Recognized `classification` values:

* `test_helper` — only reachable from tests / fixtures / dev tools.
* `static_invariant` — provably unreachable given enclosing types.
* `boundary` — pre-checked at a boundary above this call.
* `rt_fast_path` — RT path; receipted by code review.
* `legacy_debt` — debt to migrate to fallible flow.
* `external_api` — forced by an upstream API surface.
* `build_or_proc_macro` — codegen / build.rs path.

Every entry must have an `owner` and either an explicit `expires` ISO
date or rely on the file-level `default_expires`. Stale entries (no
matching finding seen in the most recent run) and expired entries are
both reported by `policy_no_panic.py` and become blocking after
Stage B.

## Stages

| Stage | Clippy panic lints | Semantic checker | Allowlist required |
|---|---|---|---|
| A (now) | `warn` | non-blocking, reports drift | optional |
| B | `warn` | blocking | required |
| C | `deny` | blocking | required + `#[expect(...)]` in source |

## Workflow

1. Run `python scripts/policy_no_panic.py` locally. It writes:
   * `target/no-panic-report.json`
   * `target/no-panic-report.md`
2. To regenerate proposed entries from current findings:
   ```
   python scripts/policy_no_panic.py --propose
   ```
   This writes `target/no-panic-proposed-allowlist.toml`. The script
   never mutates `policy/no-panic-allowlist.toml` automatically; you
   review proposed entries, add owner/reason/expiry, and move them in.
3. CI runs the same script. During Stage A it is non-blocking; during
   Stage B it fails on unreceipted findings.

## Why a separate semantic checker

Clippy's `unwrap_used` / `expect_used` / `indexing_slicing` lints are
useful but they cannot:

* identify a finding stably across edits (they key by source location);
* carry an owner / reason / classification / expiry per exception;
* report stale entries when an exception is no longer needed;
* report drift when a finding moves within the same file.

The semantic checker fills exactly that gap. It is intentionally narrow
in scope (it does not try to replace Clippy) and runs in seconds against
the whole workspace.
