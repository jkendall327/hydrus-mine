# GUI migration explorer

Open [gui-progress.html](../gui-progress.html) in a browser. It is a standalone,
offline map of the Rust reimplementation of Hydrus. JSON, styles and code are
embedded; there are no external assets, runtime requests or server requirements.
Source links open GitHub only when clicked.

If a managed browser blocks `file://`, serve the same artifact locally:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/rust
```

Then open `http://127.0.0.1:8765/gui-progress.html`. Stop the server when finished.

The default reference view contains 250 selected user-visible features. The
optional native view contains 340 menu entries and native window/component
occurrences. These are different inventories with different granularity, not
equivalent denominators. Status totals include organizational containers and
shared references, so they must not become a whole-client parity percentage.

Use search for labels, hierarchy paths, source paths or limitations. Status
filters retain the ancestors of matching nodes and expand those paths. Expand,
collapse and reset control the tree. `/` focuses search; Escape clears a focused
search. Buttons and source links work with the keyboard. Shared editor buttons
jump to canonical nodes without copying their subtrees. Candidate native links
switch views and remain proposals requiring review.

Nested descendants follow containment and exclude the selected node. Unique
nested features exclude root/menu/submenu/family containers and shared aliases.
Reachable features follow canonical/editor links with cycle protection, including
the selected concrete feature. Alternative entrypoints are incoming navigation
links and do not add descendants. These counts are computed from the displayed
snapshot; they are not copied from its preparatory aggregate estimates.

## Maintain the snapshots

The canonical inputs are [reference-inventory.json](reference-inventory.json)
and [native-inventory.json](native-inventory.json). Their provenance and status
definitions are preserved from the read-only inventories. Source excerpts and
absolute workspace paths were removed. The snapshots retain their individual
`git_head` commits; every source/evidence URL pins its inventory's commit. Module
hashes record preparatory provenance and are not claims of whole-file behavioral
parity or a requirement that the current checkout remain unchanged.

1. Read the reference behavior, native implementation, limitations and relevant
   oracle/regression evidence before changing an assessment. Callback presence
   and existing evidence filenames alone do not establish behavioral parity.
2. Update the canonical JSON with stable IDs, labels, parents, statuses,
   repository-relative source paths and one-based lines. Preserve limitations
   and shared references; give candidate mappings `review_required: true`.
3. When refreshing sources, choose a committed snapshot and update `git_head`
   with the new anchors. Do not simply replace the commit while retaining
   unchecked line numbers. Reconcile counts and provenance deliberately.
4. Run `python3 scripts/gui_coverage.py` from the repository root, then inspect
   the generated HTML in desktop and narrow browser layouts. Commit both the
   input changes and regenerated HTML.

`python3 scripts/gui_coverage.py --validate` checks IDs, parent acyclicity,
statuses, shared links, candidate mappings, recorded status totals and source
anchor bounds against pinned Git blobs. `--check` also verifies that the checked-in
HTML is current. The script uses only the Python standard library and Git. It
does not run Cargo, Qt, Slint, reference oracles or mutation tests, and does not
auto-assess statuses. Git must have both inventory commits available locally.

Edit [viewer.template.html](viewer.template.html) for presentation changes, then
regenerate. Embedded JSON escapes HTML delimiters; displayed data uses
`textContent`. Retain those boundaries and the absence of external dependencies.

Known scope limits include much media context-menu depth, remaining system
predicate and options controls, remote administration, and recursive formula
nests beyond the finite editor map. Consult the selected node and inventory
caveats before interpreting a status as a broader feature claim.
