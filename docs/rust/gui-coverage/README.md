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

The default reference view follows selected Hydrus features through their nested
controls, workflows and shared editors. The optional native view follows main
menus and native windows through their implemented and missing child surfaces. These are different inventories with different granularity, not
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
snapshot and validated against the recorded node aggregates. They measure
navigation breadth, rather than implementation completion.

## Maintain the snapshots

The canonical inputs are [reference-inventory.json](reference-inventory.json)
and [native-inventory.json](native-inventory.json). Their provenance and status
definitions remain conservative. The source audit expands options, search,
media, tagging, services, database, network, import/export and shared editors.
Each audited node records an assessment rationale, concrete remaining work and
relevant inspection/regression evidence. Existing tests and recordings were read;
they were not rerun to produce this map. Native first-pass assessments retain the
reference-backed regression requirement. A source-supported slice with an
unverified behavioral boundary is partial until precise existing assertion or
recording scope supports the claimed slice. An evidence filename by itself does
not establish that scope.

Both inventories pin their source/evidence links to the same committed source
snapshot. Every source line has a SHA-256 fingerprint, and every linked module
records the exact pinned Git blob hash. The audit directory preserves curated
patches, inspection notes and the old-to-new anchor mapping. These hashes prove
source provenance; they do not prove behavior or constrain the current checkout.

1. Read the reference behavior, native implementation, limitations and relevant
   oracle/regression evidence before changing an assessment. Callback presence
   and existing evidence filenames alone do not establish behavioral parity.
2. Update the canonical JSON with stable IDs, labels, parents, statuses,
   repository-relative source paths and one-based lines. Preserve limitations
   and shared references; give candidate mappings `review_required: true`.
3. To refresh sources, run `python3 scripts/gui_coverage.py --repin COMMIT`.
   This maps anchors only through identical Git blobs or uniquely unchanged
   lines/context, fingerprints the new lines, refreshes module hashes, node
   aggregates and candidate mappings, and writes an anchor-remapping audit.
   Changed or ambiguous anchors require human review; no nearest-line guess is
   accepted. Curated patch files can be applied with repeated
   `--apply-audit docs/rust/gui-coverage/audit/NAME-patch.json` arguments.
   A patch's `baseline_git_head` declares the inspected source snapshot.
   Explicit per-anchor `anchor_git_head` takes precedence when newer source was
   inspected; inherited anchors keep their original source origin until remapped.
4. Run `python3 scripts/gui_coverage.py` from the repository root, then inspect
   the generated HTML in desktop and narrow browser layouts. Commit both the
   input changes and regenerated HTML.

`python3 scripts/gui_coverage.py --validate` checks IDs, parent acyclicity,
statuses, shared links, candidate mappings, all recorded navigation aggregates,
module hashes and source-line fingerprints against pinned Git blobs. The native
window audit also detects exported Slint windows reduced to opaque catalog
entries. Diagnostic reports surface weak first-pass rationales without
silently changing any status or forbidding truthful unassessed leaves. `--check` also verifies that the checked-in
HTML is current. The script uses only the Python standard library and Git. It
does not run Cargo, Qt, Slint, reference oracles or mutation tests, and does not
auto-assess statuses. Git must have the pinned inventory commit available locally, plus original
inspection commits when repinning historical anchors.

Edit [viewer.template.html](viewer.template.html) for presentation changes, then
regenerate. Embedded JSON escapes HTML delimiters; displayed data uses
`textContent`. Retain those boundaries and the absence of external dependencies.

The map covers exported native windows and important non-window surfaces such
as popups and importer controls. It still selects semantic control families
rather than enumerating every raw widget; remote administration and recursive
formula nesting have concrete missing entries or finite shared targets. Consult the selected node and inventory
caveats before interpreting a status as a broader feature claim.
