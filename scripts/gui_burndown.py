#!/usr/bin/env python3
"""Count original GUI completions separately from pending agent proposals.

This reads the frozen overnight IDs, the last validated checkpoint and the
continuation packets. It does not assess behavior or promote coverage statuses.
Parents, shared-editor aliases, evidence-only changes and new inventory IDs do
not count as implemented original leaves. Use --commit to inspect a pushed
checkpoint without including newer work in the checkout.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
DATA = "docs/rust/gui-coverage"
STRUCTURAL = {"root", "menu", "submenu", "feature_family", "family"}


def report(commit=None):
    def read(path):
        if commit:
            return json.loads(subprocess.check_output(
                ["git", "show", f"{commit}:{path}"], cwd=ROOT, text=True,
            ))
        return json.loads((ROOT / path).read_text())

    baseline = read(f"{DATA}/overnight/baseline.json")
    validated = read(f"{DATA}/overnight/progress.json")
    inventory = read(f"{DATA}/reference-inventory.json")
    nodes = {node["id"]: node for node in inventory["nodes"]}
    children = Counter(node["parent_id"] for node in nodes.values())
    completed = set(validated["completed_feature_ids"])
    if len(completed) != validated["concrete_implementation_completions"]:
        raise ValueError("Validated checkpoint contains a stale completion count")
    if commit:
        paths = subprocess.check_output(
            ["git", "ls-tree", "-r", "--name-only", commit, f"{DATA}/parity"],
            cwd=ROOT, text=True,
        ).splitlines()
        paths = sorted(path for path in paths if path.endswith(".json"))
    else:
        paths = sorted(str(path.relative_to(ROOT)) for path in
                       (ROOT / DATA / "parity").glob("*.json"))
    claims, proposed, improvements, excluded = {}, [], [], []
    for path in paths:
        packet = read(path)
        for claim in packet.get("claims", packet.get("entries", [])):
            key = claim.get("reference_id", claim.get("id", claim.get("originalID")))
            if key not in baseline["reference"]:
                raise ValueError(f"Not an original reference ID: {key}")
            if key in claims:
                raise ValueError(f"Duplicate original reference proposal: {key}")
            claims[key] = claim
            status = claim.get("proposed_status", claim.get("proposedStatus"))
            before = baseline["reference"][key]
            declared = claim.get("previous_status", claim.get("before"))
            if declared != before:
                raise ValueError(f"Frozen baseline mismatch for {key}: {declared} vs {before}")
            if status not in {"missing", "partial", "first_pass"}:
                raise ValueError(f"Invalid proposal status for {key}: {status}")
            node = nodes[key]
            row = {"id": key, "before": before, "after": status,
                   "manifest": path, "label": node["label"]}
            if key in completed:
                continue
            alias = node.get("target_id") or node.get("canonical_editor")
            parent = children[key] or node["kind"] in STRUCTURAL
            evidence_only = claim.get("change_kind", "implementation").startswith("evidence")
            if status == "first_pass" and before in {"missing", "partial"}:
                if alias or parent or evidence_only or claim.get("countAsCompletedLeaf") is False:
                    row["reason"] = "alias" if alias else "parent" if parent else "evidence_or_scope"
                    excluded.append(row)
                else:
                    proposed.append(row)
            elif before in {"missing", "partial"} and status == "partial":
                improvements.append(row)
    return {
        "baseline": baseline["git_head"],
        "baseline_counts": dict(Counter(baseline["reference"].values())),
        "inspected_commit": commit,
        "validated_source_commit": validated["source_commit"],
        "validated_ci": validated["ci_evidence"]["url"],
        "validated_original_leaf_completions": len(completed),
        "validated_inventory_counts": dict(Counter(node["status"] for node in nodes.values())),
        "additional_proposed_original_leaf_completions": len(proposed),
        "additional_proposed_by_manifest": dict(Counter(Path(row["manifest"]).stem for row in proposed)),
        "additional_partial_improvements": len(improvements),
        "excluded_alias_parent_or_evidence_promotions": len(excluded),
        "proposals": proposed, "partial_improvements": improvements, "excluded": excluded,
        "proposed_total_if_validated": len(completed) + len(proposed),
        "method": "Original IDs only; authored proposals remain pending until integrated CI and source review. Inventory counts also include historical evidence and parent assessments.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", help="read packets and validated ledger from this Git commit")
    parser.add_argument("--json", action="store_true", help="include individual original IDs")
    args = parser.parse_args()
    data = report(args.commit)
    if not args.json:
        for key in ("proposals", "partial_improvements", "excluded"):
            data.pop(key)
    print(json.dumps(data, indent=2))


if __name__ == "__main__":
    main()
