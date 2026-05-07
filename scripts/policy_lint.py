#!/usr/bin/env python3
"""Validate that the workspace lint configuration matches policy.

Checks:

  1. Every workspace member crate Cargo.toml inherits
     ``[lints] workspace = true``.
  2. ``clippy.toml`` does not contain forbidden test carveouts
     (``allow-unwrap-in-tests`` and friends).
  3. The active workspace lint levels in root ``Cargo.toml`` are
     consistent with ``policy/clippy-lints.toml`` Stage A and the
     burndown ledger in ``policy/clippy-debt.toml``.

Stage A: this script runs and reports drift; non-blocking unless
``--strict`` is passed or a hard rule (test carveout, missing
inheritance) is violated.

CLI:

  python scripts/policy_lint.py
  python scripts/policy_lint.py --strict

Exit codes:

  0  policy holds
  1  hard violation (test carveout / missing inheritance / schema)
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ROOT_CARGO = REPO_ROOT / "Cargo.toml"
CLIPPY_TOML = REPO_ROOT / "clippy.toml"
POLICY_TOML = REPO_ROOT / "policy" / "clippy-lints.toml"
DEBT_TOML = REPO_ROOT / "policy" / "clippy-debt.toml"

FORBIDDEN_CLIPPY_KEYS = {
    "allow-unwrap-in-tests",
    "allow-expect-in-tests",
    "allow-panic-in-tests",
    "allow-indexing-slicing-in-tests",
    "allow-dbg-in-tests",
}


def load_toml(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def workspace_members() -> list[Path]:
    data = load_toml(ROOT_CARGO)
    members = data.get("workspace", {}).get("members", [])
    paths: list[Path] = []
    missing: list[str] = []
    for member in members:
        # cargo glob expansion is rare here; the workspace is explicit.
        manifest = REPO_ROOT / member / "Cargo.toml"
        if manifest.exists():
            paths.append(manifest)
        else:
            missing.append(member)
    if missing:
        sys.exit(
            "lint-policy: [workspace.members] references missing manifests: "
            + ", ".join(missing)
        )
    return paths


def crate_inherits_workspace_lints(manifest: Path) -> bool:
    data = load_toml(manifest)
    lints = data.get("lints", {})
    if isinstance(lints, dict) and lints.get("workspace") is True:
        return True
    return False


def check_inheritance() -> list[str]:
    errors: list[str] = []
    for manifest in workspace_members():
        if not crate_inherits_workspace_lints(manifest):
            errors.append(
                f"{manifest.relative_to(REPO_ROOT)}: missing "
                "[lints] workspace = true"
            )
    return errors


def check_clippy_toml_carveouts() -> list[str]:
    if not CLIPPY_TOML.exists():
        return []
    data = load_toml(CLIPPY_TOML)
    bad = sorted(set(data.keys()) & FORBIDDEN_CLIPPY_KEYS)
    if not bad:
        return []
    return [
        f"clippy.toml: forbidden test carveouts present: {bad}. "
        "Use policy/no-panic-allowlist.toml instead "
        "(see docs/CLIPPY_POLICY.md)."
    ]


def check_workspace_lints_present() -> list[str]:
    """The [workspace.lints] block must exist; specific lint presence is
    governed by policy/clippy-debt.toml (which lists each managed lint
    with its current level, including `absent`)."""
    errors: list[str] = []
    data = load_toml(ROOT_CARGO)
    lints = data.get("workspace", {}).get("lints", {})
    if not lints:
        errors.append(
            "Cargo.toml: missing [workspace.lints] section "
            "(see policy/clippy-lints.toml)."
        )
    return errors


def check_debt_consistency() -> list[str]:
    """Every entry in clippy-debt.toml must have warn-level in workspace."""
    errors: list[str] = []
    if not DEBT_TOML.exists():
        return errors
    debt = load_toml(DEBT_TOML).get("debt", [])
    cargo = load_toml(ROOT_CARGO)
    clippy_lints = cargo.get("workspace", {}).get("lints", {}).get("clippy", {})
    rust_lints = cargo.get("workspace", {}).get("lints", {}).get("rust", {})
    for entry in debt:
        lint = entry.get("lint", "")
        level = entry.get("level")
        # Strip "clippy::" / "rust::" prefix
        if lint.startswith("clippy::"):
            name = lint[len("clippy::"):]
            actual = _level_of(clippy_lints.get(name))
        else:
            name = lint.removeprefix("rust::")
            actual = _level_of(rust_lints.get(name))
        if level == "absent":
            # The debt entry asserts the lint is intentionally NOT yet
            # present in [workspace.lints]; flag drift if it appears.
            if actual is not None:
                errors.append(
                    f"clippy-debt.toml: lint '{lint}' is recorded as "
                    f"level='absent' but Cargo.toml has {actual!r}."
                )
            continue
        if actual is None:
            errors.append(
                f"clippy-debt.toml: lint '{lint}' is in the debt ledger "
                f"(level={level!r}) but not present in [workspace.lints]."
            )
            continue
        if actual != level:
            errors.append(
                f"clippy-debt.toml: lint '{lint}' is recorded as "
                f"level={level!r} but Cargo.toml has {actual!r}."
            )
    return errors


def _level_of(value) -> str | None:
    if value is None:
        return None
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        return value.get("level")
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    inheritance_errors = check_inheritance()
    carveout_errors = check_clippy_toml_carveouts()
    presence_errors = check_workspace_lints_present()
    debt_errors = check_debt_consistency()

    hard = inheritance_errors + carveout_errors + presence_errors
    soft = debt_errors

    if hard:
        print("lint-policy: hard violations:", file=sys.stderr)
        for err in hard:
            print(f"  {err}", file=sys.stderr)
    if soft:
        print("lint-policy: drift (soft):", file=sys.stderr)
        for err in soft:
            print(f"  {err}", file=sys.stderr)
    if not hard and not soft:
        print("lint-policy: OK", file=sys.stderr)

    if hard:
        return 1
    if soft and args.strict:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
