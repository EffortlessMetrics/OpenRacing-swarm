# Pull Request Template

## Summary
Brief description of changes and motivation.


## Source-of-Truth Links

Proposal: n/a
Spec: n/a
ADR: n/a
Plan item: n/a
Active goal: n/a

## Scope

- [ ] Proposal / why
- [ ] Spec / behavior contract
- [ ] ADR / durable decision
- [ ] Plan / sequencing
- [ ] Active goal / current execution state
- [ ] Runtime / implementation
- [ ] Policy ledger
- [ ] Support-tier update
- [ ] Generated status / receipt

## Claim Boundary

What may be claimed after this PR? What may not be claimed yet?

## Rollback

How can this PR be reverted or disabled safely?

## Migration Notes
**Required for all PRs that modify public APIs, schemas, or cross-crate interfaces**

### Call-site Actions Per Crate
- [ ] **wheelctl**: What changes are needed in CLI code?
- [ ] **racing-wheel-service**: What changes are needed in service layer?
- [ ] **racing-wheel-plugins**: What changes are needed in plugin system?
- [ ] **racing-wheel-integration-tests**: What changes are needed in tests?
- [ ] **Other crates**: List any additional affected crates and required changes

### Breaking Changes
- [ ] No breaking changes to public APIs
- [ ] Breaking changes documented with migration guide
- [ ] Field name updates or removals documented
- [ ] Type signature changes documented

## Schema/API Versioning
**Required for changes to schemas, protobuf definitions, or public APIs**

- [ ] **Protobuf package version**: No changes needed / Bumped `wheel.v1` to `wheel.v2` (reason: ___)
- [ ] **JSON schema version**: No changes needed / Updated version (reason: ___)
- [ ] **API compatibility**: Backward compatibility maintained / Breaking change with migration path
- [ ] **Deprecation window**: N/A / Deprecated items will be removed in version ___

### Schema Change Justification
If bumping protobuf package or JSON schema version, explain:
- What necessitated the breaking change?
- What alternatives were considered?
- How will existing clients migrate?

## Compat Debt Delta
**Required for all PRs - helps track technical debt trends**

- [ ] **Compat usage count delta**: +/- ___ usages (run `scripts/track_compat_usage.py` to measure)
- [ ] **Compat debt trending**: ⬇️ Down / ➡️ Stable / ⬆️ Up (explain if increasing)
- [ ] **Removal planning**: N/A / Created issue #___ for removing compat shims in next minor version

### Compat Layer Impact
- [ ] No compat layer usage changes
- [ ] Reduced compat layer usage (good!)
- [ ] Increased compat layer usage (justify why necessary)

## CI Verification Checklist
**All items must pass before merge**

### Compilation Verification
- [ ] **Workspace build**: `cargo build --workspace` passes
- [ ] **All features**: `cargo build --workspace --all-features` passes  
- [ ] **No default features**: `cargo build --workspace --no-default-features` passes

### Isolation Builds (Critical Path)
- [ ] **CLI isolation**: `cargo build -p wheelctl` passes ([CI link](___))
- [ ] **Service isolation**: `cargo build -p racing-wheel-service` passes ([CI link](___))
- [ ] **Plugins isolation**: `cargo build -p racing-wheel-plugins` passes ([CI link](___))

### Cross-Platform Verification
- [ ] **Linux build**: All targets compile on ubuntu-latest ([CI link](___))
- [ ] **Windows build**: All targets compile on windows-latest ([CI link](___))

### Schema Validation
- [ ] **Protobuf breaking check**: `buf breaking --against main` passes ([CI link](___))
- [ ] **JSON schema round-trip**: Schema serialization tests pass ([CI link](___))
- [ ] **Trybuild guards**: Compile-fail tests prevent regression ([CI link](___))

### Dependency Governance
- [ ] **No version conflicts**: `cargo tree --duplicates` shows no duplicates
- [ ] **Feature unification**: `cargo hakari generate` produces no diff
- [ ] **Minimal versions**: `cargo +nightly -Z minimal-versions build` passes
- [ ] **Unused dependencies**: `cargo udeps` shows no unused deps

### Lint Gates
- [ ] **Warnings**: `RUSTFLAGS="-D warnings" cargo clippy --workspace` passes
- [ ] **Format**: `cargo fmt --check` passes
- [ ] **API guards**: No glob re-exports (`rg 'pub use .*::\*;' crates/` empty)
- [ ] **Deprecated tokens**: No removed field names in codebase

## Testing
- [ ] **Unit tests**: New/modified code has appropriate test coverage
- [ ] **Integration tests**: Cross-crate functionality tested
- [ ] **Trybuild tests**: Added compile-fail guards for API changes
- [ ] **Manual testing**: Verified functionality works as expected

## Documentation
- [ ] **API docs**: Public APIs documented with examples
- [ ] **Migration guide**: Breaking changes have migration instructions
- [ ] **ADR**: Architectural decisions recorded if applicable
- [ ] **Changelog**: User-facing changes documented

## Security & Performance
- [ ] **Security review**: No new security vulnerabilities introduced
- [ ] **Performance impact**: No significant performance regressions
- [ ] **Resource usage**: Memory/CPU usage remains acceptable

---

## Reviewer Guidelines

### For Schema/API Changes
1. Verify migration notes are complete and actionable
2. Check that breaking changes follow deprecation policy
3. Ensure protobuf/JSON schema versions are bumped appropriately
4. Validate that compat debt is trending downward

### For Cross-Crate Changes  
1. Verify all isolation builds pass independently
2. Check that public API usage follows prelude patterns
3. Ensure no private module imports across crate boundaries
4. Validate async trait patterns are consistent

### For CI/Build Changes
1. Verify all build matrix combinations are tested
2. Check that lint gates catch the intended violations
3. Ensure dependency governance rules are enforced
4. Validate that trybuild guards prevent regression

**Merge Criteria**: All checkboxes must be checked and CI must be green before merge.