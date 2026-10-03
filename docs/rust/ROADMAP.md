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

**Next**:

1. **The edit subscription dialog**. Recorded already:
   `oracle/record_edit_subscription.py` (fixture
   `oracle/fixtures/edit_subscription.json`) has its fields as it opens,
   the query list's buttons with the paste-queries messages, and what
   "apply" gives back. The dialog holds:
   - name, downloader, and the queries list (`query_row` is ready);
   - the query buttons;
   - limits, checker options, and publication options.

   The reference's layout, strings and button behaviour are written up
   in `docs/rust/notes/manage_subscriptions.md`. Extend the recorder for
   anything it doesn't cover yet (adding a query, the query editor).
2. Then the rest of the list's buttons:
   - add (choose a downloader),
   - edit,
   - reset (empties the queries' file logs, after a question),
   - retry failed/ignored,
   - overwrite downloader, overwrite checker options (the checker
     options editor exists: `checker_options_window.rs`),
   - merge, separate, deduplicate, lowercase,
   - export/import/duplicate,
   - import options.

## 2. Manage import folders / export folders

file > import/export folders > "manage import folders…" and "manage
export folders…", and the "check import folder now" and "run export
folder now" submenus there. Not started in the GUI.

- The backend (`hydrus-download::folders`, `hydrus-parse::sidecar`) runs
  import and export folders with sidecars, migrated from the reference.
  See `DIFFERENCES.md`, "Import folders and sidecars".
- Same shape as subscriptions: a list dialog of folders, an edit dialog
  per folder, apply/cancel.
- The lists' rows are recorded already: `oracle/record_folders_lists.py`
  (fixture `oracle/fixtures/folders_lists.json`).
- Record the reference's `ClientGUIImportFolders.EditImportFoldersPanel`
  and the export folders panel first (`ClientGUI._ManageImportFolders`,
  `_ManageExportFolders` open them).

## 3. The duplicates page: preparation and auto-resolution tabs

- The duplicates page has its filtering tab (the duplicate filter works).
- The preparation tab is missing: search distance, "search now",
  maintenance numbers.
- The auto-resolution tab is missing: the rules list, the rule editor,
  and review of pending actions.
- The backend has the similar files search and auto-resolution
  (`hydrus-duplicates`, `hydrus-store::duplicates`), checked against the
  reference.

## 4. Downloader pages: leftovers

- The lists' right-click menus:
  - copy queries;
  - file and search logs;
  - presentation options.
- Dragging across rows to select.
- Watcher pages have no import options buttons.
- The "highlighted" boxes don't show a search's or watcher's own file
  limit and import options.
- The Client API's `/manage_pages/get_page_info` for gallery and watcher
  pages.
- See `DIFFERENCES.md`, "Pages".

## Later

- **Menu bar.**
  - "clear and load" a session. Already recorded in
    `oracle/fixtures/sessions_menu.json`, including pages that veto
    closing.
  - Menu entries' descriptions in the status bar.
  - network > pause > "nudge subscriptions awake".
- **Options window.** The pages hydrus-rs honours are done. The rest of
  the reference's options wait on the features they configure
  (`DIFFERENCES.md`).
  - `get_client_options` in the Client API should reflect options
    changed in the window.
- **Search page.**
  - The larger system predicate editors that remain.
  - Domain and tag service buttons' remaining cases.
- **Native file picker** for "review files to import": use the `rfd`
  crate, which uses the desktop portal on Linux. This is decided.
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
