# Roadmap

What is being worked on in hydrus-rs, what comes next, and what is half
done. Keep it current: when you finish something here, take it out (the
commit, `GUI.md` and `DIFFERENCES.md` say what was done); when you stop
partway, say exactly where.

The owner's current priority (2026-10) is **breadth**: get every major
area of the client working in a first pass, rather than perfecting each
corner before moving on. The subscriptions dialog, the import and export
folders, the duplicates page's tabs and the downloader pages' leftovers
have had their first pass. Next, in the owner's order (2026-10-03), the
screens not started yet:

1. The about window (help > about). **Done.**
2. The simple downloader page. **Done**: its engine (a
   `simple_downloader` queue, `hydrus-download`'s `run_simple_jobs`), the
   formulae (`hydrus-parse::simple`, the reference's defaults, imported
   from its options), and the page (pages > download > new simple
   downloader page, or the page chooser), carried over from hydrus's
   sessions. **Left**: "edit formulae", with the formula editor (which the
   parser editors need too, so it comes with item 6).
3. The media manage dialogs. **First pass done**: URLs, ratings, times,
   force filetype, detailed embedded metadata, and share > export files
   (previews, worker progress/cancel, sidecars and confirmed trash).
4. Services: **Done**: review local/built-in services (native counts, id/key, refresh). **Next**: manage local services; remote repositories/IPFS/account administration and bulk maintenance remain subsequent work.
5. Tag siblings and parents. **First pass done**: service editors, staged
   add/delete/rescind, automatic conflict/cycle repair, remembered workspace
   filters, clipboard/.txt import/export, and atomic graph/count refresh.
   **Left**: write autocomplete and asynchronous list loading, default service
   tabs, repository permission/reason suggestions; tags > display/search,
   migrate tags, sibling/parent sync.
6. The downloader definition editors: URL classes, parsers, gallery URL
   generators, URL class links, logins, import/export downloaders.

Then the gaps listed under "Later", and the half-done items below.

### Detailed embedded file metadata

**Done**: info > "show detailed embedded file metadata" in thumbnails and the
viewer opens "Detailed File Metadata". Basics renders the full info tree;
a worker reads the local file and fills the conditional EXIF, XMP, IPTC,
human-readable text and extra sections. EXIF uses ListTable's selection and
sorting, and double-click copies raw values (bytes as plain hex and text with
its original NULs). PNG EXIF carries the reference's orientation note. PDFs use
the existing PDF decoder for Author, Title, Subject and Keywords. Non-local
files show the reference's message; read errors preserve basics. Closing or
superseding a request cannot publish into another window.

Evidence: `oracle/dump_embedded_metadata.py` records the media corpus and
metadata samples including the reference's raw EXIF copy callback;
`oracle/record_embedded_metadata_window.py` records the live panel's visibility,
PNG note, read-only sections, nested basics, EXIF sorting, non-local worker and
PDF fields, with empty, encrypted and malformed PDFs. Fixture-backed model and
GUI tests replay the values and menu actions; the GUI test saves
`embedded_metadata.png`. Existing HEIF/AVIF/JXL, IPTC charset and malformed-XMP
decoder differences are in DIFFERENCES.md.

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
  editor. **Next**: the JSON formula editor; the router editor's testing
  panel.

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
`filetype_tree`).

**Next**:

- The tag filter editor is done (`src/tag_filter_window.rs`, recorded by
  `oracle/record_tag_filter_editor.py`), for "get tags" filters and the
  blacklist; its favourites are not. Next, a tags input with
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
