# Source-of-truth stack and namespace boundaries

Status: draft
Owner: hardware
Created: 2026-05-21

OpenRacing uses a linked source-of-truth stack:

Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof.

The durable repo-owned rails live in `/.openracing-spec/`.

Tool-specific directories like `.codex/`, `.spec/`, `.claude/`, and `.jules/` may exist for agent/session workflows, but they are awareness-only for this lane.
