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
...), where the class is instantiated elsewhere in the GUI, and the handlers a
subclass inherits from a reference-side base class (a hover or dialog list
that only subclasses ListBoxTags still shows ListBoxTags' middle-click).
ShortcutsHandler( and QShortcut( attachments are listed in a second table.
What it cannot see is in docs/rust/tracking/interaction-audit.md.
Triage against hydrus-rs is done by hand (docs/rust/tracking/interaction-audit.md).
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


def call_name(node):
    f = node.func
    return f.attr if isinstance(f, ast.Attribute) else getattr(f, "id", "")


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
        elif isinstance(node, ast.Call) and call_name(node) in ("ShortcutsHandler", "QShortcut"):
            args = ", ".join(segment(src, a) for a in node.args[:3])
            yield {"kind": "shortcuts:" + call_name(node), "line": node.lineno,
                   "buttons": [], "keys": [], "modifiers": [], "events": [],
                   "via_shortcuts": True, "args": args[:80]}
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
    classes = {}   # name -> [def] for classes with their own interactions
    table = defaultdict(list)   # every class: name -> [{file, line, bases}]
    sources = {}
    for path in py_files():
        src = path.read_text(encoding="utf-8", errors="replace")
        sources[path] = src
        try:
            tree = ast.parse(src)
        except SyntaxError:
            continue
        rel = str(path.relative_to(ROOT))
        for cls in [n for n in ast.walk(tree) if isinstance(n, ast.ClassDef)]:
            recs = sorted(scan_class(path, src, cls), key=lambda r: r["line"])
            d = {"file": rel, "line": cls.lineno, "bases": base_names(cls), "interactions": recs}
            table[cls.name].append(d)
            if recs:
                classes.setdefault(cls.name, []).append(d)
    # Where is each class used? Any mention outside class/import lines.
    uses = defaultdict(list)
    word = re.compile(r"\b\w+\b")
    for path, src in sources.items():
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(src.splitlines(), 1):
            if line.lstrip().startswith(("class ", "import ", "from ", "#")):
                continue
            for w in set(word.findall(line)):
                if w in table:
                    uses[w].append(f"{rel}:{i}")

    def used_in(name, own_file):
        others = [u for u in uses[name] if not u.startswith(own_file + ":")]
        return sorted({u.split(":")[0] for u in others})

    for name, defs in classes.items():
        for d in defs:
            d["used_in"] = used_in(name, d["file"])
            d["inherited"] = False

    # Subclasses: inherit every handler kind they do not override themselves.
    def ancestors(name, seen):
        for d in table.get(name, []):
            for b in d["bases"]:
                if b not in seen:
                    seen.add(b)
                    yield b
                    yield from ancestors(b, seen)

    for name, defs in table.items():
        for d in defs:
            own_kinds = {r["kind"] for r in d["interactions"]}
            inherited = []
            for anc in ancestors(name, set()):
                for ad in classes.get(anc, []):
                    for r in ad["interactions"]:
                        if r["kind"] not in own_kinds and not r["kind"].startswith(("shortcuts:", "filter:")):
                            inherited.append(dict(r, inherited_from=f"{anc} ({ad['file'].removeprefix('hydrus/client/gui/')}:{r['line']})"))
                            own_kinds.add(r["kind"])
            if inherited and used_in(name, d["file"]):
                entry = {"file": d["file"], "line": d["line"], "bases": d["bases"],
                         "interactions": inherited, "used_in": used_in(name, d["file"]),
                         "inherited": True}
                classes.setdefault(name, []).append(entry)
    return classes


def render_md(classes, only):
    lines = ["| Class | Where | Handler | Buttons | Keys | Modifiers | Used in |",
             "|---|---|---|---|---|---|---|"]
    shortcuts = ["| Class | Where | Attached | Arguments |", "|---|---|---|---|"]
    n = 0
    for name in sorted(classes):
        if only and only.lower() not in name.lower():
            continue
        for d in classes[name]:
            where = d["file"].removeprefix("hydrus/client/gui/")
            used = ", ".join(Path(u).name for u in d["used_in"][:4])
            if len(d["used_in"]) > 4:
                used += f" +{len(d['used_in']) - 4}"
            for r in d["interactions"]:
                if r["kind"].startswith("shortcuts:"):
                    shortcuts.append("| {} | {}:{} | {} | {} |".format(
                        name, where, r["line"], r["kind"].split(":")[1], r.get("args", "")))
                    continue
                n += 1
                kind = r["kind"] + (" (" + ",".join(r["events"]) + ")" if r["events"] else "")
                if r.get("inherited_from"):
                    kind += " [inherited from " + r["inherited_from"] + "]"
                lines.append("| {} | {}:{} | {} | {} | {} | {} | {} |".format(
                    name, where, d["line"] if d["inherited"] else r["line"], kind,
                    ", ".join(r["buttons"]),
                    ", ".join(r["keys"]) + (" +shortcuts" if r["via_shortcuts"] else ""),
                    ", ".join(r["modifiers"]), used))
    lines.append(f"\n{n} handlers (own and inherited) in {len(classes)} classes.")
    lines.append("\n### Shortcut attachments (ShortcutsHandler, QShortcut)\n")
    lines.extend(shortcuts)
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
    the GUI tests named after its file (tests/gui/<stem>*.rs) or mentioning its
    window type, and say whether
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
    texts = {t: t.read_text(encoding="utf-8", errors="replace") for t in tests}
    rows = []
    for f in sorted(ui.glob("*.slint")):
        text = f.read_text(encoding="utf-8", errors="replace")
        wins = re.findall(r"export component (\w+) inherits (?:Window|Dialog)", text)
        if not wins:
            continue
        stem = f.stem
        names = [re.compile(r"\b" + re.escape(w) + r"\b") for w in wins]
        mine = [t for t in tests
                if t.stem == stem or t.stem.startswith(stem + "_")
                or any(n.search(texts[t]) for n in names)]
        rec = [t for t in mine if RECORDING.search(texts[t])]
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
