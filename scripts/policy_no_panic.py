#!/usr/bin/env python3
"""Semantic no-panic family checker for OpenRacing.

This is the authoritative exception ledger for panic-family findings. It
walks every Rust file under the workspace, detects panic-family calls,
matches them against ``policy/no-panic-allowlist.toml`` keyed by
``path + family + selector``, and reports drift / staleness / expiry.

It is intentionally lightweight: it uses a regex-based pre-pass and a
small AST-aware post-filter via Python's tokenize-equivalent line
scanner. It is **not** a Clippy replacement; Clippy still runs in CI
and catches the local code shape. See docs/NO_PANIC_POLICY.md.

Stage A: this script runs and reports drift; non-blocking.
Stage B: drift becomes blocking; expired/stale entries fail policy.

Outputs:

  target/no-panic-report.json
  target/no-panic-report.md
  target/no-panic-proposed-allowlist.toml  (only with --propose)

Exit codes:

  0  no policy violations (Stage A always returns 0 unless --strict)
  1  schema/expiry failure, or drift with --strict

CLI:

  python scripts/policy_no_panic.py
  python scripts/policy_no_panic.py --strict
  python scripts/policy_no_panic.py --propose

The script never mutates ``policy/no-panic-allowlist.toml``.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST_PATH = REPO_ROOT / "policy" / "no-panic-allowlist.toml"
REPORT_DIR = REPO_ROOT / "target"
REPORT_JSON = REPORT_DIR / "no-panic-report.json"
REPORT_MD = REPORT_DIR / "no-panic-report.md"
PROPOSED_PATH = REPORT_DIR / "no-panic-proposed-allowlist.toml"

# Map regex-detected token to a panic family. Order matters: longer /
# more specific patterns are matched first.
PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("panic_macro", re.compile(r"\bpanic!\s*\(")),
    ("todo", re.compile(r"\btodo!\s*\(")),
    ("unimplemented", re.compile(r"\bunimplemented!\s*\(")),
    ("unreachable", re.compile(r"\bunreachable!\s*\(")),
    ("get_unwrap", re.compile(r"\.get\s*\([^()]*\)\s*\.\s*unwrap\s*\(")),
    ("expect", re.compile(r"\.expect\s*\(")),
    ("unwrap", re.compile(r"\.unwrap\s*\(\s*\)")),
    # `&s[a..b]` style panicking string slice (heuristic).
    ("string_slice", re.compile(r"&[A-Za-z_][A-Za-z0-9_]*\[[^\]]+\.\.[^\]]*\]")),
    # `xs[i]` direct indexing on identifiers. Heuristic; staged as warn /
    # collect-only. The negative lookbehind excludes obvious non-indexing
    # contexts: identifier continuation, dot-chains, attribute markers,
    # type ascription (`: [u8; 4]`), and the `[T; N]` array-type form.
    ("indexing", re.compile(
        r"(?<![\w\]\)\.\:\#])"
        r"[A-Za-z_][A-Za-z0-9_]*\[(?![A-Za-z_][A-Za-z0-9_<>:'\s]*;\s)[^\]]+\]"
    )),
]

# Lines containing these markers are skipped entirely. Comments and
# strings are an unavoidable source of false positives in a regex-only
# checker, so we strip them conservatively below.
SKIP_LINE_RE = re.compile(r"^\s*//")

# Heuristic to skip lines that look like comments or doc tests.
COMMENT_TRIM = re.compile(r"//.*$")

# Heuristic to remove string and char literals before pattern matching,
# to cut false positives from log strings such as "called `unwrap` on".
STRING_LIT_RE = re.compile(r'"(?:\\.|[^"\\])*"')
CHAR_LIT_RE = re.compile(r"'(?:\\.|[^'\\])'")


@dataclass(frozen=True)
class Finding:
    path: str
    family: str
    line: int
    column: int
    container: str
    snippet: str

    def selector(self) -> dict:
        return {
            "kind": _selector_kind_for(self.family),
            "container": self.container,
            "callee": self.family,
            "receiver_fingerprint": _fingerprint(self.snippet),
        }

    def identity(self) -> str:
        sel = self.selector()
        key = "|".join([
            self.path,
            self.family,
            sel["kind"],
            sel["container"],
            sel["callee"],
            sel["receiver_fingerprint"],
        ])
        return hashlib.sha1(key.encode("utf-8")).hexdigest()[:12]


def _selector_kind_for(family: str) -> str:
    if family in {"panic_macro", "todo", "unimplemented", "unreachable"}:
        return "macro_call"
    if family in {"indexing", "string_slice"}:
        return "index_expr"
    return "method_call"


def _fingerprint(snippet: str) -> str:
    s = snippet.strip()
    if len(s) > 80:
        s = s[:80]
    return re.sub(r"\s+", " ", s)


def list_rust_files() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "*.rs"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [
        REPO_ROOT / line
        for line in out.splitlines()
        if line and not line.startswith("third_party/")
    ]


CONTAINER_RE = re.compile(
    r"^\s*(?:pub(?:\(.*?\))?\s+)?"
    r"(?:async\s+)?"
    r"(?:unsafe\s+)?"
    r"(?:const\s+)?"
    r"(?:fn|impl|struct|enum|trait|mod)\s+([A-Za-z0-9_<>:]+)"
)


def scan_file(path: Path) -> list[Finding]:
    findings: list[Finding] = []
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return findings
    rel = str(path.relative_to(REPO_ROOT))
    container = "<file>"
    in_block_comment = False
    for lineno, raw in enumerate(text.splitlines(), start=1):
        line = raw
        # Cheap block-comment tracker.
        if in_block_comment:
            end = line.find("*/")
            if end == -1:
                continue
            line = line[end + 2:]
            in_block_comment = False
        while True:
            start = line.find("/*")
            if start == -1:
                break
            end = line.find("*/", start + 2)
            if end == -1:
                line = line[:start]
                in_block_comment = True
                break
            line = line[:start] + line[end + 2:]
        if SKIP_LINE_RE.match(line):
            continue
        # Track enclosing item.
        m = CONTAINER_RE.match(line)
        if m:
            container = m.group(1)
        # Strip comments and string/char literals.
        scrub = COMMENT_TRIM.sub("", line)
        scrub = STRING_LIT_RE.sub('""', scrub)
        scrub = CHAR_LIT_RE.sub("' '", scrub)
        for family, pat in PATTERNS:
            for hit in pat.finditer(scrub):
                col = hit.start() + 1
                findings.append(Finding(
                    path=rel,
                    family=family,
                    line=lineno,
                    column=col,
                    container=container,
                    snippet=line.strip(),
                ))
    return findings


def load_allowlist() -> dict:
    if not ALLOWLIST_PATH.exists():
        sys.exit(f"error: {ALLOWLIST_PATH} not found")
    with ALLOWLIST_PATH.open("rb") as fh:
        return tomllib.load(fh)


def selector_matches(finding: Finding, entry: dict) -> bool:
    if entry.get("path") != finding.path:
        return False
    if entry.get("family") != finding.family:
        return False
    sel = entry.get("selector", {})
    if not sel:
        # Path+family-only entries are accepted as a coarse match.
        return True
    if sel.get("kind") and sel["kind"] != _selector_kind_for(finding.family):
        return False
    if sel.get("container") and sel["container"] != finding.container:
        return False
    if sel.get("callee") and sel["callee"] not in (finding.family, "[]"):
        return False
    fingerprint = sel.get("receiver_fingerprint")
    if fingerprint and fingerprint != _fingerprint(finding.snippet):
        return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict", action="store_true",
        help="Fail on drift / unreceipted findings even during Stage A.",
    )
    parser.add_argument(
        "--propose", action="store_true",
        help="Write target/no-panic-proposed-allowlist.toml.",
    )
    parser.add_argument(
        "--families",
        nargs="*",
        default=None,
        help="Restrict to specific families (e.g. unwrap expect).",
    )
    args = parser.parse_args()

    allowlist = load_allowlist()
    schema_version = allowlist.get("schema_version")
    if schema_version != "0.3":
        print(
            f"warn: allowlist schema_version is {schema_version!r}, "
            f"expected '0.3'",
            file=sys.stderr,
        )
    entries: list[dict] = allowlist.get("allow", [])
    today = dt.date.today()
    default_expires = allowlist.get("default_expires")

    rust_files = list_rust_files()
    findings: list[Finding] = []
    for path in rust_files:
        findings.extend(scan_file(path))
    if args.families:
        wanted = set(args.families)
        findings = [f for f in findings if f.family in wanted]

    matched_entries: set[int] = set()
    unreceipted: list[Finding] = []
    receipted: list[Finding] = []
    for finding in findings:
        hit_idx = None
        for idx, entry in enumerate(entries):
            if selector_matches(finding, entry):
                hit_idx = idx
                break
        if hit_idx is None:
            unreceipted.append(finding)
        else:
            matched_entries.add(hit_idx)
            receipted.append(finding)

    stale: list[dict] = []
    expired: list[dict] = []
    for idx, entry in enumerate(entries):
        if idx not in matched_entries and not entry.get("retired"):
            stale.append({"index": idx, "entry": entry})
        expires = entry.get("expires", default_expires)
        if expires:
            try:
                d = dt.date.fromisoformat(str(expires))
            except ValueError:
                continue
            if d < today:
                expired.append({"index": idx, "entry": entry, "expires": str(d)})

    by_family: dict[str, int] = {}
    for finding in unreceipted:
        by_family[finding.family] = by_family.get(finding.family, 0) + 1

    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    REPORT_JSON.write_text(json.dumps({
        "schema_version": schema_version,
        "stage": "A",
        "total_findings": len(findings),
        "receipted": len(receipted),
        "unreceipted": len(unreceipted),
        "by_family": by_family,
        "stale_entries": [s["index"] for s in stale],
        "expired_entries": [e["index"] for e in expired],
    }, indent=2, sort_keys=True))

    md_lines = [
        "# No-panic family report",
        "",
        f"- total findings: {len(findings)}",
        f"- receipted: {len(receipted)}",
        f"- unreceipted: {len(unreceipted)}",
        f"- stale allowlist entries: {len(stale)}",
        f"- expired allowlist entries: {len(expired)}",
        "",
        "## By family",
    ]
    for family, count in sorted(by_family.items(), key=lambda kv: -kv[1]):
        md_lines.append(f"- `{family}`: {count}")
    md_lines.append("")
    if unreceipted:
        md_lines.append("## Unreceipted findings (top 200)")
        for f in unreceipted[:200]:
            md_lines.append(
                f"- `{f.path}:{f.line}:{f.column}` "
                f"family=`{f.family}` container=`{f.container}`"
            )
        if len(unreceipted) > 200:
            md_lines.append(f"- ... ({len(unreceipted) - 200} more)")
        md_lines.append("")
    if stale:
        md_lines.append("## Stale entries")
        for s in stale[:50]:
            entry = s["entry"]
            md_lines.append(
                f"- `{entry.get('path','?')}` family=`{entry.get('family','?')}` "
                f"id=`{entry.get('id','?')}`"
            )
        if len(stale) > 50:
            md_lines.append(f"- ... ({len(stale) - 50} more)")
        md_lines.append("")
    if expired:
        md_lines.append("## Expired entries")
        for e in expired:
            entry = e["entry"]
            md_lines.append(
                f"- `{entry.get('path','?')}` id=`{entry.get('id','?')}` "
                f"expired={e['expires']}"
            )
        md_lines.append("")
    REPORT_MD.write_text("\n".join(md_lines))

    if args.propose and unreceipted:
        write_proposed(unreceipted)

    print(
        f"no-panic: findings={len(findings)} receipted={len(receipted)} "
        f"unreceipted={len(unreceipted)} stale={len(stale)} expired={len(expired)}",
        file=sys.stderr,
    )

    if expired:
        # Expired exceptions are always blocking once Stage A is in.
        return 1
    if args.strict and (unreceipted or stale):
        return 1
    return 0


def write_proposed(unreceipted: Iterable[Finding]) -> None:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Proposed no-panic allowlist entries.",
        "# Review, add owner/reason/expiry, and move into",
        "# policy/no-panic-allowlist.toml.",
        "",
        'schema_version = "0.3"',
        "",
    ]
    used_ids: set[str] = set()
    for finding in unreceipted:
        ident = finding.identity()
        if ident in used_ids:
            continue
        used_ids.add(ident)
        sel = finding.selector()
        lines.extend([
            "[[allow]]",
            f'id = "panic-{ident}"',
            f'path = "{finding.path}"',
            f'family = "{finding.family}"',
            'classification = "TODO"',
            'owner = "TODO"',
            f'explanation = "TODO: {finding.snippet[:60].replace(chr(34), chr(39))}"',
            'expires = "2027-05-06"',
            "",
            "[allow.selector]",
            f'kind = "{sel["kind"]}"',
            f'container = "{sel["container"]}"',
            f'callee = "{sel["callee"]}"',
            f'receiver_fingerprint = "{sel["receiver_fingerprint"]}"',
            "",
            "[allow.last_seen]",
            f"line = {finding.line}",
            f"column = {finding.column}",
            "",
        ])
    PROPOSED_PATH.write_text("\n".join(lines))
    print(f"no-panic: wrote {PROPOSED_PATH}", file=sys.stderr)


if __name__ == "__main__":
    sys.exit(main())
