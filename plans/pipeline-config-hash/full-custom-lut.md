# Full custom LUT hashing

Status: candidate
Owner: pipeline/configuration
Issue: #322
PR: #331
Proposal: n/a — correctness repair to the existing change-detection contract
Spec: n/a — preserves the existing public API and pipeline semantics
ADR: n/a — no durable architecture change
Active goal: n/a — focused backlog correction

## Work item: represent-every-custom-lut-entry

### Goal

Remove the deterministic equivalence class created by hashing only five entries
of a 256-entry custom response curve. A custom LUT that changes at any accepted
entry must present different input to the final `u64` hasher.

### Production delta

- Keep the existing `CurveType` variant discriminator.
- Add an explicit custom-LUT domain marker and fixed table length before content.
- Hash `to_bits()` for every one of the 256 raw LUT entries, in table order.
- Keep hashing off the 1 kHz execution path; the compiler calculates the hash
  while producing a pipeline.
- Treat floating-point values bitwise, matching the existing hash treatment for
  filter and curve fields: `-0.0` and `0.0` are distinct bit patterns. NaN
  payloads are likewise distinct if they reach this function; issue #312 owns
  rejecting non-finite LUT entries at the accepted-value boundary.

### Compatibility audit

Repository search shows the value is used as an in-memory pipeline/configuration
fingerprint, debug/status field, and swap/change-detection input. The snapshot
and compiled-pipeline types carrying it are not serialized. No persisted hash
format or cross-release comparison contract was found. Public hash functions
remain public but their documentation must not promise cryptographic uniqueness
or cross-version stability.

### Acceptance

- Two finite 256-entry custom LUTs that agree at indices 0, 64, 128, 192 and
  255 but differ at an unsampled interior entry produce different hashes.
- Identical custom LUT bit patterns remain deterministic.
- Custom LUT hashing includes variant, domain, length and every table entry.
- No response-curve evaluation or RT-path code changes.
- Focused tests, Clippy, policy and the normalized routed result pass before
  integration; the routed result may not bypass #302.

### Proof

```text
python scripts/cargo_fmt_workspace.py --check
cargo test --locked -p openracing-pipeline hash
cargo test --locked -p openracing-pipeline --test hash_sensitivity_contract
cargo clippy --locked -p openracing-pipeline --all-targets --all-features -- -D warnings
python scripts/policy_file.py --strict
git diff --check
OpenRacing Rust Small Result
```

### Rollback

Revert the full-table hash and its regression together. No data migration is
required because no persisted hash contract was found.
