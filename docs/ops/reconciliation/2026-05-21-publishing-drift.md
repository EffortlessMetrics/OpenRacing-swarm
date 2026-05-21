# 2026-05-21 Publishing Drift Reconciliation

This record captures the distilled repository-routing facts from the publishing
drift incident. It is an operations reconciliation note, not a hardware
readiness claim and not a substitute for Moza evidence receipts.

## Current routing rule

```text
development/integration repo = EffortlessMetrics/OpenRacing-swarm
publishing repo              = EffortlessMetrics/OpenRacing
direct feature PRs to publishing repo are frozen
```

`OpenRacing-swarm` is the development and integration rail. `OpenRacing` is the
publishing/output rail. Reconciliation evidence, architecture cleanup, and
future development work must land in `OpenRacing-swarm` first.

## Second transcript reconciliation addendum

A second transcript paste was reviewed as incident evidence. It confirms:

- `OpenRacing-swarm` PR #8 merged the recovery import after the routing
  inversion.
- `OpenRacing-swarm` PR #10 merged a post-recovery no-output Moza slice,
  proving development resumed in swarm.
- Several Moza slices had previously been opened or merged directly in
  `OpenRacing`; these are historical misroutes and must not define future
  routing.
- Repeated blocked-state messages around passive Pit House sniffing are
  retained locally as raw transcript evidence only.

Verified recovery facts:

| Fact | Repository | PR | Merge commit |
| --- | --- | ---: | --- |
| Recovery import merged | `EffortlessMetrics/OpenRacing-swarm` | 8 | `fbe7ef9b889deba9d87b95effa9acd0c867cd07c` |
| Post-recovery no-output Moza slice merged | `EffortlessMetrics/OpenRacing-swarm` | 10 | `c3a7f43fb14be57f5249588e56b1d56714dee308` |

Historical publishing-repo misroute examples retained as incident context:

```text
EffortlessMetrics/OpenRacing#636
EffortlessMetrics/OpenRacing#648
EffortlessMetrics/OpenRacing#657
EffortlessMetrics/OpenRacing#658
```

These examples do not change the current rule: direct feature PRs to the
publishing repo are frozen.

## Local raw evidence

Raw transcript retained locally:

```text
H:\Code\Rust\_openracing-reconciliation\2026-05-21-publishing-drift\raw\pasted-after-recovery-02.txt
```

SHA256:

```text
33CF4C5F9449A60423569C06D7DCF5E2E898C4B33D1604BECAD6A9D9746B99D7
```

The transcript is not source-of-truth. It contains useful incident evidence
plus noisy tool output, repeated blocked-state messages, local paths, and
historical PR logs. Only the distilled facts above are committed here.

## Out-of-scope architecture material

The second transcript also contained useful architecture material about
shrinking public crate surface into SRP submodules. That material is not part
of this routing incident record. Recreate or rebase it through
`OpenRacing-swarm` as a separate architecture PR before it becomes source of
truth.
