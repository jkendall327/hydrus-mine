# Roadmap

What is being worked on in hydrus-rs and what comes next. Keep it short and
current; history lives in git.

## Now

- **Goal:** every feature of the reference client's GUI, ported and tested
  against reference recordings, Linux first (Windows/macOS deferred; CI can
  still run them by hand).
- **Work tracking:** `docs/rust/tracking/` holds the remaining work, grouped
  into workstreams that can run in parallel. A leaf is done when a test tagged
  with its ID passes on master; the tracker counts that from the tests. See
  `docs/rust/tracking/README.md`.
- **Process:** `AGENTS.md`. The local loop is `scripts/dev.sh`; CI checks
  every push. There is no per-feature publication step.
- **Workflow overhaul in progress:** `docs/rust/WORKPLAN.md` (2026-10-08).

## Before 2026-10-08

Until 2026-10-08 completed leaves were signed off one checkpoint at a time
with evidence packets under `docs/rust/gui-coverage/` (removed; see git history
up to commit `89a7c2a0c`). That ledger reached 376 implementation sign-offs plus
nine historical re-verifications. Its leaf IDs carry over into the new tracker.

## Older per-area notes

These were written during the first week and may be partly stale; check the
code and `GUI.md` before relying on them.

### 1. Manage subscriptions (network > subscriptions…)

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

### 2. Manage import folders / export folders

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

### 3. The duplicates page: preparation and auto-resolution tabs

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

### 4. Downloader pages: leftovers

- See `DIFFERENCES.md`, "Pages".

### 5. The import options editor

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

### Later

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
