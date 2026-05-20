# Microcrate to SRP Submodule Transition

Status: draft
Owner: architecture
Created: 2026-05-19
Linked proposal: n/a
Linked ADRs: n/a
Linked plan: n/a

## Operating doctrine

This document is an execution addendum to `docs/architecture/crate-surface.md`.
The canonical package-surface policy remains `policy/crate-boundaries.toml`
plus `[workspace.metadata.publish].allow` in `Cargo.toml`, as enforced by
`cargo run --locked -p openracing-tools --bin package-surface -- --check`.

OpenRacing should **not** become less modular. It should become less published, less package-fragmented, and more owner-centered.

- Public crates = support contracts.
- Workspace packages = ownership/release boundaries.
- SRP submodules = architecture boundaries.

This transition is not `microcrates -> blob crate`; it is `microcrates -> owner packages -> SRP module families with façade boundaries`.

## Transition states

Every current microcrate is assigned one state:

`unclassified -> inventoried -> frozen -> owner_assigned -> target_module_defined -> moved_or_shimmed -> verified -> retired_or_public_confirmed`

## Classification taxonomy

Each package is classified as one of:

- public_contract
- forced_crate
- shared_contract
- single_owner_internal
- vendor_adapter_internal
- runtime_internal
- dev_tooling
- compat_shim
- delete

## Architecture rails

1. Public-surface rail: only allowlisted support contracts can be publishable.
2. No behavior-change-in-move rail: topology changes only in collapse PRs.
3. Owner rail: each seam has exactly one owner.
4. Façade rail: module families expose one façade and avoid deep imports.
5. Dependency-direction rail: preserve strict layering.
6. Feature rail: features describe user capabilities, not historical crate names.
7. Compatibility rail: shims only for plausible external users.
8. Hardware evidence rail: topology PRs do not change readiness claims.
9. Workspace default-members rail: avoid accidental root-wide operations on transitional packages.

## Policy ledgers for this transition

- `policy/crate-boundaries.toml` for the enforced package-surface rail.
- `policy/seam-transition.toml`
- `policy/module-boundaries.toml`
- `policy/compat-shims.toml`

The transition ledgers are draft scaffolds until a follow-up policy checker
enforces their schema and cross-file consistency.

## Initial PR sequence

1. Encode doctrine and policy skeletons (docs + policy only).
2. Inventory all workspace package seams.
3. Freeze non-public packages (`publish = false` unless allowlisted).
4. Add seam/module/feature policy checkers.
5. Execute owner-family collapses in bounded PRs.

## Completion criteria (summary)

- Every package classified in seam-transition.
- Every public package allowlisted in `policy/crate-boundaries.toml` and
  `[workspace.metadata.publish].allow`.
- Non-public packages frozen with `publish = false`.
- Each collapsed seam has one owner + one façade.
- Policy checkers pass in CI.
