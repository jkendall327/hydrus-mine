#!/usr/bin/env python3
"""Validate curated GUI snapshots and build the standalone offline explorer.

No GUI behavior is assessed by this script. Source anchors are checked against
each inventory's pinned Git commit, rather than the current working tree.
"""

import argparse
from collections import Counter
from functools import lru_cache
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "docs/rust/gui-coverage"
OUTPUT = ROOT / "docs/rust/gui-progress.html"
STATUSES = {"first_pass", "partial", "missing", "unassessed"}


@lru_cache(maxsize=None)
def source_lines(commit, path):
    result = subprocess.run(
        ["git", "show", f"{commit}:{path}"], cwd=ROOT,
        capture_output=True, check=False,
    )
    if result.returncode:
        raise ValueError(f"source absent from {commit[:8]}: {path}")
    return result.stdout.decode("utf-8").splitlines()


def anchors(value):
    """Accept both original inventory source schemas, including nested evidence."""
    if isinstance(value, dict):
        path = value.get("path", value.get("relativepath"))
        if path is not None and "line" in value:
            yield path, value["line"]
        for child in value.values():
            yield from anchors(child)
    elif isinstance(value, list):
        for child in value:
            yield from anchors(child)


def validate(data, name):
    commit = data["git_head"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError(f"{name}: git_head must be a full commit hash")
    if data.get("snapshot_repository_url") != "https://github.com/jkendall327/hydrus-mine":
        raise ValueError(f"{name}: unexpected source repository URL")
    raw = json.dumps(data)
    if "/workspace/" in raw:
        raise ValueError(f"{name}: absolute workspace path in snapshot")
    nodes = data["nodes"]
    by_id = {n["id"]: n for n in nodes}
    if len(by_id) != len(nodes):
        raise ValueError(f"{name}: duplicate node IDs")
    if not any(n["parent_id"] is None for n in nodes):
        raise ValueError(f"{name}: missing root")
    for node in nodes:
        if node["status"] not in STATUSES:
            raise ValueError(f"{name}: unknown status at {node['id']}")
        if not isinstance(node["label"], str) or not node["label"]:
            raise ValueError(f"{name}: missing label at {node['id']}")
        seen = set()
        cursor = node
        while cursor["parent_id"] is not None:
            if cursor["id"] in seen:
                raise ValueError(f"{name}: parent cycle at {node['id']}")
            seen.add(cursor["id"])
            parent = cursor["parent_id"]
            if parent not in by_id:
                raise ValueError(f"{name}: missing parent {parent}")
            cursor = by_id[parent]
        refs = [node.get("target_id"), node.get("canonical_editor")]
        refs += node.get("editor_refs", []) + node.get("additional_parents", [])
        for target in filter(None, refs):
            if target not in by_id:
                raise ValueError(f"{name}: unresolved shared/navigation target {target}")
    count = 0
    for path, line in set(anchors(data)):
        pure = PurePosixPath(path)
        if pure.is_absolute() or ".." in pure.parts or "\\" in path:
            raise ValueError(f"{name}: non-relative source path {path}")
        if isinstance(line, bool) or not isinstance(line, int) or line < 1:
            raise ValueError(f"{name}: invalid source line {path}:{line}")
        lines = source_lines(commit, path)
        if line > len(lines):
            raise ValueError(f"{name}: line outside pinned source {path}:{line}")
        count += 1
    actual = dict(Counter(n["status"] for n in nodes))
    recorded = data.get("counts", {}).get("by_status") or data.get("summary", {}).get("status_counts")
    if recorded != actual:
        raise ValueError(f"{name}: stale recorded status counts")
    return {"nodes": len(nodes), "source_anchors": count, "statuses": actual}


def load_and_validate():
    snapshots = {}
    reports = {}
    for name in ["reference", "native"]:
        snapshots[name] = json.loads((DATA / f"{name}-inventory.json").read_text(encoding="utf-8"))
        reports[name] = validate(snapshots[name], name)
    native_ids = {n["id"] for n in snapshots["native"]["nodes"]}
    reference_ids = {n["id"] for n in snapshots["reference"]["nodes"]}
    for node in snapshots["reference"]["nodes"]:
        for target in node.get("native_id_candidates", []):
            if target not in native_ids:
                raise ValueError(f"Unresolved candidate native mapping: {node['id']} -> {target}")
    for proposal in snapshots["reference"].get("native_mapping_proposal", {}).get("proposals", []):
        if proposal["reference_id"] not in reference_ids or proposal["native_id"] not in native_ids:
            raise ValueError(f"Unresolved mapping proposal: {proposal}")
        if proposal.get("review_required") is not True:
            raise ValueError("Candidate mappings must retain review_required=true")
    return snapshots, reports


def render(snapshots):
    # Escaping '<' blocks script end tags even inside malicious string content;
    # the payload is parsed as JSON and all user-facing values use textContent.
    payload = json.dumps(snapshots, ensure_ascii=False, separators=(",", ":"))
    payload = payload.replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")
    payload = payload.replace("\u2028", "\\u2028").replace("\u2029", "\\u2029")
    template = (DATA / "viewer.template.html").read_text(encoding="utf-8")
    marker = "__GUI_COVERAGE_DATA__"
    if template.count(marker) != 1:
        raise ValueError("Template must contain exactly one data marker")
    return template.replace(marker, payload)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--validate", action="store_true", help="validate snapshots without writing HTML")
    parser.add_argument("--check", action="store_true", help="also require generated HTML to be up to date")
    args = parser.parse_args()
    try:
        snapshots, reports = load_and_validate()
        if args.check:
            if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != render(snapshots):
                raise ValueError("Generated HTML is stale; run python3 scripts/gui_coverage.py")
        elif not args.validate:
            OUTPUT.write_text(render(snapshots), encoding="utf-8")
        print(json.dumps(reports, indent=2))
        if not args.validate and not args.check:
            print(f"Built {OUTPUT.relative_to(ROOT)}")
    except (ValueError, KeyError, OSError, json.JSONDecodeError) as error:
        print(f"GUI coverage: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
