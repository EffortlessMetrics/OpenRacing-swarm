# OpenRacing Proposals

Proposals are the **why** layer of OpenRacing's source-of-truth stack. They explain why a lane exists before specs, ADRs, implementation plans, or runtime changes claim how the work will land.

## Role in the stack

```text
Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof
```

A proposal owns:

- user pain, repo risk, or product gap;
- affected users and surfaces;
- success criteria;
- alternatives rejected;
- risks and non-goals;
- the specs, ADRs, plans, support-tier updates, and policy ledgers the lane needs.

A proposal must not own:

- detailed PR order;
- implementation details;
- generated status;
- proof receipt state.

## Naming

Use stable, boring IDs:

```text
docs/proposals/OR-PROP-0001-<lane>.md
```

Use the next available number and a short kebab-case lane name.

## Template

```md
# OR-PROP-0001: Lane title

Status: proposed
Owner: n/a
Created: YYYY-MM-DD
Target milestone: n/a
Linked specs: n/a
Linked ADRs: n/a
Linked plan: n/a
Support/status impact: n/a
Policy impact: n/a

## Problem

What user pain, repo risk, or product gap exists?

## Users and surfaces

Who benefits? Which commands, APIs, packages, workflows, docs, or hardware surfaces are affected?

## Success criteria

What must be true when this lane is complete?

## Proposed shape

What are we doing at a product or repository level?

## Alternatives considered

What did we reject and why?

## Specs to create or update

- OR-SPEC-...

## ADRs needed

- OR-ADR-...

## Implementation campaign shape

High-level PR phases only. Keep detailed sequencing in `plans/<lane>/implementation-plan.md`.

## Evidence plan

Proof commands, fixtures, support-tier updates, CI lanes, and receipts.

## Risks

What can go wrong?

## Non-goals

What is explicitly out of scope?

## Exit criteria

When is this proposal done?

## Claim boundary

What this proposal does not claim.
```
