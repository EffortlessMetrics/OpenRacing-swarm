# Sequential shifter overflow correction

Status: candidate
Owner: input/shifter
Linked proposal: n/a — arithmetic correctness repair with no product-scope decision
Linked specs: n/a — preserves the existing sequential shift and gear-clamp contract
Linked ADRs: n/a — no durable architectural decision
Active goal: n/a — focused backlog correction while the prior service lane is being closed
Linked issue/PR: EffortlessMetrics/OpenRacing#513; EffortlessMetrics/OpenRacing-swarm#310

## Work item: saturate-before-clamp

Status: candidate

### Goal

Make sequential upshift and downshift arithmetic total for every `i32` input
without changing the existing supported gear range or ordinary shift behavior.

### Production delta

Evaluate the one-step direction change with `saturating_add(1)` or
`saturating_sub(1)` before applying the existing directional bounds: upshift is
capped at `MAX_GEARS`, and downshift is floored at gear `1`. This removes the
debug panic and release wraparound at the integer extremes while retaining the
same result for every input that does not overflow.

### Acceptance

- An upshift from `i32::MAX` returns `MAX_GEARS` instead of panicking or
  wrapping.
- A downshift from `i32::MIN` returns gear `1` instead of panicking or wrapping.
- Existing ordinary upshift, downshift, and no-shift behavior remains covered
  by the crate's current tests.
- Formatting, focused tests, Clippy with warnings denied, file policy, and the
  normalized required Rust result pass on the candidate revision.

### Proof commands

```text
python scripts/cargo_fmt_workspace.py --check
cargo test --locked -p openracing-shifter
cargo clippy --locked -p openracing-shifter --all-targets --all-features -- -D warnings
python scripts/policy_file.py --strict
git diff --check
OpenRacing Rust Small Result
```

### Non-goals

- No parser, calibration, device, HID, persistence, API, schema, FFB, or
  support-tier changes.
- No expansion of the supported gear range.
- No import of the predecessor PR's unrelated trait, formatting, or default
  value characterization tests.

### Rollback

Revert the saturating arithmetic and focused boundary regression together. No
data or configuration migration is required.
