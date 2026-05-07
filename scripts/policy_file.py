#!/usr/bin/env python3
"""Validate `policy/non-rust-allowlist.toml` against the working tree.

This is OpenRacing's non-Rust file policy checker. Every git-tracked
non-Rust file (other than Cargo.toml/Cargo.lock and similar
Rust-native config) must match an allowlist entry, or the file fails
policy.

Stages (see docs/FILE_POLICY.md):

  Stage 1 (current): runs and reports drift; non-blocking.
  Stage 2:            blocking; new non-Rust surfaces require a policy
                      entry to land.
  Stage 3:            ``expires`` is enforced; retired entries with no
                      matching files are deleted in cleanup PRs.

Outputs:

  target/file-policy.json
  target/file-policy.md

Exit codes:

  0  all matched (or warnings only)
  1  failure (Stage 2+) or --strict at Stage 1

CLI:

  python scripts/policy_file.py
  python scripts/policy_file.py --strict
  python scripts/policy_file.py --json target/file-policy.json
"""
from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import json
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST_PATH = REPO_ROOT / "policy" / "non-rust-allowlist.toml"
REPORT_DIR = REPO_ROOT / "target"
REPORT_JSON = REPORT_DIR / "file-policy.json"
REPORT_MD = REPORT_DIR / "file-policy.md"

# Files that the policy implicitly allows; they are governed by other
# tooling (cargo, rustup, clippy) rather than the non-Rust file policy.
IMPLICIT_ALLOW_BASENAMES = {
    "Cargo.toml",
}

# File extensions that are considered Rust-native and skipped entirely.
RUST_EXTS = {".rs"}

# Required fields per allowlist entry.
REQUIRED_FIELDS = {"kind", "owner", "surface", "classification", "reason", "covered_by"}

VALID_SURFACES = {
    "ci", "docs", "fixtures", "tooling", "packaging", "proto",
    "schema", "shader", "editor", "website", "badge", "scripts",
    "hardware", "telemetry", "thirdparty",
}
VALID_CLASSIFICATIONS = {
    "config", "test", "production", "tooling", "generated", "thirdparty",
}


def git_tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def is_rust_file(path: str) -> bool:
    return any(path.endswith(ext) for ext in RUST_EXTS)


def is_implicit_allow(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    return name in IMPLICIT_ALLOW_BASENAMES


def load_allowlist() -> dict:
    if not ALLOWLIST_PATH.exists():
        sys.exit(f"error: {ALLOWLIST_PATH} not found")
    with ALLOWLIST_PATH.open("rb") as fh:
        return tomllib.load(fh)


def validate_entry(entry: dict, idx: int) -> list[str]:
    errors: list[str] = []
    if "glob" not in entry and "path" not in entry:
        errors.append(f"entry #{idx}: missing required 'glob' or 'path' field")
    if "glob" in entry and "path" in entry:
        errors.append(f"entry #{idx}: must not set both 'glob' and 'path'")
    for field in REQUIRED_FIELDS:
        if field not in entry:
            errors.append(f"entry #{idx}: missing required field '{field}'")
    if "surface" in entry and entry["surface"] not in VALID_SURFACES:
        errors.append(
            f"entry #{idx}: invalid surface '{entry['surface']}' "
            f"(valid: {sorted(VALID_SURFACES)})"
        )
    if "classification" in entry and entry["classification"] not in VALID_CLASSIFICATIONS:
        errors.append(
            f"entry #{idx}: invalid classification '{entry['classification']}' "
            f"(valid: {sorted(VALID_CLASSIFICATIONS)})"
        )
    if "covered_by" in entry and not isinstance(entry["covered_by"], list):
        errors.append(f"entry #{idx}: 'covered_by' must be a list")
    if "expires" in entry:
        try:
            dt.date.fromisoformat(str(entry["expires"]))
        except ValueError:
            errors.append(
                f"entry #{idx}: 'expires' must be ISO date "
                f"(got {entry['expires']!r})"
            )
    return errors


def matches(entry: dict, path: str) -> bool:
    if entry.get("retired"):
        return False
    if "path" in entry:
        return path == entry["path"]
    pattern = entry["glob"]
    # fnmatch does not honor `**`; use a simple translation.
    return _glob_match(pattern, path)


def _specificity(entry: dict) -> int:
    if "path" in entry:
        # Exact path is always most specific: bias by a large constant
        # plus the path's segment count.
        return 1000 + entry["path"].count("/")
    pattern = entry["glob"]
    score = 0
    for segment in pattern.split("/"):
        if segment in ("**", "*"):
            continue
        score += 1
        # Reward fully literal segments slightly more than glob segments.
        if "*" not in segment and "?" not in segment:
            score += 1
    return score


def _glob_match(pattern: str, path: str) -> bool:
    # Path-aware glob: `*` matches a single segment (no `/`); `**`
    # matches zero or more full segments. Patterns without a `/` only
    # match the file's basename.
    parts = pattern.split("/")
    path_parts = path.split("/")
    if "/" not in pattern:
        # Basename-only pattern (e.g. "Cargo.lock", "*.md").
        return fnmatch.fnmatchcase(path_parts[-1], pattern)
    return _match_parts(parts, path_parts)


def _match_parts(pat_parts: list[str], path_parts: list[str]) -> bool:
    if not pat_parts:
        return not path_parts
    head, *tail = pat_parts
    if head == "**":
        if not tail:
            return True
        for i in range(len(path_parts) + 1):
            if _match_parts(tail, path_parts[i:]):
                return True
        return False
    if not path_parts:
        return False
    if not fnmatch.fnmatchcase(path_parts[0], head):
        return False
    return _match_parts(tail, path_parts[1:])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Fail on drift even during Stage 1.",
    )
    parser.add_argument(
        "--json",
        type=Path,
        default=REPORT_JSON,
        help="Path for JSON report.",
    )
    parser.add_argument(
        "--md",
        type=Path,
        default=REPORT_MD,
        help="Path for Markdown report.",
    )
    args = parser.parse_args()

    data = load_allowlist()
    entries: list[dict] = data.get("allow", [])
    schema_errors: list[str] = []
    for idx, entry in enumerate(entries):
        schema_errors.extend(validate_entry(entry, idx))

    files = git_tracked_files()
    today = dt.date.today()

    unmatched: list[str] = []
    matched_by_entry: dict[int, list[str]] = {i: [] for i in range(len(entries))}
    expired: list[dict] = []

    for path in files:
        if is_rust_file(path) or is_implicit_allow(path):
            continue
        # Prefer the most specific matching entry. Specificity score:
        # number of non-wildcard path segments. `path` entries beat
        # `glob` entries on ties.
        best_idx = None
        best_score = -1
        for idx, entry in enumerate(entries):
            if not matches(entry, path):
                continue
            score = _specificity(entry)
            if score > best_score:
                best_idx = idx
                best_score = score
        if best_idx is None:
            unmatched.append(path)
        else:
            matched_by_entry[best_idx].append(path)

    unused: list[dict] = []
    for idx, entry in enumerate(entries):
        if entry.get("retired"):
            continue
        if not matched_by_entry[idx]:
            unused.append({"index": idx, "entry": entry})
        if "expires" in entry:
            try:
                expiry = dt.date.fromisoformat(str(entry["expires"]))
            except ValueError:
                continue
            if expiry < today:
                expired.append({"index": idx, "entry": entry, "expires": str(expiry)})

    report = {
        "schema_errors": schema_errors,
        "total_files_scanned": len(files),
        "non_rust_unmatched": unmatched,
        "unused_entries": unused,
        "expired_entries": expired,
        "matched_count": sum(len(v) for v in matched_by_entry.values()),
        "stage": "1",
    }

    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(report, indent=2, sort_keys=True))

    md_lines = ["# File policy report", ""]
    md_lines.append(f"- total tracked files: {len(files)}")
    md_lines.append(f"- non-Rust matched: {report['matched_count']}")
    md_lines.append(f"- unmatched: {len(unmatched)}")
    md_lines.append(f"- unused allowlist entries: {len(unused)}")
    md_lines.append(f"- expired allowlist entries: {len(expired)}")
    md_lines.append(f"- schema errors: {len(schema_errors)}")
    md_lines.append("")
    if schema_errors:
        md_lines.append("## Schema errors")
        md_lines.extend(f"- {e}" for e in schema_errors)
        md_lines.append("")
    if unmatched:
        md_lines.append("## Unmatched files (top 200)")
        for p in unmatched[:200]:
            md_lines.append(f"- `{p}`")
        if len(unmatched) > 200:
            md_lines.append(f"- ... ({len(unmatched) - 200} more)")
        md_lines.append("")
    if unused:
        md_lines.append("## Unused entries")
        for u in unused:
            entry = u["entry"]
            sel = entry.get("path", entry.get("glob", "?"))
            md_lines.append(f"- `{sel}` (kind={entry.get('kind')})")
        md_lines.append("")
    if expired:
        md_lines.append("## Expired entries")
        for u in expired:
            entry = u["entry"]
            sel = entry.get("path", entry.get("glob", "?"))
            md_lines.append(f"- `{sel}` expired {u['expires']}")
        md_lines.append("")
    args.md.write_text("\n".join(md_lines))

    failed = bool(schema_errors) or bool(expired)
    drift = bool(unmatched) or bool(unused)

    if schema_errors:
        print(f"file-policy: {len(schema_errors)} schema errors", file=sys.stderr)
        for err in schema_errors:
            print(f"  {err}", file=sys.stderr)
    if unmatched:
        print(
            f"file-policy: {len(unmatched)} unmatched non-Rust files",
            file=sys.stderr,
        )
    if unused:
        print(
            f"file-policy: {len(unused)} unused allowlist entries",
            file=sys.stderr,
        )
    if expired:
        print(
            f"file-policy: {len(expired)} expired allowlist entries",
            file=sys.stderr,
        )

    if failed:
        return 1
    if drift and args.strict:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
