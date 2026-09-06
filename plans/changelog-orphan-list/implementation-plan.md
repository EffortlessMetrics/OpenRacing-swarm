# Reject orphan changelog list items

Status: candidate
Owner: release/changelog
Issue: #318
Proposal/spec/ADR/active goal: n/a — focused parser correctness fix

## Goal

Prevent `ChangelogEntry::from_markdown` from returning `Ok` after silently
dropping a list item that appears before any `###` section heading.

## Production delta

Track the source line while parsing entry lines. If a `- ` list item is
encountered with no current section, return `ChangelogError::InvalidFormat`
with a stable line number and a short orphan-item diagnostic. Do not echo the
arbitrary item contents.

Preserve the existing compatibility behavior for unknown `###` section names:
they establish a section context but their ordinary items remain ignored.
This work does not impose section ordering or whole-file changelog policy.

## Acceptance

- An orphan item immediately after the version header returns `InvalidFormat`
  and identifies line 2.
- Blank lines before an orphan item are counted accurately.
- A list item immediately after a recognized section still parses normally.
- Existing semantic-version metadata, breaking markers, unknown sections,
  Deprecated, and Security behavior is unchanged.
- New tests use neither `unwrap()` nor `expect()`.
- Focused tests, Clippy, policy, diff hygiene, and the normalized required Rust
  result pass before integration. #302 may not be bypassed.

## Proof

```text
python scripts/cargo_fmt_workspace.py --check
cargo test --locked -p openracing-changelog
cargo clippy --locked -p openracing-changelog --all-targets --all-features -- -D warnings
python scripts/policy_file.py --strict
git diff --check
OpenRacing Rust Small Result
```

## Rollback

Revert the orphan-item error and its focused regressions together. No persisted
data migration is required.
