# OpenRacing Specs

This directory is "how it should work" documentation: concrete enough to review against code, and strict enough to write tests against.

## Specs

- Telemetry integrations: `telemetry.md`
- Safety-critical FFB control loop: `ffb-safety.md`

## Conventions

- **MUST / SHOULD / MAY** language is intentional.
- Specs link to **implementation touchpoints** in `crates/...` so reviewers can trace behavior.
- Where vendors do not publish docs publicly, the spec points to the **authoritative shipped header/config** on a developer machine, and calls out any assumptions.

## Role in the source-of-truth stack

Specs are the **what must be true** layer:

```text
Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof
```

A spec owns required behavior, non-goals, acceptance examples, proof requirements, test mapping, implementation mapping, CI proof, and support-tier impact. A spec must not own product rationale, PR order, active queue state, or durable architecture decisions unless the decision cannot be separated into an ADR.

New source-of-truth specs should use stable IDs such as `OR-SPEC-0001-<contract>.md` and should link the proposal, ADRs, plan, issues, PRs, support-tier impact, and policy impact. Existing legacy specs can be normalized as they are touched.
