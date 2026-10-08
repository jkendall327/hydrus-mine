#!/usr/bin/env python3
"""What is left to port, by workstream, counted from the tests.

    scripts/track.py                      summary per workstream
    scripts/track.py next WORKSTREAM [-n N] [--all]
                                          the next leaves to work on (missing and
                                          partial first, then implemented-but-untagged)
    scripts/track.py show LEAF_ID         one leaf in full
    scripts/track.py check                fail on tags naming unknown leaf IDs (CI)

A leaf is *done* when a test carries the comment `// leaf: <id>` (several IDs
may be separated by commas or spaces), or when it was signed off under the
retired per-leaf ledger (state "carried"). Leaf data is
docs/rust/tracking/leaves.json; see docs/rust/tracking/README.md.
"""

import argparse
import json
import signal
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEAVES = ROOT / "docs/rust/tracking/leaves.json"
TAG = re.compile(r"//\s*leaf:\s*(.+)$")
ORDER = {"missing": 0, "partial": 1, "implemented": 2}


def load():
    return json.loads(LEAVES.read_text(encoding="utf-8"))["leaves"]


def tags():
    """leaf id -> list of 'path:line' where a test tags it."""
    found = defaultdict(list)
    for path in (ROOT / "crates").rglob("*.rs"):
        rel = path.relative_to(ROOT)
        for n, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            m = TAG.search(line)
            if m:
                for leaf in re.split(r"[,\s]+", m.group(1).strip()):
                    if leaf:
                        found[leaf].append(f"{rel}:{n}")
    return found


def status(leaf, tagged):
    if leaf["id"] in tagged:
        return "done"
    if leaf["state"] == "carried":
        return "done"
    return leaf["state"]


def summary(leaves, tagged):
    by = defaultdict(Counter)
    for leaf in leaves:
        if leaf["priority"] == "out-of-scope":
            by[leaf["workstream"]]["out-of-scope"] += 1
            continue
        by[leaf["workstream"]][status(leaf, tagged)] += 1
    cols = ["done", "implemented", "partial", "missing", "out-of-scope"]
    print(f"{'workstream':16s}" + "".join(f"{c:>13s}" for c in cols) + f"{'total':>8s}")
    total = Counter()
    for ws in sorted(by):
        total.update(by[ws])
        print(f"{ws:16s}" + "".join(f"{by[ws][c]:13d}" for c in cols) + f"{sum(by[ws].values()):8d}")
    print(f"{'all':16s}" + "".join(f"{total[c]:13d}" for c in cols) + f"{sum(total.values()):8d}")
    tagged_count = sum(1 for leaf in leaves if leaf["id"] in tagged)
    print(f"\n{tagged_count} leaves tagged in tests; "
          f"{sum(1 for l in leaves if l['state'] == 'carried' and l['id'] not in tagged)} carried over untagged.")
    print("implemented = written before 2026-10-08 but no tagged test yet: find or write the test, then tag it.")


def show(leaf, tagged):
    print(f"{leaf['id']}  [{leaf['workstream']}, {status(leaf, tagged)}, priority {leaf['priority']}]")
    print(f"  {leaf['path']} > {leaf['label']}")
    if leaf.get("reference"):
        print(f"  reference: {leaf['reference']}")
    if leaf.get("native"):
        print(f"  native:    {leaf['native']}")
    for where in tagged.get(leaf["id"], []):
        print(f"  tagged:    {where}")
    for note in leaf.get("notes", []):
        print(f"  - {note}")


def main():
    # (quiet when piped into `head`)
    signal.signal(signal.SIGPIPE, signal.SIG_DFL)
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd")
    nx = sub.add_parser("next")
    nx.add_argument("workstream")
    nx.add_argument("-n", type=int, default=15)
    nx.add_argument("--all", action="store_true", help="include low priority")
    sh = sub.add_parser("show")
    sh.add_argument("leaf")
    sub.add_parser("check")
    args = ap.parse_args()

    leaves = load()
    tagged = tags()
    ids = {leaf["id"] for leaf in leaves}

    if args.cmd == "check":
        unknown = {t: w for t, w in tagged.items() if t not in ids}
        for t, where in sorted(unknown.items()):
            print(f"unknown leaf id {t!r} at {', '.join(where)}", file=sys.stderr)
        sys.exit(1 if unknown else 0)
    if args.cmd == "show":
        match = [leaf for leaf in leaves if leaf["id"] == args.leaf]
        if not match:
            sys.exit(f"no leaf {args.leaf}")
        show(match[0], tagged)
        return
    if args.cmd == "next":
        todo = [
            leaf for leaf in leaves
            if leaf["workstream"] == args.workstream
            and status(leaf, tagged) != "done"
            and leaf["priority"] != "out-of-scope"
            and (args.all or leaf["priority"] != "low")
        ]
        if not todo and not any(l["workstream"] == args.workstream for l in leaves):
            sys.exit(f"no workstream {args.workstream}")
        todo.sort(key=lambda l: (ORDER[status(l, tagged)], l["path"], l["label"]))
        for leaf in todo[: args.n]:
            show(leaf, tagged)
            print()
        print(f"({len(todo)} remaining in {args.workstream})")
        return
    summary(leaves, tagged)


if __name__ == "__main__":
    main()
