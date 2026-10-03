# Roadmap

What is being worked on in hydrus-rs, what comes next, and what is half
done. Keep it current: when you finish something here, take it out (the
commit, `GUI.md` and `DIFFERENCES.md` say what was done); when you stop
partway, say exactly where.

The owner's current priority (2026-10) is **breadth**: get every major
area of the client working in a first pass, rather than perfecting each
corner before moving on. In order:

1. The manage subscriptions dialog.
2. Manage import folders and export folders.
3. The duplicates page's preparation and auto-resolution tabs.
4. The downloader pages' leftovers.
5. Then the gaps listed under "Later".

The backends for all of these exist. Subscriptions, import and export
folders, duplicates search and auto-resolution all run in `hydrus serve`
and are tested against the reference. The work is the GUI over them.

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

**Next**:

1. The list's last buttons: export/import (they need the reference's
   serialised form written, not just read), and the import options
   column's copy, paste and clear.

## 2. Manage import folders / export folders

**Done**: file > import/export folders > "manage import folders…" and
"manage export folders…" (`ui/folders.slint`, `src/folders_window.rs`,
`hydrus-gui-model`'s `folders`), recorded by
`oracle/record_folders_lists.py` and `oracle/record_folders_dialogs.py`,
tested in `tests/model/folders.rs` and `tests/gui/folders.rs`.

**Next**:

- The sidecar routers editor, which the import and export folder dialogs
  and the "filename tagging" dialog's "sidecars" tab open.
- An import folder's import options, after an import options editor.

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
  in the media viewer, and its selected pairs in a new page. **Next**:
  editing a rule's custom merge options; and its searches' locations.
- The filtering tab's search editor (`EditPotentialDuplicatesSearch
  ContextPanel`) and its "quick and dirty processing" box.

## 4. Downloader pages: leftovers

- See `DIFFERENCES.md`, "Pages".

## 5. The import options editor

**Done**: the editor for an importer's own options (`ui/import_options.slint`,
`src/import_options_window.rs`, `hydrus-gui-model`'s `import_options_editor`),
recorded by `oracle/record_import_options_editor.py`, opened from the
edit subscription and edit import folder dialogs and gallery and watcher
pages. It edits every kind but external programs; tag filters are typed
as lines (the reference has a tag filter editor). File filtering's
filetypes are ticked in the reference's tree (`ui/filetype_tree.slint`,
shared with the system:filetype editor; `hydrus-gui-model`'s
`filetype_tree`).

**Next**:

- The tag filter editor for "get tags" filters and the blacklist: its
  workings are done (`hydrus-gui-model`'s `tag_filter_editor`, recorded
  by `oracle/record_tag_filter_editor.py`); its window, and opening it
  from the import options editor, are next. Then a tags input with
  autocomplete for additional tags and the whitelist.
- The subscriptions list's import options row (copy, paste and clear
  of the selected subscriptions' options): copying and pasting want the
  reference's serialised import options container (hydrus-legacy reads
  it; nothing writes it yet).
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
