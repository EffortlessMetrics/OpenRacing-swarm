#!/usr/bin/env python3
"""
Compatibility Usage Tracking Script

This script tracks usage of deprecated APIs and compatibility shims across the workspace
to ensure technical debt trends downward over time.

Usage:
    python scripts/track_compat_usage.py --baseline    # Create baseline measurement
    python scripts/track_compat_usage.py --current     # Show current usage
    python scripts/track_compat_usage.py --trend       # Show trend analysis
    python scripts/track_compat_usage.py --trend --fail-if-increasing  # CI mode
"""

import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Tuple, Optional

# Enable UTF-8 mode on Windows to handle emoji output
if sys.platform == "win32":
    import codecs
    sys.stdout = codecs.getwriter('utf-8')(sys.stdout.buffer, 'strict')
    sys.stderr = codecs.getwriter('utf-8')(sys.stderr.buffer, 'strict')


class CompatUsageTracker:
    """Tracks usage of deprecated APIs and compatibility shims."""
    
    def __init__(self, workspace_root: Path):
        self.workspace_root = workspace_root
        self.baseline_file = workspace_root / "compat_baseline.json"
        self.current_file = workspace_root / "compat_current.json"
        
        # Define deprecated patterns to search for
        self.deprecated_patterns = {
            # Telemetry field renames — match only dot-access to detect deprecated field usage,
            # not standalone variable names that happen to share a name.
            "temp_c": {
                "pattern": r"\.temp_c\b",
                "category": "telemetry_fields",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "temperature_c"
            },
            "wheel_angle_mdeg": {
                "pattern": r"\.wheel_angle_mdeg\b",
                "category": "telemetry_fields",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "wheel_angle_deg"
            },
            "wheel_speed_mrad_s": {
                "pattern": r"\.wheel_speed_mrad_s\b",
                "category": "telemetry_fields",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "wheel_speed_rad_s"
            },
            "faults": {
                "pattern": r"\.faults\b",
                "category": "telemetry_fields",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "fault_flags"
            },
            # NOTE: "sequence" pattern removed — the field name `.sequence` is too
            # generic (protocol sequence counters, frame sequence numbers, LED
            # pattern sequences, etc.) causing unmanageable false positives.  The
            # deprecated TelemetryData.sequence field has no replacement anyway.
            
            # DeviceId constructors
            "create_device_id": {
                "pattern": r"\bcreate_device_id\s*\(",
                "category": "device_id",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "DeviceId::from_str()"
            },
            "DeviceId::new": {
                "pattern": r"\bDeviceId::new\s*\(",
                "category": "device_id", 
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "DeviceId::from_str()"
            },
            
            # Async patterns
            "BoxFuture": {
                "pattern": r"\bBoxFuture\b",
                "category": "async_patterns",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0", 
                "replacement": "#[async_trait]"
            },
            "impl Future": {
                "pattern": r"-> impl .*Future",
                "category": "async_patterns",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "#[async_trait]"
            },
            
            # Glob re-exports
            "glob_reexport": {
                "pattern": r"pub use .*::\*;",
                "category": "api_patterns",
                "deprecated_in": "1.2.0", 
                "remove_in": "1.4.0",
                "replacement": "explicit prelude usage"
            },
            
            # Cross-crate private imports
            "private_import": {
                "pattern": r"use \w+::(internal|private|tests)::",
                "category": "api_patterns",
                "deprecated_in": "1.2.0",
                "remove_in": "1.4.0",
                "replacement": "public API usage"
            }
        }
        
        # Directories to search
        self.search_dirs = [
            "crates/",
            "benches/",
            "scripts/",
            "docs/"
        ]

        # Directories to exclude from scanning (contain definitions, not usages)
        self.excluded_dirs = [
            "crates/compat/",  # defines deprecated APIs
            "crates/schemas/tests/compile-fail/",  # tests that deprecated APIs fail
            "crates/schemas/tests/compile_fail/",  # same
            "crates/integration-tests/",  # test crate exercises all device protocol APIs
            "crates/tools/",  # repo-maintenance binaries; test fixtures contain literal pattern strings by design
            "crates/hid-simucube-protocol/",  # intra-crate glob re-exports (not deprecated API pattern)
            "crates/hid-asetek-protocol/",  # intra-crate glob re-exports (not deprecated API pattern)
            "crates/hid-openffboard-protocol/",  # intra-crate glob re-exports (not deprecated API pattern)
            "crates/hid-vrs-protocol/",  # intra-crate glob re-exports (not deprecated API pattern)
            "crates/hid-cammus-protocol/",  # intra-crate glob re-exports (not deprecated API pattern)
            "crates/schemas/tests/",  # compile-fail tests and DeviceId::new test fixtures
        ]
        
        # File extensions to search
        self.search_extensions = [".rs", ".toml", ".md", ".py"]

    def scan_usage(self) -> Dict[str, List[Dict]]:
        """Scan workspace for deprecated API usage."""
        usage_data = defaultdict(list)
        
        for search_dir in self.search_dirs:
            search_path = self.workspace_root / search_dir
            if not search_path.exists():
                continue
                
            for pattern_name, pattern_info in self.deprecated_patterns.items():
                matches = self._search_pattern(
                    pattern_info["pattern"], 
                    search_path,
                    pattern_name
                )
                usage_data[pattern_name].extend(matches)
        
        return dict(usage_data)

    def _search_pattern(self, pattern: str, search_path: Path, pattern_name: str) -> List[Dict]:
        """Search for a specific pattern in files."""
        matches = []

        try:
            # Use ripgrep for fast searching
            cmd = [
                "rg",
                "--json",
                "--type-add", "rust:*.rs",
                "--type-add", "toml:*.toml", 
                "--type-add", "markdown:*.md",
                "--type-add", "python:*.py",
                "-e", pattern,
                str(search_path)
            ]

            result = subprocess.run(cmd, capture_output=True, text=True, check=False)

            if result.returncode == 0:
                for line in result.stdout.strip().split('\n'):
                    if not line:
                        continue
                    try:
                        match_data = json.loads(line)
                        if match_data.get("type") == "match":
                            data = match_data["data"]
                            file_path = data["path"]["text"]
                            # Skip files in excluded directories
                            rel_path = file_path.replace("\\", "/")
                            if any(excl in rel_path for excl in self.excluded_dirs):
                                continue
                            matches.append({
                                "file": file_path,
                                "line": data["line_number"],
                                "column": data["submatches"][0]["start"],
                                "text": data["lines"]["text"].strip(),
                                "pattern": pattern_name,
                                "category": self.deprecated_patterns[pattern_name]["category"]
                            })
                    except (json.JSONDecodeError, KeyError):
                        continue

        except subprocess.SubprocessError as e:
            print(f"Warning: Failed to search for pattern {pattern_name}: {e}")

        return matches

    def save_usage_data(self, usage_data: Dict, filename: Path):
        """Save usage data to JSON file."""
        output_data = {
            "timestamp": datetime.now().isoformat(),
            "total_usages": sum(len(matches) for matches in usage_data.values()),
            "by_pattern": usage_data,
            "by_category": self._group_by_category(usage_data),
            "summary": self._generate_summary(usage_data)
        }
        
        with open(filename, 'w') as f:
            json.dump(output_data, f, indent=2)

    def _group_by_category(self, usage_data: Dict) -> Dict[str, int]:
        """Group usage counts by category."""
        by_category = defaultdict(int)
        
        for pattern_name, matches in usage_data.items():
            category = self.deprecated_patterns[pattern_name]["category"]
            by_category[category] += len(matches)
            
        return dict(by_category)

    def _generate_summary(self, usage_data: Dict) -> Dict:
        """Generate summary statistics."""
        total = sum(len(matches) for matches in usage_data.values())
        by_category = self._group_by_category(usage_data)
        
        # Find most problematic patterns
        top_patterns = sorted(
            [(name, len(matches)) for name, matches in usage_data.items()],
            key=lambda x: x[1],
            reverse=True
        )[:5]
        
        return {
            "total_usages": total,
            "categories": len(by_category),
            "patterns_with_usage": len([p for p in usage_data.values() if p]),
            "top_patterns": top_patterns,
            "by_category": by_category
        }

    def load_usage_data(self, filename: Path) -> Optional[Dict]:
        """Load usage data from JSON file."""
        if not filename.exists():
            return None
            
        try:
            with open(filename, 'r') as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError) as e:
            print(f"Warning: Failed to load {filename}: {e}")
            return None

    def compare_usage(self, baseline_data: Dict, current_data: Dict) -> Dict:
        """Compare current usage against baseline."""
        if not baseline_data or not current_data:
            return {"error": "Missing baseline or current data"}
            
        baseline_total = baseline_data.get("total_usages", 0)
        current_total = current_data.get("total_usages", 0)
        delta = current_total - baseline_total
        
        # Compare by category
        baseline_categories = baseline_data.get("by_category", {})
        current_categories = current_data.get("by_category", {})
        
        category_deltas = {}
        for category in set(baseline_categories.keys()) | set(current_categories.keys()):
            baseline_count = baseline_categories.get(category, 0)
            current_count = current_categories.get(category, 0)
            category_deltas[category] = current_count - baseline_count
            
        # Determine trend
        if delta < 0:
            trend = "DOWN"
            trend_emoji = "⬇️"
        elif delta > 0:
            trend = "UP" 
            trend_emoji = "⬆️"
        else:
            trend = "STABLE"
            trend_emoji = "➡️"
            
        return {
            "baseline_total": baseline_total,
            "current_total": current_total,
            "delta": delta,
            "trend": trend,
            "trend_emoji": trend_emoji,
            "category_deltas": category_deltas,
            "baseline_timestamp": baseline_data.get("timestamp"),
            "current_timestamp": current_data.get("timestamp")
        }

    def print_current_report(self, usage_data: Dict):
        """Print current usage report."""
        summary = usage_data.get("summary", {})
        total = summary.get("total_usages", 0)
        
        print("Compatibility Usage Report")
        print("=" * 50)
        print(f"Total deprecated API usages: {total}")
        print(f"Timestamp: {usage_data.get('timestamp', 'unknown')}")
        print()
        
        if total == 0:
            print("🎉 No deprecated API usage found!")
            return
            
        # By category
        by_category = summary.get("by_category", {})
        if by_category:
            print("Usage by Category:")
            for category, count in sorted(by_category.items(), key=lambda x: x[1], reverse=True):
                print(f"  {category}: {count} usages")
            print()
            
        # Top patterns
        top_patterns = summary.get("top_patterns", [])
        if top_patterns:
            print("Most Used Deprecated APIs:")
            for pattern, count in top_patterns[:5]:
                if count > 0:
                    replacement = self.deprecated_patterns[pattern]["replacement"]
                    remove_in = self.deprecated_patterns[pattern]["remove_in"]
                    print(f"  {pattern}: {count} usages (remove in v{remove_in})")
                    print(f"    → Use: {replacement}")
            print()

    def print_trend_report(self, comparison: Dict):
        """Print trend analysis report."""
        print("Compatibility Debt Trend Report")
        print("=" * 50)
        
        if "error" in comparison:
            print(f"Error: {comparison['error']}")
            return
            
        baseline_total = comparison["baseline_total"]
        current_total = comparison["current_total"] 
        delta = comparison["delta"]
        trend = comparison["trend"]
        trend_emoji = comparison["trend_emoji"]
        
        print(f"Baseline: {baseline_total} usages")
        print(f"Current:  {current_total} usages")
        print(f"Delta:    {delta:+d} usages")
        print(f"Trend:    {trend_emoji} {trend}")
        print()
        
        # Category breakdown
        category_deltas = comparison.get("category_deltas", {})
        if category_deltas:
            print("Changes by Category:")
            for category, delta in sorted(category_deltas.items(), key=lambda x: abs(x[1]), reverse=True):
                if delta != 0:
                    sign = "+" if delta > 0 else ""
                    emoji = "⬆️" if delta > 0 else "⬇️"
                    print(f"  {category}: {sign}{delta} {emoji}")
            print()
            
        # Trend assessment
        if trend == "DOWN":
            print("✅ Good: Compatibility debt is decreasing")
        elif trend == "STABLE":
            print("⚠️  Neutral: Compatibility debt is stable")
        else:
            print("❌ Bad: Compatibility debt is increasing")
            print("   Action required: Review recent changes and migrate away from deprecated APIs")

    def create_baseline(self):
        """Create baseline measurement."""
        print("Creating compatibility usage baseline...")
        usage_data = self.scan_usage()
        self.save_usage_data(usage_data, self.baseline_file)
        print(f"Baseline saved to {self.baseline_file}")
        self.print_current_report(usage_data)

    def show_current(self):
        """Show current usage without comparison."""
        print("Scanning current compatibility usage...")
        usage_data = self.scan_usage()
        self.save_usage_data(usage_data, self.current_file)
        self.print_current_report(usage_data)

    def analyze_trend(self, fail_if_increasing: bool = False):
        """Analyze trend against baseline."""
        print("Analyzing compatibility debt trend...")
        
        # Load baseline
        baseline_data = self.load_usage_data(self.baseline_file)
        if not baseline_data:
            print("Error: No baseline found. Run with --baseline first.")
            sys.exit(1)
            
        # Scan current
        current_usage = self.scan_usage()
        current_data = {
            "timestamp": datetime.now().isoformat(),
            "total_usages": sum(len(matches) for matches in current_usage.values()),
            "by_category": self._group_by_category(current_usage),
            "summary": self._generate_summary(current_usage)
        }
        
        # Compare
        comparison = self.compare_usage(baseline_data, current_data)
        self.print_trend_report(comparison)
        
        # Fail if increasing and requested
        if fail_if_increasing and comparison.get("trend") == "UP":
            print("\n❌ FAIL: Compatibility debt increased (--fail-if-increasing)")
            sys.exit(1)
            
        print("\n✅ PASS: Trend analysis complete")


def main():
    parser = argparse.ArgumentParser(description="Track compatibility usage across workspace")
    parser.add_argument("--baseline", action="store_true", help="Create baseline measurement")
    parser.add_argument("--current", action="store_true", help="Show current usage")
    parser.add_argument("--trend", action="store_true", help="Show trend analysis")
    parser.add_argument("--fail-if-increasing", action="store_true", help="Exit 1 if debt increased")
    parser.add_argument("--workspace", type=Path, default=Path.cwd(), help="Workspace root directory")
    
    args = parser.parse_args()
    
    # Validate workspace
    workspace_root = args.workspace.resolve()
    if not (workspace_root / "Cargo.toml").exists():
        print(f"Error: {workspace_root} does not appear to be a Rust workspace")
        sys.exit(1)
        
    tracker = CompatUsageTracker(workspace_root)
    
    if args.baseline:
        tracker.create_baseline()
    elif args.current:
        tracker.show_current()
    elif args.trend:
        tracker.analyze_trend(args.fail_if_increasing)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()