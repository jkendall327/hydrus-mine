# Roadmap

What is being worked on in hydrus-rs, what comes next, and what is half
done. Keep it current: when you finish something here, take it out (the
commit, `GUI.md` and `DIFFERENCES.md` say what was done); when you stop
partway, say exactly where.

The owner's current priority (2026-10) is **breadth**: get every major
area of the client working in a first pass, rather than perfecting each
corner before moving on. The subscriptions dialog, import/export folders,
duplicates tabs, about window and simple downloader page have had their
first pass.

The first parallel slate (2026-10-03) adds detailed embedded metadata,
manual file export, local service review/management, tag sibling/parent
editors, HTML/JSON formula editors, and URL class/single/nested gallery
generator editors. Their behavior and recorded evidence are described in
`GUI.md`; remaining differences are in `DIFFERENCES.md`. The metadata
handover patch has been applied and removed. Store snapshot revisions also
propagate service, URL-class and tag-graph edits to a running daemon.
Service deletion also refreshes open viewer and locked-selection tags.

The second parallel slate (2026-10-03) adds Client API key administration and
supported server settings; tag display/search and relationship application
configuration; and page/content parser editors with URL-class links. It uses
three feature worktrees with staged integration and batched GUI validation.
Reference recordings, behavioral regressions and independent review replace
mutation runs at the owner's request. This slate was reviewed and merged.

The third parallel slate (2026-10-04) adds bandwidth usage/rules and live network
jobs, cookie/session and HTTP-header management, clipboard URL monitoring,
service-to-service tag migration, and reference-compatible downloader definition
text/PNG import/export. The subscription Add flow now uses a separate gallery
list followed by the editor. Definitions come from saved configuration; new
recordings use synthetic domains. See the area sections in `GUI.md` and
`DIFFERENCES.md` for coverage and remaining first-pass limitations.

The continuous overnight run (2026-10-04) implements 240 distinct original
reference leaves: 222 formerly Missing and 18 formerly Partial. It adds
notebook/tab operations and independent named-session snapshots; live search,
viewer, tag and import Options consumers; autocomplete preferences; namespace
sort schemes and independent primary/fallback tag-service cog menus; network
job controls and URL/domain/parser workflows; subsidiary/content parser editing;
login credential/result editors; subscription concurrency and failure-stop
handling; and a configured default export destination. The two sorting cog menu
nodes are assessed separately and excluded from the concrete leaf count.
The next 51 validated leaves add palette, filename tagging, migration, viewing
statistics, Files/Trash, OR controls, Manage Tags, frame reset/flip, tag banners
and local-service actions. Source `028fd72f` passed all four hosted CI jobs;
455 native GUI integration tests passed on Linux, macOS and Windows.
Exact accounting and validation are in [the overnight report](notes/overnight_gui_burndown.md)
and [the frozen-baseline ledger](gui-coverage/overnight/progress.json). Remaining
boundaries stay explicit in the map and `DIFFERENCES.md`.

The [GUI migration map](gui-progress.html) expands selected reference features
and all 97 exported native windows into nested work, including shared editors,
all 38 reference option tabs and 19 system-predicate groups. It contains 1,812
reference nodes and 1,733 native nodes, with per-node assessments, concrete
remaining work and pinned source/evidence links. The frozen reference inventory
now has 581 Missing, 390 Partial and 841 First pass entries; its status changes
also include parent/alias assessments, which do not inflate the 240-item completion count.
Native first-pass claims cite scoped regression evidence; source-supported but
unverified behavior is partial. These counts are not a whole-client completion
percentage. Maintenance instructions and scope limits are in
[gui-coverage/README.md](gui-coverage/README.md).

Continuous source work now proposes 99 further original leaf completions over
that validated 240 checkpoint: 53 on `codex/parity-more-controls`, five more
on the dependent `codex/parity-next-details` branch, and 13 more on
`codex/parity-preview-and-launching`, plus nine on
`codex/parity-popup-and-file-views`, and 12 more on
`codex/parity-cache-and-favourites`, and seven more on
`codex/parity-tag-sort-and-refresh`. The 71-control branch adds two preview viewing-time
controls, two saved formatting controls with backend consumers, one keyboard
capture control, and eight ordered Open Externally routing controls. It includes tab
appearance/drag, notebook tree, notes, rating sizes, archive repair, duplicate
colours, tag suggestions/weights, autocomplete panes, sidebar cogs, thumbnail
navigation, namespace colour actions, external-call list and command controls,
sibling connector colours, browsing/API idle timeouts, viewer
drag/hover-wheel preferences, staged subscription merging, registered login
scripts in mixed downloader packages, all five importing work-slot limits,
local-domain copy/move confirmations, the per-service already-exists tag filter,
and global viewing-statistics clear/cull actions. The nine further controls add saved popup width/fixed-width policies;
producer-owned clipboard, callable and yes/no job actions; and the four
Files/Trash view-removal policies with captured-page consumers. The next 12 add three
thumbnail-cache memory/timeout/debug controls with byte-accounted owned consumers;
five sidebar splitter and Pages-menu controls with page-local geometry and accepted-exit
saving; and four Ctrl/Shift preview-focus preferences with duration-aware selection.
The next seven add the separate search/viewer Manage Tags opening sorts, the raw
OR connecting-string editor, and the four
experimental gallery/watcher list update intervals and denominators. Accepted preview
images and intervals now survive per-page return under global hide, with a bounded
frame cache and owned retry paths. That hide preference remains Partial because
first-raster admission, reconstruction and terminal cleanup still differ from Qt. Regex favourite selection
now uses the reference read-only chooser; that original leaf was already completed
and receives no additional completion credit. Historical subscription
seed-cache compatibility, direct ordered subscription imports and login editor
controls are further parent/Partial improvements with zero leaf credit.
These remain proposals while exact hosted CI
runs and rendered review are pending. Window rescue remains a Partial
improvement with zero completion credit; GUI formatting now has two conditional
original control proposals after its backend waits and diagnostics were ported. Owned asynchronous
metadata filesystem jobs also retain their two original Partial assessments,
and mouse idle tracking remains Partial because it observes application windows.
Broader external
call/editor boundaries also remain Partial. Inspect the current branch with
`python3 scripts/gui_burndown.py --commit HEAD`; do not substitute its proposed
total of 339 for the validated 240 ledger. New work and diagnostics continue
while hosted validation runs, as authorized by the owner.

The next breadth work, in the owner's existing order:

1. **Network management**: remaining scheduling, complete login execution and
   broader editor boundaries listed in the map. Monthly usage/history controls,
   cookie exchange, header questions, runtime job controls and URL/domain/parser
   editors are implemented within their assessed scopes. Remote repositories,
   IPFS and account administration remain outside the owner's priorities.
2. **Tags**: sibling/parent sync, migration archives/hash conversion and pair
   mapping-count filters. Native service-to-service mappings/siblings/parents
   migration is implemented. Write-autocomplete and presentation preferences
   now reach their consumers, while full relationship autocomplete,
   asynchronous loading and repository permission/reason suggestions remain
   Partial or Missing as recorded.
3. **Downloader definitions**: complete login-script management/execution and
   serialized subscription exchange, file-based test-data fetching and remaining
   formula-specific boundaries. Native page/content/subsidiary parser editors,
   URL-class parser links and definition text/PNG interchange are implemented.
   All six native formula kinds have editors; URL fetching and multiple-example
   selection reach shared children. Credential/result and partial login-script
   editors are present; their remaining scope is recorded per node.

Then the gaps listed under "Later", and the half-done items below. The third
parallel slate and its Windows portability follow-up were reviewed and merged.
The owner has authorized continuous work rather than fixed slates. Use concrete
leaves and their remaining boundaries in the GUI map to choose independent work;
exclude parent groups, aliases and evidence-only reassessments from implementation
completion goals.

## 1. Manage subscriptions (network > subscriptions…)

**Done**:

- `oracle/record_subscriptions_list.py` records the reference dialog on
  four subscriptions in varied states. Fixture:
  `oracle/fixtures/subscriptions_list.json`. It holds:
  - the subscriptions list's rows;
  - each subscription's queries' rows;
  - a run of list actions with what each asked and the state after:
    pause/resume, scrub delays, check queries now with its "Check
    which?" prompts, select subscriptions by text, and delete.
- `hydrus-gui-model`:
  - `subscriptions_list` writes the rows: `subscription_row`,
    `query_row`, and the column titles.
  - `subscriptions_dialog` holds the dialog state:
    - `Subscriptions`: the subscriptions held until apply, the sort,
      the selection, the buttons' actions, and the deleted;
    - `CheckNow`: the check-now prompts.
  - Tests: `tests/model/subscriptions_list.rs` replays the recording.
- The window (`ui/subscriptions.slint`, `src/subscriptions_window.rs`),
  opened from network > "subscriptions…".
  - It lists the subscriptions and has delete, pause/resume, scrub
    delays, check queries now (with the prompts, in the window's own
    question panel, `Asking`) and select subscriptions.
  - "apply" writes only what changed.
  - `tests/gui/subscriptions.rs` tests it.
- The edit subscription dialog (`ui/edit_subscription.slint`,
  `src/edit_subscription_window.rs`, `hydrus-gui-model`'s
  `edit_subscription`), opened by "add" and "edit": tested against
  `oracle/fixtures/edit_subscription.json` in
  `tests/model/edit_subscription.rs`, and `tests/gui/edit_subscription.rs`.
  The query editor's additional tags wait on the import options
  editor's tags page.
- The list's other buttons (merge, separate, lowercase, retry, reset,
  overwrite downloader and checker options), recorded by
  `oracle/record_subscriptions_buttons.py` and tested in
  `tests/model/subscriptions_buttons.rs`.
- The import options column's copy, paste and clear actions, including
  the reference JSON container and staged clipboard changes, recorded by
  `oracle/record_subscription_import_options.py`.

**Next**:

1. The list's subscription export/import buttons need the reference's
   serialised subscription form written, not just read. This is separate
   from the implemented import-options clipboard container.

## 2. Manage import folders / export folders

**Done**: file > import/export folders > "manage import folders…" and
"manage export folders…" (`ui/folders.slint`, `src/folders_window.rs`,
`hydrus-gui-model`'s `folders`), recorded by
`oracle/record_folders_lists.py` and `oracle/record_folders_dialogs.py`,
tested in `tests/model/folders.rs` and `tests/gui/folders.rs`.

**Next**:

- The sidecar routers editor, which the import and export folder dialogs
  and the "filename tagging" dialog's "sidecars" tab open. **Done**: how
  routers describe themselves (`hydrus-gui-model`'s `sidecars`, recorded
  by `oracle/dump_sidecar_descriptions.py`), on the dialogs' sidecars
  buttons; how string processing describes itself (hydrus-core's
  `string_descriptions`); and the editors' workings (`hydrus-gui-model`'s
  `sidecar_editors`, recorded by `oracle/record_sidecar_editors.py`).
  Their windows are done (`src/sidecars_window.rs`), from the import and
  export folder dialogs and the "filename tagging" dialog's "sidecars"
  tab, whose rows show what each file's sidecars give (`file_preview`,
  recorded by `oracle/dump_sidecar_previews.py`); an import page runs
  its routers. The string processor editor's workings are done
  (`hydrus-gui-model`'s `string_editors`, recorded by
  `oracle/record_string_processor_editor.py`): its steps, test panels and
  the splitter, joiner, sorter and slicer editors, and their windows
  (`src/string_processor_window.rs`), from a router's or source's
  processing button, and the string match editor
  (`oracle/record_string_match_editor.py`) and string converter editor
  (`oracle/record_string_converter_editor.py`), and the tag filter step's
  editor. JSON sidecar sources now open the reusable HTML/JSON formula
  editor, whose typed rules, extraction controls, test panel and string
  processing are checked by `oracle/record_formula_editors.py` and GUI/store
  tests. Formula interchange, additional kinds, URL test-data fetching and
  multiple-example controls now use the shared editor. **Next**: the router
  editor's testing panel and file-based test-data fetching.

## 3. The duplicates page: preparation and auto-resolution tabs

**Done**: the sidebar's three tabs (`ui/duplicates_page.slint`,
`src/duplicates_sidebar.rs`, `hydrus-gui-model`'s `duplicates_page`):
preparation's numbers, distance, working hard and cog menu, and
auto-resolution's rules list, pause/play and resets. Recorded by
`oracle/record_duplicates_preparation.py` and
`oracle/record_auto_resolution_rows.py`; tested in
`tests/model/duplicates_page.rs` and `tests/gui/duplicates_page.rs`.

**Next**:

- The auto-resolution rules editor (`EditDuplicatesAutoResolutionRules
  Panel` and the rule editor with its comparators,
  `ClientGUIDuplicatesAutoResolution.py`), and "review actions"
  (`ClientGUIDuplicatesAutoResolutionRuleReview.py`). **Done**: what the
  list and editor say of rules (`hydrus-gui-model`'s
  `auto_resolution_rules`: rows, comparator summaries, which can tell A
  from B), recorded by `oracle/record_auto_resolution_summaries.py`.
  The "edit rules" window, the rule editor and the comparator editors
  are done too (`src/auto_resolution_rules_window.rs`), and "review
  actions" (`src/auto_resolution_review_window.rs`, recorded by
  `oracle/record_auto_resolution_review.py`). The pending
  pairs' merge summary is done (`hydrus-gui-model`'s `merge_summary`, on
  hydrus-store's `duplicates::merge::plan`). The rule editor's preview
  tab is done (`src/auto_resolution_preview_window.rs`), its pairs
  opening in the duplicate filter. "Review actions" opens its pending
  pairs in the duplicate filter, to approve or deny, and its other pairs
  in the media viewer, and its selected pairs in a new page. The merge
  options editor is done (`src/merge_options_window.rs`, recorded by
  `oracle/record_merge_options_editor.py`), for a rule's custom merge
  options and the page's defaults, and the duplicate filter's "custom
  action", and the rules' searches' location. **Next**: the rule
  editor's search autocompletes (with the tags autocomplete input
  below).
- The filtering tab's search editor (`EditPotentialDuplicatesSearch
  ContextPanel`) and its "quick and dirty processing" box.

## 4. Downloader pages: leftovers

- See `DIFFERENCES.md`, "Pages".

## 5. The import options editor

**Done**: the editor for an importer's own options (`ui/import_options.slint`,
`src/import_options_window.rs`, `hydrus-gui-model`'s `import_options_editor`),
recorded by `oracle/record_import_options_editor.py`, opened from the
edit subscription and edit import folder dialogs and gallery and watcher
pages. It edits every kind but external programs. File filtering's
filetypes are ticked in the reference's tree (`ui/filetype_tree.slint`,
shared with the system:filetype editor; `hydrus-gui-model`'s
`filetype_tree`). Additional-tags and whitelist lists can open the shared
write-tag autocomplete editor; child cancellation preserves the parent draft.

**Next**:

- The tag filter editor is done (`src/tag_filter_window.rs`, recorded by
  `oracle/record_tag_filter_editor.py`), for "get tags" filters and the
  blacklist, including favourite CRUD/exchange and bulk paste. Remaining
  import-options boundaries are listed in `DIFFERENCES.md`.
- The editor's copy and paste and favourites buttons.

## Later

- **Menu bar.**
  - Menu entries' descriptions in the status bar.
- **Options window.** The pages hydrus-rs honours are done. The rest of
  the reference's options wait on the features they configure
  (`DIFFERENCES.md`).
  - `get_client_options` in the Client API should reflect options
    changed in the window.
- **Search page.**
  - The larger system predicate editors that remain.
  - Domain and tag service buttons' remaining cases.
- **Testing.**
  - Mutation work below is deferred during the current breadth pass, at the
    owner's request. Use reference recordings and behavioral integration checks.
  - Fuzz/property tests for the parsers that take untrusted input
    (system predicates, Client API parameters, URL parsing).
  - A cargo-mutants sweep of `hydrus-search` and `hydrus-core`.
  - Owed mutation checks:
    - `hydrus-gui`'s downloader lists (commit cd43dd5). The run was
      stopped after a few of its 91 mutants; their one survivor was
      fixed.
    - The subscriptions dialog: 27 survivors in its last run.
      - `subscriptions_dialog`'s `sort_key`: no test sorts by status,
        times, delay or items with data that tells them apart.
      - `DialogQuery::latest_added` and `can_reset`: no test selects
        only the subscription with no queries.
      - `CheckNow`'s filter of paused subscriptions' queries.
      - The ">" and ">=" boundaries of the time texts in
        `subscriptions_list`.
      - `subscriptions_window`'s `changes`: no test applies with a name
        or a query unchanged.
      - Its `now`.
