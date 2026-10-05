#!/usr/bin/env python3
"""Dump the reference's shortcut sets for Options > shortcuts: the built-in
(reserved) set names in their sorted order with pretty names and
descriptions, every default set (`ClientDefaults.GetDefaultShortcuts`) as
gesture identities with each command's code and text, the simple command
names, the panel's help text and `GetNonDupeName` examples."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from hydrus.core import HydrusData
from hydrus.client import ClientDefaults
from hydrus.client import ClientApplicationCommand as CAC
from hydrus.client.gui import ClientGUIShortcuts as S

defaults = []
for shortcut_set in ClientDefaults.GetDefaultShortcuts():
    bindings = []
    for (shortcut, command) in shortcut_set:
        simple = command.IsSimpleCommand()
        bindings.append(dict(
            kind=shortcut.shortcut_type,
            key=shortcut.shortcut_key,
            press=shortcut.shortcut_press_type,
            modifiers=sorted(shortcut.modifiers),
            shortcut_text=shortcut.ToString(),
            action=command.GetSimpleAction() if simple else None,
            has_data=simple and command.GetSimpleData() is not None,
            text=command.ToString(),
        ))
    defaults.append(dict(name=shortcut_set.GetName(), bindings=bindings))

out = dict(
    reserved=S.SHORTCUTS_RESERVED_NAMES,
    sorted=S.shortcut_names_sorted,
    pretty_names=S.shortcut_names_to_pretty_names,
    descriptions=S.shortcut_names_to_descriptions,
    defaults=defaults,
    simple_names={str(k): v for (k, v) in CAC.simple_enum_to_str_lookup.items()},
    non_dupe=[
        [name, sorted(existing), HydrusData.GetNonDupeName(name, set(existing))]
        for (name, existing) in [
            ("new shortcuts", []),
            ("new shortcuts", ["new shortcuts"]),
            ("new shortcuts", ["new shortcuts", "new shortcuts (1)"]),
            ("rating keys", ["new shortcuts"]),
        ]
    ],
)
path = os.path.join(HERE, "fixtures/shortcut_sets.json")
with open(path, "w") as stream:
    json.dump(out, stream, indent=2, sort_keys=True)
    stream.write("\n")
print("wrote " + path)
