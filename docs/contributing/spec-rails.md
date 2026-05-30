# Contributing: repo-native spec rails

Status: draft
Owner: hardware
Created: 2026-05-21

When contributing source-of-truth artifacts:

1. Put durable proposal/spec/ADR/lane/closeout material in `/.openracing-spec/`.
2. Keep `/docs/` focused on explanation and contributor guidance.
3. Keep live policy enforcement data in `/policy/*.toml` and reference it from durable rails.
4. Do not place durable rails in `.codex/`, `.spec/`, `.claude/`, or `.jules/`.

This maintains tool-neutral, repo-owned long-term memory while allowing agent tools to coexist.
