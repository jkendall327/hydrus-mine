#!/usr/bin/env python3
"""Validate curated GUI snapshots and build the standalone offline explorer.

No GUI behavior is assessed by this script. Source anchors are checked against
each inventory's pinned Git commit, rather than the current working tree.
"""

import argparse
from collections import Counter
from functools import lru_cache
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "docs/rust/gui-coverage"
OUTPUT = ROOT / "docs/rust/gui-progress.html"
STATUSES = {"first_pass", "partial", "missing", "unassessed"}
STRUCTURAL_KINDS = {"root", "menu", "submenu", "feature_family", "family"}


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


def anchor_objects(value):
    """Yield mutable source records without copying source text into JSON."""
    if isinstance(value, dict):
        if (value.get("path") or value.get("relativepath")) and "line" in value:
            yield value
        for child in value.values():
            yield from anchor_objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from anchor_objects(child)


def line_digest(line):
    return hashlib.sha256(line.encode("utf-8")).hexdigest()


@lru_cache(maxsize=None)
def remap_line(old_commit, new_commit, path, line):
    """Map an unchanged source line only when its file/line/context is unique.

    Changed or ambiguous source text needs human review; no nearest-line guess
    or unchecked replacement of git_head is permitted.
    """
    old = source_lines(old_commit, path)
    new = source_lines(new_commit, path)
    if not 1 <= line <= len(old):
        raise ValueError(f"invalid original anchor {path}:{line}")
    if old == new:
        return line, "identical_git_blob"
    text = old[line - 1]
    candidates = [i + 1 for i, candidate in enumerate(new) if candidate == text]
    if len(candidates) == 1:
        return candidates[0], "unique_unchanged_line"
    contexts = []
    for candidate in candidates:
        offset = candidate - line
        for radius in (1, 2, 3, 5, 10):
            start = max(0, line - 1 - radius)
            stop = min(len(old), line + radius)
            if start + offset < 0 or stop + offset > len(new):
                continue
            if old[start:stop] == new[start + offset:stop + offset]:
                contexts.append(candidate)
                break
    if len(contexts) == 1:
        return contexts[0], "unique_unchanged_context"
    raise ValueError(
        f"manual anchor review required: {path}:{line}; "
        f"matching lines at target: {candidates[:20]}"
    )


def repin(data, target):
    """Return a fully checked copy and a source remapping audit trail."""
    data = json.loads(json.dumps(data))
    original = data["git_head"]
    report = {"from_commit": original, "to_commit": target, "anchors": []}
    unique = set()
    for anchor in anchor_objects(data):
        path = anchor.get("path", anchor.get("relativepath"))
        previous_commit = anchor.pop("anchor_git_head", original)
        previous_line = anchor["line"]
        line, method = remap_line(previous_commit, target, path, previous_line)
        anchor["line"] = line
        anchor["line_sha256"] = line_digest(source_lines(target, path)[line - 1])
        key = (previous_commit, path, previous_line)
        if key not in unique:
            unique.add(key)
            report["anchors"].append({
                "path": path, "from_commit": previous_commit,
                "old_line": previous_line, "line": line,
                "method": method, "line_sha256": anchor["line_sha256"],
            })
    data["git_head"] = target
    provenance_map = data.setdefault("module_provenance", {})
    for path in {anchor.get("path", anchor.get("relativepath")) for anchor in anchor_objects(data)}:
        provenance_map.setdefault(path, {})
    for path, provenance in provenance_map.items():
        result = subprocess.run(
            ["git", "show", f"{target}:{path}"], cwd=ROOT,
            capture_output=True, check=True,
        )
        provenance["sha256"] = hashlib.sha256(result.stdout).hexdigest()
        provenance["line_count"] = len(result.stdout.decode("utf-8").splitlines())
    data["source_anchor_audit"] = {
        "git_head": target, "unique_anchors_checked": len(unique),
        "method": "Git blob equality or uniquely unchanged line/context; line fingerprints verified",
        "prior_git_head": original,
    }
    return data, report


def node_metrics(data):
    """Use the viewer's definitions, including cycle-safe shared reachability."""
    by_id = {node["id"]: node for node in data["nodes"]}
    children = {}
    for node in data["nodes"]:
        children.setdefault(node["parent_id"], []).append(node["id"])

    def aliases(node):
        return list(filter(None, [node.get("target_id"), node.get("canonical_editor")])) + node.get("editor_refs", [])

    def concrete(node):
        return not node.get("target_id") and not node.get("canonical_editor") and node["kind"] not in STRUCTURAL_KINDS

    def resolve(key):
        seen = set()
        while key not in seen:
            seen.add(key)
            node = by_id[key]
            target = node.get("target_id") or node.get("canonical_editor")
            if not target:
                return key
            key = target
        return key

    result = {}
    for node in data["nodes"]:
        descendants = []
        pending = list(children.get(node["id"], []))
        descendant_seen = set()
        while pending:
            key = pending.pop()
            if key in descendant_seen:
                raise ValueError(f"parent cycle while counting descendants: {key}")
            descendant_seen.add(key)
            descendants.append(key)
            pending.extend(children.get(key, []))
        seen, reachable = set(), set()
        pending = [node["id"]]
        while pending:
            key = pending.pop()
            if key in seen:
                continue
            seen.add(key)
            canonical = resolve(key)
            if concrete(by_id[canonical]):
                reachable.add(canonical)
            pending.extend(children.get(key, []) + aliases(by_id[key]))
        result[node["id"]] = {
            "descendant_count": len(descendants),
            "descendant_status_counts": dict(Counter(by_id[key]["status"] for key in descendants)),
            "unique_feature_descendant_count": sum(concrete(by_id[key]) for key in descendants),
            "unique_reachable_feature_count_including_shared": len(reachable),
        }
    return result


def audit_diagnostics(data, name):
    """Surface opaque windows and weak green assessments for source review."""
    by_id = {node["id"]: node for node in data["nodes"]}
    metrics = node_metrics(data)
    weak = []
    for node in data["nodes"]:
        if node["status"] != "first_pass":
            continue
        reasons = []
        if not isinstance(node.get("assessment"), str) or not node["assessment"].strip():
            reasons.append("no per-node assessment rationale")
        if not node.get("reference_source"):
            reasons.append("no reference source anchor")
        if "enclosing_module" in node.get("native_source_scope", ""):
            reasons.append("native anchor is only an enclosing module")
        if not node.get("evidence"):
            reasons.append("no recorded supporting evidence")
        if name == "native":
            evidence_paths = [source.get("path", source.get("relativepath", ""))
                              for source in anchor_objects(node.get("evidence", []))]
            if not any("/tests/" in path or path.startswith("oracle/") for path in evidence_paths):
                reasons.append("native first-pass definition requires a scoped regression/recording anchor; only source evidence recorded")
        if reasons:
            weak.append({"id": node["id"], "reasons": reasons})
    report = {"weak_first_pass": weak}
    if name != "native":
        return report
    command = ["git", "ls-tree", "-r", "--name-only", data["git_head"], "crates/hydrus-gui/ui"]
    paths = subprocess.check_output(command, cwd=ROOT).decode("utf-8").splitlines()
    windows, opaque = [], []
    for path in paths:
        if not path.endswith(".slint"):
            continue
        text = "\n".join(source_lines(data["git_head"], path))
        for match in re.finditer(r"export component (\w+) inherits Window", text):
            component = match.group(1)
            occurrences = []
            for node in by_id.values():
                source = node.get("native_source") or {}
                if node.get("component") != component or not source.get("path"):
                    continue
                # Audited nodes may anchor a constructor/callback module rather
                # than the Slint declaration. Require that cited file to name
                # the exported component before accepting the association.
                module = "\n".join(source_lines(data["git_head"], source["path"]))
                if re.search(r"\b" + re.escape(component) + r"\b", module):
                    occurrences.append(node)
            navigable = [node for node in occurrences if node["status"] != "unassessed" and (metrics[node["id"]]["descendant_count"] or node.get("target_id") or node.get("canonical_editor") or node.get("editor_refs"))]
            record = {"path": path, "component": component, "node_ids": [node["id"] for node in occurrences]}
            windows.append(record)
            if not navigable:
                opaque.append(record)
    report.update({"exported_window_count": len(windows), "opaque_native_windows": opaque,
                   "unassessed_nodes": [node["id"] for node in data["nodes"] if node["status"] == "unassessed"],
                   "catch_all_nodes": [node["id"] for node in data["nodes"] if node["id"] == "catalog.other"]})
    return report


def refresh_aggregates(data, name):
    """Refresh recorded navigation counts without changing any assessment."""
    metrics = node_metrics(data)
    for node in data["nodes"]:
        node.update(metrics[node["id"]])
    data["aggregation_schema"] = "concrete_features_v2"
    counts = {
        "total": len(data["nodes"]),
        "by_status": dict(Counter(node["status"] for node in data["nodes"])),
        "by_kind": dict(Counter(node["kind"] for node in data["nodes"])),
    }
    by_id = {node["id"]: node for node in data["nodes"]}
    depth = 0
    for node in data["nodes"]:
        cursor, current = node, 0
        seen = set()
        while cursor["parent_id"]:
            if cursor["id"] in seen:
                raise ValueError(f"parent cycle while counting depth: {cursor['id']}")
            seen.add(cursor["id"])
            current += 1
            cursor = by_id[cursor["parent_id"]]
        depth = max(depth, current)
    if name == "native":
        counts["max_parent_depth"] = depth
        data["counts"] = counts
        if data.get("window_depth_audit"):
            diagnostics = audit_diagnostics(data, name)
            data["window_depth_audit"].update({
                "source_git_head": data["git_head"],
                "exported_window_count": diagnostics["exported_window_count"],
            })
    else:
        summary = data["summary"]
        summary.update({
            "node_count": counts["total"], "status_counts": counts["by_status"],
            "kind_counts": counts["by_kind"], "maximum_tree_depth": depth,
            "reference_node_count": sum(bool(node.get("target_id")) for node in data["nodes"]),
        })
        for key in list(summary.get("deep_feature_families", {})):
            if key not in metrics:
                del summary["deep_feature_families"][key]
                continue
            summary["deep_feature_families"][key] = {
                "tree_descendants": metrics[key]["descendant_count"],
                "unique_reachable_including_shared": metrics[key]["unique_reachable_feature_count_including_shared"],
            }


def refresh_mappings(snapshots):
    """Keep only reviewed-candidate semantics and recompute cached statuses."""
    reference = snapshots["reference"]
    native = {node["id"]: node for node in snapshots["native"]["nodes"]}
    proposals = []
    mismatches = []
    for node in reference["nodes"]:
        candidates = list(dict.fromkeys(node.get("native_id_candidates", [])))
        node["native_id_candidates"] = candidates
        for key in candidates:
            if key not in native:
                raise ValueError(f"unresolved candidate after audit: {node['id']} -> {key}")
            proposals.append({"reference_id": node["id"], "native_id": key,
                              "relation": "candidate_same_feature", "review_required": True})
            if node["status"] != native[key]["status"]:
                mismatches.append({"reference_id": node["id"], "native_id": key,
                                   "reference_status": node["status"], "native_status": native[key]["status"]})
    mapping = reference.setdefault("native_mapping_proposal", {})
    mapping.update({"proposals": proposals, "proposal_count": len(proposals), "status_mismatches": mismatches})


def apply_patches(snapshots, paths):
    """Apply curated field edits, marking inherited versus newly inspected anchors.

    Full-node audit updates may inherit old line numbers. Those inherited
    anchors retain their original commit until the separate repinning pass.
    Newly supplied anchors use the audit's declared source-inspection commit.
    """
    touched = {"native": set(), "reference": set()}
    reports = []
    for path in paths:
        patch = json.loads(path.read_text(encoding="utf-8"))
        baseline = patch["baseline_git_head"]
        report = {"patch": str(path.relative_to(ROOT)), "baseline_git_head": baseline}
        for name in ("native", "reference"):
            data = snapshots[name]
            by_id = {node["id"]: node for node in data["nodes"]}
            changes = patch.get(name, {})
            additions = changes.get("additions", [])
            updates = changes.get("updates", [])
            deletions = changes.get("deletions", [])
            report[name] = {"additions": len(additions), "updates": len(updates), "deletions": len(deletions)}
            for edit in updates:
                key = edit["id"]
                if key in touched[name] or key not in by_id:
                    raise ValueError(f"conflicting or unknown audit update: {name}:{key}")
                previous = by_id[key]
                inherited = {(source.get("path", source.get("relativepath")), source["line"]): source.get("anchor_git_head", data["git_head"]) for source in anchor_objects(previous)}
                updated = json.loads(json.dumps(previous))
                updated.update(edit)
                for source in anchor_objects(updated):
                    signature = (source.get("path", source.get("relativepath")), source["line"])
                    # An auditor's explicit source snapshot wins, even when a
                    # path/line coincides with an older inherited anchor.
                    source.setdefault("anchor_git_head", inherited.get(signature, baseline))
                by_id[key] = updated
                touched[name].add(key)
            for node in additions:
                key = node["id"]
                if key in by_id or key in touched[name]:
                    raise ValueError(f"duplicate audit addition: {name}:{key}")
                node = json.loads(json.dumps(node))
                for source in anchor_objects(node):
                    source.setdefault("anchor_git_head", baseline)
                by_id[key] = node
                touched[name].add(key)
            for key in deletions:
                if key not in by_id or key in touched[name]:
                    raise ValueError(f"conflicting or unknown audit deletion: {name}:{key}")
                del by_id[key]
                touched[name].add(key)
            data["nodes"] = list(by_id.values())
        reports.append(report)
    return reports


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
    if data.get("source_anchor_audit", {}).get("git_head", commit) != commit:
        raise ValueError(f"{name}: source audit pins a different commit")
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
    for anchor in anchor_objects(data):
        path = anchor.get("path", anchor.get("relativepath"))
        expected = anchor.get("line_sha256")
        if expected and expected != line_digest(source_lines(commit, path)[anchor["line"] - 1]):
            raise ValueError(f"{name}: stale source fingerprint {path}:{anchor['line']}")
        if data.get("source_anchor_audit") and not expected:
            raise ValueError(f"{name}: missing verified source fingerprint {path}:{anchor['line']}")
    actual = dict(Counter(n["status"] for n in nodes))
    recorded = data.get("counts", {}).get("by_status") or data.get("summary", {}).get("status_counts")
    if recorded != actual:
        raise ValueError(f"{name}: stale recorded status counts")
    metrics = node_metrics(data)
    for node in nodes:
        for field, expected in metrics[node["id"]].items():
            if field in node and data.get("aggregation_schema") == "concrete_features_v2" and node[field] != expected:
                raise ValueError(f"{name}: stale {field} at {node['id']}")
    if data.get("aggregation_schema") == "concrete_features_v2":
        refreshed = json.loads(json.dumps(data))
        refresh_aggregates(refreshed, name)
        field = "counts" if name == "native" else "summary"
        if data[field] != refreshed[field]:
            raise ValueError(f"{name}: stale recorded {field} aggregation")
    for path, provenance in data.get("module_provenance", {}).items():
        blob = subprocess.check_output(["git", "show", f"{commit}:{path}"], cwd=ROOT)
        if provenance.get("sha256") != hashlib.sha256(blob).hexdigest():
            raise ValueError(f"{name}: stale module fingerprint {path}")
        if provenance.get("line_count") != len(blob.decode("utf-8").splitlines()):
            raise ValueError(f"{name}: stale module line count {path}")
    diagnostics = audit_diagnostics(data, name)
    if data.get("window_depth_audit") and diagnostics.get("opaque_native_windows"):
        raise ValueError(f"{name}: native window depth missing: {diagnostics['opaque_native_windows']}")
    if data.get("window_depth_audit") and diagnostics.get("catch_all_nodes"):
        raise ValueError(f"{name}: catalog catch-all returned")
    window_audit = data.get("window_depth_audit")
    if window_audit and (window_audit.get("source_git_head") != commit or
                         window_audit.get("exported_window_count") != diagnostics.get("exported_window_count")):
        raise ValueError(f"{name}: stale exported-window source audit")
    return {"nodes": len(nodes), "source_anchors": count, "statuses": actual, "audit": diagnostics}


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
    mapping = snapshots["reference"].get("native_mapping_proposal", {})
    if mapping.get("proposal_count") != len(mapping.get("proposals", [])):
        raise ValueError("Stale native proposal count")
    reference = {node["id"]: node for node in snapshots["reference"]["nodes"]}
    native = {node["id"]: node for node in snapshots["native"]["nodes"]}
    for row in mapping.get("status_mismatches", []):
        if row["reference_status"] != reference[row["reference_id"]]["status"] or row["native_status"] != native[row["native_id"]]["status"]:
            raise ValueError(f"Stale candidate status mismatch: {row['reference_id']}")
    if snapshots["reference"].get("aggregation_schema") == "concrete_features_v2":
        refreshed = json.loads(json.dumps(snapshots))
        refresh_mappings(refreshed)
        for field in ("proposals", "proposal_count", "status_mismatches"):
            if mapping.get(field) != refreshed["reference"]["native_mapping_proposal"][field]:
                raise ValueError(f"Stale candidate mapping {field}")
    import gui_verify
    gui_verify.check_working(snapshots["reference"])
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
    parser.add_argument("--repin", metavar="COMMIT", help="verify/remap all anchors and refresh counts before pinning this commit")
    parser.add_argument("--apply-audit", action="append", default=[], metavar="PATCH", help="apply a curated audit patch before --repin; repeat for separate owners")
    args = parser.parse_args()
    try:
        if args.apply_audit and not args.repin:
            raise ValueError("--apply-audit requires --repin so inherited/new anchors are checked")
        if args.repin:
            if args.validate or args.check:
                raise ValueError("--repin writes checked snapshots; use --check in a separate invocation")
            target = subprocess.check_output(
                ["git", "rev-parse", "--verify", args.repin + "^{commit}"], cwd=ROOT,
            ).decode("utf-8").strip()
            raw = {name: json.loads((DATA / f"{name}-inventory.json").read_text(encoding="utf-8")) for name in ("reference", "native")}
            patch_report = apply_patches(raw, [ROOT / path for path in args.apply_audit])
            remapped, anchor_report = {}, {}
            for name, data in raw.items():
                remapped[name], anchor_report[name] = repin(data, target)
                refresh_aggregates(remapped[name], name)
            refresh_mappings(remapped)
            for name, data in remapped.items():
                validate(data, name)
            # Finish all independent checks before writing either snapshot.
            for name, data in remapped.items():
                (DATA / f"{name}-inventory.json").write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
            audit_dir = DATA / "audit"
            audit_dir.mkdir(exist_ok=True)
            audit_path = audit_dir / f"anchor-remap-{target[:8]}.json"
            pass_number = 2
            while audit_path.exists():
                audit_path = audit_dir / f"anchor-remap-{target[:8]}-pass{pass_number}.json"
                pass_number += 1
            audit_path.write_text(
                json.dumps({"target_git_head": target, "patches": patch_report, "inventories": anchor_report}, indent=2) + "\n", encoding="utf-8",
            )
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
