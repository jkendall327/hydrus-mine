#!/usr/bin/env python3
"""Inventory of widget-level interactions in the reference GUI.

    scripts/interaction_audit.py            markdown table of every handler
    scripts/interaction_audit.py --json     the same as JSON
    scripts/interaction_audit.py --unrecorded-windows
                                            windows none of whose tagged tests load a recording
    scripts/interaction_audit.py --class NAME   only classes whose name contains NAME

The tracker's leaves came from menus, dialogs and options pages, so
interactions that live inside a widget (clicks, context menus, keys, wheel,
drag and drop) were easy to miss. This walks hydrus/client/gui with `ast` and
lists, per class, each Qt event handler it overrides, the mouse buttons, keys
and modifiers it names in that handler (static reading only), signals it
connects that carry interaction (customContextMenuRequested, doubleClicked,
...), and where the class is instantiated elsewhere in the GUI.
Triage against hydrus-rs is done by hand (docs/rust/tracking/interactions.md).
"""

import argparse
import ast
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GUI = ROOT / "hydrus/client/gui"

HANDLERS = {
    "mousePressEvent", "mouseReleaseEvent", "mouseDoubleClickEvent",
    "mouseMoveEvent", "contextMenuEvent", "keyPressEvent", "keyReleaseEvent",
    "wheelEvent", "dragEnterEvent", "dragMoveEvent", "dropEvent", "eventFilter",
}
SIGNALS = {
    "customContextMenuRequested", "doubleClicked", "itemDoubleClicked",
    "itemActivated", "middleClicked",
}
# eventFilter only counts when it looks at mouse or key events.
EVENT_FILTER_TYPES = re.compile(
    r"QEvent\.Type\.(Mouse\w+|Key\w+|Wheel|ContextMenu|Drop|DragEnter\w*)")
BUTTONS = re.compile(r"Qt\.MouseButton\.(\w+)")
KEYS = re.compile(r"Qt\.Key\.(\w+)")
MODS = re.compile(r"Qt\.KeyboardModifier\.(\w+)")
SHORTCUT_CALL = re.compile(r"\b(ProcessShortcut|ProcessKeyPressEvent|ProcessEvent|"
                           r"ProcessMouseEvent|shortcut_set|ShortcutsHandler)\w*")


def py_files():
    return sorted(GUI.rglob("*.py"))


def segment(src, node):
    return ast.get_source_segment(src, node) or ""


def walk_own(cls):
    """ast.walk over a class without descending into nested classes."""
    todo = list(ast.iter_child_nodes(cls))
    while todo:
        n = todo.pop()
        if isinstance(n, ast.ClassDef):
            continue
        yield n
        todo.extend(ast.iter_child_nodes(n))


def scan_class(path, src, cls):
    """Yield one record per interaction-bearing method or connect in `cls`."""
    for node in walk_own(cls):
        if isinstance(node, ast.FunctionDef) and node.name in HANDLERS:
            body = segment(src, node)
            if node.name == "eventFilter" and not EVENT_FILTER_TYPES.search(body):
                continue
            rec = {
                "kind": node.name,
                "line": node.lineno,
                "buttons": sorted(set(BUTTONS.findall(body))),
                "keys": sorted(set(KEYS.findall(body))),
                "modifiers": sorted(set(MODS.findall(body))),
                "events": sorted(set(EVENT_FILTER_TYPES.findall(body))) if node.name == "eventFilter" else [],
                "via_shortcuts": bool(SHORTCUT_CALL.search(body)),
            }
            yield rec
        elif isinstance(node, ast.Attribute) and node.attr in SIGNALS:
            # self.x.customContextMenuRequested.connect(...)
            yield {"kind": "signal:" + node.attr, "line": node.lineno,
                   "buttons": [], "keys": [], "modifiers": [], "events": [],
                   "via_shortcuts": False}
        elif (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr.startswith("EVT_")):
            # QtPorting.WidgetEventFilter: self._filter.EVT_RIGHT_DOWN(self.callback)
            yield {"kind": "filter:" + node.func.attr, "line": node.lineno,
                   "buttons": [], "keys": [], "modifiers": [], "events": [],
                   "via_shortcuts": False}


def base_names(cls):
    out = []
    for b in cls.bases:
        out.append(b.attr if isinstance(b, ast.Attribute) else getattr(b, "id", "?"))
    return out


def collect():
    classes = {}
    sources = {}
    for path in py_files():
        src = path.read_text(encoding="utf-8", errors="replace")
        sources[path] = src
        try:
            tree = ast.parse(src)
        except SyntaxError:
            continue
        for cls in [n for n in ast.walk(tree) if isinstance(n, ast.ClassDef)]:
            recs = list(scan_class(path, src, cls))
            if recs:
                rel = str(path.relative_to(ROOT))
                classes.setdefault(cls.name, []).append(
                    {"file": rel, "line": cls.lineno, "bases": base_names(cls),
                     "interactions": sorted(recs, key=lambda r: r["line"])})
    # Where is each class used? Count constructions/references in other lines.
    uses = defaultdict(list)
    names = set(classes)
    pat = re.compile(r"\b(" + "|".join(map(re.escape, sorted(names))) + r")\b")
    for path, src in sources.items():
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(src.splitlines(), 1):
            if line.lstrip().startswith(("class ", "import ", "from ", "#")):
                continue
            for m in pat.finditer(line):
                uses[m.group(1)].append(f"{rel}:{i}")
    for name, defs in classes.items():
        for d in defs:
            own = d["file"]
            others = [u for u in uses[name] if not u.startswith(own + ":")]
            d["used_in"] = sorted({u.split(":")[0] for u in others})
            d["use_count"] = len(uses[name])
    return classes


def render_md(classes, only):
    lines = ["| Class | Where | Handler | Buttons | Keys | Modifiers | Used in |",
             "|---|---|---|---|---|---|---|"]
    n = 0
    for name in sorted(classes):
        if only and only.lower() not in name.lower():
            continue
        for d in classes[name]:
            for r in d["interactions"]:
                n += 1
                kind = r["kind"] + (" (" + ",".join(r["events"]) + ")" if r["events"] else "")
                used = ", ".join(Path(u).name for u in d["used_in"][:4])
                if len(d["used_in"]) > 4:
                    used += f" +{len(d['used_in']) - 4}"
                lines.append("| {} | {}:{} | {} | {} | {} | {} | {} |".format(
                    name, d["file"].removeprefix("hydrus/client/gui/"), r["line"], kind,
                    ", ".join(r["buttons"]),
                    ", ".join(r["keys"]) + (" +shortcuts" if r["via_shortcuts"] else ""),
                    ", ".join(r["modifiers"]), used))
    lines.append(f"\n{n} handlers in {len(classes)} classes.")
    return "\n".join(lines)


TAG = re.compile(r"//\s*leaf:\s*(.+)$")
RECORDING = re.compile(r"fixture_json\(|oracle/fixtures/|fixtures/")


def tagged_files():
    """leaf id -> set of Rust test files carrying its tag, and which of them load a recording."""
    leaf_files = defaultdict(set)
    recorded = {}
    for path in (ROOT / "crates").rglob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="replace")
        ids = set()
        for line in text.splitlines():
            m = TAG.search(line)
            if m:
                ids.update(re.split(r"[,\s]+", m.group(1).strip()))
        if ids:
            recorded[path] = bool(RECORDING.search(text))
            for i in ids:
                leaf_files[i].add(path)
    return leaf_files, recorded


def unrecorded_windows():
    """Slint windows with no recording-backed GUI test of their own name.

    For each `export component X inherits Window` in crates/hydrus-gui/ui, find
    the GUI tests named after its file (tests/gui/<stem>*.rs) and say whether
    any of them loads a recording (`fixture_json(` / `oracle/fixtures/`), plus
    the leaves they tag. A window with no such test is a candidate for 'no
    test compares its control list, order and visibility with the reference';
    it is a triage list, not a verdict (the controls may be tested under
    another file's name, or be plain strings).
    """
    ui = ROOT / "crates/hydrus-gui/ui"
    tests = sorted((ROOT / "crates/hydrus-gui/tests/gui").glob("*.rs"))
    leaf_files, recorded = tagged_files()
    by_file = defaultdict(set)
    for leaf, files in leaf_files.items():
        for f in files:
            by_file[f].add(leaf)
    rows = []
    for f in sorted(ui.glob("*.slint")):
        text = f.read_text(encoding="utf-8", errors="replace")
        wins = re.findall(r"export component (\w+) inherits (?:Window|Dialog)", text)
        if not wins:
            continue
        stem = f.stem
        mine = [t for t in tests if t.stem == stem or t.stem.startswith(stem + "_")]
        rec = [t for t in mine if RECORDING.search(t.read_text(encoding="utf-8", errors="replace"))]
        if rec:
            continue
        leaves = sorted({l for t in mine for l in by_file.get(t, ())})
        shown = ", ".join(leaves[:5]) + (f" (+{len(leaves) - 5})" if len(leaves) > 5 else "")
        rows.append(f"| {f.name} | {', '.join(wins)} | {len(mine)} | {shown or '(none tagged)'} |")
    return "\n".join(["| File | Window | GUI test files of that name | Leaves they tag |",
                      "|---|---|---|---|"] + rows)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--class", dest="only")
    ap.add_argument("--unrecorded-windows", action="store_true",
                    help="windows none of whose tagged tests load a recording")
    a = ap.parse_args()
    if a.unrecorded_windows:
        print(unrecorded_windows())
        return
    classes = collect()
    if a.json:
        json.dump(classes, sys.stdout, indent=1)
    else:
        print(render_md(classes, a.only))


if __name__ == "__main__":
    main()
