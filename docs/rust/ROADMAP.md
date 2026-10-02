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

**Done** (model only, no window yet):

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

**Next**:

1. **A choices window.** A generic Slint window for "Check which?" and
   other button-choice questions: a title, a message, N buttons, and
   cancel by closing it. The reference's `SelectFromListButtons`.
2. **The window.**
   - `ui/manage_subscriptions.slint`: a `ListTable` with
     `SUBSCRIPTION_COLUMNS`, and the buttons the model supports:
     - add (later),
     - edit (later),
     - delete,
     - pause/resume,
     - scrub delays,
     - check queries now,
     - reset (later),
     - select subscriptions,
     - overwrite checker options (the existing checker options editor,
       `checker_options_window.rs`),
     - apply and cancel.
   - Show the "globally paused" warning when subscriptions are paused
     from the network menu.
   - Open it from the network menu entry "subscriptions…"; see
     `hydrus-gui-model::main_menu`.
3. **Loading and applying** (`hydrus_store::subscriptions` has the
   reads and writes).
   - Load each subscription's settings and queries.
   - For each query, load its file log counts
     (`queues::file_seed_counts`) and its seeds' times
     (`queues::file_seeds` → `SeedTime`).
   - Apply writes:
     - changed settings and query states;
     - renames;
     - deletes (`delete_subscription`).
   - The daemon may run a subscription meanwhile. The reference pauses
     its subscriptions while the dialog is open ("Waiting for current
     subscription work to finish."). Decide whether to do the same (a
     pause flag the daemon honours) or write only the fields the dialog
     changed. Record the choice in `DECISIONS.md`.
4. **The edit subscription dialog**: name, downloader, queries list
   (`query_row` is ready), query buttons, limits, checker options,
   publication options. The reference's layout, strings and button
   behaviour are written up in `docs/rust/notes/manage_subscriptions.md`.
   Record it before porting (extend the recorder).
5. Then the rest of the list's buttons:
   - add (choose a downloader),
   - edit,
   - reset (empties the queries' file logs, after a question),
   - retry failed/ignored,
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
